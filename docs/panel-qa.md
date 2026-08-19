# Black UI acceptance checklist

- Navigation contains Dashboard, Users, Inbounds, Outbounds, Routing & DNS,
  Runtime, and Settings.
- Typed forms cover supported relational fields and reject invalid input.
- Saving returns revision, parent, active revision, activation class/state, and a
  useful message.
- Runtime displays database health, schema version, desired/active revisions,
  last reconciliation, reload errors, immutable history, and rollback.
- Every valid write enters `activating` and applies automatically; no confirmation
  control or process-restart action is present.
- Stale edits are rejected and database loss makes mutation views read-only.
- No raw server configuration editor, file path, write/apply/import control,
  SQLite backup, or user-facing gRPC configuration control is present.
- User Copy subscription fetches and copies client content that Hiddify accepts.
- Subscription tokens for disabled, expired, or unknown users return not found.
