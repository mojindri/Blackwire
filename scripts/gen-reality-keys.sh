#!/usr/bin/env bash
# gen-reality-keys.sh — Generate an X25519 key pair for REALITY transport.
#
# Run this once when setting up a new server. Copy the output into your
# typed REALITY settings in Black UI:
#
#   "realitySettings": {
#     "privateKey": "<output of this script, server only>",
#     "publicKey":  "<output of this script, distribute to clients>"
#   }
#
# The private key must stay secret on the server. Never put it in client configs.
# The public key is safe to distribute — it only lets clients verify the server.

set -euo pipefail

BINARY="${BINARY:-blackwire}"

# Check that the binary exists.
if ! command -v "$BINARY" &>/dev/null; then
    # Try the build output directory.
    BINARY="./target/release/blackwire"
    if [[ ! -x "$BINARY" ]]; then
        BINARY="./target/debug/blackwire"
        if [[ ! -x "$BINARY" ]]; then
            echo "Error: blackwire binary not found. Run 'cargo build' first." >&2
            exit 1
        fi
    fi
fi

echo "Generating X25519 key pair for REALITY transport..."
echo ""
"$BINARY" x25519
echo ""
echo "Copy the private key into the server REALITY form."
echo "Copy the public key into the client connection settings."
