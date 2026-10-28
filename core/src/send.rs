//! Sender: offer files, then stripe chunks over adaptive parallel TLS streams.

use crate::progress::Snapshot;
use crate::proto::{self, CHUNK_SIZE, FileEntry, bad, chunk_count};
use crate::tls::Identity;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

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

/// An offered file and where its bytes come from (None = synthetic).
struct Item {
    entry: FileEntry,
    src: Option<PathBuf>,
}

fn collect(sources: &[Source]) -> io::Result<Vec<Item>> {
    let mut out = Vec::new();
    for s in sources {
        match s {
            Source::Synthetic { name, size } => out.push(Item {
                entry: FileEntry { path: name.clone(), size: *size, mtime: 0 },
                src: None,
            }),
            Source::Path(p) => {
                let name = match p.file_name() {
                    Some(n) => n.to_owned(),
                    None => fs::canonicalize(p)?.file_name().ok_or_else(|| bad("path has no name"))?.to_owned(),
                };
                let mut found = Vec::new();
                walk(p, utf8(&name)?, &fs::metadata(p)?, &mut found)?;
                found.sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
                out.extend(found);
            }
        }
    }
    Ok(out)
}

fn walk(path: &Path, rel: String, md: &fs::Metadata, out: &mut Vec<Item>) -> io::Result<()> {
    if md.is_dir() {
        for e in fs::read_dir(path)? {
            let p = e?.path();
            let md = fs::symlink_metadata(&p)?;
            // ponytail: symlinks inside a directory are skipped, not followed or recreated.
            if md.file_type().is_symlink() {
                continue;
            }
            let rel = format!("{rel}/{}", utf8(p.file_name().unwrap())?);
            walk(&p, rel, &md, out)?;
        }
    } else if md.is_file() {
        out.push(Item {
            entry: FileEntry { path: rel, size: md.len(), mtime: md.mtime() },
            src: Some(path.to_owned()),
        });
    }
    Ok(())
}

fn utf8(s: &OsStr) -> io::Result<String> {
    s.to_str().map(str::to_owned).ok_or_else(|| bad(&format!("non-UTF-8 name {}", s.to_string_lossy())))
}

fn chunk_len(size: u64, index: u64) -> u64 {
    CHUNK_SIZE.min(size - index * CHUNK_SIZE)
}

/// Every (file, chunk) the receiver lacks, file-major, plus the bytes it already has.
fn plan(files: &[FileEntry], have: &[Vec<u64>]) -> (Vec<(u32, u64)>, u64) {
    let (mut work, mut resumed) = (Vec::new(), 0);
    for (i, f) in files.iter().enumerate() {
        let have: HashSet<u64> = have.get(i).map(|h| h.iter().copied().collect()).unwrap_or_default();
        for c in 0..chunk_count(f.size, CHUNK_SIZE) {
            if have.contains(&c) {
                resumed += chunk_len(f.size, c);
            } else {
                work.push((i as u32, c));
            }
        }
    }
    (work, resumed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_tree() {
        let root = std::env::temp_dir().join(format!("uft-send-{}", proto::random_u64()));
        let d = root.join("d");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("b.txt"), "bb").unwrap();
        fs::write(d.join("a.txt"), "").unwrap();
        fs::write(d.join("sub/c"), "ccc").unwrap();
        std::os::unix::fs::symlink(d.join("b.txt"), d.join("link")).unwrap();
        fs::write(root.join("single"), "x").unwrap();

        let items = collect(&[
            Source::Path(d.clone()),
            Source::Synthetic { name: "mem".into(), size: 9 },
            Source::Path(root.join("single")),
        ])
        .unwrap();
        let got: Vec<_> = items.iter().map(|i| (i.entry.path.as_str(), i.entry.size)).collect();
        assert_eq!(got, [("d/a.txt", 0), ("d/b.txt", 2), ("d/sub/c", 3), ("mem", 9), ("single", 1)]);
        assert!(items[0].entry.mtime > 0);
        assert_eq!((items[3].entry.mtime, items[3].src.is_none()), (0, true));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_work_around_have() {
        let f = |size| FileEntry { path: String::new(), size, mtime: 0 };
        let files = [f(2 * CHUNK_SIZE + 5), f(0), f(CHUNK_SIZE)];
        assert_eq!(chunk_len(files[0].size, 2), 5);
        assert_eq!(chunk_len(files[2].size, 0), CHUNK_SIZE);

        let (work, resumed) = plan(&files, &[vec![2, 0, 99], vec![], vec![]]);
        assert_eq!(work, [(0, 1), (2, 0)]);
        assert_eq!(resumed, CHUNK_SIZE + 5);
        let (work, resumed) = plan(&files, &[]);
        assert_eq!((work.len(), resumed), (4, 0));
    }
}
