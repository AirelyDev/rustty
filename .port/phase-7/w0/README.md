# Phase-7 pin advance — gate evidence (2026-09-28)

Oracle advanced `20abdb50a` → `12752b2ac` (= `origin/main`) at Phase-7 start (`P7-W0 T3`).
Range: 333 commits, 234 non-merge, 233 files, +35,723 / −9,235 (`../UPSTREAM_*`).

Preconditions: the only worktrees were the main checkout (`main`, `284d59d6`) and this phase
worktree (`phase-7-upstream-resync`, clean at `5200d7af`, after `P7-W0 T1`/`T2`). Oracle clean at
`20abdb50a6216c450d6d4d010c41c7edf5ab15b2`; `zig version` 0.16.0.

After the move: the oracle `zig-out/include` was rebuilt with `zig build -Demit-lib-vt=true`
(installed headers byte-equal to the oracle's `include/ghostty/`; no `mouse/shape.h`), and
`.port/phase-0/harness/build_ref_driver.sh` rebuilt `bin/vt_driver_ref` against it.
`_oracle_root.sh --self-test` GREEN (`g-oracle-sh-self-after-at-12752b2ac.txt`).

## Before / after

Script names used (the brief's gate names → the scripts `ci.sh` registers):
G-TESTS `audit_test_coverage.py --limit 500`, G-FILES `audit_file_coverage.py --limit 500`,
G-SYMS `audit_symbol_coverage.py --limit 500`, G-REFGEOM `check_ref_item_geometry.py`,
A5 `sh check_rustty_vt_headers.sh`.

| gate | before at `20abdb50a` | after at `12752b2ac` | expected (R5 §3) | match |
|---|---|---|---|---|
| G1b | GREEN | RED, 1 row: `input_key_encode` zig 104 / rust 96, drop 8 | RED, 1 row | yes |
| G3b (default) | GREEN, 0 missing | RED, 610 missing: vt 437, vt-c 60, core 29, bench 27, os 17, config 13, font 12, apprt-api 5, lib 5, termio 5; **plus 8 NOT-CARRIED-INVALID** | RED, 610 missing, NOT-CARRIED green | **no** (F1) |
| G3b `--strict` | RED: renderer 2 missing | RED: apprt 55, bin 5, renderer 100 (plus the default set) | not stated | pre-existing RED (F2) |
| G-TESTS | GREEN, 4085 blocks, 0 unexplained | RED, 4211 blocks, 94 unexplained | 4211 / 94 | yes |
| G-FILES | GREEN, 545 files | RED, 557 files, 8 MISSING | 557 / 8 | yes |
| G-SYMS | GREEN, 2845 `pub fn` | RED, 2922, 57 unexplained | 2922 / 57 | yes |
| G-CENSUS | GREEN (4288 / 17127 / 348) | RED at arrival (`g-census-arrival-at-12752b2ac.txt`), GREEN after the ledger moved to 4397 / 17717 / 357 | GREEN with the new delta | yes |
| G-LOGFILE | GREEN | RED, 6 arms: 4 undeclared files (16 sites), 4 dead rows, 36 absent rows, 1 absent row excusing a live emission, 14 files short, 7 N20 comments | RED, 6 arms, same counts | yes |
| G-REFGEOM | GREEN | GREEN (36 lines, 3 targets) | GREEN (by reading) | yes |
| A3 | GREEN | GREEN after the `oracle-ghostty-h.sha256` bump to `78232aec…` | GREEN | yes |
| A5 | RED: (c) registry lacks `GhosttyMouseShape`, `GhosttyRenderStateOverscan`, `GhosttyRenderStateRowId` | same RED | RED already (DECISIONS D1) | yes, pre-existing |
| G9 | GREEN 677/677 (frozen `4749c4e93` surface) | RED 692/693: `UNCOVERED config: vt-window-resize-allowed` | "unaffected" (R5); DECISIONS R10 regenerates | **no** (F3) |

## Findings

- **F1 — 8 `COMMENT-NOT-CARRIED.tsv` rows go stale at the new pin** (R5 §3 simulated
  `--not-carried-check` GREEN). Rows 40–45 (`lib_vt.zig:505/508/509/510/511/513`, the
  `std.Options.debug_io` block moved), row 75 (`apprt/gtk/class/application.zig:3486`, out of
  range) and row 76 (`apprt/gtk/winproto/x11.zig:486`). They must be re-anchored with the G3b
  work; G3b stays RED on them even after the 610 are ported.
- **F2 — G3b `--strict` renderer was already RED before the advance**: 2 missing sentences
  from `renderer/generic.zig:1614` ("We hold a the draw mutex to prevent changes to any / data
  we access while we're in the middle of drawing."). `P7-W0 T1` dropped it: the comment is at
  `crates/rustty-renderer/src/generic.rs:6331` in `284d59d6` and absent at `5200d7af`
  (`89d327ee`/`d8f53fd7` rewrote that block). Renderer is outside the default G3b scope (PORT
  STATUS not done), so no registered gate saw it.
- **F3 — G9 goes RED after `parity-surface.json` is regenerated** (DECISIONS R10). The surface
  grows 677 → 693 ids (config 209 → 216, keybind 85 → 88, cli 18 → 19, terminfo 268 → 270, modes
  41 → 42, osc 25 → 27); this is cumulative since the phase-0 pin `4749c4e93`, not only this
  range. Only `vt-window-resize-allowed` (`714fe9b90`/`219cca932`/`2a987d59d`) is uncovered; it
  closes with that port. The apprt action `resize_window` is not a keybind action, so G9 still
  does not see it (audit by hand, as R5 says). The generator hard-coded the phase-0
  `reference_rev`/`zig_version`; it now reads them from the parsed tree, so the regenerated
  `_meta` says `12752b2ac…` / `0.16.0`.
- **F4 — G4 canonical serialization does not include page capacity (DECISIONS R11).** Leg A runs
  `vt_driver_ref`/`vt_driver_port` at 80×24, `--scrollback 0`, `--format plain --trim`, and
  compares the formatter's text output (`run_differential.sh`, `comparator.py`,
  `ref_driver/vt_driver.c`). Page capacity and `Page.layout` never appear in it, and with
  scrollback 0 no fixture spans pages. So `d2ff6d77a` is not expected to turn G4 red, and G4 is
  blind to the capacity change: N88 and the UD64/UD80/UD131 constants must be re-derived by hand.
  G4 itself was not run (it needs the port driver built; long runs are deferred).
- **F5 — R5's G1b slack figure for `kitty` is off:** after is zig 351 / rust 381 (R5 said 345).
  The `kitty` MAP row globs `kitty/*.zig`, so it already counts the NEW `kitty/windows.zig`,
  contrary to R5's list of uncovered test files. Still GREEN by slack.
- **F6 — the reference driver is per-checkout.** `.port/phase-0/harness/bin/` is ignored, so the
  rebuild landed only in this worktree. The main checkout's `bin/vt_driver_ref` dates from
  2026-08-10; rebuild it there before G4 runs from the main checkout.

## Files

| file | what |
|---|---|
| `*-before-at-20abdb50a.txt` | G1b, G3b, G3b `--strict`, G-TESTS, G-FILES, G-SYMS, G-CENSUS, G-LOGFILE, G-REFGEOM, A3, A5 |
| `*-after-at-12752b2ac.txt` | the same set, plus G9 and G-ORACLE-SH-SELF |
| `g-census-arrival-at-12752b2ac.txt` | G-CENSUS at the new pin before `CENSUS_DELTA.md` moved (RED by design) |
| `g9-before-regen-at-12752b2ac.txt` | G9 at the new pin against the old frozen surface (GREEN 677/677) |
