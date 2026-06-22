#!/usr/bin/env bash
# run_unicode_diff.sh — rustty-unicode differential gate (ORACLE #2, G4-class).
#
# The faithful, full-coverage reproduction of ghostty's
#   props_uucode.zig  test "unicode props: tables match uucode"
#   symbols_uucode.zig test "unicode symbols: tables match uucode"
# (and the grapheme.zig precompute / grapheme.zig main() uucode diff), whose
# LIVE external oracle (uucode) R3 forbids as an in-crate Cargo dep. Here the
# uucode oracle is an INDEPENDENT second source built in a separate process.
#
# Pipeline:
#   1. zig build the `uucode-oracle` (uucode 0.2.0 + ghostty's uucode_config.zig)
#   2. run it: dump width / is_symbol / Properties fields / grapheme_break /
#      8192-entry grapheme precompute for every cp 0..=0x10FFFF -> oracle dir
#   3. cargo run --example dump_tables: dump rustty's EMBEDDED tables, same range
#   4. diff_unicode.py: assert 0 diffs for every field
#
# Exit codes (match .port/tools/ci.sh convention):
#   0  -> GREEN  (0 diffs)
#   3  -> BLOCKED (zig toolchain unavailable; surfaced, never silently greened)
#   other non-zero -> RED (a real diff, or build failure) — a BLOCKER (R5).
#
# R3: the Zig generator + uucode dependency are harness-only. No rustty Cargo
# crate depends on uucode or ghostty.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$HERE/../../../.." && pwd)"
WORK="${UNICODE_DIFF_WORK:-$(mktemp -d)}"
ORACLE_DIR="$WORK/oracle"
RUST_DIR="$WORK/rust"
PY="${PYTHON:-python3}"

echo "=================================================================="
echo " rustty-unicode differential gate (ORACLE #2)"
echo " harness: $HERE"
echo " work:    $WORK"
echo "=================================================================="

if ! command -v zig >/dev/null 2>&1; then
  echo ">> BLOCKED: zig toolchain not found — cannot build the uucode oracle." >&2
  echo "   (install zig 0.15.2; this is harness tooling, R3, not a rustty dep)" >&2
  exit 3
fi

mkdir -p "$ORACLE_DIR" "$RUST_DIR"

echo "--- [1/4] zig build uucode-oracle ---"
( cd "$HERE" && zig build ) || { echo ">> RED: oracle build failed" >&2; exit 1; }

echo "--- [2/4] run uucode-oracle (live uucode, full range) ---"
"$HERE/zig-out/bin/uucode-oracle" "$ORACLE_DIR" || { echo ">> RED: oracle run failed" >&2; exit 1; }

echo "--- [3/4] cargo run --example dump_tables (rustty embedded tables) ---"
( cd "$RUSTTY_ROOT" && cargo run --quiet --example dump_tables -p rustty-unicode -- "$RUST_DIR" ) \
  || { echo ">> RED: rust dumper failed" >&2; exit 1; }

echo "--- [4/4] diff_unicode.py (assert 0 diffs) ---"
"$PY" "$HERE/diff_unicode.py" "$ORACLE_DIR" "$RUST_DIR"
rc=$?

if [ "${UNICODE_DIFF_WORK:-}" = "" ]; then rm -rf "$WORK"; fi
exit $rc
