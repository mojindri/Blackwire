#!/usr/bin/env bash
# Local proxy micro-benchmark: origin + blackwire(SOCKS->freedom) + loadgen.
# Measures both keepalive (warm relay) and churn (per-connection) throughput.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAT_DIR="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$LAT_DIR/../../.." && pwd)"

BIN="${BLACKWIRE_BIN:-$ROOT/target/release/blackwire}"
FIXTURE="${BLACKWIRE_FIXTURE:-$LAT_DIR/configs/blackwire-socks-direct.json}"
ORIGIN_PORT="${ORIGIN_PORT:-18080}"
PROXY_PORT="${PROXY_PORT:-1080}"
DURATION="${DURATION:-10}"
CONCURRENCY="${CONCURRENCY:-8}"
PAYLOAD="${PAYLOAD:-1k}"
LABEL="${LABEL:-blackwire}"
LAB_DATABASE_URL="${BLACKWIRE_LAB_DATABASE_URL:?set BLACKWIRE_LAB_DATABASE_URL to a disposable MySQL database}"

cleanup() {
    [ -n "${PROXY_PID:-}" ] && kill "$PROXY_PID" 2>/dev/null || true
    [ -n "${ORIGIN_PID:-}" ] && kill "$ORIGIN_PID" 2>/dev/null || true
}
trap cleanup EXIT

echo "binary: $BIN"
[ -x "$BIN" ] || { echo "ERROR: binary not found/executable: $BIN" >&2; exit 1; }

python3 "$HERE/origin_static.py" --port "$ORIGIN_PORT" >/tmp/origin.log 2>&1 &
ORIGIN_PID=$!
sleep 0.5

bash "$ROOT/labs/realistic/scripts/prepare-mysql-fixture.sh" "$BIN" "$FIXTURE" "$LAB_DATABASE_URL"
BLACKWIRE_DATABASE_URL="$LAB_DATABASE_URL" \
    "$BIN" run >/tmp/blackwire-bench.log 2>&1 &
PROXY_PID=$!

# wait for proxy port
for _ in $(seq 1 50); do
    if python3 -c "import socket;socket.create_connection(('127.0.0.1',$PROXY_PORT),0.2)" 2>/dev/null; then
        break
    fi
    sleep 0.1
done

run() {
    local mode="$1"
    python3 "$HERE/socks_loadgen.py" \
        --proxy-port "$PROXY_PORT" --dst-port "$ORIGIN_PORT" \
        --payload "$PAYLOAD" --duration "$DURATION" --concurrency "$CONCURRENCY" \
        --mode "$mode" --label "$LABEL"
}

echo "=== $LABEL : payload=$PAYLOAD conc=$CONCURRENCY dur=${DURATION}s ==="
run keepalive
run churn
