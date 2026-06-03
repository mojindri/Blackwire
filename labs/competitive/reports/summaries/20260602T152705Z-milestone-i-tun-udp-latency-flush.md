# Milestone I TUN UDP latency-flush VPS validation

Date: 2026-06-02

## Scope

Validated the Milestone I TUN UDP p99 regression after adding latency-sized TUN write flushes. The run used native Linux binaries on the Hetzner VPS pair:

- Server: `<server-host>`
- Client: `<client-host>`
- Candidate binary: `target/linux-amd64/blackwire-candidate-milestone-i-latency-flush`

## Implementation Change

Blackwire now flushes latency-sized packets from the TUN write batch immediately instead of waiting for the configured batch delay. The default threshold is `256` bytes and is configurable as `tun.batch.latencyFlushBytes` / `tun.batch.latency_flush_bytes`; `0` disables the latency fast path.

This preserves the existing batch controls for larger packets while removing the dominant p99 delay for 64-byte UDP/DNS/game-style packets.

## Benchmark Evidence

Command shape:

```bash
COMPETITIVE_MODE=remote \
BLACKWIRE_CANDIDATE_BIN=$PWD/target/linux-amd64/blackwire-candidate-milestone-i-latency-flush \
COMPETITIVE_SERVER_HOST=<server-host> \
COMPETITIVE_CLIENT_HOST=<client-host> \
COMPETITIVE_SSH_KEY=id_hetzner \
COMPETITIVE_DURATION=10 \
TUN_UDP_COUNT=500 \
TUN_TCP_PAYLOAD=64m \
bash labs/competitive/scripts/run_matrix.sh tun
```

Raw local reports:

- `labs/competitive/reports/tun-remote-20260602T152513Z.jsonl`
- `labs/competitive/reports/tun-remote-20260602T152705Z.jsonl`

UDP rows:

| Run | Variant | Status | UDP p99 |
| --- | --- | --- | --- |
| `20260602T152513Z` | Blackwire candidate TUN | ok | `0.235 ms` |
| `20260602T152513Z` | sing-box TUN | ok | `0.203 ms` |
| `20260602T152705Z` | Blackwire candidate TUN | ok | `0.218 ms` |
| `20260602T152705Z` | sing-box TUN | ok | `0.221 ms` |

The rerun is within the strict `<= 1.15x` p99 gate: `0.218 / 0.221 = 0.99x`. This is a large improvement from the previous VPS run where Blackwire TUN UDP p99 was `1.441 ms` versus sing-box `0.352 ms`.

## Acceptance Status

Milestone I UDP/DNS/game-style TUN latency: accepted for the native VPS candidate run.

Milestone I TCP/bulk TUN evidence: not accepted from these rows. Both Blackwire and sing-box TCP rows failed during the same harness runs, so those rows are invalid for competitive acceptance. The likely harness issue is that the TCP payload target is the same server VPS IP used by the TUN runtime egress path, while the harness safety route currently protects only the SSH control peer.

## Cleanup Check

After the rerun, the client VPS had only normal routing rules and no leftover Blackwire or sing-box TUN runtime process was found.

