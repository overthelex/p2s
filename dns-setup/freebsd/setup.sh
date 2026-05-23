#!/bin/sh
# P2S DNS setup for FreeBSD / generic Unix
#
# Uses local-unbound (default on FreeBSD) or unbound.
# Adds stub-zone for each custom TLD → forward to p2s-resolve.

set -eu

P2S_RESOLVER_IP="${P2S_RESOLVER_IP:-127.0.0.53}"
P2S_RESOLVER_PORT="${P2S_RESOLVER_PORT:-5353}"
P2S_TLDS="${P2S_TLDS:-p2s vovkes 100500}"

if [ "$(id -u)" -ne 0 ]; then
    echo "Run as root: sudo $0"
    exit 1
fi

# Detect unbound config location
if [ -d /var/unbound/conf.d ]; then
    CONF_DIR="/var/unbound/conf.d"        # FreeBSD local-unbound
elif [ -d /etc/unbound/unbound.conf.d ]; then
    CONF_DIR="/etc/unbound/unbound.conf.d" # Linux unbound
else
    mkdir -p /etc/unbound/unbound.conf.d
    CONF_DIR="/etc/unbound/unbound.conf.d"
fi

cat > "$CONF_DIR/p2s.conf" <<HEADER
# P2S custom TLD resolution
# Forward only custom TLD queries to p2s-resolve daemon
HEADER

for tld in $P2S_TLDS; do
    cat >> "$CONF_DIR/p2s.conf" <<EOF

stub-zone:
    name: "$tld."
    stub-addr: $P2S_RESOLVER_IP@$P2S_RESOLVER_PORT
    stub-first: yes
EOF
done

# Restart unbound
if command -v service >/dev/null 2>&1; then
    service local_unbound restart 2>/dev/null || service unbound restart 2>/dev/null || true
elif command -v systemctl >/dev/null 2>&1; then
    systemctl restart unbound
fi

echo "Created $CONF_DIR/p2s.conf"
echo ""
echo "Verify with: unbound-checkconf"
echo "Test with:   drill @127.0.0.1 myservice.p2s"
echo "Uninstall:   sudo rm $CONF_DIR/p2s.conf && sudo service local_unbound restart"
