# Changelog

All notable release-facing changes are documented here.

This project is pre-1.0. The support contract is owned by
[docs/release.md](docs/release.md), and detailed feature evidence is owned by
[docs/feature-matrix.md](docs/feature-matrix.md).

## Unreleased

### Removed

- Removed the deprecated mKCP transport from the runtime, configuration schema,
  Black UI, persistence layer, test labs, and capability reporting. Existing
  configurations that select `network: "kcp"` are rejected instead of being
  silently converted to another transport.

### Changed

- Moved TUN device capture, protected egress, and FakeIP configuration out of
  the server runtime and Black UI into the dedicated `blackwire-client`
  application. Shared protocol and platform implementations remain available;
  only product ownership and configuration moved. Existing database values are
  retained in archival tables for manual migration or rollback.
- Removed the unused TUN-over-datagram toggle and the obsolete FakeIP budget
  override from the shared schema, server settings API, and database.

## 0.2.5 - 2026-08-19

### Fixed

- VLESS Reality Vision now remains eligible for authenticated direct-copy
  handoff after traffic sniffing replays an already-unpadded prefix. This
  prevents raw destination TLS records from being misread as outer Reality TLS
  records when sniffing is enabled.
- Server-side Vision now completes the authenticated handoff in both
  directions before removing outer Reality TLS, matching Xray and sing-box
  clients and preventing intermittent TLS `bad record MAC` disconnects.
- Vision preserves plaintext buffered at the transition boundary and reliably
  recovers nested stream adapters on Linux without adding work to the steady
  state relay hot path.

## 0.2.4 - 2026-08-19

### Fixed

- VLESS Reality Vision now performs Xray-compatible direct-copy handoff as
  soon as the authenticated Vision command arrives, removing the outer TLS
  record layer before raw destination TLS records are relayed on every
  platform. Splice remains Linux- and policy-controlled, ordinary non-Vision
  REALITY streams cannot trigger the handoff, and buffered plaintext is
  preserved in wire order.
- Targeted debug logging now correlates Reality authentication and TLS stages,
  sniffed destination rewrites, outbound selection, relay connection IDs,
  duration, byte counts, and close errors without exposing credentials or
  traffic payloads.
- Black UI subscription actions once again fetch and copy or encode the actual
  base64 subscription content. The configured public URL is used only to fetch
  that content, so Hiddify receives an importable payload without a loopback
  panel address.

## 0.2.3 - 2026-08-19

### Fixed

- Black UI subscription actions now copy and encode the public subscription
  URL for Hiddify instead of exporting the fetched base64 response body as if
  it were an import URL.
- Black UI now honors `BLACK_UI_PUBLIC_BASE_URL` and
  `BLACK_UI_SUBSCRIPTION_HOST` as runtime overrides, preventing stale loopback
  values in MySQL from reappearing in subscription URLs after an upgrade.
- Native and Docker installers now carry the public URL settings consistently;
  public native installs require both values explicitly.
- Black UI browser QA now follows the current subscription URL controls, so
  its end-to-end check covers the public URL copy and QR flow again.

## 0.2.2 - 2026-08-18

### Fixed

- Freedom TCP dialing now applies bounded RFC 8305-style Happy Eyeballs across
  every resolved address, interleaves IPv4 and IPv6 candidates, staggers new
  attempts by 250 ms, and caps concurrent dials at four. This avoids waiting
  serially when the first addresses for a domain are unreachable while keeping
  the established connection hot path unchanged.

## 0.2.1 - 2026-08-18

### Fixed

- Black UI now has a minimal unauthenticated bootstrap-status endpoint, so an
  existing panel shows an enabled login form instead of being stuck in
  “Checking panel”. The frontend remains compatible with v0.2.0 servers.
- The native installer now completes temporary-file cleanup before its strict
  shell exit trap runs.

### Changed

- Native Black UI installation now uses the explicit
  `BLACK_UI_EXPOSURE=private|public` setting; private loopback binding remains
  the default and public binding uses `0.0.0.0` unless overridden.

## 0.2.0 - 2026-08-18

### Added

- MySQL 8.4/InnoDB is now the sole persistent control plane for the runtime,
  CLI, and Black UI, with explicit schema migrations and immutable revisions.
- Black UI now provides typed control surfaces for users, endpoints, routing,
  DNS, runtime activation, performance settings, and contextual field help.
- Docker and native deployment flows now support separate least-privilege
  runtime, UI, and migrator database credentials.

### Changed

- Runtime configuration files, SQLite persistence, raw server configuration
  editing, and deployable JSON examples were removed.
- Runtime activation now supports automatic hot swap, prepared in-process
  handover, rollback, last-known-good retention, and database-outage reconciliation.

### Fixed

- Hiddify-compatible subscription exports now cover VLESS/REALITY transports,
  VMess, Trojan, Shadowsocks 2022, Hysteria2, and TUIC with canonical URL
  encoding, SNI parameters, IPv6 authorities, and base64 subscription content.
- Fast deterministic subscription tests now validate every supported exported
  parameter, decoded VMess payloads, URL-safe SIP002 credentials, TLS/insecure
  modes, and unsupported-protocol rejection without network or Docker startup.
- VPS bootstrap scripts now install and verify their actual dependencies,
  populate TUIC TLS names, discover Caddy certificates across issuer storage
  layouts, restart only real services after renewal, expand local fixtures from
  the selected environment file, ship an executable APT publisher, and document
  exact ports.
- External-client Docker scenarios now bootstrap fresh relational MySQL
  revisions instead of invoking the removed file-config runtime interface.
- REALITY and Vision Docker interoperability scenarios now use the lab's live
  TLS cover and keep Blackwire, Xray, sing-box, and Hiddify cover names aligned.
- Realistic, latency, competitive, and VPS lab runners now import fixtures into
  disposable MySQL databases, preserve fixture outbound order, and start the
  normal database-backed runtime.
- Inbound protocol network mode and authentication timeout now survive MySQL
  revision persistence, restoring UDP-only Shadowsocks 2022 listeners.
- Structured inbound and outbound editors preserve complete transport and
  security settings, including SplitHTTP option groups and outbound TLS
  certificate verification policy.
- Black UI forms, Routing & DNS, responsive layouts, and strict Rust 1.97
  lint compatibility were brought into alignment with the current core.

## 0.1.40 - 2026-06-28

### Fixed

- Hysteria2 now treats clean peer QUIC close code `0` as normal session drain,
  logging the active stream count without bubbling the accept-loop close as a
  connection task error.

## 0.1.39 - 2026-06-28

### Fixed

- Hysteria2 now treats Blackwire's synthetic unbounded bandwidth value as a
  QUIC window-sizing hint instead of a fixed-rate Brutal congestion target.
- Hysteria2 auth no longer advertises fake high `hysteria-cc-rx` values when no
  explicit bandwidth cap is configured, improving stability with Hiddify/sing-box
  during interrupted browser speed tests.

## 0.1.38 - 2026-06-28

### Added

- Hysteria2 now logs structured QUIC close diagnostics, including close kind,
  error code, frame type, reason text, and where the close was observed.

## 0.1.36 - 2026-06-28

### Fixed

- Hysteria2 now aligns its per-connection stream guard and QUIC idle cleanup
  with the reference Hysteria2 behavior: up to 1024 incoming TCP streams per
  QUIC connection and 30-second idle session cleanup, with summary counters for
  active stream peaks and backpressure.

## 0.1.34 - 2026-06-28

### Fixed

- Hysteria2 no longer advertises UDP datagram relay by default when no explicit
  datagram policy is configured, matching Black UI defaults and avoiding
  unstable client UDP/TUN sessions until stateful UDP association relay is
  promoted.

## 0.1.33 - 2026-06-28

### Fixed

- Hysteria2 UDP relay now uses the official `host:port` QUIC datagram address
  format expected by Hiddify/sing-box clients, while still accepting older
  Blackwire compact datagrams for backward compatibility.

## 0.1.32 - 2026-06-28

### Added

- Hysteria2 server diagnostics now record QUIC connection lifecycle,
  authentication outcomes, TCP stream results, UDP datagram scheduler failures,
  and per-connection debug summaries without logging auth secrets.
- Hysteria2 server metrics now expose connection, TCP stream, and UDP event
  counters labeled by inbound and result/event for production debugging.

## 0.1.28 - 2026-06-27

### Added

- Freedom outbounds now support `domainStrategy` / `ipStrategy` values
  `Auto`, `UseIP`, `PreferIPv4`, `PreferIPv6`, `UseIPv4`, and `UseIPv6`, plus
  `rejectIpv6Literal` for VPSes with broken outbound IPv6 paths.
- Black UI now exposes Freedom IP strategy as a dropdown and the IPv6 literal
  guard as a toggle in the outbound editor.

## 0.1.27 - 2026-06-27

### Fixed

- Black UI save actions now use the latest field values in Inbounds, Outbounds,
  Users, Settings, and Advanced Config, preventing quick Save clicks from
  dropping recently changed structured fields such as Hysteria2 performance
  mode.

## 0.1.26 - 2026-06-27

### Added

- Black UI inbound and outbound editors now expose Hysteria2 auth and a simple
  performance-mode selector, with lower-level congestion, QUIC socket, datagram,
  and FEC fields shown only for custom tuning instead of requiring manual
  Settings JSON edits.

## 0.1.25 - 2026-06-27

### Changed

- Bandwidth shaping and adaptive Mbps tuning are disabled: Black UI no longer
  exposes per-user Mbps fields or adaptive Hysteria2 tuning controls, the
  adaptive tuning worker is hard-gated off, and generated Hysteria2 configs no
  longer include default `upMbps` / `downMbps` values.
- Legacy per-user `upMbps` / `downMbps` fields are ignored by the runtime and
  dropped by Black UI when user credentials are saved.
- Release metadata and install examples now reference `v0.1.25`.

## 0.1.24 - 2026-06-27

### Fixed

- Black UI now rejects enabled shareable proxy inbounds bound to loopback
  addresses, preventing public subscription links from pointing at listeners
  that only accept local connections.
- Subscription link generation now replaces a stale local subscription host with
  the current public request host when the panel is accessed over a non-local
  address.

### Changed

- Release metadata and install examples now reference `v0.1.24`.

## 0.1.23 - 2026-06-27

### Added

- Installer `INIT_SERVER=hysteria2` now generates a TLS Hysteria2 server config
  with top-level `settings.auth`, client hints, and UDP firewall guidance.

### Fixed

- Installer-generated Trojan TLS and Hysteria2 configs now stage certificate
  and key files under `/etc/blackwire/certs` with service-readable permissions
  before config validation and service restart.
- Black UI generated Hysteria2 configs now promote the first active user's auth
  secret into top-level `settings.auth`, matching the runtime requirement.

### Changed

- Release metadata and install examples now reference `v0.1.23`.

## 0.1.22 - 2026-06-27

### Fixed

- Black UI startup now migrates localhost/default public link settings to
  `BLACK_UI_PUBLIC_BASE_URL` and `BLACK_UI_SUBSCRIPTION_HOST` when those
  environment values are provided, while preserving explicit custom settings.
- Black UI startup now migrates stale QA/temp config paths back to the packaged
  `BLACK_UI_CONFIG_PATH`, and config writes now report clear permission/read-only
  path hints.
- Black UI now validates duplicate inbound tags before insert/update so users
  see a clear validation error instead of a raw persistence constraint failure.

### Changed

- Release metadata and install examples now reference `v0.1.22`.

## 0.1.21 - 2026-06-27

### Added

- Black UI TLS inbound editing can now generate self-signed certificate/key
  files on the server and apply the generated paths to the inbound form.

### Changed

- Black UI capabilities now classify V2Ray QUIC transport as supported, with
  notes about sing-box coverage and the Xray 26+ legacy-client skip.
- Installer and Debian package hardening keep `/etc/blackwire/certs`
  group-writable for Black UI while preserving `0640` certificate/key files.
- Release metadata and install examples now reference `v0.1.21`.

## 0.1.20 - 2026-06-27

### Fixed

- Black UI subscription copy buttons now copy the base64 subscription body from
  `/sub/{token}` instead of the raw single-link URI from `/sub/{token}/raw`.

### Changed

- Release metadata and install examples now reference `v0.1.20`.

## 0.1.19 - 2026-06-27

### Fixed

- Packaged Black UI installs now use `/etc/blackwire/config.json` by default,
  preventing the panel from saving a different config than the running service.
- Black UI live user removal now treats an already-absent runtime user as a
  successful idempotent remove instead of surfacing a false failure.
- REALITY inbound validation now rejects missing or non-socket
  `realitySettings.dest` before runtime restart.
- The structured REALITY editor now exposes fallback destination and carries it
  through server-value loading and QA fixtures.

### Changed

- Release metadata and install examples now reference `v0.1.19`.

## 0.1.18 - 2026-06-27

### Added

- Black UI now exposes server-side REALITY value generation/loading so generated
  VLESS/REALITY links use matching private key, public key, short ID, and server
  name values.
- Structured inbound QA now covers TLS certificate/key validation and
  VLESS/REALITY TCP persistence.

### Fixed

- TLS structured inbound validation now blocks certificate-only or key-only
  configurations before save.
- Structured inbound QA now matches the current UI surface: VMess/TCP no longer
  expects a removed encryption field, and unavailable KCP is reported as skipped
  instead of failed.

### Changed

- Release metadata and install examples now reference `v0.1.18`.

## 0.1.17 - 2026-06-27

### Changed

- Relay performance defaults were adjusted for better wrapped-transport
  throughput under load.
- Release metadata and install examples now reference `v0.1.17`.

## 0.1.16 - 2026-06-27

### Fixed

- Installer upgrades now re-own existing Black UI data files under
  the panel data directory when the service user/group changes, preventing local database
  `attempt to write a readonly database` crash loops after migration from older
  `nobody:nogroup` installs.

### Changed

- Release metadata and install examples now reference `v0.1.16`.

## 0.1.15 - 2026-06-27

### Fixed

- Freedom outbound DNS resolution now explicitly queries A and AAAA records with
  IPv4-first ordering, reducing tail latency when VPS IPv6 routes or destination
  AAAA records are flaky.
- Freedom outbound now tries IPv4 resolved addresses before IPv6 addresses while
  keeping IPv6 available as fallback.
- Installer systemd updates now restart already-running Blackwire and Black UI
  services after unit rewrites, so changed service users/groups take effect
  immediately.
- Installer now uses a dedicated `blackwire` system user by default instead of
  `nobody`, avoiding fragile runtime permissions and systemd safety warnings.

### Changed

- Release metadata and install examples now reference `v0.1.15`.

## 0.1.14 - 2026-06-27

### Added

- REALITY server inbounds now support explicit `serverNames` / `server_names`
  SNI allow-lists and reject wildcard or missing public SNI configuration.
- REALITY config now supports explicit `maxTimeDiffSeconds` /
  `max_time_diff_seconds` to avoid Xray-style millisecond/second ambiguity.
- Added an offline ClientHello fingerprint summary tool for lab captures and
  fixtures.
- Added static installer hardening assertions for config permissions and
  generated REALITY defaults.

### Changed

- VLESS inbound config validation now fails closed for misleading unsupported
  security-looking settings such as non-`none` `decryption`, Xray
  `settings.fallbacks`, and padding/encryption fields Blackwire does not
  implement.
- VLESS wrong UUID, malformed headers, and wrong flow now share the same
  fallback behavior when a fallback backend is configured.
- Installer-generated REALITY configs include conservative connection limits,
  per-inbound handshake limits, explicit REALITY `serverNames`, and neutral
  nginx fallback pages.

### Fixed

- Installer config permissions no longer fall back to world-readable
  `/etc/blackwire/config.json` when the service group is missing.
- Normal REALITY debug logs no longer include short IDs, auth-key prefixes, or
  session/random prefixes.
- Expired REALITY timestamps now have explicit fallback coverage.
- Black UI now emits REALITY `serverNames` and explicit
  `maxTimeDiffSeconds`, and subscription links use `serverNames` as the fallback
  source for `sni`.

### Changed

- Release metadata and install examples now reference `v0.1.14`.

## 0.1.13 - 2026-06-27

### Fixed

- Installer-generated public server configs now enable explicit DNS upstreams
  (`1.1.1.1`, `8.8.8.8`) so outbound `freedom` traffic does not inherit slow or
  broken VPS provider resolvers. This prevents client delay-test probes from
  timing out while real browsing still works.
- Black UI now enables the same DNS defaults for new databases and migrates only
  the old empty default DNS section, while preserving custom operator DNS
  settings.

### Changed

- Release metadata and install examples now reference `v0.1.13`.

## 0.1.12 - 2026-06-27

### Fixed

- Black UI subscription links now apply self-signed TLS verification bypass flags
  consistently across supported TLS-backed share formats. VMess TLS exports include
  `allowInsecure` when Blackwire-generated certificates require it, and TUIC exports
  now use the same self-signed certificate detection as VLESS, Trojan, and Hysteria2.
- Added coverage for the shared TLS share-link policy so Blackwire self-signed
  certificates, explicit insecure settings, Let’s Encrypt certificates, and custom
  public certificate paths stay distinct.

### Changed

- Release metadata and install examples now reference `v0.1.12`.

## 0.1.11 - 2026-06-27

### Fixed

- Prevented live config reload and gRPC runtime sync from rebuilding listener
  sockets inside the running process. Structural listener changes are now
  persisted but require a clean service restart, avoiding overlapping
  `SO_REUSEPORT` accept loops after UI/config writes.
- Fixed listener-change reporting so added inbounds are listed once and removed
  inbounds are reported by tag.

### Changed

- Release metadata and install examples now reference `v0.1.11`.

## 0.1.10 - 2026-06-27

### Changed

- Release metadata and install examples now reference `v0.1.10`.
- The release asset workflow now skips the optional GitHub Pages apt repository
  publish step when `BLACKWIRE_APT_SIGNING_KEY` is not configured, while still
  allowing signed apt publication when the secret exists.

## 0.1.9 - 2026-06-26

### Changed

- Black UI now reflects the current protocol and fast-profile surface: TUIC users
  are treated as structured credentials, per-user upload/download Mbps fields are
  editable, and Fast Profile relay engine/flush/buffer controls are exposed.
- Release metadata and install examples now reference `v0.1.9`.

### Fixed

- VMess inbound editing no longer shows a misleading body encryption field; the UI
  now documents the current AEAD-only behavior instead.
- Deprecated mKCP transport options stay hidden for new configs while legacy
  existing entries remain editable.

## 0.1.8 - 2026-06-26

### Changed

- Repository release metadata was aligned to `0.1.8`: workspace/version manifest,
  frontend version, and all user-facing install examples/release docs now reference
  `v0.1.8`.
- Release and docs consistency were verified during the version bump to keep installer
  and manual install command examples in sync across README and user documentation.

## 0.1.7 - 2026-06-26

### Changed

- Client compatibility warnings were added for QUIC/TUIC/Hysteria2 in the Black UI
  inbound editor to make unsupported or sensitive combinations visible before you save
  or test links.
- Subscription generation and copy now keeps compatibility labels for `experimental`
  and `client-sensitive` transport/protocol options, while continuing to preserve
  link content integrity.

### Fixed

- TUIC/VMess/VLESS UDP relay families now use destination-family-aware socket
  handling, so IPv6/IPv4 mixing is less likely to trigger OS family mismatch and
  connection failures under UDP-heavy conditions.
- V2Ray/Sing-box interoperability paths now prefer IPv4 on mixed DNS answers for
  UDP relay resolution in vless/udp and vless/mux, reducing resolution ambiguity
  for edge cases.

## 0.1.6 - 2026-06-26

### Fixed

- VLESS UDP and mux/XUDP relays now bind UDP sockets using the resolved
  upstream address family, avoiding `Address family not supported` failures
  when clients send IPv6 UDP destinations through VLESS/REALITY mux paths.
- VMess inbound now handles Mux.Cool and XUDP frames after authentication, so
  VMess QUIC links imported by Hiddify/sing-box with XUDP packet encoding can
  relay TCP mux substreams and UDP packets instead of failing with `unknown ATYP`.

## 0.1.5 - 2026-06-25

### Fixed

- Black UI subscription copy actions now fetch the current raw subscription
  endpoint (`/sub/{token}/raw`) before writing clipboard content, so copied
  content is the actual client link payload instead of the base64 subscription
  wrapper.
- Centralized Black UI subscription content copy behavior across the users table
  and user drawer, and disabled the table copy action when no subscription URL is
  available.

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
- Connection manager, runtime stats, data-plane planning, and expanded metrics coverage.
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
