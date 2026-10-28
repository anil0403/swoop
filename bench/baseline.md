# Baselines

Ceilings the engine is measured against. Engine target: ≥95% of the matching iperf3 number.

## Rig A — Linux laptop
Intel Core 7 150U (AES-NI, VAES, AVX2) · 16 GB · Intel AX211 Wi-Fi 6E · WD SN5000S NVMe · kernel 7.2 · no Ethernet port

### Storage (dd, 4 MiB blocks, direct I/O, 4 GiB)
| Write | Read |
|---|---|
| 1.5 GB/s | 5.2 GB/s |

### Wi-Fi link (home router, 5 GHz ch36, 80 MHz)
| Signal | RX rate | TX rate |
|---|---|---|
| −59 to −62 dBm | 865–961 Mbps | 367–649 Mbps |

TX rate varies a lot between samples — measure throughput, not link rate.

### Hotspot capability (`iw list`, regdomain IN)
AP + P2P-GO supported. AP allowed on 5 GHz ch149–161 (80 MHz); ch36–48 only while also connected on the same channel; DFS and 6 GHz not allowed.

## Rig B — Android phone
OnePlus Nord CE3 5G · Snapdragon 782G · Wi-Fi 6 dual-band

_TODO: storage write/read (Termux)._

## iperf3 (`-P 4`, 20 s, MB/s)
| Path | Phone → laptop | Laptop → phone |
|---|---|---|
| Router, both Wi-Fi | TODO | TODO |
| Phone hotspot 5 GHz | TODO | TODO |
| Laptop hotspot 5 GHz | TODO | TODO |
