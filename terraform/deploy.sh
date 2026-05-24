#!/bin/bash
# Deploy 200-node P2S testnet with network shaping
#
# Two-phase deploy:
# 1. Start bootstrap node, get its peer ID
# 2. Start remaining 199 nodes with bootstrap-peer set
#
# Usage: ./deploy.sh [apply|destroy|status]

set -euo pipefail
cd "$(dirname "$0")"

case "${1:-apply}" in
    apply)
        echo "═══ Phase 1: Initialize and start bootstrap node ═══"
        terraform init -input=false

        # First apply: only the bootstrap node (no peer ID needed)
        terraform apply -auto-approve \
            -target=docker_network.backbone \
            -target=docker_network.region \
            -target=docker_container.bootstrap \
            -target=docker_network_connect.bootstrap_backbone

        echo ""
        echo "═══ Waiting for bootstrap node... ═══"
        for i in $(seq 1 30); do
            if curl -s "http://localhost:20001/health" >/dev/null 2>&1; then break; fi
            sleep 1
            echo -n "."
        done
        echo " up"

        BOOT_PEER_ID=$(curl -s "http://localhost:20001/node/info" | python3 -c "import sys,json; print(json.load(sys.stdin)['peer_id'])")
        echo "Bootstrap Peer ID: ${BOOT_PEER_ID}"

        echo ""
        echo "═══ Phase 2: Deploy 199 remaining nodes ═══"
        terraform apply -auto-approve \
            -var="bootstrap_peer_id=${BOOT_PEER_ID}"

        echo ""
        echo "═══ Phase 3: Apply netem shaping ═══"
        for container in $(docker ps --filter "name=p2s-" --format "{{.Names}}"); do
            docker exec "$container" sh /tmp/netem.sh 2>/dev/null && echo "  ✓ $container" || echo "  ✗ $container (netem failed — may need iproute2)"
        done

        echo ""
        echo "═══ Phase 4: Wait for DHT convergence ═══"
        echo -n "Waiting 30s for routing tables to settle..."
        sleep 30
        echo " done"

        echo ""
        echo "═══ Testnet deployed ═══"
        terraform output

        # Quick health check
        echo ""
        echo "═══ Health check (sampling 10 nodes per region) ═══"
        for base in 20001 21001 22001 23001 24001; do
            region=""
            case $base in
                20001) region="datacenter" ;;
                21001) region="broadband" ;;
                22001) region="emerging" ;;
                23001) region="mobile" ;;
                24001) region="satellite" ;;
            esac
            ok=0
            fail=0
            for i in $(seq 0 9); do
                port=$((base + i))
                if curl -s --max-time 3 "http://localhost:${port}/health" >/dev/null 2>&1; then
                    ok=$((ok + 1))
                else
                    fail=$((fail + 1))
                fi
            done
            echo "  ${region}: ${ok}/10 healthy"
        done
        ;;

    destroy)
        echo "═══ Destroying testnet ═══"
        terraform destroy -auto-approve
        echo "Done."
        ;;

    status)
        echo "═══ Testnet status ═══"
        running=$(docker ps --filter "name=p2s-" --format "{{.Names}}" | wc -l)
        echo "Running containers: ${running}/200"
        echo ""
        terraform output 2>/dev/null || echo "(terraform state not found)"
        ;;

    *)
        echo "Usage: $0 {apply|destroy|status}"
        exit 1
        ;;
esac
