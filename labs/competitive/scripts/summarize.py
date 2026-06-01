#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path


def rows(report_dir: Path) -> list[dict]:
    out: list[dict] = []
    for path in sorted(report_dir.glob("*.jsonl")):
        for line in path.read_text().splitlines():
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                continue
            row["_file"] = path.name
            out.append(row)
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", default="reports")
    parser.add_argument("--out")
    args = parser.parse_args()
    report_dir = Path(args.dir)
    data = rows(report_dir)
    lines = ["# Competitive Benchmark Summary", ""]
    if not data:
        lines.append("_No competitive JSONL rows found._")
    else:
        lines += [
            "| File | Scenario | Variant | Status | Payload | p50 ms | p95 ms | p99 ms | req/s | Errors | Reason |",
            "|---|---|---|---|---|---:|---:|---:|---:|---:|---|",
        ]
        for row in data:
            lines.append(
                "| {file} | {scenario} | {variant} | {status} | {payload} | {p50:.2f} | {p95:.2f} | {p99:.2f} | {rps:.2f} | {errors} | {reason} |".format(
                    file=row.get("_file", ""),
                    scenario=row.get("scenario", ""),
                    variant=row.get("variant", ""),
                    status=row.get("status", ""),
                    payload=row.get("payload_size", ""),
                    p50=float(row.get("latency_p50", 0)) * 1000,
                    p95=float(row.get("latency_p95", 0)) * 1000,
                    p99=float(row.get("latency_p99", 0)) * 1000,
                    rps=float(row.get("requests_per_sec", 0)),
                    errors=row.get("errors", 0),
                    reason=str(row.get("reason", "")).replace("|", "/"),
                )
            )
    text = "\n".join(lines) + "\n"
    out = Path(args.out) if args.out else report_dir / "summary.md"
    out.write_text(text)
    print(text)
    print(f"Wrote {out}")


if __name__ == "__main__":
    main()
