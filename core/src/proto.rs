//! Wire protocol v1.
//!
//! Every connection is TLS 1.3 with both sides presenting self-signed certs; a peer is
//! identified by `tls::fingerprint` of its cert. After the handshake the sender speaks first.
//!
//! ```text
//! frame = kind: u8 | len: u32 big-endian (bytes after this field) | body
//! kind 1  Msg    body = JSON `Msg`
//! kind 2  Chunk  body = file: u32 | index: u64 | blake3: [u8; 32] | data
//! ```
//!
//! ```text
//! control conn  S→R Hello{role: control, commit}   R→S Welcome{nonce}   S→R Offer{reveal}
//!               R→S Accept | Reject
//!               R→S Progress (~every 100 ms) …           R→S Done | Error
//! data conns    S→R Hello{role: data, same session}      S→R Chunk* then close
//! ```
//!
//! Chunks of all files are striped across data connections in any order. The receiver
//! checks each chunk's BLAKE3 and writes it at `index * chunk_size` of file `file`.
//! `Accept.have[i]` lists chunk indices of file `i` the receiver already holds (resume).
//! `Progress.done` counts bytes written, including resumed ones.

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const KIND_MSG: u8 = 1;
pub const KIND_CHUNK: u8 = 2;
/// Chunk body header: file u32 + index u64 + blake3 hash.
pub const CHUNK_HEADER: usize = 4 + 8 + 32;
/// Default chunk size: amortises per-frame cost, small enough for phone RAM.
pub const CHUNK_SIZE: u64 = 2 << 20;
/// Largest chunk a receiver accepts.
pub const MAX_CHUNK: u64 = 16 << 20;
/// Largest JSON message: an offer for `MAX_FILES` files with typical names fits, and parsing one
/// stays within a few hundred MB on a phone.
pub const MAX_MSG: usize = 32 << 20;
/// Most files one offer may list.
pub const MAX_FILES: usize = 250_000;
/// Messages bigger than this (large offers) share `BIG_BUDGET` bytes at a time, so a flood of
/// them can't run a phone out of memory.
const BIG_MSG: usize = 1 << 20;
const BIG_BUDGET: usize = 2 * MAX_MSG;
static BIG_IN_USE: AtomicUsize = AtomicUsize::new(0);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Caps {
    /// Hardware AES available (picks AES-GCM vs ChaCha20).
    pub aes: bool,
    pub cores: u32,
    pub os: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Control,
    Data,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FileEntry {
    /// Relative path with `/` separators. Untrusted on the receiving side.
    pub path: String,
    pub size: u64,
    /// Modification time, unix seconds.
    pub mtime: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Msg {
    Hello {
        version: u32,
        session: u64,
        role: Role,
        caps: Caps,
        name: String,
        /// Control only: `pair::commit` of the sender's pairing nonce. Senders that predate
        /// it can only send to devices that trust them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
    },
    Welcome {
        version: u32,
        caps: Caps,
        name: String,
        /// The receiver's pairing nonce, when the Hello had a commit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nonce: Option<String>,
    },
    Offer {
        chunk_size: u64,
        /// The sender's pairing nonce, matching the commit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reveal: Option<String>,
        #[serde(deserialize_with = "bounded")]
        files: Vec<FileEntry>,
        /// Empty folders to create, relative paths like `files`. Optional on the wire, so
        /// receivers that predate it just skip them.
        #[serde(
            default,
            skip_serializing_if = "Vec::is_empty",
            deserialize_with = "bounded"
        )]
        dirs: Vec<String>,
    },
    Accept {
        have: Vec<Vec<u64>>,
    },
    Reject {
        reason: String,
    },
    Progress {
        done: u64,
    },
    Done,
    Error {
        message: String,
    },
}

pub enum Frame {
    Msg(Msg),
    /// Payload is left in the caller's buffer.
    Chunk {
        file: u32,
        index: u64,
        hash: [u8; 32],
    },
}

pub fn chunk_count(size: u64, chunk_size: u64) -> u64 {
    size.div_ceil(chunk_size)
}

pub fn write_msg<W: Write>(w: &mut W, msg: &Msg) -> io::Result<()> {
    let body = serde_json::to_vec(msg)?;
    w.write_all(&[KIND_MSG])?;
    w.write_all(&(body.len() as u32).to_be_bytes())?;
    w.write_all(&body)
}

pub fn write_chunk<W: Write>(
    w: &mut W,
    file: u32,
    index: u64,
    hash: &[u8; 32],
    data: &[u8],
) -> io::Result<()> {
    let mut head = [0u8; 5 + CHUNK_HEADER];
    head[0] = KIND_CHUNK;
    head[1..5].copy_from_slice(&((CHUNK_HEADER + data.len()) as u32).to_be_bytes());
    head[5..9].copy_from_slice(&file.to_be_bytes());
    head[9..17].copy_from_slice(&index.to_be_bytes());
    head[17..].copy_from_slice(hash);
    w.write_all(&head)?;
    w.write_all(data)
}

/// Reads the next frame. A chunk's payload replaces the contents of `buf`
/// (no zero-fill, so pooled buffers stay cheap).
pub fn read_frame<R: Read>(r: &mut R, buf: &mut Vec<u8>) -> io::Result<Frame> {
    let mut head = [0u8; 5];
    r.read_exact(&mut head)?;
    let len = u32::from_be_bytes(head[1..5].try_into().unwrap()) as usize;
    match head[0] {
        KIND_MSG => {
            if len > MAX_MSG {
                return Err(bad("message too large"));
            }
            let _big = (len > BIG_MSG).then(|| Big::take(len)).transpose()?;
            let mut body = vec![0; len];
            r.read_exact(&mut body)?;
            Ok(Frame::Msg(serde_json::from_slice(&body)?))
        }
        KIND_CHUNK => {
            if len < CHUNK_HEADER || (len - CHUNK_HEADER) as u64 > MAX_CHUNK {
                return Err(bad("bad chunk length"));
            }
            let mut h = [0u8; CHUNK_HEADER];
            r.read_exact(&mut h)?;
            // One read_exact of the whole chunk: big reads skip the BufReader's buffer, where
            // read_to_end's small growing ones were copied through it (one extra copy of every byte).
            // Zero-fills only when a pooled buffer grows.
            buf.resize(len - CHUNK_HEADER, 0);
            r.read_exact(buf)?;
            Ok(Frame::Chunk {
                file: u32::from_be_bytes(h[0..4].try_into().unwrap()),
                index: u64::from_be_bytes(h[4..12].try_into().unwrap()),
                hash: h[12..].try_into().unwrap(),
            })
        }
        k => Err(bad(&format!("unknown frame kind {k}"))),
    }
}

/// Reads a frame that must be a message.
pub fn read_msg<R: Read>(r: &mut R) -> io::Result<Msg> {
    match read_frame(r, &mut Vec::new())? {
        Frame::Msg(m) => Ok(m),
        Frame::Chunk { .. } => Err(bad("expected message, got chunk")),
    }
}

/// A share of `BIG_BUDGET`, given back when dropped.
struct Big(usize);

impl Big {
    fn take(n: usize) -> io::Result<Big> {
        if BIG_IN_USE.fetch_add(n, Ordering::SeqCst) + n > BIG_BUDGET {
            BIG_IN_USE.fetch_sub(n, Ordering::SeqCst);
            return Err(io::Error::other("busy with other large offers, try again"));
        }
        Ok(Big(n))
    }
}

impl Drop for Big {
    fn drop(&mut self) {
        BIG_IN_USE.fetch_sub(self.0, Ordering::SeqCst);
    }
}

/// A list of at most `MAX_FILES`, refused while parsing: a 32 MiB message of tiny entries
/// would otherwise take over a GB before the count is checked.
fn bounded<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    struct Bounded<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Bounded<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "a list of at most {MAX_FILES}")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut v = Vec::new();
            while let Some(x) = seq.next_element()? {
                if v.len() == MAX_FILES {
                    return Err(de::Error::custom(format!(
                        "too many files (at most {MAX_FILES} at a time)"
                    )));
                }
                v.push(x);
            }
            Ok(v)
        }
    }
    d.deserialize_seq(Bounded(PhantomData))
}

pub fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// A peer's device name, safe to show next to a pairing code: one line, no control or bidi
/// characters that could hide or reorder text (on screen or in a terminal), at most 64 chars.
pub fn clean_name(name: &str) -> String {
    let bidi = |c: char| matches!(c, '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
    let s: String = name
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|&c| !c.is_control() && !bidi(c))
        .take(64)
        .collect();
    match s.trim() {
        "" => "unknown device".into(),
        t => t.into(),
    }
}

/// A socket timeout reads "Resource temporarily unavailable"; say what happened instead.
pub fn stalled(e: io::Error) -> io::Error {
    match e.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => io::Error::new(
            io::ErrorKind::TimedOut,
            "the other device stopped responding",
        ),
        _ => e,
    }
}

/// Random session id without a rand dependency (RandomState is randomly seeded).
pub fn random_u64() -> u64 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    );
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn roundtrip() {
        let msg = Msg::Offer {
            chunk_size: CHUNK_SIZE,
            reveal: Some("n".into()),
            files: vec![FileEntry {
                path: "a/b.txt".into(),
                size: 5,
                mtime: 1,
            }],
            dirs: vec!["a/empty".into()],
        };
        let mut wire = Vec::new();
        write_msg(&mut wire, &msg).unwrap();
        write_chunk(&mut wire, 7, 9, &[3; 32], b"hello").unwrap();

        let mut r = Cursor::new(wire);
        assert_eq!(read_msg(&mut r).unwrap(), msg);
        let mut buf = Vec::new();
        match read_frame(&mut r, &mut buf).unwrap() {
            Frame::Chunk { file, index, hash } => {
                assert_eq!((file, index, hash), (7, 9, [3; 32]));
                assert_eq!(buf, b"hello");
            }
            Frame::Msg(_) => panic!("expected chunk"),
        }
    }

    #[test]
    fn rejects_bad_frames() {
        let mut wire = vec![KIND_CHUNK];
        wire.extend(((CHUNK_HEADER as u64 + MAX_CHUNK + 1) as u32).to_be_bytes());
        assert!(read_frame(&mut Cursor::new(wire), &mut Vec::new()).is_err());
        assert!(read_frame(&mut Cursor::new(vec![9, 0, 0, 0, 0]), &mut Vec::new()).is_err());
        let mut short = Vec::new();
        write_chunk(&mut short, 0, 0, &[0; 32], b"abc").unwrap();
        short.pop();
        assert!(read_frame(&mut Cursor::new(short), &mut Vec::new()).is_err());
    }

    #[test]
    fn offer_lists_are_capped_while_parsing() {
        let dirs = vec!["\"a\""; MAX_FILES + 1].join(",");
        let json = format!(r#"{{"type":"offer","chunk_size":65536,"files":[],"dirs":[{dirs}]}}"#);
        let err = serde_json::from_str::<Msg>(&json).unwrap_err();
        assert!(err.to_string().contains("too many files"));
    }

    #[test]
    fn names_are_one_safe_line() {
        assert_eq!(
            clean_name("Bob\n\ncode 123\u{1b}[8m\u{202e}x"),
            "Bob  code 123[8mx"
        );
        assert_eq!(clean_name(&"a".repeat(500)).len(), 64);
        assert_eq!(clean_name(" \u{202e} "), "unknown device");
    }
}
