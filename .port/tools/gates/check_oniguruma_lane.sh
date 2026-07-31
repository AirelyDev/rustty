#!/usr/bin/env bash
# G-ONIG — run the tests that ONLY exist under `--features oniguruma`.
#
# WHY THIS EXISTS
# ---------------
# `P3-W9` found that `.port/tools/ci.sh` never passed `--features oniguruma`
# (`grep -c oniguruma ci.sh` = 0). The feature gates real, ported code —
# `rustty-vt`'s `string_map.rs` regex search surface and `rustty-config`'s URL
# regex behaviour table, both faithful to the reference's optional `oniguruma`
# build dependency — and NOT ONE CI LANE COMPILED IT. Those tests could have
# rotted to a compile error and every lane would still have been green, because
# a test that is never built cannot fail.
#
# This is the same shape as the other blind spots this project has hit: the
# thing that looked verified was merely never looked at. A feature that gates
# ported code needs a lane, or the code is unverified by construction.
#
# ANTI-VACUITY — AND THE FIRST VERSION OF THIS GATE WAS ITSELF VACUOUS
# -------------------------------------------------------------------
# Running the suite is not enough. If the feature ever stops pulling in the
# gated modules — a renamed `cfg`, a moved test, a dependency drop — the suite
# still exits 0 while testing NOTHING, which is the exact failure this gate is
# meant to catch.
#
# The first draft floored the ABSOLUTE test count (`vt >= 13`). That was
# worthless: `cargo test -p rustty-vt --features oniguruma` runs the WHOLE
# crate — 2269 tests — so all 13 gated tests could disappear and 2256 would
# still sail past a floor of 13. The gate would have been green precisely when
# the thing it exists to watch had vanished.
#
# So it floors the DIFFERENCE instead: tests-with-feature minus
# tests-without-feature. That number IS the count of oniguruma-gated tests, and
# nothing else moves it. Raise the floor when gated tests are added; never lower
# one without saying why in the same commit.
#
# EXIT CODES
#   0  clean
#   1  a suite failed, or a test-count floor was breached (possible vacuity)
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

# Measured 2026-07-30: rustty-vt runs 2258 tests without the feature and 2269
# with it, so exactly 11 are oniguruma-gated (string_map's regex search).
# rustty-config's url:: filter yields 1 (the url_regex behaviour table, which
# only COMPILES under the feature), so its "without" arm is 0.
#
# The floor was first written as 13 — a number taken from ONE `test result: ok.
# 13 passed` line in the multi-suite output, mistaken for the total. The delta
# arm caught it immediately by going RED, which is the first thing this gate did
# and a fair demonstration that its FAIL path is reachable.
VT_DELTA_FLOOR=11
CONFIG_FLOOR=1

fail=0

count_passed() {
  # Sum the `N passed` across every suite in the output.
  awk -F'[ ;]' '/^test result/ {p += $4} END {print p + 0}'
}

echo "G-ONIG: the --features oniguruma lane (no other CI lane compiles it)"

vt_out="$(cargo test -p rustty-vt --features oniguruma 2>&1)"
vt_rc=$?
vt_with="$(printf '%s\n' "$vt_out" | count_passed)"
if [ "$vt_rc" -ne 0 ]; then
  echo "  RED: rustty-vt --features oniguruma FAILED (exit $vt_rc)"
  printf '%s\n' "$vt_out" | grep -E "^(error|test result: FAILED|---- )" | head -20
  fail=1
fi

# The baseline arm. Its ONLY purpose is to make the delta meaningful; a failure
# here is not this gate's business, so only the count is taken.
vt_base="$(cargo test -p rustty-vt 2>&1 | count_passed)"
vt_delta=$(( vt_with - vt_base ))
echo "  rustty-vt: $vt_with with feature, $vt_base without -> $vt_delta gated"

cfg_out="$(cargo test -p rustty-config --features oniguruma url:: 2>&1)"
cfg_rc=$?
cfg_n="$(printf '%s\n' "$cfg_out" | count_passed)"
if [ "$cfg_rc" -ne 0 ]; then
  echo "  RED: rustty-config --features oniguruma url:: FAILED (exit $cfg_rc)"
  printf '%s\n' "$cfg_out" | grep -E "^(error|test result: FAILED|---- )" | head -20
  fail=1
else
  echo "  rustty-config url::: $cfg_n passed"
fi

# The floors. A suite that runs zero tests exits 0 — that is the vacuity.
if [ "$vt_rc" -eq 0 ] && [ "$vt_delta" -lt "$VT_DELTA_FLOOR" ]; then
  echo "  RED (VACUITY): the feature adds only $vt_delta tests, floor is $VT_DELTA_FLOOR."
  echo "  Fewer GATED tests ran than when this gate was written. Either tests were"
  echo "  removed (lower the floor DELIBERATELY, in the same commit) or the feature"
  echo "  stopped pulling in the gated modules — in which case this lane would be"
  echo "  green while verifying nothing, which is the whole reason it measures a"
  echo "  DELTA rather than an absolute count."
  fail=1
fi
if [ "$cfg_rc" -eq 0 ] && [ "$cfg_n" -lt "$CONFIG_FLOOR" ]; then
  echo "  RED (VACUITY): rustty-config ran $cfg_n url:: tests, floor is $CONFIG_FLOOR."
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "G-ONIG: RED"
  exit 1
fi
echo "G-ONIG: GREEN (gated-test floors: vt delta>=$VT_DELTA_FLOOR, config>=$CONFIG_FLOOR)"
