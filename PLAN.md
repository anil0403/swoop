# Ultra-fast offline transfer — Plan

App name: **TBD** (needed for mDNS service type, package id, BLE UUID).

## Requirements
- Full GUI apps: **Linux** and **Android**.
- Pairings: Linux↔Linux, Linux↔Android, Android↔Android, Android↔Linux.
- One-way per transfer (sender → receiver). Every device can be either.
- **Completely offline**: router Wi-Fi, Ethernet, phone/laptop hotspot, Wi-Fi Direct, no network at all (BLE bootstrap).
- **General devices**: detect capabilities at runtime, never hardcode per-device values.
- **Zero-config**: install → open → tap device → send. Settings are overrides only.
- **Peak throughput**: ≥95% of `iperf3 -P 4` between the same two devices, sustained.

## Stack (decided)
- **Core: Rust** — discovery, pairing/trust, transfer engine, tuning, capability probe.
- **UI: Flutter** (Linux desktop + Android) via `flutter_rust_bridge`. UI is never on the data path.
- **Android platform layer: Kotlin** — foreground service, Wi-Fi/multicast/wake locks, network binding, hotspot, Wi-Fi Direct, BLE advertise, Share intent, MediaStore/SAF.
- **Linux platform layer** — NetworkManager (D-Bus) hotspot, BlueZ BLE, XDG portals, tray, firewall helper (polkit).
- **Min Android: 8 (API 26)**. Rust targets: `aarch64`, `armv7` (32-bit phones, often no AES → ChaCha20), `x86_64`; Linux `x86_64`, `aarch64`.

### Android 8 → 16 version gates
| Feature | API 26–28 | API 29+ | Newer |
|---|---|---|---|
| Join hotspot | `WifiManager.addNetwork` (legacy) | `WifiNetworkSpecifier` (1-tap dialog) | — |
| Wi-Fi/BLE permissions | Location | Location | 31+: `BLUETOOTH_SCAN/ADVERTISE/CONNECT`; 33+: `NEARBY_WIFI_DEVICES` |
| Background transfer | Foreground service | Foreground service | 34+: FGS type `dataSync` or user-initiated data transfer job; 35+: dataSync time limit |
| Local-only hotspot | ✅ API 26+ | ✅ | 33+: more config options |
| BLE advertising | Only if `isMultipleAdvertisementSupported` | same | — |

## Architecture
```
Flutter UI  ── device list · send · accept · progress · diagnostics
   │ flutter_rust_bridge
Rust core   ── discovery · BLE bootstrap · pairing · engine · tuner · capability probe
   │ platform channels / D-Bus
Platform    ── Android (Kotlin) · Linux (NM, BlueZ, portals, polkit)
```
Every device is the same peer: announces itself, listens while receiving is enabled. Sender connects to receiver; if inbound is blocked, **reverse direction** (receiver dials sender).

## Discovery & connectivity (all automatic)
1. mDNS/DNS-SD · 2. UDP multicast + broadcast on every interface · 3. /24 subnet sweep on default port range · 4. QR (addr, port, fingerprint, optional hotspot creds).
- **No shared network → BLE bootstrap**: discover over BLE → exchange capabilities → best device hosts hotspot / Wi-Fi Direct on best allowed band (from regdomain + chip caps) → creds over BLE → peer joins → transfer → tear down.
- Path choice: Ethernet > 6 GHz > 5 GHz > 2.4 GHz; never cellular (bind sockets to the local network).
- Client isolation detected → switch to hotspot/Wi-Fi Direct path.

## Security
- Per-device key + self-signed cert, TLS 1.3, pinned fingerprints.
- First transfer from an unknown device: receiver confirms (kept on purpose). "Trust" → auto-accept later.
- Cipher by hardware: AES-GCM with AES instructions, ChaCha20-Poly1305 without. No plaintext mode.

## Transfer engine (performance)
- Pipeline (read ∥ hash ∥ encrypt ∥ send), never stop-and-wait. Buffer pool, ≤2 copies, 256 KB–4 MB I/O.
- BLAKE3 in the same pass; chunk-level verify → resume from last verified chunk.
- **Adaptive parallel streams** (start 1, add while +5%, cap 8), striped by chunk.
- Buffers sized from measured rate × RTT; Linux autotuning left on.
- Small files packed into one stream; parallel prefetch/create.
- Receiver: preallocate, sequential writes, one sync at the end.
- Thermal / battery-saver aware back-off (Android).
- **Learned profiles**: cache best settings per device pair + network.
- Later, only if profiling says CPU-bound (2.5/10 GbE, Wi-Fi 7): kTLS + sendfile, io_uring.

## UX
- Transfer screen: overall + per-file bar (receiver-acknowledged bytes), smoothed MB/s, peak, ETA, 60 s speed graph, pause/cancel; Android notification; Linux tray/taskbar progress.
- **Diagnostics line**: both devices' link (band, Wi-Fi gen, rate, signal), expected max for the pair, current bottleneck + one tip.
- Zero-config defaults: name from hostname/model, save to `Downloads/<App>`, rename on conflict, free-space + FAT32 4 GB checks.

## Phases
| # | Deliverable | Done when |
|---|---|---|
| 0 | Baselines on test rig | iperf3 + storage numbers recorded per path |
| 1 | Rust core engine + bench CLI | Loopback ≥1.5 GB/s (laptop); ≥95% of iperf3 on 2 hardware classes; resume works; 100k-small-file run passes |
| 2 | Discovery (4 layers), pairing, auto path choice, direction reversal | Fresh devices find each other on router + hotspot with 0 settings |
| 2b | BLE bootstrap (Linux + Android) | Devices with no shared network see each other |
| 3 | Flutter Linux app | Full flow Linux↔Linux |
| 4 | Flutter Android app + Kotlin layer | All 4 pairings ≥95% of baseline; API 26 and latest both pass |
| 5 | Auto hotspot / Wi-Fi Direct via BLE | Transfer with no router, ≤3 taps from fresh install |
| 6 | Hardening + diagnostics | 50 GB file + 10k photos clean; path failover; app-kill resume |
| 7 | Release | AUR, AppImage, Flatpak, signed APK (F-Droid later) |

## Phase 0 tasks
- [ ] 0.1 iperf3 laptop↔phone, both directions, `-P 1` and `-P 4`, 20 s: (a) router both Wi-Fi, (b) phone hotspot 5 GHz, (c) laptop hotspot 5 GHz.
- [ ] 0.2 Storage: laptop ✅ + phone sequential write/read (1–4 GB, direct I/O).
- [ ] 0.3 Laptop hotspot with home Wi-Fi off → does 5 GHz AP still work (regdomain fallback)?
- [x] 0.4 Record everything in `bench/baseline.md` (laptop side done; phone + iperf3 rows pending).

## Phase 1 tasks
- [x] 1.1 One crate `core/` (package `uft`): library + `uft` CLI binary.
- [x] 1.2 Protocol v1 (`core/src/proto.rs`): hello/capabilities with version, offer, accept/reject (resume), chunk frames with BLAKE3, progress, done, error.
- [x] 1.3 Identity: key/cert generation, fingerprint, pinned TLS 1.3 both ways (CLI takes the fingerprint until pairing exists).
- [x] 1.4 Pipelined sender/receiver with buffer pools, preallocation, chunk frames.
- [x] 1.5 Progress: receiver-acked bytes, smoothed speed, ETA.
- [x] 1.6 Resume from verified chunks (state file per partial file; finished files skipped).
- [x] 1.7 Multi-stream striping by chunk + adaptive stream controller.
- [x] 1.8 Small files: one shared chunk queue, no per-file round trips (2,000-file test; 100k run pending).
- [ ] 1.9 Capability probe: AES detection + cipher selection ✅; link info on Linux pending (Phase 6 diagnostics).
- [x] 1.10 Bench modes: `uft bench` (mem→loopback→null), `send --synthetic` + `recv --null` (mem→net→null), `send`/`recv` (disk→disk, full).
- [ ] 1.11 CI runs build, tests, clippy, fmt and the bench (informational) ✅; hard >5% gate needs stable self-hosted hardware.
- [ ] 1.12 Android aarch64 build ✅ (CI + local); run in Termux for phone engine numbers.

## Phase 2 tasks
- [x] 2.1 UDP discovery (`core/src/discover.rs`): a query from every interface (never cellular) to multicast 239.255.47.74, broadcast, and each host of the /24; receivers answer unicast. Tested between two network namespaces, also with multicast and broadcast dropped (sweep only).
- [ ] 2.2 mDNS/DNS-SD: skipped for now, the layers in 2.1 reach the same networks. Add if a real network shows a gap, or on Android (NsdManager) if the multicast lock gets in the way.
- [x] 2.3 Manual fallback: `uft://HOST:PORT/FINGERPRINT` link (what the apps' QR code will hold).
- [x] 2.4 Pairing: the receiver confirms an unknown sender; both sides show a 6-digit code; "always" saves it in `trusted`.
- [x] 2.5 Path choice: per answering interface, Ethernet > Wi-Fi > other. Wi-Fi band order needs link info (1.9, Phase 6).
- [ ] 2.6 Direction reversal (receiver dials the sender when inbound is blocked): deferred. Both sides must listen for it, and the main cause (a Linux firewall) goes away when the Linux app opens its port (Phase 3).
- [ ] 2.7 Real networks, 0 settings: laptop ↔ phone (Termux) on the router, the phone hotspot and the laptop hotspot.

## Phase 3 tasks
- [x] 3.1 Bridge (`app/rust/src/api/uft.rs`, flutter_rust_bridge): init, scan, receive (with offers), answer, send_files; progress as streams.
- [x] 3.2 Main screen (`app/lib/main.dart`): this device's name and save folder, nearby devices (rescanned every few seconds), tap → files or folder → send, transfer cards (bar, MB/s, peak, ETA), offer dialog with the code (Decline / Accept / Always accept).
- [x] 3.3 Zero-config: name from the hostname, save to `Downloads/uft`, config in the XDG data dir; receiving and visible while the app is open.
- [x] 3.4 End-to-end (`app/test-e2e.sh`): app ↔ `uft` CLI in a network namespace, both directions, files intact. The file picker itself is not driven.
- [x] 3.5 CI: analyze, test, Linux build.
- [ ] 3.6 Later: cancel/pause (needs core support), drag and drop, receiving on/off switch, tray icon, QR code for the `uft://` link, app icon.

## Phase 4 tasks
- [x] 4.1 Kotlin layer (`MainActivity.kt`, `TransferService.kt`): device name, save to `Download/uft` (storage permission up to Android 10), multicast lock, Wi-Fi high-perf/low-latency + wake locks while transferring, process bound to Wi-Fi/Ethernet (works on networks without internet while mobile data is on), foreground service (`dataSync`) so receiving continues in the background.
- [x] 4.2 Picking through the system picker (files, or a whole folder): each file is handed to Rust as `/proc/self/fd/N` and read from a duplicated descriptor (core `Source::Named`), no copying.
- [x] 4.3 Connect by address or `uft://` link (both apps): for networks where scanning can't reach.
- [x] 4.4 End-to-end on emulators (`app/test-android.sh`): app ↔ CLI both directions, API 26 and API 37. Found and fixed: Android shared storage refusing to set file times.
- [x] 4.5 CI builds the APK; releases ship a signed `uft-android.apk` once the owner adds the keystore secrets (7.3).
- [ ] 4.6 Real phones: all 4 pairings at ≥95% of iperf3 (needs the Phase 0 numbers), API 26 and latest; Wi-Fi without internet; hotspot (the phone hosting it while also on Wi-Fi is not handled yet).
- [ ] 4.7 Later: Share-to-uft intent, progress in the notification, media scan for Android 8–10 (Gallery), cancel, keep transferring after the app is swiped away, app icon and name.

## Phase 5 tasks: no shared network (direct link)
Flow: every app advertises over Bluetooth LE while open. A device seen only over BLE shows up in the list; tapping it
→ pick files → the **sender hosts** a hotspot (Android LocalOnlyHotspot, Linux NetworkManager) → writes an invite to the
receiver over BLE → the receiver confirms once (name + code) and joins → normal discovery finds it on the new network →
the transfer runs over TLS as usual (the invited offer is accepted without a second prompt) → both sides undo the
network change. Sender: tap device, pick. Receiver: one tap (Android 10+ adds the system "Connect" dialog).

BLE contract (both platforms):
- Service `b7e3c1a0-5d2f-4e8b-9c61-2a4f8d0e7b10`; advertising carries service data for it = the first 8 bytes of the
  device fingerprint (16 hex chars decoded), connectable; the device name in the scan response (may be truncated).
- Invite characteristic `b7e3c1a1-5d2f-4e8b-9c61-2a4f8d0e7b10`, write with response, value = UTF-8 JSON
  `{"v":1,"ssid":..,"pass":..,"sec":"wpa2"|"wpa3t"|"wpa3","fp":<sender fingerprint>,"name":<sender name>}`
  (≤ 240 bytes; the client asks for a large MTU, the server also accepts long/offset writes).
- The platform layer moves the JSON as an opaque string; the Dart app builds and parses it.
- ponytail: Wi-Fi credentials go over BLE unencrypted; they only open a short-lived hotspot, and the transfer itself is
  pinned TLS with the code shown on both sides. Encrypt the invite (fingerprint-derived key) if that ever matters.

- [ ] 5.1 Linux platform side (`app/rust/src/api/direct.rs`, BlueZ via `bluer`, NetworkManager via `nmcli`).
- [ ] 5.2 Android platform side (`MainActivity.kt` + `Direct.kt`): BLE advertise/scan/GATT, LocalOnlyHotspot, join
  (`WifiNetworkSpecifier` on 10+, `addNetwork` on 8–9), permissions.
- [ ] 5.3 App flow (`app/lib`): BLE-only devices in the list, host → invite → wait → send → tear down; invite dialog.
- [ ] 5.4 Tests: Linux↔Linux on virtual radios (mac80211_hwsim, virtual BLE controllers); Android↔Android on two emulators;
  real devices (all 4 pairings) with no router.
- [ ] 5.5 Later: Wi-Fi Direct (when neither side can host), the receiver hosting when the sender can't, 5/6 GHz band choice
  from link info, QR code instead of BLE for devices without Bluetooth.

## Release tasks (Phase 7, see VERSIONING.md)
- [x] 7.1 `release.yml`: on each `v*` tag, `uft` tarballs (Linux x86_64 + aarch64, Android aarch64) and `SHA256SUMS`; one-line install commands in README.
- [x] 7.2 Flutter Linux app inside the Linux tarballs (`lib/uft/`, `bin/uft-app`, `share/applications/uft.desktop`), so the same one-liner installs it.
- [ ] 7.3 Signed `uft-android.apk` on each release: built ✅; waits for the owner to add the keystore secrets (VERSIONING.md).
- [x] 7.4 `version.yml`: automatic versions and tags from commit types (`next-version.sh`, tested in CI).

## Test matrix (each release)
4 pairings × {router Wi-Fi, one side wired, Android hotspot, Linux hotspot, isolated guest network, no network (BLE)} × hardware classes {budget Android, mid-range Android, flagship Android, Linux Intel Wi-Fi, Linux Realtek/MediaTek, Raspberry Pi 4/5, wired 1/2.5/10 GbE}.
Record: discovery layer used, MB/s vs iperf3, taps from fresh install, resume, hash.

## Deferred
Windows/macOS/iOS, text/clipboard, folder sync, compression (adaptive zstd), QUIC.
