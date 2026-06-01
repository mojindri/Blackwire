# Milestone B Relay v2 Remote A/B - 2026-06-01T20:11:15Z

## Command

```sh
COMPETITIVE_MODE=remote \
COMPETITIVE_DURATION=3 \
COMPETITIVE_CONCURRENCY=8 \
COMPETITIVE_PAYLOADS=1k \
BLACKWIRE_CANDIDATE_BIN=$PWD/target/linux-amd64/blackwire-candidate-relay-v2 \
make competitive-clean competitive-report
```

## Environment

- Server VPS: `91.107.164.107`
- Client VPS: `91.107.176.118`
- Upstream: native nginx stream/static HTTP on server port `18080`
- Baseline binary: `target/linux-amd64/blackwire`
- Candidate binary: natively built on `91.107.164.107`, fetched to `target/linux-amd64/blackwire-candidate-relay-v2`
- Candidate policy: `fast.relay.engine=v2`, `fast.relay.flush=deferred`, `initialBuffer=16384`, `maxBuffer=262144`

## Result File

`labs/competitive/reports/clean-remote-20260601T200948Z.jsonl`

## Results

| Variant | Status | p50 ms | p95 ms | p99 ms | req/s | Errors |
|---|---|---:|---:|---:|---:|---:|
| direct-vps-native | ok | 0.20 | 0.40 | 1.20 | 36212.71 | 0 |
| blackwire-current | ok | 0.50 | 0.90 | 1.30 | 14441.64 | 0 |
| blackwire-candidate | ok | 0.30 | 0.70 | 1.00 | 21295.12 | 0 |
| xray | ok | 0.40 | 0.70 | 1.00 | 20414.40 | 0 |
| sing-box | ok | 0.30 | 0.60 | 0.80 | 20912.36 | 0 |
| hysteria | ok | 0.50 | 0.80 | 1.20 | 16182.89 | 0 |
| shoes | ok | 0.30 | 0.60 | 0.80 | 23886.92 | 0 |

## Conclusion

Relay Engine v2 candidate improved Blackwire clean 1k throughput by about `47.5%` over current (`21295.12` vs `14441.64` req/s), reduced p50 from `0.50ms` to `0.30ms`, reduced p95 from `0.90ms` to `0.70ms`, and completed with zero errors.

## Setup Note

An earlier candidate run in `clean-remote-20260601T200518Z.jsonl` failed because the benchmark firewall setup had not opened candidate server port `10090/tcp`. The matrix now opens `10090/tcp`, and the successful result above is the valid A/B run.
