#!/usr/bin/env bash
# Pool A/B: disabled vs adaptive, churn mode.
# Runs entirely on loopback — numbers are relative not absolute.
set -euo pipefail

BINARY="${BINARY:-./target/debug/blackwire}"
SCRIPTS="labs/realistic/latency/scripts"
CONFIGS="labs/realistic/latency/configs"
DURATION=30
CONCURRENCY=16
ORIGIN_PORT=18081
PROXY_PORT=10080
SOCKS_PORT=1082

CLIENT_CFG=$(mktemp /tmp/bw-pool-client-XXXX.json)
cat > "$CLIENT_CFG" <<EOF
{
  "log": { "level": "warn" },
  "inbounds": [{"tag":"socks-in","protocol":"socks","listen":"127.0.0.1","port":$SOCKS_PORT}],
  "outbounds": [{"tag":"vless-out","protocol":"vless","settings":{
    "address":"127.0.0.1","port":$PROXY_PORT,
    "users":[{"id":"00000000-0000-4000-8000-000000000001","flow":""}]
  }}]
}
EOF

run_variant() {
    local label="$1" server_cfg="$2"

    # start origin
    python3 "$SCRIPTS/origin_static.py" --port $ORIGIN_PORT &
    local origin_pid=$!

    # start server
    "$BINARY" run -c "$server_cfg" &
    local server_pid=$!

    # start client
    "$BINARY" run -c "$CLIENT_CFG" &
    local client_pid=$!

    sleep 2  # let everything bind

    # warmup: 1s to push past min_hotness_for_pool=8
    python3 "$SCRIPTS/socks_loadgen.py" \
        --proxy-port $SOCKS_PORT --dst-port $ORIGIN_PORT \
        --mode churn --duration 1 --concurrency $CONCURRENCY \
        > /dev/null 2>&1 || true

    sleep 0.5

    # measured run
    echo -n "  $label: "
    python3 "$SCRIPTS/socks_loadgen.py" \
        --proxy-port $SOCKS_PORT --dst-port $ORIGIN_PORT \
        --mode churn --duration $DURATION --concurrency $CONCURRENCY \
        2>/dev/null

    # grab pool metrics if available
    local metrics_port
    metrics_port=$(python3 -c "import json,sys; d=json.load(open('$server_cfg')); print(d.get('metricsAddr','').split(':')[-1] or 'none')" 2>/dev/null || echo "")
    if [[ -n "$metrics_port" && "$metrics_port" != "none" && "$metrics_port" != "" ]]; then
        local hits misses stales
        hits=$(curl -s "http://127.0.0.1:$metrics_port/metrics" 2>/dev/null | grep '^freedom_pool_leases_total' | awk '{print $2}' || echo "?")
        misses=$(curl -s "http://127.0.0.1:$metrics_port/metrics" 2>/dev/null | grep '^freedom_pool_misses_total' | awk '{print $2}' || echo "?")
        stales=$(curl -s "http://127.0.0.1:$metrics_port/metrics" 2>/dev/null | grep '^freedom_pool_stales_total' | awk '{print $2}' || echo "?")
        echo "  pool metrics → hits=$hits  misses=$misses  stales=$stales"
    fi

    kill $client_pid $server_pid $origin_pid 2>/dev/null || true
    wait $client_pid $server_pid $origin_pid 2>/dev/null || true
    sleep 1
}

cd /home/user/Blackwire

echo "=== pool A/B benchmark (loopback, churn, ${DURATION}s, concurrency=${CONCURRENCY}) ==="
echo ""
run_variant "no-pool (baseline)" "$CONFIGS/blackwire-fast-lab-server.json"
echo ""
run_variant "pool: adaptive     " "$CONFIGS/fast-knobs/server-pool-adaptive.json"
echo ""
echo "note: loopback TCP connect is ~0.01ms; pool benefit is ~100x larger on a real VPS."

rm -f "$CLIENT_CFG"
