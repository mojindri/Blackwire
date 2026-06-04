#!/usr/bin/env bash
set -euo pipefail

VARIANT="${VARIANT:?}"
RUNTIME_NAME="${RUNTIME_NAME:?}"
RUNTIME_CMD="${RUNTIME_CMD:?}"
SERVER_HOST="${SERVER_HOST:?}"
TS="${TS:?}"
SCENARIO="${SCENARIO:-tun}"
TUN_UDP_ECHO_PORT="${TUN_UDP_ECHO_PORT:-1056}"
TUN_UDP_COUNT="${TUN_UDP_COUNT:-500}"
TUN_UDP_PAYLOAD_BYTES="${TUN_UDP_PAYLOAD_BYTES:-64}"
TUN_UDP_TIMEOUT_MS="${TUN_UDP_TIMEOUT_MS:-3000}"
COMPETITIVE_REMOTE_UPSTREAM_PORT="${COMPETITIVE_REMOTE_UPSTREAM_PORT:-18080}"
TUN_TCP_PAYLOAD="${TUN_TCP_PAYLOAD:-64m}"
TUN_TCP_TIMEOUT_SEC="${TUN_TCP_TIMEOUT_SEC:-30}"
TUN_TCP_TARGET_URL="${TUN_TCP_TARGET_URL:-}"
COMPETITIVE_CONCURRENCY="${COMPETITIVE_CONCURRENCY:-16}"
COMPETITIVE_DURATION="${COMPETITIVE_DURATION:-10}"
LOSS_PERCENT="${LOSS_PERCENT:-0}"
RTT_MS="${RTT_MS:-0}"
JITTER_MS="${JITTER_MS:-0}"

cleanup_tun() {
    if [ -f "${RUNTIME_NAME}.pid" ]; then
        kill "$(cat "${RUNTIME_NAME}.pid")" 2>/dev/null || true
    fi
    if [ -f "${VARIANT}-cpu.pid" ]; then
        kill "$(cat "${VARIANT}-cpu.pid")" 2>/dev/null || true
    fi
    ip link del bw-tun-i 2>/dev/null || true
    ip link del sb-tun-i 2>/dev/null || true
    ip route del default dev bw-tun-i table 100 2>/dev/null || true
    while ip rule del not fwmark 0x1234 lookup 100 2>/dev/null; do :; done
    iptables -t nat -D OUTPUT -p udp --dport 53 -j REDIRECT --to-port 15300 2>/dev/null || true
    ip6tables -t nat -D OUTPUT -p udp --dport 53 -j REDIRECT --to-port 15300 2>/dev/null || true
    ip route flush cache 2>/dev/null || true
}

trap cleanup_tun EXIT
cleanup_tun

bash -lc "timeout 120s ${RUNTIME_CMD}" > "${RUNTIME_NAME}.log" 2>&1 &
echo "$!" > "${RUNTIME_NAME}.pid"
sleep 2

if ! kill -0 "$(cat "${RUNTIME_NAME}.pid")" 2>/dev/null; then
    reason="$(sed -n '1,40p' "${RUNTIME_NAME}.log" | tr '\n' ' ')"
    python3 - "$VARIANT" "$SCENARIO" "$TUN_TCP_PAYLOAD" "$TS" "$reason" <<'PY'
import json, sys
variant, scenario, payload, ts, reason = sys.argv[1:]
print(json.dumps({
  "timestamp": ts, "variant": variant, "scenario": scenario, "protocol": "direct",
  "transport": "tun", "profile": "tun", "payload_size": payload, "concurrency": 16,
  "duration": 10, "keepalive_on": True, "loss_percent": 0, "rtt_ms": 0, "jitter_ms": 0,
  "bandwidth_limit": "", "requests_per_sec": 0, "throughput_mbps": 0, "ttfb_p50": 0,
  "ttfb_p90": 0, "ttfb_p95": 0, "ttfb_p99": 0, "ttfb_p999": 0, "latency_p50": 0,
  "latency_p90": 0, "latency_p95": 0, "latency_p99": 0, "latency_p999": 0,
  "cpu_user": 0, "cpu_system": 0, "cpu_percent": 0, "rss_mb": 0,
  "allocations_per_sec": 0, "syscalls_per_sec": 0, "bytes_up": 0, "bytes_down": 0,
  "errors": 1, "handshake_failures": 0, "reconnect_time_ms": 0, "route_time_us": 0,
  "dns_time_us": 0, "relay_path": "", "status": "failed",
  "reason": "TUN runtime exited before probes: " + reason
}, separators=(",", ":")))
PY
    exit 0
fi

python3 direct_udp_bench.py \
    --host "$SERVER_HOST" \
    --port "$TUN_UDP_ECHO_PORT" \
    --count "$TUN_UDP_COUNT" \
    --payload-bytes "$TUN_UDP_PAYLOAD_BYTES" \
    --timeout-ms "$TUN_UDP_TIMEOUT_MS" \
    --variant "$VARIANT" \
    --scenario "$SCENARIO" \
    --timestamp "$TS" \
    --loss-percent "$LOSS_PERCENT" \
    --rtt-ms "$RTT_MS" \
    --jitter-ms "$JITTER_MS" | tee "${VARIANT}-tun-udp.raw.log"

pid="$(cat "${RUNTIME_NAME}.pid")"
(while kill -0 "$pid" 2>/dev/null; do ps -p "$pid" -o pcpu=,rss= 2>/dev/null; sleep 0.2; done) > "${VARIANT}-cpu.log" 2>/dev/null &
echo "$!" > "${VARIANT}-cpu.pid"

target="${TUN_TCP_TARGET_URL:-http://${SERVER_HOST}:${COMPETITIVE_REMOTE_UPSTREAM_PORT}/${TUN_TCP_PAYLOAD}}"
curl_status=0
if curl_raw="$(curl -fsS --max-time "$TUN_TCP_TIMEOUT_SEC" -o /dev/null -w 'speed_download=%{speed_download}\ntime_total=%{time_total}\nsize_download=%{size_download}\n' "$target" 2>&1)"; then
    printf '%s\n' "$curl_raw" > "${VARIANT}-tun-tcp.raw.log"
else
    curl_status=$?
    printf '%s\n' "$curl_raw" > "${VARIANT}-tun-tcp.raw.log"
fi

if [ -f "${VARIANT}-cpu.pid" ]; then
    kill "$(cat "${VARIANT}-cpu.pid")" 2>/dev/null || true
fi

RAW_TEXT="$curl_raw" python3 - "$VARIANT" "$SCENARIO" "$TUN_TCP_PAYLOAD" "$COMPETITIVE_CONCURRENCY" "$COMPETITIVE_DURATION" "$TS" "$LOSS_PERCENT" "$RTT_MS" "$JITTER_MS" "${VARIANT}-cpu.log" "$curl_status" <<'PY'
import json, os, re, sys
variant, scenario, payload, conc, duration, ts, loss, rtt, jitter, cpu_file, curl_status = sys.argv[1:]
raw = os.environ.get("RAW_TEXT", "")
def val(name):
    m = re.search(rf"{name}=([0-9.]+)", raw)
    return float(m.group(1)) if m else 0.0
cpu_vals = []
rss_vals = []
try:
    for line in open(cpu_file):
        parts = line.split()
        if parts:
            cpu_vals.append(float(parts[0]))
        if len(parts) > 1:
            rss_vals.append(float(parts[1]) / 1024.0)
except FileNotFoundError:
    pass
speed_bps = val("speed_download")
elapsed = val("time_total")
size = val("size_download")
ok = int(curl_status) == 0 and speed_bps > 0
row = {
    "timestamp": ts, "variant": variant, "scenario": scenario,
    "protocol": "direct", "transport": "tun-tcp", "profile": "tun",
    "payload_size": payload, "concurrency": int(conc), "duration": float(duration),
    "keepalive_on": True, "loss_percent": float(loss), "rtt_ms": float(rtt),
    "jitter_ms": float(jitter), "bandwidth_limit": "",
    "requests_per_sec": 1.0 / elapsed if elapsed else 0,
    "throughput_mbps": round(speed_bps * 8.0 / 1_000_000.0, 4),
    "ttfb_p50": 0, "ttfb_p90": 0, "ttfb_p95": 0, "ttfb_p99": 0, "ttfb_p999": 0,
    "latency_p50": elapsed, "latency_p90": elapsed, "latency_p95": elapsed,
    "latency_p99": elapsed, "latency_p999": elapsed, "cpu_user": 0, "cpu_system": 0,
    "cpu_percent": round(sum(cpu_vals) / len(cpu_vals), 3) if cpu_vals else 0,
    "rss_mb": round(max(rss_vals), 3) if rss_vals else 0,
    "allocations_per_sec": 0, "syscalls_per_sec": 0, "bytes_up": 0,
    "bytes_down": int(size), "errors": 0 if ok else 1,
    "handshake_failures": 0, "reconnect_time_ms": 0, "route_time_us": 0,
    "dns_time_us": 0, "relay_path": "", "status": "ok" if ok else "failed",
    "reason": "" if ok else raw[:200],
}
print(json.dumps(row, separators=(",", ":")))
PY
