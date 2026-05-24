#!/bin/bash
# P2S client simulator — publishes cards with random domains and zones
#
# Environment:
#   NODE_URL    — target node HTTP API (e.g. http://10.60.1.10:8080)
#   CLIENT_ID   — unique client identifier
#   CARD_COUNT  — how many cards to publish (default: 3)
#   INTERVAL    — seconds between publishes (default: 10)

set -uo pipefail

NODE_URL="${NODE_URL:-http://localhost:8080}"
CLIENT_ID="${CLIENT_ID:-client-1}"
CARD_COUNT="${CARD_COUNT:-3}"
INTERVAL="${INTERVAL:-10}"

TLDS=("p2s" "vovkes" "100500" "agent" "service" "node" "mesh" "edge" "core" "hub")
PREFIXES=("api" "auth" "data" "search" "chat" "pay" "docs" "cdn" "ws" "rpc" "ml" "compute" "store" "queue" "proxy")

echo "[${CLIENT_ID}] Starting — target: ${NODE_URL}, cards: ${CARD_COUNT}, interval: ${INTERVAL}s"

# Wait for node to be healthy
for i in $(seq 1 60); do
    if curl -s --max-time 3 "${NODE_URL}/health" >/dev/null 2>&1; then
        echo "[${CLIENT_ID}] Node is healthy"
        break
    fi
    sleep 2
done

seq_num=0
published=0

while true; do
    for c in $(seq 1 "$CARD_COUNT"); do
        seq_num=$((seq_num + 1))

        # Generate random domain
        tld=${TLDS[$((RANDOM % ${#TLDS[@]}))]}
        prefix=${PREFIXES[$((RANDOM % ${#PREFIXES[@]}))]}
        domain="${prefix}-${CLIENT_ID}.${tld}"
        endpoint="https://${domain}/mcp"

        # Generate random keys (32-byte pubkey, 64-byte sig — will fail validation but tests the API pressure)
        pubkey=$(openssl rand -hex 32)
        sig=$(openssl rand -hex 64)
        manifest_hash=$(echo -n "${domain}-${seq_num}" | sha256sum | cut -d' ' -f1)

        # Build card JSON
        card_json=$(cat <<CARD_EOF
{
  "record": {
    "pubkey": "${pubkey}",
    "seq": ${seq_num},
    "status": "active",
    "endpoint": "${endpoint}",
    "manifest_hash": "${manifest_hash}",
    "domain": "${domain}",
    "label": "${CLIENT_ID} service ${c}"
  },
  "sig": "${sig}"
}
CARD_EOF
)

        # POST to node (expect 400 since sig is random — this tests API throughput)
        http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
            -X POST "${NODE_URL}/cards" \
            -H "Content-Type: application/json" \
            -d "$card_json" 2>/dev/null)

        published=$((published + 1))

        if [ $((published % 10)) -eq 0 ]; then
            echo "[${CLIENT_ID}] Published ${published} cards (last: ${domain}, HTTP ${http_code})"
        fi
    done

    sleep "${INTERVAL}"
done
