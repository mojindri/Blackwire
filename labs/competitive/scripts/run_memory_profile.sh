#!/usr/bin/env bash
set -euo pipefail

REPORT_DIR="${REPORT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/reports}"
mkdir -p "$REPORT_DIR"
if command -v heaptrack >/dev/null 2>&1; then
    echo "heaptrack available" | tee "$REPORT_DIR/memory-profile.log"
elif command -v valgrind >/dev/null 2>&1; then
    echo "valgrind available" | tee "$REPORT_DIR/memory-profile.log"
else
    echo "SKIP: heaptrack/valgrind not found" | tee "$REPORT_DIR/memory-profile.log"
fi
