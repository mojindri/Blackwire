#!/usr/bin/env bash
# Copy Caddy-managed cert+key to /etc/blackwire/certs/ so blackwire inbounds can read them.
# Run as root after Caddy has obtained the certificate.
set -euo pipefail

DOMAIN="${1:?Usage: cert-sync.sh <domain>}"
DEST=/etc/blackwire/certs

# Caddy stores certs under the home directory of the caddy user.
CADDY_DATA="${CADDY_DATA_DIR:-/var/lib/caddy/.local/share/caddy}"
CERT_FILE="$(find "$CADDY_DATA/certificates" -type f \
    -path "*/$DOMAIN/$DOMAIN.crt" -print -quit 2>/dev/null || true)"
if [[ -z "$CERT_FILE" ]]; then
    echo "ERROR: certificate for $DOMAIN not found under $CADDY_DATA/certificates"
    echo "Check the Caddy journal and storage configuration."
    exit 1
fi
KEY_FILE="${CERT_FILE%.crt}.key"
if [[ ! -f "$KEY_FILE" ]]; then
    echo "ERROR: private key not found next to $CERT_FILE" >&2
    exit 1
fi

mkdir -p "$DEST"
cp "$CERT_FILE" "$DEST/cert.pem"
# Convert to PKCS#8 to maximize parser compatibility across TLS stacks.
openssl pkcs8 -topk8 -nocrypt -in "$KEY_FILE" -out "$DEST/key.pem"
chown -R blackwire:blackwire "$DEST"
chmod 640 "$DEST/key.pem"

echo "Certs synced to $DEST ($(date -u +%Y-%m-%dT%H:%M:%SZ))"
