# ultra-fast-data-transfer

Offline, zero-config file transfer between Linux and Android at the full speed of your local link (Wi-Fi, Ethernet, hotspot or Wi-Fi Direct). No internet, no accounts.

Status: the transfer engine, the `uft` command, and the Linux and Android apps work (Android tested on emulators,
API 26 and 37; real phones pending). See
[PLAN.md](PLAN.md).

## Install

One line, from the newest [release](https://github.com/anil0403/ultra-fast-data-transfer/releases) (no root).

Linux (x86_64 or aarch64, glibc 2.35+, GTK 3): the app (`uft-app`, also in the app menu as uft) and the `uft`
command, into `~/.local`. Run it again to update:

```sh
mkdir -p ~/.local && curl -fsSL https://github.com/anil0403/ultra-fast-data-transfer/releases/latest/download/uft-linux-$(uname -m).tar.gz | tar -xz -C ~/.local
```

Android app (8.0 and newer; in releases once the signing key is set up, see [VERSIONING.md](VERSIONING.md)): open
`https://github.com/anil0403/ultra-fast-data-transfer/releases/latest/download/uft-android.apk` on the phone, or
from a computer with `adb`:

```sh
curl -fsSLo /tmp/uft.apk https://github.com/anil0403/ultra-fast-data-transfer/releases/latest/download/uft-android.apk && adb install -r /tmp/uft.apk
```

The `uft` command in [Termux](https://termux.dev) (64-bit ARM phones):

```sh
curl -fsSL https://github.com/anil0403/ultra-fast-data-transfer/releases/latest/download/uft-android-aarch64.tar.gz | tar -xz -C $PREFIX
```

Check with `uft version`. A beta: use `releases/download/vX.Y.Z-beta.N` in place of `releases/latest/download`.
Each release has `SHA256SUMS`.

## Build

Linux (Rust stable, cmake, clang):

```sh
cd core
cargo build --release
./target/release/uft bench --size 4G         # in-process loopback benchmark
./target/release/uft recv --dir ~/Downloads  # receiver: nearby devices find it
./target/release/uft scan                    # on another device: list receivers
./target/release/uft send phone-name file1 dir2
```

`send` takes a device name or fingerprint prefix from `scan`, an address, or the `uft://` link that `recv`
prints (for networks where scanning can't reach). The receiver confirms a device it doesn't know yet: both sides
show the same pairing code; answer `a` to trust that device from then on.

The Linux app (Flutter, see `app/.flutter-version`, plus the Linux build tools: clang, cmake, ninja, GTK 3
headers):

```sh
cd app
flutter build linux --release  # -> build/linux/x64/release/bundle/uft-app
./test-e2e.sh                  # full flow, app ↔ CLI in a network namespace (needs sudo and a display)
```

Devices on networks where they can't find each other (guest Wi-Fi, different subnets): tap the link icon in the
app and type the other device's address, or one of the `uft://` links it shows there.

The Android app (also needs a JDK 17+ and the Android SDK; set `JAVA_HOME`):

```sh
cd app
flutter build apk --release   # -> build/app/outputs/flutter-apk/app-release.apk (arm64, armv7, x86_64)
emulator -avd NAME -no-window & ./test-android.sh   # full flow, app ↔ CLI on this computer (or a phone over adb)
```

The `uft` command for Android (NDK, API 26+):

```sh
rustup target add aarch64-linux-android
scripts/build-android.sh   # or: scripts/build-android.sh armv7-linux-androideabi x86_64-linux-android
# -> core/target/aarch64-linux-android/release/uft
```

The NDK comes from `$ANDROID_NDK_HOME`, `$ANDROID_NDK_LATEST_HOME`, or the newest `~/Android/Sdk/ndk/*`.

Run it in [Termux](https://termux.dev) (binaries can't execute from shared storage, so copy into Termux's `$HOME`):

```sh
adb push core/target/aarch64-linux-android/release/uft /data/local/tmp/
# in Termux:
cp /data/local/tmp/uft ~ && chmod +x ~/uft
~/uft bench --size 2G
termux-setup-storage && ~/uft recv --dir ~/storage/downloads
```
