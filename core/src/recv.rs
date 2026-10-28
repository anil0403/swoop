//! Receiver: accept sessions, verify chunks, write files, resume.

use crate::VERSION;
use crate::proto::{self, Caps, FileEntry, Frame, Msg, Role, bad, chunk_count};
use crate::tls::{self, Identity};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

const MIN_CHUNK: u64 = 64 << 10;
/// ponytail: caps per-session chunk bookkeeping (1 byte/chunk, 64 MB); bitsets if offers need more.
const MAX_CHUNKS: u64 = 1 << 26;
const POOL: usize = 5;

/// Accepts connections forever, one thread per connection. Sessions are accepted
/// automatically (pairing comes in phase 2).
pub fn serve(
    listener: TcpListener,
    id: Arc<Identity>,
    opts: RecvOptions,
    on_event: impl Fn(RecvEvent) + Send + Sync + 'static,
) -> io::Result<()> {
    let cfg = tls::server_config(&id)?;
    let ctx = Arc::new(Ctx::new(opts, tls::caps(), on_event));
    accept_loop(listener, ctx, move |tcp| tls::accept(tcp, cfg.clone()))
}

/// Maps an untrusted offered path to a safe relative path, or None if it is absolute,
/// empty, or contains `..`, `.`, empty components, NUL or backslash.
pub fn safe_rel_path(path: &str) -> Option<PathBuf> {
    if path.contains(['\0', '\\']) {
        return None;
    }
    let mut out = PathBuf::new();
    for c in path.split('/') {
        if c.is_empty() || c == "." || c == ".." {
            return None;
        }
        out.push(c);
    }
    Some(out)
}

struct Ctx {
    dir: PathBuf,
    null_sink: bool,
    name: String,
    caps: Caps,
    sessions: Mutex<HashMap<u64, Arc<Session>>>,
    on_event: Box<dyn Fn(RecvEvent) + Send + Sync>,
}

impl Ctx {
    fn new(opts: RecvOptions, caps: Caps, on_event: impl Fn(RecvEvent) + Send + Sync + 'static) -> Ctx {
        Ctx {
            dir: opts.dir,
            null_sink: opts.null_sink,
            name: opts.name,
            caps,
            sessions: Mutex::new(HashMap::new()),
            on_event: Box::new(on_event),
        }
    }
}

fn accept_loop<S, A>(listener: TcpListener, ctx: Arc<Ctx>, accept: A) -> io::Result<()>
where
    S: Read + Write,
    A: Fn(TcpStream) -> io::Result<(S, String)> + Send + Sync + 'static,
{
    let accept = Arc::new(accept);
    for tcp in listener.incoming() {
        let Ok(tcp) = tcp else { continue };
        let (ctx, accept) = (ctx.clone(), accept.clone());
        thread::spawn(move || {
            if let Ok((s, fp)) = accept(tcp) {
                let _ = handle(s, fp, &ctx);
            }
        });
    }
    Ok(())
}

fn send<W: Write>(w: &mut W, msg: &Msg) -> io::Result<()> {
    let mut buf = Vec::new();
    proto::write_msg(&mut buf, msg)?;
    w.write_all(&buf)?;
    w.flush()
}

fn handle<S: Read + Write>(mut s: S, peer_fp: String, ctx: &Ctx) -> io::Result<()> {
    let Msg::Hello { version, session, role, name, .. } = proto::read_msg(&mut s)? else {
        return Err(bad("expected hello"));
    };
    if version != VERSION {
        let message = format!("unsupported protocol version {version}, want {VERSION}");
        return send(&mut s, &Msg::Error { message });
    }
    match role {
        Role::Control => control(s, ctx, session, peer_fp, name),
        Role::Data => {
            let sess = ctx.sessions.lock().unwrap().get(&session).cloned();
            match sess {
                Some(sess) if sess.peer_fp == peer_fp => {
                    let r = data(s, &sess);
                    if let Err(e) = &r {
                        sess.fail(e.to_string());
                    }
                    r
                }
                _ => Ok(()),
            }
        }
    }
}

/// One offered file on disk.
struct Target {
    dest: PathBuf,
    part: PathBuf,
    size: u64,
    mtime: i64,
}

impl Target {
    fn state_path(&self) -> PathBuf {
        let mut p = self.part.clone().into_os_string();
        p.push(".state");
        p.into()
    }
}

#[derive(Serialize, Deserialize)]
struct PartState {
    size: u64,
    mtime: i64,
    chunk_size: u64,
    done: Vec<u64>,
}

struct State {
    done: Vec<Vec<bool>>,
    /// Chunks still missing per file.
    left: Vec<u64>,
    /// Changed since the last persist.
    dirty: Vec<bool>,
    /// Files with chunks missing or not yet renamed into place.
    unfinished: usize,
    error: Option<String>,
}

struct Session {
    peer_fp: String,
    dir: PathBuf,
    chunk_size: u64,
    null_sink: bool,
    files: Vec<Target>,
    st: Mutex<State>,
    /// Bytes written this session.
    written: AtomicU64,
}

impl Session {
    fn fail(&self, e: String) {
        self.st.lock().unwrap().error.get_or_insert(e);
    }

    fn chunk_len(&self, file: usize, index: u64) -> u64 {
        (self.files[file].size - index * self.chunk_size).min(self.chunk_size)
    }

    fn put(&self, file: u32, index: u64, hash: &[u8; 32], data: &[u8], cache: &mut Option<(u32, File)>) -> io::Result<()> {
        let fi = file as usize;
        let t = self.files.get(fi).ok_or_else(|| bad("chunk for unknown file"))?;
        if index >= chunk_count(t.size, self.chunk_size) {
            return Err(bad("chunk index out of range"));
        }
        if data.len() as u64 != self.chunk_len(fi, index) {
            return Err(bad("bad chunk length"));
        }
        if blake3::hash(data) != *hash {
            return Err(bad("chunk hash mismatch"));
        }
        if self.st.lock().unwrap().done[fi][index as usize] {
            return Ok(());
        }
        if !self.null_sink {
            if cache.as_ref().is_none_or(|(f, _)| *f != file) {
                *cache = Some((file, open_part(t)?));
            }
            cache.as_ref().unwrap().1.write_all_at(data, index * self.chunk_size)?;
        }
        let last = {
            let mut st = self.st.lock().unwrap();
            if let Some(e) = &st.error {
                return Err(io::Error::other(e.clone()));
            }
            if std::mem::replace(&mut st.done[fi][index as usize], true) {
                return Ok(());
            }
            st.left[fi] -= 1;
            st.dirty[fi] = true;
            st.left[fi] == 0
        };
        self.written.fetch_add(data.len() as u64, Ordering::Relaxed);
        if last {
            if !self.null_sink {
                *cache = None;
                finalize(t)?;
            }
            self.st.lock().unwrap().unfinished -= 1;
        }
        Ok(())
    }

    /// Writes state files for incomplete files that changed, after a syncfs so a state
    /// file never claims data that is not on disk.
    fn persist(&self) -> io::Result<()> {
        if self.null_sink {
            return Ok(());
        }
        let snap: Vec<(usize, Vec<u64>)> = {
            let mut st = self.st.lock().unwrap();
            let mut snap = Vec::new();
            for i in 0..self.files.len() {
                if st.dirty[i] && st.left[i] > 0 {
                    st.dirty[i] = false;
                    snap.push((i, (0..).zip(&st.done[i]).filter(|(_, d)| **d).map(|(n, _)| n).collect()));
                }
            }
            snap
        };
        if snap.is_empty() {
            return Ok(());
        }
        syncfs(&self.dir)?;
        for (i, done) in &snap {
            let t = &self.files[*i];
            let ps = PartState { size: t.size, mtime: t.mtime, chunk_size: self.chunk_size, done: done.clone() };
            fs::write(t.state_path(), serde_json::to_vec(&ps)?)?;
        }
        // A file finalized meanwhile must not keep the state we just wrote.
        let st = self.st.lock().unwrap();
        for (i, _) in &snap {
            if st.left[*i] == 0 {
                let _ = fs::remove_file(self.files[*i].state_path());
            }
        }
        Ok(())
    }
}

fn syncfs(dir: &Path) -> io::Result<()> {
    Ok(rustix::fs::syncfs(File::open(dir)?)?)
}

fn open_part(t: &Target) -> io::Result<File> {
    if let Some(p) = t.part.parent() {
        fs::create_dir_all(p)?;
    }
    let f = OpenOptions::new().write(true).create(true).truncate(false).open(&t.part)?;
    if f.metadata()?.len() < t.size {
        use rustix::io::Errno;
        match rustix::fs::fallocate(&f, rustix::fs::FallocateFlags::empty(), 0, t.size) {
            Err(Errno::OPNOTSUPP | Errno::NOSYS) => f.set_len(t.size)?,
            r => r?,
        }
    }
    Ok(f)
}

/// Sets mtime and renames the part into place, picking `name (n).ext` on conflict.
// ponytail: no per-file fsync; one syncfs at session end covers data and renames.
fn finalize(t: &Target) -> io::Result<()> {
    if let Some(p) = t.part.parent() {
        fs::create_dir_all(p)?;
    }
    let f = OpenOptions::new().write(true).create(true).truncate(false).open(&t.part)?;
    if f.metadata()?.len() != t.size {
        f.set_len(t.size)?;
    }
    if let Some(m) = system_time(t.mtime) {
        f.set_modified(m)?;
    }
    drop(f);
    let mut dest = t.dest.clone();
    // ponytail: exists-then-rename races with other writers into the same dir; RENAME_NOREPLACE if it matters.
    for n in 1.. {
        if fs::symlink_metadata(&dest).is_err() {
            break;
        }
        dest = numbered(&t.dest, n);
    }
    fs::rename(&t.part, &dest)?;
    match fs::remove_file(t.state_path()) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn numbered(path: &Path, n: u32) -> PathBuf {
    let mut name: OsString = path.file_stem().unwrap_or_default().into();
    name.push(format!(" ({n})"));
    if let Some(ext) = path.extension() {
        name.push(".");
        name.push(ext);
    }
    path.with_file_name(name)
}

fn system_time(secs: i64) -> Option<SystemTime> {
    let d = Duration::from_secs(secs.unsigned_abs());
    if secs >= 0 { UNIX_EPOCH.checked_add(d) } else { UNIX_EPOCH.checked_sub(d) }
}

fn unix_secs(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// Chunks of `t` already on disk: all if the final file matches, else those in a
/// matching state file, else none.
fn resume(t: &Target, chunk_size: u64) -> (bool, Vec<u64>) {
    let n = chunk_count(t.size, chunk_size);
    if let Ok(m) = fs::metadata(&t.dest)
        && m.is_file()
        && m.len() == t.size
        && m.modified().is_ok_and(|mt| unix_secs(mt).abs_diff(t.mtime) <= 2)
    {
        return (true, (0..n).collect());
    }
    let Ok(raw) = fs::read(t.state_path()) else { return (false, Vec::new()) };
    match serde_json::from_slice::<PartState>(&raw) {
        Ok(ps) if t.part.is_file() && (ps.size, ps.mtime, ps.chunk_size) == (t.size, t.mtime, chunk_size) => {
            let mut done: Vec<u64> = ps.done.into_iter().filter(|&i| i < n).collect();
            done.sort_unstable();
            done.dedup();
            (false, done)
        }
        _ => (false, Vec::new()),
    }
}

type Plan = (Session, Vec<Vec<u64>>, Vec<usize>);

/// Validates an offer and builds the session plus `Accept.have` and the files that
/// need finalizing without any chunk. Err(reason) means Reject.
fn plan(
    ctx: &Ctx,
    peer_fp: String,
    chunk_size: u64,
    files: &[FileEntry],
) -> Result<Plan, String> {
    if !(MIN_CHUNK..=proto::MAX_CHUNK).contains(&chunk_size) {
        return Err(format!("chunk size {chunk_size} out of range"));
    }
    if files.len() > u32::MAX as usize {
        return Err("too many files".into());
    }
    let mut seen = HashSet::new();
    let mut chunks = 0u64;
    let mut targets = Vec::with_capacity(files.len());
    for f in files {
        let rel = safe_rel_path(&f.path).ok_or_else(|| format!("unsafe path {:?}", f.path))?;
        if !seen.insert(rel.clone()) {
            return Err(format!("duplicate path {:?}", f.path));
        }
        chunks = chunks.saturating_add(chunk_count(f.size, chunk_size));
        if chunks > MAX_CHUNKS {
            return Err("offer too large".into());
        }
        let dest = ctx.dir.join(&rel);
        let mut part = dest.clone().into_os_string();
        part.push(".uftpart");
        targets.push(Target { dest, part: part.into(), size: f.size, mtime: f.mtime });
    }
    let mut st = State {
        done: Vec::new(),
        left: Vec::new(),
        dirty: vec![false; targets.len()],
        unfinished: 0,
        error: None,
    };
    let (mut have, mut now) = (Vec::new(), Vec::new());
    for (i, t) in targets.iter().enumerate() {
        let n = chunk_count(t.size, chunk_size);
        let (complete, h) = if ctx.null_sink { (false, Vec::new()) } else { resume(t, chunk_size) };
        let mut done = vec![false; n as usize];
        for &c in &h {
            done[c as usize] = true;
        }
        let left = n - h.len() as u64;
        if left > 0 {
            st.unfinished += 1;
        } else if !complete {
            now.push(i);
        }
        st.done.push(done);
        st.left.push(left);
        have.push(h);
    }
    let sess = Session {
        peer_fp,
        dir: ctx.dir.clone(),
        chunk_size,
        null_sink: ctx.null_sink,
        files: targets,
        st: Mutex::new(st),
        written: AtomicU64::new(0),
    };
    Ok((sess, have, now))
}

fn control<S: Read + Write>(mut s: S, ctx: &Ctx, id: u64, peer_fp: String, peer_name: String) -> io::Result<()> {
    let welcome = Msg::Welcome { version: VERSION, caps: ctx.caps.clone(), name: ctx.name.clone() };
    send(&mut s, &welcome)?;
    let Msg::Offer { chunk_size, files } = proto::read_msg(&mut s)? else {
        return Err(bad("expected offer"));
    };
    let (sess, have, now) = match plan(ctx, peer_fp.clone(), chunk_size, &files) {
        Ok(p) => p,
        Err(reason) => return send(&mut s, &Msg::Reject { reason }),
    };
    let sess = Arc::new(sess);
    let resumed: u64 = have.iter().enumerate().map(|(f, h)| h.iter().map(|&i| sess.chunk_len(f, i)).sum::<u64>()).sum();
    let total = files.iter().map(|f| f.size).fold(0u64, u64::saturating_add);
    {
        // Registered before Accept so the sender's data connections never race us.
        let mut reg = ctx.sessions.lock().unwrap();
        if reg.contains_key(&id) {
            drop(reg);
            return send(&mut s, &Msg::Error { message: "session already active".into() });
        }
        reg.insert(id, sess.clone());
    }
    let start = Instant::now();
    let r = send(&mut s, &Msg::Accept { have }).and_then(|()| {
        (ctx.on_event)(RecvEvent::Started { session: id, peer_fp, peer_name, files: files.len(), total, resumed });
        run(&mut s, ctx, id, &sess, now, resumed, total, start)
    });
    ctx.sessions.lock().unwrap().remove(&id);
    if let Err(e) = &r {
        let error = e.to_string();
        sess.fail(error.clone());
        let _ = sess.persist();
        let _ = send(&mut s, &Msg::Error { message: error.clone() });
        (ctx.on_event)(RecvEvent::Failed { session: id, error });
    }
    r
}

#[allow(clippy::too_many_arguments)]
fn run<S: Write>(
    s: &mut S,
    ctx: &Ctx,
    id: u64,
    sess: &Session,
    now: Vec<usize>,
    resumed: u64,
    total: u64,
    start: Instant,
) -> io::Result<()> {
    if !sess.null_sink {
        fs::create_dir_all(&sess.dir)?;
        for i in now {
            finalize(&sess.files[i])?;
        }
    }
    let mut persisted = Instant::now();
    loop {
        let (complete, err) = {
            let st = sess.st.lock().unwrap();
            (st.unfinished == 0, st.error.clone())
        };
        if let Some(e) = err {
            return Err(io::Error::other(e));
        }
        let done = resumed + sess.written.load(Ordering::Relaxed);
        send(s, &Msg::Progress { done })?;
        (ctx.on_event)(RecvEvent::Progress { session: id, done, total });
        if complete {
            if !sess.null_sink {
                syncfs(&sess.dir)?;
            }
            send(s, &Msg::Done)?;
            let bytes = sess.written.load(Ordering::Relaxed);
            (ctx.on_event)(RecvEvent::Done { session: id, bytes, secs: start.elapsed().as_secs_f64() });
            return Ok(());
        }
        if persisted.elapsed() >= Duration::from_secs(2) {
            sess.persist()?;
            persisted = Instant::now();
        }
        thread::sleep(Duration::from_millis(100));
    }
}

type Job = (u32, u64, [u8; 32], Vec<u8>);

/// Reads chunk frames on this thread, verifies and writes them on a second one.
fn data<S: Read>(s: S, sess: &Session) -> io::Result<()> {
    let mut r = BufReader::with_capacity(256 << 10, s);
    let (tx, rx) = sync_channel::<Job>(4);
    let (pool_tx, pool_rx) = channel();
    for _ in 0..POOL {
        pool_tx.send(Vec::with_capacity(sess.chunk_size as usize)).unwrap();
    }
    thread::scope(|sc| {
        let writer = sc.spawn(move || write_loop(sess, rx, pool_tx));
        let read = (|| {
            while !r.fill_buf()?.is_empty() {
                // The writer is gone (error) once the pool and job channel close.
                let Ok(mut buf) = pool_rx.recv() else { break };
                match proto::read_frame(&mut r, &mut buf)? {
                    Frame::Chunk { file, index, hash } => {
                        if tx.send((file, index, hash, buf)).is_err() {
                            break;
                        }
                    }
                    Frame::Msg(_) => return Err(bad("unexpected message on data connection")),
                }
            }
            Ok(())
        })();
        drop(tx);
        let wrote = writer.join().unwrap();
        read.and(wrote)
    })
}

fn write_loop(sess: &Session, rx: Receiver<Job>, pool: Sender<Vec<u8>>) -> io::Result<()> {
    let mut cache = None;
    for (file, index, hash, buf) in rx {
        sess.put(file, index, &hash, &buf, &mut cache)?;
        let _ = pool.send(buf);
    }
    Ok(())
}
