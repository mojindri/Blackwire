#!/usr/bin/env python3
import argparse
import json
import socket
import statistics
import time


def percentile(values, pct):
    if not values:
        return 0.0
    ordered = sorted(values)
    idx = min(len(ordered) - 1, int(round((pct / 100.0) * (len(ordered) - 1))))
    return ordered[idx]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", required=True)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--count", type=int, default=500)
    parser.add_argument("--payload-bytes", type=int, default=64)
    parser.add_argument("--timeout-ms", type=int, default=3000)
    parser.add_argument("--variant", required=True)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--timestamp", required=True)
    parser.add_argument("--loss-percent", type=float, default=0.0)
    parser.add_argument("--rtt-ms", type=float, default=0.0)
    parser.add_argument("--jitter-ms", type=float, default=0.0)
    args = parser.parse_args()

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(args.timeout_ms / 1000.0)
    payload_len = max(0, args.payload_bytes - 8)
    latencies = []
    errors = 0
    start = time.perf_counter()

    for seq in range(args.count):
        payload = seq.to_bytes(8, "big") + (b"x" * payload_len)
        sent = time.perf_counter()
        try:
            sock.sendto(payload, (args.host, args.port))
            data, _ = sock.recvfrom(max(65535, args.payload_bytes + 64))
            if len(data) < 8 or int.from_bytes(data[:8], "big") != seq:
                errors += 1
                continue
            latencies.append(time.perf_counter() - sent)
        except OSError:
            errors += 1

    elapsed = max(time.perf_counter() - start, 0.000001)
    row = {
        "timestamp": args.timestamp,
        "variant": args.variant,
        "scenario": args.scenario,
        "protocol": "direct",
        "transport": "tun-udp",
        "profile": "tun",
        "payload_size": f"{args.payload_bytes}b",
        "concurrency": 1,
        "duration": round(elapsed, 6),
        "keepalive_on": True,
        "loss_percent": args.loss_percent,
        "rtt_ms": args.rtt_ms,
        "jitter_ms": args.jitter_ms,
        "bandwidth_limit": "",
        "requests_per_sec": round(len(latencies) / elapsed, 4),
        "throughput_mbps": 0,
        "ttfb_p50": 0,
        "ttfb_p90": 0,
        "ttfb_p95": 0,
        "ttfb_p99": 0,
        "ttfb_p999": 0,
        "latency_p50": round(percentile(latencies, 50), 6),
        "latency_p90": round(percentile(latencies, 90), 6),
        "latency_p95": round(percentile(latencies, 95), 6),
        "latency_p99": round(percentile(latencies, 99), 6),
        "latency_p999": round(percentile(latencies, 99.9), 6),
        "udp_latency_p99_ms": round(percentile(latencies, 99) * 1000.0, 3),
        "cpu_user": 0,
        "cpu_system": 0,
        "cpu_percent": 0,
        "rss_mb": 0,
        "allocations_per_sec": 0,
        "syscalls_per_sec": 0,
        "bytes_up": args.payload_bytes * args.count,
        "bytes_down": args.payload_bytes * len(latencies),
        "errors": errors,
        "handshake_failures": 0,
        "reconnect_time_ms": 0,
        "route_time_us": 0,
        "dns_time_us": 0,
        "relay_path": "",
        "status": "ok" if errors == 0 else "failed",
        "reason": "" if errors == 0 else f"{errors} UDP probe errors",
    }
    print(json.dumps(row, separators=(",", ":")))


if __name__ == "__main__":
    main()
