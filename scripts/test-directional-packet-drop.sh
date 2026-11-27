#!/usr/bin/env bash
# Real packet-drop proof: two Linux network namespaces joined by a veth pair.
# Requires root, iproute2 (ip + tc), OpenSSL, and a built target/debug/ares-peer.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/debug/ares-peer"
WORK="$(mktemp -d)"
A_LOG="$WORK/a.log"
B_LOG="$WORK/b.log"
A_PID=""
B_PID=""
cleanup() {
  [[ -n "$A_PID" ]] && kill "$A_PID" 2>/dev/null || true
  [[ -n "$B_PID" ]] && kill "$B_PID" 2>/dev/null || true
  ip netns del ares-a 2>/dev/null || true
  ip netns del ares-b 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

if [[ "$(id -u)" -ne 0 ]]; then echo "packet-drop test requires root" >&2; exit 2; fi
for cmd in ip tc openssl; do command -v "$cmd" >/dev/null || { echo "missing command: $cmd" >&2; exit 2; }; done
[[ -x "$BIN" ]] || { echo "build first: cargo build --bin ares-peer" >&2; exit 2; }

ip netns add ares-a
ip netns add ares-b
ip link add ares-va type veth peer name ares-vb
ip link set ares-va netns ares-a
ip link set ares-vb netns ares-b
ip -n ares-a addr add 10.241.0.1/24 dev ares-va
ip -n ares-b addr add 10.241.0.2/24 dev ares-vb
ip -n ares-a link set lo up
ip -n ares-b link set lo up
ip -n ares-a link set ares-va up
ip -n ares-b link set ares-vb up

openssl req -x509 -newkey ed25519 -nodes -keyout "$WORK/ca.key" -out "$WORK/ca.pem" -subj "/CN=ARES test CA" -days 1 >/dev/null 2>&1
openssl x509 -in "$WORK/ca.pem" -outform DER -out "$WORK/ca.der"
for peer in a b; do
  openssl req -new -newkey ed25519 -nodes -keyout "$WORK/$peer.key" -out "$WORK/$peer.csr" -subj "/CN=$peer" >/dev/null 2>&1
  cat > "$WORK/$peer.ext" <<EOF
subjectAltName=DNS:$peer
extendedKeyUsage=serverAuth,clientAuth
keyUsage=digitalSignature
basicConstraints=CA:FALSE
EOF
  openssl x509 -req -in "$WORK/$peer.csr" -CA "$WORK/ca.pem" -CAkey "$WORK/ca.key" -CAcreateserial -out "$WORK/$peer.pem" -days 1 -extfile "$WORK/$peer.ext" >/dev/null 2>&1
  openssl x509 -in "$WORK/$peer.pem" -outform DER -out "$WORK/$peer.der"
  openssl pkey -in "$WORK/$peer.key" -outform DER -out "$WORK/$peer-key.der"
done
A_PIN="$(openssl x509 -in "$WORK/a.pem" -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ':' | tr 'A-F' 'a-f')"
B_PIN="$(openssl x509 -in "$WORK/b.pem" -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ':' | tr 'A-F' 'a-f')"
SESSION="00112233445566778899aabbccddeeff"
PSK="$(openssl rand -hex 32)"

ip netns exec ares-a "$BIN" a 10.241.0.1:41001 10.241.0.2:41001 b "$WORK/ca.der" "$WORK/a.der" "$WORK/a-key.der" "$B_PIN" "$SESSION" "$PSK" >"$A_LOG" 2>&1 &
A_PID=$!
ip netns exec ares-b "$BIN" b 10.241.0.2:41001 10.241.0.1:41001 a "$WORK/ca.der" "$WORK/b.der" "$WORK/b-key.der" "$A_PIN" "$SESSION" "$PSK" >"$B_LOG" 2>&1 &
B_PID=$!

echo "Waiting for authenticated bidirectional baseline..."
for _ in $(seq 1 40); do
  if grep -q 'session=BidirectionalHealthy' "$A_LOG" && grep -q 'session=BidirectionalHealthy' "$B_LOG"; then break; fi
  sleep 1
done
if ! grep -q 'session=BidirectionalHealthy' "$A_LOG" || ! grep -q 'session=BidirectionalHealthy' "$B_LOG"; then
  cat "$A_LOG" "$B_LOG"
  echo "baseline did not become bidirectionally healthy" >&2
  exit 1
fi

echo "Applying 100% egress packet loss to A's veth (A -> B only)..."
ip netns exec ares-a tc qdisc add dev ares-va root netem loss 100%

for _ in $(seq 1 25); do
  if grep -q 'session=OneWay a_to_b=Failed b_to_a=Healthy' "$A_LOG"; then
    echo "VALIDATED: A detected A->B failure while B->A remained receiver-confirmed healthy."
    grep 'session=OneWay a_to_b=Failed b_to_a=Healthy' "$A_LOG" | tail -n 1
    exit 0
  fi
  sleep 1
done

echo "Packet-drop classification was not observed. Logs follow:" >&2
cat "$A_LOG" "$B_LOG" >&2
exit 1
