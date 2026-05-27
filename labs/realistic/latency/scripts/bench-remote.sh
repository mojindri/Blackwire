#!/usr/bin/env bash
# bench-remote.sh — run make bench on a remote VPS over SSH
#
# Usage:
#   bench-remote.sh <ssh-host>
#   bench-remote.sh root@1.2.3.4
#
# Environment:
#   SSH_KEY       SSH key file (default: ssh-agent)
#   SSH_PORT      SSH port (default: 22)
#   REPO_URL      Git repo to clone (default: https://github.com/mojindri/Blackwire)
#   REPO_PATH     Remote path to clone into (default: ~/Blackwire)
#   BENCH_DURATION  Seconds per variant (default: 30)
#   BENCH_CONC      Concurrency (default: 32)
set -euo pipefail

HOST="${1:?Usage: bench-remote.sh <user@host>}"
SSH_PORT="${SSH_PORT:-22}"
SSH_KEY="${SSH_KEY:-}"
REPO_URL="${REPO_URL:-https://github.com/mojindri/Blackwire}"
REPO_PATH="${REPO_PATH:-~/Blackwire}"
BENCH_DURATION="${BENCH_DURATION:-30}"
BENCH_CONC="${BENCH_CONC:-32}"

SSH_OPTS=(-p "$SSH_PORT" -o StrictHostKeyChecking=accept-new -o ConnectTimeout=15)
[ -n "$SSH_KEY" ] && SSH_OPTS+=(-i "$SSH_KEY")

log() { echo "==> [bench-remote] $*"; }

log "connecting to $HOST"

ssh "${SSH_OPTS[@]}" "$HOST" bash -s -- \
    "$REPO_URL" "$REPO_PATH" "$BENCH_DURATION" "$BENCH_CONC" <<'REMOTE'
set -euo pipefail
REPO_URL="$1"
REPO_PATH="$2"
BENCH_DURATION="$3"
BENCH_CONC="$4"

log() { echo "==> [vps] $*"; }

# ── Docker ────────────────────────────────────────────────────────────────────
if ! command -v docker >/dev/null 2>&1; then
    log "installing Docker..."
    curl -fsSL https://get.docker.com | sh
    log "Docker installed"
fi
docker info >/dev/null 2>&1 || { log "ERROR: Docker not running"; exit 1; }
log "Docker: $(docker --version)"

# ── Repo ──────────────────────────────────────────────────────────────────────
REPO_PATH="${REPO_PATH/#\~/$HOME}"
if [ -d "$REPO_PATH/.git" ]; then
    log "pulling latest: $REPO_PATH"
    git -C "$REPO_PATH" pull --ff-only
else
    log "cloning $REPO_URL → $REPO_PATH"
    git clone "$REPO_URL" "$REPO_PATH"
fi

# ── Run bench ─────────────────────────────────────────────────────────────────
log "running make bench (${BENCH_DURATION}s × ${BENCH_CONC} conc)"
cd "$REPO_PATH"
make bench BENCH_DURATION="$BENCH_DURATION" BENCH_CONC="$BENCH_CONC"
REMOTE

log "done"
