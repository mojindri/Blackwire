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

if rg -n '/(Users|home)/[^/]+/' README.md docs examples; then
    fail "machine-specific absolute paths remain in repository documentation"
fi

if rg -n 'github\.com/mojindri/v2ray' README.md docs deploy scripts; then
    fail "the old repository URL remains in active documentation or deployment files"
fi

while IFS= read -r reference; do
    path="${reference#../}"
    [ -e "$path" ] || fail "documentation references missing path: $reference"
done < <(rg --no-filename -o '(\.\./)?examples/[A-Za-z0-9._/-]+' README.md docs | sort -u)

workspace_version="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' Cargo.toml | head -n 1)"
frontend_version="$(sed -n 's/[[:space:]]*"version":[[:space:]]*"\([^"]*\)",/\1/p' black-ui/frontend/package.json | head -n 1)"
[ -n "$workspace_version" ] || fail "workspace version could not be read"
[ "$workspace_version" = "$frontend_version" ] \
    || fail "workspace and frontend versions differ"
rg -q "^## ${workspace_version} -" CHANGELOG.md \
    || fail "CHANGELOG has no entry for version ${workspace_version}"

for dockerfile in deploy/docker/Dockerfile deploy/docker/Dockerfile.ui; do
    rg -q '^FROM rust:slim-bookworm AS ' "$dockerfile" \
        || fail "$dockerfile does not use the repository stable Rust toolchain"
done

if rg -n '^(User=nobody|Group=black-ui)$' deploy/systemd; then
    fail "checked-in systemd units do not use the canonical blackwire service account"
fi

rg -q '^Environment=BLACK_UI_STATIC_DIR=/usr/local/share/black-ui/frontend/dist$' \
    deploy/systemd/black-ui.service \
    || fail "checked-in Black UI unit disagrees with the native installer static path"

echo "MySQL-only consistency checks passed"
