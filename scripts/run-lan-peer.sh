#!/usr/bin/env bash
# Run one provisioned ARES peer on a LAN without requiring internet access.
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: $0 <a|b> <local-ip:port> <peer-ip:port> <bundle-directory>" >&2
  exit 2
fi
ROLE="$1"
LOCAL_ADDR="$2"
PEER_ADDR="$3"
BUNDLE="$4"

case "$ROLE" in
  a) PEER_ID="b" ;;
  b) PEER_ID="a" ;;
  *) echo "role must be a or b" >&2; exit 2 ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ARES_PEER_BIN:-$ROOT/target/release/ares-peer}"
[[ -x "$BIN" ]] || { echo "missing executable: $BIN (build with cargo build --release --bin ares-peer)" >&2; exit 2; }
for file in ca.der "$ROLE.der" "$ROLE-key.der" peer.pin session.id session.key; do
  [[ -r "$BUNDLE/$file" ]] || { echo "missing bundle file: $BUNDLE/$file" >&2; exit 2; }
done
[[ "$(wc -c < "$BUNDLE/session.key" | tr -d ' ')" == "32" ]] || { echo "session.key must be exactly 32 bytes" >&2; exit 2; }

exec "$BIN" "$ROLE" "$LOCAL_ADDR" "$PEER_ADDR" "$PEER_ID" \
  "$BUNDLE/ca.der" "$BUNDLE/$ROLE.der" "$BUNDLE/$ROLE-key.der" \
  "$(cat "$BUNDLE/peer.pin")" "$(cat "$BUNDLE/session.id")" "$BUNDLE/session.key"
