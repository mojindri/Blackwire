# Configuration For Dummies

Blackwire does not use deployable JSON configuration files. You configure it in
Black UI or through its database-backed CLI; MySQL stores each typed change as
an immutable revision. The runtime reconstructs and validates that revision
before it can run.

This guide explains the concepts you see in Black UI. It is deliberately about
what you are configuring, not a file format to copy into a server.

## Start With The Five Questions

Before you add anything, decide:

1. Which port should accept your clients?
2. Which protocol should that inbound speak?
3. Where should the traffic leave from?
4. Which routing rules choose that outbound?
5. Do you need a transport or security layer such as TLS, REALITY, WebSocket,
   or QUIC?

For your first setup, keep the answers boring: one local SOCKS inbound, one
direct outbound, and one rule that sends everything to it.

## Your First Safe Setup

An empty inbound list is safe and valid: no proxy port is exposed until you add
an inbound. In Black UI, create the following:

| Section | Add | Suggested values |
| --- | --- | --- |
| Inbounds | Local SOCKS listener | `127.0.0.1`, port `1080`, protocol `socks` |
| Outbounds | Direct path | protocol `freedom` |
| Routing | Default rule | choose the direct outbound |

Save the revision, then look at Runtime. Blackwire tells you whether it can
activate immediately or needs confirmed maintenance activation.

For a disposable local starting point, you can also create a relational preset:

```sh
blackwire db seed socks-local
```

The other presets are `vless-local`, `trojan-local`, and `shadowsocks-local`.
They create relational state and, where needed, a user credential. They are not
JSON imports and should not be treated as a production migration tool.

## The Main Sections

## Inbounds: What You Accept

An inbound is a listener. It needs a tag, listening address, port, and
protocol. You may also select transport, security, users, limits, and
sniffing.

- Bind local client proxies to `127.0.0.1` unless you intentionally need LAN
  access.
- Bind a public server inbound only after you have chosen authentication and a
  suitable security/transport design.
- Keep tags descriptive, such as `socks-local` or `vless-reality-in`.

Common choices:

| You want to… | Inbound protocol |
| --- | --- |
| Proxy applications on this machine | SOCKS or HTTP CONNECT |
| Accept a VLESS client | VLESS |
| Accept a Trojan client | Trojan |
| Accept a Shadowsocks 2022 client | Shadowsocks |

## Outbounds: Where Traffic Goes

An outbound is the path Blackwire takes after routing selects it.

- Use `freedom` for a direct connection to the requested destination.
- Use VLESS, Trojan, VMess, Shadowsocks, Hysteria2, or TUIC when this instance
  must act as a client to another proxy server.
- Give each path a clear tag, because routing refers to it by name.

## Routing: Choosing An Outbound

Routing rules match traffic and choose an outbound tag. You can match by
domain, IP, port, source, or inbound. A sensible default rule is important:
without a matching path, traffic cannot leave the runtime.

Start with one direct default rule. Add domain and DNS policies only when you
can describe the outcome you want, for example “send this domain group through
my remote outbound.”

## Transport And Security

Transport controls how the proxy connection moves; security controls how it is
protected or disguised. They are related but not interchangeable.

| Item | What it changes |
| --- | --- |
| TCP | The normal stream transport and the simplest starting point. |
| TLS | Encrypts and authenticates a TLS-backed path. |
| REALITY | Authenticated TLS camouflage for supported VLESS TCP paths. |
| WebSocket / gRPC / HTTPUpgrade / SplitHTTP | HTTP-shaped choices with interoperability trade-offs. |
| QUIC, Hysteria2, TUIC | UDP/QUIC paths that depend on firewall, NAT, MTU, and client support. |

Use the [Feature Matrix](feature-matrix.md) before selecting an advanced path.
It is the source of truth for supported clients, known caveats, and test
evidence.

## Users And Client Subscriptions

Users belong to an inbound. In Black UI you can create or disable users, set
credentials, expiry, and quota, then use **Copy subscription** for
database-derived client content that Hiddify can scan or import.

This is not a server-configuration export. Blackwire does not accept or emit
Xray, sing-box, or Blackwire server configuration files.

## Runtime And Settings

The Settings page controls shared runtime behavior: profile, logs, metrics,
API, limits, DNS, routing, and optional performance policies. Start with the
`compat` profile and defaults. Turn on Fast-path tuning only when you have a
measured reason and understand its compatibility constraints.

See [Fast Profile](fast-profile.md) for the constraints and safe operating
rules.

## How Changes Become Active

Every completed UI or CLI edit creates a new revision. Blackwire validates the
desired revision and either activates it immediately, hands over a supported
listener, or holds it for confirmed maintenance activation. During a temporary
MySQL outage, it keeps serving the active in-memory revision.

Useful recovery commands:

```sh
blackwire db validate
blackwire db status
blackwire db history --limit 20
blackwire db rollback REVISION
blackwire db activate-maintenance REVISION
```

Revision history is configuration rollback, not a database backup. Protect
your MySQL service with dumps, snapshots, and binlogs.

## Common Mistakes

- Do not create a legacy JSON configuration file; it is not part of the
  supported runtime workflow.
- Do not expose a local SOCKS or HTTP listener publicly.
- Do not use an unencrypted public inbound.
- Do not assume an Xray or sing-box feature is supported because its name
  appears in another project; check the Feature Matrix.
- Do not use lab fixtures or `blackwire db import-fixture` against production
  data.

## Where To Go Next

- [User Guide](user-guide.md) — install and operate the service.
- [Feature Matrix](feature-matrix.md) — support evidence and caveats.
- [REALITY For Dummies](04-reality-for-dummies.md) — the REALITY handshake and
  terminology.
