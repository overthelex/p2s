#!/bin/bash
# Remove P2S DNS config from macOS

set -euo pipefail

P2S_TLDS="${P2S_TLDS:-p2s vovkes 100500}"
RESOLVER_DIR="/etc/resolver"

if [ "$EUID" -ne 0 ]; then
    echo "Run with sudo: sudo $0"
    exit 1
fi

for tld in $P2S_TLDS; do
    if [ -f "$RESOLVER_DIR/$tld" ]; then
        rm "$RESOLVER_DIR/$tld"
        echo "Removed $RESOLVER_DIR/$tld"
    fi
done

echo "P2S DNS config removed."
