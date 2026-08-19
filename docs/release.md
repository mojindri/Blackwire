# Release process

Blackwire releases are MySQL-only. Do not publish a dual-source build.

## Gates

1. Run workspace tests and Black UI QA.
2. Run migration and relational round-trip tests against MySQL 8.4/InnoDB.
3. Test stale writers, deadlocks, automatic rollback reload, failed handover
   last-known-good retention, and temporary database loss.
4. Verify Docker health ordering and distinct runtime, UI, and migrator secrets.
5. Verify systemd credential loading and that services refuse incompatible
   schema versions without migrating them.
6. Confirm the user Copy subscription action still produces Hiddify-compatible
   client content.
7. Reject runtime configuration files, SQLite dependencies/artifacts, MySQL JSON
   columns, generic configuration values, and active file-deployment guidance.

## Packaging

Native packages require an existing MySQL endpoint. Operators provide protected
runtime/UI URLs and explicitly run migrations using the migrator account. The
installer reports legacy JSON/SQLite data and leaves it untouched.

## Recovery

Revision history provides application rollback only. Release notes must remind
operators to use MySQL dumps, storage snapshots, and binlogs for disaster recovery.
