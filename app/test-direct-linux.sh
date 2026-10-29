#!/bin/bash
# The direct link (rust/src/api/direct.rs) on virtual hardware only: two mac80211_hwsim Wi-Fi radios (one hosts, one
# joins) and two btvirt Bluetooth LE controllers (one advertises and serves, one scans and invites). Real Wi-Fi and
# Bluetooth devices are never touched. Needs sudo, NetworkManager, dnsmasq, iw, and BlueZ's btvirt, which distros
# don't ship: build it from the bluez release matching `bluetoothctl --version`:
#   ./configure --enable-testing --disable-tools --disable-monitor --disable-client --disable-obex --disable-cups \
#     --disable-manpages --disable-udev --disable-systemd --disable-mesh --disable-midi && make emulator/btvirt
# Usage: BTVIRT=path/to/btvirt app/test-direct-linux.sh   (no BTVIRT: Wi-Fi only)
set -euo pipefail
cd "$(dirname "$0")/rust"
sudo -n true
lsmod | grep -q '^mac80211_hwsim' && { echo "mac80211_hwsim is already loaded; unload it first" >&2; exit 1; }
BT_STORE='/var/lib/bluetooth/00:AA:01:*' # btvirt's addresses
cleanup() {
  nmcli connection delete uft-host uft-join > /dev/null 2>&1 || true
  [ -n "${BTVIRT_PID:-}" ] && sudo -n kill "$BTVIRT_PID" 2> /dev/null
  sudo -n modprobe -r mac80211_hwsim 2> /dev/null || true
  sudo -n sh -c "sleep 1; rm -rf $BT_STORE"
}
trap cleanup EXIT
cargo test --lib --no-run -q 2> /dev/null

# Wi-Fi: the radios mac80211_hwsim adds, once NetworkManager manages them.
sudo -n modprobe mac80211_hwsim radios=2
hwsim() { for d in /sys/class/net/*; do [[ $(readlink "$d/device/driver") == */mac80211_hwsim ]] && basename "$d"; done; }
for _ in $(seq 20); do
  mapfile -t W < <(hwsim)
  [ ${#W[@]} = 2 ] && nmcli -t -f DEVICE,STATE device | grep -q "^${W[1]}:disconnected" && break
  sleep 0.5
done
echo "Wi-Fi: hosting on ${W[0]}, joining from ${W[1]}"
UFT_WIFI_DEV=${W[0]} UFT_WIFI_JOIN=${W[1]} cargo test --lib wifi_host_join -- --ignored --nocapture 2>&1 |
  grep -E 'hosting|joined|inet|test result|panicked' -A1

[ -z "${BTVIRT:-}" ] && { echo "no BTVIRT, skipping Bluetooth"; exit 0; }
# Bluetooth: the controllers btvirt adds (bluetoothd powers them on).
sudo -n sh -c "rm -rf $BT_STORE"
BTVIRT_PID=$(sudo -n sh -c "'$BTVIRT' -L -l2 > /dev/null 2>&1 & echo \$!")
vhci() { for h in /sys/class/bluetooth/hci*; do [[ $(readlink "$h") == */virtual/* ]] && basename "$h"; done; }
for _ in $(seq 20); do mapfile -t H < <(vhci); [ ${#H[@]} = 2 ] && break; sleep 0.5; done
sleep 2
echo "Bluetooth: ${H[0]} serves, ${H[1]} invites"
UFT_BT_ADAPTER=${H[0]} UFT_BT_ADAPTER2=${H[1]} cargo test --lib ble_find_invite -- --ignored --nocapture 2>&1 |
  grep -E 'sees|test result|panicked' -A1
