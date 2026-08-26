#!/usr/bin/env bash
# ci.sh — the SINGLE entry point the rustty kickoff CI runs.
#
# Governed by .port/PORT_RULES.md (R1 fidelity, R2 identity, R3 self-contained /
# no ghostty Cargo dep, R4 acyclic crate graph, R5 gates-invariant) and
# .port/phase-0/plan/DEFINITION_OF_PARITY.md (the authoritative gate defs G1-G9).
# .github/workflows/ci.yml mirrors this file so CI is REAL (not a paper gate).
#
# WHAT IT DOES (in order; collects ALL failures, never stops at the first):
#   1. cargo fmt --check                       (HARD-FAIL)
#   2. cargo clippy --workspace -D warnings    (HARD-FAIL)
#   3. cargo build --workspace                 (HARD-FAIL; also the live R4/G7
#                                               proof — Cargo refuses a normal-dep
#                                               cycle, so a clean build IS hard
#                                               acyclicity)
#   4. cargo test --workspace                  (HARD-FAIL; PASS/FAIL oracle for
#                                               every ported test, R1/G1)
#   5. the parity gates in .port/tools/gates/  (the canonical Phase-0 gate suite)
#
# GATE SUITE (canonical scripts; ci.sh does NOT reimplement them — it runs them):
#   G7  check_acyclic.sh             crate graph acyclic (R4)        -> HARD-FAIL
#   G4  run_differential.sh          differential VT + harness ctrl  -> HARD-FAIL
#   G-REL check_release_lane.sh      `cargo test --release` green    -> HARD-FAIL
#                                    (steps 3/4 only cover the DEBUG profile;
#                                    this is the release counterpart. Build +
#                                    test under `--release`, watchdogged so a
#                                    hang is RED rather than an infinite wait.)
#   G4-uni run_unicode_diff.sh       unicode tables vs LIVE uucode    -> HARD-FAIL
#                                    (ORACLE #2: rustty's embedded width/symbol/
#                                    Properties/grapheme tables diffed full-range
#                                    0..=0x10FFFF + 8192 grapheme entries against
#                                    uucode 0.2.0 — the faithful reproduction of
#                                    ghostty's props/symbols_uucode "tables match
#                                    uucode" tests whose live oracle R3 forbids
#                                    in-crate. 0 diffs required.)
#   G-LINT check_lints.sh            the non-Rust linters: shellcheck,
#                                    prettier, typos, swiftlint, alejandra,
#                                    pinact                          -> HARD-FAIL
#                                    (the reference runs each as its own
#                                    test.yml job via `nix develop -c <tool>`;
#                                    rustty runs them from one gate so this
#                                    file stays the single source of truth.
#                                    A tool that is absent reports `skipped`
#                                    BY NAME; all-skipped exits 3 BLOCKED.)
#   G-CCONSUMER check_c_consumer.sh  the C/CMake/pkg-config consumer lane
#                                    -> HARD-FAIL. cmake configure/build/
#                                    install, the installed artifact set, both
#                                    .pc files, and a C smoke program compiled,
#                                    linked and RUN against pkg-config output
#                                    twice (shared + static). Merges the
#                                    reference's build-cmake and
#                                    test-lib-vt-pkgconfig jobs.
#   G-EXAMPLES check_examples.sh     every example/*/build.sh compiles + RUNS,
#                                    and the c-vt-cmake* examples configure +
#                                    build through cmake         -> HARD-FAIL.
#                                    The reference's list-examples +
#                                    build-examples-* jobs. wasm-* belong to
#                                    the opt-in --webgl lane and are reported
#                                    skipped by name.
#   G-DIST check_dist.sh             `xtask dist` builds and the tarball has
#                                    the right shape                -> HARD-FAIL
#                                    (single rustty-<v>/ prefix, build-critical
#                                    files present, vendored crates-sys C
#                                    present, .port/ + docs/superpowers/
#                                    excluded). `xtask distcheck` additionally
#                                    under RUSTTY_DISTCHECK=1 — a cold full
#                                    build, so opt-in; skipped is reported by
#                                    name, never greened.
#   G-NIX check_nix.sh               `nix flake check` (librustty-vt + its five
#                                    passthru tests) and `alejandra --check`
#                                    -> HARD-FAIL where nix exists, BLOCKED
#                                    (exit 3) where it does not. The hermetic
#                                    counterpart to G-CCONSUMER: same install,
#                                    no ambient system libraries.
#   G-THEMES check_theme_corpus.py   the 602 vendored iterm2 theme files match
#                                    a committed per-file SHA-256 manifest
#                                    -> HARD-FAIL. res_themes.rs only counts
#                                    them; content drift is invisible to a
#                                    count. P3-W9 T5.
#   G-BENCH check_bench_tests.sh     `rustty-bench` lib tests, debug  -> HARD-FAIL
#                                    AND --release, plus a test-count floor.
#                                    The workspace lanes already RUN the crate;
#                                    what they cannot see is a test being
#                                    DELETED. WS10 T4.
#   G-WINPROTO-DEFAULT               the DEFAULT GUI dep graph per target
#     check_default_linux_winproto.sh (Linux + FreeBSD: gtk4/libadwaita and both
#                                    winproto arms; macOS: none)     -> HARD-FAIL
#                                    F-007 wrote it; W2D/F-002 added the FreeBSD
#                                    arm, the only machine-checkable half of
#                                    "FreeBSD builds the GTK app" off a FreeBSD
#                                    host. `cargo tree` only — no build.
#   G1  check_tests_ported.py        100% test{} ported (==3234)     -> PROGRESS*
#   G1b check_test_counts.py         per-module rust #[test] >= zig  -> PROGRESS*
#                                    test{} (minus documented apc-deferred) —
#                                    the ONLY fidelity net for the non-src/
#                                    crates outside the 3234 contract (vaxis/
#                                    zf/xev/wuffs/z2d/...). It greps the
#                                    read-only ghostty oracle checkout
#                                    directly (R3: external oracle, never
#                                    shipped to CI), so ci.sh probe-gates it:
#                                    oracle absent (CI) -> exit 3 BLOCKED
#                                    (WARNING, never a vacuous green); oracle
#                                    present (local) -> BINDING, any module
#                                    with rust < zig-deferred is RED and
#                                    fails CI.
#   G2  check_assertions.py          assertion-for-assertion         -> PROGRESS*
#   G3  check_comments_fidelity.py   zero comment-inventory delta    -> PROGRESS*
#   G3b check_comment_coverage.py    content-aware comment coverage  -> PROGRESS*
#                                    (every distinctive reference comment has a
#                                    match in the port; 0 MISSING — closes the
#                                    aggregate-count hole G3 can mask)
#   G9  check_parity_surface.py      config/keybind/action/terminfo  -> PROGRESS*
#   G8  check_baseline_regression.py no perf/mem/size/startup regress-> ARMED
#   G8b check_perf_fence.sh          paired same-session perf fence  -> PERF-FENCE
#   G5  run_fuzz_smoke.sh            differential fuzz clean         -> PROGRESS*
#   G6  check_leaks.sh               leak-free (tracking-alloc/Miri) -> PROGRESS*
#   G6b check_pagelist_compress_leaks.sh
#                                    PageList compressed-page encodings are not
#                                    leaked (global-allocator Vec that no other
#                                    memory gate can see)              -> HARD
#   G6c check_leaks.sh --debug-assertions-off
#                                    the vt page-compression family under Miri
#                                    with `Page::assert_integrity` off — the
#                                    tests G6 cannot reach at all -> PROGRESS
#
#   D1-D9  Bun semantic-mismatch detectors (BUN_PORT_LEARNINGS §4) — each kills a
#          class of invisible Zig->Rust divergence that slipped every Bun gate:
#   D1  check_slice_reinterpret.sh    slice-reinterpret allowlist    -> PROGRESS
#   D2  check_capacity_saturation.sh  fixed-array/counter inventory  -> PROGRESS
#   D3  check_constants_parity.py     derived-constant expr parity   -> PROGRESS
#   D4  check_comptime_phase.sh       golden VT bytes + ST-integrity -> PROGRESS
#   D5  check_paired_invariants.sh    cross-site invariant catalog   -> PROGRESS
#   D6  check_sanitizer_canary.sh     ASan-visibility canary + cfg   -> PROGRESS
#   D8  check_clone_injection.sh      clone-injection review list    -> PROGRESS (advisory)
#   D9  check_stale_comments.sh       stale-comment contract check   -> PROGRESS (advisory)
#   D10 check_stb_resize_parity.sh    vendored stb_image_resize output vs ghostty's
#                                     own build recipe (4 zig optimize modes) +
#                                     a machine-checked negative control  -> PROGRESS
#   G-KAITAI check_kaitai_snapshot.sh the snapshot .ksy wire spec parses all
#                                     checked-in snapshot goldens (CRC32C +
#                                     byte accounting); BLOCKED without
#                                     kaitai tooling, never silent   -> PROGRESS
#          Registered PROGRESS not HARD (R5): brand-new, fail-closed against
#          hand-maintained audit ledgers; PROGRESS avoids a transiently
#          un-passable CI while a new hit precedes its ledger row by a commit.
#          All GREEN on today's tree; each fail-closed one ships a --self-test.
#
#  HARD-FAIL gates must be green from day one (G7 holds via the topo-ordered
#   acyclic rustty Cargo graph; G4 is ARMED: it keys on the port driver
#   harness/bin/vt_driver_port (override: $RUSTTY_VT_DRIVER; build it with
#   .port/phase-0/harness/build_port_driver.sh) and runs the real byte-exact
#   differential — every harness fixture through vt_driver_ref vs
#   vt_driver_port via comparator.py, plus every c-vt golden case against its
#   port-built example binary. Any diff is RED (exit 1), never a warning; the
#   empty-divergence self-proof runs only while the port driver is absent).
#   A red here fails CI.
#
#   PROGRESS* gates: these are the parity gates that "PRINT progress without
#   failing CI while crates are unported." They are NOT weakened (R5): each keeps
#   its full DoP pass condition. The canonical checkers implement this natively —
#   a STUB crate with 0 tests/comments/etc. is GREEN (progress, exit 0); a crate
#   that *claims done* yet is short is RED (exit non-zero). So while crates are
#   unported they all exit 0 and print progress; the moment a claimed-done crate
#   regresses they go RED and ci.sh fails. That self-flip is exactly R5-correct:
#   progress now, binding the instant a crate is completed. A BLOCKED checker
#   (missing contract/tooling) is surfaced as a WARNING, never a silent green.
#
# CI exits non-zero iff ANY *non-progress* check failed — i.e. any HARD-FAIL
# check is red, OR a PROGRESS gate went RED because a claimed-done crate
# regressed (which is no longer "while unported").
#
# OPT-IN LANES (never part of the default run):
#   --webgl  WS14's wasm+browser lane — builds the freestanding
#            `rustty_wasm.wasm` cdylib and runs the WebGL2 pixel gate in headless
#            Chrome. Opt-in because it is heavyweight relative to everything above
#            (a second full target's artifacts + a browser process) and needs two
#            things CI boxes do not all have: the `wasm32-unknown-unknown` rustup
#            target and a Chrome/Chromium binary. Missing either is exit 3
#            (BLOCKED/env-missing) — NEVER a silent green.
#
# Usage:
#   bash .port/tools/ci.sh                   # full run (what CI runs)
#   CI_SKIP_CARGO=1 bash .port/tools/ci.sh   # gates only (no cargo; local/fast)
#   bash .port/tools/ci.sh --webgl           # ONLY the opt-in WS14 wasm/WebGL lane
set -uo pipefail   # NOTE: no `-e` — run every check, aggregate failures.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1
GATES="$ROOT/.port/tools/gates"
PY="${PYTHON:-python3}"
# Metrics file G8b emits and G8 then consumes (WS10 T3). Under $TMPDIR so a CI
# run with no measurement substrate simply leaves it absent -> G8 progress mode.
PERF_METRICS="${PERF_METRICS:-${TMPDIR:-/tmp}/rustty-perf-metrics.json}"

# ---- opt-in lanes ----------------------------------------------------------
# Parsed and dispatched BEFORE the default run so `--webgl` is a lane you ask for,
# not an addition to what CI runs by default.
WEBGL_LANE=0
LZ4_SLOW_LANE=0
# ---- --fast: the LANE-LEVEL subset -----------------------------------------
# A full ci.sh is dominated by a handful of gates (G6/G6c Miri — a single vt lane
# measured 3092s — plus G4 differential, G5 fuzz, G8/G8b perf). Running that per
# working lane costs hours and finds almost nothing a lane can act on: EVERY real
# UB this program has found came from a SCOPED Miri harness or a specific test,
# never from the breadth of the full sweep. The full sweep's value is
# regression-catching breadth, which is an INTEGRATION concern.
#
# So: lanes run `--fast`; the integrator runs the full suite once per merge batch.
#
# ANTI-VACUITY: --fast must never be mistakable for a full pass. It refuses to
# print the ALL-GREEN banner, names every gate it skipped, and exits 4 (a distinct
# code) so no script can treat a fast run as a release verdict.
FAST_LANE=0
SKIPPED_GATES=()
for arg in "$@"; do
  case "$arg" in
    --webgl) WEBGL_LANE=1 ;;
    --lz4-slow) LZ4_SLOW_LANE=1 ;;
    --fast) FAST_LANE=1 ;;
    *) echo "ci.sh: unknown argument '$arg' (known: --webgl, --lz4-slow, --fast)" >&2; exit 2 ;;
  esac
done

# Gates skipped by --fast, by id. Deliberately a NAMED list rather than a
# duration heuristic: a gate that becomes slow should be added here consciously,
# with the cost written down, not silently dropped because it crossed a timeout.
FAST_SKIP_IDS=" G4 G4-uni G5 G6 G6c G8 G8b "

skip_in_fast() {
  # usage: skip_in_fast <id> ; returns 0 (true) when --fast should skip it
  [ "$FAST_LANE" -eq 1 ] || return 1
  case "$FAST_SKIP_IDS" in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

# P3-W0 T7 step 3 — the PERIODIC lane for the exhaustive LZ4 differential.
#
# `compress::lz4_differential::lz4_differential_exhaustive` early-returns unless
# RUSTTY_LZ4_SLOW is set, and that variable was set NOWHERE in the repo: no
# gate, no ci.sh entry, no workflow. The shape is faithful to upstream, but a
# Rust early return reports PASSED where Zig's error.SkipZigTest reports
# SKIPPED, so the default run read as "4 seeds x the exhaustive budget just
# ran". The test is now `#[ignore]`d (honest reporting) AND this lane exists so
# the coverage is actually obtainable rather than theoretical.
#
# Opt-in because it is minutes, not seconds. Run it on any change under
# crates/rustty-vt/src/compress/, alongside G-LZ4.
run_lz4_slow_lane() {
  section "opt-in lane: exhaustive LZ4 differential  (--lz4-slow)"
  echo "--- RUSTTY_LZ4_SLOW=1 cargo test -p rustty-vt --lib --release --ignored lz4_differential_exhaustive"
  local out
  out="$(RUSTTY_LZ4_SLOW=1 cargo test -p rustty-vt --lib --release -- --ignored --nocapture \
          --exact compress::lz4_differential::lz4_differential_exhaustive 2>&1)"
  local crc=$?
  printf '%s\n' "$out" | tail -5
  if [ "$crc" -ne 0 ]; then
    echo ">> FAIL: the exhaustive LZ4 differential did not pass" >&2
    return 1
  fi
  # ANTI-VACUITY: the whole point of this lane is that the test really ran. An
  # `ignored` or `0 passed` result here means the env gate or the filter did not
  # take effect, and must NOT be reported as coverage.
  if ! printf '%s\n' "$out" | grep -q 'test result: ok\. 1 passed'; then
    echo ">> FAIL: expected exactly 1 passed test; the exhaustive differential" >&2
    echo "         did not actually run (filter did not take)." >&2
    return 1
  fi
  # "1 passed" is ALSO what an env-gated early return prints, so it proves
  # nothing on its own. The test emits this marker only after all four seeds
  # have actually run.
  if ! printf '%s\n' "$out" | grep -q 'RUSTTY_LZ4_SLOW_RAN seeds=4'; then
    echo ">> FAIL: the test reported success but never emitted its" >&2
    echo "         RUSTTY_LZ4_SLOW_RAN marker — the env gate did not take and" >&2
    echo "         the suite did NOT run. This is not coverage." >&2
    return 1
  fi
  echo "OK: the exhaustive LZ4 differential ran and passed."
  return 0
}

# WS14 — the wasm/WebGL lane. Two commands, in order; the gate itself owns the
# pass/fail verdict (positive arm must match AND both negative controls must fail)
# and its own exit-3 environment check for Chrome.
run_webgl_lane() {
  section "opt-in lane: WS14 wasm/WebGL  (--webgl)"
  if ! rustup target list --installed 2>/dev/null | grep -qx 'wasm32-unknown-unknown'; then
    echo "BLOCKED: the wasm32-unknown-unknown target is not installed." >&2
    echo "         Install it with: rustup target add wasm32-unknown-unknown" >&2
    echo "         Exiting 3 (environment missing) rather than passing vacuously." >&2
    return 3
  fi
  echo "--- .port/tools/build-main-wasm.sh --release"
  if ! sh "$ROOT/.port/tools/build-main-wasm.sh" --release; then
    echo ">> FAIL: the freestanding rustty_wasm.wasm cdylib did not build" >&2
    return 1
  fi
  echo "--- .port/tools/gates/ws14_webgl_browser_gate.sh"
  sh "$GATES/ws14_webgl_browser_gate.sh"
}

# G1b runner: check_test_counts.py greps the read-only ghostty oracle checkout
# directly (R3: the oracle is an EXTERNAL reference, never shipped to CI). With
# the checkout absent, every zig ref glob would expand to nothing -> zig=0 for
# every module -> a VACUOUS green. Two defenses, NEITHER env-overridable (an
# override that could redirect the probe away from the path the script actually
# greps was a spoof hole — probe passes on a fake dir while the script counts
# zig=0 everywhere -> vacuous green; killed, R5):
#   1. this probe checks the SAME hard-coded root the script greps (one source
#      of truth) — absent -> exit 3 BLOCKED (surfaced as a WARNING by
#      run_gate, never a silent green);
#   2. check_test_counts.py itself exits 3 BLOCKED if any explicitly listed
#      zig reference file is missing (absent/partial oracle or zig-cache dep
#      checkout), so even a bypassed probe can never yield a vacuous green.
# Oracle present -> the per-module count gate runs BINDING (any module with
# rust #[test] < zig test{} minus documented apc-deferred is RED and fails
# CI — no done-status grace; it binds per-module today).
run_test_counts() {
  # Must stay identical to the root check_test_counts.py hard-codes (defense 1).
  local oracle="/Users/gael/projects/ghostty"
  if [ ! -d "$oracle/src" ]; then
    echo "BLOCKED: ghostty oracle checkout not found at $oracle (R3 — the"
    echo "         external oracle never ships to CI); the per-module"
    echo "         test-count gate cannot bind here."
    return 3
  fi
  # WS19 T1 (N4), defense 3: the ZIG-DEP corpora under ~/.cache/zig/p/ are a CACHE and
  # get evicted (a zig upgrade or `rm -rf ~/.cache` is enough). check_test_counts.py
  # already exits 3 BLOCKED in that case, but it does so with a wall of paths and no
  # remedy. Pre-flight it here so the BLOCKED message names the one command that fixes
  # it. --verify-only never fetches, so CI stays hermetic.
  if ! bash "$ROOT/.port/tools/restore_zig_deps.sh" --verify-only >/dev/null 2>&1; then
    echo "BLOCKED: the pinned zig-dep source corpora (libxev / zf / vaxis) under"
    echo "         ~/.cache/zig/p/ are absent or incomplete, so G1b's ZIG side"
    echo "         cannot bind (a missing ref would count zig=0 -> vacuous green)."
    echo "         Restore with: bash .port/tools/restore_zig_deps.sh"
    bash "$ROOT/.port/tools/restore_zig_deps.sh" --verify-only 2>&1 | sed 's/^/         /'
    return 3
  fi
  "$PY" "$GATES/check_test_counts.py"
}

# Gate registry: "DoP_id|classification|runner...". classification is HARD or
# PROGRESS. HARD-fail gates run first (fail-fast visibility), then PROGRESS.
# Each runner is invoked verbatim; its exit code is interpreted below:
#   0  -> PASS (or, for a PROGRESS gate, "progress, not failing CI")
#   3  -> BLOCKED (missing contract/tooling) -> WARNING, never a silent green
#   other non-zero -> FAIL  (HARD gate: fails CI; PROGRESS gate: a claimed-done
#                            regression -> fails CI, per R5)
GATE_REGISTRY=(
  "G7|HARD|bash $GATES/check_acyclic.sh"
  "G4|HARD|bash $GATES/run_differential.sh"
  "G4-uni|HARD|bash $ROOT/.port/phase-0/harness/unicode/run_unicode_diff.sh"
  # G-LINT (2026-08-25) — the non-Rust linter lane: shellcheck, prettier,
  # typos, swiftlint, alejandra, pinact. The reference runs each as its own
  # `test.yml` job through `nix develop -c <tool>`; rustty's configs for all
  # six (.shellcheckrc, .prettierignore, typos.toml, .swiftlint.yml,
  # .github/pinact.yml) were checked in during the top-level-files port but
  # NOTHING EVER RAN THEM, so they were decoration. HARD: every lane is green
  # as of this commit. A linter that is not installed reports `skipped` by
  # name (and the gate exits 3 BLOCKED if EVERY lane is skipped) rather than
  # contributing a vacuous green — see the gate's header.
  "G-LINT|HARD|bash $GATES/check_lints.sh"
  # G-CCONSUMER (2026-08-25) — the C/CMake/pkg-config consumer lane, the merge
  # of the reference's `build-cmake` + `test-lib-vt-pkgconfig` jobs. Nothing
  # here was exercised before: `cargo test` never invokes cmake, so a 400-line
  # CMakeLists.txt, the package config, the install rules and the (entirely
  # absent) pkg-config files were unverified. Its first run found three real
  # defects — no .pc files at all, an unresolvable `OnigEncodingKOI8` in the
  # static archive, and a CMake custom command that never re-invoked cargo.
  # HARD: green, and the artifact arm has a demonstrated RED flip (remove a
  # .pc from the install list -> "the install tree is incomplete").
  "G-CCONSUMER|HARD|bash $GATES/check_c_consumer.sh"
  # G-EXAMPLES (2026-08-25) — build (and run) every example/ program, the
  # analog of the reference's list-examples + build-examples-* jobs. 32 example
  # programs were checked in and none were built by anything.
  # example/README.md already promised "CI discovers examples by globbing
  # example/*/build.sh"; this gate is what makes that true. HARD: 27 build and
  # run today. The 5 wasm examples belong to ci.sh's opt-in --webgl lane and
  # are reported skipped BY NAME, never counted as green.
  "G-EXAMPLES|HARD|bash $GATES/check_examples.sh"
  # G-DIST (2026-08-25) — the source-tarball lane (the reference's build-dist /
  # build-dist-lib-vt). `xtask dist` was invoked from nothing but the release
  # workflows, so a broken tarball would first have surfaced DURING a release.
  # Builds it and asserts its shape (single versioned prefix, the files a
  # from-scratch build needs, >1000 vendored crates-sys files, and no .port/ or
  # docs/superpowers/ leakage). `xtask distcheck` — extract and BUILD the
  # tarball — is a cold full-workspace build and runs only under
  # RUSTTY_DISTCHECK=1 (set by ci.yml on the Ubuntu leg); otherwise the gate
  # reports it skipped BY NAME rather than implying it ran.
  "G-DIST|HARD|bash $GATES/check_dist.sh"
  # G-NIX (2026-08-26) — `nix flake check` + alejandra, the reference's
  # build-nix and alejandra jobs. Builds librustty-vt in a hermetic sandbox
  # with a pinned toolchain and runs the five passthru tests (symbols, both
  # .pc files, and a C consumer linked BOTH ways). BLOCKED, never green, where
  # nix is absent — which is every macOS leg here.
  "G-NIX|HARD|bash $GATES/check_nix.sh"
  # G-REL (WS18) — the `--release` test lane. Steps 3/4 above only ever exercise
  # the DEBUG profile, so nothing noticed that release had rotted to 22 failures
  # across 7 targets. Most were tests driving a seam the port deliberately
  # compiles out without `debug_assertions` (tripwire fail points, the
  # slow-runtime-safety integrity gate); those now carry
  # `#[cfg_attr(not(debug_assertions), ignore)]`, and this gate is what keeps a
  # NEW release-only failure — or a new injection test that forgot the attribute
  # — from going unnoticed again. HARD: it is green today and its verdict has a
  # `--self-test` RED-flip proof.
  "G-REL|HARD|bash $GATES/check_release_lane.sh"
  # G-GTKINIT (WS19 T4, audit row N3) — one `gtk4::init()` per test binary. GTK is
  # thread-pinned and libtest gives every `#[test]` its own thread, so a second
  # `gtk4::init()` in one binary ALWAYS panics. `tests/gtk_wayland_winproto.rs`
  # shipped with two and nothing noticed, because the GTK KATs are `#[ignore]`d and
  # only ever run by hand on the VM. This is a STATIC check on purpose: it binds on
  # macOS too, where those KATs cannot even compile, so a macOS lane catches a bad
  # merge. HARD: green at landing, milliseconds, and BOTH its FAIL arms (doubled
  # init; vacuous zero-match) are mutation-proven.
  "G-GTKINIT|HARD|bash $GATES/check_gtk_one_init.sh"
  # G-P2 (WS19 T19) — the Phase 2 CLOSURE gate, and the standing invariant that keeps
  # Phase 2 closed. It parses the four Phase-2 ledgers and FAILS if any open item lacks
  # an owner in an OPEN workstream, where "open workstream" is DERIVED from the ROADMAP's
  # own checklist rather than hardcoded (proven live: marking a WS merged flips the
  # positive-control fixture from PASS to FAIL). This is the gate that makes the WS19
  # audit's headline finding — 21 of 38 genuinely-open items were ORPHANED, filed into
  # workstreams that had already merged — structurally impossible to repeat silently.
  #
  # HARD, not PROGRESS, and deliberately so even though it is RED at landing: it is RED
  # because Phase 2 genuinely is not closed (2 items remain — see its output), and
  # downgrading a gate to avoid an inconvenient truth is exactly the R5 weakening this
  # workstream exists to stop. Its FAIL, PASS and BLOCKED arms are each fixture-proven
  # (`.port/tools/gates/fixtures/phase2-closure-{orphan,owned}.md`), which is the bar
  # G6b and G-REL met. When the last two items land it goes green and STAYS green.
  # --self-test runs FIRST and in the same step: it proves the ENVIRONMENT-BLOCKED
  # acceptance arm and the workstream derivation can still FAIL. Without it, a green
  # G-P2 could mean "nothing is open" or "the checker stopped binding", and those look
  # identical from the outside.
  "G-P2|HARD|$PY $GATES/check_phase2_closure.py --self-test && $PY $GATES/check_phase2_closure.py"
  # G-XEVREACH (WS20, 2026-07-30 — was G-XEVSCOPE, WS19 T7). The gate's FACT is
  # unchanged; its CONCLUSION is deleted. It used to justify classifying the
  # `iocp`/`wasi_poll` backends as "out of scope by platform" — that
  # classification was WRONG and is retracted (both backends are now PORTED:
  # `crates/rustty-xev/src/{iocp,windows,wasi_poll,wasi}.rs`), so the gate no
  # longer says anything about scope. What it still holds is a plain dependency
  # invariant worth keeping on its own terms: the wasm artifacts
  # (`rustty-wasm`, `rustty-web-canvas`, `rustty-vt-c`) run the synchronous C-ABI
  # with no event loop, so an xev edge in one of their trees is an unintended
  # dependency, and `rustty-os`'s single xev edge must stay macOS-gated or every
  # wasm artifact inherits it. HARD: green at landing, seconds, and BOTH FAIL arms
  # are mutation-proven (the `--self-test` arm drives the real macOS `rustty-os`
  # edge through the same detection).
  # G-ONIG (2026-07-30). P3-W9 found that NO ci.sh lane passed --features
  # oniguruma, so rustty-vt's string_map regex search and rustty-config's URL
  # regex table were compiled by nothing. A test that is never built cannot
  # fail, so those lanes were green by construction rather than by verification.
  # The gate floors the DELTA (with-feature minus without), not the absolute
  # count: -p rustty-vt --features oniguruma runs the WHOLE crate, so an
  # absolute floor would hold even if every gated test vanished.
  # G-FILES (2026-07-31). The blunt question no other gate asks: does every
  # oracle file have a Rust counterpart at all? G1/G2/G3 read a FROZEN phase-0
  # inventory, so a file upstream ADDED after the pin is invisible to all three
  # — which is how three post-pin files sat unexamined until the file-level
  # audit. An absence is acceptable only when ARGUED in NON_PORTS.md.
  "G-FILES|HARD|$PY $ROOT/.port/tools/audit_file_coverage.py"
  # G-SYMS (2026-07-31). Pass 2 of the audit: G-FILES proves a counterpart FILE
  # exists; this proves every oracle `pub fn` has a counterpart SYMBOL. A file
  # can exist, be the right size, cite its oracle source and still be missing
  # individual functions — that is what a partial port looks like, and nothing
  # else here looks for it.
  "G-SYMS|HARD|$PY $ROOT/.port/tools/audit_symbol_coverage.py"
  # G-TESTS (2026-07-31). Pass 3, and the same frozen-inventory blind spot one
  # level down: G1 compares test blocks against `.port/phase-0/inventory/`
  # captured at 4749c4e93, so a test upstream added since the pin is in neither
  # its numerator nor its denominator and G1 stays green while a whole commit's
  # worth of tests goes unported. This reads the LIVE oracle tree.
  #
  # Its own FAIL path is checked FIRST and separately, because the previous
  # version of this gate was VACUOUS and the control did not fire: renaming a
  # ported test away left it GREEN. `--self-test` breaks each of the four
  # acceptance arms (name / citation / TEST_AUDIT entry / vacuity) on a real row
  # and requires the gate to notice. If the self-test is red the gate's verdict
  # means nothing, so it runs as its own HARD entry rather than as a flag on the
  # main one.
  "G-TESTS-SELF|HARD|$PY $ROOT/.port/tools/audit_test_coverage.py --self-test"
  "G-TESTS|HARD|$PY $ROOT/.port/tools/audit_test_coverage.py"
  # G-CENSUS (2026-07-31) — the same blind spot as G-FILES/G-SYMS/G-TESTS, but
  # for the NUMBERS. G1/G1b/G2 measure against the phase-0 inventory frozen at
  # 4749c4e93; re-running that same tokenizer against the live tree at 6ad1fe7d8
  # gives 3550 blocks / 13533 assertions vs the contract's 3234 / 11975, i.e.
  # G2's floor is 88% of the real figure and 316 test blocks sit outside all
  # three gates entirely. The inventory stays FROZEN on purpose — it is the
  # contract the port was planned against, and regenerating it would restate the
  # denominator of every historical completion claim. This gate does not demand
  # a zero delta; it demands the delta be written down, so it cannot widen
  # unnoticed the next time the pin moves.
  "G-CENSUS-SELF|HARD|$PY $GATES/check_census_staleness.py --self-test"
  "G-CENSUS|HARD|$PY $GATES/check_census_staleness.py"
  "G-ONIG|HARD|bash $GATES/check_oniguruma_lane.sh"
  # G-VTCLIB (2026-08-10, codex-vet W36-B03) — the lib-vt C artifact must keep
  # the oracle's NULL decode_png default (sys.zig:16-20 `.lib => null`;
  # GhosttyZig.zig:67-68 pins artifact=.lib). Cargo feature UNIFICATION is the
  # hazard: any combined selection containing rustty-core resolves ONE rustty-vt
  # with `wuffs` unified in, flipping the vt-c artifact's default decoder — so
  # librustty-vt artifacts must come from ISOLATED `-p rustty-vt-c` invocations
  # only (the publish rule, ledgered in DEFERRALS.md §"decode_png"). The gate
  # pins the isolated graph wuffs-free (cargo tree, no compile), proves its own
  # detector on a simulated unification, and runs the `#[ignore]`d vt-c
  # null-default test under the one selection where it is binding. The SELF
  # entry first (G-CENSUS-SELF/G-TESTS-SELF convention): the runtime arm's FAIL
  # path is proven reachable by re-running the test with
  # `--features rustty-vt/wuffs`, which must go red — if it cannot, the main
  # entry's green means nothing.
  "G-VTCLIB-SELF|HARD|bash $GATES/check_vtc_lib_isolation.sh --self-test"
  "G-VTCLIB|HARD|bash $GATES/check_vtc_lib_isolation.sh"
  # G-GTKFEAT (2026-07-31) — the same hole as G-ONIG, one crate over. The
  # workspace clippy lane above builds with DEFAULT features, and rustty-apprt's
  # gtk/wayland/x11 features are default-OFF, so ~30k lines of ported GTK4/
  # Wayland/X11 code were linted by nothing: a lane that is never compiled cannot
  # fail. It had been red on main for some time (unused_mut in the wayland
  # queue-source, large_enum_variant on winproto::Window). Like G-ONIG the gate
  # floors the DELTA of gated tests — plus per-backend wayland_*/x11_* floors,
  # since losing a whole backend moves a 140-test delta by only 3 — so it cannot
  # go green by linting an empty shell. Exits 3 BLOCKED (a WARNING, never a pass)
  # on non-Linux, where --features gtk resolves to a no-op.
  "G-GTKFEAT|HARD|bash $GATES/check_gtk_feature_lane.sh"
  "G-XEVREACH|HARD|sh $GATES/xev_scope_boundary_gate.sh"
  # G-BENCH (WS10 T4) — the `rustty-bench` unit-test lane, debug AND --release.
  # `B-compression.md` §4.5 records "`rustty-bench` is in NO gate". Half right:
  # the crate is a workspace member, so a FAILING bench test already fails
  # `cargo test --workspace` and the G-REL release lane. What was genuinely
  # absent is (a) attribution — nothing in either aggregate lane names this
  # crate, which is how it came to be believed ungated — and (b) a test-count
  # FLOOR, so DELETING a bench test (or a whole module) was invisible. WS10
  # Layer 2 rewrites what these benchmarks measure, so that second gap is the
  # live one. HARD: green at landing (14 tests, both profiles, <1 s) and its
  # count arm is mutation-proven both synthetically (`--self-test`, 4 arms) and
  # live (floor raised to 15 against the real crate -> exit 1).
  "G-BENCH|HARD|bash $GATES/check_bench_tests.sh"
  # G-WINCHK (2026-08-11, codex-vet F-001 / W2D_WINDOWS_OS) — the shared OS layer
  # plus VT and VT-C must TYPE-CHECK for `x86_64-pc-windows-msvc`. The oracle
  # CI-tests exactly this surface (`build-libghostty-vt-windows`,
  # `.github/workflows/test.yml`); rustty's `rustty-os` had never compiled for
  # windows at all (25 errors: unix-gated `libc` used from windows-visible
  # modules, two eager `compile_error!`s, a `WindowsPtyError` with no `Error`
  # impl), which cut vt / vt-c / termio off entirely. No windows host exists here,
  # so this proves TYPE-CHECKING ONLY — never linking or execution; that limit is
  # restated in the script's own header rather than left implicit. Step 2 re-runs
  # the host check with `RUSTTY_SYS_CHECK_ONLY` UNSET, so the check-only `-sys`
  # skip cannot silently swallow the native build. Exits 3 BLOCKED (a warning,
  # never a pass) when the windows rust-std is not installed.
  "G-WINCHK|HARD|bash $GATES/w2f001_windows_check_gate.sh"
  # G-WINPROTO-DEFAULT (wired 2026-08-11, W2D §E/WI-14; the script predates this
  # and had never been on any roster) — the DEFAULT GUI dependency graph, pinned
  # per target. It was F-007's Linux/macOS predicate (both winproto arms on Linux,
  # neither on macOS) and now also carries W2D/F-002's FreeBSD arm: the oracle
  # gives FreeBSD the same `.gtk` runtime Linux gets (`apprt/runtime.zig:14-24`),
  # so the FreeBSD graph must resolve the same gtk4/libadwaita/winproto set. This
  # is the ONLY machine-checkable half of F-002 available off a FreeBSD host — the
  # gtk4-rs `-sys` crates probe pkg-config for the target, so a gtk-featured
  # `cargo check --target x86_64-unknown-freebsd` cannot run here (DEFERRALS.md
  # "FreeBSD — the platform's explicit ledger row"). `cargo tree` resolves without
  # building, so this is cheap and host-agnostic; the macOS control arm keeps it
  # from passing merely because a target gate stopped gating.
  "G-WINPROTO-DEFAULT|HARD|bash $GATES/check_default_linux_winproto.sh"
  "G1|PROGRESS|$PY $GATES/check_tests_ported.py"
  "G1b|PROGRESS|run_test_counts"
  "G2|PROGRESS|$PY $GATES/check_assertions.py"
  "G3|PROGRESS|$PY $GATES/check_comments_fidelity.py"
  "G3b|PROGRESS|$PY $GATES/check_comment_coverage.py"
  "G9|PROGRESS|$PY $GATES/check_parity_surface.py"
  # G8b — the real perf regression fence (paired, same-session, vs a stored
  # binary). BLOCKS (2/3) rather than passing when the box is loaded or the
  # untracked oracle binary/corpora are absent. MUST precede G8: it emits the
  # metrics file G8 consumes.
  "G8b|PERF-FENCE|bash $GATES/check_perf_fence.sh --emit-metrics $PERF_METRICS"
  # G8 is ARMED once G8b has emitted that metrics file; with no substrate
  # present G8b BLOCKs, the file is absent, and G8 falls back to progress mode.
  # NOT run with --strict: this harness measures the 14 benchmarks but NOT the
  # binary-size axis BASELINE.json also carries, and --strict would RED on those
  # 9 absent size labels. That axis is verification debt, not faked green.
  "G8|ARMED|$PY $GATES/check_baseline_regression.py --metrics $PERF_METRICS"
  "G5|PROGRESS|bash $GATES/run_fuzz_smoke.sh"
  "G6|PROGRESS|bash $GATES/check_leaks.sh --all-done"
  # G6b — PageList compressed-page encodings are not leaked (WS7 Phase E).
  # `CompressedPage::encoded` is a GLOBAL-allocator Vec, so the page-allocator
  # leak accounting cannot see it and G6's Miri lane cannot reach it (the
  # PageList compression tests are Miri-intractable). Deleting the discard-restore
  # loops from `PageList::{deinit,reset}` — the only thing that frees these
  # encodings — leaves the entire native rustty-vt suite GREEN. This gate runs the
  # mirihar-pagelist-compress harness's native counting-`#[global_allocator]`
  # `leakcheck` scenario (milliseconds, no Miri). HARD: green today, fast, and its
  # FAIL arm is proven reachable by `--self-test` (a negative control that
  # `mem::forget`s a compressed list, i.e. the real leak via the real path).
  "G6b|HARD|bash $GATES/check_pagelist_compress_leaks.sh"
  # G6c — the `-C debug-assertions=off` vt Miri lane (WS19 T6, closure-audit row
  # O2). ADDITIVE to G6, never a replacement: G6 keeps debug assertions ON (that
  # is what upholds the integrity-assertion coverage, and without the existing
  # `cfg_attr(miri, ignore)` annotations the vt G6 lane cannot complete at all).
  # G6c disables `Page::assert_integrity` — which is O(page), runs on nearly
  # every PageList mutation, and adds no SB/TB or leak coverage — and thereby
  # RUNS the page-level compression tests that are pure TIMEOUTs in G6. It is
  # NOT vacuous: the tests it counts are `cfg_attr(all(miri, debug_assertions),
  # ignore)`, so if the flag is ever lost they all report `ignored`, the vacuity
  # floor trips, and the gate goes RED rather than degenerating into a slow copy
  # of G6 — that arm is proven by `--debug-assertions-off --self-test` (arm 1),
  # alongside an injected-UB arm (arm 2). PROGRESS, not HARD, on first landing —
  # the same bedding-in convention the D1-D9 detectors use; it is a multi-minute
  # Miri lane on a shared host, so a loaded machine must not make CI
  # un-passable (R5). Promote to HARD once it has ridden clean.
  "G6c|PROGRESS|bash $GATES/check_leaks.sh --debug-assertions-off"
  # D1-D9 — Bun semantic-mismatch detectors (.port/phase-2/BUN_PORT_LEARNINGS.md
  # §4). Each kills one class of the invisible Zig->Rust divergence that slipped
  # every Bun gate. They are ADDITIVE (weaken no existing gate, R5). Registered
  # PROGRESS, not HARD, on purpose: they are brand-new, fail-closed against
  # hand-maintained audit ledgers (slice-reinterpret.tsv, capacity-saturation.tsv,
  # VT_INVARIANTS.md, stale-comments-D9.tsv, …), so a legitimate new hit that
  # lands one commit before its ledger row would flip a HARD gate RED and make CI
  # transiently un-passable. PROGRESS keeps R5's "never un-passable" contract
  # while the detectors bed in; they are proven GREEN on today's tree (each
  # ships a --self-test proving the RED flip). Promote a detector to HARD once it
  # has ridden the port clean for a while. D8/D9 are advisory by construction
  # (exit 0 + a review list); the fail-closed ones (D1/D2/D4/D6) still surface a
  # real RED — and in ci.sh's run_gate a PROGRESS RED (non-0/non-3) is still a
  # HARD_FAILURE, so a genuine regression is never silently swallowed.
  "D1|PROGRESS|bash $GATES/check_slice_reinterpret.sh"
  "D2|PROGRESS|bash $GATES/check_capacity_saturation.sh"
  "D3|PROGRESS|$PY $GATES/check_constants_parity.py"
  # G-LZ4 — the LZ4 encoder BYTE STREAM matches the oracle's, not merely
  # round-trips. Sits next to D3 because it extends D3's subject matter from
  # derived constants to the emitted bytes. `lz4_differential_light` is a real
  # guard for format conformance and every decoder bounds/margin path (4 of 7
  # injected faults caught, measured in assessment B §2.3) but it is BLIND to
  # match-finder heuristics: HASH_LOG 12->11 diverges 1,631/7,976 encoded blocks
  # with the whole rustty suite green (reproduced at P3-W0 T1 step 1).
  # Upstream's compress/AGENTS.md mandates exactly this proof; it is now ported
  # to crates/rustty-vt/src/compress/AGENTS.md. PROGRESS while it beds in — it
  # exits 3 BLOCKED without the oracle checkout or a zig toolchain, and
  # run_gate's PROGRESS-went-RED branch still escalates a genuine mismatch. Its
  # FAIL arm is proven by `--self-test` (exactly 1,631 diverging rows).
  "G-LZ4|PROGRESS|bash $GATES/check_lz4_byte_exact.sh"
  # G-THEMES (P3-W9 T5) — the vendored iterm2 theme corpus vs a committed
  # per-file SHA-256 manifest. `xtask/src/res_themes.rs` asserts the emitted
  # count is exactly 602 (20260810-152212-0173c3c re-vendor, phase-4
  # w8-themes), which catches an add/delete and NOTHING else: content
  # drift inside an existing theme is invisible to a count, and exactly ONE
  # theme body is ever read by a test. Not hypothetical — the 20260720 re-vendor
  # changed the CONTENT of four `Catppuccin *` files with the count unmoved.
  # Its FAIL arm is executed, not asserted: flipping one byte in `0x96f` turned
  # this RED while `emits_full_theme_corpus` stayed green (P3-W9 T5 step 3), and
  # tampering with one manifest row trips the total-digest self-consistency arm.
  # HARD: pure stdlib Python over in-repo files, no oracle and no toolchain, so
  # it cannot legitimately BLOCK.
  "G-THEMES|HARD|$PY $GATES/check_theme_corpus.py"
  "D4|PROGRESS|bash $GATES/check_comptime_phase.sh"
  "D5|PROGRESS|bash $GATES/check_paired_invariants.sh"
  "D6|PROGRESS|bash $GATES/check_sanitizer_canary.sh"
  "D8|PROGRESS|bash $GATES/check_clone_injection.sh"
  "D9|PROGRESS|bash $GATES/check_stale_comments.sh"
  # D10 — the vendored stb_image_resize glyph resampler vs ghostty's OWN build of the
  # byte-identical header, at all four Zig optimize modes, plus a negative control
  # proving the +/-1-on-3% envelope still rejects nearest/bilinear. PROGRESS because
  # it reports BLOCKED (exit 3) on a box without `zig` or the oracle checkout; a
  # genuine mismatch is still escalated by run_gate's PROGRESS-went-RED branch.
  "D10|PROGRESS|bash $GATES/check_stb_resize_parity.sh"
  # D11 — no CLOSED #[repr(C)] enum may be received BY VALUE across the C ABI.
  # An out-of-range tag is materialized by the callee's prologue, before any
  # `match` can narrow it; every flagged enum's header declaration carries a
  # `…_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` enumerator, so the header itself
  # documents that value as writable by a conforming C caller.
  # (P3-W0 T3 calls this "D10" in prose; that id was taken by the stb gate
  # above, so it is D11.)
  # RED ON ARRIVAL by design — 33 sites across 27 exports, including
  # rustty_terminal_scroll_viewport(behavior.tag), whose ScrollViewportTag
  # gained `Row = 3` in 3a2e28329 inside the Phase 3 range. A gate that lands
  # green is unproven; this one is provably non-vacuous the day it lands.
  #
  # HARD as of P3-W6 T2, with the gate's ALLOWLIST EMPTY. All 33 sites are
  # closed: the C parameter (or by-value struct field) is the C integer type and
  # every one has an explicit `from_c_int` narrowing point, the shape
  # crates/rustty-vt-c/src/modes.rs:26 `ReportState(pub c_int)` established. A
  # `const _: () = assert!(size_of::<E>() == size_of::<c_int>())` sits beside
  # each narrowed enum, so the published ABI is provably unchanged. Because the
  # allowlist is empty AND a stale allowlist row is itself a FAIL, this gate can
  # now only ever move in one direction.
  "D11|HARD|bash $GATES/check_closed_enum_abi.sh"
  # D11r — the RUNTIME half of D11 (P3-W6 T2 Step 4). D11 is a SOURCE scanner and
  # is satisfied by the parameter TYPE alone, so it cannot tell a narrowing point
  # that REJECTS an undeclared value from one that silently clamps it. D11r
  # compiles a real C program against the published header + shipped dylib,
  # passes the `…_MAX_VALUE` enumerator the header itself declares, and asserts a
  # DEFINED result comes back — plus that the in-range enumerators are unchanged.
  # PROVEN NON-REDUNDANT: replacing one `from_c_int`'s `_ => None` with a clamp
  # leaves D11 GREEN and turns D11r RED (2 checks). That test could not be
  # written correctly before T2: the parameter was the closed enum, so the call
  # was UB and there was no defined answer to assert.
  "D11r|HARD|sh $GATES/check_closed_enum_abi_runtime.sh"
  # A3 — the rustty-embed.h product-umbrella ABI gate (rebrand Task 4; formerly
  # the ghostty.h byte-freeze). Asserts include/rustty-embed.h is EXACTLY
  # ghostty's upstream include/ghostty.h (read from the pinned $ORACLE_HEADER_PATH
  # checkout + a SHA-256 freeze-pin; no longer vendored locally) with every
  # ghostty_* token renamed to rustty_* per CABI_SYMBOLS.tsv's proven name-map —
  # nothing added, dropped, or left un-renamed. PROGRESS not HARD (R5): a genuine
  # mismatch/leftover RED is still escalated to a HARD failure by run_gate's
  # PROGRESS-went-RED branch.
  "A3|PROGRESS|bash $GATES/check_rustty_h_abi.sh"
  # A4 — the rustty-embed.h ABI-SIZE gate (companion to A3's name gate). Clang-
  # probes every umbrella-header aggregate's C sizeof and HARD-requires every
  # by-value-return type (the AAPCS64 sret crash class) to carry a Rust
  # `@embed-abi` size-freeze that matches. Built after rustty_config_trigger
  # shipped a 20-byte TriggerC vs the header's 12 → sret crash → blank menu
  # shortcuts (MACOS_UI_PARITY_GAPS.md). PROGRESS: a coverage gap or size drift
  # RED is escalated to HARD by run_gate's PROGRESS-went-RED branch.
  "A4|PROGRESS|bash $GATES/check_embed_h_abi.sh"
  # A5 — the rustty/vt.h family struct-layout ABI gate. Cross-checks every
  # cbindgen/curated vt struct's sizeof + field OFFSETS against the Rust
  # `#[repr(C)]` layout via `rustty_type_json()` (the vt surface's answer to the
  # embedding A4/abi_layout freeze). Self-builds the vt-c dylib+wasm. It existed
  # but was never wired into this runner; registered here so the vt C-ABI is
  # continuously layout-gated (the vt header declares no by-value struct returns,
  # so this covers field-offset drift rather than the A4 sret crash class).
  "A5|PROGRESS|sh $GATES/check_rustty_vt_headers.sh"
  # G-VTFEAT (gap-close 2026-08-26, oracle 1fdbb8c91) — the `-Dvt-features`
  # trimming gate. The reference lets a LIB artifact drop optional features so
  # embedders can shrink the binary, and each feature gates a block of
  # `@export`s in lib_vt.zig. rustty spells it as nine cargo features on
  # rustty-vt + rustty-vt-c, all named in `default` (an additive feature set
  # equals the reference's subtract-from-all model only if the default is the
  # full set — arm 1 asserts exactly that, because a feature silently leaving
  # `default` would quietly cut C API surface from the whole application tier).
  # Arms 2-3 build all eleven configurations (each feature alone, -all, and
  # default) and require each to compile CLEANLY and to export EXACTLY the
  # symbol set derived from the oracle's lib_vt.zig — so a missed `#[cfg]`
  # surfaces as an untrimmed symbol and an over-eager one as a missing symbol.
  # Deliberately NOT run: the test suite under a trimmed build. Upstream cannot
  # do that either (`Config.terminalOptions` gives `.ghostty` every feature and
  # only `.lib` gets `vt_features`), so trimming is a build configuration, not
  # a test configuration. PROGRESS on first landing per the bedding-in
  # convention; a genuine drift RED is escalated by run_gate's
  # PROGRESS-went-RED branch.
  "G-VTFEAT|PROGRESS|python3 $GATES/check_vt_features.py"
  # G-ABISCHEMA (P4-W5 Bundle T, oracle c75559589) — the librustty-vt ABI type
  # manifest (`rustty_type_json`) validates against its checked-in JSON Schema
  # (`crates/rustty-vt-c/types.schema.json`) via the ported
  # `types-schema-verify.py` (the oracle's `test-lib-vt-schema` build step).
  # The gate builds the ISOLATED `-p rustty-vt-c` dylib (G-VTCLIB publish rule)
  # and ctypes-loads it, so the validated bytes are the SHIPPED manifest, not a
  # fixture. FAIL arm proven by a NEGATIVE CONTROL that runs on EVERY
  # invocation: a deliberately broken local schema copy (`"schema"` const
  # flipped 1 -> 2) must be rejected or the gate goes RED — executed at
  # landing (broken copy failed with `/schema: 2 was expected`; the real
  # schema validated 134 types). PROGRESS on first landing per the D1-D9
  # bedding-in convention; a genuine schema regression is still escalated by
  # run_gate's PROGRESS-went-RED branch. Exits 3 BLOCKED (never green) when no
  # python with jsonschema exists and the cached venv cannot be provisioned.
  "G-ABISCHEMA|PROGRESS|bash $GATES/check_abi_schema.sh"
  # G-KAITAI (phase-4 wbench, oracle 38d92c50c) — the snapshot wire format's
  # normative Kaitai spec (crates/rustty-vt/src/snapshot/snapshot.ksy) parses
  # every checked-in golden fixture, byte-accounted and CRC32C-checked, via the
  # ported verify-kaitai.py living beside the .ksy (the oracle runs the same
  # script CI-side). Anti-vacuity: the success banner's fixture count must
  # equal the on-disk *.hex golden count, and the count arm's FAIL side is
  # proven by the script's --self-test. Exits 3 BLOCKED (never a silent green
  # — the check_test_counts.py oracle-absent precedent) when
  # kaitai-struct-compiler or a python with kaitaistruct is unavailable; the
  # python side self-provisions a cached venv (check_abi_schema.sh precedent).
  # PROGRESS per the bedding-in convention; a genuine spec/golden drift RED is
  # escalated by run_gate's PROGRESS-went-RED branch.
  "G-KAITAI|PROGRESS|bash $GATES/check_kaitai_snapshot.sh"
)

HARD_FAILURES=()   # hard-fail checks that failed, or a claimed-done regression
WARNINGS=()        # BLOCKED gates (pending contract/tooling)
PROGRESS_NOTES=()  # progress gates reporting progress (exit 0 while unported)
PASSED=()          # checks that passed cleanly

hr() { printf '%s\n' "------------------------------------------------------------------"; }
section() { hr; printf '### %s\n' "$1"; hr; }

# Run a HARD-FAIL cargo step. Records failure; never aborts the script.
run_cargo() {
  local label="$1"; shift
  section "$label  (HARD-FAIL)"
  if [ "${CI_SKIP_CARGO:-0}" = "1" ]; then
    echo "SKIPPED (CI_SKIP_CARGO=1): $label"
    PROGRESS_NOTES+=("$label (skipped via CI_SKIP_CARGO)")
    return 0
  fi
  if "$@"; then
    echo ">> PASS: $label"; PASSED+=("$label")
  else
    local rc=$?; echo ">> FAIL ($rc): $label" >&2; HARD_FAILURES+=("$label (exit $rc)")
  fi
}

# Run one registry gate, interpreting its exit code per classification.
run_gate() {
  local id="$1" kind="$2"; shift 2
  if skip_in_fast "$id"; then
    echo "### gate $id  [SKIPPED by --fast]"
    SKIPPED_GATES+=("$id")
    return 0
  fi
  section "gate $id  [$kind]  ->  $*"
  "$@"; local rc=$?
  if [ "$rc" -eq 0 ]; then
    echo ">> PASS: $id"
    if [ "$kind" = "PROGRESS" ]; then PROGRESS_NOTES+=("$id (progress/green)"); else PASSED+=("$id"); fi
  elif [ "$rc" -eq 3 ]; then
    echo ">> BLOCKED ($rc): $id [pending contract/tooling — surfaced, not greened]" >&2
    WARNINGS+=("$id BLOCKED (exit 3)")
  else
    # Non-zero, non-BLOCKED => a real RED.
    if [ "$kind" = "HARD" ]; then
      echo ">> FAIL ($rc): $id [HARD-FAIL gate]" >&2
      HARD_FAILURES+=("$id (exit $rc)")
    else
      echo ">> FAIL ($rc): $id [PROGRESS gate went RED — a claimed-done crate regressed; R5]" >&2
      HARD_FAILURES+=("$id (claimed-done regression, exit $rc)")
    fi
  fi
}

echo "=================================================================="
echo " rustty CI — single entry (.port/tools/ci.sh)"
echo " root: $ROOT"
echo " toolchain: $( (rustc --version 2>/dev/null) || echo 'rustc: n/a')"
echo "=================================================================="

# ---- opt-in lane dispatch (exits; the default run below is not performed) ---
if [ "$WEBGL_LANE" = "1" ]; then
  run_webgl_lane; rc=$?
  hr
  case "$rc" in
    0) echo "CI RESULT (--webgl lane): PASS"; ;;
    3) echo "CI RESULT (--webgl lane): BLOCKED — environment missing, NOT green" >&2 ;;
    *) echo "CI RESULT (--webgl lane): FAIL (exit $rc)" >&2 ;;
  esac
  exit "$rc"
fi
if [ "$LZ4_SLOW_LANE" = "1" ]; then
  run_lz4_slow_lane; rc=$?
  hr
  case "$rc" in
    0) echo "CI RESULT (--lz4-slow lane): PASS"; ;;
    *) echo "CI RESULT (--lz4-slow lane): FAIL (exit $rc)" >&2 ;;
  esac
  exit "$rc"
fi

# ---- 1..4: cargo steps (all HARD-FAIL) ------------------------------------
run_cargo "cargo fmt --check"        cargo fmt --check
run_cargo "cargo clippy -D warnings" cargo clippy --workspace --all-targets -- -D warnings
run_cargo "cargo build --workspace"  cargo build --workspace
# WS19 T1 (N5): --no-fail-fast. Bare `cargo test --workspace` STOPS at the first failing
# test target, so every crate ordered after it is never run and its failures are MASKED.
# That is how `rustty-extra`'s golden failures hid behind `rustty-apprt-c --test smoke`.
# `check_release_lane.sh:155` is the precedent (it already runs --no-fail-fast).
# NOTE: the `cargo build --workspace` step immediately above already pays the cold-target/
# compile cost, so the "one early failure also wastes the uplift" half of that symptom
# does not bite INSIDE ci.sh — only outside it, which is what WS19 T16 addresses for the
# example/*/build.sh scripts.
run_cargo "cargo test --workspace"   cargo test --workspace --no-fail-fast

# ---- 5: parity gates (HARD first, then PROGRESS) ---------------------------
for phase in HARD PROGRESS; do
  for entry in "${GATE_REGISTRY[@]}"; do
    IFS='|' read -r gid gkind grun <<< "$entry"
    [ "$gkind" = "$phase" ] || continue
    # grun is an intentional word-split command line.
    # shellcheck disable=SC2086
    run_gate "$gid" "$gkind" $grun
  done
done

# ---- summary ---------------------------------------------------------------
section "CI SUMMARY"
echo "PASSED (${#PASSED[@]}):";          for x in "${PASSED[@]:-}";        do [ -n "$x" ] && echo "  + $x"; done
echo "PROGRESS — not failing CI (${#PROGRESS_NOTES[@]}):"; for x in "${PROGRESS_NOTES[@]:-}"; do [ -n "$x" ] && echo "  ~ $x"; done
echo "WARNINGS — BLOCKED (pending contract/tooling) (${#WARNINGS[@]}):";   for x in "${WARNINGS[@]:-}";     do [ -n "$x" ] && echo "  ! $x"; done
echo "HARD FAILURES (${#HARD_FAILURES[@]}):"; for x in "${HARD_FAILURES[@]:-}"; do [ -n "$x" ] && echo "  X $x"; done

hr
if [ "${#HARD_FAILURES[@]}" -ne 0 ]; then
  echo "CI RESULT: FAIL  (${#HARD_FAILURES[@]} non-progress check(s) red)" >&2
  exit 1
fi

if [ "$FAST_LANE" -eq 1 ]; then
  # ANTI-VACUITY: a subset that prints the full-pass banner is exactly how a
  # green comes to mean nothing. Name what was skipped and exit on a DISTINCT
  # code so no caller can mistake this for a release verdict.
  echo "CI RESULT: FAST-SUBSET GREEN — *** NOT A RELEASE GATE ***"
  echo "  ${#SKIPPED_GATES[@]} expensive gate(s) were SKIPPED by --fast:"
  for x in "${SKIPPED_GATES[@]:-}"; do [ -n "$x" ] && echo "    - $x"; done
  echo "  These are the breadth/regression gates (Miri leak lanes, differential,"
  echo "  fuzz, perf fences). A lane may ship on this result; INTEGRATION may not."
  echo "  Run the full suite (no --fast) once per merge batch before declaring green."
  exit 4
fi

echo "CI RESULT: PASS  (all hard-fail checks green; progress gates printed"
echo "           progress without failing CI — R5: gates invariant, never weakened)"
exit 0
