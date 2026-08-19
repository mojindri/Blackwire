# Relay engine local A/B — 2026-08-19

## Method

Criterion drives the real asynchronous userspace relay functions through paired
Tokio duplex streams. Each sample sends the same payload simultaneously in both
directions. The matrix covers 1 KiB, 16 KiB, 64 KiB and 1 MiB payloads plus an
eight-connection concurrency run at 1 KiB and 64 KiB.

Command:

```sh
cargo bench -p blackwire-benches --bench tcp_relay_throughput -- \
  --noplot --sample-size 30 --warm-up-time 1 --measurement-time 2
```

## Median results

| Payload / concurrency | Legacy | V2 adaptive | Local winner |
|---|---:|---:|---|
| 1 KiB / 1 | 15.07 µs | 15.89 µs | Legacy |
| 16 KiB / 1 | 22.18 µs | 29.01 µs | Legacy |
| 64 KiB / 1 | 48.67 µs | 75.34 µs | Legacy |
| 1 MiB / 1 | 320.91 µs | 583.77 µs | Legacy |
| 1 KiB / 8 | 33.31 µs | 35.26 µs | Legacy |
| 64 KiB / 8 | 366.99 µs | 557.62 µs | Legacy |

## Decision

Do not delete Legacy yet. V2 remains the Automatic default because the existing
remote 1 KiB benchmark measured a 47.5% request-rate improvement on a real
network path, where its scheduling and flush behavior matters. This local
in-memory matrix shows that Legacy still has materially lower pure-copy overhead,
especially for large payloads. Legacy therefore remains an expert troubleshooting
fallback until V2 closes the local bulk-copy gap or a broader remote matrix shows
that the gap is irrelevant in production.
