#!/usr/bin/env bash
set -euo pipefail

RTT_MS=50 bash "$(dirname "$0")/run_matrix.sh" "hysteria2-rtt-50"
RTT_MS=100 bash "$(dirname "$0")/run_matrix.sh" "hysteria2-rtt-100"
JITTER_MS=20 bash "$(dirname "$0")/run_matrix.sh" "hysteria2-jitter-20"
BANDWIDTH_LIMIT=10mbps bash "$(dirname "$0")/run_matrix.sh" "hysteria2-bandwidth-10mbps"
RTT_MS=100 JITTER_MS=20 LOSS_PERCENT=3 bash "$(dirname "$0")/run_matrix.sh" "hysteria2-mobile-radio-pause"
