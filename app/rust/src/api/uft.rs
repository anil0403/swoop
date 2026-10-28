//! What the app calls. Long work runs on its own threads and reports through streams.
//! Sizes and ids are i64 so Dart gets plain ints.

use crate::frb_generated::StreamSink;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use uft::discover::{self, Device, Found};
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
}

static STATE: OnceLock<State> = OnceLock::new();
static NEXT_OFFER: AtomicI64 = AtomicI64::new(1);

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
}

fn peer(f: &Found, me: &str) -> Peer {
    Peer {
        name: f.device.name.clone(),
        os: f.device.os.clone(),
        fingerprint: f.device.fp.clone(),
        addr: f.addrs[0].to_string(),
        code: pair::code(me, &f.device.fp),
    }
}

/// Scans for `secs`, sending the whole list of other devices each time it changes.
pub fn scan(secs: u32, sink: StreamSink<Vec<Peer>>) -> Result<(), String> {
    let me = state()?.fp.clone();
    thread::spawn(move || {
        let mut list: Vec<Peer> = Vec::new();
        let _ = discover::scan(DEFAULT_PORT, Duration::from_secs(secs.into()), &[], |f| {
            if f.device.fp == me {
                return false;
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
        files: i64,
        total: i64,
        code: String,
    },
    Started {
        session: i64,
        peer_name: String,
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
pub fn receive(dir: String, sink: StreamSink<Incoming>) -> Result<(), String> {
    let st = state()?;
    let port_err = |e: std::io::Error| format!("port {DEFAULT_PORT} is busy ({e}); is uft already running?");
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, DEFAULT_PORT)).map_err(port_err)?;
    let udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DEFAULT_PORT)).map_err(port_err)?;
    let me = Device {
        fp: st.fp.clone(),
        name: st.name.clone(),
        os: std::env::consts::OS.into(),
        port: DEFAULT_PORT,
    };
    thread::spawn(move || discover::respond(udp, &me));

    let asks = sink.clone();
    let accept: recv::Accept = Arc::new(move |o: &Offer| ask(o, &asks));
    let opts = RecvOptions {
        dir: dir.into(),
        null_sink: false,
        name: st.name.clone(),
        accept: Some(accept),
    };
    let meters: Mutex<HashMap<u64, Meter>> = Mutex::new(HashMap::new());
    let id = st.id.clone();
    thread::spawn(move || {
        recv::serve(listener, id, opts, move |ev| {
            let _ = sink.add(incoming(ev, &meters));
        })
    });
    Ok(())
}

fn incoming(ev: RecvEvent, meters: &Mutex<HashMap<u64, Meter>>) -> Incoming {
    match ev {
        RecvEvent::Started {
            session,
            peer_name,
            files,
            total,
            resumed,
            ..
        } => {
            meters.lock().unwrap().insert(session, Meter::new(total, resumed));
            Incoming::Started {
                session: session as i64,
                peer_name,
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
    if st.trust.lock().unwrap().contains(&o.peer_fp) {
        return true;
    }
    let id = NEXT_OFFER.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = channel();
    st.offers.lock().unwrap().insert(id, tx);
    let shown = sink
        .add(Incoming::Offer {
            id,
            peer_name: o.peer_name.clone(),
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
    Failed {
        error: String,
    },
}

/// Sends files and folders to `peer`; reports progress, then Done or Failed.
pub fn send_files(peer: Peer, paths: Vec<String>, sink: StreamSink<Outgoing>) -> Result<(), String> {
    let st = state()?;
    let addr: SocketAddr = peer.addr.parse().map_err(|e| format!("{}: {e}", peer.addr))?;
    thread::spawn(move || {
        let sources: Vec<Source> = paths.into_iter().map(|p| Source::Path(p.into())).collect();
        let opts = SendOptions {
            streams: Streams::Auto,
            name: st.name.clone(),
        };
        let r = send::send(addr, &st.id, &peer.fingerprint, &sources, &opts, |s, streams| {
            let _ = sink.add(Outgoing::Progress {
                done: s.done as i64,
                total: s.total as i64,
                speed: s.speed,
                peak: s.peak,
                eta: s.eta,
                streams,
            });
        });
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
