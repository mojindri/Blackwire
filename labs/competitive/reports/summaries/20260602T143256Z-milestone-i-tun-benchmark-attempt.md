# Milestone I - TUN Benchmark Attempt

Date: 2026-06-02 14:32:56 UTC

## Objective

Run the missing privileged TUN acceptance benchmark on the VPS pair:

- Server: `91.107.164.107`
- Client: `91.107.176.118`

Acceptance target:

- Blackwire TUN UDP p99 `<= sing-box TUN * 1.15`
- Blackwire TUN TCP throughput `>= sing-box TUN * 0.90`
- Blackwire TUN CPU `<= sing-box TUN * 1.15`

## Implemented Before Run

- Added `labs/competitive/scripts/direct_udp_bench.py` for direct UDP echo p99 measurement through TUN.
- Expanded nginx benchmark payloads with `64m` for TCP throughput measurement.
- Replaced the remote `tun` scaffold in `labs/competitive/scripts/run_matrix.sh` with a real privileged VPS path:
  - starts nginx and UDP echo on the server VPS,
  - writes Blackwire and sing-box TUN configs on the client VPS,
  - starts each TUN runtime,
  - runs UDP p99 and TCP throughput probes,
  - samples TUN process CPU,
  - emits JSONL rows.
- Added Linux packet-level TCP bridge support by enabling the existing TCP bridge on Linux.
- Changed Linux TUN route setup so TCP is handled by the packet-level bridge instead of pre-TUN iptables TCP REDIRECT.
- Added self-cleaning `timeout 120s` wrappers around future TUN runtime starts so a failed future attempt should recover route state automatically.

## Validation Before Remote Run

Commands passed:

```text
bash -n labs/competitive/scripts/run_matrix.sh
python3 -m py_compile labs/competitive/scripts/direct_udp_bench.py labs/competitive/scripts/udp_echo.py labs/competitive/scripts/socks5_udp_bench.py
cargo fmt --all
cargo check -q
cargo test -p blackwire-transport tun:: -- --nocapture
```

Native Linux candidate build:

```text
ssh -i id_hetzner root@91.107.164.107 \
  'cd /root/blackwire-milestone-i-build; cargo build --release -p blackwire --bin blackwire'
```

Built binary copied to:

```text
target/linux-amd64/blackwire-candidate-milestone-i
```

## Remote Run

Command:

```text
COMPETITIVE_MODE=remote \
BLACKWIRE_CANDIDATE_BIN=$PWD/target/linux-amd64/blackwire-candidate-milestone-i \
COMPETITIVE_SERVER_HOST=91.107.164.107 \
COMPETITIVE_CLIENT_HOST=91.107.176.118 \
COMPETITIVE_SSH_KEY=id_hetzner \
COMPETITIVE_DURATION=10 \
TUN_UDP_COUNT=500 \
TUN_TCP_PAYLOAD=64m \
bash labs/competitive/scripts/run_matrix.sh tun
```

Output JSONL:

```text
labs/competitive/reports/tun-remote-20260602T142743Z.jsonl
```

Rows emitted:

- `remote-inventory`: ok
- `blackwire-candidate-tun`: failed, `Blackwire TUN runtime did not start`
- `sing-box-tun`: failed, `sing-box TUN runtime did not start or config check failed`

sing-box config check log:

```text
labs/competitive/reports/singbox-tun-check-20260602T142743Z.log
```

## Current Status

Milestone I is not accepted.

The acceptance comparison was not produced because both TUN rows failed before UDP/TCP/CPU measurements were available.

After the failed run, the client VPS `91.107.176.118` became unreachable over IPv4 SSH and ICMP from both the local machine and the server VPS. This is consistent with a broken runtime TUN route/rule state on the client. The local environment does not have `hcloud` or a Hetzner API token configured, so provider-level reboot could not be issued from this workspace.

## Recovery Needed

Reboot client VPS `91.107.176.118` from Hetzner Cloud Console or API. Runtime route/ip rule state should clear on reboot.

After recovery, rerun the updated `competitive-tun` harness. The harness now wraps TUN runtimes in a `timeout 120s` self-cleanup path to reduce the chance of another persistent route lockout.
