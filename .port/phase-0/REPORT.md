# Phase-0 Report — rustty parity port

**Date:** 2026-06-21
**Reference oracle:** /Users/gael/projects/ghostty (READ-ONLY), pinned 4749c4e93731067049bfbf2e4572061cef2bdd17, Zig 0.15.2.
**Verified clean after all work:** ghostty HEAD unchanged, git status empty.

This report is the adversarial-completeness audit of every Phase-0 deliverable. Numbers were independently re-derived by the critic, not copied from prior agents.

---

## VERDICT: GREEN — blocker B1 RESOLVED (2026-06-21, main loop)

> **B1 resolution stamp:** the test-count contract was corrected from the wrong
> **3189** to the authoritative **3234** in `plan/DEFINITION_OF_PARITY.md` (G1
> pass condition, floor, summary table) and `build-summary.md` (with a note
> recording the earlier undercount). References to 3189 elsewhere in this file and
> in `report.json` are retained because they correctly *document* the blocker.
> Phase-0 gate is now fully GREEN with no open blockers.

## VERDICT: GREEN (with one mandatory documentation-fix blocker) [original]

The four hard GATE conditions are all satisfied:

1. Full reference test suite passed — `zig build test` exit 0, no FAIL/panic/leak lines.
2. Test-count contract: critic re-derivation == extractor count == 3234 (exact match).
3. BASELINE.json populated with real numbers (binary sizes + 28 throughput benchmarks).
4. Diff harness proved 100.0% divergence vs the empty workspace (control 0.0%), re-run live by the critic.

One blocker is recorded against deliverable text (not the data): two deliverables froze the test-count contract at the WRONG number 3189 instead of the correct 3234. Must be corrected before the contract gates Phase-1, because under a literal `== 3189` reading a correct 3234-test port would FALSELY FAIL G1. See "Blocker".

---

## What was produced (by deliverable)

### Build / reference test run
- test-run.log — `zig build test`, EXIT_CODE=0. The 17 grep hits for fail/error/panic are all (warn) strings from error-path tests (verified); zero genuine failures.
- test-lib-vt.log — `zig build test-lib-vt`, exit 0.
- libghostty-vt artifacts built (.a 12,065,920 B; .0.1.0.dylib 5,742,200 B; xcframework macOS/iOS); bench runner + 21 c-vt/cpp-vt examples built.
- build-summary.md.

### Inventory (inventory/)
- tests.jsonl + tests-summary.json (authoritative).
  - Total test{} blocks: 3234 (named 3158 = 3113 string-named + 45 identifier-named; anonymous 76).
  - Files with tests: 279.
  - Assertions: 11,975 (expectEqual 5971, expect 4014, expectEqualStrings 1558, expectError 274, expectEqualSlices 105, expectApproxEqAbs 29, expectEqualDeep 20, debug.assert 3, expectStringStartsWith 1).
  - Per-file sum == 3234; jsonl re-tally == summary; assertion sum == 11975. All internally consistent.
- comments-census.json (R1 fidelity baseline).
  - Files scanned: 512 (entire src/**.zig; 491 with comments) — superset of the 279 test files.
  - Total comment lines: 42,069 (doc-module //! 1006, doc-decl /// 18268, line // 22795).
  - TODO/FIXME 109, NOTE-family 109, spec/citation 244, ASCII-diagram 446, license-header files 4.
- parity-surface.json — config 203 public / 209 total; keybind actions 85; CLI actions 18; terminfo caps 268 (3 entry names); modes 41; SGR 31; OSC 25; CSI 37; ESC 23; DCS 3.

### Baseline (baseline/BASELINE.json)
- Apple M4 Pro, Zig 0.15.2, ghostty 4749c4e.
- Binary sizes (real): static lib 12,065,920 B; dylib 5,742,200 B; bench Debug 1,628,128 / ReleaseFast 1,553,824; full test exe 102,264,336 B.
- 28 benchmark entries, each with raw per-rep timings + throughput (terminal-parser/utf8 249.83 MiB/s n=7; osc-parser 1160.22 MiB/s; terminal-stream/ascii 118.77 MiB/s; screen-clone.clone 20264 clones/s).
- Startup proxy (no-GUI) median ~9.3 ms.
- Documented unmeasured TODOs (with reasons): GUI +version startup, GUI frame timing, peak RSS — GUI binary not built in build stage. Justified gaps, not fabricated.

### Corpora (corpora/) — 33 golden artifacts
- 21 c-vt/cpp-vt example goldens incl. cvt/c-vt-formatter.golden (184 B, the VT formatter golden) — exists, non-empty.
- 6 terminfo goldens, 6 config goldens.
- MANIFEST.md (per-golden source + reproduce command; 26 reproduce refs) + MANIFEST.sha256.

### Harness (harness/)
- DESIGN.md, FUZZ-INVENTORY.md.
- ref_driver/ + bin/vt_driver_ref (statically linked vs reference libghostty-vt.a — real working driver).
- comparator.py, diff_vt.sh, port_hook/vt_driver_port.sh (empty stub).
- gate_empty_divergence.sh — re-run LIVE by critic: 11 fixtures, empty-divergence 100.0%, control 0.0%, exit 0, pass=true. The 0% control proves the metric is sound.

### Plan (plan/)
- PORT_PLAN.md — 29-crate strict topological order (util … vt … rustty(bin) … synthetic); machine-checked topo validity; A1 render-types/vt resolution documented. Uses CORRECT number 3234.
- DEFINITION_OF_PARITY.md — gates G1–G9, each with explicit check command (49 command refs). Uses WRONG number 3189 in G1 — see blocker.

---

## Audit findings (per kickoff checklist)

1. Re-derive test-block count vs extractor: MATCH — critic 3234 == extractor 3234 (raw grep 3234; independent lexical scanner 3234). Build/DoP's 3189 is a 45-test undercount -> blocker on those docs, not on the inventory.
2. Extractor edge cases: PASS — `// test {` in shaders.zig correctly excluded; 3234 bodies captured with balanced-brace matching; multi-line + conditional tests counted.
3. Comment census coverage: PASS — 512/512 src files scanned (superset of 279 test files).
4. Golden MANIFEST reproduce cmds + VT formatter golden: PASS.
5. Diff harness gate ran ~100% divergence: PASS (re-run live: 100.0% empty, 0.0% control, exit 0).
6. BASELINE.json real numbers: PASS (real sizes + 28 throughput benchmarks).
7. PORT_PLAN valid topo sort + DoP check command per gate: PASS (29-crate topo sort machine-checked; all 9 gates have commands).
8. Full zig build test passed: PASS (exit 0, no real failures).

---

## BLOCKER

B1 — Test-count contract frozen at the WRONG number (3189) in two deliverables.

- Authoritative inventory (tests-summary.json, tests.jsonl), critic's independent lexical re-derivation, and raw grep all agree: 3234 total test{} blocks (76 anonymous + 3113 string-named + 45 identifier-named).
- build-summary.md / BUILD reported 3189, and plan/DEFINITION_OF_PARITY.md froze G1's pass condition at `== 3189`. 3189 = 3113 + 76 — it OMITS the 45 identifier-named `test name {` blocks.
- Why a blocker: G1 is the literal no-loss test contract. Read as `== 3189`, a correct 3234-test port FAILS the gate; read as `>= 3189`, a port missing up to 45 reference tests PASSES. Either way the contract is wrong and violates R1 (any delta is a blocker).
- Fix: change the G1 contract in DEFINITION_OF_PARITY.md and the headline in build-summary.md to 3234. tests-summary.json per-file table is already correct; PORT_PLAN.md already uses 3234.

No other blockers. All underlying Phase-0 data is correct and internally consistent; B1 is a number frozen in prose contradicting the agents' own authoritative data.

---

## Reference integrity
ghostty HEAD = 4749c4e93731067049bfbf2e4572061cef2bdd17, working tree clean, never modified.
