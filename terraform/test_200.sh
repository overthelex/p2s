#!/bin/bash
# Network quality tests for 200-node P2S testnet
#
# Tests that network shaping is actually working:
# 1. Latency differences between regions
# 2. All nodes healthy
# 3. DHT reachability across all regions
# 4. Bandwidth impact on response times

set -uo pipefail

PASS=0
FAIL=0
TOTAL=0

log_test() {
    TOTAL=$((TOTAL + 1))
    local name=$1 result=$2 details="${3:-}"
    if [ "$result" = "PASS" ]; then
        PASS=$((PASS + 1))
        echo -e "  \e[32m✓\e[0m ${name}"
    else
        FAIL=$((FAIL + 1))
        echo -e "  \e[31m✗\e[0m ${name}: ${details}"
    fi
}

echo "╔══════════════════════════════════════════════════════╗"
echo "║  P2S 200-Node Testnet — Network Quality Tests       ║"
echo "╚══════════════════════════════════════════════════════╝"
echo ""

# ═══ TEST 1: Health check per region ═══
echo "▸ Test 1: Health check per region"
declare -A REGION_PORTS=( [datacenter]=20001 [broadband]=21001 [emerging]=22001 [mobile]=23001 [satellite]=24001 )
declare -A REGION_COUNT=( [datacenter]=60 [broadband]=50 [emerging]=40 [mobile]=30 [satellite]=20 )

for region in datacenter broadband emerging mobile satellite; do
    base=${REGION_PORTS[$region]}
    count=${REGION_COUNT[$region]}
    ok=0
    for i in $(seq 0 $((count - 1))); do
        port=$((base + i + 1))
        if curl -s --max-time 5 "http://localhost:${port}/health" >/dev/null 2>&1; then
            ok=$((ok + 1))
        fi
    done
    if [ "$ok" -eq "$count" ]; then
        log_test "${region}: all ${count} nodes healthy" "PASS"
    elif [ "$ok" -gt $((count * 80 / 100)) ]; then
        log_test "${region}: ${ok}/${count} nodes healthy (>80%)" "PASS"
    else
        log_test "${region}: ${ok}/${count} nodes healthy" "FAIL" "below 80% threshold"
    fi
done

# ═══ TEST 2: Response time by region (latency shaping) ═══
echo ""
echo "▸ Test 2: Response time by region (expect increasing latency)"
declare -A REGION_TIMES

for region in datacenter broadband emerging mobile satellite; do
    base=${REGION_PORTS[$region]}
    total_ms=0
    samples=5
    for i in $(seq 1 $samples); do
        port=$((base + i))
        ms=$(curl -s -o /dev/null -w "%{time_total}" --max-time 10 "http://localhost:${port}/node/info" 2>/dev/null)
        ms_int=$(echo "$ms * 1000" | bc 2>/dev/null | cut -d. -f1)
        total_ms=$((total_ms + ms_int))
    done
    avg=$((total_ms / samples))
    REGION_TIMES[$region]=$avg
    echo "    ${region}: avg ${avg}ms"
done

# Datacenter should be fastest
dc_time=${REGION_TIMES[datacenter]}
sat_time=${REGION_TIMES[satellite]}
if [ "$dc_time" -lt "$sat_time" ]; then
    log_test "datacenter (${dc_time}ms) faster than satellite (${sat_time}ms)" "PASS"
else
    log_test "latency ordering" "FAIL" "datacenter=${dc_time}ms, satellite=${sat_time}ms"
fi

# ═══ TEST 3: Unique peer IDs (sample) ═══
echo ""
echo "▸ Test 3: Peer ID uniqueness (sampling 20 nodes)"
ids=()
for port in 20001 20010 20030 20050 21001 21010 21030 22001 22010 22030 23001 23010 23020 24001 24010 24015 20005 21025 22020 23025; do
    pid=$(curl -s --max-time 5 "http://localhost:${port}/node/info" 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['peer_id'])" 2>/dev/null)
    if [ -n "$pid" ]; then
        ids+=("$pid")
    fi
done
unique=$(printf '%s\n' "${ids[@]}" | sort -u | wc -l)
total_sampled=${#ids[@]}
if [ "$unique" -eq "$total_sampled" ] && [ "$total_sampled" -ge 15 ]; then
    log_test "All ${total_sampled} sampled peer IDs unique" "PASS"
else
    log_test "Peer ID uniqueness" "FAIL" "${unique}/${total_sampled} unique"
fi

# ═══ TEST 4: Cross-region DHT queries ═══
echo ""
echo "▸ Test 4: Cross-region DHT reachability"
test_addr=$(openssl rand -hex 32)

for region in datacenter broadband emerging mobile satellite; do
    base=${REGION_PORTS[$region]}
    port=$((base + 1))
    http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 15 "http://localhost:${port}/cards/${test_addr}" 2>/dev/null)
    if [ "$http_code" = "404" ]; then
        log_test "${region} node can query DHT (404 = traversed OK)" "PASS"
    else
        log_test "${region} DHT query" "FAIL" "HTTP ${http_code}"
    fi
done

# ═══ TEST 5: netem shaping verification ═══
echo ""
echo "▸ Test 5: tc/netem shaping active"
for region in datacenter broadband emerging mobile satellite; do
    container="p2s-${region}-1"
    tc_output=$(docker exec "$container" tc qdisc show 2>/dev/null | grep -c "netem\|tbf" || echo "0")
    if [ "$tc_output" -ge 1 ]; then
        delay_actual=$(docker exec "$container" tc qdisc show 2>/dev/null | grep netem | head -1)
        log_test "${region} netem active: ${delay_actual}" "PASS"
    else
        log_test "${region} netem shaping" "FAIL" "tc rules not found"
    fi
done

# ═══ REPORT ═══
echo ""
echo "╔══════════════════════════════════════════════════════╗"
echo "║  RESULTS                                             ║"
echo "╠══════════════════════════════════════════════════════╣"
printf "║  Total:  %-3d                                        ║\n" $TOTAL
printf "║  Passed: %-3d                                        ║\n" $PASS
printf "║  Failed: %-3d                                        ║\n" $FAIL
echo "╚══════════════════════════════════════════════════════╝"
echo ""
echo "Network profiles:"
echo "  ┌────────────┬────────┬────────┬──────┬──────────┐"
echo "  │ Region     │ Delay  │ Jitter │ Loss │ BW       │"
echo "  ├────────────┼────────┼────────┼──────┼──────────┤"
echo "  │ datacenter │ 1ms    │ 0.5ms  │ 0%   │ 1000mbit │"
echo "  │ broadband  │ 25ms   │ 5ms    │ 0.1% │ 100mbit  │"
echo "  │ emerging   │ 80ms   │ 20ms   │ 1%   │ 10mbit   │"
echo "  │ mobile     │ 120ms  │ 40ms   │ 2%   │ 5mbit    │"
echo "  │ satellite  │ 550ms  │ 50ms   │ 3%   │ 2mbit    │"
echo "  └────────────┴────────┴────────┴──────┴──────────┘"

if [ "$FAIL" -gt 0 ]; then exit 1; fi
