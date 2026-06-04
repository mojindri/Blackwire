# Phase 4 route cache summary

Date: 2026-06-02

Scope:
- Added compiled-router route cache keyed by destination, port, network, inbound tag, user, sniffed protocol, and sniffed domain.
- Added cache invalidation on router reload via generation bump and cache clear.
- Added short cache TTL for health/balancer-style outbound tags (`auto-*` and tags containing `balancer`).
- Added user routing condition support from config schema through compiled rules.
- Added short-lived negative DNS failure caching.
- Added route-cache and compiled-rule Prometheus metrics plus DNS prefetch outcome metric.
- Added explicit inbound shutdown/accounting for outbound connect failures before relay starts.
- Added outbound TLS handshake timeout guard while triaging the deterministic TLS SNI mismatch e2e blocker.

Route-cache benchmark result:
- Command: `cargo test -p blackwire-app router:: -- --nocapture`
- Scenario: cached typical route with domain, CIDR, port, inbound tag, user, and sniffed protocol constraints.
- Measured cached route p99: 3.083 us.
- Acceptance target: p99 < 50 us.
- Result: target satisfied.

Verification summary:
- Focused router suite passed.
- Focused DNS suite passed.
- `blackwire-core` package tests passed.
- Direct TLS SNI mismatch security boundary check passed.
- Full `cargo test --workspace --all-targets` was not clean because `integration-tests --test e2e_hostility tls_handshake_failure` deterministically timed out waiting for EOF after a Trojan TLS wrong-SNI path. This appears outside the Phase 4 route-cache path and remains a separate e2e blocker to finish before calling the entire workspace green.
