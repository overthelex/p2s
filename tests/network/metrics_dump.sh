#!/bin/bash
# Collect and aggregate metrics from all P2S nodes during benchmark
#
# Usage:
#   ./metrics_dump.sh snapshot              — one-time dump from all nodes
#   ./metrics_dump.sh watch 5               — continuous dump every 5 seconds
#   ./metrics_dump.sh bench 1000            — run benchmark with periodic metrics snapshots

set -uo pipefail

REGIONS="datacenter:20001:60 broadband:21001:50 emerging:22001:40 mobile:23001:30 satellite:24001:20"
CARD_GEN="/home/vovkes/p2s/target/release/p2s-card-gen"
REPORT_DIR="/tmp/p2s-metrics-$(date +%Y%m%d-%H%M%S)"

collect_snapshot() {
    local tag=$1
    local out_file="${REPORT_DIR}/${tag}.json"

    local total_put=0 total_put_ok=0 total_put_rej=0
    local total_get=0 total_get_found=0 total_get_miss=0
    local total_sig_ok=0 total_sig_fail=0
    local total_http=0 total_http_err=0
    local total_records=0

    declare -A region_put region_get region_records region_put_lat region_get_lat

    for spec in $REGIONS; do
        region=$(echo $spec | cut -d: -f1)
        base=$(echo $spec | cut -d: -f2)
        count=$(echo $spec | cut -d: -f3)

        r_put=0; r_get=0; r_records=0; r_put_lat=0; r_get_lat=0; r_sampled=0

        for i in $(seq 1 $count); do
            port=$((base + i))
            json=$(curl -s --max-time 2 "http://localhost:${port}/metrics/json" 2>/dev/null) || continue
            r_sampled=$((r_sampled + 1))

            c_put=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['cards']['put_total'])" 2>/dev/null || echo 0)
            c_put_ok=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['cards']['put_success'])" 2>/dev/null || echo 0)
            c_put_rej=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['cards']['put_rejected'])" 2>/dev/null || echo 0)
            c_get=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['cards']['get_total'])" 2>/dev/null || echo 0)
            c_get_found=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['cards']['get_found'])" 2>/dev/null || echo 0)
            c_get_miss=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['cards']['get_not_found'])" 2>/dev/null || echo 0)
            c_records=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['dht']['records_stored'])" 2>/dev/null || echo 0)
            c_sig_ok=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['signature']['verify_ok'])" 2>/dev/null || echo 0)
            c_sig_fail=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['signature']['verify_fail'])" 2>/dev/null || echo 0)
            c_http=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['http']['requests_total'])" 2>/dev/null || echo 0)
            c_http_err=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['http']['errors_total'])" 2>/dev/null || echo 0)
            c_put_lat=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['latency_us']['put_avg'])" 2>/dev/null || echo 0)
            c_get_lat=$(echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)['latency_us']['get_avg'])" 2>/dev/null || echo 0)

            r_put=$((r_put + c_put))
            r_get=$((r_get + c_get))
            r_records=$((r_records + c_records))
            r_put_lat=$((r_put_lat + c_put_lat))
            r_get_lat=$((r_get_lat + c_get_lat))

            total_put=$((total_put + c_put))
            total_put_ok=$((total_put_ok + c_put_ok))
            total_put_rej=$((total_put_rej + c_put_rej))
            total_get=$((total_get + c_get))
            total_get_found=$((total_get_found + c_get_found))
            total_get_miss=$((total_get_miss + c_get_miss))
            total_sig_ok=$((total_sig_ok + c_sig_ok))
            total_sig_fail=$((total_sig_fail + c_sig_fail))
            total_http=$((total_http + c_http))
            total_http_err=$((total_http_err + c_http_err))
            total_records=$((total_records + c_records))
        done

        region_put[$region]=$r_put
        region_get[$region]=$r_get
        region_records[$region]=$r_records
        region_put_lat[$region]=$(( r_sampled > 0 ? r_put_lat / r_sampled : 0 ))
        region_get_lat[$region]=$(( r_sampled > 0 ? r_get_lat / r_sampled : 0 ))
    done

    echo "╔══════════════════════════════════════════════════════════════════╗"
    echo "║  P2S Metrics Snapshot: ${tag}"
    echo "╠══════════════════════════════════════════════════════════════════╣"
    printf "║  Cards PUT:    %6d (ok: %d, rejected: %d)\n" $total_put $total_put_ok $total_put_rej
    printf "║  Cards GET:    %6d (found: %d, miss: %d)\n" $total_get $total_get_found $total_get_miss
    printf "║  HTTP total:   %6d (errors: %d)\n" $total_http $total_http_err
    printf "║  Signatures:   ok=%d  fail=%d\n" $total_sig_ok $total_sig_fail
    printf "║  DHT records:  %6d (across all nodes)\n" $total_records
    echo "║"
    echo "║  Per-region breakdown:"
    echo "║  ┌────────────┬───────┬───────┬─────────┬───────────┬───────────┐"
    echo "║  │ Region     │ PUT   │ GET   │ Records │ PUT lat   │ GET lat   │"
    echo "║  ├────────────┼───────┼───────┼─────────┼───────────┼───────────┤"
    for region in datacenter broadband emerging mobile satellite; do
        printf "║  │ %-10s │ %5d │ %5d │ %7d │ %7dμs │ %7dμs │\n" \
            "$region" "${region_put[$region]}" "${region_get[$region]}" "${region_records[$region]}" \
            "${region_put_lat[$region]}" "${region_get_lat[$region]}"
    done
    echo "║  └────────────┴───────┴───────┴─────────┴───────────┴───────────┘"
    echo "╚══════════════════════════════════════════════════════════════════╝"
}

run_bench_with_metrics() {
    local total=${1:-1000}
    local cards_dir=$(mktemp -d)
    mkdir -p "$REPORT_DIR"

    echo "═══ Generating ${total} signed cards... ═══"
    for i in $(seq 1 $total); do
        $CARD_GEN "${i}.test" 1 > "${cards_dir}/${i}.json"
    done
    echo "Done."

    echo ""
    echo "═══ Pre-benchmark snapshot ═══"
    collect_snapshot "pre-bench"

    echo ""
    echo "═══ Running benchmark: ${total} cards to datacenter ═══"

    # Background metrics collector — snapshot every 5 seconds
    (
        snap=0
        while true; do
            sleep 5
            snap=$((snap + 1))
            collect_snapshot "during-${snap}" 2>/dev/null
        done
    ) &
    METRICS_PID=$!

    # Write phase
    write_start=$(date +%s%3N)
    ok=0; fail=0
    for i in $(seq 1 $total); do
        post=$(python3 -c "import json; d=json.load(open('${cards_dir}/${i}.json')); d.pop('_address',None); print(json.dumps(d))")
        http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
            -X POST "http://localhost:20001/cards" \
            -H "Content-Type: application/json" -d "$post" 2>/dev/null)
        [ "$http_code" = "201" ] && ok=$((ok+1)) || fail=$((fail+1))
        [ $((i % 200)) -eq 0 ] && echo "  Written: ${i}/${total} (ok:${ok} fail:${fail})"
    done
    write_end=$(date +%s%3N)
    write_ms=$((write_end - write_start))
    echo "  Write complete: ${ok}/${total} in ${write_ms}ms ($(( total * 1000 / (write_ms+1) )) req/s)"

    # Read phase
    echo ""
    echo "═══ Reading ${total} cards back ═══"
    read_start=$(date +%s%3N)
    found=0; miss=0
    for i in $(seq 1 $total); do
        addr=$(python3 -c "import json; print(json.load(open('${cards_dir}/${i}.json'))['_address'])")
        http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
            "http://localhost:20001/cards/${addr}" 2>/dev/null)
        [ "$http_code" = "200" ] && found=$((found+1)) || miss=$((miss+1))
        [ $((i % 200)) -eq 0 ] && echo "  Read: ${i}/${total} (found:${found} miss:${miss})"
    done
    read_end=$(date +%s%3N)
    read_ms=$((read_end - read_start))
    echo "  Read complete: ${found}/${total} in ${read_ms}ms ($(( total * 1000 / (read_ms+1) )) req/s)"

    # Stop metrics collector
    kill $METRICS_PID 2>/dev/null
    wait $METRICS_PID 2>/dev/null

    echo ""
    echo "═══ Post-benchmark snapshot ═══"
    collect_snapshot "post-bench"

    echo ""
    echo "╔══════════════════════════════════════════════════════════╗"
    echo "║  BENCHMARK SUMMARY                                      ║"
    echo "╠══════════════════════════════════════════════════════════╣"
    printf "║  WRITE: %d/%d cards | %d ms | %d req/s               ║\n" $ok $total $write_ms $((total * 1000 / (write_ms+1)))
    printf "║  READ:  %d/%d cards | %d ms | %d req/s               ║\n" $found $total $read_ms $((total * 1000 / (read_ms+1)))
    echo "║  Metrics snapshots saved to: ${REPORT_DIR}              ║"
    echo "╚══════════════════════════════════════════════════════════╝"

    rm -rf "$cards_dir"
}

case "${1:-snapshot}" in
    snapshot)
        collect_snapshot "$(date +%H:%M:%S)"
        ;;
    watch)
        interval=${2:-5}
        while true; do
            clear
            collect_snapshot "$(date +%H:%M:%S)"
            sleep "$interval"
        done
        ;;
    bench)
        run_bench_with_metrics "${2:-1000}"
        ;;
    *)
        echo "Usage: $0 {snapshot|watch [interval]|bench [count]}"
        ;;
esac
