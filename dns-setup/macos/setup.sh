#!/bin/bash
# P2S DNS setup for macOS
#
# macOS natively supports per-TLD resolvers via /etc/resolver/<tld>
# No need to change the system DNS — only custom TLD queries go to p2s-resolve.
# Everything else uses the default resolver untouched.

set -euo pipefail

P2S_RESOLVER_IP="${P2S_RESOLVER_IP:-127.0.0.53}"
P2S_RESOLVER_PORT="${P2S_RESOLVER_PORT:-5353}"
P2S_TLDS="${P2S_TLDS:-p2s vovkes 100500}"

RESOLVER_DIR="/etc/resolver"

if [ "$EUID" -ne 0 ]; then
    echo "Run with sudo: sudo $0"
    exit 1
fi

mkdir -p "$RESOLVER_DIR"

for tld in $P2S_TLDS; do
    cat > "$RESOLVER_DIR/$tld" <<EOF
# P2S custom TLD resolver for .$tld
nameserver $P2S_RESOLVER_IP
port $P2S_RESOLVER_PORT
search_order 1
timeout 2
EOF
    echo "Created $RESOLVER_DIR/$tld"
done

echo ""
echo "Done. Verify with: scutil --dns | grep -A5 'p2s\|vovkes\|100500'"
echo "Test with:   dig @$P2S_RESOLVER_IP -p $P2S_RESOLVER_PORT myservice.p2s"
echo "Uninstall:   sudo $0 --remove"
