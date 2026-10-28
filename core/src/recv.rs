//! Receiver: accept sessions, verify chunks, write files, resume.

use crate::tls::Identity;
use std::io;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

pub struct RecvOptions {
    /// Where received files go.
    pub dir: PathBuf,
    /// Verify but discard data (benchmarks).
    pub null_sink: bool,
    /// Device name shown to senders.
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum RecvEvent {
    Started { session: u64, peer_fp: String, peer_name: String, files: usize, total: u64, resumed: u64 },
    Progress { session: u64, done: u64, total: u64 },
    Done { session: u64, bytes: u64, secs: f64 },
    Failed { session: u64, error: String },
}

/// Accepts connections forever, one thread per connection. Sessions are accepted
/// automatically (pairing comes in phase 2).
pub fn serve(
    listener: TcpListener,
    id: Arc<Identity>,
    opts: RecvOptions,
    on_event: impl Fn(RecvEvent) + Send + Sync + 'static,
) -> io::Result<()> {
    let _ = (listener, id, opts.dir, on_event);
    todo!()
}

/// Maps an untrusted offered path to a safe relative path, or None if it is absolute,
/// empty, or contains `..`, `.`, empty components, NUL or backslash.
pub fn safe_rel_path(path: &str) -> Option<PathBuf> {
    todo!("{path}")
}
