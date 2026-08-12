#!/usr/bin/env bash
# check_release_lane.sh — GATE G-REL (the `--release` test lane is green) [R5]
#
# DoP id:  G-REL  (project-health gate added by WS18 "release-lane repair")
# Reads:   the real workspace — `cargo build --release --workspace` then
#          `cargo test --release --workspace --no-fail-fast`.
# Pass:    zero failing tests in the release profile.
#
# WHY THIS GATE EXISTS
# --------------------
# Several of the port's failure-injection seams are DELIBERATELY compiled out in
# release: the pagelist integrity gate / `pin_is_valid` mirror the reference's
# `build_options.slow_runtime_safety`. Tests that drive those seams therefore
# carry `#[cfg_attr(not(debug_assertions), ignore)]`. That is a correct,
# documented arrangement — but nothing enforced it, so the release lane silently
# rotted to 22 failures across 7 targets before WS18. This gate is the
# enforcement: any NEW release-only failure (a genuinely release-sensitive bug,
# or a new injection test that forgot the attribute) turns it RED.
#
# `rustty_util::tripwire` is NO LONGER in that compiled-out set for
# `rustty-util`/`rustty-vt` (W57-B03 / W59-B04): `Module::enabled()` is
# `cfg!(any(test, debug_assertions, feature = "tripwire-test"))` and `rustty-vt`
# enables the off-by-default `tripwire-test` feature as a DEV-dependency, so its
# fail-point tests RUN (not "ignored") in this release lane — reproducing Zig's
# `builtin.is_test`, which is true at ANY optimize mode. Dev-dep features do not
# reach `cargo build --release`, so production still compiles `check` away.
# Fail-point tests in crates that have not opted in yet (`rustty-font`,
# `rustty-font-shaper`) keep the `not(debug_assertions)` ignore.
#
# The build step is not incidental: `rustty-apprt-c`'s C-ABI smoke asserts the
# published cdylib/staticlib exist in `target/<profile>/`, and `cargo test`
# alone leaves those artifacts un-uplifted in `deps/`. `cargo build` first is
# the documented precondition, in BOTH profiles.
#
# The run is wrapped in a watchdog (default 45 min, `RELEASE_LANE_TIMEOUT`
# seconds). A HANG is reported RED, never waited on forever — `rustty-termio`'s
# `subprocess_drop_repro` has been observed wedging under full-workspace
# parallel load (macOS `login(1)` children stuck in kernel "exiting" state, so
# `killPid`'s faithful SIGHUP/waitpid loop spins). See WS18's report.
#
# Usage:
#   check_release_lane.sh                 # full gate (build + test, release)
#   check_release_lane.sh --crate <name>  # scope to one package (cheap smoke)
#   check_release_lane.sh --self-test     # prove the verdict flips RED
#
# Exit 0 = GREEN, 1 = RED (failures / hang), 3 = BLOCKED (no cargo).
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
TIMEOUT_SECS="${RELEASE_LANE_TIMEOUT:-2700}"

SCOPE_PKG=""
SELF_TEST=0
while [ $# -gt 0 ]; do
  case "$1" in
    --crate) SCOPE_PKG="${2:-}"; shift 2 ;;
    --self-test) SELF_TEST=1; shift ;;
    *) echo "check_release_lane.sh: unknown arg '$1'" >&2; exit 2 ;;
  esac
done

# ---------------------------------------------------------------------------
# verdict(): the whole pass/fail decision, isolated so --self-test can drive it
# with canned cargo output instead of a 20-minute build. Prints a summary and
# returns 0 (GREEN) / 1 (RED).
# ---------------------------------------------------------------------------
verdict() {
  local log="$1"
  local failed_targets ignored passed

  # `cargo test` prints one `test result: FAILED.` line per failing target and a
  # trailing `error: N targets failed:` summary.
  failed_targets="$(grep -c '^test result: FAILED' "$log" 2>/dev/null || true)"
  passed="$(awk '/^test result: ok\./ {s += $4} END {print s+0}' "$log")"
  ignored="$(awk '/^test result: ok\./ {for (i=1;i<=NF;i++) if ($(i+1) ~ /^ignored/) s += $i} END {print s+0}' "$log")"

  if [ "${failed_targets:-0}" -gt 0 ] || grep -q '^error: .* targets\? failed' "$log"; then
    echo "RED: release lane has failing tests"
    echo "--- failing targets ---"
    grep -E '^error: test failed|^error: [0-9]+ targets? failed|^    `-p ' "$log" || true
    echo "--- failing test names ---"
    awk '/^failures:$/ {inblock=1; next} inblock && /^    [a-zA-Z_]/ {print} inblock && !/^    / {inblock=0}' "$log" | sort -u
    return 1
  fi

  if grep -qE '^error(\[|:)' "$log"; then
    echo "RED: release lane failed to build"
    grep -E '^error(\[|:)' "$log" | head -20
    return 1
  fi

  echo "GREEN: release lane clean — ${passed} passed, ${ignored} ignored, 0 failed"
  echo "(ignored = the debug-only failure-injection tests; they run and pass in \`cargo test\`)"
  return 0
}

# ---------------------------------------------------------------------------
run_self_test() {
  echo "=== G-REL SELF-TEST (drive verdict() with canned cargo output) ==="
  local tmp; tmp="$(mktemp -d "${TMPDIR:-/tmp}/grel_selftest.XXXXXX")"

  cat > "$tmp/green.log" <<'EOF'
running 3 tests
test a ... ok
test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
EOF

  cat > "$tmp/red.log" <<'EOF'
running 3 tests
test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out

failures:
    tabstops::tests::tabstops_resize_alloc_failure_preserves_state

test result: FAILED. 10 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
error: test failed, to rerun pass `-p rustty-vt --lib`
error: 1 target failed:
    `-p rustty-vt --lib`
EOF

  local rc_green rc_red
  verdict "$tmp/green.log" >/dev/null; rc_green=$?
  verdict "$tmp/red.log"   >/dev/null; rc_red=$?
  rm -rf "$tmp"

  echo "  clean output  -> exit $rc_green (expect 0)"
  echo "  failing output-> exit $rc_red (expect 1)"
  if [ "$rc_green" -eq 0 ] && [ "$rc_red" -eq 1 ]; then
    echo "SELF-TEST GREEN: the gate flips RED on a failing release run."
    return 0
  fi
  echo "SELF-TEST RED: the gate did not flip." >&2
  return 1
}

[ "$SELF_TEST" -eq 1 ] && { run_self_test; exit $?; }

command -v cargo >/dev/null 2>&1 || { echo "BLOCKED: cargo not on PATH" >&2; exit 3; }

cd "$RUSTTY_ROOT" || exit 3

SCOPE_ARGS=(--workspace)
[ -n "$SCOPE_PKG" ] && SCOPE_ARGS=(-p "$SCOPE_PKG")

# NOTE: macOS `mktemp` requires the X's to be the LAST characters of the
# template — no `.log` suffix.
LOG="$(mktemp "${TMPDIR:-/tmp}/release_lane_XXXXXX")"

# perl alarm, not coreutils `timeout` (absent on stock macOS).
run_watchdogged() {
  perl -e 'alarm shift; exec @ARGV' "$TIMEOUT_SECS" "$@"
}

echo "=== G-REL: cargo build --release ${SCOPE_ARGS[*]} ==="
run_watchdogged cargo build --release "${SCOPE_ARGS[@]}" 2>&1 | tee "$LOG"
if [ "${PIPESTATUS[0]}" -ne 0 ]; then
  echo "RED: release build failed (or exceeded ${TIMEOUT_SECS}s)" >&2
  exit 1
fi

echo "=== G-REL: cargo test --release ${SCOPE_ARGS[*]} --no-fail-fast ==="
run_watchdogged cargo test --release "${SCOPE_ARGS[@]}" --no-fail-fast 2>&1 | tee "$LOG"
rc="${PIPESTATUS[0]}"
if [ "$rc" -ge 128 ]; then
  echo "RED: release test run HUNG (watchdog fired after ${TIMEOUT_SECS}s)" >&2
  exit 1
fi

verdict "$LOG"
exit $?
