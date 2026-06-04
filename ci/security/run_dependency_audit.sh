#!/usr/bin/env bash
set -euo pipefail

cargo audit
cargo deny check advisories licenses bans sources
python3 ci/security/check_source_policy.py
cargo outdated --workspace || true
cargo geiger --all-features --workspace || true
cargo udeps --workspace --all-targets || true
