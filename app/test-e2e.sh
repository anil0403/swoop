#!/bin/bash
# The app (this desktop) ↔ the uft CLI in a network namespace "peer" behind a veth.
# Needs sudo and a display. Usage: app/test-e2e.sh
set -eu
cd "$(dirname "$0")"
T=$(mktemp -d); (cd ../core && cargo build --release -q) && cp ../core/target/release/uft "$T/uft"; U=$T/uft
head -c 200M /dev/urandom > "$T/out.bin"; head -c 30M /dev/urandom > "$T/back.bin"
mkdir -p "$T/inbox" "$T/xdg/config" "$T/dl"
echo "XDG_DOWNLOAD_DIR=\"$T/dl\"" > "$T/xdg/config/user-dirs.dirs"
sudo -n bash -s "$T" "$U" "$(cat /proc/sys/kernel/hostname)" <<'ROOT'
T=$1 U=$2 ME=$3
ip netns del peer 2>/dev/null || true
ip netns add peer
ip link add uft-host type veth peer name uft-peer netns peer
ip addr add 10.88.0.1/24 dev uft-host; ip link set uft-host up
ip -n peer addr add 10.88.0.2/24 dev uft-peer; ip -n peer link set uft-peer up; ip -n peer link set lo up
inpeer() { ip netns exec peer unshare --uts sh -c "echo peer-ns > /proc/sys/kernel/hostname; UFT_HOME=$T/peerhome exec $*"; }
inpeer "$U recv --dir $T/inbox --yes" > "$T/recv.log" 2>&1 < /dev/null &
# Once the app's file has arrived, send one back to the app by its name.
( while [ ! -f "$T/inbox/out.bin" ]; do sleep 0.5; done; sleep 2
  inpeer "$U send $ME $T/back.bin" > "$T/send.log" 2>&1 ) > /dev/null 2>&1 &
ROOT
cleanup() { sudo -n sh -c "ip netns pids peer | xargs -r kill; ip netns del peer; ip link del uft-host; pkill -f 'bash -s $T'; rm -rf $T" 2>/dev/null || true; }
trap cleanup EXIT
export PATH=$HOME/development/flutter/bin:$HOME/.cargo/bin:$PATH
XDG_CONFIG_HOME=$T/xdg/config XDG_DATA_HOME=$T/xdg/data UFT_E2E_PEER=peer-ns UFT_E2E_FILE=$T/out.bin \
  flutter test integration_test -d linux
cmp "$T/out.bin" "$T/inbox/out.bin" && echo "app → CLI: intact"
cmp "$T/back.bin" "$T/dl/uft/back.bin" && echo "CLI → app: intact"
tr '\r' '\n' < "$T/send.log" | grep -v '%' || true
