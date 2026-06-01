#!/usr/bin/env bash
set -euo pipefail

LOSS_VALUES="${COMPETITIVE_LOSS_VALUES:-1 3 5 10}"
for loss in $LOSS_VALUES; do
    LOSS_PERCENT="$loss" bash "$(dirname "$0")/run_matrix.sh" "hysteria2-loss-${loss}"
done
