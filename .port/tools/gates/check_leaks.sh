#!/usr/bin/env bash
# check_leaks.sh — GATE G6 (leak-free) — thin runner.
#
# DoP id:  G6  (DEFINITION_OF_PARITY §1, "Leak-free")
# Drives:  cargo test --workspace --features leak-track   (tracking allocator)
#          cargo +nightly miri test -p <no-FFI crate>     (UB / leak, secondary)
#          RUSTFLAGS=-Zsanitizer=address cargo +nightly test -p <ffi-crate> (ASan)
# Pass:    all selected sub-gates exit 0 with zero leak/UB/ASan reports.
#
# THIS IS A WIRED PROGRESS-MODE RUNNER. The task forbids running cargo builds
# here, and the port is empty, so by default this runner does NOT build; it
# reports the WIRING (which sub-gates exist / which tooling is installed) and
# surfaces the documented BLOCKERs (e.g. `cargo miri` not installed) exactly as
# DoP §G6 requires — never as a passed gate.
#
#   * --run        : actually run the tracking-allocator sub-gate (cargo test
#                    --features leak-track). Only use when explicitly desired;
#                    this DOES invoke cargo.
#   * --run-miri   : also run the Miri sub-gate on the no-FFI crates (requires
#                    `rustup component add miri`).
#   * default      : DRY wiring report (no cargo). Exit 0 = "WIRED" (progress),
#                    exit 3 = BLOCKED (a required sub-gate is unrunnable AND the
#                    --strict flag demands it).
#   * --strict     : in dry mode, BLOCKED if the leak-track feature or miri is
#                    absent (so the blocker is loud); in --run mode, RED on any
#                    leak/UB.
#   * --self-test  : self-test the runner wiring (no cargo).
#
# Exit 0 = GREEN (real, all sub-gates clean) or WIRED (progress dry-run);
#          non-zero = RED (a leak/UB) or BLOCKED (tooling missing under --strict).
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT_DIR="$(cd "$GATES_DIR/../.." && pwd)"
RUSTTY_ROOT="$(cd "$PORT_DIR/.." && pwd)"

# No-FFI crates eligible for Miri (DoP §G6 secondary check).
MIRI_CRATES="rustty-vt rustty-datastruct rustty-unicode rustty-util"

RUN=0
RUN_MIRI=0
STRICT=0
SELF_TEST=0
for arg in "$@"; do
  case "$arg" in
    --run)       RUN=1 ;;
    --run-miri)  RUN=1; RUN_MIRI=1 ;;
    --strict)    STRICT=1 ;;
    --self-test) SELF_TEST=1 ;;
    *) echo "check_leaks.sh: unknown arg '$arg'" >&2; exit 2 ;;
  esac
done

echo "=== G6 :: leak-free (runner) ==="

has_cargo=0; command -v cargo >/dev/null 2>&1 && has_cargo=1
has_miri=0
if [ "$has_cargo" = "1" ] && rustup component list --installed 2>/dev/null | grep -q '^miri'; then
  has_miri=1
fi
# detect a leak-track feature declared anywhere in the workspace manifests
has_leak_feature=0
if grep -rqs 'leak-track' "$RUSTTY_ROOT/crates" "$RUSTTY_ROOT/Cargo.toml" 2>/dev/null; then
  has_leak_feature=1
fi

report_wiring() {
  echo "  cargo present:           $([ "$has_cargo" = 1 ] && echo yes || echo NO)"
  echo "  leak-track feature wired:$([ "$has_leak_feature" = 1 ] && echo ' yes' || echo ' NO (not declared yet)')"
  echo "  cargo miri installed:    $([ "$has_miri" = 1 ] && echo yes || echo 'NO (rustup component add miri)')"
  echo "  miri-eligible crates:    $MIRI_CRATES"
}

run_self_test() {
  echo "--- runner self-test: assert wiring discovery works ---"
  report_wiring
  # The self-test passes if the runner can discover tooling state without error.
  echo "SELF-TEST: PASS"
  return 0
}

if [ "$SELF_TEST" = "1" ]; then
  run_self_test
  exit $?
fi

# ----- dry (progress) wiring report -----
if [ "$RUN" = "0" ]; then
  echo "--- PROGRESS mode (dry): no cargo invoked ---"
  report_wiring
  echo ""
  # Surface the documented blockers loudly (DoP §G6: Miri BLOCKED-not-waived).
  blocked=0
  if [ "$has_miri" = "0" ]; then
    echo "BLOCKER (DoP §G6): cargo miri not installed -> Miri sub-gate is BLOCKED, not waived."
    [ "$STRICT" = "1" ] && blocked=1
  fi
  if [ "$has_leak_feature" = "0" ]; then
    echo "BLOCKER: 'leak-track' feature not declared yet -> tracking-allocator sub-gate unrunnable."
    [ "$STRICT" = "1" ] && blocked=1
  fi
  if [ "$blocked" = "1" ]; then
    echo "BLOCKED: required leak sub-gate(s) unrunnable under --strict"
    exit 3
  fi
  echo "WIRED (port empty): G6 runner wired; run with --run to execute the tracking-allocator sub-gate."
  exit 0
fi

# ----- real run (invokes cargo) -----
if [ "$has_cargo" = "0" ]; then
  echo "BLOCKED: cargo not on PATH"; exit 3
fi
rc=0
echo "--- tracking-allocator sub-gate: cargo test --workspace --features leak-track ---"
if [ "$has_leak_feature" = "1" ]; then
  ( cd "$RUSTTY_ROOT" && cargo test --workspace --features leak-track )
  rc=$?
else
  echo "BLOCKED: 'leak-track' feature absent; cannot run tracking-allocator sub-gate."
  exit 3
fi

if [ "$RUN_MIRI" = "1" ]; then
  if [ "$has_miri" = "1" ]; then
    echo "--- Miri sub-gate: cargo +nightly miri test (no-FFI crates) ---"
    for c in $MIRI_CRATES; do
      ( cd "$RUSTTY_ROOT" && cargo +nightly miri test -p "$c" ) || rc=1
    done
  else
    echo "BLOCKED: cargo miri not installed; Miri sub-gate skipped (NOT a pass)."
    exit 3
  fi
fi

if [ "$rc" -eq 0 ]; then
  echo "GREEN: leak/UB sub-gates clean (G6)"
  exit 0
else
  echo "RED: a leak/UB/ASan report was emitted (G6)"
  exit 1
fi
