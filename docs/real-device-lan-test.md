# Real-device offline LAN validation

This is a manual acceptance test for **two physical Linux devices**. It is not
covered by the GitHub network-namespace test and must not be marked passed
until the two devices' logs and environment details are recorded.

## Requirements

- Two Linux devices with compatible ARES native binaries.
- Both devices connected to the same LAN, with stable private IPv4 addresses.
- TCP and UDP port 41001 allowed between the two devices.
- Wi-Fi client/AP isolation disabled; guest networks may isolate clients.
- A secure way to provision the peer bundle to each device.
- Internet access is needed only to obtain/build dependencies beforehand. The
  active ARES session uses direct LAN addresses and should not require WAN.

The current runtime does not perform discovery, NAT traversal, or relay. Use
direct reachable LAN IP addresses. The same numeric port is used for TCP
(mutual TLS identity bootstrap) and UDP (encrypted heartbeats and receipts).

## 1. Build before disconnecting WAN

On each device, install Rust and build from a trusted checkout while package
access is available:

    cargo build --release --bin ares-peer

Transfer the resulting `target/release/ares-peer` binary if building on only
one compatible machine. Do not assume Linux binaries are portable across
different CPU architectures or libc environments.

## 2. Create and provision unique credentials

On a trusted setup machine:

    bash scripts/provision-lan-pair.sh ares-lan-provisioning

This generates independent A and B private keys, a shared test CA certificate,
a unique session ID, and a 32-byte session PSK. It creates two directories:

- `ares-lan-provisioning/a/`: A identity and A's expected B certificate pin.
- `ares-lan-provisioning/b/`: B identity and B's expected A certificate pin.

Copy directory A only to device A and directory B only to device B using a
trusted channel. Keep `session.key` private and mode 0600. Never put generated
credentials in Git, CI artifacts, chat, or public storage. Generate a fresh
bundle for every new session; do not reuse a session ID/PSK after restart.

## 3. Start peers on the LAN

Find each device's LAN IP address locally. Example addresses below are
illustrative; replace them with the actual assigned addresses.

On device A:

    bash scripts/run-lan-peer.sh a 192.168.1.10:41001 192.168.1.11:41001 ./a

On device B:

    bash scripts/run-lan-peer.sh b 192.168.1.11:41001 192.168.1.10:41001 ./b

Use the correct bundle directory path on each device. Keep both processes in
the foreground so logs are captured. Expected steady-state evidence on both:

    session=BidirectionalHealthy a_to_b=Healthy b_to_a=Healthy

Observe for at least 5 minutes before applying the WAN outage.

## 4. Prove operation without internet

Keep both devices connected to the same local Wi-Fi/Ethernet network. Disable
the router's WAN uplink or otherwise disconnect internet access **without**
turning off the LAN/Wi-Fi network. Do not enable a phone hotspot as a substitute
for the same LAN test.

Record:
- Both device models, OS/kernel, CPU architecture, ARES commit and binary hash.
- LAN interface type, private IPs, subnet, AP/router model and client-isolation
  setting if known.
- TCP/UDP firewall rules and the exact method used to remove WAN.
- Timestamp of WAN removal/restoration and at least 5 minutes of per-peer
  `ARES_STATE` output while offline.
- Whether both peers stayed `BidirectionalHealthy`, any state transitions,
  and whether the processes restarted or changed interfaces.

Restore WAN only after the offline observation period. The expected result is
that the LAN session remains bidirectionally healthy during WAN loss. If the
same-LAN test fails, inspect local firewall policy, client isolation, VLAN
boundaries, IP changes, and whether the selected addresses are actually
reachable; do not infer that ARES needs cloud service from a failed topology.

## 5. What counts as validation

This test is **not passed** by the namespace/veth test, by two processes on one
machine, or by a successful ping alone. A real-device offline-LAN result
requires logs from both physical endpoints and the environment record above.
Until those artifacts are reviewed, status remains **NOT RUN**.

The current runtime is a heartbeat/reliability proof of concept, not an
application data, audio, or video transport. The test establishes only whether
its authenticated heartbeat and receiver-report path survives WAN loss on the
documented LAN.
