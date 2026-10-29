//! uft core: offline, pinned-TLS, multi-stream file transfer engine.

/// Protocol version sent in every hello.
pub const VERSION: u32 = 1;
/// Default listen port; discovery's subnet sweep probes this.
pub const DEFAULT_PORT: u16 = 47474;

pub mod discover;
pub mod link;
mod os;
pub mod pair;
pub mod progress;
pub mod proto;
pub mod recv;
pub mod send;
pub mod tls;
