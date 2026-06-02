# Milestone I TUN TCP/bulk harness closure

Date: 2026-06-02

## Scope

Closed the TUN TCP/bulk benchmark harness gap so TCP rows no longer depend on the broken `91.107.176.118` outbound-TCP client role or on same-destination TUN recursion.

The usable TCP/bulk role split is:

- TCP-capable TUN client: `91.107.164.107`
- Remote UDP/nginx control host: `91.107.176.118`
- External IP bulk target: `http://80.249.99.148/50MB.zip`

## Harness Fixes

- Reversed the usable benchmark role for TCP/bulk because `91.107.176.118` still cannot open ordinary outbound TCP connections.
- Added nginx readiness fallback to accept a listening socket when local curl is unavailable on the TCP-restricted VPS.
- Added bounded TUN TCP curl probes via `TUN_TCP_TIMEOUT_SEC` so failed TCP rows cannot hang the harness or leave TUN policy routes active.
- Added `TUN_TCP_TARGET_URL` so TCP/bulk can use an external target instead of the same host used for control/upstream setup.
- Fixed TCP row status handling so partial timed-out curl downloads remain `failed`.
- Updated the sing-box TUN competitor config to use Linux `stack: system` with `auto_redirect`, producing valid TCP rows.

## Blackwire TCP Bridge Fix

Added ACK-based backpressure to the Blackwire packet-level TUN TCP bridge. This prevents large downloads from flooding the client receive window with unrecoverable data.

## Evidence

Valid 1 MB TCP/bulk row:

- Raw report: `labs/competitive/reports/tun-remote-20260602T160551Z.jsonl`
- Blackwire TUN TCP: `47.0496 Mbps`, `ok`
- sing-box TUN TCP: `47.2893 Mbps`, `ok`
- Ratio: `0.995x` sing-box, passes `>= 0.90x`

Valid 50 MB TCP/bulk row after ACK backpressure:

- Raw report: `labs/competitive/reports/tun-remote-20260602T161446Z.jsonl`
- Blackwire TUN TCP: `158.3541 Mbps`, `ok`
- sing-box TUN TCP: `839.0437 Mbps`, `ok`
- Ratio: `0.189x` sing-box, fails `>= 0.90x`

Regression/tuning attempts retained as evidence:

- `labs/competitive/reports/tun-remote-20260602T160730Z.jsonl`: before ACK backpressure, Blackwire timed out on 50 MB after `3.32 MB`; sing-box completed at `836.4518 Mbps`.
- `labs/competitive/reports/tun-remote-20260602T161912Z.jsonl`: 4 MB bridge window overran the client and timed out.
- `labs/competitive/reports/tun-remote-20260602T162542Z.jsonl`: 512 KB bridge window also timed out.

## Acceptance Status

Harness gap: closed. The benchmark can now produce valid Blackwire and sing-box TUN TCP rows, and cleanup leaves the VPS route table clean.

Milestone I TCP/bulk acceptance: not accepted for the 50 MB bulk gate. Blackwire now completes the large transfer with ACK backpressure, but throughput is still far below the `>= sing-box * 0.90` target.

Next required fix: replace or substantially improve the packet-level Linux TUN TCP bridge. The current bridge is correctness-oriented and still much slower than sing-box's Linux TUN TCP path on large bulk transfers.

