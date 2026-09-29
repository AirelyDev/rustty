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

## Phase-start records and gate maps (`P7-W0 T4`)

### Gate maps (Step 1: negative controls)

- **G1b MAP rows** (`check_test_counts.py`): `datastruct_untouched_pool`,
  `datastruct_comptime_int_set`, `datastruct_wasm_page_pool`, `lib_parse_int`, `lib_types`,
  `apprt_gtk_scale`, `font_nerd_font_attributes`, `snapshot_terminal`; `zlib.zig` added to the
  `wuffs` row. A Rust file that does not exist yet is named by a one-file glob, so the row counts
  rust 0 instead of tripping the stale-mapping arm. G1b now reads (`g1b-after-t4-maps.txt`):

  | row | zig | rust | verdict |
  |---|---|---|---|
  | `input_key_encode` | 104 | 96 | RED (arrival) |
  | `datastruct_untouched_pool` | 5 | 0 | **RED** (negative control) |
  | `datastruct_comptime_int_set` | 5 | 0 | **RED** |
  | `lib_parse_int` | 1 | 0 | **RED** |
  | `lib_types` | 1 | 0 | **RED** (`String.init empty output`, `dd2edd760`) |
  | `apprt_gtk_scale` | 7 | 0 | **RED** |
  | `font_nerd_font_attributes` | 1 | 0 | **RED** (`Constraints lookup table`, `a3f3331d4`) |
  | `datastruct_wasm_page_pool` | 1 | 1 | ok |
  | `snapshot_terminal` | 12 | 14 | ok (by slack; 3 changed bodies, `R5-upstream-tests.tsv`) |
  | `wuffs` | 8 → 13 | 20 | ok (by slack; the 5 `zlib.zig` names are the meter) |

- **G3b `crate_map`** (`.port/phase-0/inventory/tests-summary.json`, `_amendments` row
  `P7-W0-T4`): `datastruct/{comptime_int_set,untouched_pool,wasm_page_pool}.zig`,
  `lib/parse_int.zig`, `apprt/gtk/scale.zig`, `terminal/kitty/windows.zig`,
  `terminal/snapshot/terminal.zig`, each with `test_blocks` 0 so G1's frozen 3234 contract does not
  move (G1 and G2 re-run GREEN, D3 PASS). The new files' missing comments:
  `untouched_pool` 55, `comptime_int_set` 36, `wasm_page_pool` 4, `parse_int` 10,
  `kitty/windows` 32, `snapshot/terminal` 1. `lib/types.zig` and `font/nerd_font_attributes.zig`
  were already mapped. Not added (outside this task's list; see the task report):
  `apprt/gtk/class/{Overrides,render_surface}.zig`, `font/nerd_font_tables.zig`,
  `renderer/Dmabuf.zig`.

### F1 resolved

The 8 stale `COMMENT-NOT-CARRIED.tsv` rows are re-anchored at `12752b2ac`: `lib_vt.zig`
505/508/509/510/511/513 → 506/509/510/511/512/514, `application.zig:3486` → `:3348`,
`x11.zig:486` → `:482`, with the gag-prose evidence record in `.port/phase-3/NON_PORTS.md` and the
phase-4 `std_options_debug_io` row updated. `--not-carried-check`: GREEN, 37 rows
(`g3b-not-carried-after-t4.txt`).

### F2 corrected: nothing to restore

The sentence F2 names ("We hold a the draw mutex to prevent changes to any data we access while
we're in the middle of drawing.") was deleted upstream by `72cf50855` itself, which replaced it
with "Everything that touches draw state happens under the draw mutex. …" (`generic.zig:1682-1685`
at `12752b2ac`). `P7-W0 T1` ported that end state (`generic.rs` `draw_frame_raw`). The pre-advance
`--strict` run compared a post-`72cf50855` port against the `20abdb50a` oracle, so the 2 missing
sentences were an artifact of the order (fix before advance). At `12752b2ac`, G3b `--strict
--crate renderer --ref-files renderer/generic.zig` lists no draw-mutex or display-link sentence as
missing (41 missing, all from other range commits). Restoring the old comment would contradict
the reference, so it was not restored.

### Records (Step 2)

- UD statuses (R5 §1.1, DECISIONS M1/R2): UD150 fixed upstream in `5ca09acf6` (docs row
  removed); UD140, UD141, UD31, UD40, UD200 partly fixed (docs rows reworded to the remaining
  difference where the entry has one); UD32 obsolete upstream.
- Re-anchors (DECISIONS D4): 76 `reference:` fields remapped from `20abdb50a` to `12752b2ac` by a
  content line map of each cited changed file, plus hand re-anchors of UD205 (pin anchors were 12
  lines off), UD206 (the pin text had the declaration and call swapped), UD250, UD300 (sibling
  text corrected), UD310, UD313, UD315 and UD343, and the seven status UDs. Entries whose cited
  code did not move stay stamped `at 20abdb50a`.
- NEW-FINDINGS **N435** (search `unreachable!()`, `06178eeaa`, P7-W5 T4), **N436** (search tick
  use-after-free, `b1c264163`, P7-W2 T1), **N437** (render-thread whole-`Renderer` references
  overlapping the main thread's raw sync draw, P7-W7 T2), all OPEN.
- NON_PORTS: phase-4 TinyIo row extended to `src/lib/tinyio/{posix,windows,test}.zig` with the
  reopen-trigger re-read; a phase-3 "Phase 7 start" section (the tinyio cross-reference that
  G-SYMS/G-FILES need, `passthrough.glsl`, `7adbb5160`, `7644e6d62`, translate-c/cImport, the
  Zig-package and Nix renderings, the GTK build cache, VOUCHED).
- LEDGER `oracle` column re-anchored for the 38 WAIVED rows of R5 §2.2 (34 moved; S37-F1, S46-F5,
  S94-F4, S207-F2 unchanged). The column has no pin stamp; these 38 now read at `12752b2ac`.
- Closure README "Still open": the stale N311 line dropped; N435-N437 listed.

### Gates after `P7-W0 T4`

| gate | arrival (`*-after-at-12752b2ac.txt`) | after T4 |
|---|---|---|
| G1b | RED, 1 row | RED, 7 rows (above; the 6 new rows are the honest meter) |
| G3b (default) | RED, 610 missing + 8 NOT-CARRIED-INVALID | RED, **748** missing, 0 NOT-CARRIED-INVALID: vt 470 (+33: `kitty/windows` 32, `snapshot/terminal` 1), datastruct 95 (new), vt-c 60, core 29, bench 27, os 17, lib 15 (+10: `parse_int`), config 13, font 12, apprt-api 5, termio 5 (`g3b-after-t4.txt`) |
| G3b `--not-carried-check` | RED (8 stale rows) | GREEN, 37 rows |
| G-TESTS | RED, 4211 blocks, 94 unexplained | RED, 4188 blocks, 76 unexplained (the 18 TinyIo gaps gone) |
| G-SYMS | RED, 2922, 57 unexplained | RED, 2890, 31 unexplained (the 26 TinyIo gaps gone) |
| G-FILES | RED, 557, 8 MISSING, 14 NON-PORT | RED, 557, 8 MISSING, 18 NON-PORT (`TinyIo.zig` + the 3 tinyio files) |
| G-UDDOCS | PASS (116 docs rows) | PASS (115 docs rows; UD150's removed) |
| G-PARITY | GREEN | GREEN, 497: 403 closed, 94 waived, 0 open |
| phase-2 closure | GREEN | GREEN, 97 open-claim entries |

Evidence files: `*-after-t4*.txt` in this directory.
