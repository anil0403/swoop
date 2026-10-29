//! Receiver: accept sessions, verify chunks, write files, resume.

use crate::VERSION;
use crate::os::{self, FileExt};
use crate::pair;
use crate::proto::{self, Caps, FileEntry, Frame, Msg, Role, bad, chunk_count};
use crate::tls::{self, Identity};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct RecvOptions {
    /// Where received files go. May be changed while serving: each new offer uses the current one.
    pub dir: Arc<Mutex<PathBuf>>,
    /// Verify but discard data (benchmarks).
    pub null_sink: bool,
    /// Device name shown to senders.
    pub name: String,
    /// Asked before each offer is planned. None = accept every offer. May block (e.g. to
    /// ask the user) for up to ~2 minutes; the sender waits that long.
    pub accept: Option<Accept>,
}

/// What a sender offers, as shown to the `accept` callback.
#[derive(Debug, Clone)]
pub struct Offer {
    pub peer_fp: String,
    pub peer_name: String,
    pub files: usize,
    pub total: u64,
    /// The pairing code both screens show for this connection; None from a sender that
    /// predates them, whose offers only devices that trust it should take.
    pub code: Option<String>,
}

/// Returns whether to accept an offer.
pub type Accept = Arc<dyn Fn(&Offer) -> bool + Send + Sync>;

#[derive(Debug, Clone)]
pub enum RecvEvent {
    Started {
        session: u64,
        peer_fp: String,
        peer_name: String,
        files: usize,
        total: u64,
        resumed: u64,
        /// The save folder this one writes to (the one when it was offered).
        dir: PathBuf,
    },
    Progress {
        session: u64,
        done: u64,
        total: u64,
    },
    Done {
        session: u64,
        bytes: u64,
        secs: f64,
    },
    Failed {
        session: u64,
        error: String,
    },
}

const MIN_CHUNK: u64 = 64 << 10;
/// ponytail: caps per-session chunk bookkeeping (1 byte/chunk, 64 MB); bitsets if offers need more.
const MAX_CHUNKS: u64 = 1 << 26;
/// Connections handled at once: a few senders' streams (8 each) and their control links.
const MAX_CONNS: usize = 64;
/// From one address: two transfers' worth, so one machine can't take every slot.
const MAX_CONNS_PER_IP: usize = 20;
const POOL: usize = 5;

/// Accepts connections forever, one thread per connection. Each offer goes to
/// `opts.accept` first, if set.
pub fn serve(
    listener: TcpListener,
    id: Arc<Identity>,
    opts: RecvOptions,
    on_event: impl Fn(RecvEvent) + Send + Sync + 'static,
) -> io::Result<()> {
    let cfg = tls::server_config(&id)?;
    let ctx = Arc::new(Ctx::new(opts, tls::caps(), id.fingerprint(), on_event));
    accept_loop(listener, ctx, move |tcp| tls::accept(tcp, cfg.clone()))
}

/// Maps an untrusted offered path to a safe relative path, or None if it is absolute,
/// empty, or contains `..`, `.`, empty components, NUL or backslash. On Windows, names it
/// can't hold (`a:b`, `CON`, a trailing dot) are changed to ones it can.
pub fn safe_rel_path(path: &str) -> Option<PathBuf> {
    if path.contains(['\0', '\\']) {
        return None;
    }
    let mut out = PathBuf::new();
    for c in path.split('/') {
        if c.is_empty() || c == "." || c == ".." {
            return None;
        }
        #[cfg(windows)]
        out.push(windows_name(c));
        #[cfg(not(windows))]
        out.push(c);
    }
    Some(out)
}

/// `name` made legal on Windows: `<>:"|?*` and control characters become `_` (`:` would
/// otherwise mean a drive or an NTFS stream), as do trailing dots and spaces (which Windows
/// drops), and device names like `CON` or `com1.txt` get a `_` in front.
#[cfg(any(windows, test))]
fn windows_name(name: &str) -> String {
    let mut s: Vec<char> = name
        .chars()
        .map(|c| if c < ' ' || "<>:\"|?*".contains(c) { '_' } else { c })
        .collect();
    for c in s.iter_mut().rev() {
        if *c != '.' && *c != ' ' {
            break;
        }
        *c = '_';
    }
    let s: String = s.into_iter().collect();
    let stem = s.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
        || ["COM", "LPT"].iter().any(|p| {
            stem.strip_prefix(p)
                .is_some_and(|d| matches!(d, "¹" | "²" | "³"))
        });
    if device { format!("_{s}") } else { s }
}

struct Ctx {
    /// This device's fingerprint, for pairing codes.
    fp: String,
    dir: Arc<Mutex<PathBuf>>,
    null_sink: bool,
    name: String,
    accept: Option<Accept>,
    caps: Caps,
    sessions: Mutex<HashMap<u64, Arc<Session>>>,
    on_event: Box<dyn Fn(RecvEvent) + Send + Sync>,
}

impl Ctx {
    fn new(
        opts: RecvOptions,
        caps: Caps,
        fp: String,
        on_event: impl Fn(RecvEvent) + Send + Sync + 'static,
    ) -> Ctx {
        Ctx {
            fp,
            dir: opts.dir,
            null_sink: opts.null_sink,
            name: opts.name,
            accept: opts.accept,
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
    let live: Live = Arc::default();
    for tcp in listener.incoming() {
        let Ok(tcp) = tcp else {
            // Out of descriptors and the like: don't spin.
            thread::sleep(Duration::from_millis(50));
            continue;
        };
        // A thread per connection, before TLS or any trust: capped, so a flood can't use them all up.
        let ip = tcp
            .peer_addr()
            .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |a| a.ip());
        {
            let mut m = live.lock().unwrap();
            if m.values().sum::<usize>() >= MAX_CONNS || m.get(&ip) >= Some(&MAX_CONNS_PER_IP) {
                continue;
            }
            *m.entry(ip).or_default() += 1;
        }
        let (ctx, accept, slot) = (ctx.clone(), accept.clone(), Slot(live.clone(), ip));
        // Builder, not thread::spawn: no thread left is a dropped connection, not a dead receiver.
        let _ = thread::Builder::new().spawn(move || {
            let _slot = slot;
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

/// Connections being handled, by address.
type Live = Arc<Mutex<HashMap<IpAddr, usize>>>;

/// Frees a connection slot when its thread ends (or never starts: dropped with the closure).
struct Slot(Live, IpAddr);

impl Drop for Slot {
    fn drop(&mut self) {
        let mut m = self.0.lock().unwrap();
        if let Some(n) = m.get_mut(&self.1) {
            *n -= 1;
            if *n == 0 {
                m.remove(&self.1);
            }
        }
    }
}

fn handle<S: Read + Write>(mut s: S, peer_fp: String, ctx: &Ctx) -> io::Result<()> {
    // Anyone can connect: the first message is small, so a stranger can't make us take in 32 MiB.
    let Msg::Hello {
        version,
        session,
        role,
        name,
        commit,
        ..
    } = proto::read_msg(&mut (&mut s).take(64 << 10))?
    else {
        return Err(bad("expected hello"));
    };
    if version != VERSION {
        let message = format!("unsupported protocol version {version}, want {VERSION}");
        return send(&mut s, &Msg::Error { message });
    }
    match role {
        Role::Control => control(s, ctx, session, peer_fp, proto::clean_name(&name), commit),
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
    /// The device whose chunks these are: another one's are never spliced in.
    #[serde(default)]
    peer: String,
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

    fn put(
        &self,
        file: u32,
        index: u64,
        hash: &[u8; 32],
        data: &[u8],
        cache: &mut Option<(u32, File)>,
    ) -> io::Result<()> {
        let fi = file as usize;
        let t = self
            .files
            .get(fi)
            .ok_or_else(|| bad("chunk for unknown file"))?;
        if index >= chunk_count(t.size, self.chunk_size) {
            return Err(bad("chunk index out of range"));
        }
        if data.len() as u64 != self.chunk_len(fi, index) {
            return Err(bad("bad chunk length"));
        }
        if blake3::hash(data) != *hash {
            return Err(bad("chunk hash mismatch"));
        }
        {
            // Checked before open_part too: a stream still running after the session failed would
            // otherwise leave a new, preallocated .uftpart behind.
            let st = self.st.lock().unwrap();
            if let Some(e) = &st.error {
                return Err(io::Error::other(e.clone()));
            }
            if st.done[fi][index as usize] {
                return Ok(());
            }
        }
        if !self.null_sink {
            if cache.as_ref().is_none_or(|(f, _)| *f != file) {
                *cache = Some((file, open_part(t)?));
            }
            cache
                .as_ref()
                .unwrap()
                .1
                .write_all_at(data, index * self.chunk_size)?;
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
                    snap.push((
                        i,
                        (0..)
                            .zip(&st.done[i])
                            .filter(|(_, d)| **d)
                            .map(|(n, _)| n)
                            .collect(),
                    ));
                }
            }
            snap
        };
        if snap.is_empty() {
            return Ok(());
        }
        let parts: Vec<&Path> = snap
            .iter()
            .map(|(i, _)| self.files[*i].part.as_path())
            .collect();
        os::sync(&self.dir, &parts).map_err(at(&self.dir))?;
        for (i, done) in &snap {
            let t = &self.files[*i];
            let ps = PartState {
                peer: self.peer_fp.clone(),
                size: t.size,
                mtime: t.mtime,
                chunk_size: self.chunk_size,
                done: done.clone(),
            };
            let sp = t.state_path();
            fs::write(&sp, serde_json::to_vec(&ps)?).map_err(at(&sp))?;
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

/// Puts the path into an I/O error, so "No such file or directory" says which.
fn at(path: &Path) -> impl Fn(io::Error) -> io::Error + '_ {
    move |e| io::Error::new(e.kind(), format!("{}: {e}", path.display()))
}

fn open_part(t: &Target) -> io::Result<File> {
    if let Some(p) = t.part.parent() {
        fs::create_dir_all(p)?;
    }
    let f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&t.part)
        .map_err(at(&t.part))?;
    if f.metadata()?.len() < t.size {
        os::preallocate(&f, t.size)?;
    }
    Ok(f)
}

/// Sets mtime and renames the part into place, picking `name (n).ext` on conflict.
// ponytail: no per-file fsync on Unix; one syncfs at session end covers data and renames.
// Windows has no syncfs, so each file is flushed here, before its rename.
fn finalize(t: &Target) -> io::Result<()> {
    if let Some(p) = t.part.parent() {
        fs::create_dir_all(p)?;
    }
    let f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&t.part)
        .map_err(at(&t.part))?;
    if f.metadata()?.len() != t.size {
        f.set_len(t.size)?;
    }
    if let Some(m) = system_time(t.mtime) {
        // Android's shared storage (Download/) refuses to set times on some versions; keep the file.
        // ponytail: such a file then looks changed to a resend, which saves a "name (1)" copy.
        match f.set_modified(m) {
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {}
            r => r?,
        }
    }
    #[cfg(windows)]
    f.sync_all()?;
    drop(f);
    let mut dest = t.dest.clone();
    // ponytail: exists-then-rename races with other writers into the same dir; RENAME_NOREPLACE if it matters.
    for n in 1.. {
        if fs::symlink_metadata(&dest).is_err() {
            break;
        }
        dest = numbered(&t.dest, n);
    }
    fs::rename(&t.part, &dest).map_err(at(&dest))?;
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
    if secs >= 0 {
        UNIX_EPOCH.checked_add(d)
    } else {
        UNIX_EPOCH.checked_sub(d)
    }
}

fn unix_secs(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// Chunks of `t` already on disk: all if the final file matches, else those in a
/// matching state file `peer` wrote, else none.
fn resume(t: &Target, chunk_size: u64, peer: &str) -> (bool, Vec<u64>) {
    let n = chunk_count(t.size, chunk_size);
    if let Ok(m) = fs::metadata(&t.dest)
        && m.is_file()
        && m.len() == t.size
        && m.modified()
            .is_ok_and(|mt| unix_secs(mt).abs_diff(t.mtime) <= 2)
    {
        return (true, (0..n).collect());
    }
    let Ok(raw) = fs::read(t.state_path()) else {
        return (false, Vec::new());
    };
    match serde_json::from_slice::<PartState>(&raw) {
        Ok(ps)
            if t.part.is_file()
                && ps.peer == peer
                && (ps.size, ps.mtime, ps.chunk_size) == (t.size, t.mtime, chunk_size) =>
        {
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
fn plan(ctx: &Ctx, peer_fp: String, chunk_size: u64, files: &[FileEntry]) -> Result<Plan, String> {
    if !(MIN_CHUNK..=proto::MAX_CHUNK).contains(&chunk_size) {
        return Err(format!("chunk size {chunk_size} out of range"));
    }
    if files.len() > proto::MAX_FILES {
        return Err(format!(
            "too many files (at most {} at a time)",
            proto::MAX_FILES
        ));
    }
    // Read once: a change of folder mid-offer doesn't split it.
    let dir = ctx.dir.lock().unwrap().clone();
    let mut seen = HashSet::new();
    let mut chunks = 0u64;
    let mut targets = Vec::with_capacity(files.len());
    for f in files {
        let rel = safe_rel_path(&f.path).ok_or_else(|| format!("unsafe path {:?}", f.path))?;
        // Windows file names ignore case: A.txt and a.txt are the same file.
        let key = rel.to_string_lossy();
        #[cfg(windows)]
        let key = key.to_lowercase();
        // Swoop's own part and state names: a received one could pass for another transfer's.
        if key.ends_with(".uftpart") || key.ends_with(".uftpart.state") {
            return Err(format!("unsafe path {:?}", f.path));
        }
        if !seen.insert(key.to_string()) {
            return Err(format!("duplicate path {:?}", f.path));
        }
        chunks = chunks.saturating_add(chunk_count(f.size, chunk_size));
        if chunks > MAX_CHUNKS {
            return Err("offer too large".into());
        }
        let dest = dir.join(&rel);
        let mut part = dest.clone().into_os_string();
        part.push(".uftpart");
        targets.push(Target {
            dest,
            part: part.into(),
            size: f.size,
            mtime: f.mtime,
        });
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
        let (complete, h) = if ctx.null_sink {
            (false, Vec::new())
        } else {
            resume(t, chunk_size, &peer_fp)
        };
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
    if !ctx.null_sink {
        // Parts that exist were preallocated when they started.
        let need = targets
            .iter()
            .zip(&st.left)
            .filter(|(t, left)| **left > 0 && !t.part.exists())
            .map(|(t, _)| t.size)
            .fold(0u64, u64::saturating_add);
        let big = targets.iter().find(|t| t.size >= 4 << 30);
        fits(&dir, need, big.map(|t| (&t.dest, t.size)))?;
    }
    let sess = Session {
        peer_fp,
        dir,
        chunk_size,
        null_sink: ctx.null_sink,
        files: targets,
        st: Mutex::new(st),
        written: AtomicU64::new(0),
    };
    Ok((sess, have, now))
}

/// Room for `need` more bytes in `dir`, and no file of 4 GiB or more on FAT32.
// ponytail: Android shared storage is FUSE, so an SD card's FAT32 isn't seen there; the
// write then fails at 4 GiB with an error instead of this message.
fn fits(dir: &Path, need: u64, big: Option<(&PathBuf, u64)>) -> Result<(), String> {
    // The folder may not exist yet; its nearest existing parent is on the same disk.
    let Some(d) = dir.ancestors().find(|d| d.exists()) else {
        return Ok(());
    };
    let Some(free) = os::free_space(d) else {
        return Ok(());
    };
    if need > free {
        return Err(format!(
            "not enough space: needs {} MB, {} MB free",
            need.div_ceil(1_000_000),
            free / 1_000_000
        ));
    }
    if let Some((dest, size)) = big
        && os::is_fat(d)
    {
        return Err(format!(
            "the save folder is on FAT32, which can't hold files of 4 GB or more ({} is {} MB)",
            dest.file_name().unwrap_or_default().to_string_lossy(),
            size / 1_000_000
        ));
    }
    Ok(())
}

fn control<S: Read + Write>(
    mut s: S,
    ctx: &Ctx,
    id: u64,
    peer_fp: String,
    peer_name: String,
    commit: Option<String>,
) -> io::Result<()> {
    let nonce = commit.as_ref().map(|_| pair::nonce()).transpose()?;
    let welcome = Msg::Welcome {
        version: VERSION,
        caps: ctx.caps.clone(),
        name: ctx.name.clone(),
        nonce: nonce.clone(),
    };
    send(&mut s, &welcome)?;
    let Msg::Offer {
        chunk_size,
        reveal,
        files,
        dirs,
    } = proto::read_msg(&mut s)?
    else {
        return Err(bad("expected offer"));
    };
    let code = match (commit, nonce, reveal) {
        (Some(c), Some(n), Some(r)) if pair::commit(&r) == c => {
            Some(pair::code(&peer_fp, &ctx.fp, &r, &n))
        }
        (Some(_), ..) => return Err(bad("pairing nonce doesn't match its commit")),
        _ => None,
    };
    // Ones with names that can't be made safely are skipped; they hold no files.
    let dirs: Vec<_> = dirs.iter().filter_map(|d| safe_rel_path(d)).collect();
    let total = files.iter().map(|f| f.size).fold(0u64, u64::saturating_add);
    // Planned first: an offer that can't fit is refused without asking the user.
    let (sess, have, now) = match plan(ctx, peer_fp.clone(), chunk_size, &files) {
        Ok(p) => p,
        Err(reason) => return send(&mut s, &Msg::Reject { reason }),
    };
    if let Some(accept) = &ctx.accept {
        let offer = Offer {
            peer_fp: peer_fp.clone(),
            peer_name: peer_name.clone(),
            files: files.len(),
            total,
            code,
        };
        if !accept(&offer) {
            let reason = "declined by receiver".into();
            return send(&mut s, &Msg::Reject { reason });
        }
    }
    let sess = Arc::new(sess);
    let resumed: u64 = have
        .iter()
        .enumerate()
        .map(|(f, h)| h.iter().map(|&i| sess.chunk_len(f, i)).sum::<u64>())
        .sum();
    {
        // Registered before Accept so the sender's data connections never race us.
        let mut reg = ctx.sessions.lock().unwrap();
        if reg.contains_key(&id) {
            drop(reg);
            return send(
                &mut s,
                &Msg::Error {
                    message: "session already active".into(),
                },
            );
        }
        reg.insert(id, sess.clone());
    }
    let start = Instant::now();
    let r = send(&mut s, &Msg::Accept { have }).and_then(|()| {
        (ctx.on_event)(RecvEvent::Started {
            session: id,
            peer_fp,
            peer_name,
            files: files.len(),
            total,
            resumed,
            dir: sess.dir.clone(),
        });
        if !sess.null_sink {
            for d in &dirs {
                // Best effort: one that clashes with an existing file shouldn't cost the files.
                let _ = fs::create_dir_all(sess.dir.join(d));
            }
        }
        run(&mut s, ctx, id, &sess, now, resumed, total, start)
    });
    ctx.sessions.lock().unwrap().remove(&id);
    let r = r.map_err(proto::stalled);
    if let Err(e) = &r {
        let error = e.to_string();
        sess.fail(error.clone());
        let _ = sess.persist();
        let _ = send(
            &mut s,
            &Msg::Error {
                message: error.clone(),
            },
        );
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
    // Off the progress loop: with data still arriving, a syncfs can take longer than the sender
    // waits for progress (30 s).
    let stop = AtomicBool::new(false);
    thread::scope(|sc| {
        sc.spawn(|| {
            let mut persisted = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(100));
                if persisted.elapsed() >= Duration::from_secs(2) {
                    if let Err(e) = sess.persist() {
                        return sess.fail(e.to_string());
                    }
                    persisted = Instant::now();
                }
            }
        });
        let r = progress(s, ctx, id, sess, resumed, total, start);
        stop.store(true, Ordering::Relaxed);
        r
    })
}

fn progress<S: Write>(
    s: &mut S,
    ctx: &Ctx,
    id: u64,
    sess: &Session,
    resumed: u64,
    total: u64,
    start: Instant,
) -> io::Result<()> {
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
        (ctx.on_event)(RecvEvent::Progress {
            session: id,
            done,
            total,
        });
        if complete {
            if !sess.null_sink {
                // Windows: finalize() flushed each file, so there's nothing left to list.
                os::sync(&sess.dir, &[]).map_err(at(&sess.dir))?;
            }
            send(s, &Msg::Done)?;
            let bytes = sess.written.load(Ordering::Relaxed);
            (ctx.on_event)(RecvEvent::Done {
                session: id,
                bytes,
                secs: start.elapsed().as_secs_f64(),
            });
            return Ok(());
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
        pool_tx
            .send(Vec::with_capacity(sess.chunk_size as usize))
            .unwrap();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufWriter;
    use std::net::SocketAddr;

    const CS: u64 = 64 << 10;

    struct F {
        path: &'static str,
        data: Vec<u8>,
        mtime: i64,
    }

    fn f(path: &'static str, len: usize) -> F {
        F {
            path,
            data: (0..len).map(|i| (i * 31 + i / 997) as u8).collect(),
            mtime: 1_600_000_000,
        }
    }

    fn caps() -> Caps {
        Caps {
            aes: false,
            cores: 1,
            os: "test".into(),
        }
    }

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("uft-recv-{name}-{:x}", proto::random_u64()))
    }

    /// Plain-TCP receiver; events go to the returned channel.
    fn start(dir: &Path, null_sink: bool) -> (SocketAddr, Receiver<RecvEvent>) {
        start_with(dir, null_sink, None)
    }

    fn start_with(
        dir: &Path,
        null_sink: bool,
        accept: Option<Accept>,
    ) -> (SocketAddr, Receiver<RecvEvent>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (tx, rx) = channel();
        let opts = RecvOptions {
            dir: Arc::new(Mutex::new(dir.into())),
            null_sink,
            name: "rx".into(),
            accept,
        };
        let ctx = Arc::new(Ctx::new(opts, caps(), "me".into(), move |e| {
            let _ = tx.send(e);
        }));
        thread::spawn(move || accept_loop(l, ctx, |tcp| Ok((tcp, "peer".to_string()))));
        (addr, rx)
    }

    /// The test sender's pairing nonce.
    const NONCE: &str = "n";

    fn hello(addr: SocketAddr, session: u64, role: Role) -> TcpStream {
        let mut s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        send(
            &mut s,
            &Msg::Hello {
                version: VERSION,
                session,
                role,
                caps: caps(),
                name: "tx".into(),
                commit: (role == Role::Control).then(|| pair::commit(NONCE)),
            },
        )
        .unwrap();
        s
    }

    fn offer(addr: SocketAddr, session: u64, chunk_size: u64, files: &[F]) -> (TcpStream, Msg) {
        offer_revealing(addr, session, chunk_size, files, NONCE)
    }

    fn offer_revealing(
        addr: SocketAddr,
        session: u64,
        chunk_size: u64,
        files: &[F],
        reveal: &str,
    ) -> (TcpStream, Msg) {
        let mut c = hello(addr, session, Role::Control);
        assert!(matches!(
            proto::read_msg(&mut c).unwrap(),
            Msg::Welcome { .. }
        ));
        let files = files
            .iter()
            .map(|f| FileEntry {
                path: f.path.into(),
                size: f.data.len() as u64,
                mtime: f.mtime,
            })
            .collect();
        let dirs = vec![];
        send(
            &mut c,
            &Msg::Offer {
                chunk_size,
                reveal: Some(reveal.into()),
                files,
                dirs,
            },
        )
        .unwrap();
        let reply = proto::read_msg(&mut c).unwrap_or(Msg::Done);
        (c, reply)
    }

    /// Sends the chunks not in `have` in reverse order, alternating over two data
    /// connections (optionally corrupting one hash), and returns the final control message.
    fn transfer(
        addr: SocketAddr,
        session: u64,
        mut c: TcpStream,
        files: &[F],
        have: &[Vec<u64>],
        corrupt: bool,
    ) -> Msg {
        let mut jobs = Vec::new();
        for (fi, file) in files.iter().enumerate() {
            for (i, data) in file.data.chunks(CS as usize).enumerate() {
                if !have[fi].contains(&(i as u64)) {
                    jobs.push((fi as u32, i as u64, data));
                }
            }
        }
        jobs.reverse();
        let mut conns: Vec<_> = (0..2)
            .map(|_| BufWriter::new(hello(addr, session, Role::Data)))
            .collect();
        for (n, (fi, i, data)) in jobs.into_iter().enumerate() {
            let mut h = *blake3::hash(data).as_bytes();
            if corrupt && n == 0 {
                h[0] ^= 1;
            }
            let _ = proto::write_chunk(&mut conns[n % 2], fi, i, &h, data);
        }
        drop(conns);
        loop {
            match proto::read_msg(&mut c).unwrap() {
                Msg::Progress { .. } => {}
                m => return m,
            }
        }
    }

    fn events(rx: &Receiver<RecvEvent>) -> Vec<RecvEvent> {
        let mut out = Vec::new();
        loop {
            let e = rx.recv_timeout(Duration::from_secs(10)).unwrap();
            let end = matches!(e, RecvEvent::Done { .. } | RecvEvent::Failed { .. });
            out.push(e);
            if end {
                return out;
            }
        }
    }

    fn accepted(reply: Msg) -> Vec<Vec<u64>> {
        match reply {
            Msg::Accept { have } => have,
            m => panic!("expected accept, got {m:?}"),
        }
    }

    fn mtime(p: &Path) -> i64 {
        unix_secs(fs::metadata(p).unwrap().modified().unwrap())
    }

    #[test]
    fn transfers_files() {
        let dir = tmp("basic");
        let (addr, rx) = start(&dir, false);
        let files = [
            f("empty.txt", 0),
            f("big.bin", 5 * CS as usize + 123),
            f("sub/deep/c.txt", 1000),
        ];
        let (c, reply) = offer(addr, 1, CS, &files);
        let have = accepted(reply);
        assert_eq!(have, vec![Vec::<u64>::new(); 3]);
        assert_eq!(transfer(addr, 1, c, &files, &have, false), Msg::Done);
        for file in &files {
            let p = dir.join(file.path);
            assert_eq!(fs::read(&p).unwrap(), file.data, "{}", file.path);
            assert_eq!(mtime(&p), file.mtime);
        }
        assert!(!dir.join("big.bin.uftpart").exists());
        let ev = events(&rx);
        assert!(matches!(
            &ev[0],
            RecvEvent::Started {
                files: 3,
                resumed: 0,
                dir: d,
                ..
            } if *d == dir
        ));
        let total: u64 = files.iter().map(|f| f.data.len() as u64).sum();
        assert!(matches!(ev.last(), Some(RecvEvent::Done { bytes, .. }) if *bytes == total));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bad_hash_fails() {
        let dir = tmp("badhash");
        let (addr, rx) = start(&dir, false);
        let files = [f("a.bin", 2 * CS as usize)];
        let (c, reply) = offer(addr, 2, CS, &files);
        let have = accepted(reply);
        assert!(matches!(
            transfer(addr, 2, c, &files, &have, true),
            Msg::Error { .. }
        ));
        assert!(matches!(events(&rx).last(), Some(RecvEvent::Failed { .. })));
        assert!(!dir.join("a.bin").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn refuses_what_wont_fit() {
        let dir = tmp("fits").join("not/yet/made");
        assert!(fits(&dir, 0, None).is_ok());
        let e = fits(&dir, u64::MAX, None).unwrap_err();
        assert!(e.starts_with("not enough space"), "{e}");
    }

    #[test]
    fn rejects_bad_offers() {
        let dir = tmp("reject");
        let (addr, _rx) = start(&dir, false);
        assert!(matches!(
            offer(addr, 3, CS, &[f("../x", 1)]).1,
            Msg::Reject { .. }
        ));
        assert!(matches!(
            offer(addr, 4, 1024, &[f("x", 1)]).1,
            Msg::Reject { .. }
        ));
        assert!(matches!(
            offer(addr, 5, CS, &[f("x", 1), f("x", 1)]).1,
            Msg::Reject { .. }
        ));
        assert!(matches!(
            offer(addr, 7, CS, &[f("a/x.uftpart.state", 1)]).1,
            Msg::Reject { .. }
        ));
        assert!(!dir.exists());
    }

    #[test]
    fn declined_offer_is_rejected() {
        let dir = tmp("decline");
        let seen = Arc::new(Mutex::new(None));
        let s = seen.clone();
        let accept: Accept = Arc::new(move |o: &Offer| {
            *s.lock().unwrap() = Some(o.clone());
            false
        });
        let (addr, _rx) = start_with(&dir, false, Some(accept));
        let reply = offer(addr, 12, CS, &[f("a", 10), f("b/c", 5)]).1;
        assert_eq!(
            reply,
            Msg::Reject {
                reason: "declined by receiver".into()
            }
        );
        let o = seen.lock().unwrap().take().unwrap();
        assert_eq!(
            (o.peer_fp.as_str(), o.peer_name.as_str(), o.files, o.total),
            ("peer", "tx", 2, 15)
        );
        assert!(o.code.is_some_and(|c| c.len() == 7));
        assert!(!dir.exists());
    }

    #[test]
    fn nonce_not_matching_its_commit_is_refused() {
        let dir = tmp("commit");
        let asked = Arc::new(AtomicBool::new(false));
        let a = asked.clone();
        let accept: Accept = Arc::new(move |_: &Offer| {
            a.store(true, Ordering::SeqCst);
            true
        });
        let (addr, _rx) = start_with(&dir, false, Some(accept));
        let reply = offer_revealing(addr, 13, CS, &[f("a", 10)], "other").1;
        assert!(!matches!(reply, Msg::Accept { .. }));
        assert!(!asked.load(Ordering::SeqCst));
    }

    #[test]
    fn resumes_from_state() {
        let dir = tmp("resume");
        let (addr, rx) = start(&dir, false);
        let files = [f("r.bin", 3 * CS as usize + 10)];
        let size = files[0].data.len() as u64;
        fs::create_dir_all(&dir).unwrap();
        let mut part = vec![0u8; size as usize];
        for i in [0usize, 2] {
            let r = i * CS as usize..(i + 1) * CS as usize;
            part[r.clone()].copy_from_slice(&files[0].data[r]);
        }
        fs::write(dir.join("r.bin.uftpart"), part).unwrap();
        let ps = PartState {
            peer: "peer".into(),
            size,
            mtime: files[0].mtime,
            chunk_size: CS,
            done: vec![2, 0, 9],
        };
        fs::write(
            dir.join("r.bin.uftpart.state"),
            serde_json::to_vec(&ps).unwrap(),
        )
        .unwrap();

        let (c, reply) = offer(addr, 6, CS, &files);
        let have = accepted(reply);
        assert_eq!(have, vec![vec![0, 2]]);
        assert_eq!(transfer(addr, 6, c, &files, &have, false), Msg::Done);
        assert_eq!(fs::read(dir.join("r.bin")).unwrap(), files[0].data);
        assert!(!dir.join("r.bin.uftpart.state").exists());
        let ev = events(&rx);
        assert!(matches!(ev[0], RecvEvent::Started { resumed, .. } if resumed == 2 * CS));
        assert!(
            matches!(ev.last(), Some(RecvEvent::Done { bytes, .. }) if *bytes == size - 2 * CS)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn interrupted_session_persists_state() {
        let dir = tmp("interrupt");
        let (addr, rx) = start(&dir, false);
        let files = [f("i.bin", 3 * CS as usize)];
        let (mut c, reply) = offer(addr, 10, CS, &files);
        accepted(reply);
        let mut d = hello(addr, 10, Role::Data);
        let chunk = &files[0].data[CS as usize..2 * CS as usize];
        proto::write_chunk(&mut d, 0, 1, blake3::hash(chunk).as_bytes(), chunk).unwrap();
        while proto::read_msg(&mut c).unwrap() != (Msg::Progress { done: CS }) {}
        drop((c, d));
        assert!(matches!(events(&rx).last(), Some(RecvEvent::Failed { .. })));

        let (c, reply) = offer(addr, 11, CS, &files);
        let have = accepted(reply);
        assert_eq!(have, vec![vec![1]]);
        assert_eq!(transfer(addr, 11, c, &files, &have, false), Msg::Done);
        assert_eq!(fs::read(dir.join("i.bin")).unwrap(), files[0].data);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn complete_final_has_all() {
        let dir = tmp("complete");
        let (addr, rx) = start(&dir, false);
        let files = [f("done.bin", CS as usize + 1)];
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("done.bin");
        fs::write(&p, &files[0].data).unwrap();
        File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(system_time(files[0].mtime + 1).unwrap())
            .unwrap();

        let (c, reply) = offer(addr, 7, CS, &files);
        let have = accepted(reply);
        assert_eq!(have, vec![vec![0, 1]]);
        assert_eq!(transfer(addr, 7, c, &files, &have, false), Msg::Done);
        assert!(!dir.join("done (1).bin").exists());
        assert!(matches!(
            events(&rx).last(),
            Some(RecvEvent::Done { bytes: 0, .. })
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn renames_on_conflict() {
        let dir = tmp("conflict");
        let (addr, _rx) = start(&dir, false);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "old").unwrap();
        fs::write(dir.join("a (1).txt"), "older").unwrap();
        let files = [f("a.txt", 100)];
        let (c, reply) = offer(addr, 8, CS, &files);
        let have = accepted(reply);
        assert_eq!(transfer(addr, 8, c, &files, &have, false), Msg::Done);
        assert_eq!(fs::read(dir.join("a.txt")).unwrap(), b"old");
        assert_eq!(fs::read(dir.join("a (2).txt")).unwrap(), files[0].data);
        assert_eq!(numbered(Path::new("d/noext"), 1), Path::new("d/noext (1)"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn null_sink_counts_without_disk() {
        let dir = tmp("null");
        let (addr, rx) = start(&dir, true);
        let files = [f("x.bin", 3 * CS as usize), f("y/z", 0)];
        let (c, reply) = offer(addr, 9, CS, &files);
        let have = accepted(reply);
        assert_eq!(transfer(addr, 9, c, &files, &have, false), Msg::Done);
        assert!(!dir.exists());
        assert!(
            matches!(events(&rx).last(), Some(RecvEvent::Done { bytes, .. }) if *bytes == 3 * CS)
        );
    }

    #[test]
    fn safe_paths() {
        assert_eq!(safe_rel_path("a/b.txt"), Some(PathBuf::from("a/b.txt")));
        assert_eq!(safe_rel_path("..a/.b"), Some(PathBuf::from("..a/.b")));
        #[cfg(windows)]
        assert!(safe_rel_path("C:/Windows").is_some_and(|p| p.is_relative()));
        for p in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "./a",
            "a/.",
            "a//b",
            "a/",
            "a\\b",
            "a\0b",
            "..",
        ] {
            assert_eq!(safe_rel_path(p), None, "{p:?}");
        }
    }

    #[test]
    fn windows_names() {
        for (from, to) in [
            ("a.txt", "a.txt"),
            ("C:", "C_"),
            ("a:stream", "a_stream"),
            ("<>\"|?*\x01", "_______"),
            ("a.", "a_"),
            ("a .. ", "a____"),
            ("CON", "_CON"),
            ("con.txt", "_con.txt"),
            ("Com1.tar.gz", "_Com1.tar.gz"),
            ("lpt¹", "_lpt¹"),
            ("nul .x", "_nul .x"),
            ("COM10", "COM10"),
            ("console", "console"),
        ] {
            assert_eq!(windows_name(from), to, "{from:?}");
        }
    }
}
