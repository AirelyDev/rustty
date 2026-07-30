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
#   G8  check_baseline_regression.py no perf/mem/size/startup regress-> PROGRESS*
#   G5  run_fuzz_smoke.sh            differential fuzz clean         -> PROGRESS*
#   G6  check_leaks.sh               leak-free (tracking-alloc/Miri) -> PROGRESS*
#   G6b check_pagelist_compress_leaks.sh
#                                    PageList compressed-page encodings are not
#                                    leaked (global-allocator Vec that no other
#                                    memory gate can see)              -> HARD
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
# Usage:
#   bash .port/tools/ci.sh                   # full run (what CI runs)
#   CI_SKIP_CARGO=1 bash .port/tools/ci.sh   # gates only (no cargo; local/fast)
set -uo pipefail   # NOTE: no `-e` — run every check, aggregate failures.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
GATES="$ROOT/.port/tools/gates"
PY="${PYTHON:-python3}"

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
  "G-P2|HARD|$PY $GATES/check_phase2_closure.py"
  "G1|PROGRESS|$PY $GATES/check_tests_ported.py"
  "G1b|PROGRESS|run_test_counts"
  "G2|PROGRESS|$PY $GATES/check_assertions.py"
  "G3|PROGRESS|$PY $GATES/check_comments_fidelity.py"
  "G3b|PROGRESS|$PY $GATES/check_comment_coverage.py"
  "G9|PROGRESS|$PY $GATES/check_parity_surface.py"
  "G8|PROGRESS|$PY $GATES/check_baseline_regression.py"
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
    # shellcheck disable=SC2086 — grun is an intentional word-split command line.
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
if [ "${#HARD_FAILURES[@]}" -eq 0 ]; then
  echo "CI RESULT: PASS  (all hard-fail checks green; progress gates printed"
  echo "           progress without failing CI — R5: gates invariant, never weakened)"
  exit 0
else
  echo "CI RESULT: FAIL  (${#HARD_FAILURES[@]} non-progress check(s) red)" >&2
  exit 1
fi
