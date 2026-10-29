//! Link diagnostics: what each side's network link is, what a pair can reach, and one tip.
//!
//! ```text
//! link = JSON  {"kind":"wifi","mhz":5180,"mbps":1201,"dbm":-59}   (unknown fields left out)
//!              {"kind":"ethernet","mbps":1000}
//! ```

use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Link {
    /// "wifi" or "ethernet".
    pub kind: String,
    /// Wi-Fi channel frequency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mhz: Option<u32>,
    /// PHY rate (Wi-Fi) or line speed (Ethernet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps: Option<u32>,
    /// Wi-Fi signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dbm: Option<i32>,
}

/// This device's main local link (Linux): the Wi-Fi or Ethernet interface carrying the default
/// route, else the first one up with IPv4. Ethernet: /sys/class/net/IF/speed. Wi-Fi:
/// `iw dev IF link` (freq, signal, bitrate); None fields when unknown; None when there's no such
/// interface. Never on Android (the app passes Android's link in).
pub fn local() -> Option<Link> {
    if cfg!(target_os = "android") {
        return None;
    }
    let names: Vec<String> = default_route()
        .into_iter()
        .chain(
            if_addrs::get_if_addrs()
                .unwrap_or_default()
                .into_iter()
                .filter(|i| i.ip().is_ipv4() && !i.is_loopback())
                .map(|i| i.name),
        )
        .collect();
    names
        .iter()
        .find_map(|name| match crate::discover::rank(name) {
            0 => Some(Link {
                kind: "ethernet".into(),
                mhz: None,
                // -1 or unreadable while the link is down or the driver doesn't say.
                mbps: fs::read_to_string(format!("/sys/class/net/{name}/speed"))
                    .ok()
                    .and_then(|s| s.trim().parse().ok()),
                dbm: None,
            }),
            1 => Some(
                Command::new("iw")
                    .args(["dev", name, "link"])
                    .output()
                    .ok()
                    .map(|o| parse_iw(&String::from_utf8_lossy(&o.stdout)))
                    .unwrap_or_else(|| parse_iw("")),
            ),
            _ => None,
        })
}

/// Interface of the IPv4 default route, from /proc/net/route.
fn default_route() -> Option<String> {
    let t = fs::read_to_string("/proc/net/route").ok()?;
    t.lines().skip(1).find_map(|l| {
        let mut f = l.split_whitespace();
        let (name, dest) = (f.next()?, f.next()?);
        (dest == "00000000").then(|| name.to_string())
    })
}

/// A Wi-Fi link from `iw dev IF link` output ("Not connected." gives all None). The rate is the
/// higher of tx and rx: the ceiling, the tx rate alone jumps around between samples.
fn parse_iw(out: &str) -> Link {
    let mut l = Link {
        kind: "wifi".into(),
        mhz: None,
        mbps: None,
        dbm: None,
    };
    let num = |v: &str| v.split_whitespace().next()?.parse::<f64>().ok();
    for line in out.lines().map(str::trim) {
        let Some((k, v)) = line.split_once(": ") else {
            continue;
        };
        match k {
            "freq" => l.mhz = num(v).map(|f| f as u32),
            "signal" => l.dbm = num(v).map(|d| d as i32),
            "tx bitrate" | "rx bitrate" => {
                l.mbps = l.mbps.max(num(v).map(|r| r.round() as u32));
            }
            _ => {}
        }
    }
    l
}

// ponytail: rough ceiling model, TCP ≈ 0.6 × Wi-Fi PHY rate, 0.94 × Ethernet speed; two Wi-Fi
// sides are assumed to share one AP channel (airtime adds up). A direct hotspot link is one hop,
// so faster than this. Calibrate the factors against measured iperf3 numbers in bench/baseline.md.
const WIFI_EFF: f64 = 0.6;
const ETHERNET_EFF: f64 = 0.94;

/// One side's best TCP throughput in bytes/s.
fn rate(l: &Link) -> Option<f64> {
    let eff = if l.kind == "wifi" {
        WIFI_EFF
    } else {
        ETHERNET_EFF
    };
    Some(l.mbps? as f64 * 1e6 / 8.0 * eff)
}

/// Expected best-case throughput in bytes/s between two links, or None if a rate is unknown.
pub fn expected(a: &Link, b: &Link) -> Option<f64> {
    let (ra, rb) = (rate(a)?, rate(b)?);
    Some(if a.kind == "wifi" && b.kind == "wifi" {
        1.0 / (1.0 / ra + 1.0 / rb)
    } else {
        ra.min(rb)
    })
}

/// One short tip for the user given both links and the measured speed (bytes/s), or None if
/// nothing stands out.
pub fn tip(me: &Link, peer: &Link, speed: f64) -> Option<String> {
    let sides = [(me, "This device"), (peer, "The other device")];
    if let Some((_, who)) = sides.iter().find(|(l, _)| l.mhz.is_some_and(|m| m < 3000)) {
        return Some(format!(
            "{who} is on 2.4 GHz; 5 GHz is several times faster"
        ));
    }
    if let Some((_, who)) = sides.iter().find(|(l, _)| l.dbm.is_some_and(|d| d < -70)) {
        return Some(format!(
            "{who} has a weak Wi-Fi signal; move closer to the router"
        ));
    }
    if expected(me, peer).is_some_and(|e| speed < 0.5 * e) {
        return Some(
            "slower than this link allows: the other device's storage or CPU may be the limit"
                .into(),
        );
    }
    None
}

/// "Wi-Fi 5 GHz · 1201 Mbps · -59 dBm" / "Ethernet 1000 Mbps".
pub fn describe(l: &Link) -> String {
    let mut parts = Vec::new();
    if l.kind == "wifi" {
        parts.push(match l.mhz {
            Some(m) if m < 3000 => "Wi-Fi 2.4 GHz".to_string(),
            Some(m) if m < 5925 => "Wi-Fi 5 GHz".to_string(),
            Some(_) => "Wi-Fi 6 GHz".to_string(),
            None => "Wi-Fi".to_string(),
        });
        parts.extend(l.mbps.map(|r| format!("{r} Mbps")));
        parts.extend(l.dbm.map(|d| format!("{d} dBm")));
        parts.join(" · ")
    } else {
        match l.mbps {
            Some(r) => format!("Ethernet {r} Mbps"),
            None => "Ethernet".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wifi(mhz: u32, mbps: u32, dbm: i32) -> Link {
        Link {
            kind: "wifi".into(),
            mhz: Some(mhz),
            mbps: Some(mbps),
            dbm: Some(dbm),
        }
    }

    fn eth(mbps: Option<u32>) -> Link {
        Link {
            kind: "ethernet".into(),
            mhz: None,
            mbps,
            dbm: None,
        }
    }

    #[test]
    fn parses_iw_link() {
        let out = "Connected to d0:5f:af:d1:8c:5a (on wlo1)
\tSSID: Home_5G
\tfreq: 5180.0
\tRX: 160171232 bytes (170508 packets)
\tTX: 27323549 bytes (56004 packets)
\tsignal: -60 dBm
\trx bitrate: 720.6 MBit/s 80MHz HE-MCS 7 HE-NSS 2 HE-GI 0 HE-DCM 0
\ttx bitrate: 408.3 MBit/s 80MHz HE-MCS 4 HE-NSS 2 HE-GI 1 HE-DCM 0
\tbss flags: short-preamble short-slot-time
\tdtim period: 1
\tbeacon int: 100
";
        assert_eq!(parse_iw(out), wifi(5180, 721, -60));
        let none = parse_iw("Not connected.\n");
        assert_eq!((none.mhz, none.mbps, none.dbm), (None, None, None));
    }

    #[test]
    fn expected_rates() {
        let mb = |b: f64| b * 8.0 / 1e6;
        // Wi-Fi ↔ Wi-Fi share airtime: 600 and 600 usable → 300.
        let e = expected(&wifi(5180, 1000, -50), &wifi(5180, 1000, -50)).unwrap();
        assert!((mb(e) - 300.0).abs() < 1e-6);
        // Wired ↔ Wi-Fi: the slower side.
        let e = expected(&eth(Some(1000)), &wifi(5180, 1000, -50)).unwrap();
        assert!((mb(e) - 600.0).abs() < 1e-6);
        let e = expected(&eth(Some(1000)), &eth(Some(2500))).unwrap();
        assert!((mb(e) - 940.0).abs() < 1e-6);
        assert_eq!(expected(&eth(None), &eth(Some(1000))), None);
    }

    #[test]
    fn tips_first_match_wins() {
        let good = wifi(5180, 1200, -55);
        let fast = expected(&good, &good).unwrap();
        assert_eq!(tip(&good, &good, fast), None);
        assert_eq!(
            tip(&good, &wifi(2437, 144, -80), fast).unwrap(),
            "The other device is on 2.4 GHz; 5 GHz is several times faster"
        );
        assert_eq!(
            tip(&wifi(5180, 1200, -75), &good, fast).unwrap(),
            "This device has a weak Wi-Fi signal; move closer to the router"
        );
        assert!(tip(&good, &good, fast * 0.4).unwrap().starts_with("slower"));
        // Unknown rate: nothing to compare against.
        assert_eq!(tip(&eth(None), &good, 1.0), None);
    }

    #[test]
    fn describes() {
        assert_eq!(
            describe(&wifi(5180, 1201, -59)),
            "Wi-Fi 5 GHz · 1201 Mbps · -59 dBm"
        );
        assert_eq!(describe(&eth(Some(1000))), "Ethernet 1000 Mbps");
        assert_eq!(describe(&eth(None)), "Ethernet");
        assert_eq!(describe(&parse_iw("")), "Wi-Fi");
        assert_eq!(
            describe(&wifi(5955, 2402, -40)),
            "Wi-Fi 6 GHz · 2402 Mbps · -40 dBm"
        );
    }
}
