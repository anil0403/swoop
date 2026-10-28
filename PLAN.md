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

## Test matrix (each release)
4 pairings × {router Wi-Fi, one side wired, Android hotspot, Linux hotspot, isolated guest network, no network (BLE)} × hardware classes {budget Android, mid-range Android, flagship Android, Linux Intel Wi-Fi, Linux Realtek/MediaTek, Raspberry Pi 4/5, wired 1/2.5/10 GbE}.
Record: discovery layer used, MB/s vs iperf3, taps from fresh install, resume, hash.

## Deferred
Windows/macOS/iOS, text/clipboard, folder sync, compression (adaptive zstd), QUIC.
