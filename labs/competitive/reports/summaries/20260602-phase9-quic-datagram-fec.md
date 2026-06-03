# Phase 9 QUIC DATAGRAM/FEC Summary

Date: 2026-06-02

## Scope

Implemented the first Milestone G / Phase 9 slice:

- Added top-level `datagram` config:
  - `enabled`
  - `udpOverDatagram`
  - `tunPacketsOverDatagram`
- Added top-level `fec` config:
  - `mode`
  - `maxOverheadPercent`
  - `protectClasses`
  - `avoidBulkTcp`
- Added reliable/unreliable/priority lane labels for datagram metrics.
- Wired Hysteria2 UDP to the explicit QUIC DATAGRAM lane policy.
- Added FEC wrapping around Hysteria2 UDP datagrams without changing the normal Hysteria2 UDP datagram wire format when FEC is off.
- Added XOR 1-of-N FEC recovery for one missing datagram per group.
- Added Reed-Solomon FEC recovery for one missing datagram per group using `reed-solomon-erasure`.
- Added adaptive `fec.mode = "auto"` policy thresholds:
  - loss `< 1%`: FEC off
  - loss `1-3%`: XOR for protected classes
  - loss `3-8%`: Reed-Solomon for protected classes
  - loss `> 8%`: stronger `raptor-like` policy label, currently using the same parity envelope pending a dedicated raptor implementation
  - bulk TCP remains unprotected when `avoidBulkTcp` is true
- Updated remote competitive Hysteria2 candidate configs to emit `datagram` and `fec` blocks.

## Metrics

Added/described:

- `blackwire_fec_mode_total{mode}`
- `blackwire_fec_recovered_packets_total`
- `blackwire_fec_overhead_bytes_total`
- `blackwire_fec_stale_drops_total`
- `blackwire_datagram_packets_total{class,direction}`
- `blackwire_datagram_fallback_total{reason}`

## Validation Summary

Local validation:

- Config parsing accepts `datagram` and `fec` blocks.
- Adaptive FEC threshold policy matches the plan classes and loss bands.
- XOR FEC recovers one missing Hysteria2 UDP datagram in codec tests.
- Reed-Solomon FEC recovers one missing Hysteria2 UDP datagram in codec tests.
- Reload restart detection treats datagram/FEC changes as structural.
- Hysteria2 UDP roundtrip still works with default policy.
- Hysteria2 UDP roundtrip works with XOR FEC enabled on the real QUIC DATAGRAM path.
- Competitive harness shell syntax accepts the updated candidate config generation.

Native VPS validation:

- Server VPS: `<server-host>`
- SSH key: `id_hetzner`
- Native Linux codec tests passed for XOR and Reed-Solomon FEC.
- Native Linux Hysteria2 UDP integration passed with default and XOR-FEC-enabled datagram paths.

## Commands Run

Local:

```bash
cargo fmt
bash -n labs/competitive/scripts/run_matrix.sh
cargo test -p blackwire-config datagram_and_fec_policy_deserialise -- --nocapture
cargo test -p blackwire-config fec_auto_policy_tracks_loss_and_packet_class -- --nocapture
cargo test -p blackwire-transport fec_recovers -- --nocapture
cargo test -p blackwire-core --test reload_listeners -- --nocapture
cargo test -p integration-tests --test e2e_hysteria2_udp -- --nocapture
cargo fmt --check
```

Remote:

```bash
rsync -az --delete --exclude target --exclude .git --exclude perf.data -e 'ssh -i id_hetzner -o StrictHostKeyChecking=accept-new' ./ root@<server-host>:/root/v2ray-phase9/
ssh -i id_hetzner root@<server-host> 'cd /root/v2ray-phase9 && cargo test -p blackwire-transport fec_recovers -- --nocapture && cargo test -p integration-tests --test e2e_hysteria2_udp -- --nocapture'
```

## Result

Phase 9 implementation is functionally complete for config, lane policy, Hysteria2 UDP QUIC DATAGRAM wiring, XOR FEC, Reed-Solomon FEC, adaptive policy thresholds, metrics, reload semantics, and native Linux validation.

The performance acceptance target (`>= 15%` p99 improvement for UDP/DNS/interactive traffic at 3-10% loss with overhead caps) still needs a dedicated lossy UDP/DNS benchmark row. The current competitive harness config is ready to carry DATAGRAM/FEC in the candidate path, but this summary does not claim that p99 performance gate yet.
