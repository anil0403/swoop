<p align="center"><img src="assets/logo.svg" width="128" alt="Swoop logo"></p>

# Swoop

Offline, zero-config file transfer between Linux and Android at the full speed of your local link (Wi-Fi, Ethernet, hotspot or Wi-Fi Direct). No internet, no accounts.

Status: the transfer engine, the `swoop` command, and the Linux and Android apps work (Android tested on emulators,
API 26 and 37; real phones pending). See
[PLAN.md](PLAN.md).

## Install

One line, from the newest [release](https://github.com/anil0403/swoop/releases) (no root).

Linux (x86_64 or aarch64, glibc 2.35+, GTK 3): the app (`swoop-app`, also in the app menu as Swoop) and the `swoop`
command, into `~/.local`. Run it again to update:

```sh
mkdir -p ~/.local && curl -fsSL https://github.com/anil0403/swoop/releases/latest/download/swoop-linux-$(uname -m).tar.gz | tar -xz -C ~/.local
```

Or as an [AppImage](https://appimage.org) (`chmod +x swoop-$(uname -m).AppImage`, then run it), a Flatpak bundle
(x86_64: `flatpak install --user swoop-x86_64.flatpak`; no direct link without a shared network inside the sandbox),
or from the AUR on Arch (`swoop-bin`, e.g. `yay -S swoop-bin`), all on the
[releases](https://github.com/anil0403/swoop/releases/latest) page.

Android app (8.0 and newer): open `https://github.com/anil0403/swoop/releases/latest/download/swoop-android.apk`
on the phone, or from a computer with `adb`. For now each release's APK is signed with a different temporary key: to
update, uninstall the old one first (you pair devices again).

```sh
curl -fsSLo /tmp/swoop.apk https://github.com/anil0403/swoop/releases/latest/download/swoop-android.apk && adb install -r /tmp/swoop.apk
```

The `swoop` command in [Termux](https://termux.dev) (64-bit ARM phones):

```sh
curl -fsSL https://github.com/anil0403/swoop/releases/latest/download/swoop-android-aarch64.tar.gz | tar -xz -C $PREFIX
```

Check with `swoop version`. A beta: use `releases/download/vX.Y.Z-beta.N` in place of `releases/latest/download`.
Each release has `SHA256SUMS`.

## License

[MIT](LICENSE).

## Build

Linux (Rust stable, cmake, clang):

```sh
cd core
cargo build --release
./target/release/swoop bench --size 4G         # in-process loopback benchmark
./target/release/swoop recv --dir ~/Downloads  # receiver: nearby devices find it
./target/release/swoop scan                    # on another device: list receivers
./target/release/swoop send phone-name file1 dir2
```

`send` takes a device name or fingerprint prefix from `scan`, an address, or the `swoop://` link that `recv`
prints (for networks where scanning can't reach). The receiver confirms a device it doesn't know yet: both sides
show the same pairing code; answer `a` to trust that device from then on.

The Linux app (Flutter, see `app/.flutter-version`, plus the Linux build tools: clang, cmake, ninja, GTK 3
headers):

```sh
cd app
flutter build linux --release  # -> build/linux/x64/release/bundle/swoop-app
./test-e2e.sh                  # full flow, app ↔ CLI in a network namespace (needs sudo and a display)
```

Devices on networks where they can't find each other (guest Wi-Fi, different subnets): tap the link icon in the
app and type the other device's address, or one of the `swoop://` links it shows there.

The Android app (also needs a JDK 17+ and the Android SDK; set `JAVA_HOME`):

```sh
cd app
flutter build apk --release   # -> build/app/outputs/flutter-apk/app-release.apk (arm64, armv7, x86_64)
emulator -avd NAME -no-window & ./test-android.sh   # full flow, app ↔ CLI on this computer (or a phone over adb)
```

The `swoop` command for Android (NDK, API 26+):

```sh
rustup target add aarch64-linux-android
scripts/build-android.sh   # or: scripts/build-android.sh armv7-linux-androideabi x86_64-linux-android
# -> core/target/aarch64-linux-android/release/swoop
```

The NDK comes from `$ANDROID_NDK_HOME`, `$ANDROID_NDK_LATEST_HOME`, or the newest `~/Android/Sdk/ndk/*`.

Run it in [Termux](https://termux.dev) (binaries can't execute from shared storage, so copy into Termux's `$HOME`):

```sh
adb push core/target/aarch64-linux-android/release/swoop /data/local/tmp/
# in Termux:
cp /data/local/tmp/swoop ~ && chmod +x ~/swoop
~/swoop bench --size 2G
termux-setup-storage && ~/swoop recv --dir ~/storage/downloads
```
