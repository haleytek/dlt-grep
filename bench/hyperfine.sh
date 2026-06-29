#!/usr/bin/env bash
# Benchmark dg vs dlt-convert — warm and cold cache
#
# Two target sets:
#   single  — one DLT file                  (first positional arg)
#   dir     — directory of *.dlt files      (second positional arg)
#
# Three competitors in the "dir" section:
#   dg <dir>                            — parallel, native
#   dlt-convert -a f1 f2 … | grep -c   — sequential (one dlt-convert call)
#   xargs -P$(nproc) dlt-convert | grep — parallel (one dlt-convert per file)
#
# Usage:
#   ./bench/hyperfine.sh SINGLE_DLT_FILE DLT_DIR
#
# Pass '-' as SINGLE_DLT_FILE to skip the single-file benchmarks.
#
# Examples:
#   ./bench/hyperfine.sh /logs/ECU1.dlt /logs/bugreport/
#   ./bench/hyperfine.sh - ~/Downloads
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DG="$REPO_ROOT/target/release/dg"
RESULTS_DIR="$REPO_ROOT/bench/results"
NPROC=$(nproc)

if [[ $# -lt 2 ]]; then
    echo "Usage: $0 SINGLE_DLT_FILE DLT_DIR"
    echo "       Pass '-' as SINGLE_DLT_FILE to skip single-file benchmarks."
    exit 1
fi

F="$1"
DL_DIR="$2"

# ── pre-flight ────────────────────────────────────────────────────────────────
[[ -f "$DG" ]] || { echo "release binary not found — run: cargo build --release"; exit 1; }
if [[ "$F" != "-" ]]; then
    [[ -f "$F" ]] || { echo "DLT file not found: $F"; exit 1; }
fi
[[ -d "$DL_DIR" ]] || { echo "DLT directory not found: $DL_DIR"; exit 1; }
command -v dlt-convert >/dev/null || { echo "dlt-convert not found"; exit 1; }
echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null 2>&1 \
    || { echo "drop_caches requires: $(whoami) ALL=(ALL) NOPASSWD: /usr/bin/tee /proc/sys/vm/drop_caches"; exit 1; }

mkdir -p "$RESULTS_DIR"

# ── discover DLT files in DL_DIR once (not inside timed loops) ───────────────
# mapfile -t uses newline as delimiter; safe since these paths have no newlines.
# The null-delimited filelist for xargs is written separately.
mapfile -t DL_ARRAY < <(find "$DL_DIR" -name '*.dlt' | sort)
DL_COUNT="${#DL_ARRAY[@]}"
DL_BYTES=$(find "$DL_DIR" -name '*.dlt' -printf '%s\n' | awk '{s+=$1} END{print s}')
DL_SIZE_MB=$(awk "BEGIN{printf \"%.1f\", $DL_BYTES/1048576}")
DL_ARGS="${DL_ARRAY[*]}"

DL_FILELIST=$(mktemp /tmp/dg-bench-XXXXXX)
trap 'rm -f "$DL_FILELIST"' EXIT
find "$DL_DIR" -name '*.dlt' -print0 | sort -z > "$DL_FILELIST"

# ── summary ───────────────────────────────────────────────────────────────────
if [[ "$F" != "-" ]]; then
    echo "Single file : $F"
    echo "            : $(du -sh "$F" | cut -f1)"
    echo ""
fi
echo "Dir         : $DL_DIR"
printf "            : %d files, %s MB\n" "$DL_COUNT" "$DL_SIZE_MB"
printf "            : %s\n" "${DL_ARRAY[@]}"
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

# Cold-cache helper for a single large file: one run only — dlt-convert takes
# ~94 s per run and statistical precision doesn't justify waiting 5+ minutes.
run_cold_single() {
    local slug="$1"; shift
    local md="$RESULTS_DIR/${slug}.md"
    echo "══════════════════════════════════════════════"
    echo "  $slug  (cold cache, 1 run)"
    echo "══════════════════════════════════════════════"
    hyperfine --prepare "$DROP_CACHES" -r 1 --export-markdown "$md" "$@"
    echo "  → $md"; echo ""
}

# Cold-cache helper for directory sets: 3 runs (files are small, runs are fast).
run_cold() {
    local slug="$1"; shift
    local md="$RESULTS_DIR/${slug}.md"
    echo "══════════════════════════════════════════════"
    echo "  $slug  (cold cache)"
    echo "══════════════════════════════════════════════"
    hyperfine --prepare "$DROP_CACHES" --min-runs 3 --export-markdown "$md" "$@"
    echo "  → $md"; echo ""
}

# dlt-convert sequential: all files in one invocation
seq_dl() {
    printf "dlt-convert -a %s 2>/dev/null | grep -c '%s'; true" "$DL_ARGS" "$1"
}

# dlt-convert parallel: one process per file via xargs
par_dl() {
    printf "xargs -0 -P%d -I{} dlt-convert -a {} 2>/dev/null < '%s' | grep -c '%s'; true" \
        "$NPROC" "$DL_FILELIST" "$1"
}

# Short display name for the dir arg (basename, not full path)
DL_LABEL="$(basename "$DL_DIR")"

# ══════════════════════════════════════════════════════════════════════════════
# SINGLE-FILE — warm  (skipped when F == -)
# ══════════════════════════════════════════════════════════════════════════════
if [[ "$F" != "-" ]]; then

run_warm "warm_single_file_01_match_all" \
    --command-name "dg '.'" \
    "$DG -c '.' '$F'" \
    --command-name "dlt-convert | grep '.'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '.'"

run_warm "warm_single_file_02_literal_error" \
    --command-name "dg 'error'" \
    "$DG -c 'error' '$F'" \
    --command-name "dlt-convert | grep 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c 'error'"

run_warm "warm_single_file_03_icase_error" \
    --command-name "dg -i 'error'" \
    "$DG -ic 'error' '$F'" \
    --command-name "dlt-convert | grep -i 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -ic 'error'"

run_warm "warm_single_file_04_ecu_HPA" \
    --command-name "dg '^…HPA'" \
    "$DG -c '^[0-9.]+ HPA ' '$F'" \
    --command-name "dlt-convert | grep 'HPA'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c ' HPA'"

run_warm "warm_single_file_05_no_match" \
    --command-name "dg '__sentinel__'" \
    "$DG -c '__unlikely_sentinel__' '$F'; true" \
    --command-name "dlt-convert | grep '__sentinel__'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '__unlikely_sentinel__'; true"

# ══════════════════════════════════════════════════════════════════════════════
# SINGLE-FILE — cold
# ══════════════════════════════════════════════════════════════════════════════

run_cold_single "cold_single_file_01_match_all" \
    --command-name "dg '.'" \
    "$DG -c '.' '$F'" \
    --command-name "dlt-convert | grep '.'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '.'"

run_cold_single "cold_single_file_02_literal_error" \
    --command-name "dg 'error'" \
    "$DG -c 'error' '$F'" \
    --command-name "dlt-convert | grep 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c 'error'"

run_cold_single "cold_single_file_03_icase_error" \
    --command-name "dg -i 'error'" \
    "$DG -ic 'error' '$F'" \
    --command-name "dlt-convert | grep -i 'error'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -ic 'error'"

run_cold_single "cold_single_file_04_ecu_HPA" \
    --command-name "dg '^…HPA'" \
    "$DG -c '^[0-9.]+ HPA ' '$F'" \
    --command-name "dlt-convert | grep 'HPA'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c ' HPA'"

run_cold_single "cold_single_file_05_no_match" \
    --command-name "dg '__sentinel__'" \
    "$DG -c '__unlikely_sentinel__' '$F'; true" \
    --command-name "dlt-convert | grep '__sentinel__'" \
    "dlt-convert -a '$F' 2>/dev/null | grep -c '__unlikely_sentinel__'; true"

fi  # end single-file section

# ══════════════════════════════════════════════════════════════════════════════
# MULTI-FILE — warm
# ══════════════════════════════════════════════════════════════════════════════

run_warm "warm_dir_01_match_all" \
    --command-name "dg '.' $DL_LABEL" \
    "$DG -c '.' '$DL_DIR'" \
    --command-name "dlt-convert(seq) | grep '.'" \
    "$(seq_dl '.')" \
    --command-name "dlt-convert(par,${NPROC}) | grep '.'" \
    "$(par_dl '.')"

run_warm "warm_dir_02_literal_error" \
    --command-name "dg 'error' $DL_LABEL" \
    "$DG -c 'error' '$DL_DIR'" \
    --command-name "dlt-convert(seq) | grep 'error'" \
    "$(seq_dl 'error')" \
    --command-name "dlt-convert(par,${NPROC}) | grep 'error'" \
    "$(par_dl 'error')"

run_warm "warm_dir_03_no_match" \
    --command-name "dg '__sentinel__' $DL_LABEL" \
    "$DG -c '__unlikely_sentinel__' '$DL_DIR'; true" \
    --command-name "dlt-convert(seq) | grep '__sentinel__'" \
    "$(seq_dl '__unlikely_sentinel__')" \
    --command-name "dlt-convert(par,${NPROC}) | grep '__sentinel__'" \
    "$(par_dl '__unlikely_sentinel__')"

# ══════════════════════════════════════════════════════════════════════════════
# MULTI-FILE — cold
# ══════════════════════════════════════════════════════════════════════════════

run_cold "cold_dir_01_match_all" \
    --command-name "dg '.' $DL_LABEL" \
    "$DG -c '.' '$DL_DIR'" \
    --command-name "dlt-convert(seq) | grep '.'" \
    "$(seq_dl '.')" \
    --command-name "dlt-convert(par,${NPROC}) | grep '.'" \
    "$(par_dl '.')"

run_cold "cold_dir_02_literal_error" \
    --command-name "dg 'error' $DL_LABEL" \
    "$DG -c 'error' '$DL_DIR'" \
    --command-name "dlt-convert(seq) | grep 'error'" \
    "$(seq_dl 'error')" \
    --command-name "dlt-convert(par,${NPROC}) | grep 'error'" \
    "$(par_dl 'error')"

run_cold "cold_dir_03_no_match" \
    --command-name "dg '__sentinel__' $DL_LABEL" \
    "$DG -c '__unlikely_sentinel__' '$DL_DIR'; true" \
    --command-name "dlt-convert(seq) | grep '__sentinel__'" \
    "$(seq_dl '__unlikely_sentinel__')" \
    --command-name "dlt-convert(par,${NPROC}) | grep '__sentinel__'" \
    "$(par_dl '__unlikely_sentinel__')"

echo "All results written to $RESULTS_DIR/"
