//! Device identity, pinned TLS 1.3 and hardware-based cipher choice.

use crate::proto::Caps;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, aws_lc_rs};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    ClientConfig, ClientConnection, ConnectionCommon, DigitallySignedStruct, DistinguishedName,
    ServerConfig, ServerConnection, SignatureScheme, StreamOwned,
};
use std::fs;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub type TlsClient = StreamOwned<ClientConnection, TcpStream>;
pub type TlsServer = StreamOwned<ServerConnection, TcpStream>;

/// This device's self-signed certificate and private key.
pub struct Identity {
    pub cert: CertificateDer<'static>,
    pub key: PrivateKeyDer<'static>,
}

impl Identity {
    /// Loads `dir/cert.der` + `dir/key.der`, or generates and saves them (key mode 0600).
    pub fn load_or_create(dir: &Path) -> io::Result<Identity> {
        fs::create_dir_all(dir)?;
        let (cp, kp) = (dir.join("cert.der"), dir.join("key.der"));
        if cp.exists() && kp.exists() {
            return Ok(Identity {
                cert: CertificateDer::from(fs::read(cp)?),
                key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(fs::read(kp)?)),
            });
        }
        let id = Identity::generate()?;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        io::Write::write_all(&mut opts.open(kp)?, id.key.secret_der())?;
        fs::write(cp, &id.cert)?;
        Ok(id)
    }

    /// Fresh in-memory identity (tests, bench).
    pub fn generate() -> io::Result<Identity> {
        let ck =
            rcgen::generate_simple_self_signed(vec!["uft".into()]).map_err(io::Error::other)?;
        Ok(Identity {
            cert: ck.cert.der().clone(),
            key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der())),
        })
    }

    pub fn fingerprint(&self) -> String {
        fingerprint(&self.cert)
    }
}

/// Lowercase hex BLAKE3 of the certificate DER (64 chars).
pub fn fingerprint(cert_der: &[u8]) -> String {
    blake3::hash(cert_der).to_hex().to_string()
}

/// Runtime hardware AES detection (x86 AES-NI, ARMv8 AES); false elsewhere.
pub fn has_aes() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return is_x86_feature_detected!("aes");
    #[cfg(target_arch = "aarch64")]
    return std::arch::is_aarch64_feature_detected!("aes");
    #[allow(unreachable_code)]
    false
}

/// This device's capabilities for Hello/Welcome.
pub fn caps() -> Caps {
    Caps {
        aes: has_aes(),
        cores: std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
        os: std::env::consts::OS.into(),
    }
}

/// TLS 1.3 only. Requires a client cert (any cert, but its handshake signature is verified).
/// Cipher order: AES-128-GCM first with hardware AES, ChaCha20-Poly1305 first without;
/// without AES the server ignores the client's order.
pub fn server_config(id: &Identity) -> io::Result<Arc<ServerConfig>> {
    let p = provider();
    let verifier = Arc::new(AnyClient(p.signature_verification_algorithms));
    let mut cfg = ServerConfig::builder_with_provider(Arc::new(p))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![id.cert.clone()], id.key.clone_key())
        .map_err(io::Error::other)?;
    cfg.ignore_client_order = !has_aes();
    // No resumption tickets: senders never read data streams, and unread bytes at close
    // make the kernel send RST, which drops the tail of the stream.
    cfg.send_tls13_tickets = 0;
    Ok(Arc::new(cfg))
}

/// TLS 1.3 only. Accepts the server only if its cert fingerprint equals `server_fp`.
/// Presents `id` as client cert. Same cipher ordering rule as the server.
pub fn client_config(id: &Identity, server_fp: &str) -> io::Result<Arc<ClientConfig>> {
    let p = provider();
    let fp = server_fp.to_ascii_lowercase();
    let verifier = Arc::new(Pinned {
        fp,
        algs: p.signature_verification_algorithms,
    });
    let cfg = ClientConfig::builder_with_provider(Arc::new(p))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(vec![id.cert.clone()], id.key.clone_key())
        .map_err(io::Error::other)?;
    Ok(Arc::new(cfg))
}

/// TCP connect (5 s timeout, 30 s read timeout) and complete the TLS handshake.
pub fn connect(addr: SocketAddr, cfg: Arc<ClientConfig>) -> io::Result<TlsClient> {
    let mut tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
    prep(&tcp)?;
    let name = ServerName::try_from("uft").map_err(io::Error::other)?;
    let mut conn = ClientConnection::new(cfg, name).map_err(io::Error::other)?;
    handshake(&mut conn, &mut tcp)?;
    Ok(StreamOwned::new(conn, tcp))
}

/// Complete the server handshake on an accepted socket (30 s read timeout).
/// Returns the stream and the client's cert fingerprint.
pub fn accept(mut tcp: TcpStream, cfg: Arc<ServerConfig>) -> io::Result<(TlsServer, String)> {
    prep(&tcp)?;
    let mut conn = ServerConnection::new(cfg).map_err(io::Error::other)?;
    handshake(&mut conn, &mut tcp)?;
    let fp = match conn.peer_certificates() {
        Some([cert, ..]) => fingerprint(cert),
        _ => return Err(io::Error::other("client sent no certificate")),
    };
    Ok((StreamOwned::new(conn, tcp), fp))
}

// No SO_SNDBUF/SO_RCVBUF: setting them disables Linux autotuning.
fn prep(tcp: &TcpStream) -> io::Result<()> {
    tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
    // A stalled peer still answers TCP's window probes, so without this a write waits forever.
    tcp.set_write_timeout(Some(Duration::from_secs(30)))?;
    tcp.set_nodelay(true)
}

fn handshake<D>(conn: &mut ConnectionCommon<D>, tcp: &mut TcpStream) -> io::Result<()> {
    while conn.is_handshaking() {
        conn.complete_io(tcp)?;
    }
    Ok(())
}

fn provider() -> CryptoProvider {
    use aws_lc_rs::cipher_suite::*;
    let mut p = aws_lc_rs::default_provider();
    p.cipher_suites = if has_aes() {
        vec![
            TLS13_AES_128_GCM_SHA256,
            TLS13_AES_256_GCM_SHA384,
            TLS13_CHACHA20_POLY1305_SHA256,
        ]
    } else {
        vec![
            TLS13_CHACHA20_POLY1305_SHA256,
            TLS13_AES_128_GCM_SHA256,
            TLS13_AES_256_GCM_SHA384,
        ]
    };
    p
}

/// Client side of pinning: the server cert must hash to `fp`.
#[derive(Debug)]
struct Pinned {
    fp: String,
    algs: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if fingerprint(end_entity) == self.fp {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("server fingerprint mismatch".into()))
        }
    }

    fn verify_tls12_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(msg, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(msg, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

/// Any client cert passes; the caller decides trust by fingerprint.
#[derive(Debug)]
struct AnyClient(WebPkiSupportedAlgorithms);

impl ClientCertVerifier for AnyClient {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(msg, cert, dss, &self.0)
    }

    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(msg, cert, dss, &self.0)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Instant, SystemTime};

    type Accepted = thread::JoinHandle<io::Result<(TlsServer, String)>>;

    fn server() -> (SocketAddr, String, Accepted) {
        let id = Identity::generate().unwrap();
        let (cfg, fp) = (server_config(&id).unwrap(), id.fingerprint());
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        (addr, fp, thread::spawn(move || accept(l.accept()?.0, cfg)))
    }

    #[test]
    fn pinned_handshake_round_trips() {
        let (addr, fp, srv) = server();
        let me = Identity::generate().unwrap();
        let mut c = connect(addr, client_config(&me, &fp).unwrap()).unwrap();
        let (mut s, client_fp) = srv.join().unwrap().unwrap();
        assert_eq!(client_fp, me.fingerprint());
        c.write_all(b"ping").unwrap();
        c.flush().unwrap();
        let mut buf = [0; 4];
        s.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");
        s.write_all(b"pong").unwrap();
        s.flush().unwrap();
        c.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"pong");
    }

    #[test]
    fn wrong_fingerprint_is_refused() {
        let (addr, _, srv) = server();
        let me = Identity::generate().unwrap();
        assert!(connect(addr, client_config(&me, &"0".repeat(64)).unwrap()).is_err());
        assert!(srv.join().unwrap().is_err());
    }

    #[test]
    fn identity_persists() {
        let nanos = SystemTime::UNIX_EPOCH.elapsed().unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("uft-tls-{}-{nanos}", std::process::id()));
        let a = Identity::load_or_create(&dir).unwrap();
        let b = Identity::load_or_create(&dir).unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.fingerprint().len(), 64);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore]
    fn loopback_throughput() {
        const TOTAL: usize = 1 << 30;
        let (addr, fp, srv) = server();
        let me = Identity::generate().unwrap();
        let mut c = connect(addr, client_config(&me, &fp).unwrap()).unwrap();
        let (mut s, _) = srv.join().unwrap().unwrap();
        let rx = thread::spawn(move || {
            let mut buf = vec![0; 1 << 20];
            let mut got = 0;
            while got < TOTAL {
                got += s.read(&mut buf).unwrap();
            }
        });
        let buf = vec![7u8; 1 << 20];
        let t = Instant::now();
        for _ in 0..TOTAL / buf.len() {
            c.write_all(&buf).unwrap();
        }
        c.flush().unwrap();
        rx.join().unwrap();
        let secs = t.elapsed().as_secs_f64();
        let suite = c.conn.negotiated_cipher_suite().unwrap().suite();
        println!(
            "TLS loopback {suite:?}: {:.2} GB/s",
            TOTAL as f64 / secs / 1e9
        );
    }
}
