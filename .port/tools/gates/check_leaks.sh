#!/usr/bin/env bash
# check_leaks.sh — GATE G6: memory safety (UB + leaks).
#
# RIGHT TOOL PER JOB:
#   * pure-Rust crates  -> Miri  (`cargo +nightly miri test -p <crate>`):
#       catches UB (use-after-free, OOB, uninit, invalid aliasing) AND leaks.
#       This is the PRIMARY gate and supersedes any tracking-allocator approach.
#   * Rust<->C FFI crates -> AddressSanitizer (`-Zsanitizer=address`):
#       Miri cannot execute foreign C, so FFI crates use ASan for UB across the
#       boundary. (FFI *leak* detection: LSan/valgrind are unavailable on arm64
#       macOS, so FFI leak checks are deferred to Linux CI — see DoP §G6.)
#   * live-syscall test binaries in otherwise-pure crates -> ASan:
#       a test whose PURPOSE is a real syscall loop (kqueue/mach-port) cannot
#       execute under Miri at all ("can't call foreign function `kqueue`");
#       it is ROUTED to ASan, never skipped (R5/R6). Per-binary list +
#       4-criterion justification: .port/phase-2/MIRI_EXCEPTIONS.md
#       ("Live-syscall test binaries").
#
# Done-detection (H5 fix): shared with G1/G2/G3b via _gatelib (PORT_STATUS_RE +
# DONE_STATUSES = done|complete|parity|ported|final) — a `PORT STATUS: ported`
# crate is gated exactly like a `done` one. The enumerator visits BOTH
# crates/*/Cargo.toml and crates-sys/*/Cargo.toml (sys statuses are currently
# 'vendored'/'stub'/... which are not completion claims, so none run today —
# but they are enumerated, listed by --list, and gate automatically the moment
# one claims a done-class status).
#
# Usage:
#   check_leaks.sh                    same as --all-done (the G6 gate run)
#   check_leaks.sh --all-done         run Miri/ASan for every claimed-done crate
#   check_leaks.sh --crate <name>     run the right tool for that crate
#   check_leaks.sh --list             enumerate crates + status + lane (no run)
#   check_leaks.sh --self-test        prove Miri catches a leak and a UB
#   check_leaks.sh --debug-assertions-off
#                                     GATE G6c: the `-C debug-assertions=off`
#                                     vt Miri lane (see the G6c block below)
#   check_leaks.sh --debug-assertions-off --self-test
#                                     prove G6c's FAIL arms are reachable
# Exit 0 = GREEN (clean) ; 3 = BLOCKED (miri toolchain missing — a tooling
# input, surfaced by ci.sh as a WARNING per the CI.md exit contract, never a
# silent green and never misclassified as a claimed-done regression) ; any
# other non-zero = RED (UB/leak/unrouted binary) or bad usage (2).
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
cd "$RUSTTY_ROOT" || exit 1

# Crates whose tests cross into C (depend on a crates-sys/* crate) -> ASan, not Miri.
# rustty-wuffs wraps wuffs-sys C FFI (H5: it was absent from this list while its
# 'ported' status also escaped the old literal-'done' grep — doubly unreachable).
FFI_CRATES="rustty-simd rustty-font rustty-renderer rustty-config rustty-crash rustty-termio rustty-inspector rustty-font-shaper rustty-oniguruma rustty-input rustty-wuffs"
# Every crates-sys/* crate is FFI by construction (it exists to vendor C/C++),
# so the *-sys pattern routes the whole tree to ASan without a hand list.
is_ffi() {
  case "$1" in *-sys) return 0;; esac
  case " $FFI_CRATES " in *" $1 "*) return 0;; *) return 1;; esac
}

# Live-FFI integration-test binaries Miri cannot execute (real syscalls).
# These are NOT skipped (R5): the crate's Miri lane covers its unit surface
# (--lib) and each listed binary runs under ASan instead (R6 right-tool).
# Any addition REQUIRES a matching 4-criterion entry in
# .port/phase-2/MIRI_EXCEPTIONS.md ("Live-syscall test binaries").
# run_crate() enforces completeness: an unlisted tests/*.rs binary in a crate
# named here is a G6 RED (it would otherwise escape both lanes).
live_ffi_tests() {
  case "$1" in
    rustty-os) echo "cf_release_live";;  # real kqueue/mach-port xev loop
    *) echo "";;
  esac
}

# G6-ASAN COMPANION for live-syscall UNIT tests (miri-reaudit, 2026-07-31).
#
# live_ffi_tests() above routes whole integration-test BINARIES. rustty-xev's
# live-syscall tests are not binaries — they are in-lib unit tests, annotated at
# source `#[cfg_attr(miri, ignore = "MIRI EXCEPTION (rustty-xev C3…C7) … runs
# natively + ASan (sanitizers.sh)")]` because Miri hard-errors on them ("can't
# call foreign function `kqueue` on OS `macos`" — re-verified 2026-07-31, 2s).
#
# THE EXCEPTION IS LEGITIMATE; ITS COMPENSATING CONTROL WAS NOT RUNNING. The
# native half is real (ci.sh HARD step `cargo test --workspace`). The ASan half
# named `sanitizers.sh` — a MANUAL single-crate harness that no gate and no CI
# workflow ever invoked (grep: ci.sh registers only check_sanitizer_canary.sh,
# which proves ASan can SEE a canary heap, not that these tests ever ran under
# it). So ~30 exceptions rested on a control nobody ran. This arm runs it.
#
# WHY IT IS FILTERED, and what that hides: a crate-wide ASan run of rustty-xev
# CRASHES — `AddressSanitizer: BUS … ___chkstk_darwin` inside `pthread_create`
# from `ThreadPool::notify_slow` (thread_pool.rs:297), because the thread-pool
# tests construct a config with `stack_size: 0` (faithful to the reference;
# clamped by std to PTHREAD_STACK_MIN), and a 16 KB stack does not fit ASan's
# instrumented frames. That is an ASan/tiny-stack artifact in a test that Miri
# already covers, NOT one of the excepted tests — but it is why this arm names
# the excepted families instead of the whole crate. Widening it is an open item
# (MIRI_EXCEPTIONS.md, "the 2026-07-31 re-audit").
live_ffi_unit_filters() {
  case "$1" in
    rustty-xev) echo "kqueue:: timer:: stream:: process:: async_watcher::";;
    *) echo "";;
  esac
}
# FLOOR, DERIVED FROM THE SOURCE — never hand-listed. It is the number of
# `MIRI EXCEPTION (rustty-xev …)` annotations in the crate, so adding an
# exception whose tests this arm does not reach makes the arm RED (too few ran)
# instead of silently shrinking the covered set.
# ASan coverage for modules that are NOT Miri exceptions and therefore must NOT
# be folded into `live_ffi_unit_filters`.
#
# WHY SEPARATE: the floor above is "ASan ran >= the number of MIRI EXCEPTION
# annotations". Adding non-excepted tests to that filter INFLATES the count, so
# an excepted test could silently stop running and the floor would still pass —
# which is exactly the vacuity this arm exists to prevent. Two invocations keep
# the floor comparing like with like.
#
# `thread_pool::` is here because it was previously excluded from ASan entirely
# after the arm crashed on it (`BUS ... ___chkstk_darwin` in `pthread_create`),
# recorded as ASan frames not fitting "16 KB pool-thread stacks" — blamed on the
# tool. The cause was OURS: two tests wrote `stack_size: 0`, dropping the
# reference's 16 MiB field default (`ThreadPool.zig:84`), and `init`'s
# `max(1, ..)` made that a ONE-BYTE request that macOS clamps to
# PTHREAD_STACK_MIN (16 KiB). With the default restored the tests run clean in
# ~2s. The exclusion had left the crate's most UB-dense module — three real
# Stacked-Borrows findings live in `thread_pool.rs`, two in the implementation —
# with no ASan coverage at all.
extra_asan_filters() {  # $1 = crate
  case "$1" in
    rustty-xev) echo "thread_pool::";;
    *) echo "";;
  esac
}
# Floor for the arm above: hand-set, because these are not annotation-derived.
# 4 is the current `thread_pool::` test count; it exists so the arm goes RED if
# the filter stops matching rather than silently running zero tests.
extra_asan_floor() {  # $1 = crate
  case "$1" in
    rustty-xev) echo 4;;
    *) echo 0;;
  esac
}
live_ffi_unit_floor() {  # $1 = crate
  grep -rho "MIRI EXCEPTION ($1" "crates/$1/src" 2>/dev/null | wc -l | tr -d ' '
}

# Unified done-detection (H5): defer to _gatelib's PORT_STATUS_RE + DONE_STATUSES
# so G6 can never disagree with G1/G2/G3b about what "claimed done" means.
crate_status_of() {  # $1 = crate directory (crates/... or crates-sys/...)
  python3 -c '
import sys, os
sys.path.insert(0, sys.argv[2])
import _gatelib as G
print(G.dir_status(sys.argv[1]))
' "$1" "$GATES_DIR"
}
claimed_done() {  # $1 = crate dir. 0 = done, 1 = not done, 3 = PROBE FAILED
  # The probe must be able to say "I could not tell", distinctly from "no".
  # It used to be `sys.exit(0 if ... else 1)`, but a Python failure — e.g.
  # `ModuleNotFoundError: No module named '_gatelib'`, which really happened
  # during a G6 lane run — ALSO exits 1, so a broken probe was indistinguishable
  # from an honest "this crate is not claimed-done". The caller's
  # `claimed_done "$dir" || continue` then skipped 50 crates in a row with
  # nothing but stderr noise to show for it.
  local out rc
  out="$(python3 -c '
import sys
sys.path.insert(0, sys.argv[2])
import _gatelib as G
print("DONE" if G.dir_status(sys.argv[1]) in G.DONE_STATUSES else "NOTDONE")
' "$1" "$GATES_DIR" 2>&1)"; rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "G6 PROBE FAILED for $1 (python exit $rc): $out" >&2
    return 3
  fi
  case "$out" in
    DONE) return 0 ;;
    NOTDONE) return 1 ;;
    *) echo "G6 PROBE FAILED for $1 (unexpected output): $out" >&2; return 3 ;;
  esac
}

require_miri() {
  # Missing miri is a MISSING TOOLING INPUT, not a claimed-done regression:
  # exit 3 = BLOCKED per the documented gate exit contract (.port/tools/CI.md
  # "Gate exit-code contract" + DoP §G6 BLOCKED-not-waived). ci.sh surfaces it
  # as a WARNING — loud, never a silent green (H7 fix: the script used to exit
  # 2 here, which ci.sh misread as a RED regression). CI installs
  # nightly+miri unconditionally (.github/workflows/ci.yml), so this path is
  # for genuinely miri-less local hosts only.
  if ! cargo +nightly miri --version >/dev/null 2>&1; then
    echo "G6 BLOCKED: cargo miri not available (rustup component add miri --toolchain nightly)"; exit 3
  fi
}

run_crate() {
  local c="$1"
  if is_ffi "$c"; then
    echo "=== G6 [$c] :: ASan (FFI crate; Miri can't run C) ==="
    RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" \
      cargo +nightly test -p "$c" --target "$(rustc -vV | sed -n 's/host: //p')" 2>&1
    local rc=$?
    echo "(FFI leak detection for $c is deferred to Linux CI: LSan/valgrind unavailable on arm64 macOS)"
    return $rc
  fi
  local live
  live="$(live_ffi_tests "$c")"
  if [ -n "$live" ]; then
    # GUARD (tightening): with the Miri lane narrowed to --lib, EVERY
    # integration-test binary (tests/*.rs) of this crate must be explicitly
    # routed in live_ffi_tests(), otherwise a future unlisted binary would run
    # under neither Miri nor ASan in G6 — that is a RED, not a silent escape.
    local tf tb unrouted=""
    for tf in "crates/$c"/tests/*.rs "crates-sys/$c"/tests/*.rs; do
      [ -f "$tf" ] || continue
      tb="$(basename "$tf" .rs)"
      case " $live " in *" $tb "*) ;; *) unrouted="$unrouted $tb";; esac
    done
    if [ -n "$unrouted" ]; then
      echo "G6 RED [$c]: unrouted integration-test binaries:$unrouted"
      echo "  (this crate's Miri lane is --lib only; add each binary to live_ffi_tests()"
      echo "   WITH a 4-criterion MIRI_EXCEPTIONS.md entry, or it escapes both lanes)"
      return 1
    fi
    # ROUTED, not skipped (R5): Miri proves the crate's unit surface; each
    # live-syscall test binary (which Miri hard-errors on: "can't call foreign
    # function \`kqueue\`") runs under ASan. MIRI_EXCEPTIONS.md has the
    # 4-criterion justification per binary.
    echo "=== G6 [$c] :: Miri (unit surface --lib; live-FFI test binaries routed to ASan per MIRI_EXCEPTIONS.md) ==="
    require_miri
    local rc=0
    cargo +nightly miri test -p "$c" --lib 2>&1 || rc=1
    local t
    for t in $live; do
      echo "=== G6 [$c --test $t] :: ASan (live syscalls; Miri cannot execute them) ==="
      RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" \
        cargo +nightly test -p "$c" --test "$t" --target "$(rustc -vV | sed -n 's/host: //p')" 2>&1 || rc=1
    done
    return $rc
  fi
  echo "=== G6 [$c] :: Miri (UB + leaks) ==="
  require_miri
  local mrc=0
  cargo +nightly miri test -p "$c" 2>&1 || mrc=1

  # ASan companion for in-lib live-syscall unit tests (see live_ffi_unit_filters).
  local ufilters floor sout uran
  ufilters="$(live_ffi_unit_filters "$c")"
  if [ -n "$ufilters" ]; then
    floor="$(live_ffi_unit_floor "$c")"
    echo "=== G6 [$c] :: ASan companion (the $floor live-syscall tests Miri cannot execute) ==="
    sout="$(bash "$GATES_DIR/sanitizers.sh" "$c" "$ufilters" address 2>&1)"
    printf '%s\n' "$sout" | sed 's/^/    /'
    uran="$(printf '%s' "$sout" | sed -n 's/.*✅ address: \([0-9]*\) test(s) ran.*/\1/p')"
    if printf '%s' "$sout" | grep -q 'SANITIZERS: PROBLEM'; then
      echo "G6 RED [$c]: the ASan companion for the Miri-excepted live-syscall tests is not clean."
      mrc=1
    elif [ "${uran:-0}" -lt "${floor:-1}" ]; then
      echo "G6 RED [$c]: ASan companion ran ${uran:-0} test(s), fewer than the $floor source"
      echo "  \`MIRI EXCEPTION ($c …)\` annotations — an exception is escaping BOTH lanes."
      mrc=1
    fi
  fi

  # Second, INDEPENDENT ASan invocation: non-excepted modules (see
  # extra_asan_filters). Kept separate so the annotation-derived floor above
  # cannot be satisfied by tests that are not exceptions.
  local xfilters xfloor xout xran
  xfilters="$(extra_asan_filters "$c")"
  if [ -n "$xfilters" ]; then
    xfloor="$(extra_asan_floor "$c")"
    echo "=== G6 [$c] :: ASan (non-excepted modules: $xfilters) ==="
    xout="$(bash "$GATES_DIR/sanitizers.sh" "$c" "$xfilters" address 2>&1)"
    printf '%s\n' "$xout" | sed 's/^/    /'
    xran="$(printf '%s' "$xout" | sed -n 's/.*✅ address: \([0-9]*\) test(s) ran.*/\1/p')"
    if printf '%s' "$xout" | grep -q 'SANITIZERS: PROBLEM'; then
      echo "G6 RED [$c]: ASan is not clean on the non-excepted modules."
      mrc=1
    elif [ "${xran:-0}" -lt "${xfloor:-1}" ]; then
      echo "G6 RED [$c]: ASan ran ${xran:-0} test(s) on $xfilters, fewer than the"
      echo "  expected $xfloor — the filter has stopped matching."
      mrc=1
    fi
  fi
  return $mrc
}

# ---------------------------------------------------------------------------
# GATE G6c — the `-C debug-assertions=off` vt Miri lane (WS19 T6, closure-audit
# row O2; the follow-up WS7 Phase E recommended and WS8 never took).
#
# WHY A SECOND LANE, NOT A FLAG CHANGE ON G6 (R5). `Page::assert_integrity` is
# O(page) and runs on nearly every PageList mutation under `debug_assertions`.
# Under Miri's byte-level interpreter that dominates the page-compression tests
# so completely that they cannot finish: measured per test, in single-worker
# isolation, `pagelist_compress_page_round_trips_a_cold_history_page` is
# TIMEOUT(360s) with assertions ON and completes with them OFF. But G6 is a
# SHARED, cross-crate gate, and the integrity assertions are real coverage — so
# G6 keeps running exactly as it did (debug assertions ON, and it is what
# upholds the integrity-gate coverage), and this lane is ADDITIVE: it binds
# more, never less.
#
# WHAT THE TRADE IS. Turning integrity checking off buys interpreter reach and
# costs nothing Miri is here for: `assert_integrity` adds no Stacked-Borrows /
# Tree-Borrows and no leak coverage (it is a logical-consistency check over
# offsets and counts). `.port/phase-2/mirihar-pagelist-compress` already relies
# on exactly this accommodation (`[profile.dev] debug-assertions = false`).
#
# HOW THE PARTITION IS EXPRESSED. Tests this lane rescues are annotated
# `#[cfg_attr(all(miri, debug_assertions), ignore = "...")]` — ignored in G6,
# RUN here. Tests that are intractable in BOTH lanes keep the unconditional
# `#[cfg_attr(miri, ignore = "...")]`. So the annotation form alone decides what
# runs, and the two lanes can never silently drift apart. Per-test timings and
# the 4-criterion basis for what stays excepted: .port/phase-2/MIRI_EXCEPTIONS.md.
#
# Overridable for the self-test only; the gate run uses the defaults.
G6C_PKG="${G6C_PKG:-rustty-vt}"
G6C_DIR="${G6C_DIR:-$RUSTTY_ROOT}"
# The lane's test set is DERIVED FROM THE SOURCE ANNOTATIONS, never hand-listed.
# A hand-maintained list is the classic way a lane silently stops covering what
# it claims to: a test gets renamed or a new one gets annotated, the list rots,
# and the gate stays green over a shrinking surface. Here `g6c_names` greps the
# `cfg_attr(all(miri, debug_assertions), ignore = ...)` blocks out of
# crates/rustty-vt/src and emits the guarded test-fn names, so the annotation IS
# the registration and drift is impossible by construction.
g6c_names() {
  python3 - "$G6C_DIR" <<'PY'
import os, re, sys
root = os.path.join(sys.argv[1], 'crates/rustty-vt/src')
out = []
for dp, _, fns in os.walk(root):
    for fn in sorted(fns):
        if not fn.endswith('.rs'):
            continue
        lines = open(os.path.join(dp, fn), errors='ignore').read().split('\n')
        for i, l in enumerate(lines):
            if l.strip() != 'all(miri, debug_assertions),':
                continue
            k = i
            while k < len(lines) and not re.match(r'\s*fn ', lines[k]):
                k += 1
            if k < len(lines):
                out.append(re.search(r'fn (\w+)', lines[k]).group(1))
print(' '.join(sorted(set(out))))
PY
}
G6C_FILTERS="${G6C_FILTERS:-}"
[ -n "$G6C_FILTERS" ] || G6C_FILTERS="$(g6c_names)"
# VACUITY FLOOR — the number of tests this lane must actually RUN. Defaults to
# the size of the derived set: every one of them MUST pass, so a test that
# quietly stops running is a RED, not a smaller green. It is also the proof that
# `-C debug-assertions=off` reached rustc, because every test it counts is
# `cfg_attr(all(miri, debug_assertions), ignore)` and therefore CANNOT run
# unless the flag took (see g6c_verdict).
G6C_MIN_PASSED="${G6C_MIN_PASSED:-$(printf '%s\n' $G6C_FILTERS | grep -c .)}"
# Hard per-lane kill budget. macOS has no coreutils `timeout`; killtree, as in
# .port/tools/miri-run.sh. The measured serial lane cost is tens of minutes
# (the set is ~80 tests, individually 6s-260s under Miri), so the bound is
# generous enough that a loaded host does not flap the gate, yet finite so the
# lane always terminates.
#
# 5400 -> 10800 (parity tail C-1): the 83-test set took 3952 s in the ci run at
# 9fd17d93 (73% of the old bound), and C-1 moved 36 more tests here out of G6
# (debug-assertion integrity walks put them past G6's budget; measured
# 1,822 s for 34 of them, one test per process, on a contended host). Libtest's
# substring filtering of ~3,300 tests by every name also grows with the set
# under Miri. This is the hang guard only: the lane still has to run EVERY
# derived test, pass them all, and stay UB- and leak-clean.
G6C_BOUND_SEC="${G6C_BOUND_SEC:-10800}"

g6c_killtree() { local p=$1 c; for c in $(pgrep -P "$p" 2>/dev/null); do g6c_killtree "$c"; done; kill -9 "$p" 2>/dev/null; }

# g6c_lane <extra-rustflags> <outfile> — run the lane once, hard-bounded.
g6c_lane() {
  local extra="$1" out="$2" cpid kpid rc
  ( cd "$G6C_DIR" && RUSTFLAGS="-C debug-assertions=off $extra" MIRIFLAGS="-Zmiri-disable-isolation" \
      cargo +nightly miri test -p "$G6C_PKG" --lib -- $G6C_FILTERS --test-threads=1 >"$out" 2>&1 ) &
  cpid=$!
  ( sleep "$G6C_BOUND_SEC"; g6c_killtree "$cpid" 2>/dev/null ) & kpid=$!
  wait "$cpid" 2>/dev/null; rc=$?
  # killTREE the watchdog, not just the subshell: a bare `kill $kpid` reaps the
  # subshell and ORPHANS its `sleep`, which keeps the inherited stdout fd open —
  # any consumer downstream of this gate (`| tail`, `$( )`, a CI log collector)
  # then blocks until the full bound elapses even though the lane finished. That
  # is a hang, and it bit this gate on its first run.
  g6c_killtree "$kpid" 2>/dev/null; wait "$kpid" 2>/dev/null
  return $rc
}

# g6c_verdict <outfile> — 0 only for a clean AND non-vacuous run.
g6c_verdict() {
  local out="$1" line passed failed
  if grep -qE 'Undefined Behavior|memory leaked|error: aborting' "$out"; then
    echo "G6c RED: Miri reported UB or a leak in the page-compression family:"
    grep -nE 'Undefined Behavior|memory leaked|error: aborting' "$out" | head -5 | sed 's/^/    /'
    return 1
  fi
  line="$(grep -E '^test result:' "$out" | tail -1)"
  if [ -z "$line" ]; then
    echo "G6c RED: no libtest summary — the lane did not finish (hard bound ${G6C_BOUND_SEC}s) or failed to build:"
    tail -5 "$out" | sed 's/^/    /'
    return 1
  fi
  echo "    $line"
  passed="$(printf '%s' "$line" | sed -n 's/.*result: [a-zA-Z]*\. \([0-9]*\) passed.*/\1/p')"
  failed="$(printf '%s' "$line" | sed -n 's/.*passed; \([0-9]*\) failed.*/\1/p')"
  if [ "${failed:-1}" != 0 ]; then
    echo "G6c RED: ${failed:-?} test(s) failed under the debug-assertions-off Miri lane"
    grep -E '^(failures:|    pagelist)' "$out" | head -10 | sed 's/^/    /'
    return 1
  fi
  if [ "${passed:-0}" -lt "$G6C_MIN_PASSED" ]; then
    echo "G6c RED: only ${passed:-0} test(s) RAN, expected at least $G6C_MIN_PASSED."
    echo "  Either (a) debug assertions were NOT actually disabled — the family is"
    echo "  \`cfg_attr(all(miri, debug_assertions), ignore)\`, so with the flag lost every"
    echo "  one of them reports 'ignored' and this lane degenerates into a slow copy of"
    echo "  G6 — or (b) a test was re-annotated/renamed without updating G6C_MIN_PASSED."
    return 1
  fi
  return 0
}

run_g6c() {
  require_miri
  echo "=== G6c [$G6C_PKG] :: Miri, -C debug-assertions=off ==="
  echo "    $G6C_MIN_PASSED test(s), derived from the cfg_attr(all(miri, debug_assertions)) annotations"
  echo "    (measured GREEN in 3092s serial on an M4 Pro, 2026-07-30, and 3952s for 83 tests at 9fd17d93; set G6C_FILTERS to narrow)"
  local out; out="$(mktemp "${TMPDIR:-/tmp}/g6c.XXXXXX")"
  local t0=$SECONDS
  g6c_lane "" "$out"
  local dur=$((SECONDS - t0))
  if g6c_verdict "$out"; then
    echo "G6c GREEN (${dur}s; debug assertions off, UB/leak-clean, non-vacuous)"
    rm -f "$out"; return 0
  fi
  echo "    full output: $out (${dur}s)"
  return 1
}

# --debug-assertions-off --self-test — prove BOTH of G6c's FAIL arms reachable.
run_g6c_selftest() {
  require_miri
  local rc=0 out
  echo "=== G6c --self-test arm 1: FLAG-LOSS (the vacuity arm, on the REAL crate) ==="
  echo "    re-runs the real lane with '-C debug-assertions=on' appended, which is"
  echo "    what a dropped RUSTFLAGS looks like; the family then reports 'ignored'."
  out="$(mktemp "${TMPDIR:-/tmp}/g6cst1.XXXXXX")"
  g6c_lane "-C debug-assertions=on" "$out"
  if g6c_verdict "$out"; then
    echo "  self-test FAIL: the lane stayed GREEN with debug assertions back ON —"
    echo "  the vacuity floor is not binding. ($out)"; rc=1
  else
    echo "  arm 1 OK: G6c goes RED when the flag is lost."
  fi
  rm -f "$out"

  echo "=== G6c --self-test arm 2: UB DETECTION + exit propagation (synthetic crate) ==="
  local D; D="$(mktemp -d)"
  ( cd "$D" && cargo init --lib --name g6cscratch -q 2>/dev/null
    { echo 'pub mod pagelist { #[cfg(test)] pub mod tests {'
      # Four passing tests so the vacuity floor is CLEARED and the run fails for
      # the UB alone — otherwise arm 2 would prove nothing arm 1 does not.
      for i in 1 2 3 4; do
        echo "#[test] fn pagelist_compress_page_ok_$i() { assert_eq!(2 + 2, 4); }"
      done
      echo '#[test] fn pagelist_compress_page_selftest_ub() {'
      echo '  let v = [0u8; 3]; let p = v.as_ptr(); let _x = unsafe { *p.add(9) };'
      echo '}'
      echo '} }'; } > src/lib.rs )
  out="$(mktemp "${TMPDIR:-/tmp}/g6cst2.XXXXXX")"
  ( G6C_PKG=g6cscratch G6C_DIR="$D" G6C_FILTERS="pagelist::tests::pagelist_compress_page" \
    G6C_MIN_PASSED=4 G6C_BOUND_SEC=300
    g6c_lane "" "$out"
    if g6c_verdict "$out" >/dev/null 2>&1; then exit 1; fi
    g6c_verdict "$out" 2>&1 | head -4
    exit 0 )
  if [ $? -eq 0 ]; then
    echo "  arm 2 OK: G6c goes RED and names the UB."
  else
    echo "  self-test FAIL: injected UB did not turn G6c RED. ($out)"; rc=1
  fi
  rm -rf "$D"; rm -f "$out"
  [ $rc = 0 ] && echo "G6c --self-test: BOTH FAIL ARMS REACHABLE"
  return $rc
}

enumerate_crate_dirs() {
  local cargo_toml
  for cargo_toml in crates/*/Cargo.toml crates-sys/*/Cargo.toml; do
    [ -f "$cargo_toml" ] || continue
    dirname "$cargo_toml"
  done
}

case "${1:---all-done}" in
  --self-test)
    require_miri
    D="$(mktemp -d)"
    ( cd "$D" && cargo init --lib --name g6scratch -q 2>/dev/null
      printf '%s\n' '#[test]' 'fn lk(){let _=Box::leak(Box::new(1u32));}' \
        '#[test]' 'fn ub(){let v=[0u8;3];let p=v.as_ptr();let _=unsafe{*p.add(9)};}' > src/lib.rs
      # NOTE: capture output first — under `set -o pipefail`, `miri(exit 1)|grep`
      # returns miri's non-zero status even when grep matches, which would
      # falsely invert the check.
      lk_out="$(cargo +nightly miri test lk 2>&1 || true)"
      printf '%s' "$lk_out" | grep -qiE 'memory leaked' && echo "self-test: leak DETECTED ok" || { echo "self-test FAIL: leak not detected"; exit 1; }
      ub_out="$(cargo +nightly miri test ub 2>&1 || true)"
      printf '%s' "$ub_out" | grep -qiE 'Undefined Behavior' && echo "self-test: UB DETECTED ok" || { echo "self-test FAIL: UB not detected"; exit 1; } )
    rc=$?; rm -rf "$D"; exit $rc ;;
  --debug-assertions-off)
    case "${2:-}" in
      --self-test) run_g6c_selftest; exit $? ;;
      "") run_g6c; exit $? ;;
      *) echo "usage: check_leaks.sh --debug-assertions-off [--self-test]"; exit 2 ;;
    esac ;;
  --crate)
    [ -n "${2:-}" ] || { echo "usage: --crate <name>"; exit 2; }
    run_crate "$2"; exit $? ;;
  --list)
    printf '%-24s %-28s %-8s %s\n' "crate" "status" "gated" "lane"
    while IFS= read -r dir; do
      c="$(basename "$dir")"
      status="$(crate_status_of "$dir")"
      if claimed_done "$dir"; then gated="YES"; else gated="no"; fi
      if is_ffi "$c"; then lane="ASan"
      elif [ -n "$(live_ffi_tests "$c")" ]; then lane="Miri(--lib)+ASan(live tests)"
      else lane="Miri"; fi
      printf '%-24s %-28s %-8s %s\n' "$c" "$status" "$gated" "$lane"
    done < <(enumerate_crate_dirs)
    exit 0 ;;
  --all-done)
    rc=0; ran=0; probed=0; failed_probe=0
    while IFS= read -r dir; do
      c="$(basename "$dir")"
      probed=$((probed + 1))
      claimed_done "$dir"; st=$?
      case $st in
        0) ran=$((ran + 1)); run_crate "$c" || rc=1 ;;
        1) : ;;                       # honestly not claimed-done: skip
        *) failed_probe=$((failed_probe + 1)); rc=1 ;;   # LOUD, never silent
      esac
    done < <(enumerate_crate_dirs)
    if [ "$failed_probe" -gt 0 ]; then
      echo "G6 RED: the done-status probe FAILED for $failed_probe of $probed crate(s)."
      echo "  Those crates were neither run nor honestly skipped — treat this run as"
      echo "  covering nothing until the probe works."
      rc=1
    fi
    # A run that examined ZERO crates is not progress, it is a vacuous green.
    # With the old `|| continue`, a probe broken for every crate landed here and
    # printed the progress line on its way to exit 0.
    if [ "$ran" = 0 ]; then
      if [ "$probed" -gt 0 ] && [ "$failed_probe" = 0 ]; then
        echo "G6: no claimed-done crates yet (progress)"
      else
        echo "G6 RED (VACUOUS): ran 0 of $probed crate(s) and cannot claim coverage."
        rc=1
      fi
    fi
    [ "$rc" = 0 ] && echo "G6 GREEN ($ran done crate(s) clean)"; exit $rc ;;
  *)
    echo "usage: check_leaks.sh [--crate <name> | --all-done (default) | --list | --self-test | --debug-assertions-off [--self-test]]"; exit 2 ;;
esac
