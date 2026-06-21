#!/usr/bin/env bash
# diff_vt.sh — Thin entry point for the differential VT comparator.
#
# Given a VT byte stream (a file argument, or stdin), runs it through BOTH:
#   * the REFERENCE  (libghostty-vt via bin/vt_driver_ref), and
#   * the PORT       (port_hook/vt_driver_port.sh — a stub while rustty is empty)
# with IDENTICAL grid/format options, then diffs the canonical Screen/PageList
# serialization and prints the divergence %.
#
# Usage:
#   ./diff_vt.sh [comparator-flags] [INPUT_FILE]
#   printf '...VT bytes...' | ./diff_vt.sh [comparator-flags]
#
# Examples:
#   printf 'Hello\r\nWorld\r\n' | ./diff_vt.sh --show-diff
#   ./diff_vt.sh --format vt --cols 80 --rows 24 fixtures/basic_text.vt
#   ./diff_vt.sh --json fixtures/sgr_styles.vt
#
# All flags are forwarded verbatim to comparator.py (see its --help). Exit code
# is 0 iff the canonical outputs are byte-identical, 1 otherwise.
set -euo pipefail

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="${PYTHON:-python3}"

REF="$HARNESS_DIR/bin/vt_driver_ref"
if [ ! -x "$REF" ]; then
  echo "diff_vt.sh: reference driver not built ($REF)." >&2
  echo "  -> run: bash $HARNESS_DIR/build_ref_driver.sh" >&2
  exit 11
fi

exec "$PY" "$HARNESS_DIR/comparator.py" "$@"
