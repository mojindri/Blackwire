#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REALISTIC_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPORT_ROOT="$REALISTIC_DIR/reports"

print_report() {
    local report_dir="$1"
    local summary="$report_dir/summary.txt"
    [[ -f "$summary" ]] || return 1

    echo "==> External client compatibility summary: $(basename "$report_dir")"
    grep -E '^(PASS|FAIL|SKIP) ' "$summary" || true
    echo "Full logs: $report_dir/logs"
    echo ""
}

if [[ $# -gt 0 ]]; then
    print_report "$1" || {
        echo "No external-client summary found at $1/summary.txt"
        exit 1
    }
    exit 0
fi

found=0
for dir in "$REPORT_ROOT/external-clients" "$REPORT_ROOT/external-clients-vps"; do
    if print_report "$dir"; then
        found=1
    fi
done

if [[ "$found" -eq 0 ]]; then
    echo "No external-client summaries found under $REPORT_ROOT"
    exit 1
fi
