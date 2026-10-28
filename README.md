# ultra-fast-data-transfer

Offline, zero-config file transfer between Linux and Android at the full speed of your local link (Wi-Fi, Ethernet, hotspot or Wi-Fi Direct). No internet, no accounts.

Status: planning. See [PLAN.md](PLAN.md).

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
