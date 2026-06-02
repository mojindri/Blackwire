#!/usr/bin/env bash
set -euo pipefail

SCENARIO="${1:-smoke}"
LAB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$(cd "$LAB_DIR/../.." && pwd)"
CONFIG_DIR="$LAB_DIR/configs"
REPORT_DIR="${REPORT_DIR:-$LAB_DIR/reports}"
MODE="${COMPETITIVE_MODE:-local}"
TS="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="$REPORT_DIR/${SCENARIO}-${MODE}-${TS}.jsonl"

BLACKWIRE_BIN_WAS_DEFAULT=0
if [ -z "${BLACKWIRE_BIN:-}" ]; then
    BLACKWIRE_BIN="$ROOT/target/release/blackwire"
    BLACKWIRE_BIN_WAS_DEFAULT=1
fi
BLACKWIRE_CURRENT_BIN="${BLACKWIRE_CURRENT_BIN:-$BLACKWIRE_BIN}"
BLACKWIRE_CANDIDATE_WAS_DEFAULT=0
if [ -z "${BLACKWIRE_CANDIDATE_BIN:-}" ]; then
    BLACKWIRE_CANDIDATE_BIN="$BLACKWIRE_BIN"
    BLACKWIRE_CANDIDATE_WAS_DEFAULT=1
fi
XRAY_BIN="${XRAY_BIN:-xray}"
SING_BOX_BIN="${SING_BOX_BIN:-sing-box}"
HYSTERIA_BIN="${HYSTERIA_BIN:-hysteria}"
SHOES_BIN="${SHOES_BIN:-shoes}"
HEY_BIN="${HEY_BIN:-hey}"
COMPETITIVE_DURATION="${COMPETITIVE_DURATION:-10}"
COMPETITIVE_CONCURRENCY="${COMPETITIVE_CONCURRENCY:-16}"
COMPETITIVE_PAYLOADS="${COMPETITIVE_PAYLOADS:-1k}"
COMPETITIVE_UPSTREAM_URL="${COMPETITIVE_UPSTREAM_URL:-}"
COMPETITIVE_EXPENSIVE_UPSTREAM_URL="${COMPETITIVE_EXPENSIVE_UPSTREAM_URL:-https://www.microsoft.com}"
COMPETITIVE_UPSTREAM_KIND="${COMPETITIVE_UPSTREAM_KIND:-auto}"
COMPETITIVE_REMOTE_UPSTREAM_PORT="${COMPETITIVE_REMOTE_UPSTREAM_PORT:-18080}"
LOSS_PERCENT="${LOSS_PERCENT:-0}"
RTT_MS="${RTT_MS:-0}"
JITTER_MS="${JITTER_MS:-0}"
BANDWIDTH_LIMIT="${BANDWIDTH_LIMIT:-}"
HYSTERIA2_CANDIDATE_MODE="${HYSTERIA2_CANDIDATE_MODE:-badnet-low-latency}"
HYSTERIA2_CANDIDATE_MIN_ACK_RATE="${HYSTERIA2_CANDIDATE_MIN_ACK_RATE:-0.90}"
HYSTERIA2_CANDIDATE_MAX_QUEUE_DELAY_MS="${HYSTERIA2_CANDIDATE_MAX_QUEUE_DELAY_MS:-50}"
HYSTERIA2_CANDIDATE_PACING_GAIN="${HYSTERIA2_CANDIDATE_PACING_GAIN:-0.90}"
HYSTERIA2_CANDIDATE_ENDPOINT_SHARDS="${HYSTERIA2_CANDIDATE_ENDPOINT_SHARDS:-4}"
HYSTERIA2_CANDIDATE_VARIANT="${HYSTERIA2_CANDIDATE_VARIANT:-blackwire-candidate-${HYSTERIA2_CANDIDATE_MODE}}"
HYSTERIA2_CANDIDATE_FEC_MODE="${HYSTERIA2_CANDIDATE_FEC_MODE:-auto}"
HYSTERIA2_UDP_COUNT="${HYSTERIA2_UDP_COUNT:-500}"
HYSTERIA2_UDP_CONCURRENCY="${HYSTERIA2_UDP_CONCURRENCY:-1}"
HYSTERIA2_UDP_PAYLOAD_BYTES="${HYSTERIA2_UDP_PAYLOAD_BYTES:-64}"
HYSTERIA2_UDP_ECHO_PORT="${HYSTERIA2_UDP_ECHO_PORT:-1053}"
HYSTERIA2_UDP_TIMEOUT_MS="${HYSTERIA2_UDP_TIMEOUT_MS:-3000}"
TUN_UDP_COUNT="${TUN_UDP_COUNT:-500}"
TUN_UDP_PAYLOAD_BYTES="${TUN_UDP_PAYLOAD_BYTES:-64}"
TUN_UDP_ECHO_PORT="${TUN_UDP_ECHO_PORT:-1056}"
TUN_UDP_TIMEOUT_MS="${TUN_UDP_TIMEOUT_MS:-3000}"
TUN_TCP_PAYLOAD="${TUN_TCP_PAYLOAD:-64m}"

SERVER_HOST="${COMPETITIVE_SERVER_HOST:-91.107.164.107}"
CLIENT_HOST="${COMPETITIVE_CLIENT_HOST:-91.107.176.118}"
SSH_USER="${COMPETITIVE_SSH_USER:-root}"
SSH_KEY="${COMPETITIVE_SSH_KEY:-id_hetzner}"
SSH_OPTS=(-i "$SSH_KEY" -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new)

if [ "$MODE" = "remote" ] && [ "$BLACKWIRE_BIN_WAS_DEFAULT" = "1" ] && [ -x "$ROOT/target/linux-amd64/blackwire" ]; then
    BLACKWIRE_BIN="$ROOT/target/linux-amd64/blackwire"
    BLACKWIRE_CURRENT_BIN="$BLACKWIRE_BIN"
    if [ "$BLACKWIRE_CANDIDATE_WAS_DEFAULT" = "1" ]; then
        BLACKWIRE_CANDIDATE_BIN="$BLACKWIRE_BIN"
    fi
fi

mkdir -p "$REPORT_DIR"

json_escape() {
    python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))'
}

emit_row() {
    local variant="$1" status="$2" reason="$3" protocol="$4" transport="$5" profile="$6" payload="$7"
    local reason_json
    reason_json="$(printf '%s' "$reason" | json_escape)"
    cat >> "$OUT" <<EOF
{"timestamp":"$TS","variant":"$variant","scenario":"$SCENARIO","protocol":"$protocol","transport":"$transport","profile":"$profile","payload_size":"$payload","concurrency":$COMPETITIVE_CONCURRENCY,"duration":$COMPETITIVE_DURATION,"keepalive_on":true,"loss_percent":$LOSS_PERCENT,"rtt_ms":$RTT_MS,"jitter_ms":$JITTER_MS,"bandwidth_limit":"$BANDWIDTH_LIMIT","requests_per_sec":0,"throughput_mbps":0,"ttfb_p50":0,"ttfb_p90":0,"ttfb_p95":0,"ttfb_p99":0,"ttfb_p999":0,"latency_p50":0,"latency_p90":0,"latency_p95":0,"latency_p99":0,"latency_p999":0,"cpu_user":0,"cpu_system":0,"cpu_percent":0,"rss_mb":0,"allocations_per_sec":0,"syscalls_per_sec":0,"bytes_up":0,"bytes_down":0,"errors":0,"handshake_failures":0,"reconnect_time_ms":0,"route_time_us":0,"dns_time_us":0,"relay_path":"","status":"$status","reason":$reason_json}
EOF
}

parse_hey() {
    local variant="$1" payload="$2" raw="$3" protocol="$4" transport="$5" profile="$6"
    RAW_TEXT="$raw" python3 - "$variant" "$SCENARIO" "$payload" "$COMPETITIVE_CONCURRENCY" "$COMPETITIVE_DURATION" "$protocol" "$transport" "$profile" "$TS" <<'PY' >> "$OUT"
import json, re, sys
import os
variant, scenario, payload, conc, duration, protocol, transport, profile, ts = sys.argv[1:]
raw = os.environ.get("RAW_TEXT", "")
def first(pattern, default=0.0):
    m = re.search(pattern, raw, re.M)
    return float(m.group(1)) if m else default
def pct(p):
    return first(rf"\s{p}%+ in ([0-9.]+) secs")
errors = 0
in_errors = False
for line in raw.splitlines():
    if "Error distribution:" in line:
        in_errors = True
        continue
    if in_errors:
        m = re.match(r"\s*\[(\d+)\]", line)
        if m:
            errors += int(m.group(1))
row = {
    "timestamp": ts, "variant": variant, "scenario": scenario,
    "protocol": protocol, "transport": transport, "profile": profile,
    "payload_size": payload, "concurrency": int(conc), "duration": int(duration),
    "keepalive_on": True,
    "loss_percent": float(os.environ.get("LOSS_PERCENT", "0")),
    "rtt_ms": float(os.environ.get("RTT_MS", "0")),
    "jitter_ms": float(os.environ.get("JITTER_MS", "0")),
    "bandwidth_limit": os.environ.get("BANDWIDTH_LIMIT", ""),
    "requests_per_sec": first(r"Requests/sec:\s+([0-9.]+)"),
    "throughput_mbps": 0, "ttfb_p50": 0, "ttfb_p90": 0, "ttfb_p95": 0,
    "ttfb_p99": 0, "ttfb_p999": 0, "latency_p50": pct(50),
    "latency_p90": pct(90), "latency_p95": pct(95), "latency_p99": pct(99),
    "latency_p999": pct(99), "cpu_user": 0, "cpu_system": 0, "cpu_percent": 0,
    "rss_mb": 0, "allocations_per_sec": 0, "syscalls_per_sec": 0,
    "bytes_up": 0, "bytes_down": 0, "errors": errors, "handshake_failures": 0,
    "reconnect_time_ms": 0, "route_time_us": 0, "dns_time_us": 0,
    "relay_path": "", "status": "ok" if errors == 0 else "failed", "reason": ""
}
print(json.dumps(row, separators=(",", ":")))
PY
}

port_open() {
    local port="$1"
    if command -v nc >/dev/null 2>&1; then
        nc -z 127.0.0.1 "$port" >/dev/null 2>&1
    else
        (echo >/dev/tcp/127.0.0.1/"$port") >/dev/null 2>&1
    fi
}

start_proc() {
    local name="$1" cmd="$2" port="${3:-}"
    bash -lc "$cmd" >"$REPORT_DIR/${name}-${TS}.log" 2>&1 &
    local pid=$!
    if [ -n "$port" ]; then
        for _ in $(seq 1 40); do
            port_open "$port" && { echo "$pid"; return 0; }
            sleep 0.25
        done
        kill "$pid" 2>/dev/null || true
        echo "ERROR: $name did not open port $port" >&2
        return 1
    fi
    sleep 0.5
    echo "$pid"
}

run_hey() {
    local variant="$1" payload="$2" proxy="${3:-}" protocol="${4:-direct}" transport="${5:-tcp}" profile="${6:-baseline}"
    if ! command -v "$HEY_BIN" >/dev/null 2>&1; then
        emit_row "$variant" "skipped" "hey not found" "$protocol" "$transport" "$profile" "$payload"
        return
    fi
    local args=(-z "${COMPETITIVE_DURATION}s" -c "$COMPETITIVE_CONCURRENCY")
    [ -n "$proxy" ] && args+=(-x "socks5://$proxy")
    local raw
    local target="${COMPETITIVE_UPSTREAM_URL:-http://127.0.0.1:18080}/$payload"
    if raw="$("$HEY_BIN" "${args[@]}" "$target" 2>&1)"; then
        parse_hey "$variant" "$payload" "$raw" "$protocol" "$transport" "$profile"
    else
        emit_row "$variant" "failed" "$raw" "$protocol" "$transport" "$profile" "$payload"
    fi
}

run_local() {
    echo "competitive run: $SCENARIO ($MODE) -> $OUT"
    if [[ "$SCENARIO" =~ ^(hysteria2-|loss-|mobile-) ]]; then
        for payload in $COMPETITIVE_PAYLOADS; do
            emit_row "blackwire-current-hysteria2" "skipped" "local Hysteria2 badnet runner requires remote VPS or explicit Hysteria2 client/server orchestration" "hysteria2" "quic" "current" "$payload"
            emit_row "blackwire-candidate-hysteria2" "skipped" "local Hysteria2 badnet runner requires remote VPS or explicit Hysteria2 client/server orchestration" "hysteria2" "quic" "badnet" "$payload"
            command -v "$HYSTERIA_BIN" >/dev/null 2>&1 \
                && emit_row "hysteria" "skipped" "official Hysteria binary present; local badnet netem runner is remote-only for now" "hysteria2" "quic" "baseline" "$payload" \
                || emit_row "hysteria" "skipped" "HYSTERIA_BIN not found: $HYSTERIA_BIN" "hysteria2" "quic" "baseline" "$payload"
        done
        return
    fi
    if [[ "$SCENARIO" =~ ^(tun|quic|expensive)$ ]]; then
        for payload in $COMPETITIVE_PAYLOADS; do
            emit_row "blackwire-current" "skipped" "scenario scaffolded; protocol-specific runner not implemented in Milestone A" "mixed" "$SCENARIO" "baseline" "$payload"
            emit_row "xray" "skipped" "scenario scaffolded; protocol-specific runner not implemented in Milestone A" "mixed" "$SCENARIO" "baseline" "$payload"
            emit_row "sing-box" "skipped" "scenario scaffolded; protocol-specific runner not implemented in Milestone A" "mixed" "$SCENARIO" "baseline" "$payload"
        done
        return
    fi

    UPSTREAM_PID=""
    if [ -z "$COMPETITIVE_UPSTREAM_URL" ]; then
        if [ "$COMPETITIVE_UPSTREAM_KIND" = "nginx" ] || { [ "$COMPETITIVE_UPSTREAM_KIND" = "auto" ] && command -v nginx >/dev/null 2>&1; }; then
            bash "$LAB_DIR/scripts/start_nginx_upstream.sh" "$REPORT_DIR/nginx-${TS}" 18080
            UPSTREAM_PID="$(cat "$REPORT_DIR/nginx-${TS}/nginx.pid")"
        else
            UPSTREAM_PID="$(start_proc upstream "python3 '$LAB_DIR/scripts/upstream_static.py' --host 127.0.0.1 --port 18080" 18080)"
        fi
    fi
    cleanup() { kill "${UPSTREAM_PID:-}" ${PIDS:-} 2>/dev/null || true; }
    trap cleanup EXIT

    for payload in $COMPETITIVE_PAYLOADS; do
        run_hey direct "$payload" "" direct tcp baseline

        if [ -x "$BLACKWIRE_CURRENT_BIN" ]; then
            local s c
            s="$(start_proc bw-current-server "'$BLACKWIRE_CURRENT_BIN' run -c '$CONFIG_DIR/blackwire/vless-server.json'" 10080)" || { emit_row blackwire-current failed "server did not start" vless tcp current "$payload"; continue; }
            c="$(start_proc bw-current-client "'$BLACKWIRE_CURRENT_BIN' run -c '$CONFIG_DIR/blackwire/vless-client.json'" 1081)" || { kill "$s" 2>/dev/null || true; emit_row blackwire-current failed "client did not start" vless tcp current "$payload"; continue; }
            PIDS="${PIDS:-} $s $c"
            run_hey blackwire-current "$payload" 127.0.0.1:1081 vless tcp current
            kill "$s" "$c" 2>/dev/null || true
        else
            emit_row blackwire-current skipped "BLACKWIRE_CURRENT_BIN not executable: $BLACKWIRE_CURRENT_BIN" vless tcp current "$payload"
        fi

        if [ -x "$BLACKWIRE_CANDIDATE_BIN" ]; then
            local s2 c2
            s2="$(start_proc bw-candidate-server "'$BLACKWIRE_CANDIDATE_BIN' run -c '$CONFIG_DIR/blackwire/vless-server-candidate.json'" 10090)" || { emit_row blackwire-candidate failed "server did not start" vless tcp candidate "$payload"; continue; }
            c2="$(start_proc bw-candidate-client "'$BLACKWIRE_CANDIDATE_BIN' run -c '$CONFIG_DIR/blackwire/vless-client-candidate.json'" 1091)" || { kill "$s2" 2>/dev/null || true; emit_row blackwire-candidate failed "client did not start" vless tcp candidate "$payload"; continue; }
            PIDS="${PIDS:-} $s2 $c2"
            run_hey blackwire-candidate "$payload" 127.0.0.1:1091 vless tcp candidate
            kill "$s2" "$c2" 2>/dev/null || true
        else
            emit_row blackwire-candidate skipped "BLACKWIRE_CANDIDATE_BIN not executable: $BLACKWIRE_CANDIDATE_BIN" vless tcp candidate "$payload"
        fi

        if command -v "$XRAY_BIN" >/dev/null 2>&1; then
            local xs xc
            xs="$(start_proc xray-server "$XRAY_BIN run -config '$CONFIG_DIR/xray/vless-server.json'" 10180)" || { emit_row xray failed "server did not start" vless tcp baseline "$payload"; continue; }
            xc="$(start_proc xray-client "$XRAY_BIN run -config '$CONFIG_DIR/xray/vless-client.json'" 1082)" || { kill "$xs" 2>/dev/null || true; emit_row xray failed "client did not start" vless tcp baseline "$payload"; continue; }
            PIDS="${PIDS:-} $xs $xc"
            run_hey xray "$payload" 127.0.0.1:1082 vless tcp baseline
            kill "$xs" "$xc" 2>/dev/null || true
        else
            emit_row xray skipped "XRAY_BIN not found: $XRAY_BIN" vless tcp baseline "$payload"
        fi

        if command -v "$SING_BOX_BIN" >/dev/null 2>&1; then
            local ss sc
            ss="$(start_proc singbox-server "$SING_BOX_BIN run -c '$CONFIG_DIR/sing-box/vless-server.json'" 10182)" || { emit_row sing-box failed "server did not start" vless tcp baseline "$payload"; continue; }
            sc="$(start_proc singbox-client "$SING_BOX_BIN run -c '$CONFIG_DIR/sing-box/vless-client.json'" 1083)" || { kill "$ss" 2>/dev/null || true; emit_row sing-box failed "client did not start" vless tcp baseline "$payload"; continue; }
            PIDS="${PIDS:-} $ss $sc"
            run_hey sing-box "$payload" 127.0.0.1:1083 vless tcp baseline
            kill "$ss" "$sc" 2>/dev/null || true
        else
            emit_row sing-box skipped "SING_BOX_BIN not found: $SING_BOX_BIN" vless tcp baseline "$payload"
        fi

        command -v "$HYSTERIA_BIN" >/dev/null 2>&1 \
            && emit_row hysteria skipped "binary found but Hysteria protocol row is scaffold-only in Milestone A" hysteria2 quic baseline "$payload" \
            || emit_row hysteria skipped "HYSTERIA_BIN not found: $HYSTERIA_BIN" hysteria2 quic baseline "$payload"
        command -v "$SHOES_BIN" >/dev/null 2>&1 \
            && emit_row shoes skipped "binary found but Shoes protocol row is scaffold-only in Milestone A" socks tcp baseline "$payload" \
            || emit_row shoes skipped "SHOES_BIN not found: $SHOES_BIN" socks tcp baseline "$payload"
    done
}

run_remote() {
    echo "competitive remote inventory: server=$SERVER_HOST client=$CLIENT_HOST -> $OUT"
    local server_inv client_inv
    server_inv="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" 'hostname; uname -a; for b in blackwire xray sing-box hysteria shoes hey oha iperf3 tc perf strace nginx; do p="$(command -v "$b" || true)"; printf "%s=%s\n" "$b" "$p"; done' 2>&1 || true)"
    client_inv="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" 'hostname; uname -a; for b in blackwire xray sing-box hysteria shoes hey oha iperf3 tc perf strace nginx; do p="$(command -v "$b" || true)"; printf "%s=%s\n" "$b" "$p"; done' 2>&1 || true)"
    printf '%s\n\n%s\n' "$server_inv" "$client_inv" > "$REPORT_DIR/remote-inventory-${TS}.log"
    has_tool() {
        local inventory="$1" tool="$2"
        grep -Eq "^${tool}=/" <<<"$inventory"
    }
    remote_port_open() {
        local host="$1" port="$2"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "for i in \$(seq 1 40); do (echo >/dev/tcp/127.0.0.1/$port) >/dev/null 2>&1 && exit 0; sleep 0.25; done; exit 1" >/dev/null 2>&1
    }
    remote_default_iface() {
        local host="$1"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if(\$i==\"dev\") {print \$(i+1); exit}}'" 2>/dev/null || true
    }
    remote_netem_apply() {
        local host="$1" iface="$2"
        [ -n "$iface" ] || return 0
        local args=()
        [ "${RTT_MS:-0}" != "0" ] && args+=(delay "${RTT_MS}ms")
        [ "${JITTER_MS:-0}" != "0" ] && args+=("${JITTER_MS}ms")
        [ "${LOSS_PERCENT:-0}" != "0" ] && args+=(loss "${LOSS_PERCENT}%")
        if [ "${#args[@]}" -gt 0 ]; then
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "tc qdisc replace dev '$iface' root netem ${args[*]}" >/dev/null 2>&1 || true
        fi
    }
    remote_qdisc_snapshot() {
        local stage="$1" host="$2" iface="$3" label="$4"
        [ -n "$iface" ] || return 0
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "echo 'tc qdisc show'; tc qdisc show dev '$iface' 2>&1 || true; echo; echo 'tc -s qdisc show'; tc -s qdisc show dev '$iface' 2>&1 || true" \
            > "$REPORT_DIR/${SCENARIO}-${label}-qdisc-${stage}-${TS}.log" 2>&1 || true
    }
    remote_netem_clear() {
        local host="$1" iface="$2"
        [ -n "$iface" ] || return 0
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "tc qdisc del dev '$iface' root >/dev/null 2>&1 || true" >/dev/null 2>&1 || true
    }
    remote_start() {
        local host="$1" dir="$2" name="$3" cmd="$4" port="${5:-}"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "cd '$dir'; nohup bash -lc '$cmd' > '$name.log' 2>&1 & echo \$! > '$name.pid'"
        if [ -n "$port" ]; then
            remote_port_open "$host" "$port"
        else
            sleep 0.5
        fi
    }
    remote_stop() {
        local host="$1" dir="$2"
        [ -n "$dir" ] || return 0
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "find '$dir' -name '*.pid' -type f -print0 2>/dev/null | while IFS= read -r -d '' p; do kill \$(cat \"\$p\") 2>/dev/null || true; done; rm -rf '$dir'" >/dev/null 2>&1 || true
    }
    remote_capture_metrics() {
        local host="$1" port="$2" variant="$3" role="$4"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "curl -fsS 'http://127.0.0.1:$port/metrics' 2>/dev/null || true" \
            > "$REPORT_DIR/${variant}-${role}-metrics-${TS}.log" 2>&1 || true
    }
    remote_capture_proc_stats() {
        local host="$1" dir="$2" name="$3" variant="$4" role="$5"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "if [ -f '$dir/$name.pid' ]; then pid=\$(cat '$dir/$name.pid'); ps -p \"\$pid\" -o pid=,pcpu=,rss=,comm= 2>/dev/null || true; fi" \
            > "$REPORT_DIR/${variant}-${role}-proc-${TS}.log" 2>&1 || true
    }
    remote_tun_control_peer() {
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "printf '%s\n' \"\${SSH_CLIENT%% *}\"" 2>/dev/null || true
    }
    remote_tun_safety_add() {
        local peer="$1"
        [ -n "$peer" ] || return 0
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "ip rule add priority 50 to '$peer' lookup main 2>/dev/null || true; ip route flush cache 2>/dev/null || true" >/dev/null 2>&1 || true
    }
    remote_tun_safety_del() {
        local peer="$1"
        [ -n "$peer" ] || return 0
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "while ip rule del priority 50 to '$peer' lookup main 2>/dev/null; do :; done; ip route flush cache 2>/dev/null || true" >/dev/null 2>&1 || true
    }
    remote_tun_cleanup_client() {
        local peer="$1"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "pkill -f 'blackwire.*blackwire-tun' 2>/dev/null || true; pkill -f 'sing-box run -c singbox-tun' 2>/dev/null || true; ip link del bw-tun-i 2>/dev/null || true; ip link del sb-tun-i 2>/dev/null || true; ip route del default dev bw-tun-i table 100 2>/dev/null || true; while ip rule del not fwmark 0x1234 lookup 100 2>/dev/null; do :; done; iptables -t nat -D OUTPUT -p udp --dport 53 -j REDIRECT --to-port 15300 2>/dev/null || true; ip6tables -t nat -D OUTPUT -p udp --dport 53 -j REDIRECT --to-port 15300 2>/dev/null || true; ip route flush cache 2>/dev/null || true" >/dev/null 2>&1 || true
        remote_tun_safety_del "$peer"
    }
    remote_start_cpu_sampler() {
        local host="$1" dir="$2" name="$3" variant="$4"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "cd '$dir'; pid=\$(cat '$name.pid'); (while kill -0 \"\$pid\" 2>/dev/null; do ps -p \"\$pid\" -o pcpu=,rss= 2>/dev/null; sleep 0.2; done) > '${variant}-cpu.log' 2>/dev/null & echo \$! > '${variant}-cpu.pid'" >/dev/null 2>&1 || true
    }
    remote_stop_cpu_sampler() {
        local host="$1" dir="$2" variant="$3"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$host" "cd '$dir'; if [ -f '${variant}-cpu.pid' ]; then kill \$(cat '${variant}-cpu.pid') 2>/dev/null || true; fi" >/dev/null 2>&1 || true
        scp "${SSH_OPTS[@]}" "$SSH_USER@$host:$dir/${variant}-cpu.log" "$REPORT_DIR/${variant}-cpu-${TS}.log" >/dev/null 2>&1 || true
    }
    cpu_avg_from_log() {
        local file="$1"
        python3 - "$file" <<'PY'
import sys
vals=[]
rss=[]
try:
    for line in open(sys.argv[1]):
        parts=line.split()
        if len(parts) >= 1:
            vals.append(float(parts[0]))
        if len(parts) >= 2:
            rss.append(float(parts[1]) / 1024.0)
except FileNotFoundError:
    pass
avg=sum(vals)/len(vals) if vals else 0.0
maxrss=max(rss) if rss else 0.0
print(f"{avg:.3f} {maxrss:.3f}")
PY
    }
    write_remote_tun_configs() {
        local peer="$1"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/blackwire-tun.json'" <<'EOF'
{"log":{"level":"warn"},"tun":{"name":"bw-tun-i","address":"198.18.20.1","netmask":"255.255.255.0","mtu":1500,"bypass_mark":4660,"redirect_port":17890,"dns_port":15300,"batch":{"enabled":true,"maxPackets":32,"maxDelayUs":750,"latencyFlushBytes":256},"sessions":{"udpMax":4096,"udpIdleTimeoutSec":60,"tcpMax":4096}},"inbounds":[{"tag":"tun-socks","protocol":"socks","listen":"127.0.0.1","port":17890}],"outbounds":[{"tag":"direct","protocol":"freedom"}],"routing":{"rules":[]}}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/singbox-tun.json'" <<EOF
{"log":{"level":"warn"},"inbounds":[{"type":"tun","tag":"tun-in","interface_name":"sb-tun-i","address":["198.18.30.1/30"],"mtu":1500,"auto_route":true,"strict_route":true,"route_exclude_address":["$peer/32"]}],"outbounds":[{"type":"direct","tag":"direct"}],"route":{"auto_detect_interface":true,"final":"direct"}}
EOF
    }
    write_remote_configs() {
        local _server_dir="$1" _client_dir="$2"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/blackwire-server.json'" <<'EOF'
{"profile":"fast","fast":{"strictProduction":false,"pool":"disabled","splice":"adaptive"},"log":{"level":"warn"},"inbounds":[{"tag":"vless-in","protocol":"vless","listen":"0.0.0.0","port":10080,"settings":{"clients":[{"id":"00000000-0000-4000-8000-000000000001"}]}}],"outbounds":[{"tag":"freedom","protocol":"freedom"}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/blackwire-client.json'" <<EOF
{"log":{"level":"warn"},"inbounds":[{"tag":"socks-in","protocol":"socks","listen":"127.0.0.1","port":1081}],"outbounds":[{"tag":"vless-out","protocol":"vless","settings":{"address":"$SERVER_HOST","port":10080,"users":[{"id":"00000000-0000-4000-8000-000000000001","flow":""}]}}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/blackwire-server-candidate.json'" <<'EOF'
{"profile":"fast","fast":{"strictProduction":false,"pool":"disabled","splice":"adaptive","relay":{"engine":"v2","flush":"deferred","initialBuffer":16384,"maxBuffer":262144}},"log":{"level":"warn"},"inbounds":[{"tag":"vless-in","protocol":"vless","listen":"0.0.0.0","port":10090,"settings":{"clients":[{"id":"00000000-0000-4000-8000-000000000001"}]}}],"outbounds":[{"tag":"freedom","protocol":"freedom"}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/blackwire-client-candidate.json'" <<EOF
{"log":{"level":"warn"},"inbounds":[{"tag":"socks-in","protocol":"socks","listen":"127.0.0.1","port":1091}],"outbounds":[{"tag":"vless-out","protocol":"vless","settings":{"address":"$SERVER_HOST","port":10090,"users":[{"id":"00000000-0000-4000-8000-000000000001","flow":""}]}}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/blackwire-hysteria2-server.json'" <<'EOF'
{"log":{"level":"warn"},"metricsAddr":"127.0.0.1:19000","inbounds":[{"tag":"hy2-in","protocol":"hysteria2","listen":"0.0.0.0","port":10300,"settings":{"auth":"blackwire-lab","upMbps":100,"downMbps":100},"streamSettings":{"network":"quic","security":"tls","tlsSettings":{"certificateFile":"server.crt","keyFile":"server.key"}}}],"outbounds":[{"tag":"freedom","protocol":"freedom"}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/blackwire-hysteria2-client.json'" <<EOF
{"log":{"level":"warn"},"metricsAddr":"127.0.0.1:19001","inbounds":[{"tag":"socks-in","protocol":"socks","listen":"127.0.0.1","port":1088}],"outbounds":[{"tag":"hy2-out","protocol":"hysteria2","settings":{"server":"$SERVER_HOST:10300","serverName":"$SERVER_HOST","auth":"blackwire-lab","upMbps":100,"downMbps":100,"skipCertVerify":true,"congestion":{"mode":"brutal-compatible","minAckRate":0.8,"maxQueueDelayMs":80,"pacingGain":1.25,"lossCompensation":true},"endpointShards":1}}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/blackwire-hysteria2-server-candidate.json'" <<EOF
{"log":{"level":"warn"},"metricsAddr":"127.0.0.1:19010","quic":{"reusePort":true,"endpoints":"cpu","recvBufferBytes":8388608,"sendBufferBytes":8388608,"maxDatagramSize":"auto"},"datagram":{"enabled":true,"udpOverDatagram":true,"tunPacketsOverDatagram":true,"policy":"h2-plus","fastDnsRetry":false,"fastDnsRetryDelayMs":20},"fec":{"mode":"$HYSTERIA2_CANDIDATE_FEC_MODE","maxOverheadPercent":20,"protectClasses":["dns","interactive","control"],"avoidBulkTcp":true},"inbounds":[{"tag":"hy2-in","protocol":"hysteria2","listen":"0.0.0.0","port":10310,"settings":{"auth":"blackwire-lab","upMbps":100,"downMbps":100,"congestion":{"mode":"$HYSTERIA2_CANDIDATE_MODE","minAckRate":$HYSTERIA2_CANDIDATE_MIN_ACK_RATE,"maxQueueDelayMs":$HYSTERIA2_CANDIDATE_MAX_QUEUE_DELAY_MS,"pacingGain":$HYSTERIA2_CANDIDATE_PACING_GAIN,"lossCompensation":true},"datagram":{"policy":"h2-plus","fastDnsRetry":false,"fastDnsRetryDelayMs":20},"fec":{"mode":"$HYSTERIA2_CANDIDATE_FEC_MODE","maxOverheadPercent":20}},"streamSettings":{"network":"quic","security":"tls","tlsSettings":{"certificateFile":"server.crt","keyFile":"server.key"}}}],"outbounds":[{"tag":"freedom","protocol":"freedom"}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/blackwire-hysteria2-client-candidate.json'" <<EOF
{"log":{"level":"warn"},"metricsAddr":"127.0.0.1:19011","quic":{"reusePort":false,"endpoints":$HYSTERIA2_CANDIDATE_ENDPOINT_SHARDS,"recvBufferBytes":8388608,"sendBufferBytes":8388608,"maxDatagramSize":"auto"},"datagram":{"enabled":true,"udpOverDatagram":true,"tunPacketsOverDatagram":true,"policy":"h2-plus","fastDnsRetry":false,"fastDnsRetryDelayMs":20},"fec":{"mode":"$HYSTERIA2_CANDIDATE_FEC_MODE","maxOverheadPercent":20,"protectClasses":["dns","interactive","control"],"avoidBulkTcp":true},"inbounds":[{"tag":"socks-in","protocol":"socks","listen":"127.0.0.1","port":1098}],"outbounds":[{"tag":"hy2-out","protocol":"hysteria2","settings":{"server":"$SERVER_HOST:10310","serverName":"$SERVER_HOST","auth":"blackwire-lab","upMbps":100,"downMbps":100,"skipCertVerify":true,"congestion":{"mode":"$HYSTERIA2_CANDIDATE_MODE","minAckRate":$HYSTERIA2_CANDIDATE_MIN_ACK_RATE,"maxQueueDelayMs":$HYSTERIA2_CANDIDATE_MAX_QUEUE_DELAY_MS,"pacingGain":$HYSTERIA2_CANDIDATE_PACING_GAIN,"lossCompensation":true},"endpointShards":$HYSTERIA2_CANDIDATE_ENDPOINT_SHARDS,"datagram":{"policy":"h2-plus","fastDnsRetry":false,"fastDnsRetryDelayMs":20},"fec":{"mode":"$HYSTERIA2_CANDIDATE_FEC_MODE","maxOverheadPercent":20}}}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/blackwire-vision-server.json'" <<'EOF'
{"profile":"fast","fast":{"strictProduction":false,"pool":"disabled","splice":"adaptive"},"log":{"level":"warn"},"inbounds":[{"tag":"vless-vision-in","protocol":"vless","listen":"0.0.0.0","port":10082,"settings":{"clients":[{"id":"1791A4CD-09E3-4A29-A36D-FEA98300C845","email":"lab","flow":"xtls-rprx-vision"}]},"streamSettings":{"network":"tcp","security":"reality","realitySettings":{"dest":"127.0.0.1:18443","serverName":"www.microsoft.com","privateKey":"6f4850ca51ced64b4acfd90c73fd60392c0c2f92744933b28b1bc0f7b8683d79","shortIds":["aabbccdd00000001"]}}}],"outbounds":[{"tag":"freedom","protocol":"freedom"}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/blackwire-vision-server-candidate.json'" <<'EOF'
{"profile":"fast","fast":{"strictProduction":false,"pool":"disabled","splice":"adaptive","relay":{"engine":"v2","flush":"deferred","initialBuffer":16384,"maxBuffer":262144}},"vision":{"directCopy":"auto","maxPacketsToFilter":8,"allowSpliceAfterDirect":true},"log":{"level":"warn"},"inbounds":[{"tag":"vless-vision-in","protocol":"vless","listen":"0.0.0.0","port":10092,"settings":{"clients":[{"id":"1791A4CD-09E3-4A29-A36D-FEA98300C845","email":"lab","flow":"xtls-rprx-vision"}]},"streamSettings":{"network":"tcp","security":"reality","realitySettings":{"dest":"127.0.0.1:18443","serverName":"www.microsoft.com","privateKey":"6f4850ca51ced64b4acfd90c73fd60392c0c2f92744933b28b1bc0f7b8683d79","shortIds":["aabbccdd00000001"]}}}],"outbounds":[{"tag":"freedom","protocol":"freedom"}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/xray-vision-current-client.json'" <<EOF
{"log":{"loglevel":"warning"},"inbounds":[{"tag":"http-in","listen":"127.0.0.1","port":1086,"protocol":"http"}],"outbounds":[{"tag":"blackwire-vision-out","protocol":"vless","settings":{"vnext":[{"address":"$SERVER_HOST","port":10082,"users":[{"id":"1791A4CD-09E3-4A29-A36D-FEA98300C845","encryption":"none","flow":"xtls-rprx-vision"}]}]},"streamSettings":{"network":"tcp","security":"reality","realitySettings":{"fingerprint":"chrome","serverName":"www.microsoft.com","publicKey":"loYSsUliNDpTJ_ISdh6Q3A3fMc7TnaQfuDlpS-K46Wo","shortId":"aabbccdd00000001"}}}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/xray-vision-candidate-client.json'" <<EOF
{"log":{"loglevel":"warning"},"inbounds":[{"tag":"http-in","listen":"127.0.0.1","port":1096,"protocol":"http"}],"outbounds":[{"tag":"blackwire-vision-out","protocol":"vless","settings":{"vnext":[{"address":"$SERVER_HOST","port":10092,"users":[{"id":"1791A4CD-09E3-4A29-A36D-FEA98300C845","encryption":"none","flow":"xtls-rprx-vision"}]}]},"streamSettings":{"network":"tcp","security":"reality","realitySettings":{"fingerprint":"chrome","serverName":"www.microsoft.com","publicKey":"loYSsUliNDpTJ_ISdh6Q3A3fMc7TnaQfuDlpS-K46Wo","shortId":"aabbccdd00000001"}}}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/xray-vision-server.json'" <<'EOF'
{"log":{"loglevel":"warning"},"inbounds":[{"tag":"vless-vision-in","protocol":"vless","listen":"0.0.0.0","port":10192,"settings":{"clients":[{"id":"1791A4CD-09E3-4A29-A36D-FEA98300C845","flow":"xtls-rprx-vision"}],"decryption":"none"},"streamSettings":{"network":"tcp","security":"reality","realitySettings":{"dest":"127.0.0.1:18443","serverNames":["www.microsoft.com"],"privateKey":"QHBt24zKYs1NgFCWM0OP0RFLMpUKUOX3wB3J9aGUb0c","shortIds":["aabbccdd00000001"]}}}],"outbounds":[{"tag":"direct","protocol":"freedom"}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/xray-vision-client.json'" <<EOF
{"log":{"loglevel":"warning"},"inbounds":[{"tag":"http-in","listen":"127.0.0.1","port":1087,"protocol":"http"}],"outbounds":[{"tag":"xray-vision-out","protocol":"vless","settings":{"vnext":[{"address":"$SERVER_HOST","port":10192,"users":[{"id":"1791A4CD-09E3-4A29-A36D-FEA98300C845","encryption":"none","flow":"xtls-rprx-vision"}]}]},"streamSettings":{"network":"tcp","security":"reality","realitySettings":{"fingerprint":"chrome","serverName":"www.microsoft.com","publicKey":"YLCM0wOiSrxzyGYQuQKeQct-4gKm5MLrLy4RAH6--1w","shortId":"aabbccdd00000001"}}}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/xray-server.json'" <<'EOF'
{"log":{"loglevel":"warning"},"inbounds":[{"tag":"vless-in","protocol":"vless","listen":"0.0.0.0","port":10180,"settings":{"clients":[{"id":"00000000-0000-4000-8000-000000000001","flow":""}],"decryption":"none"}}],"outbounds":[{"tag":"direct","protocol":"freedom"}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/xray-client.json'" <<EOF
{"log":{"loglevel":"warning"},"inbounds":[{"tag":"socks-in","protocol":"socks","listen":"127.0.0.1","port":1082,"settings":{"auth":"noauth"}}],"outbounds":[{"tag":"vless-out","protocol":"vless","settings":{"vnext":[{"address":"$SERVER_HOST","port":10180,"users":[{"id":"00000000-0000-4000-8000-000000000001","encryption":"none","flow":""}]}]}}]}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/singbox-server.json'" <<'EOF'
{"log":{"level":"warn"},"inbounds":[{"type":"vless","tag":"vless-in","listen":"0.0.0.0","listen_port":10182,"users":[{"uuid":"00000000-0000-4000-8000-000000000001"}]}],"outbounds":[{"type":"direct","tag":"direct"}],"route":{"final":"direct"}}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/singbox-client.json'" <<EOF
{"log":{"level":"warn"},"inbounds":[{"type":"socks","tag":"socks-in","listen":"127.0.0.1","listen_port":1083}],"outbounds":[{"type":"vless","tag":"vless-out","server":"$SERVER_HOST","server_port":10182,"uuid":"00000000-0000-4000-8000-000000000001"}],"route":{"final":"vless-out"}}
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/hysteria-server.yaml'" <<'EOF'
listen: :10200
tls:
  cert: server.crt
  key: server.key
auth:
  type: password
  password: blackwire-lab
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/hysteria-client.yaml'" <<EOF
server: $SERVER_HOST:10200
auth: blackwire-lab
tls:
  insecure: true
socks5:
  listen: 127.0.0.1:1084
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cat > '$REMOTE_SERVER_DIR/shoes-server.yaml'" <<'EOF'
- address: 0.0.0.0:10202
  protocol:
    type: vless
    user_id: 00000000-0000-4000-8000-000000000001
EOF
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cat > '$REMOTE_CLIENT_DIR/shoes-client.yaml'" <<EOF
- address: 127.0.0.1:1085
  protocol:
    type: socks
  rules:
    - masks: "0.0.0.0/0"
      action: allow
      client_chain:
        address: "$SERVER_HOST:10202"
        protocol:
          type: vless
          user_id: 00000000-0000-4000-8000-000000000001
EOF
    }
    run_remote_hey() {
        local variant="$1" payload="$2" proxy="$3" protocol="$4" transport="$5" profile="$6"
        local target="http://$SERVER_HOST:$COMPETITIVE_REMOTE_UPSTREAM_PORT/$payload"
        if [ "$SCENARIO" = "expensive" ]; then
            target="$COMPETITIVE_EXPENSIVE_UPSTREAM_URL"
        fi
        local cmd="hey -z '${COMPETITIVE_DURATION}s' -c '$COMPETITIVE_CONCURRENCY'"
        if [ -n "$proxy" ]; then
            if [[ "$proxy" == *"://"* ]]; then
                cmd="$cmd -x '$proxy'"
            else
                cmd="$cmd -x socks5://127.0.0.1:$proxy"
            fi
        fi
        local raw
        if raw="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "$cmd '$target'" 2>&1)"; then
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-${payload}-${TS}.raw.log"
            parse_hey "$variant" "$payload" "$raw" "$protocol" "$transport" "$profile"
        else
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-${payload}-${TS}.raw.log"
            emit_row "$variant" failed "$raw" "$protocol" "$transport" "$profile" "$payload"
        fi
    }
    run_remote_hy2_udp_bench() {
        local bin="$1" variant="$2" port="$3" policy="$4" fec_mode="$5"
        local cmd="./$bin hy2-udp-bench --server '$SERVER_HOST:$port' --server-name '$SERVER_HOST' --auth blackwire-lab --skip-cert-verify --dest-host 127.0.0.1 --dest-port '$HYSTERIA2_UDP_ECHO_PORT' --count '$HYSTERIA2_UDP_COUNT' --concurrency '$HYSTERIA2_UDP_CONCURRENCY' --payload-bytes '$HYSTERIA2_UDP_PAYLOAD_BYTES' --timeout-ms '$HYSTERIA2_UDP_TIMEOUT_MS' --mode '$HYSTERIA2_CANDIDATE_MODE' --endpoint-shards '$HYSTERIA2_CANDIDATE_ENDPOINT_SHARDS' --datagram-policy '$policy' --fec-mode '$fec_mode' --fec-overhead-percent 20 --variant '$variant' --scenario '$SCENARIO'"
        local raw
        if raw="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cd '$REMOTE_CLIENT_DIR'; $cmd" 2>&1)"; then
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-udp-${TS}.raw.log"
            printf '%s\n' "$raw" >> "$OUT"
        else
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-udp-${TS}.raw.log"
            emit_row "$variant" failed "$raw" hysteria2 quic-datagram "$policy" "${HYSTERIA2_UDP_PAYLOAD_BYTES}b"
        fi
    }
    run_remote_socks_udp_bench() {
        local variant="$1" socks_port="$2"
        local raw
        if raw="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cd '$REMOTE_CLIENT_DIR'; python3 socks5_udp_bench.py --socks-port '$socks_port' --dest-host 127.0.0.1 --dest-port '$HYSTERIA2_UDP_ECHO_PORT' --count '$HYSTERIA2_UDP_COUNT' --concurrency '$HYSTERIA2_UDP_CONCURRENCY' --payload-bytes '$HYSTERIA2_UDP_PAYLOAD_BYTES' --timeout-ms '$HYSTERIA2_UDP_TIMEOUT_MS' --variant '$variant' --scenario '$SCENARIO'" 2>&1)"; then
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-udp-${TS}.raw.log"
            printf '%s\n' "$raw" >> "$OUT"
        else
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-udp-${TS}.raw.log"
            emit_row "$variant" failed "$raw" hysteria2 quic-datagram baseline "${HYSTERIA2_UDP_PAYLOAD_BYTES}b"
        fi
    }
    run_remote_hy2_udp_mix_bench() {
        local bin="$1" variant="$2" port="$3" policy="$4" fec_mode="$5"
        local cmd="./$bin hy2-udp-mix-bench --server '$SERVER_HOST:$port' --server-name '$SERVER_HOST' --auth blackwire-lab --skip-cert-verify --dest-host 127.0.0.1 --dest-port '$HYSTERIA2_UDP_ECHO_PORT' --count '$HYSTERIA2_UDP_COUNT' --concurrency '$HYSTERIA2_UDP_CONCURRENCY' --payload-bytes '$HYSTERIA2_UDP_PAYLOAD_BYTES' --timeout-ms '$HYSTERIA2_UDP_TIMEOUT_MS' --mode '$HYSTERIA2_CANDIDATE_MODE' --endpoint-shards '$HYSTERIA2_CANDIDATE_ENDPOINT_SHARDS' --datagram-policy '$policy' --fec-mode '$fec_mode' --fec-overhead-percent 20 --variant '$variant' --scenario '$SCENARIO' --dns-port 5353 --interactive-port 1054 --bulk-port 1055 --dns-count '$HYSTERIA2_UDP_COUNT' --interactive-count '$HYSTERIA2_UDP_COUNT' --bulk-count '$((HYSTERIA2_UDP_COUNT * 4))' --bulk-payload-bytes 1200"
        local raw
        if raw="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cd '$REMOTE_CLIENT_DIR'; $cmd" 2>&1)"; then
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-innerflow-${TS}.raw.log"
            printf '%s\n' "$raw" >> "$OUT"
        else
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-innerflow-${TS}.raw.log"
            emit_row "$variant" failed "$raw" hysteria2 quic-datagram "$policy" "mixed"
        fi
    }
    run_remote_tun_udp_bench() {
        local variant="$1"
        local raw
        if raw="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cd '$REMOTE_CLIENT_DIR'; python3 direct_udp_bench.py --host '$SERVER_HOST' --port '$TUN_UDP_ECHO_PORT' --count '$TUN_UDP_COUNT' --payload-bytes '$TUN_UDP_PAYLOAD_BYTES' --timeout-ms '$TUN_UDP_TIMEOUT_MS' --variant '$variant' --scenario '$SCENARIO' --timestamp '$TS' --loss-percent '$LOSS_PERCENT' --rtt-ms '$RTT_MS' --jitter-ms '$JITTER_MS'" 2>&1)"; then
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-tun-udp-${TS}.raw.log"
            printf '%s\n' "$raw" >> "$OUT"
        else
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-tun-udp-${TS}.raw.log"
            emit_row "$variant" failed "$raw" direct tun-udp tun "${TUN_UDP_PAYLOAD_BYTES}b"
        fi
    }
    run_remote_tun_tcp_bench() {
        local variant="$1" proc_name="$2"
        local target="http://$SERVER_HOST:$COMPETITIVE_REMOTE_UPSTREAM_PORT/$TUN_TCP_PAYLOAD"
        remote_start_cpu_sampler "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" "$proc_name" "$variant"
        local raw
        if raw="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "curl -fsS -o /dev/null -w 'speed_download=%{speed_download}\ntime_total=%{time_total}\nsize_download=%{size_download}\n' '$target'" 2>&1)"; then
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-tun-tcp-${TS}.raw.log"
        else
            printf '%s\n' "$raw" > "$REPORT_DIR/${variant}-tun-tcp-${TS}.raw.log"
            remote_stop_cpu_sampler "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" "$variant"
            emit_row "$variant" failed "$raw" direct tun-tcp tun "$TUN_TCP_PAYLOAD"
            return
        fi
        remote_stop_cpu_sampler "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" "$variant"
        local cpu_stats cpu_avg rss_mb
        cpu_stats="$(cpu_avg_from_log "$REPORT_DIR/${variant}-cpu-${TS}.log")"
        cpu_avg="${cpu_stats%% *}"
        rss_mb="${cpu_stats##* }"
        RAW_TEXT="$raw" python3 - "$variant" "$SCENARIO" "$TUN_TCP_PAYLOAD" "$COMPETITIVE_CONCURRENCY" "$COMPETITIVE_DURATION" "$TS" "$LOSS_PERCENT" "$RTT_MS" "$JITTER_MS" "$cpu_avg" "$rss_mb" <<'PY' >> "$OUT"
import json, os, re, sys
variant, scenario, payload, conc, duration, ts, loss, rtt, jitter, cpu, rss = sys.argv[1:]
raw = os.environ.get("RAW_TEXT", "")
def val(name):
    m = re.search(rf"{name}=([0-9.]+)", raw)
    return float(m.group(1)) if m else 0.0
speed_bps = val("speed_download")
size = val("size_download")
elapsed = val("time_total")
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
    "latency_p99": elapsed, "latency_p999": elapsed,
    "cpu_user": 0, "cpu_system": 0, "cpu_percent": float(cpu), "rss_mb": float(rss),
    "allocations_per_sec": 0, "syscalls_per_sec": 0, "bytes_up": 0,
    "bytes_down": int(size), "errors": 0 if speed_bps > 0 else 1,
    "handshake_failures": 0, "reconnect_time_ms": 0, "route_time_us": 0,
    "dns_time_us": 0, "relay_path": "", "status": "ok" if speed_bps > 0 else "failed",
    "reason": "" if speed_bps > 0 else "curl reported zero throughput",
}
print(json.dumps(row, separators=(",", ":")))
PY
    }
    REMOTE_SERVER_DIR="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" 'mktemp -d /tmp/blackwire-competitive.XXXXXX')"
    REMOTE_CLIENT_DIR="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" 'mktemp -d /tmp/blackwire-competitive.XXXXXX')"
    local nginx_started=0
    cleanup_remote_dirs() {
        remote_netem_clear "$SERVER_HOST" "${SERVER_IFACE:-}"
        remote_netem_clear "$CLIENT_HOST" "${CLIENT_IFACE:-}"
        remote_stop "$SERVER_HOST" "${REMOTE_SERVER_DIR:-}"
        remote_stop "$CLIENT_HOST" "${REMOTE_CLIENT_DIR:-}"
    }
    trap cleanup_remote_dirs EXIT
    ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "ufw allow ${COMPETITIVE_REMOTE_UPSTREAM_PORT}/tcp >/dev/null; ufw allow 10080/tcp >/dev/null; ufw allow 10082/tcp >/dev/null; ufw allow 10090/tcp >/dev/null; ufw allow 10092/tcp >/dev/null; ufw allow 10180/tcp >/dev/null; ufw allow 10182/tcp >/dev/null; ufw allow 10192/tcp >/dev/null; ufw allow 10200/udp >/dev/null; ufw allow 10202/tcp >/dev/null; ufw allow 10300/udp >/dev/null; ufw allow 10310/udp >/dev/null" >/dev/null 2>&1 || true
    scp "${SSH_OPTS[@]}" "$LAB_DIR/scripts/start_nginx_upstream.sh" "$SSH_USER@$SERVER_HOST:$REMOTE_SERVER_DIR/start_nginx_upstream.sh" >/dev/null
    scp "${SSH_OPTS[@]}" "$LAB_DIR/scripts/udp_echo.py" "$SSH_USER@$SERVER_HOST:$REMOTE_SERVER_DIR/udp_echo.py" >/dev/null
    scp "${SSH_OPTS[@]}" "$LAB_DIR/scripts/socks5_udp_bench.py" "$SSH_USER@$CLIENT_HOST:$REMOTE_CLIENT_DIR/socks5_udp_bench.py" >/dev/null
    scp "${SSH_OPTS[@]}" "$LAB_DIR/scripts/direct_udp_bench.py" "$SSH_USER@$CLIENT_HOST:$REMOTE_CLIENT_DIR/direct_udp_bench.py" >/dev/null
    scp "${SSH_OPTS[@]}" "$LAB_DIR/scripts/tun_remote_once.sh" "$SSH_USER@$CLIENT_HOST:$REMOTE_CLIENT_DIR/tun_remote_once.sh" >/dev/null
    ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "chmod +x '$REMOTE_CLIENT_DIR/tun_remote_once.sh'" >/dev/null 2>&1 || true
    write_remote_configs "$REMOTE_SERVER_DIR" "$REMOTE_CLIENT_DIR"
    if ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "bash '$REMOTE_SERVER_DIR/start_nginx_upstream.sh' '$REMOTE_SERVER_DIR/nginx' '$COMPETITIVE_REMOTE_UPSTREAM_PORT' 0.0.0.0" >/dev/null 2>&1; then
        nginx_started=1
    fi
    ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cd '$REMOTE_SERVER_DIR'; if command -v openssl >/dev/null 2>&1; then openssl req -x509 -newkey rsa:2048 -nodes -keyout server.key -out server.crt -subj '/CN=$SERVER_HOST' -days 1 >/dev/null 2>&1; elif command -v hysteria >/dev/null 2>&1; then hysteria cert --host '$SERVER_HOST' --cert server.crt --key server.key --overwrite >/dev/null 2>&1; fi" >/dev/null 2>&1 || true
    if [ -x "$BLACKWIRE_BIN" ]; then
        scp "${SSH_OPTS[@]}" "$BLACKWIRE_BIN" "$SSH_USER@$SERVER_HOST:$REMOTE_SERVER_DIR/blackwire" >/dev/null || true
        scp "${SSH_OPTS[@]}" "$BLACKWIRE_BIN" "$SSH_USER@$CLIENT_HOST:$REMOTE_CLIENT_DIR/blackwire" >/dev/null || true
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "chmod +x '$REMOTE_SERVER_DIR/blackwire'" >/dev/null 2>&1 || true
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "chmod +x '$REMOTE_CLIENT_DIR/blackwire'" >/dev/null 2>&1 || true
    fi
    if [ -x "$BLACKWIRE_CANDIDATE_BIN" ] && [ "$BLACKWIRE_CANDIDATE_BIN" != "$BLACKWIRE_BIN" ]; then
        scp "${SSH_OPTS[@]}" "$BLACKWIRE_CANDIDATE_BIN" "$SSH_USER@$SERVER_HOST:$REMOTE_SERVER_DIR/blackwire-candidate" >/dev/null || true
        scp "${SSH_OPTS[@]}" "$BLACKWIRE_CANDIDATE_BIN" "$SSH_USER@$CLIENT_HOST:$REMOTE_CLIENT_DIR/blackwire-candidate" >/dev/null || true
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "chmod +x '$REMOTE_SERVER_DIR/blackwire-candidate'" >/dev/null 2>&1 || true
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "chmod +x '$REMOTE_CLIENT_DIR/blackwire-candidate'" >/dev/null 2>&1 || true
    fi

    if [ "$SCENARIO" = "tun" ]; then
        local control_peer
        control_peer="$(remote_tun_control_peer)"
        write_remote_tun_configs "$control_peer"
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "ufw allow ${TUN_UDP_ECHO_PORT}/udp >/dev/null" >/dev/null 2>&1 || true
        remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" udp-echo-tun "python3 udp_echo.py --host 0.0.0.0 --port '$TUN_UDP_ECHO_PORT'" ""
        emit_row remote-inventory ok "wrote remote-inventory-${TS}.log; control peer ${control_peer:-unknown}" inventory ssh tun "$TUN_TCP_PAYLOAD"

        if [ -x "$BLACKWIRE_CANDIDATE_BIN" ] && [ "$nginx_started" = "1" ]; then
            remote_tun_cleanup_client "$control_peer"
            remote_tun_safety_add "$control_peer"
            if raw="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cd '$REMOTE_CLIENT_DIR'; VARIANT=blackwire-candidate-tun RUNTIME_NAME=blackwire-tun RUNTIME_CMD='./blackwire-candidate run -c blackwire-tun.json' SERVER_HOST='$SERVER_HOST' TS='$TS' SCENARIO='$SCENARIO' TUN_UDP_ECHO_PORT='$TUN_UDP_ECHO_PORT' TUN_UDP_COUNT='$TUN_UDP_COUNT' TUN_UDP_PAYLOAD_BYTES='$TUN_UDP_PAYLOAD_BYTES' TUN_UDP_TIMEOUT_MS='$TUN_UDP_TIMEOUT_MS' COMPETITIVE_REMOTE_UPSTREAM_PORT='$COMPETITIVE_REMOTE_UPSTREAM_PORT' TUN_TCP_PAYLOAD='$TUN_TCP_PAYLOAD' COMPETITIVE_CONCURRENCY='$COMPETITIVE_CONCURRENCY' COMPETITIVE_DURATION='$COMPETITIVE_DURATION' LOSS_PERCENT='$LOSS_PERCENT' RTT_MS='$RTT_MS' JITTER_MS='$JITTER_MS' ./tun_remote_once.sh" 2>&1)"; then
                printf '%s\n' "$raw" > "$REPORT_DIR/blackwire-candidate-tun-once-${TS}.raw.log"
                printf '%s\n' "$raw" >> "$OUT"
                scp "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST:$REMOTE_CLIENT_DIR/blackwire-candidate-tun-"'*.log' "$REPORT_DIR/" >/dev/null 2>&1 || true
                scp "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST:$REMOTE_CLIENT_DIR/blackwire-tun.log" "$REPORT_DIR/blackwire-tun-${TS}.log" >/dev/null 2>&1 || true
            else
                printf '%s\n' "$raw" > "$REPORT_DIR/blackwire-candidate-tun-once-${TS}.raw.log"
                emit_row blackwire-candidate-tun failed "$raw" direct tun tun "$TUN_TCP_PAYLOAD"
            fi
            remote_tun_cleanup_client "$control_peer"
        else
            emit_row blackwire-candidate-tun skipped "BLACKWIRE_CANDIDATE_BIN or nginx unavailable" direct tun tun "$TUN_TCP_PAYLOAD"
        fi

        if has_tool "$client_inv" "sing-box" && [ "$nginx_started" = "1" ]; then
            remote_tun_cleanup_client "$control_peer"
            remote_tun_safety_add "$control_peer"
            if ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cd '$REMOTE_CLIENT_DIR'; sing-box check -c singbox-tun.json" > "$REPORT_DIR/singbox-tun-check-${TS}.log" 2>&1 && raw="$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "cd '$REMOTE_CLIENT_DIR'; VARIANT=sing-box-tun RUNTIME_NAME=singbox-tun RUNTIME_CMD='sing-box run -c singbox-tun.json' SERVER_HOST='$SERVER_HOST' TS='$TS' SCENARIO='$SCENARIO' TUN_UDP_ECHO_PORT='$TUN_UDP_ECHO_PORT' TUN_UDP_COUNT='$TUN_UDP_COUNT' TUN_UDP_PAYLOAD_BYTES='$TUN_UDP_PAYLOAD_BYTES' TUN_UDP_TIMEOUT_MS='$TUN_UDP_TIMEOUT_MS' COMPETITIVE_REMOTE_UPSTREAM_PORT='$COMPETITIVE_REMOTE_UPSTREAM_PORT' TUN_TCP_PAYLOAD='$TUN_TCP_PAYLOAD' COMPETITIVE_CONCURRENCY='$COMPETITIVE_CONCURRENCY' COMPETITIVE_DURATION='$COMPETITIVE_DURATION' LOSS_PERCENT='$LOSS_PERCENT' RTT_MS='$RTT_MS' JITTER_MS='$JITTER_MS' ./tun_remote_once.sh" 2>&1)"; then
                printf '%s\n' "$raw" > "$REPORT_DIR/sing-box-tun-once-${TS}.raw.log"
                printf '%s\n' "$raw" >> "$OUT"
                scp "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST:$REMOTE_CLIENT_DIR/sing-box-tun-"'*.log' "$REPORT_DIR/" >/dev/null 2>&1 || true
                scp "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST:$REMOTE_CLIENT_DIR/singbox-tun.log" "$REPORT_DIR/singbox-tun-${TS}.log" >/dev/null 2>&1 || true
            else
                emit_row sing-box-tun failed "sing-box TUN runtime did not start or config check failed; see singbox-tun-check-${TS}.log" direct tun tun "$TUN_TCP_PAYLOAD"
            fi
            remote_tun_cleanup_client "$control_peer"
        else
            emit_row sing-box-tun skipped "sing-box or nginx unavailable on client/server" direct tun tun "$TUN_TCP_PAYLOAD"
        fi
        return
    fi

    if [[ "$SCENARIO" =~ ^hysteria2-innerflow ]]; then
        SERVER_IFACE="$(remote_default_iface "$SERVER_HOST")"
        CLIENT_IFACE="$(remote_default_iface "$CLIENT_HOST")"
        remote_qdisc_snapshot before "$SERVER_HOST" "$SERVER_IFACE" server
        remote_qdisc_snapshot before "$CLIENT_HOST" "$CLIENT_IFACE" client
        remote_netem_apply "$SERVER_HOST" "$SERVER_IFACE"
        remote_netem_apply "$CLIENT_HOST" "$CLIENT_IFACE"
        remote_qdisc_snapshot after-apply "$SERVER_HOST" "$SERVER_IFACE" server
        remote_qdisc_snapshot after-apply "$CLIENT_HOST" "$CLIENT_IFACE" client
        remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" udp-echo-dns "python3 udp_echo.py --host 127.0.0.1 --port 5353" ""
        remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" udp-echo-interactive "python3 udp_echo.py --host 127.0.0.1 --port 1054" ""
        remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" udp-echo-bulk "python3 udp_echo.py --host 127.0.0.1 --port 1055" ""
        emit_row remote-inventory ok "wrote remote-inventory-${TS}.log" inventory ssh innerflow mixed

        if [ -x "$BLACKWIRE_BIN" ]; then
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-hysteria2-server "./blackwire run -c blackwire-hysteria2-server.json" ""; then
                run_remote_hy2_udp_mix_bench blackwire-candidate blackwire-current-innerflow 10300 standard off
                remote_capture_metrics "$SERVER_HOST" 19000 blackwire-current-innerflow server
            else
                emit_row blackwire-current-innerflow failed "temporary Blackwire Hysteria2 server did not start" hysteria2 quic-datagram current mixed
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-hysteria2-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-hysteria2-server.pid'" >/dev/null 2>&1 || true
        else
            emit_row blackwire-current-innerflow skipped "BLACKWIRE_BIN not executable" hysteria2 quic-datagram current mixed
        fi

        if [ -x "$BLACKWIRE_CANDIDATE_BIN" ] && [ "$BLACKWIRE_CANDIDATE_BIN" != "$BLACKWIRE_BIN" ]; then
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-hysteria2-candidate-server "./blackwire-candidate run -c blackwire-hysteria2-server-candidate.json" ""; then
                run_remote_hy2_udp_mix_bench blackwire-candidate blackwire-candidate-innerflow 10310 h2-plus "$HYSTERIA2_CANDIDATE_FEC_MODE"
                remote_capture_metrics "$SERVER_HOST" 19010 blackwire-candidate-innerflow server
            else
                emit_row blackwire-candidate-innerflow failed "temporary Blackwire Hysteria2 candidate server did not start" hysteria2 quic-datagram h2-plus mixed
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-hysteria2-candidate-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-hysteria2-candidate-server.pid'" >/dev/null 2>&1 || true
        else
            emit_row blackwire-candidate-innerflow skipped "no distinct BLACKWIRE_CANDIDATE_BIN" hysteria2 quic-datagram h2-plus mixed
        fi
        remote_qdisc_snapshot after-run "$SERVER_HOST" "$SERVER_IFACE" server
        remote_qdisc_snapshot after-run "$CLIENT_HOST" "$CLIENT_IFACE" client
        return
    fi

    if [[ "$SCENARIO" =~ ^hysteria2-udp-dns ]]; then
        SERVER_IFACE="$(remote_default_iface "$SERVER_HOST")"
        CLIENT_IFACE="$(remote_default_iface "$CLIENT_HOST")"
        remote_qdisc_snapshot before "$SERVER_HOST" "$SERVER_IFACE" server
        remote_qdisc_snapshot before "$CLIENT_HOST" "$CLIENT_IFACE" client
        remote_netem_apply "$SERVER_HOST" "$SERVER_IFACE"
        remote_netem_apply "$CLIENT_HOST" "$CLIENT_IFACE"
        remote_qdisc_snapshot after-apply "$SERVER_HOST" "$SERVER_IFACE" server
        remote_qdisc_snapshot after-apply "$CLIENT_HOST" "$CLIENT_IFACE" client
        remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" udp-echo "python3 udp_echo.py --host 127.0.0.1 --port '$HYSTERIA2_UDP_ECHO_PORT'" ""
        emit_row remote-inventory ok "wrote remote-inventory-${TS}.log" inventory ssh udp-dns "${HYSTERIA2_UDP_PAYLOAD_BYTES}b"

        if [ -x "$BLACKWIRE_BIN" ]; then
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-hysteria2-server "./blackwire run -c blackwire-hysteria2-server.json" ""; then
                run_remote_hy2_udp_bench blackwire blackwire-current-hysteria2-udp 10300 standard off
                remote_capture_metrics "$SERVER_HOST" 19000 blackwire-current-hysteria2-udp server
            else
                emit_row blackwire-current-hysteria2-udp failed "temporary Blackwire Hysteria2 server did not start" hysteria2 quic-datagram current "${HYSTERIA2_UDP_PAYLOAD_BYTES}b"
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-hysteria2-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-hysteria2-server.pid'" >/dev/null 2>&1 || true
        else
            emit_row blackwire-current-hysteria2-udp skipped "BLACKWIRE_BIN not executable" hysteria2 quic-datagram current "${HYSTERIA2_UDP_PAYLOAD_BYTES}b"
        fi

        if [ -x "$BLACKWIRE_CANDIDATE_BIN" ] && [ "$BLACKWIRE_CANDIDATE_BIN" != "$BLACKWIRE_BIN" ]; then
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-hysteria2-candidate-server "./blackwire-candidate run -c blackwire-hysteria2-server-candidate.json" ""; then
                run_remote_hy2_udp_bench blackwire-candidate blackwire-candidate-h2plus-udp 10310 h2-plus "$HYSTERIA2_CANDIDATE_FEC_MODE"
                remote_capture_metrics "$SERVER_HOST" 19010 blackwire-candidate-h2plus-udp server
            else
                emit_row blackwire-candidate-h2plus-udp failed "temporary Blackwire Hysteria2 candidate server did not start" hysteria2 quic-datagram h2-plus "${HYSTERIA2_UDP_PAYLOAD_BYTES}b"
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-hysteria2-candidate-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-hysteria2-candidate-server.pid'" >/dev/null 2>&1 || true
        else
            emit_row blackwire-candidate-h2plus-udp skipped "no distinct BLACKWIRE_CANDIDATE_BIN" hysteria2 quic-datagram h2-plus "${HYSTERIA2_UDP_PAYLOAD_BYTES}b"
        fi

        if has_tool "$server_inv" "hysteria" && has_tool "$client_inv" "hysteria"; then
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cd '$REMOTE_SERVER_DIR'; hysteria cert --host '$SERVER_HOST' --cert server.crt --key server.key --overwrite >/dev/null 2>&1" || true
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" hysteria-server "hysteria server -c hysteria-server.yaml" "" && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" hysteria-client "hysteria client -c hysteria-client.yaml" 1084; then
                run_remote_socks_udp_bench hysteria-udp 1084
            else
                emit_row hysteria-udp failed "hysteria server/client did not become ready" hysteria2 quic-datagram baseline "${HYSTERIA2_UDP_PAYLOAD_BYTES}b"
            fi
        else
            emit_row hysteria-udp skipped "hysteria missing on at least one VPS" hysteria2 quic-datagram baseline "${HYSTERIA2_UDP_PAYLOAD_BYTES}b"
        fi
        remote_qdisc_snapshot after-run "$SERVER_HOST" "$SERVER_IFACE" server
        remote_qdisc_snapshot after-run "$CLIENT_HOST" "$CLIENT_IFACE" client
        return
    fi

    if [[ "$SCENARIO" =~ ^hysteria2- ]]; then
        SERVER_IFACE="$(remote_default_iface "$SERVER_HOST")"
        CLIENT_IFACE="$(remote_default_iface "$CLIENT_HOST")"
        remote_qdisc_snapshot before "$SERVER_HOST" "$SERVER_IFACE" server
        remote_qdisc_snapshot before "$CLIENT_HOST" "$CLIENT_IFACE" client
        remote_netem_apply "$SERVER_HOST" "$SERVER_IFACE"
        remote_netem_apply "$CLIENT_HOST" "$CLIENT_IFACE"
        remote_qdisc_snapshot after-apply "$SERVER_HOST" "$SERVER_IFACE" server
        remote_qdisc_snapshot after-apply "$CLIENT_HOST" "$CLIENT_IFACE" client
        for payload in $COMPETITIVE_PAYLOADS; do
            emit_row remote-inventory ok "wrote remote-inventory-${TS}.log" inventory ssh badnet "$payload"
            if [ -x "$BLACKWIRE_BIN" ] && [ "$nginx_started" = "1" ] && has_tool "$client_inv" "hey"; then
                if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-hysteria2-server "./blackwire run -c blackwire-hysteria2-server.json" "" && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" blackwire-hysteria2-client "./blackwire run -c blackwire-hysteria2-client.json" 1088; then
                    run_remote_hey blackwire-current-hysteria2 "$payload" 1088 hysteria2 quic current
                    remote_capture_metrics "$SERVER_HOST" 19000 blackwire-current-hysteria2 server
                    remote_capture_metrics "$CLIENT_HOST" 19001 blackwire-current-hysteria2 client
                    remote_capture_proc_stats "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-hysteria2-server blackwire-current-hysteria2 server
                    remote_capture_proc_stats "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" blackwire-hysteria2-client blackwire-current-hysteria2 client
                else
                    emit_row blackwire-current-hysteria2 failed "temporary Blackwire Hysteria2 server/client did not become ready" hysteria2 quic current "$payload"
                fi
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-hysteria2-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-hysteria2-server.pid'" >/dev/null 2>&1 || true
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/blackwire-hysteria2-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/blackwire-hysteria2-client.pid'" >/dev/null 2>&1 || true
            else
                emit_row blackwire-current-hysteria2 skipped "BLACKWIRE_BIN, hey, or nginx unavailable" hysteria2 quic current "$payload"
            fi

            if [ -x "$BLACKWIRE_CANDIDATE_BIN" ] && [ "$BLACKWIRE_CANDIDATE_BIN" != "$BLACKWIRE_BIN" ] && [ "$nginx_started" = "1" ] && has_tool "$client_inv" "hey"; then
                if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-hysteria2-candidate-server "./blackwire-candidate run -c blackwire-hysteria2-server-candidate.json" "" && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" blackwire-hysteria2-candidate-client "./blackwire-candidate run -c blackwire-hysteria2-client-candidate.json" 1098; then
                    run_remote_hey "$HYSTERIA2_CANDIDATE_VARIANT" "$payload" 1098 hysteria2 quic "$HYSTERIA2_CANDIDATE_MODE"
                    remote_capture_metrics "$SERVER_HOST" 19010 "$HYSTERIA2_CANDIDATE_VARIANT" server
                    remote_capture_metrics "$CLIENT_HOST" 19011 "$HYSTERIA2_CANDIDATE_VARIANT" client
                    remote_capture_proc_stats "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-hysteria2-candidate-server "$HYSTERIA2_CANDIDATE_VARIANT" server
                    remote_capture_proc_stats "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" blackwire-hysteria2-candidate-client "$HYSTERIA2_CANDIDATE_VARIANT" client
                else
                    emit_row "$HYSTERIA2_CANDIDATE_VARIANT" failed "temporary Blackwire candidate Hysteria2 server/client did not become ready" hysteria2 quic "$HYSTERIA2_CANDIDATE_MODE" "$payload"
                fi
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-hysteria2-candidate-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-hysteria2-candidate-server.pid'" >/dev/null 2>&1 || true
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/blackwire-hysteria2-candidate-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/blackwire-hysteria2-candidate-client.pid'" >/dev/null 2>&1 || true
            else
                emit_row "$HYSTERIA2_CANDIDATE_VARIANT" skipped "no distinct BLACKWIRE_CANDIDATE_BIN, hey, or nginx unavailable" hysteria2 quic "$HYSTERIA2_CANDIDATE_MODE" "$payload"
            fi

            if has_tool "$server_inv" "hysteria" && has_tool "$client_inv" "hysteria" && [ "$nginx_started" = "1" ] && has_tool "$client_inv" "hey"; then
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cd '$REMOTE_SERVER_DIR'; hysteria cert --host '$SERVER_HOST' --cert server.crt --key server.key --overwrite >/dev/null 2>&1" || true
                if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" hysteria-server "hysteria server -c hysteria-server.yaml" "" && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" hysteria-client "hysteria client -c hysteria-client.yaml" 1084; then
                    run_remote_hey hysteria "$payload" 1084 hysteria2 quic baseline
                else
                    emit_row hysteria failed "hysteria server/client did not become ready" hysteria2 quic baseline "$payload"
                fi
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/hysteria-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/hysteria-server.pid'" >/dev/null 2>&1 || true
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/hysteria-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/hysteria-client.pid'" >/dev/null 2>&1 || true
            else
                emit_row hysteria skipped "hysteria or hey missing on at least one VPS" hysteria2 quic baseline "$payload"
            fi
        done
        remote_qdisc_snapshot after-run "$SERVER_HOST" "$SERVER_IFACE" server
        remote_qdisc_snapshot after-run "$CLIENT_HOST" "$CLIENT_IFACE" client
        return
    fi

    if [ "$SCENARIO" = "expensive" ]; then
        for payload in $COMPETITIVE_PAYLOADS; do
            emit_row remote-inventory ok "wrote remote-inventory-${TS}.log" inventory ssh baseline "$payload"
            if [ "$nginx_started" = "1" ] && has_tool "$client_inv" "hey"; then
                run_remote_hey direct-vps-native "$payload" "" direct tcp baseline
            else
                emit_row direct-vps-native skipped "nginx on server or hey on client missing" direct tcp baseline "$payload"
            fi

            if [ -x "$BLACKWIRE_BIN" ] && has_tool "$client_inv" "xray" && [ "$nginx_started" = "1" ]; then
                if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-vision-server "./blackwire run -c blackwire-vision-server.json" 10082 && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" xray-vision-current-client "xray run -config xray-vision-current-client.json" 1086; then
                    run_remote_hey blackwire-current-vision "$payload" http://127.0.0.1:1086 vless reality-vision current
                else
                    emit_row blackwire-current-vision failed "temporary Blackwire Vision server or Xray client did not become ready" vless reality-vision current "$payload"
                fi
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-vision-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-vision-server.pid'" >/dev/null 2>&1 || true
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/xray-vision-current-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/xray-vision-current-client.pid'" >/dev/null 2>&1 || true
            else
                emit_row blackwire-current-vision skipped "BLACKWIRE_BIN, xray client, or nginx unavailable" vless reality-vision current "$payload"
            fi

            if [ -x "$BLACKWIRE_CANDIDATE_BIN" ] && [ "$BLACKWIRE_CANDIDATE_BIN" != "$BLACKWIRE_BIN" ] && has_tool "$client_inv" "xray" && [ "$nginx_started" = "1" ]; then
                if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-vision-candidate-server "./blackwire-candidate run -c blackwire-vision-server-candidate.json" 10092 && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" xray-vision-candidate-client "xray run -config xray-vision-candidate-client.json" 1096; then
                    run_remote_hey blackwire-candidate-vision "$payload" http://127.0.0.1:1096 vless reality-vision candidate
                else
                    emit_row blackwire-candidate-vision failed "temporary Blackwire candidate Vision server or Xray client did not become ready" vless reality-vision candidate "$payload"
                fi
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-vision-candidate-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-vision-candidate-server.pid'" >/dev/null 2>&1 || true
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/xray-vision-candidate-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/xray-vision-candidate-client.pid'" >/dev/null 2>&1 || true
            else
                emit_row blackwire-candidate-vision skipped "no distinct BLACKWIRE_CANDIDATE_BIN, xray client, or nginx unavailable" vless reality-vision candidate "$payload"
            fi

            if has_tool "$server_inv" "xray" && has_tool "$client_inv" "xray" && [ "$nginx_started" = "1" ]; then
                if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" xray-vision-server "xray run -config xray-vision-server.json" 10192 && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" xray-vision-client "xray run -config xray-vision-client.json" 1087; then
                    run_remote_hey xray-vision "$payload" http://127.0.0.1:1087 vless reality-vision baseline
                else
                    emit_row xray-vision failed "xray Vision server/client did not become ready" vless reality-vision baseline "$payload"
                fi
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/xray-vision-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/xray-vision-server.pid'" >/dev/null 2>&1 || true
                ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/xray-vision-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/xray-vision-client.pid'" >/dev/null 2>&1 || true
            else
                emit_row xray-vision skipped "xray missing on at least one VPS" vless reality-vision baseline "$payload"
            fi
        done
        return
    fi

    for payload in $COMPETITIVE_PAYLOADS; do
        emit_row remote-inventory ok "wrote remote-inventory-${TS}.log" inventory ssh baseline "$payload"
        if [ "$nginx_started" = "1" ] && has_tool "$client_inv" "hey"; then
            run_remote_hey direct-vps-native "$payload" "" direct tcp baseline
        else
            emit_row direct-vps-native skipped "nginx on server or hey on client missing" direct tcp baseline "$payload"
        fi
        if [ -x "$BLACKWIRE_BIN" ] && [ "$nginx_started" = "1" ]; then
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-server "./blackwire run -c blackwire-server.json" 10080 && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" blackwire-client "./blackwire run -c blackwire-client.json" 1081; then
                run_remote_hey blackwire-current "$payload" 1081 vless tcp current
            else
                emit_row blackwire-current failed "temporary Blackwire server/client did not become ready" vless tcp current "$payload"
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-server.pid'" >/dev/null 2>&1 || true
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/blackwire-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/blackwire-client.pid'" >/dev/null 2>&1 || true
        else
            emit_row blackwire-current skipped "local BLACKWIRE_BIN not executable or nginx upstream unavailable: $BLACKWIRE_BIN" vless tcp current "$payload"
        fi
        if [ -x "$BLACKWIRE_CANDIDATE_BIN" ] && [ "$BLACKWIRE_CANDIDATE_BIN" != "$BLACKWIRE_BIN" ] && [ "$nginx_started" = "1" ]; then
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" blackwire-candidate-server "./blackwire-candidate run -c blackwire-server-candidate.json" 10090 && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" blackwire-candidate-client "./blackwire-candidate run -c blackwire-client-candidate.json" 1091; then
                run_remote_hey blackwire-candidate "$payload" 1091 vless tcp candidate
            else
                emit_row blackwire-candidate failed "temporary Blackwire candidate server/client did not become ready" vless tcp candidate "$payload"
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/blackwire-candidate-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/blackwire-candidate-server.pid'" >/dev/null 2>&1 || true
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/blackwire-candidate-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/blackwire-candidate-client.pid'" >/dev/null 2>&1 || true
        else
            emit_row blackwire-candidate skipped "no distinct BLACKWIRE_CANDIDATE_BIN configured" vless tcp candidate "$payload"
        fi
        if has_tool "$server_inv" "xray" && has_tool "$client_inv" "xray" && [ "$nginx_started" = "1" ]; then
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" xray-server "xray run -config xray-server.json" 10180 && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" xray-client "xray run -config xray-client.json" 1082; then
                run_remote_hey xray "$payload" 1082 vless tcp baseline
            else
                emit_row xray failed "xray server/client did not become ready" vless tcp baseline "$payload"
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/xray-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/xray-server.pid'" >/dev/null 2>&1 || true
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/xray-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/xray-client.pid'" >/dev/null 2>&1 || true
        else
            emit_row xray skipped "xray missing on at least one VPS" vless tcp baseline "$payload"
        fi
        if has_tool "$server_inv" "sing-box" && has_tool "$client_inv" "sing-box" && [ "$nginx_started" = "1" ]; then
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" singbox-server "sing-box run -c singbox-server.json" 10182 && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" singbox-client "sing-box run -c singbox-client.json" 1083; then
                run_remote_hey sing-box "$payload" 1083 vless tcp baseline
            else
                emit_row sing-box failed "sing-box server/client did not become ready" vless tcp baseline "$payload"
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/singbox-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/singbox-server.pid'" >/dev/null 2>&1 || true
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/singbox-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/singbox-client.pid'" >/dev/null 2>&1 || true
        else
            emit_row sing-box skipped "sing-box missing on at least one VPS" vless tcp baseline "$payload"
        fi
        if has_tool "$server_inv" "hysteria" && has_tool "$client_inv" "hysteria" && [ "$nginx_started" = "1" ]; then
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "cd '$REMOTE_SERVER_DIR'; hysteria cert --host '$SERVER_HOST' --cert server.crt --key server.key --overwrite >/dev/null 2>&1" || true
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" hysteria-server "hysteria server -c hysteria-server.yaml" "" && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" hysteria-client "hysteria client -c hysteria-client.yaml" 1084; then
                run_remote_hey hysteria "$payload" 1084 hysteria2 quic baseline
            else
                emit_row hysteria failed "hysteria server/client did not become ready" hysteria2 quic baseline "$payload"
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/hysteria-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/hysteria-server.pid'" >/dev/null 2>&1 || true
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/hysteria-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/hysteria-client.pid'" >/dev/null 2>&1 || true
        else
            emit_row hysteria skipped "hysteria missing on at least one VPS" hysteria2 quic baseline "$payload"
        fi
        if has_tool "$server_inv" "shoes" && has_tool "$client_inv" "shoes" && [ "$nginx_started" = "1" ]; then
            if remote_start "$SERVER_HOST" "$REMOTE_SERVER_DIR" shoes-server "shoes --no-reload shoes-server.yaml" 10202 && remote_start "$CLIENT_HOST" "$REMOTE_CLIENT_DIR" shoes-client "shoes --no-reload shoes-client.yaml" 1085; then
                run_remote_hey shoes "$payload" 1085 vless tcp baseline
            else
                emit_row shoes failed "shoes server/client did not become ready" vless tcp baseline "$payload"
            fi
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$SERVER_HOST" "kill \$(cat '$REMOTE_SERVER_DIR/shoes-server.pid') 2>/dev/null || true; rm -f '$REMOTE_SERVER_DIR/shoes-server.pid'" >/dev/null 2>&1 || true
            ssh "${SSH_OPTS[@]}" "$SSH_USER@$CLIENT_HOST" "kill \$(cat '$REMOTE_CLIENT_DIR/shoes-client.pid') 2>/dev/null || true; rm -f '$REMOTE_CLIENT_DIR/shoes-client.pid'" >/dev/null 2>&1 || true
        else
            emit_row shoes skipped "shoes missing on at least one VPS" socks tcp baseline "$payload"
        fi
    done
}

case "$MODE" in
    local) run_local ;;
    remote) run_remote ;;
    *) echo "ERROR: COMPETITIVE_MODE must be local or remote, got $MODE" >&2; exit 1 ;;
esac

echo "wrote $OUT"
