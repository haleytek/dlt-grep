#!/usr/bin/env bash
# Benchmark dg vs dlt-convert -a | grep
# Usage: ./bench/hyperfine.sh [DLT_FILE]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DG="$REPO_ROOT/target/release/dg"
F="${1:-/home/falk/sources/logs/SPA3/ARTHTP-6395/ECU1_DHU_UXC_HPA_SGA_TCA_2026-05-03T23_50-21.958799TZ02-00.dlt}"
RESULTS_DIR="$REPO_ROOT/bench/results"

# ── pre-flight ────────────────────────────────────────────────────────────────
[[ -f "$DG" ]]       || { echo "release binary not found — run: cargo build --release"; exit 1; }
[[ -f "$F"  ]]       || { echo "DLT file not found: $F"; exit 1; }
command -v dlt-convert >/dev/null || { echo "dlt-convert not found"; exit 1; }

mkdir -p "$RESULTS_DIR"
echo "File : $F"
echo "Size : $(du -sh "$F" | cut -f1)"
echo ""

# ── helpers ───────────────────────────────────────────────────────────────────
run() {
    local slug="$1"; shift
    local md="$RESULTS_DIR/${slug}.md"

    echo "══════════════════════════════════════════════"
    echo "  $slug"
    echo "══════════════════════════════════════════════"

    hyperfine \
        --warmup 1 \
        --min-runs 3 \
        --export-markdown "$md" \
        "$@"

    echo ""
    echo "  → $md"
    echo ""
}

# ── benchmarks ────────────────────────────────────────────────────────────────

# 1. Baseline: match everything (measures pure parse cost)
run "01_match_all" \
    --command-name "dg '.'" \
    "$DG -c '.' '$F'" \
    --command-name "dlt-convert | grep '.'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '.'"

# 2. Common literal
run "02_literal_error" \
    --command-name "dg 'error'" \
    "$DG -c 'error' '$F'" \
    --command-name "dlt-convert | grep 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c 'error'"

# 3. Case-insensitive
run "03_icase_error" \
    --command-name "dg -i 'error'" \
    "$DG -ic 'error' '$F'" \
    --command-name "dlt-convert | grep -i 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -ic 'error'"

# 4. Anchored / ECU filter
run "04_ecu_HPA" \
    --command-name "dg '^…HPA'" \
    "$DG -c '^[0-9.]+ HPA ' '$F'" \
    --command-name "dlt-convert | grep 'HPA'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c ' HPA'"

# 5. No matches (fast-exit path)
run "05_no_match" \
    --command-name "dg '__sentinel__'" \
    "$DG -c '__unlikely_sentinel__' '$F'; true" \
    --command-name "dlt-convert | grep '__sentinel__'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '__unlikely_sentinel__'; true"

echo "All results written to $RESULTS_DIR/"
