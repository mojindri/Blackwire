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
- Added Hysteria2 config fields:
  - `settings.congestion.mode`
  - `settings.congestion.minAckRate`
  - `settings.congestion.maxQueueDelayMs`
  - `settings.congestion.pacingGain`
  - `settings.congestion.lossCompensation`
  - `settings.endpointShards`
- Wired Hysteria2 TCP and UDP client paths to the selected congestion controller.
- Added reusable Hysteria2 client QUIC session for TCP streams so badnet loads do not pay a full QUIC+HTTP/3 auth handshake per proxied request.
- Expanded the unsafe development TLS verifier signature scheme list used with `skipCertVerify` so RSA self-signed lab certificates negotiate correctly.
- Increased the dispatcher outbound-connect fail-closed cap from 3s to 10s so lossy QUIC dials can recover without reopening the old hostility hang.
- Added QUIC badnet metrics:
  - `blackwire_quic_congestion_mode_total{mode}`
  - `blackwire_quic_ack_rate{mode}`
  - `blackwire_quic_loss_rate{mode}`
  - `blackwire_quic_queue_delay_ms{mode}`
  - `blackwire_quic_pacing_rate_bps{mode}`
  - `blackwire_quic_cwnd_bytes{mode}`
  - `blackwire_quic_delivery_rate_bps{mode}`
  - `blackwire_quic_endpoint_shards`
  - `blackwire_quic_loss_fingerprint_total{fingerprint}`
- Updated competitive loss/mobile lab wrappers to use Hysteria2-specific scenarios:
  - `hysteria2-loss-1`
  - `hysteria2-loss-3`
  - `hysteria2-loss-5`
  - `hysteria2-loss-10`
  - `hysteria2-rtt-50`
  - `hysteria2-rtt-100`
  - `hysteria2-jitter-20`
  - `hysteria2-bandwidth-10mbps`
  - `hysteria2-mobile-radio-pause`
- Added remote VPS Hysteria2 badnet orchestration for Blackwire current, Blackwire candidate, and official Hysteria baseline, including firewall ports and `tc netem` setup/cleanup.

Verification summary:
- Hysteria2 UDP relay integration passed.
- Blackwire core production/config/reload suites passed.
- Competitive local Hysteria2 loss report smoke produced JSONL rows with `loss_percent` metadata.
- Shell syntax checks passed for `run_matrix.sh`, `run_loss.sh`, and `run_mobile_roaming.sh`.
- `integration-tests --test e2e_hostility tls_handshake_failure` still passed after increasing the outbound-connect timeout.

Remote benchmark status:
- Built Linux x86_64 binaries on VPS `91.107.164.107`:
  - Baseline before Milestone F: `target/linux-amd64/blackwire-before-f`
  - Candidate after Milestone F fixes: `target/linux-amd64/blackwire-candidate-milestone-f`
- Ran remote badnet matrix from client VPS `91.107.176.118` against server VPS `91.107.164.107` with `tc netem` scenarios and official Hysteria baseline.
- Final report files:
  - `labs/competitive/reports/hysteria2-loss-1-remote-20260602T070355Z.jsonl`
  - `labs/competitive/reports/hysteria2-loss-3-remote-20260602T070516Z.jsonl`
  - `labs/competitive/reports/hysteria2-loss-5-remote-20260602T070640Z.jsonl`
  - `labs/competitive/reports/hysteria2-loss-10-remote-20260602T070809Z.jsonl`
  - `labs/competitive/reports/hysteria2-rtt-50-remote-20260602T070948Z.jsonl`
  - `labs/competitive/reports/hysteria2-rtt-100-remote-20260602T071131Z.jsonl`
  - `labs/competitive/reports/hysteria2-jitter-20-remote-20260602T071317Z.jsonl`
  - `labs/competitive/reports/hysteria2-bandwidth-10mbps-remote-20260602T071440Z.jsonl`
  - `labs/competitive/reports/hysteria2-mobile-radio-pause-remote-20260602T071616Z.jsonl`

Final remote result summary:

| Scenario | Candidate status | Candidate RPS | Candidate p99 ms | Hysteria RPS | Hysteria p99 ms | Candidate errors |
|---|---:|---:|---:|---:|---:|---:|
| `hysteria2-loss-1` | ok | 20356.86 | 2.2 | 20842.55 | 1.8 | 0 |
| `hysteria2-loss-3` | ok | 15249.76 | 26.9 | 18812.92 | 2.0 | 0 |
| `hysteria2-loss-5` | ok | 8065.55 | 28.2 | 14972.26 | 2.8 | 0 |
| `hysteria2-loss-10` | ok | 1673.33 | 109.5 | 7548.28 | 29.3 | 0 |
| `hysteria2-rtt-50` | ok | 151.30 | 460.5 | 156.42 | 203.6 | 0 |
| `hysteria2-rtt-100` | ok | 73.33 | 1060.0 | 77.72 | 405.8 | 0 |
| `hysteria2-jitter-20` | ok | 24212.89 | 1.8 | 19082.95 | 2.1 | 0 |
| `hysteria2-bandwidth-10mbps` | ok | 23156.41 | 1.9 | 20080.87 | 1.9 | 0 |
| `hysteria2-mobile-radio-pause` | ok | 32.45 | 2521.9 | 68.47 | 500.0 | 0 |

Conclusion:
- Milestone F is implemented and remotely validated for functional badnet stability: candidate completed every scenario with zero request errors.
- The competitive performance acceptance target is not fully met. Candidate is close on `loss-1`, `rtt-50`, `rtt-100` throughput, and beats Hysteria on jitter/bandwidth smoke rows, but p99 latency and high-loss/mobile throughput still lag official Hysteria.
- Next optimization should focus on true Hysteria2 session scheduling and pacing behavior, not just congestion-window policy.

Rollback path:
- Existing configs remain compatible and default to `brutal-compatible`.
- Set `settings.congestion.mode` to `standard-quic` to return to Quinn's default congestion controller.
- Set `settings.endpointShards` to `1` to disable endpoint-shard behavior.
