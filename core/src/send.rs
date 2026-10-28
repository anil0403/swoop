//! Sender: offer files, then stripe chunks over adaptive parallel TLS streams.

use crate::progress::Snapshot;
use crate::tls::Identity;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

pub enum Source {
    /// A file, or a directory sent recursively under its own name.
    Path(PathBuf),
    /// Generated in memory, never touches disk (benchmarks).
    Synthetic { name: String, size: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Streams {
    Fixed(u32),
    /// Start with 1, add one every ~2 s while throughput rises ≥5%, cap 8.
    Auto,
}

pub struct SendOptions {
    pub streams: Streams,
    /// Device name shown to the receiver.
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SendStats {
    /// Bytes sent this session (excludes resumed).
    pub bytes: u64,
    /// Bytes the receiver already had.
    pub resumed: u64,
    pub secs: f64,
    /// Average bytes/s over the session.
    pub avg: f64,
    /// Peak smoothed bytes/s.
    pub peak: f64,
    /// Data streams in use at the end.
    pub streams: u32,
}

/// Sends `sources` to the receiver at `addr` whose cert fingerprint is `server_fp`.
/// Blocks until the receiver reports Done. `on_progress` gets receiver-acknowledged
/// progress plus the current stream count, ~10×/s.
pub fn send(
    addr: SocketAddr,
    id: &Identity,
    server_fp: &str,
    sources: &[Source],
    opts: &SendOptions,
    on_progress: impl FnMut(&Snapshot, u32),
) -> io::Result<SendStats> {
    let _ = (addr, id, server_fp, sources, opts.streams, on_progress);
    todo!()
}
