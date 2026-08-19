# Blackwire User Guide

Blackwire uses MySQL 8.4/InnoDB as its only persistent configuration store.
You manage the runtime through Black UI or database-backed CLI commands; you do
not deploy a JSON configuration file.

## Before You Install

You need:

- a reachable MySQL 8.4 server using InnoDB;
- protected files containing MySQL URLs; and
- separate least-privilege accounts for the runtime, Black UI, and schema
  migrator.

The runtime and Black UI should only receive their own credential files. Keep
the more privileged migrator credential for explicit schema operations. For
local development only, `BLACKWIRE_DATABASE_URL` is convenient; deployed
services should use `BLACKWIRE_DATABASE_URL_FILE`.

Blackwire never installs MySQL silently. Services verify schema compatibility
but never migrate your database at startup.

## Install The Native Service

Download the release installer and provide your runtime credential file:

```sh
curl -fsSLO https://raw.githubusercontent.com/mojindri/Blackwire/v0.2.5/scripts/install.sh
chmod +x install.sh
VERSION=v0.2.5 RUNTIME_DATABASE_URL_FILE=/secure/runtime-database-url ./install.sh
```

To explicitly migrate the schema during installation, provide the migrator
credential. To install Black UI, provide its separate credential too:

```sh
RUNTIME_DATABASE_URL_FILE=/secure/runtime-database-url \
MIGRATOR_DATABASE_URL_FILE=/secure/migrator-database-url \
UI_DATABASE_URL_FILE=/secure/ui-database-url \
RUN_DB_MIGRATIONS=1 INSTALL_BLACK_UI=1 VERSION=v0.2.5 ./install.sh
```

The installer copies service credentials into protected locations under
`/etc/blackwire`. It does not start a migration unless you opt in with
`RUN_DB_MIGRATIONS=1`.

Black UI defaults to `127.0.0.1:18080`. For an intentionally public panel,
set `BLACK_UI_EXPOSURE=public`; the installer then listens on `0.0.0.0:18080`
unless you supply `BLACK_UI_LISTEN`. Set `BLACK_UI_PUBLIC_BASE_URL` to the
externally reachable panel origin and `BLACK_UI_SUBSCRIPTION_HOST` to the
hostname or IP clients use for proxy connections:

```sh
BLACK_UI_EXPOSURE=public \
BLACK_UI_PUBLIC_BASE_URL=http://PUBLIC_IP:18080 \
BLACK_UI_SUBSCRIPTION_HOST=PUBLIC_IP \
INSTALL_BLACK_UI=1 ... ./install.sh
```

The service treats those values as runtime overrides, which prevents an old
loopback value in MySQL from reappearing in copied or QR subscription URLs.
Use a hardened HTTPS reverse proxy and access controls for a public panel. Keep
MySQL private regardless of the panel choice.

For a container deployment, use [Docker Compose](../deploy/docker/docker-compose.yml).
Create the secret files referenced by that Compose file before starting the
stack, then export `BLACK_UI_PUBLIC_BASE_URL` and
`BLACK_UI_SUBSCRIPTION_HOST`. It creates MySQL, migration, runtime, and Black
UI services with separate credentials.

## Database Setup And First Run

Apply migrations explicitly, then validate the desired revision with the
runtime credential:

```sh
BLACKWIRE_DATABASE_URL_FILE=/secure/migrator-database-url blackwire db init
BLACKWIRE_DATABASE_URL_FILE=/secure/runtime-database-url blackwire db validate
BLACKWIRE_DATABASE_URL_FILE=/secure/runtime-database-url blackwire db status
```

Start the runtime manually only when you are not using systemd:

```sh
BLACKWIRE_DATABASE_URL_FILE=/secure/runtime-database-url blackwire run
```

An empty inbound list is a valid idle state. Blackwire exposes no proxy port
until you add an inbound.

## Black UI

Black UI manages Users, Inbounds, Outbounds, Routing & DNS, Runtime, and
Settings. Each saved change creates an immutable revision.

The native service binds Black UI to `127.0.0.1:18080` by default. Keep it
private, or publish it only behind your own hardened HTTPS reverse proxy and
access control. Do not expose the panel directly on the public internet.

In Users, **Copy subscription** gives you database-derived client content for
Hiddify. It is intentionally not a server configuration export or import.

## Configure And Activate

Use [Configuration For Dummies](08-config-for-dummies.md) to build your first
inbound, outbound, and routing rule. Blackwire validates every revision and
applies it automatically through an atomic reload or prepared in-process
handover.

During a temporary MySQL outage, the runtime continues serving its active
in-memory revision. Edits and new activations wait for MySQL to return.

## Daily Operation And Recovery

```sh
blackwire db validate
blackwire db status
blackwire db history --limit 20
blackwire db rollback REVISION
sudo systemctl status blackwire --no-pager
sudo journalctl -u blackwire -n 100 --no-pager
```

Rollback creates a new desired revision from a historical snapshot. It is not a
database backup: protect MySQL separately with dumps, storage snapshots, and
binlogs.

## Firewall, TLS, And Public Ports

Open only the ports used by inbounds you intentionally create. Do not assume a
default proxy port exists; the active revision determines listeners. For
TLS-backed public inbounds, manage certificates and DNS before exposing the
port, and restrict panel access independently from proxy access.

## Legacy Installations And Lab Fixtures

Legacy JSON and SQLite data are reported but never modified or imported. Keep
them only as manual reference while recreating the desired typed state in
MySQL.

Repository JSON files are lab fixtures. `blackwire db import-fixture` loads one
into an explicitly disposable MySQL database for tests; do not run it against
production data.

## Further Reading

- [Feature Matrix](feature-matrix.md) — support status, evidence, and caveats.
- [Release Guide](release.md) — release contract and recovery expectations.
- [Commands](commands.md) — development and lab validation commands.
