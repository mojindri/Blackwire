#!/usr/bin/env python3
"""Guard against raw third-party source drops without license review."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SKIP_DIRS = {
    ".git",
    "target",
    "node_modules",
    "black-ui/frontend/node_modules",
    "labs/realistic/reports",
    "labs/competitive/reports",
}
SUSPICIOUS = (
    "github.com/SagerNet/sing-box",
    "github.com/XTLS/Xray-core",
    "github.com/apernet/hysteria",
    "github.com/cfal/shoes",
)
DERIVED_MARKERS = (
    "SPDX-License-Identifier:",
    "Third-party-derived:",
    "License:",
)
RAW_COPY_MARKERS = (
    "copied from",
    "derived from",
    "vendored from",
    "source from",
)


def should_skip(path: Path) -> bool:
    rel = path.relative_to(ROOT).as_posix()
    if rel == "ci/security/check_source_policy.py":
        return True
    return any(rel == item or rel.startswith(f"{item}/") for item in SKIP_DIRS)


def text_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        if path.is_dir() or should_skip(path):
            continue
        if path.suffix.lower() in {
            ".rs",
            ".sh",
            ".py",
        }:
            files.append(path)
    return files


def main() -> int:
    failures: list[str] = []
    for path in text_files():
        data = path.read_text(encoding="utf-8", errors="ignore")
        lower = data.lower()
        if any(marker in data for marker in SUSPICIOUS) and any(
            marker in lower for marker in RAW_COPY_MARKERS
        ):
            header = "\n".join(data.splitlines()[:20])
            if not any(marker in header for marker in DERIVED_MARKERS):
                failures.append(
                    f"{path.relative_to(ROOT)} references third-party source without a license header"
                )

    if failures:
        print("source policy check failed", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("source policy check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
