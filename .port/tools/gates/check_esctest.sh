#!/usr/bin/env bash
# G-ESCTEST — esctest against librustty-vt fails exactly where it fails against
# libghostty-vt (P8-W8 T1; DECISIONS P3; upstream 438d584f0, edcaf6670).
#
# WHAT IT COMPARES
# ----------------
# Upstream runs esctest2 (ThomasDickey's conformance suite) against
# libghostty-vt in the report-only `test-esctest` job; most of the suite fails
# there, so the absolute tally says little. The parity value is the
# DIFFERENTIAL: this gate runs the port's runner (`test/esctest`) and the
# reference's own runner (`test/esctest` of the oracle at the pin), parses each
# esctest log into one outcome per test (pass, fail, known-bug, skip), and is
# RED when
#   - a test's outcome differs, in EITHER direction: Rustty failing a test
#     Ghostty passes is a port bug, and Rustty passing a test Ghostty fails is
#     a divergence that must be investigated (an upstream defect the port fixed
#     is recorded as a UD, never silently accepted);
#   - a test with the same outcome reports a different failure (its log block
#     differs once the two install paths are normalized), so "fails the same
#     way" is checked, not only "fails";
#   - the two logs name different tests, or the Rustty log has no tally (the
#     runner died part way).
# A difference is accepted only through a row of `esctest_known_diffs.tsv`
# (test, Rustty outcome, oracle outcome, record id UDnnn or Nnnn, note); a row
# whose difference no longer occurs is RED too (stale).
#
# The oracle is never built in place and never in the Rustty tree (R3): its
# pinned commit is `git archive`d into a temporary directory, and its runner is
# built there with `zig build` (Debug, as upstream's `zig build run`). The
# port's runner is built Debug too (`cargo build`), so both run with their
# safety checks on.
#
# Usage:
#   check_esctest.sh                      run both runners and compare (G-ESCTEST)
#   check_esctest.sh --self-test          the comparison on fixture logs (G-ESCTEST-SELF)
#   check_esctest.sh --compare R O [KD]   compare a Rustty log R with an oracle log O
#                                         (KD: a known-differences file; default
#                                         esctest_known_diffs.tsv)
#   check_esctest.sh --print-oracle       print the oracle path (G-ORACLE-SH-SELF)
# RUSTTY_ESCTEST_OUT=<dir> keeps the two logs (esctest-rustty.log,
# esctest-oracle.log) and their outcome lists there.
#
# Exit: 0 GREEN, 1 RED, 3 BLOCKED (a missing tool, the oracle absent or
# without the pinned commit, no network for esctest2 or the oracle's Zig
# packages, an oracle log with no tally). BLOCKED is never GREEN.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
KNOWN_DIFFS="$HERE/esctest_known_diffs.tsv"
# The tools the gate needs; any missing one is BLOCKED.
TOOLS="cargo python3 zig curl tar git"

# shellcheck source=.port/tools/gates/_oracle_root.sh
source "$HERE/_oracle_root.sh" || { echo "G-ESCTEST BLOCKED: cannot source _oracle_root.sh"; exit 3; }
ORACLE="$(rustty_oracle_root "$ROOT")"
if [ "${1:-}" = "--print-oracle" ]; then
  echo "$ORACLE"
  exit 0
fi

# ---- the comparison --------------------------------------------------------

# esctest's closing tally (`RunTests` in esctest.py). A log without it is a run
# that died part way.
TALLY='^\*\*\* [0-9]+ tests? passed, '

# parse_log LOG: one line per test, `name<TAB>outcome<TAB>block`, where the
# outcome comes from the line `RunTest` logs after `Run test: <name>`, and the
# block is the test's log lines joined with \036, with the install directory
# and the log file name normalized (they differ between the two runners) and
# tabs escaped.
parse_log() {
  awk '
    function flush() {
      if (name != "") {
        sub(/(\036)+$/, "", blk)
        print name "\t" (st == "" ? "unknown" : st) "\t" blk
      }
      name = ""
    }
    function norm(s) {
      gsub(/\t/, "\\t", s)
      gsub(/[^ "\047]*\/share\/esctest\//, "<esctest>/", s)
      gsub(/\/tmp\/esctest-lib(ghostty|rustty)-[0-9]+\.log/, "<log>", s)
      gsub(/[^ "\047]*\/esctest-librustty-[A-Za-z0-9]+\/esctest\.log/, "<log>", s)
      return s
    }
    /^Run test: / { flush(); name = substr($0, 11); st = ""; blk = ""; next }
    /^\*\*\* [0-9]+ tests? passed, / { flush(); next }
    name == "" { next }
    {
      if (st == "") {
        if ($0 == "Passed.") st = "pass"
        else if (index($0, "Fails as expected: ") == 1) st = "known-bug"
        else if (index($0, "Skipped because terminal lacks requisite capability: ") == 1) st = "skip"
        else if ($0 == "*** TEST " name " FAILED:") st = "fail"
      }
      blk = blk norm($0) "\036"
    }
    END { flush() }
  ' "$1"
}

# compare_logs RUSTTY_LOG ORACLE_LOG KNOWN_DIFFS: prints the findings and
# returns 0 GREEN, 1 RED, 3 BLOCKED. A RED finding wins over a BLOCKED one.
compare_logs() {
  local r="$1" o="$2" kd="$3" work rc=0
  if [ ! -r "$r" ] || ! grep -qE "$TALLY" "$r"; then
    echo "G-ESCTEST RED: the Rustty log $r has no esctest tally (the run died or never started)"
    return 1
  fi
  if [ ! -r "$o" ] || ! grep -qE "$TALLY" "$o"; then
    echo "G-ESCTEST BLOCKED: the oracle log $o has no esctest tally, so there is nothing to compare with"
    return 3
  fi
  if [ ! -r "$kd" ]; then
    echo "G-ESCTEST RED: the known-differences file $kd is missing"
    return 1
  fi
  work="$(mktemp -d "${TMPDIR:-/tmp}/gesctest_cmp.XXXXXX")" || { echo "G-ESCTEST BLOCKED: mktemp"; return 3; }
  parse_log "$r" >"$work/r.tsv"
  parse_log "$o" >"$work/o.tsv"
  echo "Rustty: $(grep -E "$TALLY" "$r" | tail -1)"
  echo "oracle: $(grep -E "$TALLY" "$o" | tail -1)"
  awk -F'\t' '
    function firstdiff(a, b,   x, y, nx, ny, i) {
      nx = split(a, x, "\036"); ny = split(b, y, "\036")
      for (i = 1; i <= nx || i <= ny; i++)
        if (x[i] != y[i]) return "\n      Rustty: " x[i] "\n      oracle: " y[i]
      return ""
    }
    FILENAME == ARGV[1] {
      if (FNR == 1) next
      if (NF != 5 || $4 !~ /^(UD|N)[0-9]+$/ || $2 !~ /^(pass|fail|known-bug|skip)$/ ||
          $3 !~ /^(pass|fail|known-bug|skip)$/ || $5 == "") {
        print "RED   bad known-differences row " FNR ": " $0 " (want test, Rustty outcome, oracle outcome, UDnnn or Nnnn, note)"
        red++; next
      }
      KR[$1] = $2; KO[$1] = $3; KID[$1] = $4; next
    }
    FILENAME == ARGV[2] { R[$1] = $2; RB[$1] = $3; names[$1] = 1; next }
    FILENAME == ARGV[3] { O[$1] = $2; OB[$1] = $3; names[$1] = 1; next }
    END {
      for (n in names) {
        if (!(n in R)) { print "RED   " n ": in the oracle log only"; red++; continue }
        if (!(n in O)) { print "RED   " n ": in the Rustty log only"; red++; continue }
        compared++
        if (R[n] != O[n]) what = "Rustty " R[n] ", oracle " O[n]
        else if (RB[n] != OB[n]) what = "both " R[n] ", but the log differs" firstdiff(RB[n], OB[n])
        else continue
        if ((n in KR) && KR[n] == R[n] && KO[n] == O[n]) {
          print "known " n ": " what " (" KID[n] ")"; used[n] = 1; known++
        } else {
          print "RED   " n ": " what; red++
        }
      }
      for (n in KR) if (!(n in used)) {
        print "RED   stale known difference " n " (" KID[n] "): it no longer occurs"; red++
      }
      if (compared == 0) { print "RED   no test is in both logs"; red++ }
      printf "%d tests compared, %d known differences, %d findings\n", compared, known, red
      exit (red > 0)
    }
  ' "$kd" "$work/r.tsv" "$work/o.tsv" || rc=1
  rm -rf "$work"
  if [ "$rc" -ne 0 ]; then
    echo "G-ESCTEST RED: esctest does not fail the same way against librustty-vt and libghostty-vt."
    return 1
  fi
  echo "G-ESCTEST GREEN: every esctest test has the same outcome and log against both."
  return 0
}

# ---- self-test ---------------------------------------------------------------

# A fixture esctest log. Each argument is `name=outcome[:detail]`; the detail
# goes into the test's log block, so two fixtures can fail the same test in
# different ways. `@nosummary` leaves the tally out; `@dir=PATH` sets the
# install path printed in tracebacks.
fixture_log() {
  local spec name outcome detail passed=0 known=0 failed=0 summary=1 dir="/x/share/esctest"
  local failures=()
  echo "Reading X window info for 0x0"
  for spec in "$@"; do
    case "$spec" in
      @nosummary) summary=0; continue ;;
      @dir=*) dir="${spec#@dir=}"; continue ;;
    esac
    name="${spec%%=*}"
    outcome="${spec#*=}"
    detail="${outcome#*:}"
    [ "$detail" = "$outcome" ] && detail=""
    outcome="${outcome%%:*}"
    echo "Run test: $name"
    case "$outcome" in
      pass) echo "Passed."; passed=$((passed + 1)) ;;
      known-bug) echo "Fails as expected: a known bug"; known=$((known + 1)) ;;
      skip) echo "Skipped because terminal lacks requisite capability: level"; known=$((known + 1)) ;;
      fail)
        echo "*** TEST $name FAILED:"
        echo "Traceback (most recent call last):"
        echo "  File \"$dir/esctest.py\", line 109, in RunTest"
        echo "esctypes.TestFailure: Test failed: ${detail:-expected 1 but got 2}"
        failed=$((failed + 1))
        failures+=("$name")
        ;;
    esac
    echo
  done
  [ "$summary" = 1 ] || return 0
  if [ "$failed" -gt 0 ]; then
    echo "*** $passed tests passed, $known known bugs, $failed TESTS FAILED ***"
    echo "Failing tests:"
    printf "%s\n" ${failures[@]+"${failures[@]}"}
  else
    echo "*** $passed tests passed, $known known bugs, 0 tests failed ***"
  fi
}

self_test() {
  local st failed=0 rc got
  st="$(mktemp -d "${TMPDIR:-/tmp}/gesctest_self.XXXXXX")" || { echo "G-ESCTEST-SELF BLOCKED: mktemp"; return 3; }
  # EXIT, not RETURN: the self-test ends the script, and an abort (set -u)
  # never runs a RETURN trap.
  # shellcheck disable=SC2064  # expand now: the path is fixed for this run.
  trap "rm -rf '$st'" EXIT
  printf 'test\trustty\toracle\trecord\tnote\n' >"$st/empty.tsv"

  # case <want rc> <label> <known-diffs file> -- <rustty specs...> -- <oracle specs...>
  case_() {
    local want="$1" label="$2" kd="$3" r=() o=() side=r
    shift 3
    [ "$1" = "--" ] && shift
    for a in "$@"; do
      if [ "$a" = "--" ]; then side=o; continue; fi
      if [ "$side" = r ]; then r+=("$a"); else o+=("$a"); fi
    done
    fixture_log ${r[@]+"${r[@]}"} >"$st/r.log"
    fixture_log ${o[@]+"${o[@]}"} >"$st/o.log"
    rc=0
    got="$(compare_logs "$st/r.log" "$st/o.log" "$kd" 2>&1)" || rc=$?
    if [ "$rc" = "$want" ]; then
      echo "  ok   $label (exit $rc)"
    else
      echo "  FAIL $label: exit $rc, want $want"
      printf '%s\n' "$got" | sed 's/^/       | /' | head -12
      failed=1
    fi
  }

  local base=(A.test_a=pass B.test_b=fail C.test_c=known-bug D.test_d=skip)
  case_ 0 "matching pair is GREEN" "$st/empty.tsv" -- "${base[@]}" -- "${base[@]}"
  case_ 0 "matching pair under different install paths is GREEN" "$st/empty.tsv" -- \
    @dir=/r/target/share/esctest "${base[@]}" -- @dir=/o/zig-out/share/esctest "${base[@]}"
  case_ 1 "Rustty fails a test the oracle passes is RED" "$st/empty.tsv" -- \
    A.test_a=fail B.test_b=fail C.test_c=known-bug D.test_d=skip -- "${base[@]}"
  case_ 1 "Rustty passes a test the oracle fails is RED" "$st/empty.tsv" -- \
    A.test_a=pass B.test_b=pass C.test_c=known-bug D.test_d=skip -- "${base[@]}"
  case_ 1 "known-bug against pass is RED" "$st/empty.tsv" -- \
    A.test_a=pass B.test_b=fail C.test_c=pass D.test_d=skip -- "${base[@]}"
  case_ 1 "same failure, different detail is RED" "$st/empty.tsv" -- \
    A.test_a=pass "B.test_b=fail:expected 13 but got 2" C.test_c=known-bug D.test_d=skip -- \
    A.test_a=pass "B.test_b=fail:expected 13 but got 4" C.test_c=known-bug D.test_d=skip
  case_ 1 "a test missing from the Rustty log is RED" "$st/empty.tsv" -- \
    A.test_a=pass B.test_b=fail C.test_c=known-bug -- "${base[@]}"
  case_ 1 "a test missing from the oracle log is RED" "$st/empty.tsv" -- \
    "${base[@]}" -- A.test_a=pass B.test_b=fail C.test_c=known-bug
  case_ 1 "a Rustty log without a tally is RED" "$st/empty.tsv" -- \
    @nosummary "${base[@]}" -- "${base[@]}"
  case_ 3 "an oracle log without a tally is BLOCKED" "$st/empty.tsv" -- \
    "${base[@]}" -- @nosummary "${base[@]}"
  case_ 1 "two empty logs are RED (no tests compared)" "$st/empty.tsv" -- --

  # The known-differences ledger.
  { cat "$st/empty.tsv"; printf 'A.test_a\tpass\tfail\tUD999\tfixture\n'; } >"$st/kd-ok.tsv"
  case_ 0 "a difference with its ledger row is GREEN" "$st/kd-ok.tsv" -- \
    "${base[@]}" -- A.test_a=fail B.test_b=fail C.test_c=known-bug D.test_d=skip
  case_ 1 "a ledger row whose difference no longer occurs is RED (stale)" "$st/kd-ok.tsv" -- \
    "${base[@]}" -- "${base[@]}"
  case_ 1 "a ledger row with other outcomes does not accept the difference" "$st/kd-ok.tsv" -- \
    A.test_a=known-bug B.test_b=fail C.test_c=known-bug D.test_d=skip -- \
    A.test_a=fail B.test_b=fail C.test_c=known-bug D.test_d=skip
  { cat "$st/empty.tsv"; printf 'A.test_a\tpass\tfail\tsee-notes\tfixture\n'; } >"$st/kd-bad.tsv"
  case_ 1 "a ledger row without a UD/N record id is RED" "$st/kd-bad.tsv" -- \
    "${base[@]}" -- A.test_a=fail B.test_b=fail C.test_c=known-bug D.test_d=skip
  { cat "$st/empty.tsv"; printf 'B.test_b\tfail\tfail\tN999\tfixture\n'; } >"$st/kd-detail.tsv"
  case_ 0 "a detail difference with its ledger row is GREEN" "$st/kd-detail.tsv" -- \
    A.test_a=pass "B.test_b=fail:got 2" C.test_c=known-bug D.test_d=skip -- \
    A.test_a=pass "B.test_b=fail:got 4" C.test_c=known-bug D.test_d=skip

  # The oracle arms: a copy of this gate in a fixture checkout is BLOCKED, never
  # GREEN, when the oracle beside it is absent and when it lacks the pinned
  # commit. (The reason is checked only where every tool is installed, since a
  # missing tool is BLOCKED first.)
  local fx="$st/fx" f want_reason tools_ok=1 t
  for t in $TOOLS; do command -v "$t" >/dev/null 2>&1 || tools_ok=0; done
  oracle_arm() { # oracle_arm <label> <reason substring>
    rc=0
    got="$(cd "$st" && bash "$fx/main/.port/tools/gates/check_esctest.sh" 2>&1)" || rc=$?
    want_reason="$2"
    [ "$tools_ok" = 1 ] || want_reason="BLOCKED"
    if [ "$rc" = 3 ] && [[ "$got" == *"$want_reason"* ]]; then
      echo "  ok   $1 (exit 3)"
    else
      echo "  FAIL $1: exit $rc, want 3 with '$want_reason'"
      printf '%s\n' "$got" | sed 's/^/       | /' | tail -5
      failed=1
    fi
  }
  mkdir -p "$fx/main/.port/tools/gates"
  for f in check_esctest.sh _oracle_root.sh esctest_known_diffs.tsv; do
    cp "$HERE/$f" "$fx/main/.port/tools/gates/$f"
  done
  if ! rustty_fixture_repo "$fx/main" README "fixture checkout" >/dev/null; then
    echo "G-ESCTEST-SELF BLOCKED: could not build the fixture checkout"
    return 3
  fi
  printf '%040d\n' 0 | tr 0 a >"$fx/main/.port/tools/gates/ORACLE_PIN"
  oracle_arm "an absent oracle is BLOCKED" "no oracle checkout at"
  if ! rustty_fixture_repo "$fx/ghostty" README "fixture oracle" >/dev/null; then
    echo "G-ESCTEST-SELF BLOCKED: could not build the fixture oracle"
    return 3
  fi
  oracle_arm "an oracle without the pinned commit is BLOCKED" "does not have the pinned commit"

  if [ "$failed" -ne 0 ]; then
    echo "G-ESCTEST-SELF RED: an arm above did not hold."
    return 1
  fi
  echo "G-ESCTEST-SELF GREEN: every comparison arm holds on fixture logs."
  return 0
}

case "${1:-}" in
  --self-test)
    self_test
    exit $?
    ;;
  --compare)
    [ "$#" -ge 3 ] || { echo "usage: $0 --compare RUSTTY_LOG ORACLE_LOG [KNOWN_DIFFS]" >&2; exit 2; }
    compare_logs "$2" "$3" "${4:-$KNOWN_DIFFS}"
    exit $?
    ;;
  "") ;;
  *)
    echo "usage: $0 [--self-test | --compare R O [KD] | --print-oracle]" >&2
    exit 2
    ;;
esac

# ---- the gate ----------------------------------------------------------------

blocked() { echo "G-ESCTEST BLOCKED: $*"; exit 3; }
red() { echo "G-ESCTEST RED: $*"; exit 1; }

for tool in $TOOLS; do
  command -v "$tool" >/dev/null 2>&1 || blocked "$tool is not installed"
done
PIN="$(rustty_oracle_pin)" || blocked "the oracle pin file $HERE/ORACLE_PIN is missing or not a 40-hex commit"
[ -d "$ORACLE" ] || blocked "no oracle checkout at $ORACLE"
rustty_oracle_git -C "$ORACLE" cat-file -e "$PIN^{commit}" 2>/dev/null ||
  blocked "the oracle at $ORACLE does not have the pinned commit $PIN"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/gesctest.XXXXXX")" || blocked "mktemp"
trap 'rm -rf "$WORK"' EXIT
OUT="${RUSTTY_ESCTEST_OUT:-$WORK/out}"
mkdir -p "$OUT" || blocked "cannot create $OUT"

# The port's runner: fetch esctest2 (pinned, hash-checked), build, run.
echo "== Rustty: test/esctest"
rc=0
bash "$ROOT/test/esctest/fetch.sh" || rc=$?
case "$rc" in
  0) ;;
  3) blocked "could not download esctest2 (no network?)" ;;
  *) red "test/esctest/fetch.sh failed (exit $rc); a hash mismatch is never accepted" ;;
esac
cargo build --locked --manifest-path "$ROOT/test/esctest/Cargo.toml" ||
  red "test/esctest does not build"
"$ROOT/test/esctest/target/debug/esctest-librustty" >"$OUT/esctest-rustty.log" 2>"$OUT/esctest-rustty.stderr"
echo "esctest-librustty exited $?"

# The reference's runner, from the pinned commit in a temporary copy.
echo "== oracle: test/esctest at $PIN"
mkdir "$WORK/oracle" || blocked "cannot create $WORK/oracle"
rustty_oracle_git -C "$ORACLE" archive --format=tar "$PIN" | tar -xf - -C "$WORK/oracle" ||
  blocked "cannot extract the pinned oracle tree"
(cd "$WORK/oracle/test/esctest" && zig build) ||
  blocked "the oracle's test/esctest does not build here (Zig packages not fetchable?)"
"$WORK/oracle/test/esctest/zig-out/bin/esctest-libghostty" >"$OUT/esctest-oracle.log" 2>"$OUT/esctest-oracle.stderr"
echo "esctest-libghostty exited $?"

parse_log "$OUT/esctest-rustty.log" | cut -f1,2 >"$OUT/esctest-rustty.outcomes.tsv"
parse_log "$OUT/esctest-oracle.log" | cut -f1,2 >"$OUT/esctest-oracle.outcomes.tsv"
compare_logs "$OUT/esctest-rustty.log" "$OUT/esctest-oracle.log" "$KNOWN_DIFFS"
exit $?
