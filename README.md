# ultra-fast-data-transfer

Offline, zero-config file transfer between Linux and Android at the full speed of your local link (Wi-Fi, Ethernet, hotspot or Wi-Fi Direct). No internet, no accounts.

Status: the transfer engine and the `uft` command work; the Linux and Android apps are not built yet. See
[PLAN.md](PLAN.md).

## Install

One line, from the newest [release](https://github.com/anil0403/ultra-fast-data-transfer/releases) (no root;
there is no release yet: the first one is tagged by hand, see [VERSIONING.md](VERSIONING.md)).

Linux (x86_64 or aarch64, glibc 2.35+), into `~/.local/bin`. Run it again to update:

```sh
mkdir -p ~/.local && curl -fsSL https://github.com/anil0403/ultra-fast-data-transfer/releases/latest/download/uft-linux-$(uname -m).tar.gz | tar -xz -C ~/.local
```

Android app, once it ships (Phase 4): open `https://github.com/anil0403/ultra-fast-data-transfer/releases/latest/download/uft-android.apk` on the phone, or from a computer with `adb`:

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
./target/release/uft recv --dir ~/Downloads  # receiver: prints address + fingerprint
./target/release/uft send HOST FINGERPRINT file1 dir2
```

Android (NDK, API 26+):

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
