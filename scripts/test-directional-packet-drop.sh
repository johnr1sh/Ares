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
openssl rand -out "$WORK/session.key" 32

ip netns exec ares-a "$BIN" a 10.241.0.1:41001 10.241.0.2:41001 b "$WORK/ca.der" "$WORK/a.der" "$WORK/a-key.der" "$B_PIN" "$SESSION" "$WORK/session.key" >"$A_LOG" 2>&1 &
A_PID=$!
ip netns exec ares-b "$BIN" b 10.241.0.2:41001 10.241.0.1:41001 a "$WORK/ca.der" "$WORK/b.der" "$WORK/b-key.der" "$A_PIN" "$SESSION" "$WORK/session.key" >"$B_LOG" 2>&1 &
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

wait_for_state() {
  local logfile="$1"
  local expected="$2"
  local description="$3"
  local attempts="${4:-25}"
  for _ in $(seq 1 "$attempts"); do
    local latest
    latest="$(grep 'ARES_STATE' "$logfile" | tail -n 1 || true)"
    if [[ "$latest" == *"$expected"* ]]; then
      echo "PASS: $description"
      echo "$latest"
      return 0
    fi
    sleep 1
  done
  echo "FAIL: $description; last state was:" >&2
  grep 'ARES_STATE' "$logfile" | tail -n 3 >&2 || true
  return 1
}

wait_for_both_healthy() {
  wait_for_state "$A_LOG" 'session=BidirectionalHealthy a_to_b=Healthy b_to_a=Healthy a_to_b_fresh=true b_to_a_fresh=true' "$1 (A)" 30
  wait_for_state "$B_LOG" 'session=BidirectionalHealthy a_to_b=Healthy b_to_a=Healthy a_to_b_fresh=true b_to_a_fresh=true' "$1 (B)" 30
}

echo "Scenario 1: drop A -> B only..."
ip netns exec ares-a tc qdisc add dev ares-va root netem loss 100%
wait_for_state "$A_LOG" 'session=OneWay a_to_b=Failed b_to_a=Healthy a_to_b_fresh=true b_to_a_fresh=true' "A localizes A -> B failure"

echo "Restore A -> B and verify recovery..."
ip netns exec ares-a tc qdisc del dev ares-va root
wait_for_both_healthy "recovery after A -> B impairment"

echo "Scenario 2: drop B -> A only..."
ip netns exec ares-b tc qdisc add dev ares-vb root netem loss 100%
wait_for_state "$B_LOG" 'session=OneWay a_to_b=Healthy b_to_a=Failed a_to_b_fresh=true b_to_a_fresh=true' "B localizes B -> A failure"
wait_for_state "$A_LOG" 'session=Degraded a_to_b=Degraded b_to_a=Failed a_to_b_fresh=false b_to_a_fresh=true' "A downgrades stale reverse-path evidence"

echo "Restore B -> A and verify recovery..."
ip netns exec ares-b tc qdisc del dev ares-vb root
wait_for_both_healthy "recovery after B -> A impairment"

echo "Scenario 3: blackhole both directions..."
ip netns exec ares-a tc qdisc add dev ares-va root netem loss 100%
ip netns exec ares-b tc qdisc add dev ares-vb root netem loss 100%
wait_for_state "$A_LOG" 'session=Degraded a_to_b=Degraded b_to_a=Failed a_to_b_fresh=false b_to_a_fresh=true' "A reports degraded under bidirectional blackhole"
wait_for_state "$B_LOG" 'session=Degraded a_to_b=Failed b_to_a=Degraded a_to_b_fresh=true b_to_a_fresh=false' "B reports degraded under bidirectional blackhole"

echo "Restore both directions and verify recovery..."
ip netns exec ares-a tc qdisc del dev ares-va root
ip netns exec ares-b tc qdisc del dev ares-vb root
wait_for_both_healthy "recovery after bidirectional blackhole"

echo "VALIDATED: mirrored one-way loss, bidirectional blackhole ambiguity, and recovery."
