#!/bin/bash
# The Android app on an emulator or phone (adb) ↔ the uft CLI on this computer, both directions.
# Usage: app/test-android.sh [SERIAL]   (start one first, e.g. `emulator -avd uft-26`; see README)
set -eu
cd "$(dirname "$0")"
export ANDROID_SERIAL=${1:-${ANDROID_SERIAL:-$(adb devices | awk 'NR > 1 && $2 == "device" { print $1; exit }')}}
T=$(mktemp -d)
trap 'kill $(jobs -p) 2> /dev/null; adb forward --remove tcp:47481 2> /dev/null; rm -rf "$T"' EXIT
(cd ../core && cargo build --release -q)
U=$PWD/../core/target/release/uft
head -c 30M /dev/urandom > "$T/back.bin"
adb shell rm -f /sdcard/Download/uft/back.bin
UFT_HOME=$T/home "$U" recv --dir "$T/inbox" --yes > "$T/recv.log" 2>&1 &
# An emulator reaches this computer at 10.0.2.2, which it can't scan; a phone on the same Wi-Fi finds it.
addr=
[[ $(adb shell getprop ro.kernel.qemu | tr -d '\r') == 1 ]] && addr=10.0.2.2
(
  # The runtime permissions a user would grant in the dialogs.
  until adb shell pm list packages dev.uft.uft | grep -q uft; do sleep 1; done
  for p in WRITE_EXTERNAL_STORAGE POST_NOTIFICATIONS; do adb shell pm grant dev.uft.uft android.permission.$p; done
  # Once the app's file has arrived, send one back, to the fingerprint the test wrote, through adb forward.
  until [ -f "$T/inbox/out.bin" ]; do sleep 0.5; done
  fp=$(adb exec-out run-as dev.uft.uft cat files/e2e-fp)
  adb forward tcp:47481 tcp:47474
  UFT_HOME=$T/home "$U" send 127.0.0.1:47481 "$fp" "$T/back.bin" > "$T/send.log" 2>&1
) > /dev/null 2>&1 &
flutter test integration_test --dart-define=UFT_E2E_PEER="$(cat /proc/sys/kernel/hostname)" \
  ${addr:+--dart-define=UFT_E2E_ADDR=$addr}
[[ $(stat -c %s "$T/inbox/out.bin") == $((64 << 20)) ]] && echo "app → CLI: 64 MiB arrived (chunks BLAKE3-checked)"
[[ $(adb shell md5sum /sdcard/Download/uft/back.bin | cut -d' ' -f1) == $(md5sum < "$T/back.bin" | cut -d' ' -f1) ]] &&
  echo "CLI → app: intact in Download/uft"
tr '\r' '\n' < "$T/send.log" | grep -v '%' || true
