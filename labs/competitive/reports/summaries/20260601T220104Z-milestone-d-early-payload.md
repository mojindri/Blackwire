# Milestone D: Early Payload And Handshake Kick Dispatch

Timestamp: 2026-06-01T22:01:04Z

## Scope

Implemented the Phase 3 early-payload dispatch slice:

- Added `OutboundConnectResult` and `connect_with_early_payload` to outbound handlers.
- Added dispatcher support for inbound bytes already read past protocol handshake.
- Preserved early HTTP CONNECT tunnel bytes captured after the CONNECT header.
- Coalesced early payload into VLESS and Trojan outbound handshakes, including transport-wrapped and REALITY VLESS paths.
- Kept Freedom pooled-socket first-use retry protection by leaving early bytes in the dispatcher guard path for Freedom.
- Added early-payload and handshake-kick metrics, including `blackwire_first_byte_latency_seconds`.

## Validation Summary

Behavioral validation covered:

- HTTP CONNECT response ordering with a coalesced first tunnel payload.
- VLESS and Trojan outbound helper behavior: handshake decode followed by exact first payload bytes.
- Freedom pooled-socket path preserved first-write guard semantics for early bytes.
- Protocol benchmark target smoke through `cargo test --workspace --all-targets`, including SS2022, Trojan TCP, VLESS TCP, VLESS WS, VMess gRPC, protocol handshake latency, framing overhead, routing/DNS/FakeIP, and relay throughput targets.

Targeted commands run after the final metric/pool corrections:

- `cargo test -p blackwire-app metrics::tests::metrics_helpers_do_not_panic`
- `cargo test -p integration-tests --test e2e_http_connect http_connect_coalesced_first_payload_roundtrip -- --nocapture`

Full-suite command completed before the final metric descriptor/helper addition:

- `cargo test --workspace --all-targets`

## Notes

This milestone validates the early-payload plumbing and ordering guarantees locally. It does not include a remote A/B latency benchmark against the VPS candidate/old binaries, so the Phase 3 quantitative TTFB improvement target still needs a dedicated remote benchmark report before claiming the performance target numerically.
