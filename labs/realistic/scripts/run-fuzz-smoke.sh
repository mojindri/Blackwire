#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

REPORT_DIR_ARG="${1:-labs/realistic/reports/production}"
FUZZ_RUNS="${FUZZ_RUNS:-32}"
case "$REPORT_DIR_ARG" in
  /*) REPORT_DIR="$REPORT_DIR_ARG" ;;
  *) REPORT_DIR="$PROJECT_ROOT/$REPORT_DIR_ARG" ;;
esac
mkdir -p "$REPORT_DIR"

cd "$PROJECT_ROOT"

if [ ! -d "$PROJECT_ROOT/fuzz" ]; then
  echo "No fuzz/ directory found at project root. Copy fuzz.zip contents into project root first."
  exit 0
fi

if ! command -v cargo-fuzz >/dev/null 2>&1; then
  echo "cargo-fuzz not installed. Install with: cargo install cargo-fuzz"
  exit 0
fi

if ! rustup toolchain list | grep -q '^nightly'; then
  echo "nightly Rust toolchain not installed. Install with:"
  echo "  rustup toolchain install nightly"
  exit 0
fi

TARGETS="$(find "$PROJECT_ROOT/fuzz/fuzz_targets" \
  -maxdepth 1 \
  -type f \
  -name '*.rs' \
  ! -name 'common.rs' \
  -exec basename {} .rs \; 2>/dev/null | sort)"

if [ -z "$TARGETS" ]; then
  echo "No fuzz targets found under fuzz/fuzz_targets/."
  exit 0
fi

cd "$PROJECT_ROOT/fuzz"

for target in $TARGETS; do
  echo "==> fuzz smoke: $target"
  cargo +nightly fuzz run "$target" -- -runs=$FUZZ_RUNS 2>&1 | tee "$REPORT_DIR/fuzz-$target.log"
done

echo "Fuzz smoke complete. Reports written to $REPORT_DIR"
