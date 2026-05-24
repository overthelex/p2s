#!/bin/sh
# Apply tc/netem network shaping inside the container
# Called after container start to simulate real-world network conditions
#
# Parameters (templated by Terraform):
#   delay  = ${delay}
#   jitter = ${jitter}
#   loss   = ${loss}%
#   rate   = ${rate}

IFACE=$(ip -o link show | awk -F': ' '$2 != "lo" {print $2; exit}')

if [ -z "$IFACE" ]; then
    echo "No network interface found"
    exit 1
fi

# Clean existing rules
tc qdisc del dev "$IFACE" root 2>/dev/null || true

# Apply netem: delay + jitter + loss + rate limiting
tc qdisc add dev "$IFACE" root handle 1: netem \
    delay ${delay} ${jitter} distribution normal \
    loss ${loss}%

# Add rate limiting as child qdisc
tc qdisc add dev "$IFACE" parent 1: handle 2: tbf \
    rate ${rate} \
    burst 256kb \
    latency 100ms

echo "Applied: delay=${delay} jitter=${jitter} loss=${loss}% rate=${rate} on $IFACE"
