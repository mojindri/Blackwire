# Readiness

These checks provide repeatable evidence for stability, security, stress,
parser robustness, DNS behavior, fingerprints, and deployment assumptions. They
do not prove production safety.

## Gates

| Gate | Purpose | Command |
| --- | --- | --- |
| Host Rust quality | fmt, check, clippy, tests | `make verify-local` |
| Lab realism | Docker + Lima + external-client checks | `make verify-lab` |
| VPS matrix | public-network protocol coverage | `make verify-remote` |
| Load | high-concurrency pressure | `make -C labs/realistic load` |
| Soak | leak/degradation over time | `make soak` |
| Fuzz smoke | parser crash discovery | `make fuzz-smoke` |
| Fuzz long | heavier parser campaigns | `make fuzz-long` |
| Fingerprint | TLS/REALITY ClientHello capture | `make verify-lab-lima` |
| DNS chaos | DNS/FakeIP edge cases | `make -C labs/realistic dns-chaos` |
| Security hygiene | audit, deny, secrets, unsafe scan | `make security` |
| Real devices | manual client checklist template | `make -C labs/realistic real-devices` |

Recommended order:

1. `make verify-local`
2. `make verify-lab`
3. `SSH_SERVER=… SSH_CLIENT=… make verify-remote`
4. `make security`
5. `make fuzz-smoke`
6. load/soak/fingerprint/DNS chaos as risk requires
7. `make -C labs/realistic real-devices`

## Pass/Fail Policy

- Load: at least 99% success for the configured run.
- Soak: no monotonic RSS/fd growth; no reconnect storms; no silent protocol death.
- Fuzz: zero crashes, timeouts, or OOMs.
- Fingerprint: known and reviewed ClientHello differences only.
- DNS/FakeIP: deterministic behavior for NXDOMAIN, SERVFAIL, IPv4/IPv6, stale mappings, reload.
- Security: no secrets in repo; no unreviewed critical advisories; `unsafe` justified.

## Real Device Checklist

CI cannot replace real devices. Carrier NAT, mobile radio sleep/wake,
client-app behavior, and OS proxy/VPN APIs produce failures that Docker and VPS
tests will not show.

Minimum matrix:

| Device | Network | Client path | Required protocols |
| --- | --- | --- | --- |
| Android | Mobile data | Termux/curl or Android proxy client | VLESS REALITY, Trojan TLS, SS2022 |
| iPhone | Mobile data | iOS proxy/VPN client | VLESS/Trojan where supported |
| Laptop | Phone tether | browser + curl through SOCKS | VLESS TCP/WS, VMess gRPC |
| Windows | Home ISP | v2rayN/sing-box | Xray/sing-box interop |
| Linux | Home ISP | curl/sing-box | all supported paths |

Checks:

1. Confirm direct public IP.
2. Connect through proxy.
3. Confirm proxied public IP.
4. Fetch HTTP and HTTPS endpoints.
5. Run a short download.
6. Toggle network and confirm reconnect.
7. Reload config and confirm existing/new connections behave as expected.
8. Try wrong credentials and confirm rejection.
9. Review logs for secret leakage.

Generate a report template:

```sh
make -C labs/realistic real-devices
```

## Security Checklist

Attack surface:

- inbound protocol parsers
- outbound protocol builders
- TLS/REALITY handshake code
- DNS and FakeIP state
- config loader and hot reload
- TUN/privileged Linux, macOS, and Windows paths
- metrics/admin API
- logs and reports
- Docker/systemd deployment files

Questions that must have answers:

- Can unauthenticated input allocate unbounded memory?
- Can malformed input panic, loop forever, or stall a Tokio task?
- Can auth be bypassed through partial reads, parser desync, or fallback confusion?
- Are secrets redacted from logs and reports?
- Are insecure config modes explicitly visible?
- Does config reload avoid mixed old/new routing state?
- Can stale FakeIP mappings misroute traffic?
- Does DNS failure behave deterministically?
- Does TUN setup clean up routes/interfaces after failure?
- Are TLS verification defaults safe?
- Are unsafe blocks documented and justified?
- Are dependency advisories tracked?

Security commands:

```sh
make security
make fuzz-smoke
make verify-local
cargo install cargo-audit cargo-deny cargo-fuzz
make fuzz-long
```

Do not paste raw proxy logs or lab reports that may contain UUIDs, passwords,
private keys, or REALITY key material. `REALITY_DEBUG_HELLO=1` enables extra
handshake fields in debug logs; use it only in controlled environments.

## Limitation

Passing these gates does not prove censorship resistance, cryptographic
correctness, or complete production safety. They provide evidence, not a
production guarantee.

Related:

- [test-workflows.md](test-workflows.md)
- [11-testing.md](11-testing.md)
- [commands.md](commands.md)
