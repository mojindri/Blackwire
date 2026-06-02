# Phase 6 cost-budget and explain-cost summary

Date: 2026-06-02

Scope:
- Added performance budget profile names:
  - `latency`
  - `throughput`
  - `badnet`
  - `mobile`
  - `stealth`
- Kept existing `compat` and `fast` behavior compatible.
- Added optional top-level `budget` config:
  - `maxProtocolLayers`
  - `allowSniffing`
  - `allowFakeIp`
  - `maxRouteRules`
  - `maxHandshakeMs`
  - `preferDirectCopy`
  - `preferDatagramForUdp`
- Added config cost model:
  - CPU cost class
  - allocation cost class
  - latency cost class
  - copy mode
  - direct-copy/splice/early-payload/datagram capability flags
- Added `blackwire explain-cost -c CONFIG [--profile PROFILE]`.
- Added stable text output with hot-path layers, findings, and suggested fixes.

Validation summary:
- `cargo test -p blackwire-config profile:: -- --nocapture` passed.
- `cargo test -p blackwire-core --test config_fail_closed -- --nocapture` passed.
- `cargo test -p blackwire-core --test reload_listeners -- --nocapture` passed.
- `cargo test -p blackwire -- --nocapture` passed.
- `blackwire test -c labs/competitive/configs/blackwire/vless-client.json --profile mobile` returned `Config OK`.
- `blackwire explain-cost -c labs/competitive/configs/blackwire/vless-server-candidate.json --profile throughput` reported no findings for the direct VLESS/Freedom path.
- `blackwire explain-cost -c labs/competitive/configs/blackwire/vless-client.json --profile latency` reported the expected layer-count warning for a SOCKS-in to VLESS-out local client path.

Milestone status:
- Phase 6 is implemented at the config and operator-diagnostics layer.
- The new profiles do not yet enforce runtime hard rejections except for the existing `fast` profile. That is intentional for this slice: operators can now inspect and tune cost before future phases wire budget rejection into startup policy.

Rollback path:
- Use `profile: "compat"` or omit `profile` to preserve previous broad compatibility behavior.
- Omit `budget` to use profile defaults.
