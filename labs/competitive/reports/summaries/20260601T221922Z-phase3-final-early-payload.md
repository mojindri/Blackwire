# Phase 3 Final: Early Payload And Handshake Kick Dispatch

Timestamp: 2026-06-01T22:19:22Z

## Scope Closed

This finalizes the current Phase 3 milestone beyond the earlier slice:

- SOCKS5 CONNECT now preserves bytes coalesced after the CONNECT request and dispatches them as early payload after the SOCKS success reply.
- VMess outbound implements `connect_with_early_payload` and writes the first payload through the encrypted VMess stream after the response header.
- SS-2022 outbound uses the SIP022 request variable-header initial payload field for early payload.
- Freedom pooled sockets keep first-use stale-socket retry protection; early payload is replayed through the dispatcher guard instead of being written before validation.
- Existing HTTP CONNECT, VLESS, Trojan, REALITY, Vision, metrics, and dispatcher early-payload support remain in place.

## Behavioral Validation

Targeted Phase 3 checks run:

- `cargo test -p blackwire-protocol -- --nocapture`
- `cargo test -p blackwire-app pooled_first_write_retries_stale_socket_with_early_payload -- --nocapture`
- `cargo test -p integration-tests --test e2e_http_connect http_connect_coalesced_first_payload_roundtrip -- --nocapture`
- `cargo test -p integration-tests --test e2e_vmess vmess_coalesced_first_payload_echo -- --nocapture`
- `cargo test -p integration-tests --test e2e_ss2022 ss2022_coalesced_first_payload_echo -- --nocapture`

Coverage added in this final pass:

- SOCKS success response ordering with coalesced first payload.
- VMess first payload interop through SOCKS CONNECT -> VMess -> Freedom.
- SS-2022 first payload interop through SOCKS CONNECT -> SS-2022 -> Freedom.
- Pooled stale Freedom socket retry with early payload.

## Focused TTFB Benchmark

Raw result file:

- `labs/competitive/reports/phase3-early-payload-local-20260601T221907Z.jsonl`

Benchmark setup:

- Base binary: `4ac0b70` (`Add Vision direct-copy policy and benchmark`), before Phase 3.
- Candidate binary: current Phase 3 tree.
- Path: HTTP CONNECT inbound -> VLESS outbound -> VLESS inbound -> Freedom -> local HTTP upstream.
- Client behavior: sends HTTP CONNECT request and first HTTP request in the same write.
- Samples: 40 warmup, 240 short-request samples, 20 1 MiB bulk samples per variant.

Results:

| Variant | Status | Short TTFB p50 | Short TTFB p95 | 1 MiB bulk p50 | 1 MiB bulk p95 |
|---|---|---:|---:|---:|---:|
| `blackwire-base` | ok | 0.298 ms | 0.378 ms | 0.001282 s | 0.001599 s |
| `blackwire-candidate` | ok | 0.283 ms | 0.372 ms | 0.001189 s | 0.001367 s |

Acceptance read:

- Short-request p50 TTFB improvement: `5.16%`.
- 1 MiB bulk median delta: `-7.29%` wall time, so no bulk regression in this focused run.
- No data corruption observed in the coalesced-payload tests or benchmark response validation.

## Conclusion

Phase 3 is satisfied for the current milestone: the requested early-payload paths are implemented, ordering and preservation tests are in place, pooled stale retry behavior is covered, required metrics exist, and the focused short-request benchmark meets the `>= 5%` TTFB target without a bulk regression.
