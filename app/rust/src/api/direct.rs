//! Direct link when there's no shared network: find devices and send invites over Bluetooth LE, then the sender
//! hosts a Wi-Fi network the receiver joins (PLAN.md, Phase 5). Linux: BlueZ and NetworkManager; elsewhere the app
//! does this in the platform code, and these are stubs.

use crate::frb_generated::StreamSink;

#[derive(Clone, Debug)]
pub enum BleEvent {
    /// A uft device advertising nearby. `id` is its Bluetooth address, `fp` the first 16 hex of its fingerprint.
    Seen { id: String, name: String, fp: String },
    /// Another device wrote an invite to us (JSON, see PLAN.md).
    Invite { json: String },
}

/// Advertises this device (`fp`: its fingerprint, the first 16 hex are sent), serves the invite characteristic and
/// scans for other uft devices, until the app exits or the stream is dropped. Call once. Fails when Bluetooth is
/// missing or off.
pub fn ble_start(fp: String, name: String, sink: StreamSink<BleEvent>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        rt().block_on(ble::start(fp, name, move |e| sink.add(e).is_ok()))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (fp, name, sink);
        Err(NA.into())
    }
}

/// Connects to device `id` (from `Seen`), writes `json` to its invite characteristic and disconnects (≤ 30 s).
pub fn ble_invite(id: String, json: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        rt().block_on(ble::invite(id, json))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (id, json);
        Err(NA.into())
    }
}

#[derive(Clone, Debug)]
pub struct Wifi {
    pub ssid: String,
    pub pass: String,
    /// "wpa2", "wpa3t" (WPA2/WPA3 transition) or "wpa3".
    pub sec: String,
}

/// Starts a Wi-Fi network on this device (5 GHz if it can, else 2.4 GHz). The device leaves its current network
/// until `stop_host`.
pub fn host_wifi() -> Result<Wifi, String> {
    #[cfg(target_os = "linux")]
    {
        nm::host()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(NA.into())
    }
}

/// Stops the network from `host_wifi`; the device goes back to its previous network.
pub fn stop_host() {
    #[cfg(target_os = "linux")]
    nm::delete(nm::HOST);
}

/// Joins `wifi`, waiting up to ~30 s for it to show up; returns once this device has an IPv4 address on it.
pub fn join_wifi(wifi: Wifi) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        nm::join(wifi)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = wifi;
        Err(NA.into())
    }
}

/// Leaves the network from `join_wifi`; the device goes back to its previous network.
pub fn leave_wifi() {
    #[cfg(target_os = "linux")]
    nm::delete(nm::JOIN);
}

#[cfg(not(target_os = "linux"))]
const NA: &str = "not on this platform";

#[cfg(target_os = "linux")]
fn rt() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

#[cfg(target_os = "linux")]
mod ble {
    use super::{rt, BleEvent};
    use bluer::adv::{Advertisement, Type};
    use bluer::gatt::local::{
        Application, Characteristic, CharacteristicWrite, CharacteristicWriteMethod, CharacteristicWriteRequest,
        ReqError, Service,
    };
    use bluer::gatt::remote;
    use bluer::{Adapter, AdapterEvent, Address, Device, DiscoveryFilter, DiscoveryTransport, Session, Uuid};
    use futures_util::StreamExt;
    use std::collections::{BTreeMap, HashMap};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    pub(super) const SERVICE: Uuid = Uuid::from_u128(0xb7e3c1a0_5d2f_4e8b_9c61_2a4f8d0e7b10);
    pub(super) const INVITE: Uuid = Uuid::from_u128(0xb7e3c1a1_5d2f_4e8b_9c61_2a4f8d0e7b10);
    /// The most a GATT attribute value can hold.
    const MAX_VALUE: usize = 512;

    fn bt(e: bluer::Error) -> String {
        format!("Bluetooth: {e}")
    }

    /// The adapter in `UFT_BT_ADAPTER` (tests) or BlueZ's default one, if it's on.
    pub(super) async fn adapter(s: &Session) -> Result<Adapter, String> {
        let gone = |e: bluer::Error| format!("no Bluetooth adapter ({e})");
        let a = match std::env::var("UFT_BT_ADAPTER") {
            Ok(n) => s.adapter(&n).map_err(gone)?,
            Err(_) => s.default_adapter().await.map_err(gone)?,
        };
        if !a.is_powered().await.map_err(gone)? {
            return Err("Bluetooth is off".into());
        }
        Ok(a)
    }

    pub(super) async fn start(
        fp: String,
        name: String,
        emit: impl Fn(BleEvent) -> bool + Send + Sync + 'static,
    ) -> Result<(), String> {
        let short = fp
            .get(..16)
            .and_then(|h| (0..8).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()).collect())
            .ok_or("fingerprint must start with 16 hex chars")?;
        let session = Session::new().await.map_err(|e| format!("Bluetooth is not available ({e})"))?;
        let adapter = adapter(&session).await?;
        let emit = Arc::new(emit);
        // Cut to fit the scan response ourselves: BlueZ would send it as a "shortened" name, which loses to the name
        // a device cached from an earlier connection (its Bluetooth name).
        let mut name = name;
        while name.len() > 29 {
            name.pop();
        }

        let adv = adapter
            .advertise(Advertisement {
                advertisement_type: Type::Peripheral,
                service_data: BTreeMap::from([(SERVICE, short)]),
                // BlueZ puts it in the scan response.
                local_name: Some(name),
                ..Default::default()
            })
            .await
            .map_err(bt)?;

        // Writes may come in pieces (long writes): append by offset until the value is whole JSON.
        let parts: Arc<Mutex<HashMap<Address, Vec<u8>>>> = Arc::default();
        let on_invite = emit.clone();
        let write = move |value: Vec<u8>, req: CharacteristicWriteRequest| {
            let mut parts = parts.lock().unwrap();
            let buf = parts.entry(req.device_address).or_default();
            if req.offset == 0 {
                buf.clear();
            }
            let res = if usize::from(req.offset) != buf.len() {
                Err(ReqError::InvalidOffset)
            } else if buf.len() + value.len() > MAX_VALUE {
                Err(ReqError::InvalidValueLength)
            } else {
                buf.extend(value);
                if serde_json::from_slice::<serde_json::Value>(buf).is_ok() {
                    let json = String::from_utf8(parts.remove(&req.device_address).unwrap()).unwrap();
                    on_invite(BleEvent::Invite { json });
                }
                Ok(())
            };
            Box::pin(async move { res }) as _
        };
        let app = adapter
            .serve_gatt_application(Application {
                services: vec![Service {
                    uuid: SERVICE,
                    primary: true,
                    characteristics: vec![Characteristic {
                        uuid: INVITE,
                        write: Some(CharacteristicWrite {
                            write: true,
                            method: CharacteristicWriteMethod::Fun(Box::new(write)),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            })
            .await
            .map_err(bt)?;

        adapter
            .set_discovery_filter(DiscoveryFilter {
                transport: DiscoveryTransport::Le,
                // Report every advertisement, not only changes: that's how the app knows a device is still there.
                duplicate_data: true,
                ..Default::default()
            })
            .await
            .map_err(bt)?;
        let found = adapter.discover_devices_with_changes().await.map_err(bt)?;
        rt().spawn(async move {
            let _keep = (session, adv, app);
            let mut found = Box::pin(found);
            while let Some(ev) = found.next().await {
                if let AdapterEvent::DeviceAdded(addr) = ev {
                    if let Some(seen) = seen(&adapter, addr).await {
                        if !emit(seen) {
                            break;
                        }
                    }
                }
            }
            // ponytail: ends when the adapter goes off or the stream is dropped; the app calls ble_start again.
        });
        Ok(())
    }

    /// A uft device. At the start BlueZ also lists the ones it saw in the last minute or so.
    // ponytail: one D-Bus call per advertisement of any device nearby; read ServiceData from the change signal
    // itself if that ever shows up in a crowded room.
    async fn seen(adapter: &Adapter, addr: Address) -> Option<BleEvent> {
        let d = adapter.device(addr).ok()?;
        let fp = d.service_data().await.ok()??.remove(&SERVICE).filter(|v| v.len() == 8)?;
        Some(BleEvent::Seen {
            id: addr.to_string(),
            name: d.name().await.ok().flatten().unwrap_or_default(),
            fp: fp.iter().map(|b| format!("{b:02x}")).collect(),
        })
    }

    pub(super) async fn invite(id: String, json: String) -> Result<(), String> {
        let addr: Address = id.parse().map_err(|_| format!("{id:?} is not a Bluetooth address"))?;
        let session = Session::new().await.map_err(|e| format!("Bluetooth is not available ({e})"))?;
        let dev = adapter(&session).await?.device(addr).map_err(bt)?;
        // With response (bluer's default is without), so a lost write is an error here.
        let req = remote::CharacteristicWriteRequest { op_type: bluer::gatt::WriteOp::Request, ..Default::default() };
        let send = async { invite_char(&dev).await?.write_ext(json.as_bytes(), &req).await.map_err(bt) };
        let res = tokio::time::timeout(Duration::from_secs(30), send)
            .await
            .unwrap_or_else(|_| Err("Bluetooth: the device didn't answer".into()));
        let _ = dev.disconnect().await;
        res
    }

    /// Connects to `dev` and finds its invite characteristic.
    pub(super) async fn invite_char(dev: &Device) -> Result<remote::Characteristic, String> {
        if !dev.is_connected().await.map_err(bt)? {
            dev.connect().await.map_err(bt)?;
        }
        for s in dev.services().await.map_err(bt)? {
            if s.uuid().await.map_err(bt)? == SERVICE {
                for c in s.characteristics().await.map_err(bt)? {
                    if c.uuid().await.map_err(bt)? == INVITE {
                        return Ok(c);
                    }
                }
            }
        }
        Err("that device has no uft invite service".into())
    }
}

/// NetworkManager through `nmcli`. The networks are NM connections with fixed names, so a crash leaves at most one
/// of each, which the next run replaces.
#[cfg(target_os = "linux")]
mod nm {
    use super::Wifi;
    use std::io::Read;
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant};

    pub(super) const HOST: &str = "uft-host";
    pub(super) const JOIN: &str = "uft-join";

    fn nmcli(args: &[&str]) -> Result<String, String> {
        let out = Command::new("nmcli")
            .args(args)
            .output()
            .map_err(|e| format!("can't run nmcli ({e}); is NetworkManager installed?"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    pub(super) fn delete(con: &str) {
        let _ = nmcli(&["connection", "delete", con]);
    }

    /// The Wi-Fi device in `UFT_WIFI_DEV` (tests) or the first one NM manages.
    fn device() -> Result<String, String> {
        if let Ok(d) = std::env::var("UFT_WIFI_DEV") {
            return Ok(d);
        }
        nmcli(&["-t", "-f", "DEVICE,TYPE,STATE", "device"])?
            .lines()
            .find_map(|l| match l.split(':').collect::<Vec<_>>()[..] {
                [d, "wifi", s] if s != "unmanaged" => Some(d.to_string()),
                _ => None,
            })
            .ok_or_else(|| "no Wi-Fi device".into())
    }

    fn random_hex(bytes: usize) -> String {
        let mut b = vec![0; bytes];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut b))
            .expect("/dev/urandom");
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    pub(super) fn host() -> Result<Wifi, String> {
        let dev = device()?;
        // NM hands out addresses on a shared network with dnsmasq.
        if !["/usr/bin/dnsmasq", "/usr/sbin/dnsmasq", "/sbin/dnsmasq"]
            .iter()
            .any(|p| std::path::Path::new(p).exists())
        {
            return Err("hosting a network needs dnsmasq; install it".into());
        }
        let wifi = Wifi {
            ssid: format!("uft-{}", random_hex(2)),
            pass: random_hex(8),
            sec: "wpa2".into(),
        };
        // Which channels allow starting a network depends on the device and the country. Ask `iw` for a 5 GHz
        // one; without `iw`, try what NM picks (a refused one takes NM ~25 s to give up on).
        let mut tries = match free_5ghz(&dev) {
            Some(free) => free.into_iter().take(1).map(|ch| ("a", ch)).collect(),
            None => vec![("a", String::new())],
        };
        tries.push(("bg", String::new()));
        delete(HOST);
        let start = || {
            nmcli(&[
                "connection", "add", "type", "wifi", "ifname", &dev, "con-name", HOST, "autoconnect", "no",
                "ssid", &wifi.ssid, "802-11-wireless.mode", "ap",
                "ipv4.method", "shared", "ipv6.method", "ignore",
                "wifi-sec.key-mgmt", "wpa-psk", "wifi-sec.psk", &wifi.pass,
                "wifi-sec.proto", "rsn", "wifi-sec.pairwise", "ccmp", "wifi-sec.group", "ccmp",
            ])?;
            let mut err = String::new();
            for (band, ch) in tries {
                let set = ["connection", "modify", HOST, "802-11-wireless.band", band, "802-11-wireless.channel", &ch];
                match nmcli(&set).and_then(|_| nmcli(&["--wait", "30", "connection", "up", HOST])) {
                    Ok(_) => return Ok(()),
                    Err(e) => err = e,
                }
            }
            Err(err)
        };
        match start() {
            Ok(()) => Ok(wifi),
            Err(e) => {
                delete(HOST);
                Err(format!("can't host a Wi-Fi network on {dev}: {e}"))
            }
        }
    }

    /// The 5 GHz channels `iw` says `dev` may start a network on (no radar ones: NM can't wait out their check);
    /// None when it can't tell.
    fn free_5ghz(dev: &str) -> Option<Vec<String>> {
        let phy = std::fs::read_to_string(format!("/sys/class/net/{dev}/phy80211/name")).ok()?;
        let out = Command::new("iw").args(["phy", phy.trim(), "info"]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let info = String::from_utf8_lossy(&out.stdout);
        Some(
            info.lines()
                .filter(|l| !["no IR", "disabled", "radar"].iter().any(|f| l.contains(f)))
                .filter_map(|l| {
                    let (mhz, rest) = l.trim().strip_prefix("* ")?.split_once(" MHz [")?;
                    let mhz: f32 = mhz.parse().ok()?;
                    (5000.0..5900.0).contains(&mhz).then(|| rest.split(']').next().unwrap().to_string())
                })
                .collect(),
        )
    }

    pub(super) fn join(wifi: Wifi) -> Result<(), String> {
        let dev = device()?;
        // A transition network takes WPA2 too, which works on more devices.
        let key = if wifi.sec == "wpa3" { "sae" } else { "wpa-psk" };
        delete(JOIN);
        nmcli(&[
            "connection", "add", "type", "wifi", "ifname", &dev, "con-name", JOIN, "autoconnect", "no",
            "ssid", &wifi.ssid, "wifi-sec.key-mgmt", key, "wifi-sec.psk", &wifi.pass,
            // Up means it has an IPv4 address; and it has no internet, so keep the default route.
            "ipv4.may-fail", "no", "ipv4.never-default", "yes",
        ])
        .map_err(|e| format!("can't join {}: {e}", wifi.ssid))?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let visible = nmcli(&["-t", "-f", "SSID", "device", "wifi", "list", "ifname", &dev, "--rescan", "no"])
                .is_ok_and(|o| o.lines().any(|l| l == wifi.ssid));
            let err = if visible {
                match nmcli(&["--wait", "20", "connection", "up", JOIN]) {
                    Ok(_) => return Ok(()),
                    Err(e) => e,
                }
            } else {
                "the network didn't show up".into()
            };
            if Instant::now() >= deadline {
                delete(JOIN);
                return Err(format!("can't join {}: {err}", wifi.ssid));
            }
            let _ = nmcli(&["device", "wifi", "rescan", "ifname", &dev, "ssid", &wifi.ssid]);
            thread::sleep(Duration::from_secs(2));
        }
    }
}

/// Virtual hardware only (mac80211_hwsim radios, btvirt controllers): run them through app/test-direct-linux.sh.
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::env::{set_var, var};
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::Arc;
    use std::time::Duration;

    fn link(path: String) -> String {
        std::fs::read_link(&path).map(|p| p.display().to_string()).unwrap_or_default()
    }

    #[test]
    #[ignore = "needs two virtual Wi-Fi radios"]
    fn wifi_host_join() {
        let (host, join) = (var("UFT_WIFI_DEV").unwrap(), var("UFT_WIFI_JOIN").unwrap());
        for d in [&host, &join] {
            let driver = link(format!("/sys/class/net/{d}/device/driver"));
            assert!(driver.ends_with("/mac80211_hwsim"), "{d} is not a mac80211_hwsim radio");
        }
        let t = std::time::Instant::now();
        let wifi = host_wifi().unwrap();
        println!("hosting {wifi:?} on {host} after {:?}", t.elapsed());
        set_var("UFT_WIFI_DEV", &join);
        let t = std::time::Instant::now();
        let res = join_wifi(wifi);
        println!("joined after {:?}", t.elapsed());
        let ip = std::process::Command::new("ip").args(["-4", "-o", "addr", "show", "dev", &join]).output();
        let ip = String::from_utf8_lossy(&ip.unwrap().stdout).into_owned();
        println!("{join}: {ip}");
        let stations = std::process::Command::new("iw").args(["dev", &host, "station", "dump"]).output();
        println!("{host} stations:\n{}", String::from_utf8_lossy(&stations.unwrap().stdout));
        leave_wifi();
        stop_host();
        res.unwrap();
        // From the host's DHCP server, so it went over the (virtual) air.
        assert!(ip.contains("inet 10.42.0."), "{ip}");
        let cons = std::process::Command::new("nmcli").args(["-t", "-f", "NAME", "connection"]).output();
        let cons = String::from_utf8_lossy(&cons.unwrap().stdout).into_owned();
        assert!(!cons.lines().any(|c| c.starts_with("uft-")), "{cons}");
    }

    fn next(rx: &Receiver<BleEvent>, want: impl Fn(&BleEvent) -> bool) -> BleEvent {
        loop {
            let e = rx.recv_timeout(Duration::from_secs(30)).expect("no BLE event in 30 s");
            if want(&e) {
                return e;
            }
        }
    }

    #[test]
    #[ignore = "needs two virtual Bluetooth controllers"]
    fn ble_find_invite() {
        let (a, b) = (var("UFT_BT_ADAPTER").unwrap(), var("UFT_BT_ADAPTER2").unwrap());
        for x in [&a, &b] {
            assert!(link(format!("/sys/class/bluetooth/{x}")).contains("/virtual/"), "{x} is not virtual");
        }
        let (fp_a, fp_b) = ("0123456789abcdef".repeat(4), "fedcba9876543210".repeat(4));
        let long_name = "a device name too long to fit in a scan response";
        // BlueZ's MIDI plugin reads the MIDI service PipeWire serves on each adapter, which needs pairing: confirm it
        // here (virtual adapters only) instead of the desktop asking.
        let ours = Arc::new([a.clone(), b.clone()]);
        let ours2 = ours.clone();
        let _agent = rt().block_on(async {
            let s = bluer::Session::new().await.unwrap();
            let agent = bluer::agent::Agent {
                request_default: true,
                request_confirmation: Some(Box::new(move |r| {
                    let ok = ours.contains(&r.adapter);
                    Box::pin(async move { ok.then_some(()).ok_or(bluer::agent::ReqError::Rejected) })
                })),
                request_authorization: Some(Box::new(move |r| {
                    let ok = ours2.contains(&r.adapter);
                    Box::pin(async move { ok.then_some(()).ok_or(bluer::agent::ReqError::Rejected) })
                })),
                ..Default::default()
            };
            let h = s.register_agent(agent).await.unwrap();
            (s, h)
        });
        let (ta, ra) = channel();
        let (tb, rb) = channel();
        rt().block_on(ble::start(fp_a.clone(), long_name.into(), move |e| ta.send(e).is_ok())).unwrap();
        set_var("UFT_BT_ADAPTER", &b);
        rt().block_on(ble::start(fp_b.clone(), "uft-b".into(), move |e| tb.send(e).is_ok())).unwrap();

        let BleEvent::Seen { id, name, .. } = next(&rb, |e| matches!(e, BleEvent::Seen { fp, .. } if fp_a.starts_with(fp.as_str())))
        else {
            unreachable!()
        };
        println!("{b} sees {id} {name:?}");
        assert!(!name.is_empty() && long_name.starts_with(&name), "{name:?}");
        next(&ra, |e| matches!(e, BleEvent::Seen { name, .. } if name == "uft-b"));

        let json = format!(
            r#"{{"v":1,"ssid":"uft-1a2b","pass":"0123456789abcdef","sec":"wpa2","fp":"{fp_b}","name":"{}"}}"#,
            "x".repeat(80)
        );
        let invited = |rx: &Receiver<BleEvent>| match next(rx, |e| matches!(e, BleEvent::Invite { .. })) {
            BleEvent::Invite { json } => json,
            _ => unreachable!(),
        };
        ble_invite(id.clone(), json.clone()).unwrap();
        assert_eq!(invited(&ra), json);

        // In two pieces, the second one at an offset: how a long write arrives.
        rt().block_on(async {
            let s = bluer::Session::new().await.unwrap();
            let dev = ble::adapter(&s).await.unwrap().device(id.parse().unwrap()).unwrap();
            let c = ble::invite_char(&dev).await.unwrap();
            let (x, y) = json.as_bytes().split_at(100);
            let mut at = bluer::gatt::remote::CharacteristicWriteRequest {
                op_type: bluer::gatt::WriteOp::Request,
                ..Default::default()
            };
            c.write_ext(x, &at).await.unwrap();
            at.offset = 100;
            c.write_ext(y, &at).await.unwrap();
            dev.disconnect().await.unwrap();
        });
        assert_eq!(invited(&ra), json);
    }
}
