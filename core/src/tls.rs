//! Device identity, pinned TLS 1.3 and hardware-based cipher choice.

use crate::proto::Caps;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, ClientConnection, ServerConfig, ServerConnection, StreamOwned};
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
        todo!("{dir:?}")
    }

    /// Fresh in-memory identity (tests, bench).
    pub fn generate() -> io::Result<Identity> {
        todo!()
    }

    pub fn fingerprint(&self) -> String {
        fingerprint(&self.cert)
    }
}

/// Lowercase hex BLAKE3 of the certificate DER (64 chars).
pub fn fingerprint(cert_der: &[u8]) -> String {
    todo!("{}", cert_der.len())
}

/// Runtime hardware AES detection (x86 AES-NI, ARMv8 AES); false elsewhere.
pub fn has_aes() -> bool {
    todo!()
}

/// This device's capabilities for Hello/Welcome.
pub fn caps() -> Caps {
    todo!()
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
