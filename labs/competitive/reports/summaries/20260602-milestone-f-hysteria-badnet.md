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

Remote benchmark status:
- Remote badnet runner is implemented, but a VPS benchmark was not executed in this pass because no Linux `blackwire` release binary was present at `target/linux-amd64/blackwire` or `target/release/blackwire`.

Rollback path:
- Existing configs remain compatible and default to `brutal-compatible`.
- Set `settings.congestion.mode` to `standard-quic` to return to Quinn's default congestion controller.
- Set `settings.endpointShards` to `1` to disable endpoint-shard behavior.
