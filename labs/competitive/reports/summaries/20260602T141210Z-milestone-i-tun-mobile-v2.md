# Milestone I - TUN/mobile v2

Date: 2026-06-02 14:12:10 UTC

## Scope Implemented

- Added TUN writeback packet batching via `TunBatchConfig` and `TunPacketBatch`.
- Wired bounded batching into the native TUN runtime write path.
- Added configurable TUN batch knobs:
  - `tun.batch.enabled`
  - `tun.batch.maxPackets` / `tun.batch.max_packets`
  - `tun.batch.maxDelayUs` / `tun.batch.max_delay_us`
- Added configurable TUN session/NAT knobs:
  - `tun.sessions.udpMax` / `tun.sessions.udp_max`
  - `tun.sessions.udpIdleTimeoutSec` / `tun.sessions.udp_idle_timeout_sec`
  - `tun.sessions.tcpMax` / `tun.sessions.tcp_max`
- Wired the runtime to use a bounded UDP NAT/session table instead of hardcoded defaults.
- Added session-table max-flow eviction.
- Added explicit NAT/session `clear_for_network_change()` handling so mobile network handoff can drop stale bypass sockets and session state.
- Preserved existing socket-protect hooks:
  - Linux UDP NAT sockets still use `SO_MARK` through `protect_udp_socket_with_bypass_mark`.
  - macOS/Windows still require and validate `tun.outboundInterface` for protected egress.

## Validation Run

Commands run:

```text
cargo fmt --all
cargo test -p blackwire-config tun_platform_fields_accept_camel_and_snake_case -- --nocapture
cargo test -p blackwire-transport tun::batch -- --nocapture
cargo test -p blackwire-transport tun::session -- --nocapture
cargo test -p blackwire-transport tun::nat -- --nocapture
cargo check -q
```

Result:

- Config parsing for new TUN fields passed.
- TUN batch unit tests passed.
- TUN session-table unit tests passed.
- TUN NAT network-change clear unit test passed.
- Full workspace compile check passed.

## Acceptance Status

Milestone I is not fully accepted yet.

Implementation coverage is in for the requested TUN/mobile v2 building blocks, but the final acceptance gate requires a privileged native TUN benchmark against sing-box:

- TUN UDP p99 must be `<= sing-box * 1.15`.
- TUN TCP throughput must be `>= sing-box * 0.90`.
- CPU must be `<= sing-box * 1.15`.

The current `labs/competitive` `tun` scenario is still scaffolded and emits skipped rows instead of running a protocol-specific TUN benchmark, so it cannot prove the acceptance criteria.

## Next Required Work

Build and run a privileged VPS TUN benchmark harness that:

- Starts Blackwire native TUN mode on the client host.
- Starts sing-box TUN mode under equivalent routing and upstream conditions.
- Uses nginx/iperf/UDP probe targets outside the TUN capture loop.
- Captures UDP p99, TCP throughput, and CPU for both candidates.
- Writes raw JSONL plus a final acceptance summary.
