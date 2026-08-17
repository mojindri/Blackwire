# Blackwire user guide

Blackwire uses MySQL 8.4 as its only persistent configuration store. Supply a
protected database URL to the runtime and a different least-privilege URL to
Black UI. Reserve the migrator account for explicit schema operations.

## Database setup

```bash
BLACKWIRE_DATABASE_URL_FILE=/secure/migrator-database-url blackwire db init
BLACKWIRE_DATABASE_URL_FILE=/secure/runtime-database-url blackwire db validate
BLACKWIRE_DATABASE_URL_FILE=/secure/runtime-database-url blackwire db status
```

Services check schema compatibility but never alter it. A new installation can
run with no inbound while it is configured.

## Daily operation

Use Black UI for typed Users, Inbounds, Outbounds, Routing & DNS, Runtime, and
Settings workflows. Saving creates a revision. Runtime shows desired and active
revisions, activation failures, history, rollback, and maintenance confirmation.

Start the runtime with:

```bash
BLACKWIRE_DATABASE_URL_FILE=/secure/runtime-database-url blackwire run
```

During a MySQL outage, established service continues from the active in-memory
revision. Edits and activation wait until MySQL reconnects.

## Client subscriptions

In Users, choose Copy subscription. Black UI fetches the user's database-derived
subscription content and copies it to the clipboard for Hiddify. This client
export remains supported. Raw Blackwire server configuration import/export does
not exist.

## Revision recovery

```bash
blackwire db history --limit 20
blackwire db rollback REVISION
blackwire db activate-maintenance REVISION
```

Rollback creates another revision. Back up MySQL separately with dump, snapshot,
and binlog tooling.

## Legacy installations

Legacy JSON and SQLite files are reported but never modified or imported. Keep
them only for manual reference while recreating the desired typed state in MySQL.
