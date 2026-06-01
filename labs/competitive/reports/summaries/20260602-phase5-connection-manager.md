# Phase 5 connection manager summary

Date: 2026-06-02

Scope:
- Added `crates/blackwire-connmgr` with active connection metadata, guard-based cleanup, close selectors, snapshots, and metrics helpers.
- Tracked real dispatcher relay lifecycle with inbound, outbound, user, protocol, transport, bytes, age, relay path, and close reason.
- Added manager cancellation semantics so close commands cancel the tracked relay future and drop streams.
- Added Prometheus metrics:
  - `blackwire_connections_active`
  - `blackwire_connections_closed_total{reason}`
  - `blackwire_connections_lifetime_seconds`
  - `blackwire_connections_bytes_total{direction,protocol,transport}`
- Added CLI visibility/control surface:
  - `blackwire connections list`
  - `blackwire connections top --sort bytes`
  - `blackwire connections top --sort age`
  - `blackwire connections close --id ID`
  - `blackwire connections close --user USER`
  - `blackwire connections close --inbound TAG`
  - `blackwire connections close --outbound TAG`

Verification summary:
- `cargo test -p blackwire-connmgr` passed.
- `cargo test -p blackwire-app dispatcher:: -- --nocapture` passed.
- `cargo test -p blackwire-app router:: -- --nocapture` passed; route-cache p99 measured 2.000 us.
- `cargo test -p blackwire-app dns:: -- --nocapture` passed.
- `cargo test -p blackwire -- --nocapture` passed.
- `cargo run -p blackwire -- connections --help` showed the expected list/top/close command surface.
- After clearing generated Cargo build artifacts to recover disk space, `cargo test -p blackwire-core` passed.

Known limitation:
- The CLI commands operate against the in-process manager. Cross-process CLI/API control of a separately running daemon still needs a management RPC endpoint for connection snapshots and close commands.
- Full `cargo test --workspace --all-targets` still has the previously recorded unrelated blocker: `integration-tests --test e2e_hostility tls_handshake_failure` times out waiting for EOF after a Trojan TLS wrong-SNI path.
