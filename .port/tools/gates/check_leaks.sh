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
# RUNNER (P7-W15, 2026-10-02; memory-safety-lanes design §A): every Miri lane
# runs on `cargo miri nextest run` through ONE helper, `run_miri_nextest
# <crate> <model sb|tb> <debug-assertions on|off> <filterset> [scope args]`.
# Each test is its own Miri process, so one UB stops one test, not the whole
# binary (N494: a single finding used to hide every later vt test), and the
# processes run in parallel (performance cores minus 2; MIRI_TEST_THREADS
# overrides). Selection is a nextest filterset resolved from the binary's own
# test list, never libtest's in-interpreter substring filter (the G6c Tree
# Borrows stall). Miri's leak check still runs at every process exit — now per
# test. Doc tests, which nextest does not run, keep a libtest `--doc` arm, so
# G6's test set is unchanged (R5). The libtest runner stays for one release
# behind `--libtest` (any mode below), so the two can be compared.
#
# Usage:
#   check_leaks.sh                    same as --all-done (the G6 gate run)
#   check_leaks.sh --all-done         run Miri/ASan for every claimed-done crate
#   check_leaks.sh --crate <name>     run the right tool for that crate
#   check_leaks.sh --list             enumerate crates + status + lane (no run)
#   check_leaks.sh --self-test        prove Miri catches a leak and a UB (libtest
#                                     arms, kept) and that the nextest runner
#                                     goes RED on UB under SB and TB, names the
#                                     test, and calls an empty filterset vacuous
#   check_leaks.sh --debug-assertions-off
#                                     GATE G6c: the `-C debug-assertions=off`
#                                     vt Miri lane (see the G6c block below)
#   check_leaks.sh --debug-assertions-off --self-test
#                                     prove G6c's FAIL arms are reachable
#   check_leaks.sh --tree-borrows     GATE G6t: Tree Borrows over the pointer-
#                                     heavy scope (see the G6t block below)
#   check_leaks.sh --tree-borrows --self-test
#                                     prove G6t's FAIL arms and its scope
#   check_leaks.sh --libtest <mode>   the pre-nextest runner for G6 / G6c
#                                     (`--all-done`, `--crate`, `--debug-assertions-off`)
# Exit 0 = GREEN (clean) ; 3 = BLOCKED (miri toolchain missing — a tooling
# input, surfaced by ci.sh as a WARNING per the CI.md exit contract, never a
# silent green and never misclassified as a claimed-done regression) ; any
# other non-zero = RED (UB/leak/unrouted binary) or bad usage (2).
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
cd "$RUSTTY_ROOT" || exit 1

# `--libtest` selects the pre-nextest runner for the mode that follows.
MIRI_RUNNER=nextest
if [ "${1:-}" = "--libtest" ]; then MIRI_RUNNER=libtest; shift; fi

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

require_nextest() {
  # Same contract as require_miri: a missing runner is BLOCKED (exit 3), never
  # a silent green. Install: https://nexte.st (prebuilt: get.nexte.st/latest/<os>).
  if ! cargo nextest --version >/dev/null 2>&1; then
    echo "G6 BLOCKED: cargo-nextest not available (install it, or pass --libtest)"; exit 3
  fi
}

run_crate() {
  if [ "$MIRI_RUNNER" = libtest ]; then run_crate_libtest "$1"; else run_crate_nextest "$1"; fi
}

live_binaries_routed() {  # $1 = crate, $2 = its live_ffi_tests list
  # GUARD (tightening): with the Miri lane narrowed to --lib, EVERY
  # integration-test binary (tests/*.rs) of this crate must be explicitly
  # routed in live_ffi_tests(), otherwise a future unlisted binary would run
  # under neither Miri nor ASan in G6 — that is a RED, not a silent escape.
  local c="$1" live="$2" tf tb unrouted=""
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
  return 0
}

# The pre-nextest G6 crate runner (`--libtest`), unchanged.
run_crate_libtest() {
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
    live_binaries_routed "$c" "$live" || return 1
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
  asan_companions "$c" || mrc=1
  return $mrc
}

# The ASan arms that accompany a crate's Miri lane (both runners).
asan_companions() {
  local c="$1" mrc=0
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
# unless the flag took (see g6c_verdict_libtest).
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
# under Miri. Measured: the 119-test lane took 6272 s (C-1, 2026-09-28, on a
# host shared with other builds). This is the hang guard only: the lane still has to run EVERY
# derived test, pass them all, and stay UB- and leak-clean.
G6C_BOUND_SEC="${G6C_BOUND_SEC:-10800}"

g6c_killtree() { local p=$1 c; for c in $(pgrep -P "$p" 2>/dev/null); do g6c_killtree "$c"; done; kill -9 "$p" 2>/dev/null; }

# g6c_lane_libtest <extra-rustflags> <outfile> — run the lane once, hard-bounded.
g6c_lane_libtest() {
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

# g6c_verdict_libtest <outfile> — 0 only for a clean AND non-vacuous run.
g6c_verdict_libtest() {
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

run_g6c_libtest() {
  require_miri
  echo "=== G6c [$G6C_PKG] :: Miri, -C debug-assertions=off ==="
  echo "    $G6C_MIN_PASSED test(s), derived from the cfg_attr(all(miri, debug_assertions)) annotations"
  echo "    (measured GREEN in 3092s serial on an M4 Pro, 2026-07-30, and 3952s for 83 tests at 9fd17d93; set G6C_FILTERS to narrow)"
  local out; out="$(mktemp "${TMPDIR:-/tmp}/g6c.XXXXXX")"
  local t0=$SECONDS
  g6c_lane_libtest "" "$out"
  local dur=$((SECONDS - t0))
  if g6c_verdict_libtest "$out"; then
    echo "G6c GREEN (${dur}s; debug assertions off, UB/leak-clean, non-vacuous)"
    rm -f "$out"; return 0
  fi
  echo "    full output: $out (${dur}s)"
  return 1
}

# --debug-assertions-off --self-test — prove BOTH of G6c's FAIL arms reachable.
run_g6c_selftest_libtest() {
  require_miri
  local rc=0 out
  echo "=== G6c --self-test arm 1: FLAG-LOSS (the vacuity arm, on the REAL crate) ==="
  echo "    re-runs the real lane with '-C debug-assertions=on' appended, which is"
  echo "    what a dropped RUSTFLAGS looks like; the family then reports 'ignored'."
  out="$(mktemp "${TMPDIR:-/tmp}/g6cst1.XXXXXX")"
  g6c_lane_libtest "-C debug-assertions=on" "$out"
  if g6c_verdict_libtest "$out"; then
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
    g6c_lane_libtest "" "$out"
    if g6c_verdict_libtest "$out" >/dev/null 2>&1; then exit 1; fi
    g6c_verdict_libtest "$out" 2>&1 | head -4
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

# ===========================================================================
# THE NEXTEST RUNNER (P7-W15) — shared by G6, G6c and G6t.
# ===========================================================================

# Parallel Miri processes: performance cores minus 2 (at least 1). Each Miri
# process is single-threaded, so this leaves two cores for the host.
miri_threads() {
  if [ -n "${MIRI_TEST_THREADS:-}" ]; then echo "$MIRI_TEST_THREADS"; return; fi
  local p
  p="$(sysctl -n hw.perflevel0.physicalcpu 2>/dev/null || nproc 2>/dev/null || echo 3)"
  p=$((p - 2)); [ "$p" -ge 1 ] || p=1
  echo "$p"
}

mn_killtree() { local p=$1 c; for c in $(pgrep -P "$p" 2>/dev/null); do mn_killtree "$c"; done; kill -9 "$p" 2>/dev/null; }

# mn_verdict <outfile> <label> <exit-code> — 0 only for a clean, non-vacuous run.
# Reads (optional) MN_EXPECT: a file of "<crate> <test path>" lines, every one
# of which must have PASSED; MN_MIN_PASSED: a floor on the passed count.
# Writes the passed count to <outfile>.passed.
#
# RED when: the build failed; the run never printed its summary (killed at the
# hang bound, or crashed); any Miri `Undefined Behavior` or `memory leaked`
# report, attributed to the test whose output carries it; any test that did not
# PASS; zero tests passed (vacuous); an expected test that did not pass; or a
# non-zero exit nothing above explains.
mn_verdict() {
  python3 - "$1" "$2" "$3" "${MN_EXPECT:-}" "${MN_MIN_PASSED:-0}" <<'PY'
import re, sys
out, label, rc, expect, floor = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4], int(sys.argv[5])
lines = open(out, errors='replace').read().split('\n')
# `        PASS [   1.234s] ( 3/52) rustty-vt search::x::tests::y`
status_re = re.compile(r'^\s+(PASS|FAIL|ABORT|TIMEOUT|LEAK|LEAK-FAIL|FLAKY|SIG[A-Z]+|EXIT \d+|TRY \d+ [A-Z-]+)\s+\[[^\]]*\]\s+(?:\([^)]*\)\s+)?(\S+)\s+(\S+)\s*$')
header_re = re.compile(r'STD(?:OUT|ERR):\s+(\S+)\s+(\S+?)\s*-*\s*$')
final, cur, findings = {}, None, {}
for i, l in enumerate(lines):
    m = status_re.match(l)
    if m:
        cur = m.group(2) + ' ' + m.group(3)
        final[cur] = m.group(1)
        continue
    m = header_re.search(l)
    if m:
        cur = m.group(1) + ' ' + m.group(2)
        continue
    if re.search(r'error: (Undefined Behavior|memory leaked)', l):
        ex = [l.strip()]
        for k in lines[i + 1:i + 15]:
            if '-->' in k:
                ex.append(k.strip()); break
        findings.setdefault(cur or '<output not attributable to a test>', [])
        if ex not in findings[cur or '<output not attributable to a test>']:
            findings[cur or '<output not attributable to a test>'].append(ex)
summary = [l for l in lines if re.match(r'^\s*Summary \[', l)]
passed = sorted(t for t, s in final.items() if s == 'PASS')
failed = sorted(t for t, s in final.items() if s != 'PASS')
open(out + '.passed', 'w').write(str(len(passed)))
red = []
if any(re.search(r'^error\[E\d+\]|error: could not compile', l) for l in lines):
    red.append('the Miri test build FAILED:')
    red += ['    ' + l for l in lines if re.search(r'^error', l)][:6]
elif not summary:
    red.append('no nextest summary: the run did not finish (hang bound, crash or kill). Last lines:')
    red += ['    ' + l for l in lines[-6:]]
for t, exs in sorted(findings.items()):
    for ex in exs[:2]:
        red.append('Miri finding in %s:' % t)
        red += ['    ' + e for e in ex]
if failed:
    red.append('%d test(s) did not pass:' % len(failed))
    red += ['    %s %s' % (final[t], t) for t in failed[:25]]
    if len(failed) > 25:
        red.append('    ... and %d more' % (len(failed) - 25))
import os
native = os.environ.get('MN_NATIVE_RUNNABLE', '')
if summary and not passed and not failed and not findings and native == '0':
    print('    %s: no runnable test in the NATIVE build either (every test is #[ignore]d in source, not by a Miri cfg)' % label)
elif summary and not passed:
    red.append('VACUOUS: 0 tests passed (the filterset matched nothing runnable, or everything was skipped).')
if passed and len(passed) < floor:
    red.append('only %d test(s) passed, expected at least %d.' % (len(passed), floor))
if expect:
    want = [w.strip() for w in open(expect) if w.strip()]
    missing = [w for w in want if final.get(w) != 'PASS']
    if missing:
        red.append('%d of %d expected test(s) did not PASS (skipped as ignored, absent, or failed):' % (len(missing), len(want)))
        red += ['    %s (%s)' % (w, final.get(w, 'not run')) for w in missing[:15]]
if rc != 0 and not red:
    red.append('nextest exited %d with no failure in its output.' % rc)
if red:
    print('  RED [%s]:' % label)
    for r in red:
        print('    ' + r)
    sys.exit(1)
print('    %s' % summary[-1].strip())
PY
}

# run_miri_nextest <crate> <model sb|tb> <debug-assertions on|off> <filterset> [cargo scope args...]
#   The one Miri runner. Env: MN_OUT (output file, required), MN_DIR (workspace,
#   default the repo), MN_MIRIFLAGS / MN_RUSTFLAGS (extra flags), MN_BOUND_SEC
#   (hang bound for the whole invocation, 0 = none), MN_EXPECT / MN_MIN_PASSED
#   (see mn_verdict). Ignored tests stay ignored: nextest runs only the tests the
#   binary does not report as ignored, and the binary is the Miri-compiled one,
#   so `cfg_attr(miri, ignore)` and `cfg_attr(all(miri, debug_assertions),
#   ignore)` decide exactly as they did under libtest. Leak checking is Miri's
#   default and is never turned off here (no -Zmiri-ignore-leaks).
run_miri_nextest() {
  local crate="$1" model="$2" da="$3" fs="$4"; shift 4
  local mf="${MN_MIRIFLAGS:-}" rf="${MN_RUSTFLAGS:-}" out="${MN_OUT:?run_miri_nextest needs MN_OUT}"
  local bound="${MN_BOUND_SEC:-0}" cpid kpid="" rc t0=$SECONDS
  case "$model" in
    sb) ;;
    tb) mf="$mf -Zmiri-tree-borrows" ;;
    *) echo "run_miri_nextest: model must be sb or tb, got '$model'"; return 2 ;;
  esac
  case "$da" in
    on) ;;
    off) rf="-C debug-assertions=off $rf" ;;
    *) echo "run_miri_nextest: debug assertions must be on or off, got '$da'"; return 2 ;;
  esac
  mf="$(printf '%s' "$mf" | sed 's/^ *//; s/ *$//')"; rf="$(printf '%s' "$rf" | sed 's/^ *//; s/ *$//')"
  echo "    [$crate] Miri $( [ "$model" = tb ] && echo 'Tree' || echo 'Stacked') Borrows, debug assertions $da, $(miri_threads) processes, MIRIFLAGS='$mf'${rf:+, RUSTFLAGS='$rf'}"
  echo "    filterset: $(printf '%s' "$fs" | cut -c1-160)$( [ ${#fs} -gt 160 ] && echo " ... (${#fs} chars)")"
  (
    cd "${MN_DIR:-$RUSTTY_ROOT}" || exit 2
    [ -n "$rf" ] && export RUSTFLAGS="$rf"
    export MIRIFLAGS="$mf"
    exec cargo +nightly miri nextest run -p "$crate" "$@" -E "$fs" \
      --test-threads "$(miri_threads)" --no-fail-fast --no-tests=pass --color never \
      --failure-output immediate-final --success-output never >"$out" 2>&1
  ) &
  cpid=$!
  if [ "$bound" -gt 0 ]; then ( sleep "$bound"; mn_killtree "$cpid" 2>/dev/null ) & kpid=$!; fi
  wait "$cpid" 2>/dev/null; rc=$?
  # killtree the watchdog too: a bare kill orphans its `sleep`, which holds the
  # inherited stdout open and hangs any consumer for the full bound (G6c, 2026-07).
  [ -n "$kpid" ] && { mn_killtree "$kpid" 2>/dev/null; wait "$kpid" 2>/dev/null; }
  MN_SECS=$((SECONDS - t0))
  echo "    [$crate] ${MN_SECS}s"
  # MN_NATIVE_IGNORED_OK (G6's whole-crate arms only): a crate whose every test
  # is `#[ignore]`d in source (rustty-wasm: its one test needs the wasm artifact)
  # runs 0 tests, which libtest's lane reported as `ok. 0 passed; 1 ignored`. The
  # VACUOUS rule is waived only when the NATIVE build of the same selection also
  # has no runnable test, so a test skipped by `cfg_attr(miri, ignore)` still
  # leaves the run VACUOUS (NEW-FINDINGS N507).
  local native=""
  if [ -n "${MN_NATIVE_IGNORED_OK:-}" ] && grep -qE '^ *Summary \[[^]]*\] 0 tests run' "$out"; then
    native="$( ( cd "${MN_DIR:-$RUSTTY_ROOT}" && cargo +nightly nextest list -p "$crate" "$@" -E "$fs" --message-format json 2>/dev/null ) | python3 -c '
import json, sys
try: d = json.load(sys.stdin)
except Exception: print("unknown"); sys.exit(0)
print(sum(1 for s in d.get("rust-suites", {}).values() for t in s.get("testcases", {}).values()
          if t.get("filter-match", {}).get("status") == "matches" and not t.get("ignored")))')"
  fi
  MN_NATIVE_RUNNABLE="$native" mn_verdict "$out" "$crate" "$rc"
}

mn_tmp() { mktemp "${TMPDIR:-/tmp}/mn.$1.XXXXXX"; }

# Doc tests: nextest does not run them, `cargo miri test` did, so G6 keeps a
# libtest `--doc` arm per crate (same set as before, R5).
#
# A crate whose `[lib]` sets `doctest = false` has no doc tests: `cargo miri
# test -p <c>` (the libtest lane) honours that, but an explicit `--doc` overrides
# it and compiles every `///` code block anyway (found by the first full G6 on
# nextest: rustty-cli's ported shell-example comments, NEW-FINDINGS N507). So the
# arm reads the flag from `cargo metadata` and skips such a crate, naming why.
lib_doctest_disabled() {  # $1 = crate; 0 = its lib target has doctest = false
  ( cd "${MN_DIR:-$RUSTTY_ROOT}" && cargo metadata --no-deps --format-version 1 2>/dev/null ) \
    | python3 -c '
import json, sys
c = sys.argv[1]
for p in json.load(sys.stdin)["packages"]:
    if p["name"] != c: continue
    for t in p["targets"]:
        if "lib" in t["kind"] or "rlib" in t["kind"] or "cdylib" in t["kind"]:
            sys.exit(0 if t.get("doctest") is False else 1)
sys.exit(1)' "$1"
}
miri_doctests() {
  local c="$1" d r line
  if lib_doctest_disabled "$c"; then
    echo "    [$c] doc tests: none ([lib] doctest = false, which cargo miri test honours)"; return 0
  fi
  d="$(mn_tmp doc)"
  ( cd "${MN_DIR:-$RUSTTY_ROOT}" && cargo +nightly miri test -p "$c" --doc ) >"$d" 2>&1; r=$?
  if grep -q 'no library targets found' "$d"; then
    echo "    [$c] doc tests: none (no library target)"; rm -f "$d"; return 0
  fi
  if [ "$r" -ne 0 ] || grep -qE 'Undefined Behavior|memory leaked' "$d"; then
    echo "  RED [$c]: Miri doc tests (exit $r):"
    grep -nE 'Undefined Behavior|memory leaked|^test .* FAILED|^error' "$d" | head -8 | sed 's/^/    /'
    echo "    full output: $d"; return 1
  fi
  line="$(grep -E '^test result:' "$d" | tail -1)"
  echo "    [$c] doc tests: ${line:-no doc-test summary}"; rm -f "$d"; return 0
}

run_crate_nextest() {
  local c="$1" live out rc=0
  if is_ffi "$c"; then run_crate_libtest "$c"; return $?; fi   # ASan lane: no Miri
  require_miri; require_nextest
  out="$(mn_tmp "$c")"
  live="$(live_ffi_tests "$c")"
  if [ -n "$live" ]; then
    live_binaries_routed "$c" "$live" || return 1
    echo "=== G6 [$c] :: Miri via nextest (unit surface --lib; live-FFI test binaries routed to ASan per MIRI_EXCEPTIONS.md) ==="
    MN_OUT="$out" MN_NATIVE_IGNORED_OK=1 run_miri_nextest "$c" sb on 'all()' --lib || rc=1
    local t
    for t in $live; do
      echo "=== G6 [$c --test $t] :: ASan (live syscalls; Miri cannot execute them) ==="
      RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" \
        cargo +nightly test -p "$c" --test "$t" --target "$(rustc -vV | sed -n 's/host: //p')" 2>&1 || rc=1
    done
  else
    echo "=== G6 [$c] :: Miri via nextest (UB + leaks, one process per test) ==="
    MN_OUT="$out" MN_NATIVE_IGNORED_OK=1 run_miri_nextest "$c" sb on 'all()' || rc=1
    miri_doctests "$c" || rc=1
    asan_companions "$c" || rc=1
  fi
  if [ "$rc" = 0 ]; then rm -f "$out" "$out.passed"; else echo "    full output: $out"; fi
  return $rc
}

# ---- G6c on nextest ---------------------------------------------------------
# The test set is still DERIVED from the annotations (g6c_names). Each derived
# fn name is resolved to its full test path(s) from the crate's NATIVE test list
# (a native `--list` is instant; libtest's filter under Miri was not), and the
# lane selects exactly those paths: `test(=<path>) | ...`. A derived name with
# no test path is RED (the annotation sits on something that is not a test in
# this build). Every resolved path must PASS.
g6c_paths() {  # prints "<pkg> <path>" lines; exit 1 if a derived name has no test
  local list; list="$(mn_tmp g6clist)"
  ( cd "$G6C_DIR" && cargo test -q -p "$G6C_PKG" --lib -- --list 2>/dev/null ) | sed -n 's/: test$//p' >"$list"
  python3 - "$list" "$G6C_PKG" $G6C_FILTERS <<'PY'
import sys
paths = [p.strip() for p in open(sys.argv[1]) if p.strip()]
pkg, names, bad = sys.argv[2], sys.argv[3:], []
for n in names:
    hits = [p for p in paths if p.rsplit('::', 1)[-1] == n]
    if not hits:
        bad.append(n)
    for h in hits:
        print(pkg, h)
for b in bad:
    print('G6c: annotated name has no test in the native list: %s' % b, file=sys.stderr)
sys.exit(1 if bad or not names else 0)
PY
  local r=$?; rm -f "$list"; return $r
}

# exact_filterset <file of "<crate> <path>"> — `test(=a) | test(=b) | ...`
exact_filterset() { awk '{printf "%stest(=%s)", (NR>1 ? " | " : ""), $2}' "$1"; }

# g6c_lane_nextest <extra-rustflags> <outfile>
g6c_lane_nextest() {
  local extra="$1" out="$2" exp n
  exp="$(mn_tmp g6cexp)"
  if ! g6c_paths >"$exp"; then echo "  RED [G6c]: the annotation-derived set does not resolve (above)"; rm -f "$exp"; return 1; fi
  n="$(grep -c . "$exp")"
  echo "    $n test path(s) from $(printf '%s\n' $G6C_FILTERS | grep -c .) annotated name(s)"
  MN_DIR="$G6C_DIR" MN_MIRIFLAGS="-Zmiri-disable-isolation" MN_RUSTFLAGS="$extra" \
    MN_BOUND_SEC="$G6C_BOUND_SEC" MN_EXPECT="$exp" MN_MIN_PASSED="$G6C_MIN_PASSED" MN_OUT="$out" \
    run_miri_nextest "$G6C_PKG" sb off "$(exact_filterset "$exp")" --lib
  local r=$?; rm -f "$exp"; return $r
}

run_g6c() {
  if [ "$MIRI_RUNNER" = libtest ]; then run_g6c_libtest; return $?; fi
  require_miri; require_nextest
  echo "=== G6c [$G6C_PKG] :: Miri via nextest, -C debug-assertions=off ==="
  echo "    $G6C_MIN_PASSED test(s), derived from the cfg_attr(all(miri, debug_assertions)) annotations"
  local out; out="$(mn_tmp g6c)"
  if g6c_lane_nextest "" "$out"; then
    echo "G6c GREEN (${MN_SECS:-?}s; debug assertions off, UB/leak-clean, non-vacuous)"
    rm -f "$out" "$out.passed"; return 0
  fi
  echo "    full output: $out"; return 1
}

# A synthetic crate for the runner's FAIL arms: three passing tests, an
# out-of-bounds read, an aliasing violation (UB under BOTH models: the second
# `&mut x` invalidates the raw pointer's tag under SB, and disables it under TB
# once the raw write lands), and a leak.
mn_fixture() {  # prints the crate dir
  local D; D="$(mktemp -d "${TMPDIR:-/tmp}/mnfix.XXXXXX")"
  ( cd "$D" && cargo init --lib --name mnfixture -q 2>/dev/null
    cat > src/lib.rs <<'RS'
#[cfg(test)]
mod fixture {
    #[test] fn ok_1() { assert_eq!(2 + 2, 4); }
    #[test] fn ok_2() { assert_eq!(2 + 2, 4); }
    #[test] fn ok_3() { assert_eq!(2 + 2, 4); }
    #[test] fn ub_out_of_bounds() { let v = [0u8; 3]; let p = v.as_ptr(); let _x = unsafe { *p.add(9) }; }
    #[test] fn ub_aliasing() {
        let mut x = 0u8;
        let p = &mut x as *mut u8;
        let r = &mut x;
        unsafe { *p = 1; }
        *r = 2;
    }
    #[test] fn leaks() { let _ = Box::leak(Box::new(1u32)); }
}
RS
  )
  echo "$D"
}

# mn_fixture_selftest <model> — the runner goes RED on the fixture, names each
# bad test, and still runs the three good ones (one UB no longer hides the rest).
mn_fixture_selftest() {
  local model="$1" D out v rc=0 t
  D="$(mn_fixture)"; out="$(mn_tmp fix)"
  v="$(MN_DIR="$D" MN_OUT="$out" run_miri_nextest mnfixture "$model" on 'all()' 2>&1)"
  printf '%s\n' "$v" | sed 's/^/  | /' | head -24
  if ! printf '%s' "$v" | grep -q 'RED \[mnfixture\]'; then
    echo "  self-test FAIL ($model): the UB/leak fixture was not RED"; rc=1
  fi
  for t in ub_out_of_bounds ub_aliasing leaks; do
    if ! printf '%s' "$v" | grep -q "Miri finding in mnfixture fixture::$t"; then
      echo "  self-test FAIL ($model): the report does not name fixture::$t"; rc=1
    fi
  done
  if [ "$(cat "$out.passed" 2>/dev/null)" != 3 ]; then
    echo "  self-test FAIL ($model): expected the 3 good tests to pass alongside the bad ones, got $(cat "$out.passed" 2>/dev/null)"; rc=1
  fi
  [ $rc = 0 ] && echo "  arm OK ($model): UB (out of bounds, aliasing) and the leak are RED, each named; the good tests still ran"
  rm -rf "$D"; rm -f "$out" "$out.passed"
  return $rc
}

mn_vacuous_selftest() {
  local D out v rc=0
  D="$(mn_fixture)"; out="$(mn_tmp vac)"
  v="$(MN_DIR="$D" MN_OUT="$out" run_miri_nextest mnfixture sb on 'test(=fixture::no_such_test)' 2>&1)"
  printf '%s\n' "$v" | sed 's/^/  | /' | tail -3
  if printf '%s' "$v" | grep -q 'VACUOUS'; then echo "  arm OK: an empty filterset is a vacuous RED"
  else echo "  self-test FAIL: an empty filterset was not a vacuous RED"; rc=1; fi
  rm -rf "$D"; rm -f "$out" "$out.passed"
  return $rc
}

# N507's two G6 arms, proven both ways on a scratch crate: (a) a crate whose
# every test is #[ignore]d in source passes, but one skipped only by
# cfg_attr(miri, ignore) stays VACUOUS; (b) `[lib] doctest = false` skips the doc
# arm, and without it the same non-Rust code block is RED.
mn_n507_selftest() {
  local D out v rc=0
  D="$(mktemp -d "${TMPDIR:-/tmp}/mnn507.XXXXXX")"; out="$(mn_tmp n507)"
  ( cd "$D" && cargo init --lib --name mnn507 -q 2>/dev/null
    printf '\n[lib]\ndoctest = false\n' >> Cargo.toml
    cat > src/lib.rs <<'RS'
/// Examples:
///
///     mnn507 +ssh user@example.com
pub fn run() {}
#[cfg(test)]
mod t {
    #[test]
    #[ignore = "needs an external artifact"]
    fn source_ignored() {}
}
RS
  )
  v="$(MN_DIR="$D" MN_OUT="$out" MN_NATIVE_IGNORED_OK=1 run_miri_nextest mnn507 sb on 'all()' 2>&1)"; printf '%s\n' "$v" | sed 's/^/  | /' | tail -2
  if printf '%s' "$v" | grep -q 'NATIVE build either' && ! printf '%s' "$v" | grep -q 'RED'; then echo "  arm OK: all tests #[ignore]d in source is not vacuous"
  else echo "  self-test FAIL: a crate with only source-ignored tests was RED"; rc=1; fi
  sed -i '' 's/#\[ignore = "needs an external artifact"\]/#[cfg_attr(miri, ignore)]/' "$D/src/lib.rs"
  v="$(MN_DIR="$D" MN_OUT="$out" MN_NATIVE_IGNORED_OK=1 run_miri_nextest mnn507 sb on 'all()' 2>&1)"; printf '%s\n' "$v" | sed 's/^/  | /' | tail -2
  if printf '%s' "$v" | grep -q 'VACUOUS'; then echo "  arm OK: a test skipped only under Miri still leaves the run VACUOUS"
  else echo "  self-test FAIL: a Miri-only skip passed as non-vacuous"; rc=1; fi
  v="$(MN_DIR="$D" miri_doctests mnn507 2>&1)"; printf '%s\n' "$v" | sed 's/^/  | /' | head -2
  if printf '%s' "$v" | grep -q 'doctest = false' && ! printf '%s' "$v" | grep -q RED; then echo "  arm OK: [lib] doctest = false skips the doc arm"
  else echo "  self-test FAIL: doctest = false was not honoured"; rc=1; fi
  sed -i '' '/^\[lib\]$/d; /^doctest = false$/d' "$D/Cargo.toml"
  v="$(MN_DIR="$D" miri_doctests mnn507 2>&1)"; printf '%s\n' "$v" | sed 's/^/  | /' | head -2
  if printf '%s' "$v" | grep -q 'RED \[mnn507\]: Miri doc tests'; then echo "  arm OK: with doc tests enabled, the non-Rust block is RED"
  else echo "  self-test FAIL: the enabled doc arm did not go RED on a non-Rust block"; rc=1; fi
  rm -rf "$D"; rm -f "$out" "$out.passed"
  return $rc
}

run_g6c_selftest() {
  if [ "$MIRI_RUNNER" = libtest ]; then run_g6c_selftest_libtest; return $?; fi
  require_miri; require_nextest
  local rc=0 out
  echo "=== G6c --self-test arm 1: FLAG-LOSS (on the REAL crate and the REAL derived set) ==="
  echo "    re-runs the lane with '-C debug-assertions=on' appended; the family then reports 'ignored'."
  out="$(mn_tmp g6cst1)"
  if g6c_lane_nextest "-C debug-assertions=on" "$out" >"$out.v" 2>&1; then
    cat "$out.v"; echo "  self-test FAIL: the lane stayed GREEN with debug assertions back ON"; rc=1
  else
    tail -6 "$out.v"; echo "  arm 1 OK: G6c goes RED when the flag is lost."
  fi
  rm -f "$out" "$out.v" "$out.passed"
  echo "=== G6c --self-test arm 2: UB DETECTION through the G6c lane (synthetic crate) ==="
  local D; D="$(mktemp -d "${TMPDIR:-/tmp}/g6cfix.XXXXXX")"
  ( cd "$D" && cargo init --lib --name g6cscratch -q 2>/dev/null
    { echo 'pub mod pagelist { #[cfg(test)] pub mod tests {'
      for i in 1 2 3 4; do
        echo "#[test] fn pagelist_compress_page_ok_$i() { assert_eq!(2 + 2, 4); }"
      done
      echo '#[test] fn pagelist_compress_page_selftest_ub() {'
      echo '  let v = [0u8; 3]; let p = v.as_ptr(); let _x = unsafe { *p.add(9) };'
      echo '}'
      echo '} }'; } > src/lib.rs )
  out="$(mn_tmp g6cst2)"
  ( G6C_PKG=g6cscratch G6C_DIR="$D" G6C_MIN_PASSED=4 G6C_BOUND_SEC=600 \
    G6C_FILTERS="pagelist_compress_page_ok_1 pagelist_compress_page_ok_2 pagelist_compress_page_ok_3 pagelist_compress_page_ok_4 pagelist_compress_page_selftest_ub"
    g6c_lane_nextest "" "$out" ) >"$out.v" 2>&1
  if grep -q 'RED \[g6cscratch\]' "$out.v" && grep -q 'Miri finding in g6cscratch pagelist::tests::pagelist_compress_page_selftest_ub' "$out.v"; then
    grep -A3 'RED \[' "$out.v" | head -6; echo "  arm 2 OK: G6c goes RED and names the UB test."
  else
    cat "$out.v"; echo "  self-test FAIL: injected UB did not turn G6c RED with the test named"; rc=1
  fi
  rm -rf "$D"; rm -f "$out" "$out.v" "$out.passed"
  [ $rc = 0 ] && echo "G6c --self-test: BOTH FAIL ARMS REACHABLE"
  return $rc
}

# ---------------------------------------------------------------------------
# GATE G6t — Tree Borrows over the pointer-heavy scope (P7-W15; design §A).
#
# ADDITIVE (R5): G6 stays the full-coverage Stacked Borrows lane. Tree Borrows
# is the forward-looking model and accepts/rejects different patterns (N492
# was TB-only), so G6t runs it where long-lived raw aliases are dense. Same
# split as G6/G6c: the G6c-annotated tests (`cfg_attr(all(miri,
# debug_assertions), ignore)`) run with debug assertions OFF and isolation
# disabled, exactly as in G6c; everything else with them ON, as in G6.
#
# THE SCOPE, defined here and nowhere else. `<crate>` = the whole crate (every
# test target, as G6 runs it); `<crate>:<module path>` = that crate's lib tests
# under `<module path>::`; `<crate>:-<test path>` = one named test taken back
# out of a module entry, each with its reason below. `--tree-borrows
# --self-test` proves every module entry exists and matches more than 0 tests,
# and every exclusion names exactly one test.
#
# `rustty-renderer` is an FFI crate (G6 runs it under ASan, not Miri), so it is
# scoped to the modules whose tests run under Miri (measured, P7-W15: 66 of its
# 143 tests). EXCLUDED, with the reason "FFI (Metal/ObjC/glslang/SPIRV/onig/
# wuffs) — covered by G-ASAN/G-LSAN": `generic::macos_impl`, `metal::buffer`,
# `metal::iosurface_layer`, `metal::metal_impl`, `metal::pipeline`,
# `metal::shaders::post_pipeline_error_tests`, `metal::target`, `metal::tests`,
# `link`, `shadertoy`, `opengl`, and the integration-test binaries; each fails
# under Miri with `unsupported operation` (a foreign call), never with UB. The
# three `-` exclusions are the same: one FFI-bound test in an otherwise
# Miri-runnable module (Metal device, Wuffs PNG decoder, Oniguruma). The fourth,
# `state::tests::renderer_state_yield_to_demand_is_free_without_demand`, asserts
# a 500 ms wall-clock bound over 10,000 calls, which the interpreter cannot
# meet: not a memory-safety property; it runs natively and under G-ASAN/G-LSAN.
# A test inside an included module that fails, for any reason, stays RED.
# Details: .port/phase-2/MIRI_EXCEPTIONS.md, "P7-W15", G6t scope.
G6T_SCOPE="${G6T_SCOPE:-
rustty-vt:page rustty-vt:pagelist rustty-vt:search rustty-vt:render
rustty-vt:screen rustty-vt:terminal rustty-vt:snapshot rustty-vt:kitty
rustty-vt-c
rustty-renderer:cell rustty-renderer:cursor rustty-renderer:dmabuf
rustty-renderer:generic::surface_mailbox_tests rustty-renderer:generic::derived_config_tests
rustty-renderer:health_tests rustty-renderer:image::macos_impl rustty-renderer:latest_frame
rustty-renderer:log_capture rustty-renderer:message::change_config_tests
rustty-renderer:metal::gpu_abi rustty-renderer:metal::shaders::metallib_tests
rustty-renderer:metal::test_support rustty-renderer:overlay rustty-renderer:state
rustty-renderer:thread rustty-renderer:undefined rustty-renderer:webgl
rustty-renderer:-generic::derived_config_tests::decoded_bg_image_len_is_wh4
rustty-renderer:-image::macos_impl::tests::image_empty_lifecycle
rustty-renderer:-message::change_config_tests::change_config_deinit_frees_links
rustty-renderer:-state::tests::renderer_state_yield_to_demand_is_free_without_demand
rustty-xev
rustty-core
}"
G6T_BOUND_SEC="${G6T_BOUND_SEC:-14400}"

g6t_crates() { printf '%s\n' $G6T_SCOPE | sed 's/:.*//' | awk '!seen[$0]++'; }
g6t_modules() { printf '%s\n' $G6T_SCOPE | sed -n "s/^$1:\([^-].*\)/\1/p"; }
g6t_excludes() { printf '%s\n' $G6T_SCOPE | sed -n "s/^$1:-\(.*\)/\1/p"; }
g6t_filterset() {  # $1 = crate
  local mods x fs; mods="$(g6t_modules "$1" | paste -sd'|' -)"
  if [ -z "$mods" ]; then fs='all()'; else fs="test(/^($mods)::/)"; fi
  for x in $(g6t_excludes "$1"); do fs="$fs - test(=$x)"; done
  echo "$fs"
}

# g6t_crate <crate> <extra-rustflags-for-the-off-half> — one crate's G6t run.
g6t_crate() {
  local c="$1" extra="$2" fs out rc=0 exp mods
  fs="$(g6t_filterset "$c")"
  out="$(mn_tmp "g6t.$c")"
  # Module entries name library modules (the scope self-test checks src/<m>),
  # so a module-scoped crate runs --lib: without it nextest builds and lists
  # every integration-test binary under Miri for zero matches (measured on
  # the first G6t run: 1,597 tests, all in the lib, across 11 binaries).
  local scope_args=""; [ -n "$(g6t_modules "$c")" ] && scope_args="--lib"
  if [ -z "$extra" ]; then
    MN_BOUND_SEC="$G6T_BOUND_SEC" MN_OUT="$out" run_miri_nextest "$c" tb on "$fs" $scope_args || rc=1
    if [ "$rc" = 0 ]; then rm -f "$out" "$out.passed"; else echo "    full output: $out"; fi
  fi
  if [ "$c" = "$G6C_PKG" ]; then
    exp="$(mn_tmp g6texp)"
    mods="$(g6t_modules "$c" | paste -sd'|' -)"
    if ! g6c_paths >"$exp.all"; then echo "  RED [G6t]: the G6c-derived set does not resolve"; rm -f "$exp" "$exp.all"; return 1; fi
    if [ -n "$mods" ]; then grep -E "^$c ($mods)::" "$exp.all" >"$exp"; else cp "$exp.all" "$exp"; fi
    rm -f "$exp.all"
    if [ -s "$exp" ]; then
      out="$(mn_tmp "g6t.$c.off")"
      echo "    [$c] the $(grep -c . "$exp") G6c-annotated test(s) in scope, debug assertions off:"
      MN_MIRIFLAGS="-Zmiri-disable-isolation" MN_RUSTFLAGS="$extra" MN_BOUND_SEC="$G6T_BOUND_SEC" \
        MN_EXPECT="$exp" MN_MIN_PASSED="$(grep -c . "$exp")" MN_OUT="$out" \
        run_miri_nextest "$c" tb off "$(exact_filterset "$exp")" --lib || rc=1
      if [ "$rc" = 0 ]; then rm -f "$out" "$out.passed"; else echo "    full output: $out"; fi
    fi
    rm -f "$exp"
  fi
  return $rc
}

run_g6t() {
  require_miri; require_nextest
  local c rc=0 t0=$SECONDS
  echo "=== G6t :: Miri Tree Borrows via nextest over: $G6T_SCOPE ==="
  for c in $(g6t_crates); do
    echo "=== G6t [$c] ==="
    g6t_crate "$c" "" || rc=1
  done
  if [ "$rc" = 0 ]; then echo "G6t GREEN ($((SECONDS - t0))s; Tree Borrows, UB/leak-clean, non-vacuous)"
  else echo "G6t RED ($((SECONDS - t0))s)"; fi
  return $rc
}

# Scope check: every entry names an existing crate (and module), and matches
# more than 0 runnable tests under Miri.
g6t_scope_selftest() {
  local e c m dir n rc=0 fs top want
  for e in $G6T_SCOPE; do
    c="${e%%:*}"; m=""; [ "$e" != "$c" ] && m="${e#*:}"
    dir="crates/$c"
    if [ ! -f "$dir/Cargo.toml" ]; then echo "  scope FAIL: $e — no crate at $dir"; rc=1; continue; fi
    top="${m#-}"; top="${top%%::*}"
    if [ -n "$m" ] && [ ! -f "$dir/src/$top.rs" ] && [ ! -f "$dir/src/$top/mod.rs" ] \
       && ! grep -qE "^\s*(pub(\(crate\))? )?mod $top\b" "$dir/src/lib.rs"; then
      echo "  scope FAIL: $e — no module $top in $dir/src"; rc=1; continue
    fi
    # A module entry must match > 0 runnable tests; an exclusion exactly one
    # test (ignored or not), so a renamed test cannot leave a dead exclusion.
    case "$m" in
      "") fs='all()'; want=gt0 ;;
      -*) fs="test(=${m#-})"; want=one ;;
      *) fs="test(/^$m::/)"; want=gt0 ;;
    esac
    n="$(cargo +nightly miri nextest list -p "$c" -E "$fs" --message-format json 2>/dev/null | WANT="$want" python3 -c '
import json, os, sys
d = json.load(sys.stdin); n = 0
one = os.environ.get("WANT") == "one"
for s in d.get("rust-suites", {}).values():
    for t in s.get("testcases", {}).values():
        if t.get("filter-match", {}).get("status") == "matches" and (one or not t.get("ignored")):
            n += 1
print(n)' 2>/dev/null)"
    if [ "$want" = one ]; then
      if [ "${n:-0}" = 1 ]; then echo "  scope ok: $e — names 1 test"
      else echo "  scope FAIL: $e — names ${n:-0} test(s), expected exactly 1"; rc=1; fi
    elif [ "${n:-0}" -gt 0 ]; then echo "  scope ok: $e — $n test(s)"
    else echo "  scope FAIL: $e — matches ${n:-0} runnable test(s)"; rc=1; fi
  done
  return $rc
}

run_g6t_selftest() {
  require_miri; require_nextest
  local rc=0 out
  echo "=== G6t --self-test arm 1: the scope (every entry exists and matches > 0 tests) ==="
  g6t_scope_selftest || rc=1
  echo "=== G6t --self-test arm 2: deliberate UB and a leak are RED under Tree Borrows ==="
  mn_fixture_selftest tb || rc=1
  echo "=== G6t --self-test arm 3: an empty filterset is a vacuous RED ==="
  mn_vacuous_selftest || rc=1
  echo "=== G6t --self-test arm 4: FLAG-LOSS on the debug-assertions-off half (real crate) ==="
  out="$(mn_tmp g6tst4)"
  if g6t_crate "$G6C_PKG" "-C debug-assertions=on" >"$out" 2>&1; then
    cat "$out"; echo "  self-test FAIL: the off half stayed GREEN with debug assertions back ON"; rc=1
  else
    tail -5 "$out"; echo "  arm 4 OK: the G6c-annotated half goes RED when the flag is lost."
  fi
  rm -f "$out"
  [ $rc = 0 ] && echo "G6t --self-test: ALL ARMS REACHABLE"
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
    rc=$?; rm -rf "$D"
    [ "$rc" = 0 ] || exit $rc
    [ "$MIRI_RUNNER" = libtest ] && exit 0
    # The nextest runner's arms (the libtest arms above are kept).
    require_nextest
    echo "=== G6 --self-test: the nextest runner, Stacked Borrows ==="
    mn_fixture_selftest sb || rc=1
    echo "=== G6 --self-test: the nextest runner, Tree Borrows ==="
    mn_fixture_selftest tb || rc=1
    echo "=== G6 --self-test: an empty filterset ==="
    mn_vacuous_selftest || rc=1
    echo "=== G6 --self-test: N507 (source-ignored crate; [lib] doctest = false) ==="
    mn_n507_selftest || rc=1
    [ "$rc" = 0 ] && echo "G6 --self-test: ALL ARMS REACHABLE"
    exit $rc ;;
  --debug-assertions-off)
    case "${2:-}" in
      --self-test) run_g6c_selftest; exit $? ;;
      "") run_g6c; exit $? ;;
      *) echo "usage: check_leaks.sh --debug-assertions-off [--self-test]"; exit 2 ;;
    esac ;;
  --tree-borrows)
    [ "$MIRI_RUNNER" = libtest ] && { echo "G6t runs on nextest only (it is new; there is no libtest G6t)"; exit 2; }
    case "${2:-}" in
      --self-test) run_g6t_selftest; exit $? ;;
      "") run_g6t; exit $? ;;
      *) echo "usage: check_leaks.sh --tree-borrows [--self-test]"; exit 2 ;;
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
    echo "usage: check_leaks.sh [--libtest] [--crate <name> | --all-done (default) | --list | --self-test | --debug-assertions-off [--self-test] | --tree-borrows [--self-test]]"; exit 2 ;;
esac
