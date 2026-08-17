#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

fail() {
    echo "MySQL-only consistency check failed: $1" >&2
    exit 1
}

if rg -n -i 'rusqlite|libsqlite|sqlite[_-]' --glob 'Cargo.toml' --glob '*.rs' \
    Cargo.toml crates black-ui/server; then
    fail "SQLite code or a direct SQLite dependency remains"
fi

if cargo tree --workspace -e normal --prefix none 2>/dev/null \
    | rg -q '^(sqlx-sqlite|libsqlite3-sys)( |$)'; then
    fail "SQLite is active in the Rust dependency graph"
fi

if rg -q '^name = "(sqlx-sqlite|libsqlite3-sys)"$' Cargo.lock; then
    fail "SQLite remains recorded in Cargo.lock"
fi

if git ls-files | rg -q '\.(sqlite|sqlite3|db)$'; then
    fail "a SQLite database artifact is tracked in the repository"
fi

if rg -n -i '\bJSON\b' crates/blackwire-store/migrations; then
    fail "a MySQL JSON column exists in the relational schema"
fi

if rg -n 'pub\s+[^:]+:\s*(Option<)?serde_json::Value' \
    crates/blackwire-config/src crates/blackwire-store/src; then
    fail "a generic JSON value remains in a persistent configuration model"
fi

if rg -n --glob '!docs/release.md' --glob '!docs/panel-qa.md' \
    -- '--config\b|CONFIG_PATH\b|CONFIG_URL\b|BLACK_UI_CONFIG_PATH\b|config\.json' \
    crates/blackwire-cli crates/blackwire-app black-ui deploy README.md docs examples; then
    fail "an active file-configuration path or deployment instruction remains"
fi

if rg -n 'ConfigManager|blackwire-config/src/(manager|env)\.rs|file watch \+ validated reload' \
    README.md docs crates/blackwire-cli crates/blackwire-core; then
    fail "legacy file-configuration architecture documentation remains"
fi

echo "MySQL-only consistency checks passed"
