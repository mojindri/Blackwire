# Roadmap

This file collects backlog items that were previously split across traffic
policy, network resilience, and Xray/sing-box parity roadmap docs.

Roadmap items are not release promises. A feature is not supported until it is
implemented, tested, and promoted through [feature-matrix.md](feature-matrix.md)
and [release.md](release.md).

## Traffic Policy

Goal: abuse-prevention controls around public proxy/VPN experiments.

P0:

- Add `bittorrent` as a first-class sniffed protocol value.
- Add TCP BitTorrent handshake sniffing.
- Preserve sniffed protocol metadata in dispatcher/router.
- Ensure routing can match `protocol: ["bittorrent"]`.
- Add or document a block outbound.
- Add config example and tests.

P1:

- Add Black UI toggle for torrent/P2P abuse blocking.
- Avoid duplicate generated rules.
- Show recent policy-blocked events.
- Remove only panel-managed rules when disabled.

Later:

- UDP allow/deny policy.
- Conservative uTP sniffing.
- Per-user and per-inbound connection/rate limits.
- Quotas and speed caps.
- Optional tracker/domain blocklists.

Non-goals:

- perfect DPI
- legal enforcement inside core runtime
- detection of traffic hidden inside other encrypted tunnels

## Network Resilience

Goal: practical hardening for difficult networks without promising unblockability.

P0:

- REALITY deployment checks for key shape, short ID, `serverName`, `dest`, and fingerprint.
- External operator test for valid and invalid auth behavior.
- Panel warnings for weak fallback/cover choices.

P1:

- DNS bootstrap hardening for DoH/DoT.
- DNS-over-proxy option for client-side modes.
- DNS/FakeIP presets in Black UI.
- No-leakage checklist for server, local proxy, and TUN modes.
- Tests for DNS route, blocked route, and log-redaction behavior.

P2+:

- Multi-path panel workflow: REALITY primary, WS/gRPC/SplitHTTP fallback,
  optional Hysteria2.
- Adaptive balancer profiles from enabled paths.
- Transport diversity presets.
- Active probing resistance checks.
- Fingerprint and timing hardening.
- IP/domain reachability and rotation operations.
- Public panel hardening.

Non-goals:

- state-level blocking guarantees
- domain-fronting claims without explicit supported CDN/path evidence
- byte-identical browser TLS fingerprinting unless independently verified
- automatic evasion of provider abuse rules or local law

## Xray / Sing-Box Wire Parity

Strict rule: in-tree or uncommitted work does not move a feature to Supported
without external-client matrix proof, unless it is documented as an intentional
deviation.

Done/matrix-proven highlights:

- Trojan UDP ASSOCIATE
- VLESS Mux.Cool TCP
- VLESS XUDP
- SplitHTTP stream-one
- SplitHTTP packet-up
- SS2022 UDP
- Trojan/VLESS UDP outbound
- Health-check outbound failover
- QUIC server with documented Xray SKIP / sing-box PASS
- XTLS Vision
- Hot-reload routing/users
- Handler gRPC structural operations

Backlog:

- Kernel TLS (`SO_KTLS`) remains experimental and opt-in.
- In-place Handler listener RPCs remain backlog.

Rust competitor parity backlog:

- TUIC v5
- NaiveProxy
- Classic Shadowsocks AEAD
- Snell v3
- Mixed HTTP/SOCKS inbound
- General H2MUX
- AnyTLS
- SagerNet UDP-over-TCP
- Shadowsocks SIP003 WebSocket plugin-style compatibility
- Android/iOS TUN/VPN runtimes

Priority rule: do not chase protocol count at the cost of external-client
proof, fail-closed validation, no-leakage gates, operator docs, Black UI
workflows, and reproducible release evidence.

Related:

- [xray-parity-source-of-truth.md](xray-parity-source-of-truth.md)
- [parity-status.md](parity-status.md)
- [feature-matrix.md](feature-matrix.md)
