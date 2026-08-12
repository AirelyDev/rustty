#!/usr/bin/env bash
# check_vtc_lib_isolation.sh — GATE G-VTCLIB (the lib-vt C artifact keeps the
# oracle's null decode_png default; codex-vet W36-B03)
#
# WHY THIS GATE EXISTS
# --------------------
# Ghostty compiles libghostty-vt as a SEPARATE artifact with
# `vt_options.artifact = .lib` (src/build/GhosttyZig.zig:67-68), and
# src/terminal/sys.zig:16-20 comptime-defaults `decode_png` to `null` for that
# artifact — the published lib-vt NEVER has a built-in PNG decoder, no matter
# what the app tier links. Rustty maps that artifact split onto the crate
# graph: rustty-core (the SharedDeps `.ghostty` analog) enables rustty-vt's
# `wuffs` feature (Cargo.toml dep line); rustty-vt-c (the `.lib` analog) does
# not, so an ISOLATED `cargo build -p rustty-vt-c` — the artifact-publishing
# selection — compiles a rustty-vt whose DECODE_PNG_DEFAULT is None
# (crates/rustty-vt/src/sys.rs:62-76).
#
# THE HAZARD: Cargo feature UNIFICATION. Any COMBINED selection that includes
# both tiers (`--workspace`, or `-p rustty-vt-c -p rustty-core`) resolves ONE
# rustty-vt with `wuffs` unified in, silently flipping the C artifact's
# default decoder to Some(decode_png_wuffs) — a divergence Ghostty cannot
# exhibit because its lib artifact is a separate compilation. Cargo cannot
# give one compiled rustty-vt two artifact identities, so the PUBLISH RULE is:
# librustty-vt artifacts are produced ONLY by isolated `-p rustty-vt-c`
# invocations; combined workspace builds intentionally unify and are never
# release artifacts. This gate pins both halves of that rule:
#
#   1. BUILD-GRAPH arm: the isolated rustty-vt-c resolution must not contain
#      `wuffs` (neither the `rustty-vt feature "wuffs"` edge nor the
#      rustty-wuffs crate). `cargo tree` only — no compile.
#   2. DETECTOR CONTROL (anti-vacuity, always on, still no compile): the same
#      detector MUST fire on `--features rustty-vt/wuffs` (simulated
#      unification). A grep that can no longer match its target is the gate
#      being green precisely when the hazard lands — the G-ONIG lesson.
#   3. RUNTIME arm: run the `#[ignore]`d rustty-vt-c integration test
#      `isolated_vtc_selection_keeps_lib_null_decode_default`
#      (crates/rustty-vt-c/tests/lib_feature_isolation.rs) under the isolated
#      selection: the pristine startup `rustty_vt::sys::decode_png` slot must
#      read None. "1 passed" alone is what a filter typo also prints, so the
#      test's RUSTTY_VTC_LIB_NULL_DEFAULT_RAN marker is required (the lz4
#      RUSTTY_LZ4_SLOW_RAN pattern).
#
# --self-test additionally proves arm 3's FAIL path is reachable: the same
# test re-run with `--features rustty-vt/wuffs` MUST go red (this is the one
# arm that compiles rustty-vt in an extra feature config, which is why it is
# not the default).
#
# Usage:
#   check_vtc_lib_isolation.sh              # binding (arms 1-3)
#   check_vtc_lib_isolation.sh --self-test  # prove arm 3 can fail
#
# EXIT CODES
#   0  GREEN
#   1  RED — wuffs reached the isolated vt-c graph / runtime default, a
#      detector went blind, or a probe itself broke (a broken probe is a
#      SILENT SKIP and must be RED, per the G6 done-status-probe lesson)
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

TESTNAME=isolated_vtc_selection_keeps_lib_null_decode_default
MARKER=RUSTTY_VTC_LIB_NULL_DEFAULT_RAN
# Either symptom counts: the feature edge (how a dep-line edge shows up) or
# the crate itself (how CLI/graph activation shows up — `cargo tree -e
# features` does NOT print a feature edge for CLI-enabled features, measured
# 2026-08-10, which is why the crate name is grepped too).
DETECT='rustty-vt feature "wuffs"|rustty-wuffs'

if [ "${1:-}" = "--self-test" ]; then
  echo "=== G-VTCLIB SELF-TEST (arm 3 fail path: simulated unification) ==="
  echo "--- cargo test -p rustty-vt-c --features rustty-vt/wuffs --test lib_feature_isolation (MUST FAIL)"
  out="$(cargo test -p rustty-vt-c --features rustty-vt/wuffs --test lib_feature_isolation \
          -- --ignored --exact "$TESTNAME" 2>&1)"
  rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "  RED (SELF-TEST): the null-default test PASSED with wuffs unified in." >&2
    echo "  Its fail arm is unreachable, so its green verdict means nothing." >&2
    exit 1
  fi
  # It must have failed BY THE ASSERTION, not by failing to build/select.
  if ! printf '%s\n' "$out" | grep -q "test result: FAILED\. 0 passed; 1 failed"; then
    echo "  RED (SELF-TEST): the run failed, but not via the assertion (build" >&2
    echo "  error or filter typo — a broken probe, not a proven fail arm):" >&2
    printf '%s\n' "$out" | grep -E "^(error|warning: unused)" | head -10 >&2
    exit 1
  fi
  echo "G-VTCLIB SELF-TEST: GREEN (simulated unification turns arm 3 red)"
  exit 0
fi

echo "G-VTCLIB: lib-vt C artifact keeps the oracle null decode_png default (W36-B03)"
fail=0

# --- arm 1: the isolated build graph -----------------------------------------
tree_iso="$(cargo tree -p rustty-vt-c -e features --prefix none 2>&1)"
if [ $? -ne 0 ]; then
  echo "  RED: 'cargo tree -p rustty-vt-c -e features' itself failed:" >&2
  printf '%s\n' "$tree_iso" | head -5 >&2
  exit 1
fi
hits="$(printf '%s\n' "$tree_iso" | grep -E "$DETECT" | sort -u)"
if [ -n "$hits" ]; then
  echo "  RED: wuffs reached the ISOLATED rustty-vt-c resolution — the published" >&2
  echo "  C artifact would carry a built-in PNG decoder Ghostty's lib-vt never has:" >&2
  printf '%s\n' "$hits" | sed 's/^/    /' >&2
  echo "  (a wuffs edge on rustty-vt-c's dep line? sys.zig:16-20 says lib => null)" >&2
  fail=1
else
  echo "  arm 1: isolated -p rustty-vt-c graph is wuffs-free"
fi

# --- arm 2: the detector control (cargo tree only, no compile) ---------------
tree_inj="$(cargo tree -p rustty-vt-c --features rustty-vt/wuffs -e features --prefix none 2>&1)"
if [ $? -ne 0 ]; then
  echo "  RED: the detector-control 'cargo tree --features rustty-vt/wuffs' failed:" >&2
  printf '%s\n' "$tree_inj" | head -5 >&2
  exit 1
fi
if ! printf '%s\n' "$tree_inj" | grep -qE "$DETECT"; then
  echo "  RED (VACUITY): injecting rustty-vt/wuffs did NOT trip the detector." >&2
  echo "  The grep can no longer see the thing it exists to catch (renamed" >&2
  echo "  feature/crate?), so arm 1's green is meaningless until DETECT is fixed." >&2
  fail=1
else
  echo "  arm 2: detector fires on simulated unification (control)"
fi

# --- arm 3: the runtime default under the isolated selection -----------------
out="$(cargo test -p rustty-vt-c --test lib_feature_isolation \
        -- --ignored --nocapture --exact "$TESTNAME" 2>&1)"
rc=$?
if [ "$rc" -ne 0 ]; then
  echo "  RED: the null-default test FAILED under the ISOLATED selection:" >&2
  printf '%s\n' "$out" | grep -E "^(error|test result: FAILED|---- )" | head -10 >&2
  fail=1
elif ! printf '%s\n' "$out" | grep -q 'test result: ok\. 1 passed'; then
  echo "  RED: expected exactly 1 passed test; the filter did not take:" >&2
  printf '%s\n' "$out" | grep "test result" | head -3 >&2
  fail=1
elif ! printf '%s\n' "$out" | grep -q "$MARKER"; then
  echo "  RED: test passed but never emitted its $MARKER marker —" >&2
  echo "  the assertion body did not run. This is not coverage." >&2
  fail=1
else
  echo "  arm 3: pristine rustty_vt::sys::decode_png default is None in-process"
fi

if [ "$fail" -ne 0 ]; then
  echo "G-VTCLIB: RED"
  exit 1
fi
echo "G-VTCLIB: GREEN (isolated -p rustty-vt-c stays lib-null; publish rule holds)"
