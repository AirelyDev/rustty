#!/usr/bin/env bash
# check_pagelist_compress_leaks.sh — GATE: PageList compressed-page encodings are
# not leaked.
#
# WHY THIS GATE EXISTS (WS7 Phase E; DEFERRALS.md "Deferred to WS7 Phase E" #2).
# `compress::CompressedPage::encoded` is a GLOBAL-allocator `Vec`, while every
# other allocation a terminal page owns comes from the page allocator. That makes
# it invisible to every existing memory gate:
#   * `check_leaks.sh` (G6) runs Miri, which does see it — but only for crates
#     whose full Miri test run completes, and the `rustty-vt` PageList
#     compression tests are Miri-intractable (MIRI_EXCEPTIONS.md).
#   * the page-allocator leak accounting cannot see a global-allocator Vec at all.
# Concretely: delete the discard-restore loops from `PageList::deinit`/`reset` —
# the ONLY thing that frees these encodings — and the entire native `rustty-vt`
# suite stays GREEN.
#
# `.port/phase-2/mirihar-pagelist-compress` closes that hole with a counting
# `#[global_allocator]`, which a binary crate is allowed to install. Its
# `leakcheck` scenario runs NATIVELY in milliseconds (no Miri), so this gate is
# cheap enough to run every time. Before Phase E it was only ever run by hand,
# i.e. it proved leak-freedom once but could not catch a regression.
#
# FALSIFIABILITY. `--self-test` runs the harness's `leakcheck_negative` scenario,
# which reproduces the REAL leak through the REAL path (it compresses a list and
# then `mem::forget`s it instead of calling `deinit`, so no discard-restore ever
# runs) under the SAME assertion as `leakcheck`. The self-test requires that arm
# to exit non-zero and to report leaked bytes. A gate whose FAIL arm was never
# exercised would be worthless.
#
# Usage:
#   check_pagelist_compress_leaks.sh              run the gate (default)
#   check_pagelist_compress_leaks.sh --self-test  prove BOTH arms are reachable
# Exit 0 = GREEN ; 1 = RED (leak, or the FAIL arm is unreachable) ; 2 = bad usage
# ; 3 = BLOCKED (cargo unavailable — a tooling input, never a silent green).
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
HARNESS="$RUSTTY_ROOT/.port/phase-2/mirihar-pagelist-compress"

[ -f "$HARNESS/Cargo.toml" ] || {
  echo "RED: harness not found at $HARNESS"; exit 1; }
command -v cargo >/dev/null 2>&1 || {
  echo "BLOCKED: cargo not available"; exit 3; }

# The harness is a STANDALONE workspace (it is a proof artifact, deliberately
# outside the rustty root workspace) that depends on the real `rustty-vt` by
# path, so it always measures the working tree's production sources.
run_scenario() {  # $1 = scenario name; echoes output, returns the run's status
  ( cd "$HARNESS" && cargo run --quiet -- "$1" 2>&1 )
}

case "${1:-}" in
  "")
    echo "=== GATE pagelist-compress-leaks :: native counting-allocator leak check ==="
    out="$(run_scenario leakcheck)"; rc=$?
    printf '%s\n' "$out"
    if [ $rc -ne 0 ]; then
      echo "RED: PageList leaked compressed-page encodings (see the delta above)."
      echo "  The only thing that frees them is the discard-restore loop in"
      echo "  PageList::deinit / PageList::reset."
      exit 1
    fi
    echo "GREEN: compress -> reset -> compress -> deinit returns to its live-byte baseline"
    exit 0 ;;
  --self-test)
    echo "=== GATE pagelist-compress-leaks :: self-test (both arms) ==="
    # PASS arm.
    if ! run_scenario leakcheck >/dev/null 2>&1; then
      echo "self-test FAIL: the PASS arm (leakcheck) does not pass on this tree"; exit 1
    fi
    echo "self-test: PASS arm ok (leakcheck exits 0)"
    # FAIL arm — must be reachable.
    neg="$(run_scenario leakcheck_negative)"; negrc=$?
    if [ $negrc -eq 0 ]; then
      echo "self-test FAIL: the negative control did NOT fail — this gate cannot detect a leak"
      printf '%s\n' "$neg"; exit 1
    fi
    if ! printf '%s' "$neg" | grep -q 'leaked'; then
      echo "self-test FAIL: negative control exited $negrc but reported no leak"
      printf '%s\n' "$neg"; exit 1
    fi
    printf '%s\n' "$neg" | grep -o 'PageList leaked [0-9]* global-allocator bytes' | head -1
    echo "self-test: FAIL arm REACHABLE (negative control exits $negrc and reports leaked bytes)"
    exit 0 ;;
  *)
    echo "usage: check_pagelist_compress_leaks.sh [--self-test]"; exit 2 ;;
esac
