#!/usr/bin/env bash
# Cross-compile uft for Android (API 26+) with the NDK.
# usage: scripts/build-android.sh [aarch64-linux-android|armv7-linux-androideabi|x86_64-linux-android]...
set -euo pipefail

API=26
ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-}}"
if [ -z "$ndk" ]; then
    ndk="$(ls -d "$HOME"/Android/Sdk/ndk/*/ 2>/dev/null | sort -V | tail -n1)"
fi
ndk="${ndk%/}"
[ -d "$ndk" ] || { echo "NDK not found: set ANDROID_NDK_HOME" >&2; exit 1; }

bin="$(echo "$ndk"/toolchains/llvm/prebuilt/*/bin)"
[ -d "$bin" ] || { echo "no LLVM toolchain under $ndk" >&2; exit 1; }
export ANDROID_NDK_ROOT="$ndk" ANDROID_NDK_HOME="$ndk"

cd "$(dirname "$0")/../core"
[ $# -gt 0 ] || set -- aarch64-linux-android
for target in "$@"; do
    case "$target" in
        aarch64-linux-android | x86_64-linux-android) clang="$bin/$target$API-clang" ;;
        armv7-linux-androideabi) clang="$bin/armv7a-linux-androideabi$API-clang" ;;
        *) echo "unsupported target $target" >&2; exit 1 ;;
    esac
    t="${target//-/_}"
    T="${t^^}"
    export "CC_$t=$clang" "AR_$t=$bin/llvm-ar" "CARGO_TARGET_${T}_LINKER=$clang"
    cargo build --release --locked --target "$target"
    ls -l "target/$target/release/uft"
done
