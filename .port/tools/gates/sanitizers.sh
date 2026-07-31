#!/usr/bin/env bash
# sanitizers.sh — sanitizer matrix gate. Complements Miri (R6: right tool per job).
#
# Miri catches Stacked-Borrows/provenance/uninit/leak on pure Rust but is ~100-400x
# native (a few tests are intractable). The sanitizers run at ~2x native and catch
# real memory CORRUPTION (UAF/OOB/double-free) + races across ALL tests incl. the
# Miri-intractable ones, and cover the C-FFI boundary Miri can't model.
#
# Platform support (probed on this repo's dev box, aarch64-apple-darwin):
#   address (ASan)  ✅ macOS + Linux
#   thread  (TSan)  ✅ macOS + Linux  (needs -Zbuild-std)
#   leak    (LSan)  ❌ macOS (rustc-rejected) ; ✅ Linux  -> Linux-CI lane
#   memory  (MSan)  ❌ macOS ; Linux x86_64 only (needs -Zbuild-std + instrumented C) -> CI lane
#   hwaddress(HWASan)❌ macOS ; aarch64-Linux/Android only -> CI lane
#   cfi             mitigation, not a bug-checker; needs -Clto -> optional Goal-2 hardening
#
# Usage: sanitizers.sh <crate> [filter] [sanitizer ...]
set -u
CRATE="${1:?crate}"; FILTER="${2:-}"; shift 2 2>/dev/null || shift $# 2>/dev/null
TGT=$(rustc -vV | sed -n 's/host: //p')
OS=$(uname -s)
if [ "$#" -gt 0 ]; then SANS="$*"
elif [ "$OS" = "Darwin" ]; then SANS="address thread"
else SANS="address thread leak"; fi

run_san() {
  local s="$1" flags="" pre="" td="/tmp/san-gate-$CRATE-$s" log
  log="/tmp/san-gate-$CRATE-$s.log"
  case "$s" in
    address)   pre="ASAN_OPTIONS=detect_leaks=0" ;;      # macOS ASan has no leak detection
    thread|memory) flags="-Zbuild-std" ;;
    leak|hwaddress) [ "$OS" = "Darwin" ] && { echo "  ⏭  $s: skipped (Linux-only)"; return 0; } ;;
  esac
  echo "### sanitizer=$s crate=$CRATE ###"
  env $pre RUSTFLAGS="-Zsanitizer=$s" CARGO_TARGET_DIR="$td" \
    cargo +nightly test -p "$CRATE" --target "$TGT" $flags -- $FILTER >"$log" 2>&1
  local rc=$?
  if grep -qE 'ERROR: (Address|Thread|Leak|Memory|HWAddress)Sanitizer|SUMMARY:.*Sanitizer|runtime error:' "$log"; then
    echo "  ❌ $s: SANITIZER FINDING"; grep -E 'ERROR:|SUMMARY:|runtime error:' "$log" | head -4; return 1
  elif [ $rc -ne 0 ]; then
    echo "  ⚠️  $s: build/test failed rc=$rc — $(grep -m1 -iE 'error(\[|:)' "$log" | head -c 100)"; return 2
  fi
  # NON-VACUITY + HONEST REPORTING (miri-reaudit, 2026-07-31). This line used to
  # echo `grep 'test result' | tail -1`, which is the LAST summary in the log —
  # i.e. the Doc-tests target, always "0 passed; 0 filtered out". So a run that
  # really executed 31 tests reported "✅ address: test result: ok. 0 passed",
  # and a filter that matched NOTHING reported exactly the same thing. The
  # displayed evidence could not distinguish "ran and passed" from "ran nothing".
  # Now: sum the `running N tests` headers across every target, RED on a total of
  # 0 (a filter that matches nothing is a vacuous green, not a pass), and print
  # every summary rather than the least informative one.
  local ran
  ran="$(awk '/^running [0-9]+ tests?$/ {s+=$2} END {print s+0}' "$log")"
  if [ "$ran" -eq 0 ]; then
    echo "  ❌ $s: VACUOUS — 0 tests ran (filter '${FILTER:-<none>}' matched nothing); a green over an empty set is not a pass"
    return 1
  fi
  echo "  ✅ $s: $ran test(s) ran — $(grep 'test result' "$log" | tr '\n' ';' | sed 's/;$//')"
  return 0
}
fail=0
for s in $SANS; do run_san "$s" || fail=1; done
[ $fail -eq 0 ] && echo "SANITIZERS: GREEN ($CRATE: $SANS)" || echo "SANITIZERS: PROBLEM ($CRATE)"
exit $fail
