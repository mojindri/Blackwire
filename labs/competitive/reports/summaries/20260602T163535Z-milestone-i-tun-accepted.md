# Milestone I TUN acceptance

Date: 2026-06-02

## Scope

Finalized Milestone I by closing the TCP/bulk harness gap and fixing the Linux TUN TCP bridge so the native VPS benchmark now produces valid passing UDP and TCP rows against sing-box.

VPS role split used for the accepted TCP/bulk rows:

- TUN client: `<server-host>`
- Control/upstream host: `<client-host>`
- External bulk target: `http://80.249.99.148/50MB.zip`

## Final Fixes

- Added nginx readiness fallback for TCP-restricted VPS hosts.
- Added bounded TUN TCP curl probes and explicit external TCP target support in the harness.
- Updated the sing-box TUN competitor config to use Linux `stack: system` with `auto_redirect`.
- Fixed TCP row status handling so partial timed-out downloads stay failed.
- Added ACK-aware pacing to the Blackwire Linux TUN TCP bridge.
- Added TCP window-scale negotiation and paced sending against the client-advertised receive window instead of a fixed guessed bridge window.

## Acceptance Evidence

Raw accepted report:

- `labs/competitive/reports/tun-remote-20260602T163535Z.jsonl`

Accepted rows:

| Variant | Transport | Result |
| --- | --- | --- |
| Blackwire candidate | TUN UDP | `0.190 ms` p99 |
| sing-box | TUN UDP | `0.253 ms` p99 |
| Blackwire candidate | TUN TCP | `829.0384 Mbps` |
| sing-box | TUN TCP | `840.2842 Mbps` |

Gate evaluation:

- UDP p99 ratio: `0.190 / 0.253 = 0.75x` sing-box, passes `<= 1.15x`
- TCP throughput ratio: `829.0384 / 840.2842 = 0.987x` sing-box, passes `>= 0.90x`

Supporting earlier valid row:

- `labs/competitive/reports/tun-remote-20260602T160551Z.jsonl`
- 1 MB TCP row also passed at `0.995x` sing-box

## Status

Milestone I is accepted.

Both required gates are now satisfied on the native VPS benchmark:

- TUN UDP p99 `<= sing-box * 1.15`
- TUN TCP throughput `>= sing-box * 0.90`

