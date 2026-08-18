#!/usr/bin/env bash
# Load a lab-only fixture into the MySQL control plane used by a Blackwire process.
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
    echo "usage: $0 BLACKWIRE_BIN FIXTURE [DATABASE_URL]" >&2
    exit 2
fi
database_url="${3:-${BLACKWIRE_LAB_DATABASE_URL:-}}"
database_url_file="${BLACKWIRE_LAB_DATABASE_URL_FILE:-}"
if [[ -z "$database_url" && -z "$database_url_file" ]]; then
    echo "ERROR: set BLACKWIRE_LAB_DATABASE_URL or BLACKWIRE_LAB_DATABASE_URL_FILE for a disposable MySQL 8.4 database" >&2
    exit 1
fi

blackwire_bin="$1"
fixture="$2"
[ -x "$blackwire_bin" ] || { echo "ERROR: binary not executable: $blackwire_bin" >&2; exit 1; }
[ -f "$fixture" ] || { echo "ERROR: fixture not found: $fixture" >&2; exit 1; }

if [[ -n "$database_url" ]]; then
    export BLACKWIRE_DATABASE_URL="$database_url"
else
    export BLACKWIRE_DATABASE_URL_FILE="$database_url_file"
fi
"$blackwire_bin" db init
"$blackwire_bin" db import-fixture --replace "$fixture"
