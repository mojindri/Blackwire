#!/usr/bin/env python3
"""Small static payload HTTP server for competitive smoke benchmarks."""

from __future__ import annotations

import argparse
import http.server
import signal

PAYLOADS = {
    "/": 1024,
    "/1k": 1024,
    "/4k": 4 * 1024,
    "/16k": 16 * 1024,
    "/64k": 64 * 1024,
    "/1m": 1024 * 1024,
}


class ThreadingHTTPServer(http.server.ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    bodies = {path: b"x" * size for path, size in PAYLOADS.items()}

    def do_GET(self) -> None:
        path = self.path.split("?", 1)[0]
        body = self.bodies.get(path, self.bodies["/"])
        self.send_response(200)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args: object) -> None:
        return


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=18080)
    args = parser.parse_args()
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    signal.signal(signal.SIGTERM, lambda *_: server.shutdown())
    signal.signal(signal.SIGINT, lambda *_: server.shutdown())
    server.serve_forever()


if __name__ == "__main__":
    main()
