# Hysteria2 UDP/DNS Loss Benchmark Summary

Date: 2026-06-02

Scope:
- Dedicated UDP/DNS-sized Hysteria2 QUIC DATAGRAM benchmark on VPS.
- Server: `91.107.164.107`
- Client: `91.107.176.118`
- Access: `ssh -i id_hetzner`
- Upstream UDP echo: server loopback `127.0.0.1:1053`
- Probe shape: 200 sequential UDP probes, 64 byte payload, 500 ms per-probe timeout.
- Network impairment: `tc netem loss` on both VPS default interfaces.

Implementation correction made during this pass:
- Priority/DNS Hysteria2 UDP packets now use an isolated upstream UDP socket on the server side.
- Reason: the first H2-plus run showed retry/FEC duplicates could contaminate the shared session socket and produce stale replies.
- Bulk/unreliable UDP keeps the existing per-session upstream socket.

Reports:
- `labs/competitive/reports/hysteria2-udp-dns-loss-3-remote-20260602T115820Z.jsonl`
- `labs/competitive/reports/hysteria2-udp-dns-loss-5-remote-20260602T115958Z.jsonl`
- `labs/competitive/reports/hysteria2-udp-dns-loss-10-remote-20260602T120140Z.jsonl`

Final selected mode:
- Candidate: `blackwire-candidate-h2plus-udp`
- Datagram policy: `h2-plus`
- DNS retry: enabled
- FEC mode: `off`
- FEC overhead: 0% in selected rows

Why FEC is off in selected rows:
- The initial `fec=auto` run at 3% loss produced stale duplicate replies in the sequential benchmark:
  - Standard: `480/500 ok`, `20 errors`, `0 stale`, p99 `0.815 ms`
  - H2-plus + auto FEC: `366/500 ok`, `134 errors`, `106 stale`, p99 `0.868 ms`
  - Official Hysteria: `471/500 ok`, `29 errors`, p99 `1.274 ms`
- This means current FEC behavior is not acceptable for sequential DNS-style probes yet.

Final selected UDP/DNS rows:

| Loss | Variant | OK / Requests | Errors | Stale | RPS | p95 ms | p99 ms |
|---:|---|---:|---:|---:|---:|---:|---:|
| 3% | Blackwire standard | 188 / 200 | 12 | 0 | 31.06 | 0.352 | 0.808 |
| 3% | Blackwire H2-plus | 190 / 200 | 10 | 0 | 37.59 | 0.734 | 0.919 |
| 3% | Official Hysteria | 183 / 200 | 17 | 0 | 21.36 | 1.007 | 1.149 |
| 5% | Blackwire standard | 178 / 200 | 22 | 0 | 16.08 | 0.792 | 0.860 |
| 5% | Blackwire H2-plus | 184 / 200 | 16 | 0 | 22.83 | 0.784 | 0.853 |
| 5% | Official Hysteria | 183 / 200 | 17 | 0 | 21.34 | 1.056 | 1.188 |
| 10% | Blackwire standard | 169 / 200 | 31 | 0 | 10.84 | 0.795 | 1.015 |
| 10% | Blackwire H2-plus | 157 / 200 | 43 | 0 | 7.27 | 0.904 | 0.961 |
| 10% | Official Hysteria | 161 / 200 | 39 | 0 | 8.21 | 1.165 | 1.248 |

p99 gate against official Hysteria:
- 3% loss: H2-plus p99 `0.919 ms` vs Hysteria `1.149 ms`, improvement `20.0%`.
- 5% loss: H2-plus p99 `0.853 ms` vs Hysteria `1.188 ms`, improvement `28.2%`.
- 10% loss: H2-plus p99 `0.961 ms` vs Hysteria `1.248 ms`, improvement `23.0%`.

Acceptance status:
- The p99 improvement target is satisfied against official Hysteria at 3%, 5%, and 10% loss in the selected FEC-off H2-plus UDP/DNS rows.
- Overhead cap is satisfied for selected rows because FEC is off; selected client payload bytes remain `12800` for 200 x 64 byte probes.
- Reliability is not fully superior at 10% loss: H2-plus had `43` timeouts versus official Hysteria `39`.

Conclusion:
- Current milestone is now satisfied for the narrow UDP/DNS p99 acceptance gate with FEC off.
- FEC auto must remain non-selected for sequential DNS probes until it is made duplicate-safe or benchmarked with a concurrent workload that matches its group-recovery design.
