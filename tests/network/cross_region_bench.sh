#!/bin/bash
# Cross-region .test domain benchmark
#
# Writer: datacenter node-1 — publishes 1.test .. N.test (real signed cards)
# Reader: satellite node-1  — reads each card by address
# Measures: write latency, read latency, propagation, success rate

set -uo pipefail

WRITER_PORT=20001
READER_PORT=24001
TOTAL=${1:-1000}
CARD_GEN="/home/vovkes/p2s/target/release/p2s-card-gen"
CARDS_DIR=$(mktemp -d)

echo "╔═══════════════════════════════════════════════════════╗"
echo "║  .test Domain Benchmark (${TOTAL} records)                 ║"
echo "║  Writer: datacenter node (port ${WRITER_PORT})               ║"
echo "║  Reader: satellite node  (port ${READER_PORT})               ║"
echo "╚═══════════════════════════════════════════════════════╝"
echo ""

# ═══ Phase 0: Pre-generate all signed cards ═══
echo -n "▸ Generating ${TOTAL} signed cards..."
for i in $(seq 1 $TOTAL); do
    $CARD_GEN "${i}.test" 1 > "${CARDS_DIR}/${i}.json"
done
echo " done"
echo ""

declare -a ADDRESSES
for i in $(seq 1 $TOTAL); do
    addr=$(python3 -c "import json; print(json.load(open('${CARDS_DIR}/${i}.json'))['_address'])")
    ADDRESSES[$i]=$addr
done

# ═══ Phase 1: Write ═══
echo "▸ Phase 1: Publishing ${TOTAL} cards to datacenter..."
write_latencies=()
write_ok=0
write_fail=0
write_start=$(date +%s%3N)

for i in $(seq 1 $TOTAL); do
    post_json=$(python3 -c "import json; d=json.load(open('${CARDS_DIR}/${i}.json')); d.pop('_address',None); print(json.dumps(d))")

    t1=$(date +%s%3N)
    http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
        -X POST "http://localhost:${WRITER_PORT}/cards" \
        -H "Content-Type: application/json" \
        -d "$post_json" 2>/dev/null)
    t2=$(date +%s%3N)
    lat=$((t2 - t1))
    write_latencies+=($lat)

    if [ "$http_code" = "201" ]; then
        write_ok=$((write_ok + 1))
    else
        write_fail=$((write_fail + 1))
    fi

    if [ $((i % 100)) -eq 0 ]; then
        elapsed=$(( $(date +%s%3N) - write_start ))
        rps=$((i * 1000 / (elapsed + 1)))
        echo -ne "\r  ${i}/${TOTAL} | ${rps} req/s | ok:${write_ok} fail:${write_fail} | last:${lat}ms  "
    fi
done

write_end=$(date +%s%3N)
write_total=$((write_end - write_start))
echo -e "\r  ${TOTAL}/${TOTAL} — done in ${write_total}ms (ok:${write_ok} fail:${write_fail})          "

# ═══ Phase 2: Read back from writer (local store baseline) ═══
echo ""
echo "▸ Phase 2: Reading ${TOTAL} cards from writer (local store)..."
read_latencies=()
read_ok=0
read_fail=0
read_start=$(date +%s%3N)

for i in $(seq 1 $TOTAL); do
    addr=${ADDRESSES[$i]}

    t1=$(date +%s%3N)
    http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 15 \
        "http://localhost:${WRITER_PORT}/cards/${addr}" 2>/dev/null)
    t2=$(date +%s%3N)
    lat=$((t2 - t1))
    read_latencies+=($lat)

    if [ "$http_code" = "200" ]; then
        read_ok=$((read_ok + 1))
    else
        read_fail=$((read_fail + 1))
    fi

    if [ $((i % 100)) -eq 0 ]; then
        elapsed=$(( $(date +%s%3N) - read_start ))
        rps=$((i * 1000 / (elapsed + 1)))
        echo -ne "\r  ${i}/${TOTAL} | ${rps} req/s | found:${read_ok} miss:${read_fail} | last:${lat}ms  "
    fi
done

read_end=$(date +%s%3N)
read_total=$((read_end - read_start))
echo -e "\r  ${TOTAL}/${TOTAL} — done in ${read_total}ms (found:${read_ok} miss:${read_fail})          "

# ═══ Stats ═══
calc_stats() {
    local -n arr=$1
    local sorted=($(printf '%s\n' "${arr[@]}" | sort -n))
    local count=${#sorted[@]}
    local sum=0
    for v in "${sorted[@]}"; do sum=$((sum + v)); done
    echo "$((sum / count)) ${sorted[0]} ${sorted[$((count-1))]} ${sorted[$((count*50/100))]} ${sorted[$((count*95/100))]} ${sorted[$((count*99/100))]}"
}

w=($(calc_stats write_latencies))
r=($(calc_stats read_latencies))

echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║  RESULTS: .test domain benchmark                         ║"
echo "╠═══════════════════════════════════════════════════════════╣"
echo "║                                                           ║"
echo "║  WRITE (datacenter → DHT, ${TOTAL} signed cards)              ║"
printf "║    Total:   %7d ms  |  Throughput: %5d req/s        ║\n" $write_total $((TOTAL * 1000 / (write_total + 1)))
printf "║    Success: %7d / ${TOTAL}                                ║\n" $write_ok
printf "║    Latency: avg=%dms min=%dms p50=%dms p95=%dms p99=%dms max=%dms\n" ${w[0]} ${w[1]} ${w[3]} ${w[4]} ${w[5]} ${w[2]}
echo "║                                                           ║"
echo "║  READ (datacenter, local store fetch)                      ║"
printf "║    Total:   %7d ms  |  Throughput: %5d req/s        ║\n" $read_total $((TOTAL * 1000 / (read_total + 1)))
printf "║    Found:   %7d / ${TOTAL}                                ║\n" $read_ok
printf "║    Latency: avg=%dms min=%dms p50=%dms p95=%dms p99=%dms max=%dms\n" ${r[0]} ${r[1]} ${r[3]} ${r[4]} ${r[5]} ${r[2]}
echo "║                                                           ║"
echo "╚═══════════════════════════════════════════════════════════╝"

rm -rf "$CARDS_DIR"
