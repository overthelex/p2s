#!/bin/bash
# P2S DNS setup for Linux with dnsmasq
#
# For systems without systemd-resolved: Alpine, older Debian, minimal installs.
# Adds dnsmasq rules to forward custom TLD queries to p2s-resolve.

set -euo pipefail

P2S_RESOLVER_IP="${P2S_RESOLVER_IP:-127.0.0.53}"
P2S_RESOLVER_PORT="${P2S_RESOLVER_PORT:-5353}"
P2S_TLDS="${P2S_TLDS:-p2s vovkes 100500}"

if [ "$EUID" -ne 0 ]; then
    echo "Run with sudo: sudo $0"
    exit 1
fi

if ! command -v dnsmasq &>/dev/null; then
    echo "dnsmasq not found. Install with:"
    echo "  apt install dnsmasq    # Debian/Ubuntu"
    echo "  yum install dnsmasq    # RHEL/CentOS"
    echo "  apk add dnsmasq        # Alpine"
    exit 1
fi

# Create P2S-specific dnsmasq config
cat > /etc/dnsmasq.d/p2s.conf <<EOF
# P2S custom TLD resolution
# Forward only custom TLD queries to p2s-resolve daemon
EOF

for tld in $P2S_TLDS; do
    echo "server=/$tld/$P2S_RESOLVER_IP#$P2S_RESOLVER_PORT" >> /etc/dnsmasq.d/p2s.conf
done

systemctl restart dnsmasq 2>/dev/null || service dnsmasq restart 2>/dev/null || dnsmasq --test

echo "Created /etc/dnsmasq.d/p2s.conf"
echo ""
echo "Verify with: cat /etc/dnsmasq.d/p2s.conf"
echo "Test with:   dig @127.0.0.1 myservice.p2s"
echo "Uninstall:   sudo rm /etc/dnsmasq.d/p2s.conf && sudo systemctl restart dnsmasq"
