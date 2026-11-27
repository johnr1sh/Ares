#!/usr/bin/env bash
# Create two mutually authenticated peer bundles for an offline LAN session.
set -euo pipefail
umask 077

OUT="${1:-ares-lan-provisioning}"
mkdir -p "$OUT/a" "$OUT/b"
chmod 700 "$OUT" "$OUT/a" "$OUT/b"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

openssl req -x509 -newkey ed25519 -nodes \
  -keyout "$WORK/ca.key" -out "$WORK/ca.pem" \
  -subj "/CN=ARES LAN test CA" -days 365 \
  -addext "basicConstraints=critical,CA:TRUE" \
  -addext "keyUsage=critical,keyCertSign,cRLSign" >/dev/null 2>&1
openssl x509 -in "$WORK/ca.pem" -outform DER -out "$WORK/ca.der"

for peer in a b; do
  openssl req -new -newkey ed25519 -nodes \
    -keyout "$WORK/$peer.key" -out "$WORK/$peer.csr" \
    -subj "/CN=$peer" >/dev/null 2>&1
  cat > "$WORK/$peer.ext" <<EOF
subjectAltName=DNS:$peer
extendedKeyUsage=serverAuth,clientAuth
keyUsage=digitalSignature
basicConstraints=critical,CA:FALSE
EOF
  openssl x509 -req -in "$WORK/$peer.csr" \
    -CA "$WORK/ca.pem" -CAkey "$WORK/ca.key" -CAcreateserial \
    -out "$WORK/$peer.pem" -days 365 -extfile "$WORK/$peer.ext" >/dev/null 2>&1
  openssl x509 -in "$WORK/$peer.pem" -outform DER -out "$WORK/$peer.der"
  openssl pkey -in "$WORK/$peer.key" -outform DER -out "$WORK/$peer-key.der"
done

A_PIN="$(openssl x509 -in "$WORK/a.pem" -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ':' | tr 'A-F' 'a-f')"
B_PIN="$(openssl x509 -in "$WORK/b.pem" -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ':' | tr 'A-F' 'a-f')"
SESSION_ID="$(openssl rand -hex 16)"
openssl rand -out "$WORK/session.key" 32

for peer in a b; do
  cp "$WORK/ca.der" "$OUT/$peer/ca.der"
  cp "$WORK/$peer.der" "$OUT/$peer/$peer.der"
  cp "$WORK/$peer-key.der" "$OUT/$peer/$peer-key.der"
  cp "$WORK/session.key" "$OUT/$peer/session.key"
  printf '%s\n' "$SESSION_ID" > "$OUT/$peer/session.id"
done
printf '%s\n' "$B_PIN" > "$OUT/a/peer.pin"
printf '%s\n' "$A_PIN" > "$OUT/b/peer.pin"
chmod 600 "$OUT"/{a,b}/*

# The CA signing key is intentionally not included in either peer bundle.
rm -f "$WORK/ca.key"
echo "Created separate peer bundles under: $OUT"
echo "A peer pin (for B to verify): $A_PIN"
echo "B peer pin (for A to verify): $B_PIN"
echo "Session ID: $SESSION_ID"
echo
echo "Securely copy only $OUT/a to device A and $OUT/b to device B."
echo "Do not send either private key or session.key over an untrusted channel."
echo "Do not commit these generated files to source control."
