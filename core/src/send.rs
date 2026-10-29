//! Sender: offer files, then stripe chunks over adaptive parallel TLS streams.

use crate::VERSION;
use crate::progress::{Meter, Snapshot};
use crate::proto::{self, CHUNK_SIZE, FileEntry, Msg, Role, bad, chunk_count};
use crate::tls::{self, Identity};
use rustls::ClientConfig;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

pub enum Source {
    /// A file, or a directory sent recursively under its own name.
    Path(PathBuf),
    /// One file read from `path` but offered as `name` (a relative path like `album/a.jpg`).
    /// Android hands picked files over as `/proc/self/fd/N`, which carry no useful name.
    Named { path: PathBuf, name: String },
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
    /// Set to stop the transfer; `send` then fails with `Interrupted`. The receiver keeps
    /// what arrived, so sending again resumes.
    pub cancel: Option<Arc<AtomicBool>>,
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
    mut on_progress: impl FnMut(&Snapshot, u32),
) -> io::Result<SendStats> {
    let items = collect(sources)?;
    let files: Vec<FileEntry> = items.iter().map(|i| i.entry.clone()).collect();
    let total = files.iter().map(|f| f.size).sum();
    let cfg = tls::client_config(id, server_fp)?;
    let session = proto::random_u64();
    let caps = tls::caps();
    let hello = |role| Msg::Hello {
        version: VERSION,
        session,
        role,
        caps: caps.clone(),
        name: opts.name.clone(),
    };

    let mut ctrl = tls::connect(addr, cfg.clone())?;
    proto::write_msg(&mut ctrl, &hello(Role::Control))?;
    ctrl.flush()?;
    match proto::read_msg(&mut ctrl)? {
        Msg::Welcome {
            version: VERSION, ..
        } => {}
        Msg::Welcome { version, .. } => {
            return Err(bad(&format!("receiver speaks protocol {version}")));
        }
        m => return Err(refused(m)),
    }
    proto::write_msg(
        &mut ctrl,
        &Msg::Offer {
            chunk_size: CHUNK_SIZE,
            files: files.clone(),
        },
    )?;
    ctrl.flush()?;
    // The receiver may be asking its user.
    ctrl.sock.set_read_timeout(Some(Duration::from_secs(120)))?;
    let have = match proto::read_msg(&mut ctrl)? {
        Msg::Accept { have } => have,
        m => return Err(refused(m)),
    };
    ctrl.sock.set_read_timeout(Some(Duration::from_secs(30)))?;
    let (work, resumed) = plan(&files, &have);
    let job = Job {
        items,
        work,
        next: AtomicUsize::new(0),
        abort: AtomicBool::new(false),
        ctrl: ctrl.sock.try_clone()?,
        cfg,
        addr,
        hello: hello(Role::Data),
    };

    let start = Instant::now();
    let mut meter = Meter::new(total, resumed);
    let mut shown: Option<Instant> = None;
    let mut probing = opts.streams == Streams::Auto && !job.work.is_empty();
    let (mut win_start, mut win_done, mut rate_at_add) = (start, resumed, None);

    thread::scope(|s| {
        let mut handles = Vec::new();
        let initial = match opts.streams {
            _ if job.work.is_empty() => 0,
            Streams::Fixed(n) => n.max(1),
            Streams::Auto => 1,
        };
        for _ in 0..initial {
            handles.push(s.spawn(|| stream(&job)));
        }

        let cancelled = || {
            opts.cancel
                .as_ref()
                .is_some_and(|c| c.load(Ordering::Relaxed))
        };
        let res = loop {
            match proto::read_msg(&mut ctrl) {
                Ok(Msg::Progress { .. }) if cancelled() => {
                    job.abort.store(true, Ordering::SeqCst);
                    let _ = job.ctrl.shutdown(std::net::Shutdown::Both);
                    break Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
                }
                Ok(Msg::Progress { done }) => {
                    let snap = meter.update(done);
                    let now = Instant::now();
                    if shown.is_none_or(|t| now - t >= Duration::from_millis(100)) {
                        on_progress(&snap, handles.len() as u32);
                        shown = Some(now);
                    }
                    if probing && now - win_start >= WINDOW {
                        let rate =
                            done.saturating_sub(win_done) as f64 / (now - win_start).as_secs_f64();
                        if should_add(
                            handles.len() as u32,
                            rate,
                            rate_at_add,
                            total.saturating_sub(done),
                        ) {
                            rate_at_add = Some(rate);
                            handles.push(s.spawn(|| stream(&job)));
                        } else {
                            probing = false;
                        }
                        (win_start, win_done) = (now, done);
                    }
                }
                Ok(Msg::Done) => break Ok(()),
                Ok(m) => break Err(refused(m)),
                Err(e) => break Err(e),
            }
        };

        // A stream that failed first set `abort` and shut the control socket; its error explains ours.
        let stream_failed = res.is_err() && job.abort.swap(true, Ordering::SeqCst);
        let streams = handles.len() as u32;
        let mut stream_err = None;
        for h in handles {
            if let Err(e) = h
                .join()
                .unwrap_or_else(|_| Err(io::Error::other("stream panicked")))
            {
                stream_err.get_or_insert(e);
            }
        }
        // After Done the receiver has verified everything, so late stream errors don't matter.
        if let Err(e) = res {
            if e.kind() == io::ErrorKind::Interrupted {
                return Err(e);
            }
            return Err(stream_err.filter(|_| stream_failed).unwrap_or(e));
        }

        let snap = meter.update(total);
        on_progress(&snap, streams);
        let secs = start.elapsed().as_secs_f64();
        let bytes = total - resumed;
        Ok(SendStats {
            bytes,
            resumed,
            secs,
            avg: bytes as f64 / secs,
            peak: snap.peak,
            streams,
        })
    })
}

const MAX_STREAMS: u32 = 8;
/// Auto mode measures the acked rate over windows this long.
const WINDOW: Duration = Duration::from_secs(2);

/// Auto mode: after a window at `rate` bytes/s, open another stream? False ends probing.
fn should_add(streams: u32, rate: f64, rate_at_last_add: Option<f64>, remaining: u64) -> bool {
    streams < MAX_STREAMS
        && remaining as f64 > rate * 2.0 * WINDOW.as_secs_f64()
        && rate_at_last_add.is_none_or(|r| rate >= 1.05 * r)
}

fn refused(m: Msg) -> io::Error {
    match m {
        Msg::Reject { reason } => io::Error::other(format!("receiver rejected: {reason}")),
        Msg::Error { message } => io::Error::other(format!("receiver error: {message}")),
        m => bad(&format!("unexpected message {m:?}")),
    }
}

/// State shared by all data streams of a session.
struct Job {
    items: Vec<Item>,
    work: Vec<(u32, u64)>,
    /// Index of the next unclaimed `work` entry.
    next: AtomicUsize,
    abort: AtomicBool,
    /// Clone of the control socket, shut down to wake the control loop on abort.
    ctrl: TcpStream,
    cfg: Arc<ClientConfig>,
    addr: SocketAddr,
    hello: Msg,
}

type Chunk = (u32, u64, [u8; 32], Vec<u8>);

// ponytail: a failed stream aborts the session instead of requeueing its chunks; resume covers it.
fn stream(job: &Job) -> io::Result<()> {
    let res = run_stream(job);
    if res.is_err() && !job.abort.swap(true, Ordering::SeqCst) {
        let _ = job.ctrl.shutdown(Shutdown::Both);
    }
    res
}

/// One data connection: a reader thread (disk + hash) feeding this thread (TLS writes).
fn run_stream(job: &Job) -> io::Result<()> {
    let mut w = BufWriter::with_capacity(256 << 10, tls::connect(job.addr, job.cfg.clone())?);
    proto::write_msg(&mut w, &job.hello)?;
    let (tx, rx) = mpsc::sync_channel::<Chunk>(2);
    let (pool_tx, pool_rx) = mpsc::channel();
    for _ in 0..3 {
        pool_tx
            .send(Vec::with_capacity(CHUNK_SIZE as usize))
            .unwrap();
    }
    thread::scope(|s| {
        let reader = s.spawn(|| read_chunks(job, tx, pool_rx));
        // Owns rx and pool_tx so a failed write drops them and unblocks the reader.
        let wrote = (move || {
            for (file, index, hash, buf) in rx {
                proto::write_chunk(&mut w, file, index, &hash, &buf)?;
                let _ = pool_tx.send(buf);
            }
            w.flush()?;
            w.get_mut().conn.send_close_notify();
            w.flush()
        })();
        reader
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("reader panicked")))?;
        wrote
    })
}

fn read_chunks(job: &Job, tx: SyncSender<Chunk>, pool: Receiver<Vec<u8>>) -> io::Result<()> {
    let mut open: Option<(u32, File)> = None;
    while !job.abort.load(Ordering::Relaxed) {
        let Some(&(file, index)) = job.work.get(job.next.fetch_add(1, Ordering::Relaxed)) else {
            break;
        };
        let Ok(mut buf) = pool.recv() else { break };
        let item = &job.items[file as usize];
        let len = chunk_len(item.entry.size, index) as usize;
        match &item.src {
            // Pattern is written once per buffer; later chunks reuse it.
            None if buf.len() != len => {
                buf.clear();
                buf.resize(len, 0xa5);
            }
            None => {}
            Some(path) => {
                if open.as_ref().is_none_or(|(f, _)| *f != file) {
                    let f = open_src(path)?;
                    let _ = rustix::fs::fadvise(&f, 0, None, rustix::fs::Advice::Sequential);
                    open = Some((file, f));
                }
                // Same length as last time for all but a file's final chunk, so no zero-fill.
                buf.resize(len, 0);
                open.as_ref()
                    .unwrap()
                    .1
                    .read_exact_at(&mut buf, index * CHUNK_SIZE)?;
            }
        }
        let hash = *blake3::hash(&buf).as_bytes();
        if tx.send((file, index, hash, buf)).is_err() {
            break;
        }
    }
    Ok(())
}

/// Opens a file to send. `/proc/self/fd/N` is duplicated, not reopened: Android hands picked files over
/// as descriptors the app may have no right to reopen by path.
fn open_src(path: &Path) -> io::Result<File> {
    let fd = path
        .to_str()
        .and_then(|p| p.strip_prefix("/proc/self/fd/")?.parse().ok());
    match fd {
        // SAFETY: only dup()ed here; a closed or bogus number fails with EBADF.
        Some(fd) => Ok(unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) }
            .try_clone_to_owned()?
            .into()),
        None => File::open(path),
    }
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
                entry: FileEntry {
                    path: name.clone(),
                    size: *size,
                    mtime: 0,
                },
                src: None,
            }),
            Source::Named { path, name } => {
                let md = open_src(path)?.metadata()?;
                if !md.is_file() {
                    return Err(bad(&format!("{name}: not a file")));
                }
                out.push(Item {
                    entry: FileEntry {
                        path: name.clone(),
                        size: md.len(),
                        mtime: md.mtime(),
                    },
                    src: Some(path.clone()),
                });
            }
            Source::Path(p) => {
                let name = match p.file_name() {
                    Some(n) => n.to_owned(),
                    None => fs::canonicalize(p)?
                        .file_name()
                        .ok_or_else(|| bad("path has no name"))?
                        .to_owned(),
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
            entry: FileEntry {
                path: rel,
                size: md.len(),
                mtime: md.mtime(),
            },
            src: Some(path.to_owned()),
        });
    }
    Ok(())
}

fn utf8(s: &OsStr) -> io::Result<String> {
    s.to_str()
        .map(str::to_owned)
        .ok_or_else(|| bad(&format!("non-UTF-8 name {}", s.to_string_lossy())))
}

fn chunk_len(size: u64, index: u64) -> u64 {
    CHUNK_SIZE.min(size - index * CHUNK_SIZE)
}

/// Every (file, chunk) the receiver lacks, file-major, plus the bytes it already has.
fn plan(files: &[FileEntry], have: &[Vec<u64>]) -> (Vec<(u32, u64)>, u64) {
    let (mut work, mut resumed) = (Vec::new(), 0);
    for (i, f) in files.iter().enumerate() {
        let have: HashSet<u64> = have
            .get(i)
            .map(|h| h.iter().copied().collect())
            .unwrap_or_default();
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
            Source::Synthetic {
                name: "mem".into(),
                size: 9,
            },
            Source::Path(root.join("single")),
        ])
        .unwrap();
        let got: Vec<_> = items
            .iter()
            .map(|i| (i.entry.path.as_str(), i.entry.size))
            .collect();
        assert_eq!(
            got,
            [
                ("d/a.txt", 0),
                ("d/b.txt", 2),
                ("d/sub/c", 3),
                ("mem", 9),
                ("single", 1)
            ]
        );
        assert!(items[0].entry.mtime > 0);
        assert_eq!((items[3].entry.mtime, items[3].src.is_none()), (0, true));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn named_file_from_fd_path() {
        let path = std::env::temp_dir().join(format!("uft-fd-{}", proto::random_u64()));
        fs::write(&path, "hello").unwrap();
        let f = File::open(&path).unwrap();
        let fd = PathBuf::from(format!(
            "/proc/self/fd/{}",
            std::os::fd::AsRawFd::as_raw_fd(&f)
        ));
        let items = collect(&[Source::Named {
            path: fd,
            name: "album/hi.txt".into(),
        }])
        .unwrap();
        assert_eq!(
            (items[0].entry.path.as_str(), items[0].entry.size),
            ("album/hi.txt", 5)
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn plans_work_around_have() {
        let f = |size| FileEntry {
            path: String::new(),
            size,
            mtime: 0,
        };
        let files = [f(2 * CHUNK_SIZE + 5), f(0), f(CHUNK_SIZE)];
        assert_eq!(chunk_len(files[0].size, 2), 5);
        assert_eq!(chunk_len(files[2].size, 0), CHUNK_SIZE);

        let (work, resumed) = plan(&files, &[vec![2, 0, 99], vec![], vec![]]);
        assert_eq!(work, [(0, 1), (2, 0)]);
        assert_eq!(resumed, CHUNK_SIZE + 5);
        let (work, resumed) = plan(&files, &[]);
        assert_eq!((work.len(), resumed), (4, 0));
    }

    #[test]
    fn auto_stream_decision() {
        let big = u64::MAX;
        assert!(should_add(1, 100.0, None, big));
        assert!(should_add(2, 105.0, Some(100.0), big));
        assert!(!should_add(2, 104.0, Some(100.0), big));
        assert!(!should_add(MAX_STREAMS, 1e9, Some(1.0), big));
        assert!(!should_add(1, 100.0, None, 300));
    }

    #[test]
    fn reads_chunks_at_offsets() {
        let path = std::env::temp_dir().join(format!("uft-read-{}", proto::random_u64()));
        let data: Vec<u8> = (0..CHUNK_SIZE + 10).map(|i| (i % 251) as u8).collect();
        fs::write(&path, &data).unwrap();
        let mut items = collect(&[Source::Path(path.clone())]).unwrap();
        items.extend(
            collect(&[Source::Synthetic {
                name: "m".into(),
                size: 7,
            }])
            .unwrap(),
        );
        let files: Vec<_> = items.iter().map(|i| i.entry.clone()).collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let job = Job {
            items,
            work: plan(&files, &[]).0,
            next: AtomicUsize::new(0),
            abort: AtomicBool::new(false),
            ctrl: TcpStream::connect(listener.local_addr().unwrap()).unwrap(),
            cfg: Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(rustls::RootCertStore::empty())
                    .with_no_client_auth(),
            ),
            addr: listener.local_addr().unwrap(),
            hello: Msg::Done,
        };

        let (tx, rx) = mpsc::sync_channel(2);
        let (pool_tx, pool_rx) = mpsc::channel();
        for _ in 0..3 {
            pool_tx
                .send(Vec::with_capacity(CHUNK_SIZE as usize))
                .unwrap();
        }
        let got = thread::scope(|s| {
            let r = s.spawn(|| read_chunks(&job, tx, pool_rx));
            let got: Vec<_> = rx
                .iter()
                .map(|(file, index, hash, buf)| {
                    assert_eq!(hash, *blake3::hash(&buf).as_bytes());
                    let c = (file, index, buf.clone());
                    let _ = pool_tx.send(buf);
                    c
                })
                .collect();
            r.join().unwrap().unwrap();
            got
        });
        fs::remove_file(path).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].2, data[..CHUNK_SIZE as usize]);
        assert_eq!(
            (got[1].0, got[1].1, &got[1].2[..]),
            (0, 1, &data[CHUNK_SIZE as usize..])
        );
        assert_eq!((got[2].0, got[2].2.len()), (1, 7));
        assert!(got[2].2.iter().all(|&b| b != 0));
    }
}
