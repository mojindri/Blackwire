# Performance And Contention

This repository has three performance layers:

1. Lab-style performance and soak (`labs/realistic/scripts/run-bench-*.sh`, `run-soak.sh`).
2. **End-to-end protocol benches** (`cargo bench -p blackwire-benches`, `make bench-protocol`).
3. Synthetic / component Criterion benches (framing, routing, TCP copy baseline).

## E2E protocol matrix

Run all five paths (VLESS TCP, VLESS WS, VMess gRPC, SS2022, Trojan TCP):

```bash
make bench-protocol-quick   # smaller payloads (CI-friendly)
make bench-protocol         # includes 16 MiB bulk cases
```

Single path:

```bash
cargo bench -p blackwire-benches --bench e2e_vless_tcp
```

Bench groups per path:

- `{path}/handshake` — proxy connect (SOCKS5 or HTTP CONNECT for VMess)
- `{path}/bulk_relay/steady_state/*` — long-lived connection, 64 KiB chunks
- `{path}/short_lived/*` — new connection per iteration
- `{path}/mixed_small_writes/*` — chatty 64× small write/read
- `{path}/concurrency/*` — parallel short-lived sessions

Environment:

| Variable | Effect |
|----------|--------|
| `BENCH_QUICK=1` | Skip 1 MiB / 16 MiB bulk sizes |
| `BENCH_SNIFF=1` | Extra handshake group with sniffing enabled |
| `BENCH_BULK_SWEEP=1` | Bulk chunk sweep (`4 KiB`, `16 KiB`, `64 KiB`) |
| `BENCH_BULK_CHUNKS=4096,16384,65536` | Explicit bulk chunk sizes in bytes |
| `BENCH_FEATURES=bench-alloc` | Count heap allocs (local perf only) |

Baseline notes and rankings: [`benches/perf-baseline.md`](../benches/perf-baseline.md).

## Flamegraphs

On Linux (recommended):

```bash
make bench-flamegraph PROTO=vmess_grpc SCENARIO=bulk
```

Artifacts: `benches/reports/flamegraphs/`. See `benches/scripts/flamegraph-protocol.sh`.

## Component benchmarks

```bash
cargo bench -p blackwire-benches
```

Legacy / synthetic groups:

- `tcp_relay_throughput` — memcpy baseline (not protocol)
- `protocol_handshake_latency` — synthetic CPU loop (not protocol)
- `routing_dns_fakeip`
- `websocket_grpc_framing_overhead`

## Regression Gates

CI baseline thresholds live in:

- `ci/perf-baselines/smoke.json`

Gate script:

- `ci/scripts/check_perf_regression.py`

## Lock Contention Profiling

Use:

```bash
bash tools/perf/check_lock_contention.sh
```

This documents workflow for:

- `tokio-console`
- `perf record/report`
- `cargo flamegraph`
