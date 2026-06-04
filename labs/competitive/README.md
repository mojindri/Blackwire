# Blackwire Competitive Lab

Milestone A scope: build the benchmark arena before changing performance code.

This lab compares:

- `blackwire-current`
- `blackwire-candidate`
- `xray`
- `sing-box`
- `hysteria`
- `shoes`

The scripts are intentionally fail-soft. Missing competitor binaries produce structured `skipped` rows instead of failing the whole run.

## Quick Start

```bash
make competitive-smoke
make competitive-report
```

VPS defaults are intentionally left unset in-repo; provide hosts via environment:

```bash
export COMPETITIVE_SERVER_HOST=<server-host>
export COMPETITIVE_CLIENT_HOST=<client-host>
COMPETITIVE_SSH_KEY=id_hetzner
```

Run remote inventory and runnable remote rows:

```bash
make competitive-clean COMPETITIVE_MODE=remote
```

## Commands

Root Makefile wrappers:

```text
make competitive-smoke
make competitive-clean
make competitive-loss
make competitive-mobile
make competitive-tun
make competitive-quic
make competitive-expensive
make competitive-all
make competitive-report
```

## Baseline Policy

Reports in `reports/` are machine-specific. Copy accepted baselines into `baselines/` only with the machine, kernel, binary versions, scenario, and full command recorded.

Do not claim performance wins from skipped rows, partial rows, or local-loopback-only rows.
