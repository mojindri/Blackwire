# Blackwire

Blackwire is a Rust proxy runtime with a database-backed control plane and a
typed web UI. MySQL 8.4/InnoDB is the sole persistent source of truth for the
runtime, CLI, and Black UI.

## Requirements

- MySQL 8.4 with separate runtime, UI, and migrator accounts
- A protected file containing a MySQL URL, or `BLACKWIRE_DATABASE_URL` for development
- Rust stable when building from source

Blackwire never installs MySQL silently and services never migrate the schema
at startup.

## First run

Apply migrations explicitly with the migrator credential:

```bash
BLACKWIRE_DATABASE_URL_FILE=/run/credentials/migrator-database-url blackwire db init
BLACKWIRE_DATABASE_URL_FILE=/run/credentials/runtime-database-url blackwire db status
BLACKWIRE_DATABASE_URL_FILE=/run/credentials/runtime-database-url blackwire run
```

An empty inbound list is a valid idle control-plane state. Add configuration in
Black UI or start from a relational preset:

```bash
blackwire db seed socks-local
blackwire db seed vless-local
blackwire db seed trojan-local
blackwire db seed shadowsocks-local
```

## Configuration lifecycle

Every completed UI or CLI edit creates an immutable revision. The runtime polls
MySQL, validates the desired revision, and either hot-swaps it, hands over a
supported listener, or leaves it pending for confirmed maintenance activation.
It continues serving the active in-memory revision during a temporary database
outage and does not activate another revision until connectivity returns.

Useful commands:

```bash
blackwire db validate
blackwire db status
blackwire db history --limit 20
blackwire db rollback REVISION
blackwire db activate-maintenance REVISION
blackwire explain-cost
```

## Black UI

The UI provides Dashboard, Users, Inbounds, Outbounds, Routing & DNS, Runtime,
and Settings views. Configuration forms write typed relational revisions; there
is no raw server-configuration editor.

The user Copy subscription action remains supported. It copies database-derived
client subscription content suitable for Hiddify. This is deliberately separate
from server configuration import/export, which is not supported.

## Deployment

`deploy/docker/docker-compose.yml` includes MySQL 8.4, explicit migration,
separate database accounts, health ordering, secrets, an InnoDB volume, the
runtime, and Black UI. Native systemd units use protected credentials under
`/etc/blackwire`.

The native installer requires `RUNTIME_DATABASE_URL_FILE`; Black UI additionally
requires `UI_DATABASE_URL_FILE`. Set `RUN_DB_MIGRATIONS=1` together with
`MIGRATOR_DATABASE_URL_FILE` only when you explicitly want the installer to run
`blackwire db migrate`.

Legacy JSON and SQLite installations are detected and reported as incompatible.
They are left untouched and are not imported automatically. Use MySQL
dump/snapshot/binlog tooling for disaster recovery; application revision history
is rollback history, not a database backup.

## Development checks

```bash
cargo check --workspace
cargo test --workspace
cd black-ui/frontend && npm run qa
```

MySQL integration tests must run against MySQL 8.4; SQLite is not a substitute.
