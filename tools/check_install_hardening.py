#!/usr/bin/env python3
"""Static hardening assertions for scripts/install.sh."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INSTALL_SH = ROOT / "scripts" / "install.sh"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def function_body(script: str, name: str) -> str:
    match = re.search(rf"^{name}\(\) \{{\n", script, re.MULTILINE)
    require(match is not None, f"{name}() not found")
    start = match.end()
    depth = 1
    pos = start
    while pos < len(script):
        char = script[pos]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return script[start:pos]
        pos += 1
    raise AssertionError(f"{name}() body did not close")


def main() -> int:
    script = INSTALL_SH.read_text()
    protect = function_body(script, "protect_config_for_service")

    require("chmod 0644" not in protect, "config protection must not chmod 0644")
    require("world-readable" not in protect, "config protection must not allow world-readable fallback")
    require('chmod 0660 "$path"' in protect, "Black UI config mode 0660 missing")
    require('chmod 0640 "$path"' in protect, "service config mode 0640 missing")
    require("groupadd --system" in protect, "missing service group creation path")
    require('die "service group' in protect, "missing fail-closed service-group error")

    require('sudo_cmd chmod 0600 "$info_file"' in script, "client-info.txt must be chmod 0600")
    require('"serverNames": ["$REALITY_SERVER_NAME"]' in script, "REALITY serverNames missing")
    require('"maxTimeDiffSeconds": 60' in script, "explicit REALITY maxTimeDiffSeconds missing")
    require('"maxConnectionsPerUser": 64' in script, "generated per-user connection limit missing")
    require("blackwire\nHTML" not in script, "public nginx placeholder must not expose project name")

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as exc:
        print(f"install hardening check failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
