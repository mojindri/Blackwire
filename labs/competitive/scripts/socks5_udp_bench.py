#!/usr/bin/env python3
import argparse
import json
import math
import socket
import struct
import time


def encode_socks_udp(host: str, port: int, payload: bytes) -> bytes:
    try:
        addr = socket.inet_aton(host)
        return b"\x00\x00\x00\x01" + addr + struct.pack("!H", port) + payload
    except OSError:
        encoded = host.encode("utf-8")
        if len(encoded) > 255:
            raise ValueError("domain too long for SOCKS5 UDP")
        return b"\x00\x00\x00\x03" + bytes([len(encoded)]) + encoded + struct.pack("!H", port) + payload


def payload(seq: int, size: int) -> bytes:
    size = max(size, 8)
    body = bytearray(size)
    body[:8] = struct.pack("!Q", seq)
    for idx in range(8, size):
        body[idx] = ((idx - 8) * 31 + 17) & 0xFF
    return bytes(body)


def payload_offset(packet: bytes) -> int:
    if len(packet) < 4 or packet[:3] != b"\x00\x00\x00":
        raise ValueError("invalid SOCKS5 UDP reply")
    atyp = packet[3]
    if atyp == 1:
        return 10
    if atyp == 4:
        return 22
    if atyp == 3:
        if len(packet) < 5:
            raise ValueError("truncated SOCKS5 UDP domain reply")
        return 5 + packet[4] + 2
    raise ValueError(f"unsupported SOCKS5 ATYP {atyp}")


def percentile_ms(sorted_us: list[int], pct: float) -> float:
    if not sorted_us:
        return 0.0
    rank = max(1, math.ceil((pct / 100.0) * len(sorted_us)))
    return sorted_us[min(rank - 1, len(sorted_us) - 1)] / 1000.0


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--socks-host", default="127.0.0.1")
    parser.add_argument("--socks-port", type=int, required=True)
    parser.add_argument("--dest-host", required=True)
    parser.add_argument("--dest-port", type=int, required=True)
    parser.add_argument("--count", type=int, default=500)
    parser.add_argument("--concurrency", type=int, default=1)
    parser.add_argument("--payload-bytes", type=int, default=64)
    parser.add_argument("--timeout-ms", type=int, default=3000)
    parser.add_argument("--variant", default="hysteria")
    parser.add_argument("--scenario", default="hysteria2-udp-dns")
    args = parser.parse_args()

    ctrl = socket.create_connection((args.socks_host, args.socks_port), timeout=5)
    ctrl.sendall(b"\x05\x01\x00")
    if ctrl.recv(2) != b"\x05\x00":
        raise RuntimeError("SOCKS5 greeting failed")
    ctrl.sendall(b"\x05\x03\x00\x01\x00\x00\x00\x00\x00\x00")
    reply = ctrl.recv(10)
    if len(reply) != 10 or reply[1] != 0:
        raise RuntimeError(f"SOCKS5 UDP associate failed: {reply!r}")
    relay_port = struct.unpack("!H", reply[8:10])[0]

    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp.bind(("127.0.0.1", 0))
    udp.settimeout(args.timeout_ms / 1000.0)

    latencies_us: list[int] = []
    errors = 0
    stale = 0
    bytes_down = 0
    started = time.perf_counter()

    next_seq = 0
    in_flight: dict[int, float] = {}
    timeout_s = args.timeout_ms / 1000.0
    concurrency = max(1, args.concurrency)

    while next_seq < args.count or in_flight:
        while next_seq < args.count and len(in_flight) < concurrency:
            packet = encode_socks_udp(
                args.dest_host,
                args.dest_port,
                payload(next_seq, args.payload_bytes),
            )
            in_flight[next_seq] = time.perf_counter()
            udp.sendto(packet, (args.socks_host, relay_port))
            next_seq += 1

        now = time.perf_counter()
        expired = [seq for seq, sent in in_flight.items() if now - sent >= timeout_s]
        for seq in expired:
            if in_flight.pop(seq, None) is not None:
                errors += 1
        if not in_flight:
            continue

        wait_s = max(0.001, min(timeout_s - (now - sent) for sent in in_flight.values()))
        udp.settimeout(wait_s)
        try:
            reply_packet, _ = udp.recvfrom(65535)
        except socket.timeout:
            continue
        off = payload_offset(reply_packet)
        received = reply_packet[off:]
        if len(received) < 8:
            stale += 1
            continue
        seq = struct.unpack("!Q", received[:8])[0]
        sent = in_flight.pop(seq, None)
        if sent is None:
            stale += 1
            continue
        latencies_us.append(int((time.perf_counter() - sent) * 1_000_000))
        bytes_down += len(received)

    elapsed = max(0.000001, time.perf_counter() - started)
    latencies_us.sort()
    row = {
        "variant": args.variant,
        "scenario": args.scenario,
        "protocol": "hysteria2",
        "transport": "quic-datagram",
        "profile": "baseline",
        "payload_size": args.payload_bytes,
        "concurrency": concurrency,
        "requests": args.count,
        "ok": len(latencies_us),
        "errors": errors,
        "stale_replies": stale,
        "requests_per_sec": len(latencies_us) / elapsed,
        "duration_secs": elapsed,
        "latency_p50_ms": percentile_ms(latencies_us, 50),
        "latency_p90_ms": percentile_ms(latencies_us, 90),
        "latency_p95_ms": percentile_ms(latencies_us, 95),
        "latency_p99_ms": percentile_ms(latencies_us, 99),
        "latency_p999_ms": percentile_ms(latencies_us, 99.9),
        "bytes_up": args.count * max(args.payload_bytes, 8),
        "bytes_down": bytes_down,
    }
    print(json.dumps(row, separators=(",", ":")))


if __name__ == "__main__":
    main()
