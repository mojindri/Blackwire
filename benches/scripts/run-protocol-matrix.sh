#!/usr/bin/env bash
# Run the end-to-end protocol bench matrix (all five paths or a subset).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPORT_DIR="${BENCH_REPORT_DIR:-$PROJECT_ROOT/benches/reports}"
TS="$(date -u +%Y%m%dT%H%M%SZ)"
QUICK="${BENCH_QUICK:-}"
FEATURES="${BENCH_FEATURES:-}"
PROTOCOLS="${BENCH_PROTOCOLS:-vless_tcp,vless_ws,vmess_grpc,ss2022,trojan_tcp}"

mkdir -p "$REPORT_DIR"
LOG="$REPORT_DIR/protocol-matrix-$TS.log"

export RUST_LOG="${RUST_LOG:-warn}"
[ -n "$QUICK" ] && export BENCH_QUICK=1

FEATURE_ARGS=()
if [ -n "$FEATURES" ]; then
  FEATURE_ARGS=(--features "$FEATURES")
fi

{
  echo "protocol-matrix"
  echo "timestamp=$TS"
  echo "quick=${QUICK:-0}"
  echo "features=${FEATURES:-none}"
  echo "protocols=$PROTOCOLS"
  echo ""
} | tee "$LOG"

IFS=',' read -r -a PROTO_LIST <<< "$PROTOCOLS"
for proto in "${PROTO_LIST[@]}"; do
  proto_norm="${proto//-/_}"
  bench="e2e_${proto_norm}"
  echo "==> cargo bench -p blackwire-benches --bench $bench ${FEATURE_ARGS[*]-}" | tee -a "$LOG"
  (
    cd "$PROJECT_ROOT"
    if [ "${#FEATURE_ARGS[@]}" -gt 0 ]; then
      cargo bench -p blackwire-benches --bench "$bench" "${FEATURE_ARGS[@]}" 2>&1
    else
      cargo bench -p blackwire-benches --bench "$bench" 2>&1
    fi
  ) | tee -a "$LOG"
done

echo "" | tee -a "$LOG"
echo "report=$LOG" | tee -a "$LOG"
echo "criterion_html=$PROJECT_ROOT/target/criterion/report/index.html"
