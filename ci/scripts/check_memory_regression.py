#!/usr/bin/env python3
import json
import sys
from pathlib import Path


MISSING_SENTINEL = object()


def get_number(data, path):
    value = data
    for key in path.split("."):
        if not isinstance(value, dict) or key not in value:
            return MISSING_SENTINEL
        value = value[key]
    if value is None:
        return MISSING_SENTINEL
    try:
        return float(value)
    except (TypeError, ValueError):
        return MISSING_SENTINEL


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: check_memory_regression.py <baseline.json> <result.json>", file=sys.stderr)
        return 2

    baseline = json.loads(Path(sys.argv[1]).read_text())
    result = json.loads(Path(sys.argv[2]).read_text())

    failures = []
    values = {
        "memory.peak_rss_kb": get_number(result, "memory.peak_rss_kb"),
        "memory.peak_fd": get_number(result, "memory.peak_fd"),
        "memory.peak_threads": get_number(result, "memory.peak_threads"),
        "requests_per_second": get_number(result, "requests_per_second"),
        "latency_ms.p95": get_number(result, "latency_ms.p95"),
        "latency_ms.p99": get_number(result, "latency_ms.p99"),
    }
    for key, value in values.items():
        if value is MISSING_SENTINEL:
            failures.append(f"missing required result field: {key}")

    if failures:
        print("MEMORY PERF REGRESSION DETECTED")
        for item in failures:
            print(f"- {item}")
        return 1

    peak_rss_kb = values["memory.peak_rss_kb"]
    peak_fd = values["memory.peak_fd"]
    peak_threads = values["memory.peak_threads"]
    rps = values["requests_per_second"]
    p95 = values["latency_ms.p95"]
    p99 = values["latency_ms.p99"]

    if peak_rss_kb > float(baseline["max_peak_rss_kb"]):
        failures.append(f"peak_rss_kb regression: {peak_rss_kb} > {baseline['max_peak_rss_kb']}")
    if peak_fd > float(baseline["max_peak_fd"]):
        failures.append(f"peak_fd regression: {peak_fd} > {baseline['max_peak_fd']}")
    if peak_threads > float(baseline["max_peak_threads"]):
        failures.append(f"peak_threads regression: {peak_threads} > {baseline['max_peak_threads']}")
    if rps < float(baseline["min_requests_per_second"]):
        failures.append(f"rps regression: {rps} < {baseline['min_requests_per_second']}")
    if p95 > float(baseline["max_p95_latency_ms"]):
        failures.append(f"p95 regression: {p95} > {baseline['max_p95_latency_ms']}")
    if p99 > float(baseline["max_p99_latency_ms"]):
        failures.append(f"p99 regression: {p99} > {baseline['max_p99_latency_ms']}")

    if failures:
        print("MEMORY PERF REGRESSION DETECTED")
        for item in failures:
            print(f"- {item}")
        return 1

    print("memory perf gate passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
