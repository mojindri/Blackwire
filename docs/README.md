# Documentation

This folder has two layers:

- user/operator docs for installing and running Blackwire
- developer/evidence docs for support status, tests, parity, and internals

Keep long-lived facts in one place. Beginner pages should link to the release
contract and feature matrix instead of carrying their own support tables.

## Start Here

| Goal | Read |
| --- | --- |
| Install, configure, operate, troubleshoot, or use Black UI | [user-guide.md](user-guide.md) |
| Understand supported, experimental, or unsupported release paths | [release.md](release.md) |
| Check detailed feature status and evidence | [feature-matrix.md](feature-matrix.md) |
| Choose a validation command | [test-workflows.md](test-workflows.md), [commands.md](commands.md) |
| Understand the codebase | [00-project-map.md](00-project-map.md) |
| Review readiness/security/device gates | [readiness.md](readiness.md) |
| Review backlog and parity roadmap | [roadmap.md](roadmap.md) |

## Sources Of Truth

| Question | Canonical doc |
| --- | --- |
| What is supported, experimental, or unsupported for release? | [release.md](release.md) |
| What changed between release-facing snapshots? | [../CHANGELOG.md](../CHANGELOG.md) |
| What is the detailed feature status and evidence? | [feature-matrix.md](feature-matrix.md) |
| Which tests/gates should be run? | [11-testing.md](11-testing.md), [test-workflows.md](test-workflows.md) |
| Which commands should I use? | [commands.md](commands.md) |
| What does the external-client matrix prove? | [parity-status.md](parity-status.md), [../labs/realistic/external-clients/README.md](../labs/realistic/external-clients/README.md) |
| What local report results have been summarized for the repo? | [performance-evidence.md](performance-evidence.md) |
| What backlog remains? | [roadmap.md](roadmap.md) |

## User Docs

- [user-guide.md](user-guide.md)
  Install, Black UI, service operations, config basics, troubleshooting, and
  advanced operator notes.
- [08-config-for-dummies.md](08-config-for-dummies.md)
  Longer field-by-field config explanation.
- [fast-profile.md](fast-profile.md)
  Constraints and policy for the latency-first profile.

## Beginner Concepts

Read these when protocol names or config structure are still fuzzy:

1. [00-project-map.md](00-project-map.md)
2. [01-request-lifecycle.md](01-request-lifecycle.md)
3. [02-crate-guide.md](02-crate-guide.md)
4. [03-protocols-and-transports.md](03-protocols-and-transports.md)
5. [04-reality-for-dummies.md](04-reality-for-dummies.md)
6. [05-vless-vmess-trojan-comparison.md](05-vless-vmess-trojan-comparison.md)
7. [10-glossary.md](10-glossary.md)

## Developer Docs

- [06-how-to-debug.md](06-how-to-debug.md)
- [07-how-to-add-a-new-protocol-or-transport.md](07-how-to-add-a-new-protocol-or-transport.md)
- [09-trace-one-connection-in-code.md](09-trace-one-connection-in-code.md)
- [11-testing.md](11-testing.md)
- [commands.md](commands.md)
- [readiness.md](readiness.md)

## Validation And Evidence

- [test-workflows.md](test-workflows.md)
- [performance-evidence.md](performance-evidence.md)
- [performance.md](performance.md)
- [latency-lab.md](latency-lab.md)
- [xray-parity-source-of-truth.md](xray-parity-source-of-truth.md)
- [roadmap.md](roadmap.md)
- [external-client-failure-triage.md](external-client-failure-triage.md)
- [panel-qa.md](panel-qa.md)
- [../tests/interop/README.md](../tests/interop/README.md)
- [../labs/realistic/README.md](../labs/realistic/README.md)

## Example-Driven Learning

- [../examples/vless-client-server/README.md](../examples/vless-client-server/README.md)
- [../examples/reality-client-server/README.md](../examples/reality-client-server/README.md)
- [../examples/hysteria2-client-server/README.md](../examples/hysteria2-client-server/README.md)
- [../examples/vless-ws-local/README.md](../examples/vless-ws-local/README.md)
- [../examples/http-vmess-grpc-local/README.md](../examples/http-vmess-grpc-local/README.md)
- [../examples/ss2022-local/README.md](../examples/ss2022-local/README.md)
- [../examples/dns-fakeip-routing/README.md](../examples/dns-fakeip-routing/README.md)
- [../examples/tun-local/README.md](../examples/tun-local/README.md)
