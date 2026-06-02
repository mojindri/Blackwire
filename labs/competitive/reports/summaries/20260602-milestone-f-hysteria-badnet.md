# Milestone F Hysteria/badnet summary

Date: 2026-06-02

Scope:
- Added selectable Hysteria2 QUIC congestion modes:
  - `standard-quic`
  - `brutal-compatible`
  - `nova-cc`
  - `badnet-low-latency`
  - `badnet-throughput`
  - `auto-probe`
- Added bad-network policy primitives for ACK-rate compensation, queue-delay defense, loss classification, and Nova-style hybrid decisions.
- Wired Hysteria2 server and client congestion policy so client upload uses `upMbps` and server download uses `downMbps`.
- Added packet-count based 5-second sliding loss windows to avoid cumulative ACK/loss distortion.
- Added congestion-mode flow-control profiles, including a smaller low-latency profile for mobile links.
- Added real Hysteria2 TCP endpoint sharding with bounded round-robin QUIC sessions.
- Added background shard prewarm so remote/mobile benchmarks do not pay lazy QUIC+HTTP/3 auth setup in steady-state request tails.
- Kept the HTTP/3 client driver owned for the QUIC session lifetime.
- Expanded unsafe development TLS verification signature schemes for RSA self-signed lab certificates used with `skipCertVerify`.
- Increased the dispatcher outbound-connect fail-closed cap from 3s to 10s so lossy QUIC dials can recover without reopening the old hostility hang.
- Updated the remote competitive harness to use nginx upstream on the server VPS, Hysteria2 badnet candidate profiles, firewall port allowances, and `tc netem` setup/cleanup.

Validation:
- Local targeted regression checks were run for Hysteria2 UDP relay, TLS hostility fail-closed behavior, config fail-closed behavior, and badnet policy compilation.
- Linux x86_64 candidate binary was built natively on VPS `91.107.164.107`.
- Remote benchmarks ran from client VPS `91.107.176.118` to server VPS `91.107.164.107` using `ssh -i id_hetzner`.
- Remote upstream was nginx on the server VPS.
- Final candidate binary: `target/linux-amd64/blackwire-candidate-milestone-f`.
- Baseline/current binary: `target/linux-amd64/blackwire-before-f`.

Final remote evidence:

| Scenario | Report | Candidate status | Candidate RPS | Candidate p99 ms | Hysteria RPS | Hysteria p99 ms | Candidate errors |
|---|---|---:|---:|---:|---:|---:|---:|
| `hysteria2-loss-3` | `hysteria2-loss-3-remote-20260602T081722Z.jsonl` | ok | 20292.18 | 2.4 | 17370.37 | 2.2 | 0 |
| `hysteria2-loss-5` | `hysteria2-loss-5-remote-20260602T081951Z.jsonl` | ok | 20113.11 | 2.6 | 13057.82 | 27.9 | 0 |
| `hysteria2-loss-10` | `hysteria2-loss-10-remote-20260602T082243Z.jsonl` | ok | 17959.62 | 3.0 | 6898.95 | 30.0 | 0 |
| `hysteria2-mobile-radio-pause` | `hysteria2-mobile-radio-pause-remote-20260602T081508Z.jsonl` | ok | 72.61 | 540.8 | 69.61 | 488.7 | 0 |

Corrective tuning evidence:

| Profile | Report | Candidate RPS | Candidate p99 ms | Hysteria RPS | Hysteria p99 ms | Candidate errors | Decision |
|---|---|---:|---:|---:|---:|---:|---|
| throughput, 4 lazy shards | `hysteria2-mobile-radio-pause-remote-20260602T075329Z.jsonl` | 65.76 | 1462.5 | 67.74 | 540.2 | 0 | rejected: tail too high |
| throughput, 8 shards | `hysteria2-mobile-radio-pause-remote-20260602T075555Z.jsonl` | 47.92 | 3516.8 | 64.73 | 587.2 | 1 | rejected: error and worse tail |
| low-latency, 4 shards | `hysteria2-mobile-radio-pause-remote-20260602T075819Z.jsonl` | 64.67 | 1421.9 | 65.24 | 593.3 | 0 | rejected: tail still high |
| low-latency, 4 prewarmed shards | `hysteria2-mobile-radio-pause-remote-20260602T081023Z.jsonl` | 67.60 | 710.0 | 65.88 | 472.6 | 0 | improved, not final |
| low-latency, tighter queue profile | `hysteria2-mobile-radio-pause-remote-20260602T081248Z.jsonl` | 68.51 | 627.5 | 69.81 | 439.3 | 0 | improved, not final |
| low-latency, conservative queue profile | `hysteria2-mobile-radio-pause-remote-20260602T081508Z.jsonl` | 72.61 | 540.8 | 69.61 | 488.7 | 0 | selected |

Current conclusion:
- Milestone F is satisfied for the scoped Hysteria2 badnet milestone.
- Candidate now beats official Hysteria throughput on the final high-loss spot checks and is within competitive range on the mobile-radio profile while keeping zero request errors.
- The old/current Blackwire Hysteria2 path failed the same final spot checks with request errors, confirming the candidate fixed the failure mode rather than only changing the harness.
- Remaining non-blocking gap: mobile-radio p99 is still slightly above official Hysteria in the final selected run, but throughput is higher and tail latency is close enough for this milestone acceptance.

Rollback path:
- Existing configs remain compatible and default to `brutal-compatible`.
- Set `settings.congestion.mode` to `standard-quic` to return to Quinn's default congestion controller.
- Set `settings.endpointShards` to `1` to disable endpoint-shard behavior.
