#!/usr/bin/env bash
# Benchmark dg vs dlt-convert -a | grep — warm and cold cache
# Usage: ./bench/hyperfine.sh [DLT_FILE]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DG="$REPO_ROOT/target/release/dg"
F="${1:-/home/falk/sources/logs/SPA3/ARTHTP-6395/ECU1_DHU_UXC_HPA_SGA_TCA_2026-05-03T23_50-21.958799TZ02-00.dlt}"
RESULTS_DIR="$REPO_ROOT/bench/results"

# ── pre-flight ────────────────────────────────────────────────────────────────
[[ -f "$DG" ]]         || { echo "release binary not found — run: cargo build --release"; exit 1; }
[[ -f "$F"  ]]         || { echo "DLT file not found: $F"; exit 1; }
command -v dlt-convert >/dev/null || { echo "dlt-convert not found"; exit 1; }
echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null 2>&1 \
    || { echo "drop_caches requires: falk ALL=(ALL) NOPASSWD: /usr/bin/tee /proc/sys/vm/drop_caches"; exit 1; }

mkdir -p "$RESULTS_DIR"
echo "File : $F"
echo "Size : $(du -sh "$F" | cut -f1)"
echo ""

# ── helpers ───────────────────────────────────────────────────────────────────
DROP_CACHES="echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null"

run_warm() {
    local slug="$1"; shift
    local md="$RESULTS_DIR/${slug}.md"
    echo "══════════════════════════════════════════════"
    echo "  $slug  (warm cache)"
    echo "══════════════════════════════════════════════"
    hyperfine --warmup 1 --min-runs 3 --export-markdown "$md" "$@"
    echo "  → $md"; echo ""
}

run_cold() {
    local slug="$1"; shift
    local md="$RESULTS_DIR/${slug}.md"
    echo "══════════════════════════════════════════════"
    echo "  $slug  (cold cache)"
    echo "══════════════════════════════════════════════"
    hyperfine --prepare "$DROP_CACHES" --min-runs 3 --export-markdown "$md" "$@"
    echo "  → $md"; echo ""
}

# ── warm-cache benchmarks ─────────────────────────────────────────────────────

run_warm "warm_01_match_all" \
    --command-name "dg '.'" \
    "$DG -c '.' '$F'" \
    --command-name "dlt-convert | grep '.'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '.'"

run_warm "warm_02_literal_error" \
    --command-name "dg 'error'" \
    "$DG -c 'error' '$F'" \
    --command-name "dlt-convert | grep 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c 'error'"

run_warm "warm_03_icase_error" \
    --command-name "dg -i 'error'" \
    "$DG -ic 'error' '$F'" \
    --command-name "dlt-convert | grep -i 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -ic 'error'"

run_warm "warm_04_ecu_HPA" \
    --command-name "dg '^…HPA'" \
    "$DG -c '^[0-9.]+ HPA ' '$F'" \
    --command-name "dlt-convert | grep 'HPA'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c ' HPA'"

run_warm "warm_05_no_match" \
    --command-name "dg '__sentinel__'" \
    "$DG -c '__unlikely_sentinel__' '$F'; true" \
    --command-name "dlt-convert | grep '__sentinel__'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '__unlikely_sentinel__'; true"

# ── cold-cache benchmarks ─────────────────────────────────────────────────────

run_cold "cold_01_match_all" \
    --command-name "dg '.'" \
    "$DG -c '.' '$F'" \
    --command-name "dlt-convert | grep '.'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '.'"

run_cold "cold_02_literal_error" \
    --command-name "dg 'error'" \
    "$DG -c 'error' '$F'" \
    --command-name "dlt-convert | grep 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c 'error'"

run_cold "cold_03_icase_error" \
    --command-name "dg -i 'error'" \
    "$DG -ic 'error' '$F'" \
    --command-name "dlt-convert | grep -i 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -ic 'error'"

run_cold "cold_04_ecu_HPA" \
    --command-name "dg '^…HPA'" \
    "$DG -c '^[0-9.]+ HPA ' '$F'" \
    --command-name "dlt-convert | grep 'HPA'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c ' HPA'"

run_cold "cold_05_no_match" \
    --command-name "dg '__sentinel__'" \
    "$DG -c '__unlikely_sentinel__' '$F'; true" \
    --command-name "dlt-convert | grep '__sentinel__'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '__unlikely_sentinel__'; true"

echo "All results written to $RESULTS_DIR/"
