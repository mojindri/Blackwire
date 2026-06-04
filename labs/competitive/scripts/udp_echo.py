#!/usr/bin/env python3
import argparse
import socket


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, required=True)
    args = parser.parse_args()

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind((args.host, args.port))
    while True:
        data, peer = sock.recvfrom(65535)
        if data:
            sock.sendto(data, peer)


if __name__ == "__main__":
    main()
