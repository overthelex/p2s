#!/bin/bash
# P2S DNS setup for Linux with systemd-resolved
#
# Uses split DNS: only custom TLD queries go to p2s-resolve.
# System DNS unchanged for everything else.
#
# Works on: Ubuntu 18.04+, Fedora 33+, Arch, Debian 11+

set -euo pipefail

P2S_RESOLVER_IP="${P2S_RESOLVER_IP:-127.0.0.53}"
P2S_RESOLVER_PORT="${P2S_RESOLVER_PORT:-5353}"
P2S_TLDS="${P2S_TLDS:-p2s vovkes 100500}"

if [ "$EUID" -ne 0 ]; then
    echo "Run with sudo: sudo $0"
    exit 1
fi

if ! command -v resolvectl &>/dev/null; then
    echo "systemd-resolved not found. Use setup-dnsmasq.sh instead."
    exit 1
fi

# Create a drop-in config for systemd-resolved
mkdir -p /etc/systemd/resolved.conf.d

# Build the routing domains string: ~p2s ~vovkes ~100500
ROUTING_DOMAINS=""
DNS_ROUTE=""
for tld in $P2S_TLDS; do
    ROUTING_DOMAINS="$ROUTING_DOMAINS ~$tld"
done

cat > /etc/systemd/resolved.conf.d/p2s.conf <<EOF
# P2S custom TLD resolution
# Only queries for these TLDs go to the P2S resolver
[Resolve]
DNS=$P2S_RESOLVER_IP#$P2S_RESOLVER_PORT
Domains=$ROUTING_DOMAINS
EOF

systemctl restart systemd-resolved

echo "Created /etc/systemd/resolved.conf.d/p2s.conf"
echo ""
echo "Verify with: resolvectl status | grep -A5 'p2s\|vovkes'"
echo "Test with:   resolvectl query myservice.p2s"
echo "Uninstall:   sudo rm /etc/systemd/resolved.conf.d/p2s.conf && sudo systemctl restart systemd-resolved"
