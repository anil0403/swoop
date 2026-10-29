//! What the app calls. Long work runs on its own threads and reports through streams.
//! Sizes and ids are i64 so Dart gets plain ints.

use crate::frb_generated::StreamSink;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use uft::discover::{self, Device, Found};
use uft::link::{self, Link};
use uft::pair::{self, Trust};
use uft::progress::Meter;
use uft::recv::{self, Offer, RecvEvent, RecvOptions};
use uft::send::{self, SendOptions, Source, Streams};
use uft::tls::Identity;
use uft::DEFAULT_PORT;

/// How long an offer waits for the user (the sender waits 120 s).
const ANSWER_TIMEOUT: Duration = Duration::from_secs(110);

struct State {
    id: Arc<Identity>,
    fp: String,
    name: String,
    trust: Mutex<Trust>,
    offers: Mutex<HashMap<i64, Sender<(bool, bool)>>>,
    /// Devices the user accepted while the app runs: a transfer resuming after a dropped
    /// connection isn't asked about again.
    accepted: Mutex<HashMap<String, Instant>>,
    /// Running sends by the app's id, for `cancel`.
    sends: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    /// This device's link and when it was read; Android's comes from the app (`set_link`).
    link: Mutex<(Option<Link>, Option<Instant>)>,
    /// Links other devices announced, by fingerprint, for `diagnose`.
    links: Mutex<HashMap<String, Link>>,
}

/// How long an accepted device's resumed offers pass without asking.
const ACCEPTED_FOR: Duration = Duration::from_secs(30 * 60);
/// How long a send keeps trying to reach the receiver again after the connection drops.
const RECONNECT_FOR: Duration = Duration::from_secs(120);

static STATE: OnceLock<State> = OnceLock::new();
static NEXT_OFFER: AtomicI64 = AtomicI64::new(1);
static RECEIVING: AtomicBool = AtomicBool::new(true);

/// Off: every offer is declined without asking.
// ponytail: still answers discovery, so senders see it and get "declined"; hide it when that confuses people.
#[flutter_rust_bridge::frb(sync)]
pub fn set_receiving(enabled: bool) {
    RECEIVING.store(enabled, Ordering::Relaxed);
}

fn state() -> Result<&'static State, String> {
    STATE.get().ok_or_else(|| "init was not called".to_string())
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

pub struct Me {
    pub name: String,
    pub fingerprint: String,
}

/// Loads or creates this device's identity in `config_dir`. `name` is shown to other devices.
pub fn init(config_dir: String, name: String) -> Result<Me, String> {
    if STATE.get().is_none() {
        let dir = PathBuf::from(config_dir);
        let id = Identity::load_or_create(&dir).map_err(|e| format!("identity: {e}"))?;
        let trust = Trust::load(&dir).map_err(|e| format!("trusted devices: {e}"))?;
        let _ = STATE.set(State {
            fp: id.fingerprint(),
            id: Arc::new(id),
            name,
            trust: Mutex::new(trust),
            offers: Mutex::new(HashMap::new()),
            accepted: Mutex::new(HashMap::new()),
            sends: Mutex::new(HashMap::new()),
            link: Mutex::new((None, None)),
            links: Mutex::new(HashMap::new()),
        });
    }
    let st = state()?;
    Ok(Me {
        name: st.name.clone(),
        fingerprint: st.fp.clone(),
    })
}

#[derive(Clone, Debug)]
pub struct Peer {
    pub name: String,
    pub os: String,
    pub fingerprint: String,
    /// Best address to connect to.
    pub addr: String,
    /// Pairing code both devices show.
    pub code: String,
    /// Its network link, e.g. "Wi-Fi 5 GHz · 866 Mbps · -60 dBm", if it says.
    pub link: Option<String>,
    /// Best case between the two links, bytes/s.
    pub expected: Option<f64>,
}

fn peer(f: &Found, me: &str) -> Peer {
    Peer {
        name: f.device.name.clone(),
        os: f.device.os.clone(),
        fingerprint: f.device.fp.clone(),
        addr: f.addrs[0].to_string(),
        code: pair::code(me, &f.device.fp),
        link: f.device.link.as_ref().map(link::describe),
        expected: f
            .device
            .link
            .as_ref()
            .zip(local_link())
            .and_then(|(a, b)| link::expected(a, &b)),
    }
}

/// This device's link, re-read at most every 10 s (`iw` runs on Linux).
fn local_link() -> Option<Link> {
    let mut l = state().ok()?.link.lock().unwrap();
    if !cfg!(target_os = "android") && l.1.is_none_or(|t| t.elapsed() >= Duration::from_secs(10)) {
        *l = (link::local(), Some(Instant::now()));
    }
    l.0.clone()
}

/// Android's link, from the platform side (Rust can't read it there).
#[flutter_rust_bridge::frb(sync)]
pub fn set_link(kind: Option<String>, mhz: Option<u32>, mbps: Option<u32>, dbm: Option<i32>) {
    if let Ok(st) = state() {
        *st.link.lock().unwrap() = (
            kind.map(|kind| Link {
                kind,
                mhz,
                mbps,
                dbm,
            }),
            Some(Instant::now()),
        );
    }
}

/// "This device: … · Other device: … · up to N MB/s · tip", for the device with `fingerprint`
/// after a transfer at `speed` bytes/s; None when neither link is known.
#[flutter_rust_bridge::frb(sync)]
pub fn diagnose(fingerprint: String, speed: f64) -> Option<String> {
    let me = local_link();
    let peer = state()
        .ok()?
        .links
        .lock()
        .unwrap()
        .get(&fingerprint)
        .cloned();
    if me.is_none() && peer.is_none() {
        return None;
    }
    let side = |l: &Option<Link>| l.as_ref().map_or("unknown".into(), link::describe);
    let mut line = format!("This device: {} · Other device: {}", side(&me), side(&peer));
    if let (Some(a), Some(b)) = (&me, &peer) {
        if let Some(e) = link::expected(a, b) {
            line += &format!(" · up to {:.0} MB/s", e / 1e6);
        }
        if let Some(t) = link::tip(a, b, speed) {
            line += &format!(" · {t}");
        }
    }
    Some(line)
}

/// This device as `swoop://IP:PORT/FINGERPRINT`, one per local network: what to type on a device that can't find it.
#[flutter_rust_bridge::frb(sync)]
pub fn my_links() -> Result<Vec<String>, String> {
    let fp = &state()?.fp;
    Ok(discover::local_ips()
        .iter()
        .map(|ip| format!("swoop://{ip}:{DEFAULT_PORT}/{fp}"))
        .collect())
}

/// A device scanning can't find: a `swoop://HOST:PORT/FINGERPRINT` link (taken as is), or an `IP[:PORT]`
/// (asked who it is, ~2 s).
pub fn connect(target: String) -> Result<Peer, String> {
    let me = &state()?.fp;
    let target = target.trim();
    let with_port = |h: &str| -> Result<SocketAddr, String> {
        h.parse::<SocketAddr>()
            .or_else(|_| {
                h.parse::<Ipv4Addr>()
                    .map(|ip| SocketAddr::from((ip, DEFAULT_PORT)))
            })
            .map_err(|_| format!("{h:?} is not an address like 192.168.1.20"))
    };
    if let Some(link) = target.strip_prefix("swoop://") {
        let (host, fp) = link
            .trim_end_matches('/')
            .rsplit_once('/')
            .filter(|(_, f)| f.len() == 64 && f.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("bad link: expected swoop://HOST:PORT/FINGERPRINT")?;
        let fp = fp.to_ascii_lowercase();
        return Ok(Peer {
            name: host.to_string(),
            os: String::new(),
            code: pair::code(me, &fp),
            fingerprint: fp,
            addr: with_port(host)?.to_string(),
            link: None,
            expected: None,
        });
    }
    let addr = with_port(target)?;
    let std::net::IpAddr::V4(ip) = addr.ip() else {
        return Err("only IPv4 addresses for now".into());
    };
    let found = discover::scan(addr.port(), Duration::from_secs(2), &[ip], |f| {
        f.addrs.iter().any(|a| a.ip() == addr.ip())
    })
    .map_err(|e| e.to_string())?;
    found
        .iter()
        .find(|f| f.device.fp != *me && f.addrs.iter().any(|a| a.ip() == addr.ip()))
        .map(|f| Peer {
            addr: addr.to_string(),
            ..peer(f, me)
        })
        .ok_or_else(|| format!("no Swoop device answered at {addr}; is it receiving?"))
}

/// A stream's start-up error goes into the stream, where the app listens; a returned error would
/// reach Dart as an unhandled exception instead.
fn into_sink<T: Clone + crate::frb_generated::SseEncode>(
    sink: StreamSink<T>,
    start: impl FnOnce(StreamSink<T>) -> Result<(), String>,
) {
    if let Err(e) = start(sink.clone()) {
        let _ = sink.add_error(e);
    }
}

/// Scans for `secs`, sending the whole list of other devices each time it changes.
pub fn scan(secs: u32, sink: StreamSink<Vec<Peer>>) {
    into_sink(sink, |sink| start_scan(secs, sink))
}

fn start_scan(secs: u32, sink: StreamSink<Vec<Peer>>) -> Result<(), String> {
    let me = state()?.fp.clone();
    thread::spawn(move || {
        let mut list: Vec<Peer> = Vec::new();
        let _ = discover::scan(DEFAULT_PORT, Duration::from_secs(secs.into()), &[], |f| {
            if f.device.fp == me {
                return false;
            }
            if let (Some(l), Ok(st)) = (&f.device.link, state()) {
                st.links
                    .lock()
                    .unwrap()
                    .insert(f.device.fp.clone(), l.clone());
            }
            let p = peer(f, &me);
            match list.iter_mut().find(|x| x.fingerprint == p.fingerprint) {
                Some(x) => *x = p,
                None => list.push(p),
            }
            // Stop when nobody listens anymore.
            sink.add(list.clone()).is_err()
        });
    });
    Ok(())
}

#[derive(Clone, Debug)]
pub enum Incoming {
    /// Answer with `answer(id, ..)` within ~2 minutes, or it is declined.
    Offer {
        id: i64,
        peer_name: String,
        fingerprint: String,
        files: i64,
        total: i64,
        code: String,
    },
    Started {
        session: i64,
        peer_name: String,
        fingerprint: String,
        files: i64,
        total: i64,
        resumed: i64,
    },
    Progress {
        session: i64,
        done: i64,
        total: i64,
        speed: f64,
        eta: Option<f64>,
    },
    Done {
        session: i64,
        bytes: i64,
        secs: f64,
    },
    Failed {
        session: i64,
        error: String,
    },
}

/// Receives into `dir` and makes this device discoverable, until the app exits.
pub fn receive(dir: String, sink: StreamSink<Incoming>) {
    into_sink(sink, |sink| start_receive(dir, sink))
}

/// Where incoming events go. A reopened app takes over the running receiver: on Android the process
/// outlives the window when a transfer keeps it alive, and the port is still bound.
static RECV_SINK: Mutex<Option<StreamSink<Incoming>>> = Mutex::new(None);
/// The running receiver's save folder (`set_save_dir`).
static RECV_DIR: LazyLock<Arc<Mutex<PathBuf>>> = LazyLock::new(Default::default);

/// Where received files go from the next offer on; a transfer already running keeps its folder.
#[flutter_rust_bridge::frb(sync)]
pub fn set_save_dir(dir: String) {
    *RECV_DIR.lock().unwrap() = dir.into();
}

fn to_app(ev: Incoming) {
    if let Some(s) = RECV_SINK.lock().unwrap().as_ref() {
        let _ = s.add(ev);
    }
}

fn start_receive(dir: String, sink: StreamSink<Incoming>) -> Result<(), String> {
    let st = state()?;
    set_save_dir(dir);
    if let Some(old) = RECV_SINK.lock().unwrap().as_mut() {
        *old = sink;
        return Ok(());
    }
    let port_err =
        |e: std::io::Error| format!("port {DEFAULT_PORT} is busy ({e}); is Swoop already running?");
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, DEFAULT_PORT)).map_err(port_err)?;
    let udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DEFAULT_PORT)).map_err(port_err)?;
    let me = Device {
        fp: st.fp.clone(),
        name: st.name.clone(),
        os: std::env::consts::OS.into(),
        port: DEFAULT_PORT,
        link: None,
    };
    thread::spawn(move || {
        discover::respond(udp, move || Device {
            link: local_link(),
            ..me.clone()
        })
    });

    *RECV_SINK.lock().unwrap() = Some(sink);
    // Cloned out so the lock isn't held while the user decides.
    let accept: recv::Accept = Arc::new(move |o: &Offer| {
        let sink = RECV_SINK.lock().unwrap().clone();
        sink.is_some_and(|s| ask(o, &s))
    });
    let opts = RecvOptions {
        dir: RECV_DIR.clone(),
        null_sink: false,
        name: st.name.clone(),
        accept: Some(accept),
    };
    let meters: Mutex<HashMap<u64, Meter>> = Mutex::new(HashMap::new());
    let id = st.id.clone();
    thread::spawn(move || {
        recv::serve(listener, id, opts, move |ev| {
            to_app(incoming(ev, &meters));
        })
    });
    Ok(())
}

fn incoming(ev: RecvEvent, meters: &Mutex<HashMap<u64, Meter>>) -> Incoming {
    match ev {
        RecvEvent::Started {
            session,
            peer_fp,
            peer_name,
            files,
            total,
            resumed,
        } => {
            meters
                .lock()
                .unwrap()
                .insert(session, Meter::new(total, resumed));
            Incoming::Started {
                session: session as i64,
                peer_name,
                fingerprint: peer_fp,
                files: files as i64,
                total: total as i64,
                resumed: resumed as i64,
            }
        }
        RecvEvent::Progress {
            session,
            done,
            total,
        } => {
            let snap = meters
                .lock()
                .unwrap()
                .entry(session)
                .or_insert_with(|| Meter::new(total, 0))
                .update(done);
            Incoming::Progress {
                session: session as i64,
                done: done as i64,
                total: total as i64,
                speed: snap.speed,
                eta: snap.eta,
            }
        }
        RecvEvent::Done {
            session,
            bytes,
            secs,
        } => {
            meters.lock().unwrap().remove(&session);
            Incoming::Done {
                session: session as i64,
                bytes: bytes as i64,
                secs,
            }
        }
        RecvEvent::Failed { session, error } => {
            meters.lock().unwrap().remove(&session);
            Incoming::Failed {
                session: session as i64,
                error,
            }
        }
    }
}

/// Trusted devices pass; others wait for the user's `answer`.
fn ask(o: &Offer, sink: &StreamSink<Incoming>) -> bool {
    let Some(st) = STATE.get() else { return false };
    if !RECEIVING.load(Ordering::Relaxed) {
        return false;
    }
    if st.trust.lock().unwrap().contains(&o.peer_fp)
        || st
            .accepted
            .lock()
            .unwrap()
            .get(&o.peer_fp)
            .is_some_and(|t| t.elapsed() < ACCEPTED_FOR)
    {
        return true;
    }
    let id = NEXT_OFFER.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = channel();
    st.offers.lock().unwrap().insert(id, tx);
    let shown = sink
        .add(Incoming::Offer {
            id,
            peer_name: o.peer_name.clone(),
            fingerprint: o.peer_fp.clone(),
            files: o.files as i64,
            total: o.total as i64,
            code: pair::code(&st.fp, &o.peer_fp),
        })
        .is_ok();
    let reply = if shown {
        rx.recv_timeout(ANSWER_TIMEOUT).ok()
    } else {
        None
    };
    st.offers.lock().unwrap().remove(&id);
    match reply {
        Some((true, always)) => {
            st.accepted
                .lock()
                .unwrap()
                .insert(o.peer_fp.clone(), Instant::now());
            if always {
                let _ = st.trust.lock().unwrap().add(&o.peer_fp, &o.peer_name);
            }
            true
        }
        _ => false,
    }
}

/// The user's answer to an `Incoming::Offer`; `always` also trusts the device from now on.
#[flutter_rust_bridge::frb(sync)]
pub fn answer(id: i64, accept: bool, always: bool) {
    if let Some(st) = STATE.get() {
        if let Some(tx) = st.offers.lock().unwrap().remove(&id) {
            let _ = tx.send((accept, always));
        }
    }
}

#[derive(Clone, Debug)]
pub enum Outgoing {
    Progress {
        done: i64,
        total: i64,
        speed: f64,
        peak: f64,
        eta: Option<f64>,
        streams: u32,
    },
    Done {
        bytes: i64,
        resumed: i64,
        secs: f64,
        avg: f64,
        peak: f64,
    },
    /// The connection dropped; looking for the receiver again (maybe on another network) to resume.
    Reconnecting {
        error: String,
    },
    Failed {
        error: String,
    },
}

/// A file or folder to send. `name` set = one file offered under that relative name
/// (Android's picked files, which arrive as `/proc/self/fd/N`); `name` set and `path` empty =
/// an empty folder of that name (from Android's folder picker).
pub struct Pick {
    pub path: String,
    pub name: Option<String>,
}

/// Sends files and folders to `peer`; reports progress, then Done or Failed. `id` is the app's,
/// for `cancel`. A dropped connection is retried for a while, on whichever path finds the
/// receiver; the transfer resumes where it stopped.
pub fn send_files(id: i64, peer: Peer, picks: Vec<Pick>, sink: StreamSink<Outgoing>) {
    into_sink(sink, |sink| start_send(id, peer, picks, sink))
}

fn start_send(
    id: i64,
    peer: Peer,
    picks: Vec<Pick>,
    sink: StreamSink<Outgoing>,
) -> Result<(), String> {
    let st = state()?;
    let mut addr: SocketAddr = peer
        .addr
        .parse()
        .map_err(|e| format!("{}: {e}", peer.addr))?;
    let cancel = Arc::new(AtomicBool::new(false));
    st.sends.lock().unwrap().insert(id, cancel.clone());
    thread::spawn(move || {
        let sources: Vec<Source> = picks
            .into_iter()
            .map(|p| match p.name {
                Some(name) if p.path.is_empty() => Source::Dir(name),
                Some(name) => Source::Named {
                    path: p.path.into(),
                    name,
                },
                None => Source::Path(p.path.into()),
            })
            .collect();
        let opts = SendOptions {
            streams: Streams::Auto,
            name: st.name.clone(),
            cancel: Some(cancel.clone()),
        };
        let mut last_ok = Instant::now();
        let r = loop {
            let r = send::send(
                addr,
                &st.id,
                &peer.fingerprint,
                &sources,
                &opts,
                |s, streams| {
                    last_ok = Instant::now();
                    let _ = sink.add(Outgoing::Progress {
                        done: s.done as i64,
                        total: s.total as i64,
                        speed: s.speed,
                        peak: s.peak,
                        eta: s.eta,
                        streams,
                    });
                },
            );
            match r {
                Err(e) if dropped(&e) && last_ok.elapsed() < RECONNECT_FOR => {
                    let _ = sink.add(Outgoing::Reconnecting {
                        error: e.to_string(),
                    });
                    thread::sleep(Duration::from_secs(2));
                    if cancel.load(Ordering::Relaxed) {
                        break Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            "cancelled",
                        ));
                    }
                    if let Some(a) = find(&peer.fingerprint, addr) {
                        addr = a;
                    }
                }
                r => break r,
            }
        };
        st.sends.lock().unwrap().remove(&id);
        let _ = sink.add(match r {
            Ok(s) => Outgoing::Done {
                bytes: s.bytes as i64,
                resumed: s.resumed as i64,
                secs: s.secs,
                avg: s.avg,
                peak: s.peak,
            },
            Err(e) => Outgoing::Failed {
                error: e.to_string(),
            },
        });
    });
    Ok(())
}

/// Stops the send `id`; the receiver keeps what arrived, so sending again resumes.
#[flutter_rust_bridge::frb(sync)]
pub fn cancel(id: i64) {
    if let Some(c) = STATE
        .get()
        .and_then(|st| st.sends.lock().unwrap().get(&id).cloned())
    {
        c.store(true, Ordering::Relaxed);
    }
}

/// A network failure worth retrying, not a refusal or a local error.
fn dropped(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(
        e.kind(),
        ConnectionReset
            | ConnectionAborted
            | ConnectionRefused
            | BrokenPipe
            | TimedOut
            | WouldBlock
            | UnexpectedEof
            | NotConnected
            | HostUnreachable
            | NetworkUnreachable
            | NetworkDown
    )
}

/// The receiver's best address now (another path if the old one went away), also asking `old`
/// directly for receivers outside the scanned networks.
fn find(fp: &str, old: SocketAddr) -> Option<SocketAddr> {
    let extra: Vec<Ipv4Addr> = match old.ip() {
        std::net::IpAddr::V4(ip) => vec![ip],
        _ => vec![],
    };
    let found = discover::scan(old.port(), Duration::from_secs(3), &extra, |f| {
        f.device.fp == fp
    })
    .ok()?;
    Some(*found.iter().find(|f| f.device.fp == fp)?.addrs.first()?)
}
