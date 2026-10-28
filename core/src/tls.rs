//! Device identity, pinned TLS 1.3 and hardware-based cipher choice.

use crate::proto::Caps;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, ClientConnection, ServerConfig, ServerConnection, StreamOwned};
use std::fs;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::Arc;

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
        let ck = rcgen::generate_simple_self_signed(vec!["uft".into()]).map_err(io::Error::other)?;
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
    todo!("{}", id.cert.len())
}

/// TLS 1.3 only. Accepts the server only if its cert fingerprint equals `server_fp`.
/// Presents `id` as client cert. Same cipher ordering rule as the server.
pub fn client_config(id: &Identity, server_fp: &str) -> io::Result<Arc<ClientConfig>> {
    todo!("{} {server_fp}", id.cert.len())
}

/// TCP connect (5 s timeout, 30 s read timeout) and complete the TLS handshake.
pub fn connect(addr: SocketAddr, cfg: Arc<ClientConfig>) -> io::Result<TlsClient> {
    todo!("{addr} {}", Arc::strong_count(&cfg))
}

/// Complete the server handshake on an accepted socket (30 s read timeout).
/// Returns the stream and the client's cert fingerprint.
pub fn accept(tcp: TcpStream, cfg: Arc<ServerConfig>) -> io::Result<(TlsServer, String)> {
    todo!("{tcp:?} {}", Arc::strong_count(&cfg))
}
