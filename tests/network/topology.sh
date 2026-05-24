#!/bin/bash
# P2S Network Topology — 10 nodes on isolated network with unique IPs
#
# Each node gets a unique IP on a shared subnet (simulating routed internet).
# Node-1 is the bootstrap seed. Nodes 2-10 bootstrap from node-1.
# HTTP API: localhost:9001-9010
# libp2p swarm: each node listens on :4001 inside the container

set -euo pipefail

IMAGE="docker-p2s-node:latest"
PREFIX="p2s-test"
NET="${PREFIX}-net"
SUBNET="10.55.0.0/16"

cleanup() {
    echo "=== Cleaning up ==="
    for i in $(seq 1 10); do
        docker rm -f "${PREFIX}-node-${i}" 2>/dev/null || true
    done
    docker network rm "${NET}" 2>/dev/null || true
    echo "Done."
}

create_network() {
    echo "=== Creating network (${SUBNET}) ==="
    docker network create --subnet="${SUBNET}" "${NET}"
}

start_nodes() {
    echo "=== Starting 10 nodes ==="

    # Node-1: bootstrap seed
    echo -n "  node-1 (bootstrap)..."
    docker run -d \
        --name "${PREFIX}-node-1" \
        --network "${NET}" \
        --ip "10.55.0.11" \
        -p "127.0.0.1:9001:8080" \
        "${IMAGE}" \
        --listen "/ip4/0.0.0.0/tcp/4001" \
        --http-port 8080 \
        --data-dir /data \
        > /dev/null

    for j in $(seq 1 20); do
        if curl -s "http://localhost:9001/health" > /dev/null 2>&1; then break; fi
        sleep 0.5
    done
    echo " up"

    BOOT_PEER_ID=$(curl -s "http://localhost:9001/node/info" | python3 -c "import sys,json; print(json.load(sys.stdin)['peer_id'])")
    BOOT_ADDR="/ip4/10.55.0.11/tcp/4001/p2p/${BOOT_PEER_ID}"
    echo "  Bootstrap addr: ${BOOT_ADDR}"

    # Nodes 2-10: bootstrap from node-1
    for i in $(seq 2 10); do
        node_ip="10.55.0.$((10+i))"
        http_port=$((9000+i))
        echo -n "  node-${i} (${node_ip})..."
        docker run -d \
            --name "${PREFIX}-node-${i}" \
            --network "${NET}" \
            --ip "${node_ip}" \
            -p "127.0.0.1:${http_port}:8080" \
            "${IMAGE}" \
            --listen "/ip4/0.0.0.0/tcp/4001" \
            --http-port 8080 \
            --data-dir /data \
            --bootstrap-peer "${BOOT_ADDR}" \
            > /dev/null
        echo " started"
    done

    # Wait for all healthy
    echo -n "  Waiting for all nodes..."
    for i in $(seq 2 10); do
        for j in $(seq 1 20); do
            if curl -s "http://localhost:$((9000+i))/health" > /dev/null 2>&1; then break; fi
            sleep 0.5
        done
    done
    echo " all healthy"

    # Give DHT time to exchange routing tables
    echo -n "  DHT bootstrap settling..."
    sleep 5
    echo " done"

    echo ""
    echo "  ┌─────────────────────────────────────────────────┐"
    echo "  │  10 nodes running on ${SUBNET}             │"
    echo "  │  IPs: 10.55.0.11 — 10.55.0.20                  │"
    echo "  │  HTTP: localhost:9001 — localhost:9010           │"
    echo "  │  Bootstrap: node-1 (${BOOT_PEER_ID:0:16}...)  │"
    echo "  └─────────────────────────────────────────────────┘"
}

case "${1:-start}" in
    start)
        cleanup
        create_network
        start_nodes
        ;;
    stop)
        cleanup
        ;;
    *)
        echo "Usage: $0 {start|stop}"
        ;;
esac
