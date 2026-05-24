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

# Apply netem: delay + jitter + loss (replace works on noqueue)
tc qdisc replace dev "$IFACE" root handle 1: netem \
    delay ${delay} ${jitter} distribution normal \
    loss ${loss}%

echo "Applied: delay=${delay} jitter=${jitter} loss=${loss}% rate=${rate} on $IFACE"
