#!/usr/bin/env bash
# Measure blackwire SERVER CPU time per request — immune to a client-bound
# load generator. Reports CPU-ms consumed by the proxy process per 1000
# requests, for both keepalive and churn modes.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAT_DIR="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$LAT_DIR/../../.." && pwd)"

BIN="${BLACKWIRE_BIN:-$ROOT/target/release/blackwire}"
CONFIG="${BLACKWIRE_CONFIG:-$LAT_DIR/configs/blackwire-socks-direct.json}"
ORIGIN_PORT="${ORIGIN_PORT:-18080}"
PROXY_PORT="${PROXY_PORT:-1080}"
DURATION="${DURATION:-10}"
CONCURRENCY="${CONCURRENCY:-8}"
PAYLOAD="${PAYLOAD:-1k}"
LABEL="${LABEL:-blackwire}"
CLK="$(getconf CLK_TCK)"   # clock ticks per second (usually 100)

cleanup() {
    [ -n "${PROXY_PID:-}" ] && kill "$PROXY_PID" 2>/dev/null || true
    [ -n "${ORIGIN_PID:-}" ] && kill "$ORIGIN_PID" 2>/dev/null || true
}
trap cleanup EXIT

# Sum utime+stime (fields 14,15) of pid in clock ticks.
cpu_ticks() {
    local pid="$1"
    awk '{print $14 + $15}' "/proc/$pid/stat" 2>/dev/null || echo 0
}

python3 "$HERE/origin_static.py" --port "$ORIGIN_PORT" >/tmp/origin.log 2>&1 &
ORIGIN_PID=$!
sleep 0.5
"$BIN" run -c "$CONFIG" >/tmp/blackwire-bench.log 2>&1 &
PROXY_PID=$!
for _ in $(seq 1 50); do
    python3 -c "import socket;socket.create_connection(('127.0.0.1',$PROXY_PORT),0.2)" 2>/dev/null && break
    sleep 0.1
done

measure() {
    local mode="$1"
    local before after dticks out req cpu_ms cpu_per_k
    before="$(cpu_ticks "$PROXY_PID")"
    out="$(python3 "$HERE/socks_loadgen.py" \
        --proxy-port "$PROXY_PORT" --dst-port "$ORIGIN_PORT" \
        --payload "$PAYLOAD" --duration "$DURATION" --concurrency "$CONCURRENCY" \
        --mode "$mode" --label "$LABEL" --warmup 1.0)"
    after="$(cpu_ticks "$PROXY_PID")"
    dticks=$((after - before))
    req="$(printf '%s\n' "$out" | sed -n 's/.*req=\([0-9]*\).*/\1/p')"
    cpu_ms="$(awk -v t="$dticks" -v c="$CLK" 'BEGIN{printf "%.1f", t*1000.0/c}')"
    cpu_per_k="$(awk -v ms="$cpu_ms" -v r="$req" 'BEGIN{ if(r>0) printf "%.3f", ms/(r/1000.0); else print "n/a"}')"
    echo "$out"
    echo "    -> server CPU: ${cpu_ms}ms over ${req} req = ${cpu_per_k} CPU-ms / 1000 req"
}

echo "=== $LABEL : server CPU/req  payload=$PAYLOAD conc=$CONCURRENCY dur=${DURATION}s ==="
measure keepalive
measure churn
