# Original plan phases 18-21 implementation summary

Date: 2026-06-02 18:27:59 UTC

Scope:
- Phase 18: first-packet acceleration controls and observability.
- Phase 19: data-plane/control-plane split.
- Phase 20: compiled connection plans.
- Phase 21: security and license guardrails.

Implementation completed:
- Added `firstPacketBoost` / `first_packet_boost` config support with explicit knobs for DNS prefetch, TLS ClientHello eligibility, early payload forwarding, bad-network control duplication policy, and first-packet priority.
- Preserved existing DNS route-prefetch behavior when `firstPacketBoost` is absent. When the feature block is enabled, DNS prefetch can be disabled with `dns = false` and successful boost attempts are counted with `blackwire_first_packet_boost_total`.
- Added TTFB and plan observability through `blackwire_ttfb_seconds`, `blackwire_first_packet_boost_total`, and `blackwire_connection_plan_selected_total`.
- Added immutable `DataPlane` snapshots plus `DataPlaneStore` for atomic replacement of hot-path plan data.
- Added compiled `ConnectionPlan` records for listener, outbound, sniffing, routing, relay, limits, and cost data. Instance startup now compiles these plans once and wires per-inbound plan labels into the dispatcher.
- Added MIT license file, third-party reference policy, source-copy policy documentation, and a source-policy CI guard wired into `ci/security/run_dependency_audit.sh`.

Validation run:
- `cargo fmt --all`
- `cargo check -p blackwire-config -p blackwire-app -p blackwire-core`
- `cargo test -p blackwire-core data_plane::`
- `cargo test -p blackwire-config first_packet -- --nocapture`
- `cargo test -p blackwire-app dispatcher::tests:: -- --nocapture`
- `python3 ci/security/check_source_policy.py`

Result:
- All focused compile, unit, dispatcher, data-plane, config, and source-policy checks passed.

Acceptance note:
- Phases 19, 20, and 21 are implementation-complete with focused validation.
- Phase 18 implementation and observability are complete, and it preserves previous DNS-prefetch behavior by default. This pass did not run a fresh VPS short-request/TTFB benchmark, so any final Phase 18 performance acceptance should reference prior early-payload benchmark evidence or be confirmed with a new native benchmark run.
