# Phase 8 QUIC Endpoint Sharding Summary

Date: 2026-06-02

## Scope

Implemented QUIC UDP socket tuning for Blackwire native endpoints:

- Top-level `quic` config:
  - `reusePort`
  - `endpoints`
  - `recvBufferBytes`
  - `sendBufferBytes`
  - `maxDatagramSize`
- Linux/macOS UDP socket tuning via `socket2`:
  - `SO_REUSEADDR`
  - `SO_REUSEPORT` when requested
  - requested receive/send socket buffers
- Hysteria2 server endpoint sharding:
  - one endpoint by default
  - `endpoints: "cpu"` or numeric shard count when `reusePort` is enabled
  - fallback to fewer shards if an extra shard bind fails
- Hysteria2 client and UDP session tuned socket creation.
- Generic v2ray QUIC inbound tuned socket creation.
- QUIC socket tuning changes now require an instance restart on reload.
- Remote competitive Hysteria2 candidate configs now include Phase 8 `quic` tuning.

## Metrics

Added/described the Phase 8 Prometheus metric names:

- `blackwire_quic_endpoint_active_total`
- `blackwire_quic_endpoint_packets_total{endpoint,direction}`
- `blackwire_quic_endpoint_bytes_total{endpoint,direction}`
- `blackwire_quic_socket_drops_total`
- `blackwire_quic_recv_buffer_bytes`
- `blackwire_quic_send_buffer_bytes`

Packet/byte counters are currently emitted for the Hysteria2 UDP datagram path. Socket drop counter is initialized for endpoint visibility; platform drop polling can be added later against the same metric name.

## Validation Summary

Local macOS validation:

- Config parsing accepted the Phase 8 `quic` object and resolved `endpoints: "cpu"`.
- UDP socket test opened two sockets on the same loopback UDP address with `SO_REUSEPORT`.
- Core reload test confirmed `quic` socket tuning changes require instance restart.
- Hysteria2 UDP relay roundtrip still works with the new tuned endpoint path.
- Competitive harness syntax check accepted the updated Hysteria2 candidate config generation.

Native VPS validation:

- Server VPS: `91.107.164.107`
- SSH key: `id_hetzner`
- OS: Linux x86_64, kernel `7.0.0-15-generic`
- Rust toolchain: `rustc 1.93.1`
- Native Linux `SO_REUSEPORT` shard bind test passed.
- Native Linux Hysteria2 UDP relay roundtrip passed.

Remote note: the VPS toolchain did not have `cargo fmt`; formatting was verified locally with `cargo fmt`.

## Commands Run

Local:

```bash
cargo fmt
bash -n labs/competitive/scripts/run_matrix.sh
cargo test -p blackwire-core --test reload_listeners -- --nocapture
cargo test -p blackwire-config quic_socket_tuning_deserialises -- --nocapture
cargo test -p blackwire-transport tuned_udp_socket_allows_reuse_port_shards -- --nocapture
cargo test -p integration-tests --test e2e_hysteria2_udp -- --nocapture
```

Remote:

```bash
rsync -az --delete --exclude target --exclude .git --exclude perf.data -e 'ssh -i id_hetzner -o StrictHostKeyChecking=accept-new' ./ root@91.107.164.107:/root/v2ray-phase8/
ssh -i id_hetzner root@91.107.164.107 'cd /root/v2ray-phase8 && cargo test -p blackwire-transport tuned_udp_socket_allows_reuse_port_shards -- --nocapture && cargo test -p integration-tests --test e2e_hysteria2_udp -- --nocapture'
```

## Result

Phase 8 implementation is functionally complete for config parsing, tuned UDP socket construction, Linux `SO_REUSEPORT` endpoint sharding, Hysteria2 server/client wiring, generic QUIC inbound wiring, reload semantics, and required metric names.
