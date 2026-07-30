#!/usr/bin/env bash
# check_sanitizer_canary.sh — GATE D6: sanitizer-visibility canary + cfg-mapping
# audit. Kills Bun class 5 (oven-sh/bun#30875) — the flattened comptime build
# switch that blinded the ASan/LSan lane.
#
# THE #30875 FAILURE MODE: Bun hard-coded `#[global_allocator] Mimalloc`, so
# ASan's malloc interceptor never saw the port's allocations; the sanitizer lane
# reported GREEN while BLIND for the whole 1.4 line. A green sanitizer gate is
# worthless unless we can PROVE it can still SEE the port's heap. So:
#
#   (a) CANARY: build the standalone .port/phase-2/d6-canary crate under
#       -Zsanitizer=address. It deliberately heap-overflows (and, on Linux,
#       leaks). A HEALTHY sanitizer lane goes RED on it. This gate INVERTS that:
#         canary fired  (ASan finding present) -> the lane can see the heap -> PASS
#         canary silent (lane stayed GREEN)    -> the lane is BLIND (#30875)  -> FAIL
#       The canary lives in its OWN crate, run only here, so this deliberately-red
#       proof is filtered out of the real sanitizer/G6 roll-up (never counts
#       against a subsystem). We also assert the canaries compile OUT of a normal
#       (non-sanitizer) build — 0 tests — so they can never perturb cargo lanes.
#
#   (b)+(c) CFG-MAPPING + SUPPRESSION audit: d6_cfg_mapping_audit.py — every Zig
#       builtin.mode/build-option/comptime switch must map to a `cfg`, never a
#       constant-folded value (a first-party un-cfg'd `#[global_allocator]` is the
#       exact #30875 shape); and every sanitizer suppression entry needs a
#       per-entry WHY. See that script's header.
#
# Exit contract (matches ci.sh / CI.md): 0 GREEN ; 3 BLOCKED (nightly/ASan
# toolchain absent — surfaced by ci.sh as a WARNING, never a silent green) ;
# any other non-zero = RED (blind lane, compiled-out failure, or audit finding).
#
# Usage:
#   check_sanitizer_canary.sh              # full D6 gate (canary + audit)
#   check_sanitizer_canary.sh --canary     # only the sanitizer-visibility canary
#   check_sanitizer_canary.sh --audit      # only the cfg-mapping/suppression audit
#   check_sanitizer_canary.sh --self-test  # prove the canary makes ASan RED (verbose)
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
CANARY_DIR="$RUSTTY_ROOT/.port/phase-2/d6-canary"
AUDIT="$GATES_DIR/d6_cfg_mapping_audit.py"
PY="${PYTHON:-python3}"

have_nightly() { cargo +nightly --version >/dev/null 2>&1; }

# Build+run the canary crate under ASan. Echoes a verdict; returns:
#   0 = canary FIRED (ASan saw the heap-overflow) -> the lane can SEE  -> healthy
#   1 = canary SILENT (lane green / blind)         -> the #30875 failure
#   3 = BLOCKED (nightly absent or the canary failed to COMPILE under ASan)
run_canary() {
  local verbose="${1:-0}"
  if ! have_nightly; then
    echo "  ⏭  D6 canary: BLOCKED — +nightly toolchain absent (sanitizers need it)"
    return 3
  fi
  local tgt log td
  tgt="$(rustc -vV | sed -n 's/host: //p')"
  log="/tmp/d6-canary-$$.log"
  td="/tmp/d6-canary-asan"
  echo "### D6 canary: rustty-d6-canary under -Zsanitizer=address (target=$tgt) ###"
  local asan_opts="detect_leaks=0"   # WHY: macOS ASan has no LSan; this keeps the ASan header clean so ONLY the heap-overflow canary can turn the lane red (the LSan leak canary fires on the Linux leak lane)
  env ASAN_OPTIONS="$asan_opts" RUSTFLAGS="-Zsanitizer=address" \
      CARGO_TARGET_DIR="$td" \
      cargo +nightly test --manifest-path "$CANARY_DIR/Cargo.toml" --target "$tgt" \
      >"$log" 2>&1
  local rc=$?
  [ "$verbose" = 1 ] && { echo "----- ASan output (head) -----"; grep -E \
      'ERROR: AddressSanitizer|heap-buffer-overflow|READ of size|SUMMARY:|test result' \
      "$log" | head -8; echo "------------------------------"; }

  if grep -qE 'error\[|could not compile|error: could not|linking with' "$log" \
     && ! grep -qE 'ERROR: AddressSanitizer' "$log"; then
    echo "  ⏭  D6 canary: BLOCKED — canary failed to COMPILE under ASan (toolchain):"
    grep -m2 -E 'error(\[|:)' "$log" | sed 's/^/       /'
    return 3
  fi
  if grep -qE 'ERROR: (Address|Leak)Sanitizer|heap-buffer-overflow' "$log"; then
    echo "  ✅ D6 canary FIRED: the sanitizer lane SEES the port's heap (rc=$rc)"
    echo "     $(grep -m1 -E 'SUMMARY: AddressSanitizer' "$log" | sed 's/^ *//')"
    rm -f "$log"; return 0
  fi
  # Clean pass with NO finding == the lane is BLIND. This is the #30875 shape.
  echo "  ❌ D6 canary SILENT: the ASan/LSan lane is BLIND to the port's heap."
  echo "     A deliberate heap-overflow was NOT detected — the sanitizer gate"
  echo "     cannot see allocations (e.g. a custom global allocator bypassing the"
  echo "     interceptor, per oven-sh/bun#30875). A green sanitizer lane is"
  echo "     therefore MEANINGLESS. Fix the allocator/cfg wiring before merge."
  echo "     log: $log"
  return 1
}

# Assert the canaries are compiled OUT of a normal (non-sanitizer) build, so they
# can never leak into `cargo test --workspace` or the roll-up (0 canary tests).
assert_compiled_out() {
  have_nightly || { echo "  ⏭  compiled-out check: BLOCKED (no +nightly)"; return 3; }
  local log="/tmp/d6-canary-plain-$$.log"
  CARGO_TARGET_DIR=/tmp/d6-canary-plain \
    cargo +nightly test --manifest-path "$CANARY_DIR/Cargo.toml" >"$log" 2>&1
  # Every `running N tests` line must be `running 0 tests` outside a sanitizer build.
  if grep -qE 'running [1-9][0-9]* tests' "$log"; then
    echo "  ❌ D6: canary tests are NOT sanitizer-gated — they compile into a"
    echo "        normal build and would pollute the cargo lanes / roll-up."
    rm -f "$log"; return 1
  fi
  echo "  ✅ D6: canaries compile OUT of a non-sanitizer build (0 tests) — they"
  echo "        cannot pollute cargo lanes nor the sanitizer roll-up."
  rm -f "$log"; return 0
}

run_audit() {
  [ -f "$AUDIT" ] || { echo "  ⏭  cfg-mapping audit: BLOCKED — $AUDIT missing"; return 3; }
  "$PY" "$AUDIT"
}

worst=0        # 0 green, 1 red, 3 blocked; RED dominates BLOCKED
merge() { case "$1" in 1) worst=1;; 3) [ "$worst" -ne 1 ] && worst=3;; esac; }

case "${1:-}" in
  --self-test)
    # BOTH proofs: (a) the canary makes ASan RED, and (b)+(c) every audit arm
    # can still produce a finding (with a documented tree as the negative
    # control). A gate whose FAIL arm is unreachable is worse than no gate.
    run_canary 1; merge $?
    echo "== D6 (b)+(c) audit self-test =="
    "$PY" "$AUDIT" --self-test; merge $?
    exit "$worst" ;;
  --canary)
    run_canary 0; merge $?; assert_compiled_out; merge $?
    exit "$worst" ;;
  --audit)
    run_audit; exit $? ;;
  ''|--all|--full)
    echo "== D6 (a) sanitizer-visibility canary =="
    run_canary 0; merge $?
    assert_compiled_out; merge $?
    echo "== D6 (b)+(c) cfg-mapping + suppression audit =="
    run_audit; merge $?
    echo "------------------------------------------------------------------"
    case "$worst" in
      0) echo "D6: GREEN (sanitizer lane proven visible; cfg/suppression audit clean)";;
      3) echo "D6: BLOCKED (toolchain input pending; no finding — surfaced, not greened)";;
      *) echo "D6: RED (see findings above)";;
    esac
    exit "$worst" ;;
  *)
    echo "usage: check_sanitizer_canary.sh [--canary | --audit | --self-test]"; exit 2 ;;
esac
