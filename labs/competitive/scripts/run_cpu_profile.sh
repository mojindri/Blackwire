#!/usr/bin/env bash
set -euo pipefail

REPORT_DIR="${REPORT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/reports}"
mkdir -p "$REPORT_DIR"
if ! command -v perf >/dev/null 2>&1; then
    echo "SKIP: perf not found" | tee "$REPORT_DIR/cpu-profile.log"
    exit 0
fi
echo "CPU profile hook is ready. Run a benchmark under perf from this native host." | tee "$REPORT_DIR/cpu-profile.log"
