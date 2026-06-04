#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# run_experiments.sh — driver for the two VPS performance experiments.
#
#   relay   Experiment 2: Fast-profile relay knobs (io_uring splice backend,
#           userspace flush policy) on a two-hop VLESS rig.
#               hey -> client(socks->vless) -> server(vless->freedom) -> origin
#   kernel  Experiment 1: reclaim kernel network/firewall overhead on the SOCKS
#           rig (conntrack NOTRACK, nft bypass upper-bound, NIC offloads, sysctls).
#               hey -> proxy(socks->freedom) -> origin
#   all     run both.
#
# Runs ON THE SERVER (proxy) host. The load generator runs locally, or on a
# remote client host if CLIENT_SSH is set (recommended: separate hosts so the
# proxy is the bottleneck). Pin the proxy with PROXY_CPU and the origin with
# ORIGIN_CPU.
#
# Key env vars (all optional, sane defaults):
#   SERVER_ADDR   address the client proxy dials for VLESS         (default 127.0.0.1)
#   CLIENT_SSH    e.g. "user@client-ip"; if set, hey runs there    (default: local)
#   LOAD          hey | python                                     (default: hey if present)
#   HEY_BIN       path to hey                                      (default: hey)
#   DURATION      seconds per run                                  (default 10)
#   CONC          concurrency                                      (default 8)
#   RUNS          repetitions per data point (median reported)     (default 3)
#   PROXY_CPU     taskset core list for the proxy under test       (default: none)
#   ORIGIN_CPU    taskset core list for the origin                 (default: none)
#   ORIGIN_PORT   origin port                                      (default 18080)
#   ORIGIN_CMD    override origin (e.g. point at an existing nginx); empty = python origin
#   IFACE         server NIC for kernel experiment                 (default: autodetect)
#   PERF          1 to capture perf top-15 where noted             (default 0)
#   OUT           output dir for raw logs                          (default /tmp/bw-exp)
# ─────────────────────────────────────────────────────────────────────────────
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAT_DIR="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$LAT_DIR/../../.." && pwd)"
CFG="$LAT_DIR/configs"
KNOBS="$CFG/fast-knobs"

BIN="${BLACKWIRE_BIN:-$ROOT/target/release/blackwire}"
SERVER_ADDR="${SERVER_ADDR:-127.0.0.1}"
CLIENT_SSH="${CLIENT_SSH:-}"
DURATION="${DURATION:-10}"
CONC="${CONC:-8}"
RUNS="${RUNS:-3}"
ORIGIN_PORT="${ORIGIN_PORT:-18080}"
ORIGIN_CMD="${ORIGIN_CMD:-}"
PROXY_CPU="${PROXY_CPU:-}"
ORIGIN_CPU="${ORIGIN_CPU:-}"
PERF="${PERF:-0}"
OUT="${OUT:-/tmp/bw-exp}"
HEY_BIN="${HEY_BIN:-hey}"
CLK="$(getconf CLK_TCK)"
mkdir -p "$OUT"

if [ -z "${LOAD:-}" ]; then
    if command -v "$HEY_BIN" >/dev/null 2>&1; then LOAD=hey; else LOAD=python; fi
fi

PIDS=()
cleanup() { for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

pin() { [ -n "$1" ] && echo "taskset -c $1" || true; }

start_origin() {
    if [ -n "$ORIGIN_CMD" ]; then
        echo "origin: external ($ORIGIN_CMD) on :$ORIGIN_PORT" >&2
        return
    fi
    $(pin "$ORIGIN_CPU") python3 "$HERE/origin_static.py" --port "$ORIGIN_PORT" >"$OUT/origin.log" 2>&1 &
    PIDS+=($!)
    sleep 0.4
}

# start_proxy <config> <listen_port> -> echoes the pid
start_proxy() {
    local config="$1" port="$2"
    $(pin "$PROXY_CPU") "$BIN" run -c "$config" >"$OUT/proxy.log" 2>&1 &
    local pid=$!
    PIDS+=("$pid")
    for _ in $(seq 1 60); do
        python3 -c "import socket;socket.create_connection(('127.0.0.1',$port),0.2)" 2>/dev/null && break
        sleep 0.1
    done
    echo "$pid"
}

stop_pid() { kill "$1" 2>/dev/null || true; wait "$1" 2>/dev/null || true; }

cpu_ticks() { awk '{print $14 + $15}' "/proc/$1/stat" 2>/dev/null || echo 0; }

# run_load <mode keepalive|churn> <payload e.g. 1k|100m> <socks_port> -> "req/s|p50|p95|p99"
run_load() {
    local mode="$1" payload="$2" port="$3"
    local url="http://${SERVER_ADDR}:${ORIGIN_PORT}/${payload}"
    # When the load runs remotely the proxy is reached at SERVER_ADDR; locally at 127.0.0.1.
    local proxy_host="127.0.0.1"; [ -n "$CLIENT_SSH" ] && proxy_host="$SERVER_ADDR"
    if [ "$LOAD" = hey ]; then
        local ka=""; [ "$mode" = churn ] && ka="-disable-keepalive"
        local cmd="$HEY_BIN -z ${DURATION}s -c ${CONC} $ka -x socks5://${proxy_host}:${port} '$url'"
        local raw
        if [ -n "$CLIENT_SSH" ]; then raw="$(ssh -o StrictHostKeyChecking=no "$CLIENT_SSH" "$cmd" 2>&1)"
        else raw="$(eval "$cmd" 2>&1)"; fi
        printf '%s\n' "$raw" >"$OUT/load-${mode}-${payload}.log"
        local rps p50 p95 p99
        rps="$(awk '/Requests\/sec:/{print $2}' <<<"$raw")"
        p50="$(awk '/50% in/{print $3*1000}' <<<"$raw")"
        p95="$(awk '/95% in/{print $3*1000}' <<<"$raw")"
        p99="$(awk '/99% in/{print $3*1000}' <<<"$raw")"
        echo "${rps:-0}|${p50:-0}|${p95:-0}|${p99:-0}"
    else
        # Python fallback (local only). NOTE: client-bound; use only for dry-runs.
        local m="keepalive"; [ "$mode" = churn ] && m="churn"
        local out
        out="$(python3 "$HERE/socks_loadgen.py" --proxy-port "$port" --dst-port "$ORIGIN_PORT" \
              --payload "$payload" --duration "$DURATION" --concurrency "$CONC" --mode "$m" --warmup 1 2>&1)"
        printf '%s\n' "$out" >"$OUT/load-${mode}-${payload}.log"
        local rps p50 p95 p99
        rps="$(sed -n 's/.*req\/s=\([0-9.]*\).*/\1/p' <<<"$out")"
        p50="$(sed -n 's/.*p50=\([0-9.]*\)ms.*/\1/p' <<<"$out")"
        p95="$(sed -n 's/.*p95=\([0-9.]*\)ms.*/\1/p' <<<"$out")"
        p99="$(sed -n 's/.*p99=\([0-9.]*\)ms.*/\1/p' <<<"$out")"
        echo "${rps:-0}|${p50:-0}|${p95:-0}|${p99:-0}"
    fi
}

peak_rss_kb() { awk '/VmRSS/{print $2}' "/proc/$1/status" 2>/dev/null || echo 0; }

# measured run: samples proxy CPU% and peak RSS around one load run.
# echoes "req/s|p50|p95|p99|cpu%|rssMB"
measured() {
    local mode="$1" payload="$2" port="$3" pid="$4"
    local before after result peak=0
    before="$(cpu_ticks "$pid")"
    # background RSS sampler
    ( while kill -0 "$pid" 2>/dev/null; do r="$(peak_rss_kb "$pid")"; [ "${r:-0}" -gt "$peak" ] && peak="$r"; echo "$peak" >"$OUT/.rss"; sleep 0.3; done ) &
    local sampler=$!
    result="$(run_load "$mode" "$payload" "$port")"
    after="$(cpu_ticks "$pid")"
    kill "$sampler" 2>/dev/null || true
    peak="$(cat "$OUT/.rss" 2>/dev/null || echo 0)"
    local cpu
    cpu="$(awk -v t=$((after-before)) -v c="$CLK" -v d="$DURATION" 'BEGIN{printf "%.1f", (t/c)/d*100}')"
    local rss
    rss="$(awk -v k="$peak" 'BEGIN{printf "%.1f", k/1024}')"
    echo "${result}|${cpu}|${rss}"
}

# median of RUNS for a measured() data point (median by req/s)
med_runs() {
    local mode="$1" payload="$2" port="$3" pid="$4" i line
    local tmp="$OUT/.runs"; : >"$tmp"
    for i in $(seq 1 "$RUNS"); do line="$(measured "$mode" "$payload" "$port" "$pid")"; echo "$line" >>"$tmp"; done
    # median row by req/s (field 1): sort numerically, take the middle line.
    sort -t'|' -k1 -n "$tmp" | awk -v n="$RUNS" 'NR==int((n+1)/2)'
}

splice_count() { curl -s "127.0.0.1:9091/metrics" 2>/dev/null | awk '/relay_splice_selected/{s+=$2} END{print s+0}'; }

perf_top() {
    [ "$PERF" = 1 ] || return 0
    local pid="$1" tag="$2"
    command -v perf >/dev/null 2>&1 || { echo "(perf not available)" >"$OUT/perf-$tag.txt"; return; }
    perf record -g -o "$OUT/perf-$tag.data" -p "$pid" -- sleep 5 2>/dev/null || true
    perf report -i "$OUT/perf-$tag.data" --stdio --no-children 2>/dev/null \
        | awk '/^[ ]*[0-9]+\./{next} /%/{print}' | head -15 >"$OUT/perf-$tag.txt" || true
    echo "  perf top-15 -> $OUT/perf-$tag.txt"
}

row() { printf '| %-26s | %-9s | %8s | %6s | %6s | %6s | %6s | %7s |\n' "$@"; }
hdr() {
    echo "| variant/condition          | test      |    req/s |    p50 |    p95 |    p99 |  cpu% |   rssMB |"
    echo "|----------------------------|-----------|---------:|-------:|-------:|-------:|------:|--------:|"
}

# ── Experiment 2: relay knobs (VLESS two-hop) ────────────────────────────────
exp_relay() {
    echo "### Experiment 2 — Fast-profile relay knobs (VLESS two-hop)"
    echo "load=$LOAD  dur=${DURATION}s conc=$CONC runs=$RUNS  server_addr=$SERVER_ADDR"
    # client proxy (socks 1081 -> vless server). Local copy with SERVER_ADDR substituted.
    sed "s/__SERVER_ADDR__/$SERVER_ADDR/" "$KNOBS/client.json" >"$OUT/client.json"
    start_origin
    hdr
    local variant
    for variant in server-baseline server-splice-iouring server-splice-epoll \
                   server-copy-immediate server-copy-deferred server-copy-adaptive; do
        local spid cpid
        spid="$(start_proxy "$KNOBS/$variant.json" 10080)"
        $(pin "") "$BIN" run -c "$OUT/client.json" >"$OUT/client.log" 2>&1 & cpid=$!; PIDS+=("$cpid")
        for _ in $(seq 1 60); do python3 -c "import socket;socket.create_connection(('127.0.0.1',1081),0.2)" 2>/dev/null && break; sleep 0.1; done
        # measure via the client's socks port 1081 (proxy-under-test CPU = server spid)
        local ka bulk
        ka="$(med_runs keepalive 1k 1081 "$spid")"
        local churn; churn="$(med_runs churn 1k 1081 "$spid")"
        bulk="$(med_runs keepalive 100m 1081 "$spid")"
        local sc; sc="$(splice_count)"
        IFS='|' read -r r p50 p95 p99 cpu rss <<<"$ka";    row "$variant" "keepalive" "$r" "$p50" "$p95" "$p99" "$cpu" "$rss"
        IFS='|' read -r r p50 p95 p99 cpu rss <<<"$churn"; row "$variant" "churn" "$r" "$p50" "$p95" "$p99" "$cpu" "$rss"
        IFS='|' read -r r p50 p95 p99 cpu rss <<<"$bulk";  row "$variant" "bulk100m" "$r" "$p50" "$p95" "$p99" "$cpu" "$rss"
        echo "    splice_selected_total=$sc  ($variant)"
        perf_top "$spid" "$variant"
        stop_pid "$cpid"; stop_pid "$spid"
    done
}

# ── Experiment 1: kernel/firewall reclaim (SOCKS) ────────────────────────────
NFT_BACKUP="$OUT/nft.backup"
declare -A SYS_SAVE=()
save_sysctl() { local k; for k in "$@"; do SYS_SAVE[$k]="$(sysctl -n "$k" 2>/dev/null)"; done; }
restore_sysctl() { local k; for k in "${!SYS_SAVE[@]}"; do sysctl -w "$k=${SYS_SAVE[$k]}" >/dev/null 2>&1 || true; done; }

apply_condition() {
    case "$1" in
        baseline) : ;;
        notrack)
            nft add table ip raw 2>/dev/null || true
            nft 'add chain ip raw prerouting { type filter hook prerouting priority -300 ; }' 2>/dev/null || true
            nft 'add chain ip raw output { type filter hook output priority -300 ; }' 2>/dev/null || true
            nft add rule ip raw prerouting tcp dport "{ 1080, $ORIGIN_PORT }" notrack 2>/dev/null || true
            nft add rule ip raw output tcp sport "{ 1080, $ORIGIN_PORT }" notrack 2>/dev/null || true ;;
        nft-flush)  nft flush ruleset 2>/dev/null || true ;;
        offloads)   [ -n "$IFACE" ] && ethtool -K "$IFACE" gro on gso on tso on 2>/dev/null || true ;;
        sysctl-buf) save_sysctl net.core.rmem_max net.core.wmem_max
                    sysctl -w net.core.rmem_max=8388608 net.core.wmem_max=8388608 >/dev/null 2>&1 || true ;;
        sysctl-budget) save_sysctl net.core.netdev_budget net.core.netdev_budget_usecs
                    sysctl -w net.core.netdev_budget=600 net.core.netdev_budget_usecs=8000 >/dev/null 2>&1 || true ;;
    esac
}
revert_condition() {
    case "$1" in
        notrack|nft-flush) [ -f "$NFT_BACKUP" ] && nft -f "$NFT_BACKUP" 2>/dev/null || true ;;
        sysctl-buf|sysctl-budget) restore_sysctl ;;
    esac
}

exp_kernel() {
    echo "### Experiment 1 — kernel/firewall reclaim (SOCKS rig)"
    [ "$(id -u)" -eq 0 ] || echo "WARN: not root — nft/sysctl/ethtool steps will be skipped"
    [ -z "$IFACE" ] && IFACE="$(ip route get 1.1.1.1 2>/dev/null | sed -n 's/.* dev \([^ ]*\).*/\1/p' | head -1)"
    echo "iface=$IFACE  kernel=$(uname -r)"
    nft list ruleset >"$NFT_BACKUP" 2>/dev/null || true
    start_origin
    hdr
    local cond
    for cond in baseline notrack nft-flush offloads sysctl-buf sysctl-budget; do
        apply_condition "$cond"
        local pid; pid="$(start_proxy "$CFG/blackwire-socks-direct.json" 1080)"
        local ka churn
        ka="$(med_runs keepalive 1k 1080 "$pid")"
        churn="$(med_runs churn 1k 1080 "$pid")"
        IFS='|' read -r r p50 p95 p99 cpu rss <<<"$ka";    row "$cond" "keepalive" "$r" "$p50" "$p95" "$p99" "$cpu" "$rss"
        IFS='|' read -r r p50 p95 p99 cpu rss <<<"$churn"; row "$cond" "churn" "$r" "$p50" "$p95" "$p99" "$cpu" "$rss"
        [ "$cond" = nft-flush ] && echo "    ^ nft-flush is an UPPER-BOUND measurement, not a recommendation"
        perf_top "$pid" "kernel-$cond"
        stop_pid "$pid"
        revert_condition "$cond"
    done
    [ -f "$NFT_BACKUP" ] && nft -f "$NFT_BACKUP" 2>/dev/null || true
}

case "${1:-all}" in
    relay)  exp_relay ;;
    kernel) exp_kernel ;;
    all)    exp_relay; echo; exp_kernel ;;
    *) echo "usage: $0 [relay|kernel|all]"; exit 2 ;;
esac
echo
echo "raw logs in $OUT/"
