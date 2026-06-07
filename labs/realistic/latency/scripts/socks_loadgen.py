#!/usr/bin/env python3
"""Local SOCKS5 load generator for measuring proxy per-request and
per-connection overhead.

Two modes:

  keepalive  — open `concurrency` SOCKS5 tunnels once, reuse each for many
               HTTP/1.1 keep-alive requests. Mirrors `hey -x socks5://...`,
               which is what the competitive matrix used against shoes.
               Measures warm relay-loop throughput.

  churn      — open a fresh SOCKS5 tunnel for every request (Connection: close).
               Measures per-connection setup cost (Context alloc, metrics
               labels, routing) — the path Steps 2 & 3 target.

No third-party deps: raw SOCKS5 (no-auth, CONNECT) + HTTP/1.1 over stdlib.
"""
import argparse
import socket
import statistics
import struct
import sys
import threading
import time


def socks5_connect(proxy_host, proxy_port, dst_host, dst_port, timeout):
    s = socket.create_connection((proxy_host, proxy_port), timeout=timeout)
    s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    # greeting: VER=5, NMETHODS=1, METHOD=0 (no auth)
    s.sendall(b"\x05\x01\x00")
    resp = _recv_exact(s, 2)
    if resp[0] != 0x05 or resp[1] != 0x00:
        raise RuntimeError(f"socks greeting failed: {resp!r}")
    # CONNECT: VER=5, CMD=1, RSV=0, ATYP=3 (domain)
    host_bytes = dst_host.encode()
    req = b"\x05\x01\x00\x03" + bytes([len(host_bytes)]) + host_bytes + struct.pack("!H", dst_port)
    s.sendall(req)
    rep = _recv_exact(s, 4)
    if rep[1] != 0x00:
        raise RuntimeError(f"socks connect rejected: rep={rep[1]}")
    # consume BND.ADDR + BND.PORT
    atyp = rep[3]
    if atyp == 0x01:
        _recv_exact(s, 4 + 2)
    elif atyp == 0x04:
        _recv_exact(s, 16 + 2)
    elif atyp == 0x03:
        ln = _recv_exact(s, 1)[0]
        _recv_exact(s, ln + 2)
    else:
        raise RuntimeError(f"bad atyp {atyp}")
    return s


def _recv_exact(s, n):
    buf = bytearray()
    while len(buf) < n:
        chunk = s.recv(n - len(buf))
        if not chunk:
            raise RuntimeError("eof during recv")
        buf += chunk
    return bytes(buf)


def _read_http_response(s):
    """Read one HTTP/1.1 response, return (keep_alive, total_bytes)."""
    buf = bytearray()
    while b"\r\n\r\n" not in buf:
        chunk = s.recv(65536)
        if not chunk:
            raise RuntimeError("eof during headers")
        buf += chunk
    header_end = buf.index(b"\r\n\r\n") + 4
    headers = buf[:header_end].decode("latin1")
    clen = 0
    keep_alive = True
    for line in headers.split("\r\n"):
        low = line.lower()
        if low.startswith("content-length:"):
            clen = int(line.split(":", 1)[1].strip())
        elif low.startswith("connection:") and "close" in low:
            keep_alive = False
    body_have = len(buf) - header_end
    while body_have < clen:
        chunk = s.recv(65536)
        if not chunk:
            raise RuntimeError("eof during body")
        body_have += len(chunk)
    return keep_alive, header_end + clen


def worker(args, stop_at, results, errors, lock):
    host_header = f"{args.dst_host}:{args.dst_port}"
    latencies = []
    count = 0
    err = 0
    conn = None
    try:
        while time.monotonic() < stop_at:
            t0 = time.monotonic()
            try:
                if args.mode == "churn" or conn is None:
                    if conn is not None:
                        conn.close()
                    conn = socks5_connect(args.proxy_host, args.proxy_port,
                                          args.dst_host, args.dst_port, args.timeout)
                connhdr = "close" if args.mode == "churn" else "keep-alive"
                req = (f"GET /{args.payload} HTTP/1.1\r\n"
                       f"Host: {host_header}\r\n"
                       f"Connection: {connhdr}\r\n\r\n").encode()
                conn.sendall(req)
                keep_alive, _ = _read_http_response(conn)
                latencies.append((time.monotonic() - t0) * 1000.0)
                count += 1
                if args.mode == "churn" or not keep_alive:
                    conn.close()
                    conn = None
            except Exception:
                err += 1
                if conn is not None:
                    try:
                        conn.close()
                    except Exception:
                        pass
                    conn = None
    finally:
        if conn is not None:
            try:
                conn.close()
            except Exception:
                pass
    with lock:
        results.extend(latencies)
        errors[0] += err


def pct(sorted_vals, p):
    if not sorted_vals:
        return 0.0
    k = (len(sorted_vals) - 1) * (p / 100.0)
    f = int(k)
    c = min(f + 1, len(sorted_vals) - 1)
    if f == c:
        return sorted_vals[f]
    return sorted_vals[f] + (sorted_vals[c] - sorted_vals[f]) * (k - f)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--proxy-host", default="127.0.0.1")
    ap.add_argument("--proxy-port", type=int, default=1080)
    ap.add_argument("--dst-host", default="127.0.0.1")
    ap.add_argument("--dst-port", type=int, default=18080)
    ap.add_argument("--payload", default="1k")
    ap.add_argument("--duration", type=float, default=10.0)
    ap.add_argument("--concurrency", type=int, default=8)
    ap.add_argument("--mode", choices=["keepalive", "churn"], default="keepalive")
    ap.add_argument("--timeout", type=float, default=5.0)
    ap.add_argument("--warmup", type=float, default=1.0)
    ap.add_argument("--label", default="")
    args = ap.parse_args()

    # warmup (not measured)
    if args.warmup > 0:
        wu_results, wu_err, wu_lock = [], [0], threading.Lock()
        stop = time.monotonic() + args.warmup
        ts = [threading.Thread(target=worker, args=(args, stop, wu_results, wu_err, wu_lock))
              for _ in range(args.concurrency)]
        for t in ts: t.start()
        for t in ts: t.join()

    results, errors, lock = [], [0], threading.Lock()
    start = time.monotonic()
    stop_at = start + args.duration
    threads = [threading.Thread(target=worker, args=(args, stop_at, results, errors, lock))
               for _ in range(args.concurrency)]
    for t in threads: t.start()
    for t in threads: t.join()
    elapsed = time.monotonic() - start

    results.sort()
    n = len(results)
    rps = n / elapsed if elapsed > 0 else 0.0
    label = args.label or f"{args.mode}"
    print(f"[{label}] mode={args.mode} conc={args.concurrency} dur={elapsed:.1f}s "
          f"req={n} err={errors[0]} | req/s={rps:.0f} "
          f"p50={pct(results,50):.3f}ms p95={pct(results,95):.3f}ms "
          f"p99={pct(results,99):.3f}ms")


if __name__ == "__main__":
    main()
