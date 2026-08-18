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
export BLACKWIRE_SERVER_DATABASE_URL='mysql://.../blackwire_competitive_server'
export BLACKWIRE_CLIENT_DATABASE_URL='mysql://.../blackwire_competitive_client'
make competitive-smoke
make competitive-report
```

The two databases must be disposable and distinct. The runner migrates them and
replaces their relational configuration from each Blackwire lab fixture. Xray
and sing-box continue to consume their native JSON files directly.

VPS defaults are intentionally left unset in-repo; provide hosts via environment:

```bash
export COMPETITIVE_SERVER_HOST=<server-host>
export COMPETITIVE_CLIENT_HOST=<client-host>
COMPETITIVE_SSH_KEY=id_hetzner
```

Each remote host also needs a migration-capable MySQL URL in
`/etc/blackwire/lab-database-url`. Override that path with
`BLACKWIRE_REMOTE_DATABASE_URL_FILE`. Server and client hosts must point at
different disposable databases.

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
