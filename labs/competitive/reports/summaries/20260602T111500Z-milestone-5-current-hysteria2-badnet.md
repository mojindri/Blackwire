# Milestone 5 Current Hysteria2 badnet summary

Date: 2026-06-02

Scope
- Scope is limited to the candidate modes executed in the latest remote run on `91.107.164.107` → `91.107.176.118`.
- Access used: `ssh -i id_hetzner` and native Linux binaries.
- Upstream path used on server: nginx (as required).
- Local scope includes functionality checks already captured in earlier tests and firewall-tcp/udp allowances for benchmark traffic.

Execution
- `COMPETITIVE_MODE=remote`
- Modes:
  - `badnet-throughput`
  - `badnet-low-latency`
  - `nova-cc`
- Scenarios:
  - `hysteria2-loss-1`, `hysteria2-loss-3`, `hysteria2-loss-5`
  - `hysteria2-loss-10`, `hysteria2-rtt-50`, `hysteria2-rtt-100`
  - `hysteria2-mobile-radio-pause`
- All report files are under:
  - `labs/competitive/reports/hysteria2-*-remote-20260602T1*.jsonl`

TCP-stream Hysteria2 rows
- `blackwire-candidate-badnet-throughput`, `blackwire-candidate-badnet-low-latency`, `blackwire-candidate-nova-cc`
- Baseline compared with `hysteria` row inside the same file.

| Mode | Scenario | Report | Candidate RPS | Baseline RPS | Candidate p99 ms | Baseline p99 ms | p99 Δ% (improve) | Candidate status | Candidate errors |
|---|---|---|---:|---:|---:|---:|---|---:|
| badnet-throughput | hysteria2-loss-1 | `hysteria2-loss-1-remote-20260602T103756Z.jsonl` | 20259.81 | 18985.9971 | 2.30 | 2.10 | -9.52 | ok | 0 |
| badnet-throughput | hysteria2-loss-3 | `hysteria2-loss-3-remote-20260602T103933Z.jsonl` | 20868.74 | 18673.12 | 2.40 | 2.00 | -20.00 | ok | 0 |
| badnet-throughput | hysteria2-loss-5 | `hysteria2-loss-5-remote-20260602T104104Z.jsonl` | 20847.82 | 14146.74 | 2.40 | 3.50 | 31.43 | ok | 0 |
| badnet-throughput | hysteria2-loss-10 | `hysteria2-loss-10-remote-20260602T104238Z.jsonl` | 18214.55 | 7948.14 | 2.90 | 29.30 | 90.10 | ok | 0 |
| badnet-throughput | hysteria2-rtt-50 | `hysteria2-rtt-50-remote-20260602T104416Z.jsonl` | 156.34 | 156.12 | 203.60 | 204.80 | 0.59 | ok | 0 |
| badnet-throughput | hysteria2-rtt-100 | `hysteria2-rtt-100-remote-20260602T104604Z.jsonl` | 77.86 | 77.73 | 404.10 | 405.20 | 0.27 | ok | 0 |
| badnet-throughput | hysteria2-mobile-radio-pause | `hysteria2-mobile-radio-pause-remote-20260602T104810Z.jsonl` | 71.58 | 69.83 | 606.70 | 492.80 | -23.11 | ok | 0 |
| badnet-low-latency | hysteria2-loss-1 | `hysteria2-loss-1-remote-20260602T105026Z.jsonl` | 21764.27 | 20985.76 | 2.40 | 1.90 | -26.32 | ok | 0 |
| badnet-low-latency | hysteria2-loss-3 | `hysteria2-loss-3-remote-20260602T105159Z.jsonl` | 21460.71 | 17021.54 | 2.20 | 2.20 | 0.00 | ok | 0 |
| badnet-low-latency | hysteria2-loss-5 | `hysteria2-loss-5-remote-20260602T105333Z.jsonl` | 20036.34 | 14303.87 | 2.50 | 3.10 | 19.35 | ok | 0 |
| badnet-low-latency | hysteria2-loss-10 | `hysteria2-loss-10-remote-20260602T105503Z.jsonl` | 17857.95 | 6934.28 | 3.00 | 30.10 | 90.03 | ok | 0 |
| badnet-low-latency | hysteria2-rtt-50 | `hysteria2-rtt-50-remote-20260602T105650Z.jsonl` | 156.34 | 156.24 | 203.30 | 205.70 | 1.17 | ok | 0 |
| badnet-low-latency | hysteria2-rtt-100 | `hysteria2-rtt-100-remote-20260602T105840Z.jsonl` | 77.88 | 77.73 | 403.30 | 405.90 | 0.64 | ok | 0 |
| badnet-low-latency | hysteria2-mobile-radio-pause | `hysteria2-mobile-radio-pause-remote-20260602T110050Z.jsonl` | 68.62 | 69.61 | 538.40 | 509.90 | -5.59 | ok | 0 |
| nova-cc | hysteria2-loss-1 | `hysteria2-loss-1-remote-20260602T110302Z.jsonl` | 20899.51 | 18522.52 | 2.30 | 2.20 | -4.55 | ok | 0 |
| nova-cc | hysteria2-loss-3 | `hysteria2-loss-3-remote-20260602T110432Z.jsonl` | 21304.12 | 18505.05 | 2.40 | 2.00 | -20.00 | ok | 0 |
| nova-cc | hysteria2-loss-5 | `hysteria2-loss-5-remote-20260602T110608Z.jsonl` | 19903.13 | 15336.99 | 2.60 | 2.90 | 10.34 | ok | 0 |
| nova-cc | hysteria2-loss-10 | `hysteria2-loss-10-remote-20260602T110743Z.jsonl` | 18473.72 | 7303.90 | 2.80 | 29.70 | 90.57 | ok | 0 |
| nova-cc | hysteria2-rtt-50 | `hysteria2-rtt-50-remote-20260602T110928Z.jsonl` | 156.21 | 156.28 | 203.70 | 204.50 | 0.39 | ok | 0 |
| nova-cc | hysteria2-rtt-100 | `hysteria2-rtt-100-remote-20260602T111120Z.jsonl` | 77.88 | 77.73 | 404.20 | 404.50 | 0.07 | ok | 0 |
| nova-cc | hysteria2-mobile-radio-pause | `hysteria2-mobile-radio-pause-remote-20260602T111332Z.jsonl` | 67.73 | 69.14 | 546.40 | 470.30 | -16.18 | ok | 0 |

UDP/DNS/FEC rows
- Not present in this runset.
- This milestone therefore has no direct lossy UDP DNS benchmark table yet.

Performance gate check
- Required in your note: p99 improvement >= 15% at 3%, 5%, and 10% loss.
- Result: `5 / 9` rows pass this gate.
- Failing rows in this mode set:
  - `badnet-throughput`: `hysteria2-loss-1`, `hysteria2-loss-3`
  - `badnet-low-latency`: `hysteria2-loss-1`, `hysteria2-loss-3`
  - `nova-cc`: `hysteria2-loss-1`, `hysteria2-loss-3`, `hysteria2-loss-5`

Failed rows
- None for the 21 TCP-stream rows above (all `status: ok`, no errors).

Skipped rows
- Dedicated lossy UDP/DNS rows and dedicated FEC-only runs are still not executed in this phase.

Functional notes
- Candidate mode still shows stable TLS/handshake behavior in this phase (no request failures in these rows).
- Existing feedback from earlier iterations still applies:
  - old/current Blackwire failure mode around `tls: bad record MAC` is no longer seen in current candidate-focused runs.

Conclusion
- **Current milestone is not fully satisfied yet**.
- Candidate is directionally correct and stable for these TCP-stream Hysteria2 badnet runs, but final acceptance is blocked by missing required lossy UDP/DNS coverage and incomplete p99 gate coverage at all 3/5/10% loss points.
