#!/usr/bin/env bash
# REALITY key generation and setup guide.
#
# REALITY uses X25519 key pairs. The server has a private key and the client
# has the corresponding public key. Run this on your server to generate the keys.
#
# Usage:
#   1. Run this script: bash examples/reality-keygen.sh
#   2. In Black UI, create or edit a VLESS inbound and select REALITY security.
#   3. Paste the private key into the server-side REALITY settings.
#   4. Add a user with the generated UUID and save the revision.
#   5. Use the public key in the corresponding client subscription/profile.
set -euo pipefail

echo "=== REALITY Key Generation ==="
echo ""

# Generate a key pair using the built CLI tool.
# Output format is the labeled hexadecimal text emitted by `blackwire x25519`.
OUTPUT=$(cargo run -q --bin blackwire -- x25519 2>/dev/null)
echo "$OUTPUT"
echo ""

PRIVATE=$(echo "$OUTPUT" | sed -n 's/^Private key (server config): //p')
PUBLIC=$(echo "$OUTPUT" | sed -n 's/^Public key  (client config): //p')

echo "=== Server-side REALITY private key ==="
echo "$PRIVATE"
echo ""
echo "=== Client-side REALITY public key ==="
echo "$PUBLIC"
echo ""
echo "=== Also generate a UUID for the user list ==="
cargo run -q --bin blackwire -- uuid 2>/dev/null
echo ""
echo "Done. Save these values through Black UI's typed REALITY fields."
