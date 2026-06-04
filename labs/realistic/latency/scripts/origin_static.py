#!/usr/bin/env python3
"""Minimal HTTP/1.1 origin for load tests. Serves /<N>{k} with N KiB of body.

Honors the client's Connection header (keep-alive vs close). No logging, no
framework overhead — kept deliberately thin so the origin is never the
bottleneck relative to the proxy under test.
"""
import argparse
import socket
import threading


def make_body(path):
    # path like "/1k" -> 1024 bytes, "/4k" -> 4096, default 1024
    p = path.strip("/").lower()
    n = 1
    if p.endswith("k"):
        try:
            n = int(p[:-1] or "1")
        except ValueError:
            n = 1
    return b"x" * (n * 1024)


def handle(conn):
    conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    buf = bytearray()
    try:
        while True:
            while b"\r\n\r\n" not in buf:
                chunk = conn.recv(65536)
                if not chunk:
                    return
                buf += chunk
            head_end = buf.index(b"\r\n\r\n") + 4
            head = buf[:head_end].decode("latin1")
            del buf[:head_end]
            request_line = head.split("\r\n", 1)[0]
            parts = request_line.split(" ")
            path = parts[1] if len(parts) > 1 else "/1k"
            keep_alive = "close" not in head.lower()
            body = make_body(path)
            conn_hdr = "keep-alive" if keep_alive else "close"
            resp = (f"HTTP/1.1 200 OK\r\n"
                    f"Content-Length: {len(body)}\r\n"
                    f"Connection: {conn_hdr}\r\n\r\n").encode() + body
            conn.sendall(resp)
            if not keep_alive:
                return
    except Exception:
        return
    finally:
        try:
            conn.close()
        except Exception:
            pass


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=18080)
    args = ap.parse_args()
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((args.host, args.port))
    srv.listen(512)
    print(f"origin listening on {args.host}:{args.port}", flush=True)
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=handle, args=(conn,), daemon=True).start()


if __name__ == "__main__":
    main()
