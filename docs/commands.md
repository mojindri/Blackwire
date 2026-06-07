# Commands

Use this as the command entry point. Run repo-level `make` commands from your
local checkout; they orchestrate Docker, Lima, or VPS hosts when needed.

Discovery:

```sh
make help
make help-compat
make help-internal
```

## Recommended Commands

| Command | Use |
| --- | --- |
| `make verify-local` | Everyday Rust development; no Docker/Lima/VPS |
| `make verify-lab` | Docker + Lima lab gate |
| `make verify-lab-docker` | Docker interop and advanced smoke only |
| `make verify-lab-lima` | Lima browser TLS fingerprint baseline |
| `make verify-remote` | Two-VPS public-network validation |
| `make verify-sweep` | Broad pre-merge gate |
| `make verify-release` | Slow release gate |
| `make perf` | Lima VM benchmark |
| `make perf-remote` | VPS benchmark |
| `make security` | audit + deny + lab security helpers |
| `make fuzz-smoke` | Short fuzz pass |
| `make clean-generated` | Remove generated reports/logs/pcaps/bench outputs |

VPS variables:

```sh
SSH_SERVER=1.2.3.4 SSH_CLIENT=5.6.7.8 SSH_KEY=~/.ssh/id_ed25519 make verify-remote
```

Optional: `SSH_USER`, `SSH_PORT`, `SSH_EXTRA_OPTS`.

## Environment Guide

| Environment | Command | Notes |
| --- | --- | --- |
| Host Rust | `make verify-local` | fastest feedback |
| Docker lab | `make verify-lab-docker` | Xray/sing-box interop and configured scenarios |
| Lima VM | `make verify-lab-lima` | browser TLS fingerprint and VM benchmarks |
| Real VPS | `make verify-remote` | closest network signal; mutates remote hosts |

Direct VPS shell use is for debugging only:

```sh
ssh root@<server>
systemctl status blackwire-*
journalctl -u blackwire-* --no-pager | tail -200
```

Do not run repo-level `make verify-*` from inside a VPS shell.

## Command Families

Build and quality:

```sh
make build
make dev
make fmt-check
make lint
make lint-strict
make test
make audit
make deny
```

Realistic lab:

```sh
make -C labs/realistic docker-full
make -C labs/realistic interop-docker
make -C labs/realistic interop-server-docker
make -C labs/realistic interop-client-reality
make -C labs/realistic interop-server-vps
make -C labs/realistic prod-readiness
```

Remote atoms:

```sh
make remote-preflight
make remote-deploy
make remote-test-protocols
make remote-test-fingerprint
make remote-collect
```

Fuzz and perf:

```sh
make fuzz-smoke
make fuzz-long
make perf
make perf-remote
```

## Legacy Aliases

These still work and print a deprecation hint:

| Old | Prefer |
| --- | --- |
| `make check` / `make local-total` | `make verify-check-compat` |
| `make check-browser` | `make verify-lab-lima` |
| `make check-vps` | `verify-check-compat` + `verify-remote` |
| `make ci` / `make local-fast` | `make verify-local` |
| `make ci-vps` / `make vps` | `make verify-remote` |
| `make local-fuzz` | `make fuzz-smoke` |
| `make local-fuzz-total` | `make fuzz-long` |
| `make check-perf-vm` | `make perf` |
| `make perf-vps` / `make check-perf-vps` | `make perf-remote` |

Full current target details are discoverable with `make help`,
`make help-compat`, and `make help-internal`.

Related docs:

- [test-workflows.md](test-workflows.md)
- [11-testing.md](11-testing.md)
- [../labs/realistic/README.md](../labs/realistic/README.md)
- [../tests/interop/README.md](../tests/interop/README.md)
