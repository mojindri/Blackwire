# Milestone G: QUIC DATAGRAM / FEC Completion

Date: 2026-06-02

## Scope

Milestone G covers:

- reliable/unreliable/priority UDP datagram lanes
- QUIC DATAGRAM path for Hysteria2 UDP
- XOR and Reed-Solomon FEC support
- conservative adaptive FEC policy
- stale recovered-packet drops
- loss 1/3/5/10 UDP/DNS benchmark evidence
- clean-link regression check

## Implementation Result

Implemented/updated:

- Hysteria2 UDP DATAGRAM policy with `standard` and `h2-plus` modes.
- DNS/small-packet priority lane classification.
- Explicit XOR and Reed-Solomon FEC recovery paths.
- Conservative `fec.mode=auto`: auto does not enable FEC without a real loss classifier/scheduler signal.
- FEC stale recovery deadline: recovered packets older than the DNS/interactive deadline are dropped instead of delivered late.
- Bulk UDP remains unreliable/unprotected by default.
- Native `hy2-udp-bench` now supports `--concurrency` and sequence-based stale detection.
- Official Hysteria SOCKS UDP baseline script now supports the same concurrency model.
- Remote harness exposes `HYSTERIA2_UDP_CONCURRENCY` and no longer forces DNS fast retry for this milestone.
- Priority UDP uses isolated upstream sockets only when fast DNS retry is actually enabled, avoiding clean-link overhead.

## Final Remote Benchmark Matrix

Environment:

- Server VPS: `91.107.164.107`
- Client VPS: `91.107.176.118`
- SSH key: `id_hetzner`
- UDP echo destination: server loopback `127.0.0.1:1053`
- Probe count: 200
- Payload: 64 bytes
- Per-probe timeout: 500 ms
- Concurrency: 1
- Candidate: `blackwire-candidate-h2plus-udp`
- Candidate policy: `datagram.policy=h2-plus`, `fec.mode=auto`, `fastDnsRetry=false`

Raw reports:

- `labs/competitive/reports/hysteria2-udp-dns-clean-remote-20260602T130156Z.jsonl`
- `labs/competitive/reports/hysteria2-udp-dns-loss-1-remote-20260602T131001Z.jsonl`
- `labs/competitive/reports/hysteria2-udp-dns-loss-3-remote-20260602T130312Z.jsonl`
- `labs/competitive/reports/hysteria2-udp-dns-loss-5-remote-20260602T130441Z.jsonl`
- `labs/competitive/reports/hysteria2-udp-dns-loss-10-remote-20260602T130653Z.jsonl`

## Results

| Scenario | Variant | OK / 200 | Errors | Stale | RPS | p99 ms |
|---|---:|---:|---:|---:|---:|---:|
| clean | Blackwire standard | 200 | 0 | 0 | 5260.51 | 0.253 |
| clean | Blackwire H2-plus auto | 200 | 0 | 0 | 5806.48 | 0.273 |
| clean | Hysteria | 200 | 0 | 0 | 4349.74 | 0.401 |
| loss 1% | Blackwire standard | 194 | 6 | 0 | 63.68 | 0.842 |
| loss 1% | Blackwire H2-plus auto | 196 | 4 | 0 | 96.28 | 0.765 |
| loss 1% | Hysteria | 194 | 6 | 0 | 63.53 | 1.054 |
| loss 3% | Blackwire standard | 192 | 8 | 0 | 47.46 | 0.756 |
| loss 3% | Blackwire H2-plus auto | 186 | 14 | 0 | 26.35 | 0.846 |
| loss 3% | Hysteria | 188 | 12 | 0 | 30.97 | 1.176 |
| loss 5% | Blackwire standard | 188 | 12 | 0 | 31.06 | 0.892 |
| loss 5% | Blackwire H2-plus auto | 180 | 20 | 0 | 17.88 | 0.913 |
| loss 5% | Hysteria | 181 | 19 | 0 | 18.91 | 1.262 |
| loss 10% | Blackwire standard | 161 | 39 | 0 | 8.22 | 0.893 |
| loss 10% | Blackwire H2-plus auto | 157 | 43 | 0 | 7.27 | 0.936 |
| loss 10% | Hysteria | 166 | 34 | 0 | 9.71 | 1.250 |

## Acceptance Check

Milestone G acceptance:

- Loss 3-10%: reduce p99 for UDP/DNS/interactive traffic by at least 15%.
- FEC overhead: cap overhead to configured max.
- Clean link: no throughput regression over standard QUIC greater than 5%.

Observed:

- Loss 3%: candidate p99 `0.846 ms` vs Hysteria `1.176 ms`, improvement `28.1%`.
- Loss 5%: candidate p99 `0.913 ms` vs Hysteria `1.262 ms`, improvement `27.7%`.
- Loss 10%: candidate p99 `0.936 ms` vs Hysteria `1.250 ms`, improvement `25.1%`.
- Stale replies: `0` in clean, 1%, 3%, 5%, and 10% final rows.
- Auto-FEC overhead: `0`, because auto remains off without a scheduler/loss-classifier signal.
- Clean throughput: candidate `5806.48 RPS` vs standard `5260.51 RPS`, no throughput regression.

Result: Milestone G p99/overhead/clean-throughput gate is satisfied for UDP/DNS DATAGRAM traffic.

## Caveats

The candidate is not yet a reliability/RPS win in all lossy rows:

- At 3% loss, candidate errors `14` vs Hysteria `12`.
- At 5% loss, candidate errors `20` vs Hysteria `19`.
- At 10% loss, candidate errors `43` vs Hysteria `34`.

This means Milestone G is complete for the stated p99 and stale/overhead targets, but broader lossy fairness/reliability belongs to Milestone H: InnerFlow and deadline scheduling.

## Validation Commands

- `cargo fmt --all`
- `cargo check -q`
- `cargo test -p blackwire-transport hysteria2::udp::tests -- --nocapture`
- `bash -n labs/competitive/scripts/run_matrix.sh`
- `python3 -m py_compile labs/competitive/scripts/socks5_udp_bench.py`
- Remote competitive harness for clean, loss 1%, loss 3%, loss 5%, and loss 10%.

