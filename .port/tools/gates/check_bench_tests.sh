#!/usr/bin/env bash
# G-BENCH — the `rustty-bench` unit-test lane (WS10 T4).
#
# WHY THIS EXISTS, given `cargo test --workspace` already builds and runs this
# crate: `B-compression.md` §4.5 records "`rustty-bench` is in NO gate", and the
# grep it is based on (`grep -rn rustty-bench .port/tools/gates/ .port/tools/ci.sh`)
# is still empty. The finding is half right. The crate IS executed — by
# `ci.sh`'s `cargo test --workspace` step and by `check_release_lane.sh`'s
# `cargo test --release --workspace` — so a *failing* bench test does already fail
# CI. What neither lane provides is:
#
#   1. ATTRIBUTION. `--workspace --no-fail-fast` prints one aggregate verdict;
#      nothing names `rustty-bench`, which is exactly how the crate came to be
#      believed ungated.
#   2. A FLOOR. Deleting a `#[test]` — or a whole bench module — from
#      `rustty-bench` is invisible to a workspace run: fewer tests still pass.
#      WS10 Layer 2 rewrites what these benchmarks MEASURE, so a silently
#      dropped test in that crate is a live risk for the rest of this series.
#
# This gate supplies both: it runs the crate's lib tests in BOTH profiles
# (debug and --release; WS18 made the release lane a real gate and a new lane
# must not be what re-breaks it) and asserts the passing count is at least
# MIN_TESTS. The floor is a >=, not an ==, so ADDING tests never turns the gate
# red — only removing them does.
#
# Exit codes follow the ci.sh contract: 0 PASS, 3 BLOCKED (no cargo), other FAIL.

set -u

SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
RUSTTY_ROOT="$(cd "$SELF_DIR/../../.." && pwd)"

# Floor, measured at landing (WS10 T4, 2026-07-30):
#   cargo test -p rustty-bench --lib -> 15 passed; 0 failed; 0 ignored (WS10 T10)
# Raise this deliberately when a task adds tests it wants protected.
# Raised 15 -> 24 (phase-4 wbench): terminal-formatter (+4 upstream test{}s)
# and terminal-resize (+1) landed; measured 24 passed in both profiles.
# Raised 24 -> 27 (2026-08-26): the `snapshot-codec` feature gate is gone.
# terminal_snapshot.rs was written against the codec API as DOCUMENTED and had
# drifted six compile errors away from the API that actually landed, which the
# gate could not see precisely BECAUSE the feature was never enabled. The
# module now compiles unconditionally and contributes its 3 test{}s.
MIN_TESTS=27

# ---- self-test -------------------------------------------------------------
# Proves the count arm can actually go RED, on fabricated libtest output rather
# than by mutating the crate. A gate whose FAIL arm is never exercised is the
# vacuity this program has found nine times.
if [ "${1:-}" = "--self-test" ]; then
  fail=0
  probe() { # $1 = fabricated output, $2 = floor, $3 = expected verdict
    local got
    got="$(printf '%s\n' "$1" | sed -n 's/^test result: ok\. \([0-9]*\) passed.*/\1/p' | tail -1)"
    if [ -z "$got" ]; then
      [ "$3" = "nocount" ] || { echo "  self-test: expected $3, got no count"; fail=1; }
      return
    fi
    if [ "$got" -lt "$2" ]; then
      [ "$3" = "red" ] || { echo "  self-test: expected $3, got red ($got < $2)"; fail=1; }
    else
      [ "$3" = "green" ] || { echo "  self-test: expected $3, got green ($got >= $2)"; fail=1; }
    fi
  }
  probe "test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" 14 green
  probe "test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" 14 green
  # [c] the arm that matters: a deleted test module drops the count below the floor.
  probe "test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" 14 red
  # [d] no `test result:` line at all (build failed / harness changed shape) must
  #     NOT be read as a pass.
  probe "error: could not compile \`rustty-bench\`" 14 nocount
  if [ "$fail" -eq 0 ]; then
    echo "SELF-TEST: PASS (4 arms: floor-exact, floor-exceeded, floor-breached->RED, no-count)"
    exit 0
  fi
  echo "SELF-TEST: FAIL" >&2
  exit 1
fi

command -v cargo >/dev/null 2>&1 || { echo "BLOCKED: cargo not on PATH" >&2; exit 3; }
cd "$RUSTTY_ROOT" || exit 3

rc=0
for profile in "" "--release"; do
  label="cargo test -p rustty-bench --lib ${profile:-(debug)}"
  echo "=== G-BENCH: $label ==="
  # $profile is an intentional empty-or-one-flag word.
  # shellcheck disable=SC2086
  out="$(cargo test -p rustty-bench --lib $profile 2>&1)"
  crc=$?
  echo "$out" | tail -3
  if [ "$crc" -ne 0 ]; then
    echo "RED: $label failed (exit $crc)" >&2
    rc=1
    continue
  fi
  n="$(printf '%s\n' "$out" | sed -n 's/^test result: ok\. \([0-9]*\) passed.*/\1/p' | tail -1)"
  if [ -z "$n" ]; then
    echo "RED: $label produced no 'test result: ok.' line — cannot confirm anything ran" >&2
    rc=1
    continue
  fi
  if [ "$n" -lt "$MIN_TESTS" ]; then
    echo "RED: $label ran $n tests, below the floor of $MIN_TESTS — tests were removed" >&2
    rc=1
    continue
  fi
  echo "OK: $n tests (floor $MIN_TESTS)"
done

# ---------------------------------------------------------------------------
# ORACLE-DRIVEN ACTION-SET PARITY (P3-W0 T7 step 2).
#
# The checked-in list in `bench_every_action_round_trips_through_from_name`
# (crates/rustty-bench/src/cli.rs) pins rustty against ITSELF: it catches a
# deleted action, but an action upstream ADDS is invisible to it, which is
# exactly how `hyperlink-map` and `apc-parser` were absent with the suite green
# until WS10 T11. Drive the comparison from the ORACLE instead, so the arm
# cannot go vacuous by rustty shrinking its own list.
#
# `git show` only — NEVER a checkout. Oracle absent => exit 3 BLOCKED.
# ---------------------------------------------------------------------------
ORACLE="/Users/gael/projects/ghostty"
echo "=== G-BENCH: oracle action-set parity ==="
if [ ! -d "$ORACLE/.git" ]; then
  echo "BLOCKED: ghostty oracle checkout not found at $ORACLE (R3 — the external"
  echo "         oracle never ships to CI); the action-set parity arm cannot bind."
  exit 3
fi
T="${TMPDIR:-/tmp}"
git -C "$ORACLE" show origin/main:src/benchmark/cli.zig 2>/dev/null \
  | sed -n '/^pub const Action = enum {/,/^};/p' \
  | sed -n 's/^    @"\([a-z0-9-]*\)",$/\1/p' | sort -u > "$T/gbench_oracle.txt"
if [ ! -s "$T/gbench_oracle.txt" ]; then
  echo "BLOCKED: read 0 actions from oracle origin/main:src/benchmark/cli.zig."
  echo "         An empty reference side would make this arm VACUOUS (R5)."
  exit 3
fi
sed -n 's/^            (b"\([a-z0-9-]*\)", Action::.*/\1/p' \
  "$RUSTTY_ROOT/crates/rustty-bench/src/cli.rs" | sort -u > "$T/gbench_rustty.txt"
if [ ! -s "$T/gbench_rustty.txt" ]; then
  echo "RED: parsed 0 actions out of crates/rustty-bench/src/cli.rs's checked-in" >&2
  echo "     list. A parser that matches nothing must be a loud failure." >&2
  exit 1
fi
missing="$(comm -23 "$T/gbench_oracle.txt" "$T/gbench_rustty.txt")"
extra="$(comm -13 "$T/gbench_oracle.txt" "$T/gbench_rustty.txt")"
if [ -n "$missing" ] || [ -n "$extra" ]; then
  echo "RED: the rustty-bench action set differs from the oracle's." >&2
  [ -n "$missing" ] && { echo "  in the oracle, ABSENT from rustty:" >&2; \
                         printf '%s\n' "$missing" | sed 's/^/    /' >&2; }
  [ -n "$extra" ]   && { echo "  in rustty, absent from the oracle:" >&2; \
                         printf '%s\n' "$extra" | sed 's/^/    /' >&2; }
  echo "  Port the action, or (for a removal) drop it from cli.rs and its list." >&2
  rc=1
else
  echo "OK: $(wc -l < "$T/gbench_oracle.txt" | tr -d ' ') actions, identical to the oracle's"
fi

[ "$rc" -eq 0 ] && echo "G-BENCH: PASS"
exit "$rc"
