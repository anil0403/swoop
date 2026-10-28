//! LAN discovery with no settings (UDP, IPv4).
//!
//! A receiver answers `query` datagrams on its UDP port (same number as its TCP port) with an
//! `announce`. A scanner sends queries from every local interface to the multicast group, the
//! broadcast addresses and every host of the interface's /24 — the unicast sweep still works
//! where an access point drops multicast and broadcast between clients. Replies come back unicast,
//! and the reply's source address is where the receiver's TCP port is reached.
//!
//! ```text
//! datagram = JSON  {"type":"query","v":1}
//!                  {"type":"announce","v":1,"fp":"…","name":"…","os":"linux","port":47474}
//! ```

use crate::VERSION;
use serde::{Deserialize, Serialize};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};

/// Administratively scoped multicast group (never leaves the site).
pub const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 47, 74);
/// Time between query rounds.
const ROUND: Duration = Duration::from_secs(1);
/// The /24 sweep runs every this many rounds (it costs an ARP lookup per host).
const SWEEP_EVERY: u32 = 3;
/// How often a responder rejoins the group, to cover interfaces that came up later.
const REJOIN: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Device {
    /// Cert fingerprint (`tls::fingerprint`), pinned when connecting.
    pub fp: String,
    pub name: String,
    pub os: String,
    /// TCP port of the receiver.
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub device: Device,
    /// Where to connect, best path first: Ethernet, then Wi-Fi, then the rest.
    pub addrs: Vec<SocketAddr>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Packet {
    Query {
        v: u32,
    },
    Announce {
        v: u32,
        #[serde(flatten)]
        device: Device,
    },
}

/// Answers queries on `sock` (bound to `0.0.0.0:PORT`) forever.
pub fn respond(sock: UdpSocket, me: &Device) -> io::Result<()> {
    let reply = serde_json::to_vec(&Packet::Announce {
        v: VERSION,
        device: me.clone(),
    })?;
    sock.set_read_timeout(Some(REJOIN))?;
    let mut joined: Option<Instant> = None;
    let mut buf = [0u8; 1024];
    loop {
        if joined.is_none_or(|t| t.elapsed() >= REJOIN) {
            for i in interfaces() {
                // Fails harmlessly when already joined.
                let _ = sock.join_multicast_v4(&GROUP, &i.ip);
            }
            joined = Some(Instant::now());
        }
        match sock.recv_from(&mut buf) {
            Ok((n, src)) => {
                if let Ok(Packet::Query { .. }) = serde_json::from_slice(&buf[..n]) {
                    let _ = sock.send_to(&reply, src);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e),
        }
    }
}

/// Scans for up to `timeout` for receivers on UDP `port`. `extra` hosts are queried directly too
/// (a host the user typed; loopback in tests). `on_found` sees a device when first heard and again
/// when it gains an address; returning true stops the scan early.
pub fn scan(
    port: u16,
    timeout: Duration,
    extra: &[Ipv4Addr],
    mut on_found: impl FnMut(&Found) -> bool,
) -> io::Result<Vec<Found>> {
    // One socket per interface: bound to the interface address, multicast and 255.255.255.255
    // leave through that interface. The unbound one is for `extra`.
    let mut socks = Vec::new();
    for i in interfaces() {
        if let Ok(s) = UdpSocket::bind((i.ip, 0)) {
            s.set_broadcast(true)?;
            socks.push((s, Some(i)));
        }
    }
    socks.push((UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?, None));
    for (s, _) in &socks {
        s.set_read_timeout(Some(Duration::from_millis(100)))?;
    }
    let query = serde_json::to_vec(&Packet::Query { v: VERSION })?;
    let deadline = Instant::now() + timeout;
    let stop = Arc::new(AtomicBool::new(false));
    // Senders are detached: a sweep can block for seconds on ARP (unresolved hosts hold the send
    // buffer), and that must neither delay replies nor an early return.
    for (s, iface) in &socks {
        let to: Vec<Ipv4Addr> = match iface {
            None => extra.to_vec(),
            Some(i) => vec![GROUP, Ipv4Addr::BROADCAST, broadcast(i.ip, i.prefix)],
        };
        let sweep: Vec<Ipv4Addr> = iface.iter().flat_map(|i| sweep(i.ip, i.prefix)).collect();
        let (s, stop, query) = (s.try_clone()?, stop.clone(), query.clone());
        thread::spawn(move || {
            for round in 0.. {
                let next = Instant::now() + ROUND;
                let swept = if round % SWEEP_EVERY == 0 {
                    &sweep[..]
                } else {
                    &[]
                };
                for t in to.iter().chain(swept) {
                    if stop.load(Ordering::Relaxed) || Instant::now() >= deadline {
                        return;
                    }
                    let _ = s.send_to(&query, (*t, port));
                }
                while Instant::now() < next {
                    if stop.load(Ordering::Relaxed) || Instant::now() >= deadline {
                        return;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        });
    }
    let mut found: Vec<(Device, Vec<(u8, SocketAddr)>)> = Vec::new();
    let (tx, rx) = channel();
    thread::scope(|sc| {
        for (s, iface) in &socks {
            let (tx, stop) = (tx.clone(), &stop);
            let rank = iface.as_ref().map_or(2, |i| i.rank);
            sc.spawn(move || {
                let mut buf = [0u8; 1024];
                while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
                    if let Ok((n, src)) = s.recv_from(&mut buf)
                        && let Some(d) = parse(&buf[..n])
                    {
                        let addr = SocketAddr::new(src.ip(), d.port);
                        let _ = tx.send((d, addr, rank));
                    }
                }
            });
        }
        drop(tx);
        while let Ok((d, addr, rank)) =
            rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            let i = match found.iter().position(|(f, _)| f.fp == d.fp) {
                Some(i) if found[i].1.iter().any(|&(_, a)| a == addr) => continue,
                Some(i) => i,
                None => {
                    found.push((d.clone(), Vec::new()));
                    found.len() - 1
                }
            };
            let (dev, addrs) = &mut found[i];
            *dev = d;
            addrs.push((rank, addr));
            addrs.sort_by_key(|&(r, _)| r);
            if on_found(&to_found(&found[i])) {
                break;
            }
        }
        stop.store(true, Ordering::Relaxed);
    });
    Ok(found.iter().map(to_found).collect())
}

fn to_found((device, addrs): &(Device, Vec<(u8, SocketAddr)>)) -> Found {
    Found {
        device: device.clone(),
        addrs: addrs.iter().map(|&(_, a)| a).collect(),
    }
}

fn parse(b: &[u8]) -> Option<Device> {
    match serde_json::from_slice(b).ok()? {
        Packet::Announce { device: d, .. }
            if d.fp.len() == 64
                && d.fp.bytes().all(|c| c.is_ascii_hexdigit())
                && d.port != 0
                && d.name.len() <= 255
                && d.os.len() <= 32 =>
        {
            Some(d)
        }
        _ => None,
    }
}

/// This device's IPv4 addresses that discovery uses (for links and QR codes).
pub fn local_ips() -> Vec<Ipv4Addr> {
    interfaces().into_iter().map(|i| i.ip).collect()
}

struct Iface {
    ip: Ipv4Addr,
    prefix: u8,
    /// 0 Ethernet, 1 Wi-Fi, 2 other.
    rank: u8,
}

/// Local IPv4 interfaces worth scanning: not loopback, never cellular.
fn interfaces() -> Vec<Iface> {
    let cellular = ["rmnet", "ccmni", "wwan", "pdp", "clat", "v4-rmnet", "ppp"];
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !cellular.iter().any(|c| i.name.starts_with(c)))
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(a) if !a.ip.is_loopback() => Some(Iface {
                ip: a.ip,
                prefix: a.prefixlen,
                rank: rank(&i.name),
            }),
            _ => None,
        })
        .collect()
}

// ponytail: Ethernet > Wi-Fi > rest; ranking Wi-Fi bands (6 > 5 > 2.4 GHz) needs nl80211 link
// info (Phase 6 diagnostics).
fn rank(name: &str) -> u8 {
    let sys = Path::new("/sys/class/net").join(name);
    if sys.join("wireless").exists() || name.starts_with("wl") {
        1
    } else if sys.join("device").exists() || name.starts_with("eth") || name.starts_with("en") {
        0
    } else {
        2
    }
}

fn mask(prefix: u8) -> u32 {
    u32::MAX
        .checked_shl(32 - prefix.min(32) as u32)
        .unwrap_or(0)
}

fn broadcast(ip: Ipv4Addr, prefix: u8) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(ip) | !mask(prefix))
}

/// Hosts of the interface's subnet, or of the /24 around it when the subnet is bigger.
fn sweep(ip: Ipv4Addr, prefix: u8) -> impl Iterator<Item = Ipv4Addr> {
    // ponytail: /24 cap keeps a sweep at ≤254 packets; bigger subnets rely on multicast/broadcast
    // or a typed address.
    let p = prefix.max(24);
    let me = u32::from(ip);
    let net = me & mask(p);
    let hosts = if p >= 31 {
        0..0
    } else {
        net + 1..net | !mask(p)
    };
    hosts.filter(move |&h| h != me).map(Ipv4Addr::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev() -> Device {
        Device {
            fp: "ab".repeat(32),
            name: "laptop".into(),
            os: "linux".into(),
            port: 4747,
        }
    }

    #[test]
    fn subnet_math() {
        let ip = Ipv4Addr::new(192, 168, 1, 23);
        assert_eq!(broadcast(ip, 24), Ipv4Addr::new(192, 168, 1, 255));
        let hosts: Vec<_> = sweep(ip, 24).collect();
        assert_eq!(hosts.len(), 253);
        assert_eq!(hosts[0], Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(*hosts.last().unwrap(), Ipv4Addr::new(192, 168, 1, 254));
        assert!(!hosts.contains(&ip));
        // Bigger subnet: only the /24 around us. Smaller: just the subnet.
        assert_eq!(
            sweep(Ipv4Addr::new(10, 0, 7, 9), 16).next(),
            Some(Ipv4Addr::new(10, 0, 7, 1))
        );
        assert_eq!(sweep(Ipv4Addr::new(10, 42, 0, 1), 28).count(), 13);
        assert_eq!(sweep(ip, 32).count(), 0);
    }

    #[test]
    fn parses_only_valid_announces() {
        let ok = serde_json::to_vec(&Packet::Announce {
            v: 1,
            device: dev(),
        })
        .unwrap();
        assert_eq!(parse(&ok), Some(dev()));
        let bad_fp = Device {
            fp: "zz".into(),
            ..dev()
        };
        let bad = serde_json::to_vec(&Packet::Announce {
            v: 1,
            device: bad_fp,
        })
        .unwrap();
        assert_eq!(parse(&bad), None);
        assert_eq!(parse(br#"{"type":"query","v":1}"#), None);
        assert_eq!(parse(b"junk"), None);
    }

    #[test]
    fn finds_a_responder() {
        let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = sock.local_addr().unwrap().port();
        thread::spawn(move || respond(sock, &dev()));
        let start = Instant::now();
        let found = scan(port, Duration::from_secs(5), &[Ipv4Addr::LOCALHOST], |f| {
            f.device == dev()
        })
        .unwrap();
        assert!(start.elapsed() < Duration::from_secs(2), "stops early");
        assert_eq!(found[0].device, dev());
        assert!(found[0].addrs.iter().all(|a| a.port() == 4747));
    }
}
