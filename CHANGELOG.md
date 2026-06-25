# Changelog

All notable release-facing changes are documented here.

This project is pre-1.0. The support contract is owned by
[docs/release.md](docs/release.md), and detailed feature evidence is owned by
[docs/feature-matrix.md](docs/feature-matrix.md).

## Unreleased

## 0.1.4 - 2026-06-25

### Changed

- Per-user connection caps are wired into startup and hot-reload paths, so
  `limits.maxConnectionsPerUser` updates can be applied without rebuilding the
  whole instance when the limiter is already active.
- Runtime user bandwidth policies and managed auth stores for VLESS, VMess,
  Trojan, Shadowsocks 2022, Hysteria2, and TUIC are refreshed more consistently
  during Black UI config synchronization.
- Black UI panel mutations now refresh state even when a save partially
  succeeds but live apply/config generation cannot complete yet, so newly saved
  rows remain visible for follow-up edits.
- Black UI save/apply responses now treat the intermediate "no active generated
  inbounds yet" panel state as a non-400 pending state instead of surfacing a
  browser console error during first inbound/user setup.

### Fixed

- Restored the Black UI browser/API QA flow after the subscription copy action
  was intentionally renamed to `Copy subscription content`.
- Fixed workspace CI coverage for integration tests and strict rustdoc after
  reloadable auth-store API changes.

## 0.1.3 - 2026-06-25

### Changed

- Fast profile relay defaults now use relay v2 with adaptive flushing, reducing
  syscall pressure under concurrent streams while preserving low-latency
  behavior.
- Black UI user copy actions now copy fetched subscription content instead of
  the `/sub/{token}` URL.
- Black UI now displays the release version below the Blackwire title in the
  sidebar.

### Fixed

- VLESS mux no longer shares one locked upstream object across read and write
  directions, avoiding write stalls behind pending reads and reducing per-frame
  allocation churn.
- Process-wide connection limits are now shared across TCP listener shards and
  QUIC-family inbounds, including Hysteria2 and TUIC, so `limits.maxConnections`
  behaves consistently.
- Black UI no longer offers `quic` as an inbound sniffing `destOverride`
  option. Runtime sniffing currently supports `http`, `tls`, and `fakedns`;
  copied client links remain unchanged because sniffing is server-side inbound
  configuration.

## 0.1.2 - 2026-06-25

### Fixed

- TLS SNI sniffing now parses non-empty ClientHello session IDs correctly, so
  `destOverride: ["tls"]` can override IP destinations from SNI in modern TLS
  clients.

## 0.1.1 - 2026-06-25

### Fixed

- Inbound sniffing now analyzes protocol early payload before applying
  `destOverride`, so VLESS/VMess/Trojan clients that pass the first HTTP/TLS
  bytes through dispatcher early payload can still override IP destinations
  from sniffed Host/SNI metadata.

## 0.1.0 - 2026-06-25

### Changed

- Finalized the `0.1.0` release line after the RC validation cycle.
- Release docs now point at stable `v0.1.0` install commands and describe final
  release publishing instead of RC-only installation.

### Fixed

- VMess hot paths now avoid warning-level normal traffic logs, avoid per-chunk
  authenticated-length allocations, use pooled UDP relay buffers, coalesce UDP
  response flushes, and advance write buffers without splitting.

## 0.1.0-rc.45 - 2026-06-25

### Fixed

- Generated VMess TLS share links now include Hiddify/ray2sing-compatible
  lowercase insecure flags and a VMess body `security` cipher alongside `scy`,
  so copied self-signed VMess QUIC links preserve TLS trust and cipher settings.

## 0.1.0-rc.44 - 2026-06-25

### Fixed

- Generated VLESS/Trojan TLS share links now include `allowInsecure=1` when
  Blackwire is using its self-signed certificate path, matching the VMess/TUIC
  self-signed behavior and avoiding client-side certificate verification
  failures on copied QUIC links.

## 0.1.0-rc.43 - 2026-06-25

### Changed

- Deprecated mKCP from the release surface. Existing configs remain loadable,
  but Black UI no longer offers mKCP for new inbound/outbound transport
  selection.
- Release docs and feature evidence now classify mKCP as a legacy/internal path
  rather than a supported external-client target.

### Validation

- Live VPS mKCP probes with Xray `26.6.22` and `26.1.23` timed out for VLESS,
  VMess, and Trojan mKCP; current Xray FinalMask-era clients are not treated as
  a supported mKCP target.

## 0.1.0-rc.42 - 2026-06-24

### Fixed

- VLESS/VMess/Trojan QUIC inbounds now advertise `h3` ALPN during QUIC TLS
  handshakes, matching sing-box/Hiddify-style clients and fixing
  `peer doesn't support any known protocol` failures.
- Generated VLESS and Trojan QUIC links now default TLS ALPN to `h3` when the
  inbound does not explicitly set `tlsSettings.alpn`.
- Generated VMess QUIC links now default `scy` to `aes-128-gcm` instead of
  `auto`, avoiding clients that resolve `auto` to VMess `none` and then stall
  after the VMess header is accepted.
- Generated TUIC links now include `alpn=h3` alongside `insecure=1`/`sni` for
  Blackwire self-signed TLS deployments.

### Validation

- `cargo test -p blackwire-transport v2ray_quic_server_accepts_common_h3_alpns -- --nocapture`
- `cargo test -p integration-tests --test e2e_vless_quic -- --nocapture`
- `cargo test -p integration-tests --test e2e_vmess -- --nocapture`
- `cargo test -p black-ui-server subscription -- --nocapture`
- Local patched-server probes with sing-box 1.13.13:
  VLESS QUIC PASS, Trojan QUIC PASS, VMess QUIC PASS with `aes-128-gcm`.

## 0.1.0-rc.41 - 2026-06-24

### Fixed

- VMess authenticated body length framing now matches Xray/sing-box semantics:
  the authenticated length field carries plaintext payload plus padding length,
  not the encrypted payload plus AEAD tag length. This fixes external clients
  that set `authenticated_length: true` and previously connected, decoded the
  VMess header, then failed with EOF before relaying DNS or application bytes.

### Validation

- `cargo test -p blackwire-protocol vmess::stream::tests -- --nocapture`
- `cargo test -p integration-tests --test e2e_vmess -- --nocapture`
- GitHub release tag: `v0.1.0-rc.41`

## 0.1.0-rc.40 - 2026-06-24

### Fixed

- Added initial VMess authenticated-length support for clients that emit
  `authenticated_length: true`.

## 0.1.0-rc.39 - 2026-06-24

### Fixed

- Fixed VMess UDP handling over xHTTP.

## 0.1.0-rc.38 - 2026-06-24

### Fixed

- Fixed xHTTP/SplitHTTP interop paths.

## 0.1.0-rc.37 - 2026-06-24

### Fixed

- Fixed VMess gRPC subscription export.

## 0.1.0-rc.36 - 2026-06-24

### Changed

- Documented the Hiddify VMess HTTPUpgrade limitation.

## 0.1.0-rc.35 - 2026-06-24

### Fixed

- Included xHTTP mode in generated subscription links.

## 0.1.0-rc.34 - 2026-06-24

### Fixed

- Exported SplitHTTP subscription links as xHTTP.

## 0.1.0-rc.33 - 2026-06-23

### Fixed

- Marked generated self-signed VMess TLS links as insecure.

## 0.1.0-rc.32 - 2026-06-23

### Fixed

- Allowed VMess QUIC clients that do not advertise ALPN.

## 0.1.0-rc.31 - 2026-06-23

### Fixed

- Accepted common VMess QUIC ALPN variants.

## 0.1.0-rc.30 - 2026-06-23

### Fixed

- Added `h3` ALPN to VMess QUIC subscription links.

## 0.1.0-rc.29 - 2026-06-23

### Fixed

- Accepted unpadded standard Shadowsocks-2022 keys.

## 0.1.0-rc.28 - 2026-06-23

### Fixed

- Avoided unnecessary listener churn during Black UI live sync.

## 0.1.0-rc.27 - 2026-06-23

### Fixed

- Fixed Shadowsocks-2022 subscription key encoding.

## 0.1.0-rc.26 - 2026-06-23

### Fixed

- Fixed TUIC subscription generation and QUIC stream limits.

## 0.1.0-rc.25 - 2026-06-22

### Fixed

- Allowed multiple gRPC streams per HTTP/2 connection.

## 0.1.0-rc.24 - 2026-06-22

### Fixed

- Flushed small gRPC writes promptly.

## 0.1.0-rc.23 - 2026-06-22

### Fixed

- Merged gRPC and protocol stats fixes.

## 0.1.0-rc.22 - 2026-06-22

### Fixed

- Aborted mKCP listener tasks on drop.

## 0.1.0-rc.21 - 2026-06-22

### Fixed

- Aborted QUIC listener tasks during shutdown.

## 0.1.0-rc.20 - 2026-06-22

### Fixed

- Tried all resolved Freedom outbound addresses instead of stopping on the first
  failed address.

## 0.1.0-rc.19 - 2026-06-22

### Added

- Added adaptive Hysteria2 tuning.

## 0.1.0-rc.18 - 2026-06-21

### Changed

- Batched live traffic counter updates.

## 0.1.0-rc.17 - 2026-06-21

### Fixed

- Accounted TCP traffic during active relays.

## 0.1.0-rc.16 - 2026-06-21

### Fixed

- Skipped empty managed client inbounds.

## 0.1.0-rc.15 - 2026-06-21

### Fixed

- Allowed Black UI to persist managed config.

## 0.1.0-rc.14 - 2026-06-21

### Fixed

- Cleared stale managed clients from generated config.

## 0.1.0-rc.13 - 2026-06-21

### Fixed

- Reconciled quota config during enforcement.

## 0.1.0-rc.12 - 2026-06-21

### Fixed

- Fixed quota accounting across runtime restarts.

## 0.1.0-rc.11 - 2026-06-21

### Fixed

- Made the Hysteria2 insecure share flag conditional.

## 0.1.0-rc.10 - 2026-06-21

### Fixed

- Fixed the Hysteria2 subscription TLS flag.

## 0.1.0-rc.9 - 2026-06-21

### Added

- Covered UDP and user stats across protocols.

## 0.1.0-rc.8 - 2026-06-21

### Fixed

- Fixed Hysteria2 stats attribution.

## 0.1.0-rc.7 - 2026-06-21

### Fixed

- Fixed IPv6 socket address handling.

## 0.1.0-rc.6 - 2026-06-19

### Added

- TUIC v5 experimental support with TCP proxying, native UDP relay coverage, Black UI config support, and an external-client lab row.
- Hysteria2 UDP relay datagram throughput benchmark coverage.
- Linux arm64 package smoke CI for Raspberry Pi 5-class native arm64 builds and Debian package inspection.
- Beginner proxy protocol guide and consolidated operator/user documentation.

### Changed

- Release builds now enable thin LTO, a single codegen unit, and debug-info
  stripping for the whole workspace, trimming the proxy's CPU-bound hot paths
  (crypto, framing, routing) without changing runtime behaviour. Per-connection
  task isolation is preserved by keeping `panic = "unwind"`.
- Hysteria2 UDP relay reuses a per-connection buffer pool for upstream reply
  buffers instead of allocating a fresh 64 KiB buffer per relayed datagram,
  reducing allocator pressure on high-rate UDP workloads.
- Hysteria2 UDP relay now copies upstream replies back through one persistent
  reader task per session instead of spawning a Tokio task per datagram. The
  common send path runs inline, eliminating a per-datagram task spawn and its
  scheduling/allocation overhead; reader tasks are bounded by the per-connection
  session cap and torn down on idle eviction or connection close. The rare
  fast-DNS-retry priority path keeps its isolated one-shot socket semantics.
- Cross-platform CI names architecture coverage explicitly and includes Windows arm64.
- Container image publishing was removed from CI; release publishing remains focused on GitHub archives, Debian packages, and the apt repository.
- Benchmark and performance workflows were repaired to tolerate current Criterion paths and baseline variants.

### Fixed

- Hysteria2 UDP e2e tests now reserve UDP ports to reduce bind races.
- TUIC and benchmark documentation now satisfy strict rustdoc/clippy CI.
- Nginx memory latency parsing and benchmark gate path handling were repaired.

### Validation

- Current `main` includes native Linux arm64 workspace tests and Debian arm64 package smoke coverage.
- Release asset workflow remains tag-driven and builds Linux x86_64, Linux arm64, macOS, Windows, Debian packages, and Black UI Linux assets.

## 0.1.0-rc.5 - 2026-06-07

### Added

- Aggregate Black UI QA command covering the smoke flow, structured inbound matrix, structured outbound matrix, and advanced config panel checks.
- Competitive and latency benchmark harnesses for relay, Fast Profile, Hysteria2 bad-network behavior, QUIC/datagram/FEC, TUN, and memory/CPU profiling.
- Connection manager, runtime stats, data-plane planning, AF_XDP scaffolding, and expanded metrics coverage.
- InnerFlow, QUIC bad-network controls, Hysteria2 datagram/FEC work, and expanded TUN packet/session/runtime paths.
- Release-facing performance evidence, license policy, third-party reference docs, and Black UI panel QA reports.

### Changed

- Fast-path relay, stream, WebSocket, gRPC, VMess, SS2022, mKCP, ShadowTLS, router, DNS, and TUN paths received allocation, copy, buffering, and hot-path CPU reductions.
- Black UI gained structured inbound, outbound, user, and advanced config editors with broader validation and preservation of unknown JSON keys.
- CI now cancels in-progress runs on newer pushes and includes stricter source-policy and documentation checks.
- Xray interop configs now use current `allowInsecure` handling.

### Fixed

- QA flow now rebuilds current frontend assets before exercising the panel and matches the current structured UI controls.
- Strict rustdoc/clippy gates were satisfied with missing docs, broken link, formatting, and test-helper fixes.
- TLS certificate generation now happens before rendering Xray client configs.
- Multiple audit findings around leakage, parser handling, body caps, and generated config safety were addressed.

### Validation

- `npm run qa` passed for Black UI aggregate coverage during rc.5 preparation.
- Inbound structured panel QA passed VLESS, VMess, Trojan, Shadowsocks, Hysteria2, WS, gRPC, HTTPUpgrade, SplitHTTP, KCP, QUIC, TLS, sniffing/limits, delete, and edit/toggle cases.
- Main was fast-forwarded to `origin/main` before release preparation.

## 0.1.0-rc.4 - 2026-06-01

### Added

- Adaptive balancer mode with in-memory profile scoring, conservative cooldowns, health-aware selection, runtime stats, Prometheus metrics, docs, examples, and focused tests.
- Black UI setting for auto adaptive routing across enabled outbounds, with backend-generated adaptive routing when two or more valid enabled outbounds exist.
- Black UI subscription URL generation using configured public base URL / subscription host, so generated links do not default to localhost on VPS deployments.
- VLESS REALITY share-link export with public keys, short IDs, SNI, fingerprint, and Hiddify-compatible query parameters.
- Optional firewall sync for enabled public panel-managed inbounds.

### Changed

- Black UI outbound validation now rejects enabled incomplete proxy outbounds before live apply, while allowing disabled draft outbounds to remain saved.
- Release docs now state the project-level pre-1.0 status more explicitly: many paths are tested and stable-looking, but the whole project is not production-ready yet.

### Fixed

- Live apply no longer rebuilds into invalid enabled Hysteria2, VLESS, VMess, Trojan, or Shadowsocks outbounds with missing required settings.
- Subscription buttons and share links use the configured public host instead of `127.0.0.1` when deployed on a VPS.
- REALITY client links no longer emit private-key material as the client public key parameter.

### Validation

- Focused balancer, backend config, Black UI server, and frontend build checks passed during the rc.4 preparation cycle.

## 0.1.0-rc.3 - 2026-05-31

### Added

- Linux, macOS, and Windows TUN runtime support with focused privileged smoke coverage.
- Handler API structural operations using native blackwire endpoint JSON with CLI-driven instance rebuild and rollback.
- Fast Profile (`profile = "fast"` / `--profile fast`) for a narrower latency-first production path.
- External-client matrix coverage driven by `labs/realistic/external-clients/scenarios.env`.
- SplitHTTP packet-up, VLESS Vision, VLESS Mux/XUDP, Trojan UDP, SS2022 UDP, Hysteria2 TCP/UDP, QUIC, ShadowTLS v3 transport, and mKCP server-path coverage.
- Docs ownership map so release status, feature evidence, test tiers, and lab details have clear sources of truth.
- Release asset workflow for Linux, Linux arm64, macOS, and Windows binaries with SHA256 files.
- GHCR image publishing for Linux amd64/arm64 release tags, with rc tags kept separate from `latest`.
- Linux install script for GitHub Release assets with checksum verification and optional systemd unit installation.
- Installer support for `CONFIG_PATH` / `CONFIG_URL` with config validation before service start.
- Linux VPS bootstrap options for generated VLESS TCP / VLESS REALITY configs, firewall guidance, upgrade, and uninstall.
- Linux domain TLS bootstrap using generated Trojan TLS config with certbot or existing certificate paths.
- Standard nginx domain setup mode (`SETUP=domain`) with HTTPS termination and localhost WebSocket reverse proxy.
- Installed command guide for service control, uninstall, config edits, logs, and examples.
- Debian package release assets for Linux amd64 and arm64.

### Changed

- README now acts as an entry point instead of duplicating the full release contract.
- Release/status docs now describe matrix SKIPs as upstream client-model limits where applicable, not automatic unsupported server paths.
- Testing docs now use `scenarios.env` as the source of truth instead of hard-coded matrix row/PASS/SKIP counts.
- Fast Profile keeps safety checks identical to compatibility mode while rejecting high-complexity hot-path features.
- Removed unused workspace dependencies from several crates.

### Experimental

- Stats API (gRPC) exposes runtime stats, but remains experimental until soak and observability validation are complete.
- Kernel TLS (`SO_KTLS`) remains isolated and opt-in.

### Unsupported

- V2Ray/Xray JSON config import.
- VMess legacy alterId / non-AEAD.
- Xray core endpoint protobuf decoding for Handler structural RPCs.
- DNS, dokodemo, or tun as inbound `protocol` values.
- Byte-identical browser TLS fingerprinting.
- OpenWrt, Android, iOS, and standalone desktop/mobile client app packaging.

### Validation

- Local markdown link check passes across repository docs.
- Documentation stale-count/status searches are clean.
- `cargo check --workspace --all-targets --locked` and
  `cargo clippy --workspace --all-targets -- -D warnings` passed after the cleanup pass.
