#!/usr/bin/env bash
set -euo pipefail

WORK_DIR="${1:?usage: start_nginx_upstream.sh <work-dir> [port]}"
PORT="${2:-18080}"
HOST="${3:-127.0.0.1}"
STREAM_PORT="${4:-18443}"
STREAM_UPSTREAM="${5:-www.microsoft.com:443}"
mkdir -p "$WORK_DIR/html" "$WORK_DIR/logs" "$WORK_DIR/client_body"
chmod 755 "$(dirname "$WORK_DIR")"
chmod 755 "$WORK_DIR" "$WORK_DIR/html" "$WORK_DIR/logs" "$WORK_DIR/client_body"

python3 - "$WORK_DIR/html" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])
sizes = {
    "index.html": 1024,
    "1k": 1024,
    "4k": 4 * 1024,
    "16k": 16 * 1024,
    "64k": 64 * 1024,
    "1m": 1024 * 1024,
    "64m": 64 * 1024 * 1024,
}
for name, size in sizes.items():
    (root / name).write_bytes(b"x" * size)
PY
chmod 644 "$WORK_DIR/html"/*

stream_module=""
stream_block=""
if [ -f /usr/lib/nginx/modules/ngx_stream_module.so ]; then
  stream_module="load_module /usr/lib/nginx/modules/ngx_stream_module.so;"
  stream_block="
stream {
  server {
    listen $HOST:$STREAM_PORT;
    proxy_pass $STREAM_UPSTREAM;
  }
}"
fi

cat > "$WORK_DIR/nginx.conf" <<EOF
$stream_module
daemon off;
pid $WORK_DIR/nginx.pid;
error_log $WORK_DIR/logs/error.log warn;
events { worker_connections 512; }
$stream_block
http {
  access_log off;
  sendfile on;
  tcp_nopush on;
  tcp_nodelay on;
  keepalive_timeout 65;
  client_body_temp_path $WORK_DIR/client_body;
  server {
    listen $HOST:$PORT;
    root $WORK_DIR/html;
    location / { try_files \$uri /index.html =404; }
  }
}
EOF

nginx -c "$WORK_DIR/nginx.conf" -p "$WORK_DIR" > "$WORK_DIR/stdout.log" 2>&1 &
pid=$!
CHECK_HOST="$HOST"
if [ "$CHECK_HOST" = "0.0.0.0" ] || [ "$CHECK_HOST" = "::" ]; then
    CHECK_HOST="127.0.0.1"
fi
for _ in $(seq 1 40); do
    if curl -fsS "http://$CHECK_HOST:$PORT/1k" >/dev/null 2>&1; then
        echo "$pid" > "$WORK_DIR/nginx.pid"
        exit 0
    fi
    if ss -ltn 2>/dev/null | awk '{print $4}' | grep -Eq "(^|:)$PORT$"; then
        echo "$pid" > "$WORK_DIR/nginx.pid"
        exit 0
    fi
    sleep 0.25
done
kill "$pid" 2>/dev/null || true
cat "$WORK_DIR/logs/error.log" >&2 2>/dev/null || true
cat "$WORK_DIR/stdout.log" >&2 2>/dev/null || true
echo "ERROR: nginx upstream did not start on 127.0.0.1:$PORT" >&2
exit 1
