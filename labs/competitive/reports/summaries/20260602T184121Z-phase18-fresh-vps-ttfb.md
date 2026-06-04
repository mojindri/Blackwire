# Phase 18 fresh VPS TTFB benchmark

Date: 2026-06-02 18:41:21 UTC

Goal:
- Fresh TTFB validation for Phase 18 after commit `dc31b688`.
- Compare the pre-Phase-18/21 Linux binary against the fresh native Linux candidate binary.

Environment:
- VPS used for completed native run: `<server-host>`.
- Native nginx upstream, restarted under `/var/tmp/blackwire-ttfb-nginx-20260602T184121Z`.
- Old binary: `target/linux-amd64/blackwire-before-phase18-21-ttfb`.
- Candidate binary: `target/linux-amd64/blackwire-phase18-21-ttfb`, built natively on `<server-host>`.
- Candidate config enabled `firstPacketBoost` with `priority = critical`.
- Probe: `curl -w %{time_starttransfer}`, 30 warmup requests and 300 measured fresh requests per variant.

VPS networking note:
- The intended two-VPS run was attempted first with server `<server-host>` and client `<client-host>`.
- Direct TCP from `<client-host>` to `<server-host>` on benchmark ports `18080`, `10080`, and `10090` returned `Connection refused`, even though the same ports were reachable from the local machine and UFW allowed them.
- Reverse arbitrary TCP from `<server-host>` to `<client-host>` also timed out.
- Because peer-to-peer VPS TCP was blocked, the completed evidence is a same-host native VPS run on `<server-host>`.

Results:

| Variant | Samples | Errors | TTFB p50 | TTFB p90 | TTFB p95 | TTFB p99 | Max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| direct-vps-native-local | 300 | 0 | 0.363 ms | 0.411 ms | 0.428 ms | 0.526 ms | 0.649 ms |
| blackwire-old-local | 300 | 0 | 0.857 ms | 1.024 ms | 1.066 ms | 1.263 ms | 2.250 ms |
| blackwire-candidate-first-packet-local | 300 | 0 | 0.779 ms | 0.939 ms | 0.978 ms | 1.043 ms | 2.126 ms |

Computed p99 improvement:
- Old p99: `1.263 ms`
- Candidate p99: `1.043 ms`
- Improvement: `17.42%`

Observed metrics:
- Candidate server emitted `blackwire_connection_plan_selected_total{plan="vless-tcp-fast"} 330`.
- Candidate client emitted `blackwire_connection_plan_selected_total{plan="dynamic"} 330`.
- No `blackwire_first_packet_boost_total` sample was emitted in this SOCKS-to-VLESS path during the run.

Acceptance:
- The fresh native VPS TTFB run meets the Phase 18 p99 target in the same-host fallback mode: candidate p99 improved by `17.42%`, above the `>= 10%` gate.
- This is not a full two-VPS network-path acceptance because peer-to-peer TCP between the two VPSes was blocked during the attempt.
