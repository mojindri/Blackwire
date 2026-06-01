#!/usr/bin/env bash
set -euo pipefail

# Milestone A scaffold: record mobile-like scenarios without mutating routes.
for scenario in mobile-rtt-50 mobile-rtt-100 mobile-jitter-20 mobile-roaming; do
    MOBILE_SCENARIO="$scenario" bash "$(dirname "$0")/run_matrix.sh" "$scenario"
done
