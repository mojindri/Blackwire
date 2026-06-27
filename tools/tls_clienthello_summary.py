#!/usr/bin/env python3
"""Emit fingerprint-relevant fields from a TLS ClientHello.

Input may be either a complete TLS record beginning with 0x16, or a raw
ClientHello handshake message beginning with 0x01. Output is JSON so lab scripts
can archive and diff it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path


def u16(data: bytes, off: int) -> int:
    if off + 2 > len(data):
        raise ValueError("truncated u16")
    return int.from_bytes(data[off : off + 2], "big")


def u24(data: bytes, off: int) -> int:
    if off + 3 > len(data):
        raise ValueError("truncated u24")
    return int.from_bytes(data[off : off + 3], "big")


def is_grease(value: int) -> bool:
    return (value & 0x0F0F) == 0x0A0A and ((value >> 8) & 0xFF) == (value & 0xFF)


def format_list(values: list[int]) -> list[str]:
    return [f"0x{value:04x}" for value in values]


def decimal_join(values: list[int], *, skip_grease: bool = False) -> str:
    filtered = [value for value in values if not (skip_grease and is_grease(value))]
    return "-".join(str(value) for value in filtered)


def read_input(path: Path, hex_input: bool) -> bytes:
    raw = path.read_text().strip() if hex_input else path.read_bytes()
    if hex_input:
        return bytes.fromhex("".join(raw.split()))
    return raw


def clienthello_message(data: bytes) -> bytes:
    if len(data) < 4:
        raise ValueError("input too short")

    if data[0] == 0x16:
        if len(data) < 9:
            raise ValueError("TLS record too short")
        record_len = u16(data, 3)
        record_end = 5 + record_len
        if record_end > len(data):
            raise ValueError("TLS record length exceeds input")
        data = data[5:record_end]

    if data[0] != 0x01:
        raise ValueError(f"expected ClientHello handshake type 0x01, got 0x{data[0]:02x}")

    msg_len = u24(data, 1)
    msg_end = 4 + msg_len
    if msg_end > len(data):
        raise ValueError("ClientHello length exceeds input")
    return data[:msg_end]


def parse_extensions(ext_bytes: bytes) -> tuple[list[int], dict[int, bytes]]:
    order: list[int] = []
    values: dict[int, bytes] = {}
    off = 0
    while off < len(ext_bytes):
        if off + 4 > len(ext_bytes):
            raise ValueError("truncated extension header")
        ext_type = u16(ext_bytes, off)
        ext_len = u16(ext_bytes, off + 2)
        off += 4
        end = off + ext_len
        if end > len(ext_bytes):
            raise ValueError("truncated extension payload")
        order.append(ext_type)
        values[ext_type] = ext_bytes[off:end]
        off = end
    return order, values


def parse_sni(ext: bytes | None) -> str | None:
    if not ext or len(ext) < 5:
        return None
    list_len = u16(ext, 0)
    off = 2
    end = min(len(ext), 2 + list_len)
    while off + 3 <= end:
        name_type = ext[off]
        name_len = u16(ext, off + 1)
        off += 3
        name_end = off + name_len
        if name_end > end:
            return None
        if name_type == 0:
            return ext[off:name_end].decode("utf-8", "replace")
        off = name_end
    return None


def parse_alpn(ext: bytes | None) -> list[str]:
    if not ext or len(ext) < 2:
        return []
    list_len = u16(ext, 0)
    off = 2
    end = min(len(ext), 2 + list_len)
    protocols: list[str] = []
    while off < end:
        item_len = ext[off]
        off += 1
        item_end = off + item_len
        if item_end > end:
            break
        protocols.append(ext[off:item_end].decode("ascii", "replace"))
        off = item_end
    return protocols


def parse_u16_vector(ext: bytes | None) -> list[int]:
    if not ext or len(ext) < 2:
        return []
    total = u16(ext, 0)
    off = 2
    end = min(len(ext), 2 + total)
    values: list[int] = []
    while off + 2 <= end:
        values.append(u16(ext, off))
        off += 2
    return values


def parse_ec_point_formats(ext: bytes | None) -> list[int]:
    if not ext:
        return []
    count = ext[0]
    return list(ext[1 : 1 + count])


def summarize(data: bytes) -> dict[str, object]:
    hello = clienthello_message(data)
    body = hello[4:]
    off = 0

    legacy_version = u16(body, off)
    off += 2
    off += 32

    session_id_len = body[off]
    off += 1 + session_id_len

    cipher_len = u16(body, off)
    off += 2
    ciphers = [u16(body, i) for i in range(off, off + cipher_len, 2)]
    off += cipher_len

    compression_len = body[off]
    off += 1 + compression_len

    ext_total = u16(body, off)
    off += 2
    ext_order, ext_values = parse_extensions(body[off : off + ext_total])

    groups = parse_u16_vector(ext_values.get(10))
    point_formats = parse_ec_point_formats(ext_values.get(11))
    ja3 = ",".join(
        [
            str(legacy_version),
            decimal_join(ciphers, skip_grease=True),
            decimal_join(ext_order, skip_grease=True),
            decimal_join(groups, skip_grease=True),
            "-".join(str(value) for value in point_formats),
        ]
    )

    return {
        "sni": parse_sni(ext_values.get(0)),
        "alpn": parse_alpn(ext_values.get(16)),
        "cipher_suites": format_list(ciphers),
        "cipher_suites_no_grease": format_list([v for v in ciphers if not is_grease(v)]),
        "supported_groups": format_list(groups),
        "supported_groups_no_grease": format_list([v for v in groups if not is_grease(v)]),
        "extension_order": format_list(ext_order),
        "extension_order_no_grease": format_list([v for v in ext_order if not is_grease(v)]),
        "ja3": ja3,
        "ja3_md5": hashlib.md5(ja3.encode("ascii")).hexdigest(),
        "note": "REALITY interop is supported; byte-identical browser TLS fingerprinting is not guaranteed.",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="TLS record, ClientHello message, or hex file")
    parser.add_argument("--hex", action="store_true", help="read input as hex text")
    args = parser.parse_args()

    try:
        summary = summarize(read_input(args.input, args.hex))
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
