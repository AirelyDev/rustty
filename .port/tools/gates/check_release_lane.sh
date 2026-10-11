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
# The build step proves the release build itself. It used to be a precondition
# too: `rustty-apprt-c`'s C-ABI smoke looked for the published cdylib/staticlib
# in `target/<profile>/`, which `cargo test` alone never uplifts from `deps/`.
# Since N479 the smoke links the `deps/` copies its own `cargo test` built, so
# it no longer depends on this step.
#
# The run is wrapped in a watchdog (default 45 min, `RELEASE_LANE_TIMEOUT`
# seconds). A HANG is reported RED, never waited on forever — `rustty-termio`'s
# `subprocess_drop_repro` has been observed wedging under full-workspace
# parallel load (macOS `login(1)` children stuck in kernel "exiting" state, so
# `killPid`'s faithful SIGHUP/waitpid loop spins). See WS18's report.
#
# EXCLUSIVE TESTS (G-REL-LAT, CI hygiene B). A test whose verdict is a
# wall-clock budget measures the machine as much as the code, and the main
# pass runs beside every other gate of a parallel ci.sh. Such a test is a row
# of EXCLUSIVE_TESTS below: the main pass `--skip`s it, and `--exclusive` runs
# it ALONE (ci.sh registers that as G-REL-LAT, an `exclusive` HARD unit, which
# the scheduler starts only once every other gate has finished, like G8b/G8).
# Row rules, checked on every run (a bad row is RED, exit 1): four fields,
# "<package>|<exact libtest path>|<host OS (uname -s), or *>|<reason>", and
# the reason must say why the test cannot share the machine. A row whose
# test matches nothing on its OS is stale: --exclusive requires exactly one
# test to run and pass per row, so a rename turns G-REL-LAT RED.
# --exclusive follows G8b's load-check precedent: after building, and before
# running anything, the 1-minute load must fall to ws10_suite.py's IDLE_MAX
# within REL_LAT_COOLDOWN_S (default 600 s); otherwise it exits 3 BLOCKED
# (a WARNING with its reason, never a pass) having run nothing. A test that
# then misses its budget is RED.
#
# Usage:
#   check_release_lane.sh                 # full gate (build + test, release)
#   check_release_lane.sh --crate <name>  # scope to one package (cheap smoke)
#   check_release_lane.sh --exclusive     # only the EXCLUSIVE_TESTS, alone
#   check_release_lane.sh --self-test     # prove the verdict flips RED
#
# Exit 0 = GREEN, 1 = RED (failures / hang / a bad row), 3 = BLOCKED (no
# cargo; --exclusive: a loaded machine, or no row for this OS).
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
TIMEOUT_SECS="${RELEASE_LANE_TIMEOUT:-2700}"

SCOPE_PKG=""
SELF_TEST=0
EXCLUSIVE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --crate) SCOPE_PKG="${2:-}"; shift 2 ;;
    --exclusive) EXCLUSIVE=1; shift ;;
    --self-test) SELF_TEST=1; shift ;;
    *) echo "check_release_lane.sh: unknown arg '$1'" >&2; exit 2 ;;
  esac
done
if [ "$EXCLUSIVE" -eq 1 ] && [ -n "$SCOPE_PKG" ]; then
  echo "check_release_lane.sh: --exclusive runs every EXCLUSIVE_TESTS row; it does not take --crate" >&2; exit 2
fi

# The tests the main pass skips and --exclusive runs alone (see the header).
EXCLUSIVE_TESTS=(
  # N549 (P8 final validation): the BATCHING bound is a 300 us MEDIAN latency
  # over 8 interactive writes through a real pty, release only. It went RED
  # (median 10.23 ms) in the release lane of a full parallel ci.sh, and passed
  # 10/10 on a quiet machine and in the same run's debug lane. Its own doc
  # comment measured the healthy band (22-46 us) on an otherwise idle box.
  "rustty-termio|exec::tests::read_thread::readthread_pipeline_keeps_interactive_latency|Darwin|a 300 us median latency budget (release only) that a fully loaded machine breaks (N549: 10 ms under parallel CI, 10/10 quiet); it measures the scheduler as much as the pipeline"
)
OS="$(uname -s)"

# rows_check: every row has four fields and a reason. Prints each bad row.
rows_check() {
  local row pkg path os why rest bad=0
  for row in ${EXCLUSIVE_TESTS[@]+"${EXCLUSIVE_TESTS[@]}"}; do
    IFS='|' read -r pkg path os why rest <<< "$row"
    if [ -z "$pkg" ] || [ -z "$path" ] || [ -z "$os" ] || [ -n "$rest" ]; then
      echo "RED: EXCLUSIVE_TESTS row '$row' is not <package>|<test path>|<OS or *>|<reason>"; bad=1
    elif [ "${#why}" -lt 20 ]; then
      echo "RED: EXCLUSIVE_TESTS row for $path has no reason (or a token one: '$why'); say why it cannot share the machine"; bad=1
    fi
  done
  return "$bad"
}

# ---------------------------------------------------------------------------
# verdict(): the whole pass/fail decision, isolated so --self-test can drive it
# with canned cargo output instead of a 20-minute build. Prints a summary and
# returns 0 (GREEN) / 1 (RED).
# ---------------------------------------------------------------------------
verdict() {
  local log="$1"
  local build_log="${2:-}"
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

  # Build errors. The BUILD log is scanned with the broad `^error:`/`^error[`
  # pattern: it is pure cargo/rustc output, so anything shaped like a diagnostic
  # there IS one. (Before 2026-09-08 this log was overwritten by the test run
  # before verdict() ever saw it, so it was never scanned at all — this arm is
  # net-new coverage, not a relaxation.)
  if [ -n "$build_log" ] && [ -f "$build_log" ] && grep -qE '^error(\[|:)' "$build_log"; then
    echo "RED: release lane failed to build"
    grep -E '^error(\[|:)' "$build_log" | head -20
    return 1
  fi

  # The TEST log interleaves cargo diagnostics with the tests' own stderr, so a
  # bare `^error:` is NOT proof of a build failure: a PASSING test may print one.
  # `rustty_init_invalid_action_returns_1` does exactly that on Linux —
  # "error: failed to initialize rustty error=InvalidAction" is its expected
  # output — and it made this HARD gate report "failed to build" after a clean
  # build and a fully green run (found by the phase-6 Linux lane; macOS never
  # runs that GTK-only test). Match only the shapes cargo/rustc alone emit.
  if grep -qE '^error\[|^error: could not compile' "$log"; then
    echo "RED: release lane failed to build"
    grep -E '^error\[|^error: could not compile' "$log" | head -20
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

  # A PASSING test whose own stderr starts with `error:`. This is real output
  # from rustty_init_invalid_action_returns_1 on Linux; before 2026-09-08 the
  # bare `^error:` scan called it a build failure. Must be GREEN.
  cat > "$tmp/green_test_stderr.log" <<'EOF'
running 12 tests
error: failed to initialize rustty error=InvalidAction
test tests::rustty_init_invalid_action_returns_1 ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
EOF

  # A genuine compile failure in the TEST build. Must stay RED.
  cat > "$tmp/red_compile.log" <<'EOF'
   Compiling rustty-core v0.1.0
error[E0432]: unresolved import `nope`
error: could not compile `rustty-core` (lib test) due to 1 previous error
EOF

  # A genuine compile failure in the BUILD log (the net-new arm). Must be RED
  # even though the test log alone looks clean.
  cat > "$tmp/red_build.log" <<'EOF'
   Compiling rustty-vt v0.1.0
error: could not compile `rustty-vt` (lib) due to 1 previous error
EOF

  local rc_green rc_red rc_stderr rc_compile rc_build
  verdict "$tmp/green.log" >/dev/null; rc_green=$?
  verdict "$tmp/red.log"   >/dev/null; rc_red=$?
  verdict "$tmp/green_test_stderr.log" >/dev/null; rc_stderr=$?
  verdict "$tmp/red_compile.log" >/dev/null; rc_compile=$?
  verdict "$tmp/green.log" "$tmp/red_build.log" >/dev/null; rc_build=$?
  rm -rf "$tmp"

  echo "  clean output          -> exit $rc_green (expect 0)"
  echo "  failing output        -> exit $rc_red (expect 1)"
  echo "  passing test w/ stderr-> exit $rc_stderr (expect 0)"
  echo "  test-build compile err-> exit $rc_compile (expect 1)"
  echo "  build-log compile err -> exit $rc_build (expect 1)"
  if [ "$rc_green" -eq 0 ] && [ "$rc_red" -eq 1 ] && [ "$rc_stderr" -eq 0 ] \
     && [ "$rc_compile" -eq 1 ] && [ "$rc_build" -eq 1 ]; then
    echo "SELF-TEST GREEN: the gate flips RED on a failing release run, on a"
    echo "  compile error in either log, and stays GREEN on a passing test that"
    echo "  prints \`error:\` to stderr."
    return 0
  fi
  echo "SELF-TEST RED: the gate did not flip." >&2
  return 1
}

# ---------------------------------------------------------------------------
# The EXCLUSIVE_TESTS arms: copies of this script in a fixture root (one
# fixture row, a stubbed load1, a stand-in ws10_suite.py) against a fake cargo
# on PATH that records its arguments and answers as <root>/mode asks. Each
# behaviour has a negative control: a copy mutated to drop exactly it.
# ---------------------------------------------------------------------------
run_self_test_exclusive() {
  local tmp fail=0 out rc
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/grel_excl.XXXXXX")"
  echo "=== G-REL SELF-TEST: EXCLUSIVE_TESTS (the main pass skips them, --exclusive runs them alone) ==="
  mkdir -p "$tmp/bin"
  cat > "$tmp/bin/cargo" <<'FAKE'
#!/usr/bin/env bash
# A stand-in cargo: records argv, answers per $FAKE_ROOT/mode.
echo "$*" >> "$FAKE_ROOT/cargo.args"
case " $* " in *" --no-run "*) exit 0 ;; esac
case " $* " in *" build "*) echo "   Compiling fixture"; exit 0 ;; esac
case "$(cat "$FAKE_ROOT/mode")" in
  pass) echo "test m::lat ... ok"; echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out" ;;
  none) echo "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out" ;;
  fail) echo "test m::lat ... FAILED"; echo "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out"; exit 101 ;;
esac
exit 0
FAKE
  chmod +x "$tmp/bin/cargo"
  mk() {  # mk NAME ROW [OLD NEW] -> a fixture root with a mutated copy
    local d="$tmp/$1"
    mkdir -p "$d/.port/tools/gates" "$d/.port/tools/perf"
    printf 'IDLE_MAX = 2.0\n' > "$d/.port/tools/perf/ws10_suite.py"
    echo pass > "$d/mode"; echo 0.5 > "$d/load"; : > "$d/cargo.args"
    ROW="$2" OLD="${3:-}" NEW="${4:-}" python3 - "$GATES_DIR/check_release_lane.sh" "$d/.port/tools/gates/check_release_lane.sh" <<'PY2' || return 1
import os, re, sys
s = open(sys.argv[1]).read()
a = s.index("\nrun_self_test_exclusive() {")
b = s.index("\n}\n", a) + 3
s = s[:a] + "\nrun_self_test_exclusive() { :; }\n" + s[b:]       # no recursion into the fixture
a = s.index("\nEXCLUSIVE_TESTS=(\n") + 1
b = s.index("\n)\n", a) + 3
s = s[:a] + 'EXCLUSIVE_TESTS=(\n  "%s"\n)\n' % os.environ["ROW"] + s[b:]
old = "load1() { python3 -c 'import os; print(\"%.2f\" % os.getloadavg()[0])'; }"
assert s.count(old) == 1
s = s.replace(old, 'load1() { cat "$RUSTTY_ROOT/load"; }')
if os.environ["OLD"]:
    assert s.count(os.environ["OLD"]) == 1, "mutation target occurs %d times" % s.count(os.environ["OLD"])
    s = s.replace(os.environ["OLD"], os.environ["NEW"])
open(sys.argv[2], "w").write(s)
PY2
  }
  go() {  # go NAME ARGS... -> out, rc
    local d="$tmp/$1"; shift
    : > "$d/cargo.args"
    out="$(FAKE_ROOT="$d" PATH="$tmp/bin:$PATH" REL_LAT_COOLDOWN_S=0 bash "$d/.port/tools/gates/check_release_lane.sh" "$@" 2>&1)"; rc=$?
  }
  ok() { echo "  ok    $1"; }
  bad() { echo "  FAIL  $1 (exit $rc)"; printf '%s\n' "$out" | tail -4 | sed 's/^/        /'; fail=1; }
  local ROW='fakepkg|m::lat|*|a fixture latency budget that a loaded machine breaks'
  # X1: the main pass passes --skip <path> for the row.
  mk main "$ROW" || { echo "  FAIL  fixture not built"; return 1; }
  go main
  if [ "$rc" -eq 0 ] && grep -q -- 'test --release --workspace --no-fail-fast -- --skip m::lat$' "$tmp/main/cargo.args"; then ok "the main pass runs cargo test --release ... -- --skip m::lat"
  else bad "the main pass did not skip the row"; fi
  # shellcheck disable=SC2016  # this script's code, literally
  mk main-mut "$ROW" '  SKIP_ARGS+=(--skip "$path")' '  :' || { echo "  FAIL  control fixture not built"; return 1; }
  go main-mut
  if ! grep -q -- '--skip m::lat' "$tmp/main-mut/cargo.args"; then ok "control: a main pass that drops the skip runs the row with every other test, noticed"
  else bad "control: a dropped skip went unnoticed"; fi
  # X2: --exclusive on a quiet machine: built, then run alone with --exact.
  mk ex "$ROW"; go ex --exclusive
  if [ "$rc" -eq 0 ] && grep -q -- '--no-run' "$tmp/ex/cargo.args" && grep -q -- '-- --exact m::lat' "$tmp/ex/cargo.args" && printf '%s' "$out" | grep -q 'GREEN: 1 exclusive'; then
    ok "--exclusive, quiet machine: built, the row run alone with --exact, GREEN"
  else bad "--exclusive quiet"; fi
  # X3: a loaded machine is BLOCKED (exit 3) and runs nothing.
  echo 9.0 > "$tmp/ex/load"; go ex --exclusive
  if [ "$rc" -eq 3 ] && printf '%s' "$out" | grep -q 'BLOCKED: G-REL-LAT load check' && ! grep -q -- '--exact' "$tmp/ex/cargo.args"; then
    ok "--exclusive, load 9.0 > IDLE_MAX 2.0: BLOCKED (exit 3) with its reason, the test not run"
  else bad "--exclusive loaded"; fi
  mk ex-mut "$ROW" '  load_check
  for row in' '  for row in' || { echo "  FAIL  control fixture not built"; return 1; }
  echo 9.0 > "$tmp/ex-mut/load"; go ex-mut --exclusive
  if [ "$rc" -eq 0 ]; then ok "control: without the load check a loaded machine passes silently (exit 0), noticed"
  else bad "control: a dropped load check went unnoticed"; fi
  # X4/X5: a stale row (0 tests) and a failing test are RED.
  echo none > "$tmp/ex/mode"; echo 0.5 > "$tmp/ex/load"; go ex --exclusive
  if [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -q 'want exactly 1 (a stale'; then ok "--exclusive: a row matching no test is RED (stale)"
  else bad "--exclusive stale row"; fi
  echo fail > "$tmp/ex/mode"; go ex --exclusive
  if [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -q 'FAILED alone'; then ok "--exclusive: the test failing alone is RED"
  else bad "--exclusive failing test"; fi
  # X6: a row with no reason is RED in both modes; X7: no row for this OS is BLOCKED.
  mk noreason 'fakepkg|m::lat|*|'; go noreason
  local r1="$rc"; go noreason --exclusive
  if [ "$r1" -eq 1 ] && [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -q 'has no reason'; then ok "a row with no reason is RED (main pass and --exclusive)"
  else bad "a reasonless row (main exit $r1)"; fi
  mk otheros 'fakepkg|m::lat|Plan9|a fixture latency budget for another host OS'; go otheros --exclusive
  if [ "$rc" -eq 3 ] && printf '%s' "$out" | grep -q "BLOCKED: no EXCLUSIVE_TESTS row applies"; then ok "--exclusive with no row for this OS is BLOCKED (exit 3), never GREEN"
  else bad "--exclusive other OS"; fi
  # X8: the real rows pass the row rules.
  if out="$(rows_check 2>&1)"; then ok "the real EXCLUSIVE_TESTS rows (${#EXCLUSIVE_TESTS[@]}) are well-formed, each with a reason"
  else rc=1; bad "the real EXCLUSIVE_TESTS rows"; fi
  echo "  (fixtures in $tmp)"
  return "$fail"
}

# ---------------------------------------------------------------------------
# --exclusive: the EXCLUSIVE_TESTS rows for this OS, each run alone.
# ---------------------------------------------------------------------------
# IDLE_MAX is ws10_suite.py's own threshold, read from it (one source, as G8b).
REL_LAT_COOLDOWN_S="${REL_LAT_COOLDOWN_S:-600}"
REL_LAT_COOLDOWN_STEP_S="${REL_LAT_COOLDOWN_STEP_S:-15}"
SUITE="$RUSTTY_ROOT/.port/tools/perf/ws10_suite.py"
load1() { python3 -c 'import os; print("%.2f" % os.getloadavg()[0])'; }
load_check() {  # returns when idle; exits 3 (BLOCKED, nothing run) after the cooldown
  local waited=0 l idle
  idle="$(sed -n 's/^IDLE_MAX = \([0-9.]*\).*/\1/p' "$SUITE")"
  [ -n "$idle" ] || { echo "RED: cannot read IDLE_MAX from $SUITE"; exit 1; }
  while :; do
    l="$(load1)"
    if python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) <= float(sys.argv[2]) else 1)' "$l" "$idle"; then
      echo "load check: 1-minute load $l <= IDLE_MAX $idle (after ${waited}s)"
      return 0
    fi
    if [ "$waited" -ge "$REL_LAT_COOLDOWN_S" ]; then
      echo "BLOCKED: G-REL-LAT load check: 1-minute load $l > IDLE_MAX $idle after a ${waited}s cooldown; no exclusive test was run"
      exit 3
    fi
    sleep "$REL_LAT_COOLDOWN_STEP_S"; waited=$((waited + REL_LAT_COOLDOWN_STEP_S))
  done
}
run_exclusive() {
  local row pkg path os why pkgs="" p log rc passed n=0 red=0
  echo "=== G-REL-LAT: the EXCLUSIVE_TESTS rows, alone, --release ($OS) ==="
  for row in "${EXCLUSIVE_TESTS[@]}"; do
    IFS='|' read -r pkg path os why <<< "$row"
    if [ "$os" != "*" ] && [ "$os" != "$OS" ]; then echo "  not on $OS: $pkg $path ($os only)"; continue; fi
    n=$((n + 1))
    case " $pkgs " in *" $pkg "*) ;; *) pkgs="$pkgs $pkg" ;; esac
  done
  if [ "$n" -eq 0 ]; then
    echo "BLOCKED: no EXCLUSIVE_TESTS row applies to $OS, so G-REL-LAT ran nothing here"
    exit 3
  fi
  # Build first, so the load check sees the machine the tests will run on.
  for p in $pkgs; do
    echo "--- cargo test --release -p $p --no-run"
    if ! run_watchdogged cargo test --release -p "$p" --no-run; then
      echo "RED: the release test build of $p failed (or exceeded ${TIMEOUT_SECS}s)"; exit 1
    fi
  done
  load_check
  for row in "${EXCLUSIVE_TESTS[@]}"; do
    IFS='|' read -r pkg path os why <<< "$row"
    if [ "$os" != "*" ] && [ "$os" != "$OS" ]; then continue; fi
    log="$(mktemp "${TMPDIR:-/tmp}/release_lat_XXXXXX")"
    echo "--- cargo test --release -p $pkg -- --exact $path --nocapture   (alone: $why)"
    run_watchdogged cargo test --release -p "$pkg" -- --exact "$path" --nocapture 2>&1 | tee "$log"
    rc="${PIPESTATUS[0]}"
    passed="$(awk '/^test result: ok\./ {s += $4} END {print s+0}' "$log")"
    if [ "$rc" -ge 128 ]; then echo "RED: $path HUNG (watchdog fired after ${TIMEOUT_SECS}s)"; red=1
    elif [ "$rc" -ne 0 ] || grep -q '^test result: FAILED' "$log"; then echo "RED: $path FAILED alone, on a machine that passed the load check"; red=1
    elif [ "$passed" -ne 1 ]; then echo "RED: $path: $passed test(s) ran, want exactly 1 (a stale EXCLUSIVE_TESTS row: renamed or removed?)"; red=1
    else echo "ok: $path passed alone"; fi
    rm -f -- "$log"
  done
  [ "$red" -eq 0 ] || return 1
  echo "GREEN: $n exclusive test(s) passed alone"
  return 0
}

if [ "$SELF_TEST" -eq 1 ]; then
  run_self_test; st1=$?
  run_self_test_exclusive; st2=$?
  [ "$st1" -eq 0 ] && [ "$st2" -eq 0 ] && { echo "SELF-TEST GREEN (verdict and EXCLUSIVE_TESTS arms)"; exit 0; }
  echo "SELF-TEST RED" >&2; exit 1
fi

rows_check || exit 1

command -v cargo >/dev/null 2>&1 || { echo "BLOCKED: cargo not on PATH" >&2; exit 3; }

cd "$RUSTTY_ROOT" || exit 3

SCOPE_ARGS=(--workspace)
[ -n "$SCOPE_PKG" ] && SCOPE_ARGS=(-p "$SCOPE_PKG")

# N422: `cargo test --release` builds its test executables with the `release`
# profile, which strips (`-Dstrip`'s ReleaseFast default). The reference's test
# executable is never stripped (`build.zig` test_exe: `.strip = false`), so this
# lane turns the strip off, keeping panic backtraces symbolized. Exported for the
# build step too, so both steps share one set of artifacts.
export CARGO_PROFILE_RELEASE_STRIP=false

# NOTE: macOS `mktemp` requires the X's to be the LAST characters of the
# template — no `.log` suffix.
LOG="$(mktemp "${TMPDIR:-/tmp}/release_lane_XXXXXX")"
LOG_BUILD="$(mktemp "${TMPDIR:-/tmp}/release_lane_build_XXXXXX")"

# perl alarm, not coreutils `timeout` (absent on stock macOS).
run_watchdogged() {
  perl -e 'alarm shift; exec @ARGV' "$TIMEOUT_SECS" "$@"
}

if [ "$EXCLUSIVE" -eq 1 ]; then run_exclusive; exit $?; fi

# The main pass skips every EXCLUSIVE_TESTS row (on every OS: a row for
# another OS names a test that does not exist here, and skipping it is a no-op).
SKIP_ARGS=()
for row in "${EXCLUSIVE_TESTS[@]}"; do
  IFS='|' read -r _pkg path _os _why <<< "$row"
  SKIP_ARGS+=(--skip "$path")
done

echo "=== G-REL: cargo build --release ${SCOPE_ARGS[*]} ==="
run_watchdogged cargo build --release "${SCOPE_ARGS[@]}" 2>&1 | tee "$LOG_BUILD"
if [ "${PIPESTATUS[0]}" -ne 0 ]; then
  echo "RED: release build failed (or exceeded ${TIMEOUT_SECS}s)" >&2
  exit 1
fi

echo "=== G-REL: cargo test --release ${SCOPE_ARGS[*]} --no-fail-fast -- ${SKIP_ARGS[*]} ==="
echo "    (${#EXCLUSIVE_TESTS[@]} EXCLUSIVE_TESTS row(s) skipped here; G-REL-LAT runs them alone)"
run_watchdogged cargo test --release "${SCOPE_ARGS[@]}" --no-fail-fast -- "${SKIP_ARGS[@]}" 2>&1 | tee "$LOG"
rc="${PIPESTATUS[0]}"
if [ "$rc" -ge 128 ]; then
  echo "RED: release test run HUNG (watchdog fired after ${TIMEOUT_SECS}s)" >&2
  exit 1
fi

verdict "$LOG" "$LOG_BUILD"
exit $?
