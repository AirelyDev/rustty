# Phase-8 pin advance — gate evidence (2026-10-07)

Oracle advanced `12752b2ac` → `a60e9e2a5` (= `origin/main`) at Phase-8 start (`P8-W0 T3`).
Range: 107 commits, 69 non-merge, 85 files, +7,466 / −497 (`../UPSTREAM_*`).

Preconditions: the only worktrees were the main checkout (`phase-8-upstream-resync`, `37f4ced8c`)
and this task's worktree (`phase-8/w0`, clean at `37f4ced8c`, after `P8-W0 T1`/`T2`). The brief's
"only the main checkout" predates the per-task worktree; R4 §7 step 1 allows the phase worktree.
Oracle clean at `12752b2ac1bb05ce53402ed8c853ed1f96eef0b1`; `zig version` 0.16.0; 58 GiB free.

After the move:

- the oracle `zig-out` was rebuilt with `zig build -Demit-lib-vt=true`; the installed headers are
  byte-equal to the oracle's `include/ghostty/`;
- `.port/phase-0/harness/build_ref_driver.sh` relinked `bin/vt_driver_ref` against it (per
  checkout: only this worktree's driver is fresh, Phase 7 F6);
- the c-vt goldens were re-captured (see F1 for how). Only `c-vt-effects` changed: it gains
  `Sending unknown OSC:` / `unknown OSC (truncated=no, terminator=BEL, content=16 bytes):
  7400;status=busy`, the R4 §5 G4 row. The other 22 are byte-identical, except `c-vt-paste`,
  whose OSC 5522 `pw=` token is random on every run (N395; leg B masks it): it is equal with
  the token masked, so its committed bytes were kept;
- `_oracle_root.sh` resolves the oracle to `/Users/gael/projects/ghostty` from this worktree;
  `_oracle_root.sh --self-test` GREEN (`g-oracle-sh-self-after-at-a60e9e2a5.txt`);
- rvm: `/home/gael/projects/ghostty` fetched (`12752b2ac..a60e9e2a5 main -> origin/main`) and
  detached at `a60e9e2a57f73e1eef2bd1cf2995a467f69e7fb0`; HEAD = `origin/main`, tree clean (N486).

## Before / after

Script names used: G-TESTS `audit_test_coverage.py --limit 500`, G-FILES
`audit_file_coverage.py --limit 500`, G-SYMS `audit_symbol_coverage.py --limit 500`, A5
`sh check_rustty_vt_headers.sh`.

| gate | before at `12752b2ac` | after at `a60e9e2a5` | expected (R4 §5) | match |
|---|---|---|---|---|
| G1b | GREEN | RED, 2 rows: `input_mouse_encode` zig 24 / rust 23, `cli_ssh` 13 / 12 | RED, 2 rows | yes |
| G3b (default) | GREEN | RED, 411 missing: vt 277, vt-c 62, config 32, cli 21, bench 8, datastruct 4, termio 4, core 3; plus 6 NOT-CARRIED-INVALID (rows 40–45, `lib_vt.zig`) | 411 + 6 | yes |
| G3b `--strict` | GREEN | RED, 433 missing (411 + renderer 11, bin 7, apprt 4) + the same 6 | 433 + 6 | yes |
| G-TESTS | GREEN, 4188 blocks, 0 unexplained | RED, 4277 blocks, 73 unexplained: program_status 15, unknown 12, xt_checksum 11, stream_terminal 7, Terminal 5, c/terminal 4, tmux/control 3, cli/ssh 3, stream 2, 1 each in 11 files | 4277 / 73, same split | yes |
| G-FILES | GREEN, 557 files | RED, 562, 6 MISSING: `lib/windows_dll.zig`, `renderer/{metal,opengl}/Device.zig`, `osc/parsers/{program_status,unknown}.zig`, `xt_checksum.zig` | 562 / 6, same files | yes |
| G-SYMS | GREEN, 2890 | RED, 2911, 8 unexplained: `DiskCache.splitHostPort`, `UntouchedPool.freeCount`, `WasmPagePool.freeCount`, `windows_dll.DllMain`, `PageList.memoryUsage`, `Screen.memoryUsage`, `Terminal.setDefaultXtChecksum`, `Terminal.rectXtChecksum` | 2911 / 8, same names | yes |
| G-CENSUS | GREEN (4397 / 17717 / 357) | RED at arrival (`g-census-arrival-at-a60e9e2a5.txt`: live 4487 / 18258 / 360); GREEN after `CENSUS_DELTA.md` moved | GREEN with the new delta | yes |
| G-LOGFILE | GREEN (1392 sites) | RED, 5 arms (176 files, 1406 sites): 3 undeclared files / 10 sites (`opengl/Device.zig` 6, `metal/Device.zig` 2, `program_status.zig` 2); 1 dead row `Termio.zig:717`; 19 `LOG-ABSENT-SITES.tsv` rows off-line; 9 files short; 1 N20 comment (`core_callbacks.rs:475`) | RED, 5 arms, same counts | yes |
| G-REFGEOM | GREEN | GREEN (36 lines, 3 targets) | GREEN (by reading) | yes |
| A3 | GREEN | GREEN (`include/ghostty.h` unchanged; `oracle-ghostty-h.sha256` not touched) | GREEN | yes |
| A5 | RED: (d) `rustty_osc_set`; (c) 10 registry entries (`OscOption`, `OscTerminator`, `ProgramStatusKind`, `ProgramStatusState`, `SemanticPromptKind`, `SemanticPromptPromptKind`, `TerminalMemoryUsage`, `TerminalProgramStatus`, `TerminalSemanticPrompt`, `TerminalUnknownOscSequence`) | the same RED, byte-identical output | RED already (DECISIONS D1, A5 reads `origin/main`) | yes, pre-existing |
| G9 | GREEN 693/693 (frozen surface) | against the frozen surface: GREEN 693/693 (`g9-before-regen-at-a60e9e2a5.txt`). Regenerated: RED 694/697 | RED on the new config keys and OSC kinds | yes, see F2/F3 |

## Regenerated `parity-surface.json` (T2 generator)

`build_parity_surface.py` at `a60e9e2a5`; `parse_csi.py --self-test` and `parse_esc.py --self-test`
PASS on the oracle first. `_meta.reference_rev` is now `a60e9e2a5…`.

G9 ids (the kinds `surface_ids` returns):

| kind | frozen → regenerated | APPEAR | VANISH | G9 |
|---|---|---|---|---|
| config | 216 → 218 | `vt-xt-checksum-report`, `vt-xt-checksum-extension` | none | both UNCOVERED (RED); owned by the DECRQCRA/XTCHECKSUM config port (R1-D, `00a19db51`) |
| osc | 27 → 29 | `program_status`, `unknown` | none | `program_status` UNCOVERED (RED; the OSC program-status port); `unknown` counted covered, see F3 |
| keybind, cli, terminfo, modes, sgr | 88, 19, 270, 42, 31 | none | none | covered |

Other sections (not G9 ids; G9's `csi`/`esc`/`c0` kinds read nothing, see F2):

- OSC subparsers 17 → 19 (`program_status`, `unknown`); OSC numbers referenced 12 → 14 (`2`,
  `7501`).
- **CSI finals**, 24 → 38 arms in the summary. APPEAR: `@ e g h l m n p q r s t u` (the 13 the
  old window hid, R4 §5.3), `y` (DECRQCRA/XTCHECKSUM, new in the range) and `}` (DECSASD, also
  outside the old window). VANISH: `;`.
  - `;` ("An empty first parameter.", old `:962`) and the first `else` (old `:976`, "Defer to
    the state machine for anything else") were arms of `csiEntryByte` (`:949` at `12752b2ac`),
    the parser's CSI-entry fast path, not of `csiDispatch`. The second old `else` (`:1406`) was
    `execute`'s C0 fallback (`invalid C0 character, ignoring`). **All three were artifacts of
    the old hard-coded window (expected), not real losses.** `else` stays in the id set, now as
    `csiDispatch`'s own fallback (`:2668`). `c` and `d` are kept (`:1945`, `:1969`), so R4's
    simulated "window loses `c` and `d`" does not occur with the function-scoped generator.
- **ESC finals**, 5 → 23 arms. VANISH: `p q r s`; these "ESC" rows (old `:2180`–`:2330`) were CSI
  arms `p q r s` (they now appear under CSI). `n` is in both sets but changes meaning: the old row
  (`:2125`) was CSI `n` (DSR); the new one is ESC `n` LS2 (`:3037`). **Artifacts (expected); no
  real loss.** APPEAR: the real `escDispatch` arms `B A 0 7 8 D E H M N O V W Z c o ~ } | = > \`
  and `else`. (The `\` final is stored in the Zig-source spelling `'\\'`, i.e. the JSON string
  `"\\\\"`; cosmetic. Fixed in P8-W0 T5, N523: the generator decodes the literal and the JSON holds `"\\"`.)

## Findings

- **F1 — `.port/phase-0/run-cvt-corpora.sh` and `build-examples.sh` cannot run from a worktree,
  and the examples' own `zig build` fails at this pin.** Both scripts hard-code
  `/Users/gael/projects/rustty/.port/phase-0/{examples-out,corpora/cvt}` (the main checkout; the
  re-capture would have written there), and `examples-out` (the prebuilt binaries the script
  runs) does not exist in either checkout. Building an example as `build-examples.sh` does
  (`cd example/c-vt-effects && zig build --prefix … --cache-dir …`, outputs outside the oracle)
  panics at `a60e9e2a5` on macOS: `unable to find artifact 'ghostty-vt'` (only
  `ghostty-vt-static` is offered to a dependent). Not investigated whether it predates the
  advance. The goldens were therefore re-captured by N395's method: each reference example's
  `src/main.c` (`main.cpp` for `cpp-vt-stream`) at the pin compiled with Xcode clang against the
  rebuilt `zig-out/include` and `zig-out/lib/libghostty-vt.a` (the frameworks
  `build_ref_driver.sh` links), run twice (golden = stdout+stderr combined, plus stdout, stderr,
  exit), written into this worktree's `corpora/cvt/`. All 23 built and exited 0. The scripts
  were not changed (outside this task). **→ N525** (P8-W0 T5): both scripts now run from any checkout;
  the `zig build` failure is oracle-side (upstream `d65cb5128` links the Darwin dylib with Apple's linker, so
  no `ghostty-vt` artifact is registered); `build-examples.sh` implements the clang method above.
- **F2 — G9 does not check CSI or ESC finals.** `check_parity_surface.py` registers `csi`, `esc`
  and `c0` owner kinds, but `surface_ids` never reads `csi_handlers`, `esc_handlers` or the C0
  list, so they print "(no ids in parity-surface.json)". The 13 + `y` + `}` CSI finals the old
  generator hid, and the 23 real ESC arms, cannot turn G9 red; the brief's "G9 RED … on the CSI
  finals the old generator hid" does not happen. Their coverage is unchecked by any gate today.
  (A candidate N for T4/R4-F; not fixed here.) **→ N523** (P8-W0 T5): G9 reads the Rust dispatch match
  arms; CSI `y` is the one real gap (P8-W3 T1).
- **F3 — G9 counts the OSC kind `unknown` as covered although it is not ported.** `present()`
  accepts any whole-word match of an id variant in the owner crate's token corpus, and the word
  `unknown` occurs throughout `rustty-vt`. The id closes with the unknown-OSC port (P8-W4); until
  then G9 under-reports by one. Same class as any common-word id. **→ N524** (P8-W0 T5): osc kinds are read
  from the `osc::Command` variants; `unknown` and `program_status` are UNCOVERED (P8-W4 T2, T3).

No other gate is RED or BLOCKED beyond R4 §5's arrival list.

## Files

| file | what |
|---|---|
| `*-before-at-12752b2ac.txt` | G1b, G3b, G3b `--strict`, G-TESTS, G-FILES, G-SYMS, G-CENSUS, G-LOGFILE, G-REFGEOM, A3, A5, G9 |
| `*-after-at-a60e9e2a5.txt` | the same set (G9 against the regenerated surface), plus G-ORACLE-SH-SELF |
| `g-census-arrival-at-a60e9e2a5.txt` | G-CENSUS at the new pin before `CENSUS_DELTA.md` moved (RED by design) |
| `g9-before-regen-at-a60e9e2a5.txt` | G9 at the new pin against the frozen surface (GREEN 693/693) |

## P8-W0 T4: phase-start records and gate maps

Negative control for the new G1b MAP row (`g1b-with-xt-checksum-row-at-a60e9e2a5.txt`):
`check_test_counts.py` with the `xt_checksum` row (`terminal/xt_checksum.zig` against
`crates/rustty-vt/src/xt_checksum*.rs`, a one-file glob so the missing Rust file counts 0 instead of tripping
the stale-mapping arm) reads

```
  input_mouse_encode   24    23  RED  drop=1
  cli_ssh            13    12  RED  drop=1
  xt_checksum        12     0  RED  drop=12
TEST-COUNT GATE: RED — 3 module(s) with dropped tests
```

so the row is RED with upstream 12 against Rust 0, as the brief requires; the other two rows are the arrival
state recorded above. The row goes GREEN when the `xt_checksum` port lands (P8-W3).

After the T4 records (`*-after-t4.txt`):

| gate | arrival (T3) | after T4 |
|---|---|---|
| G-UDDOCS | GREEN | PASS (UD20, UD312 "fixed upstream", docs rows removed) |
| G-PARITY | GREEN | GREEN |
| phase-2 closure | GREEN | GREEN |
| G3b `--not-carried-check` | 6 NOT-CARRIED-INVALID | GREEN, 37 rows (`lib_vt.zig` rows 40-45: 519, 522-525, 527; the doc block moved +13, not +9) |
| G-FILES | 6 MISSING | 5 MISSING (`windows_dll.zig` is a provisional NON-PORT) |
| G-SYMS | 8 unexplained | 7 (`windows_dll.DllMain` gone) |
| G-TESTS | 73 unexplained | 73 (unchanged; the ports are the fix) |
| G3b default scope | RED, 411 missing | not re-run in T4 (a 12-minute run); only `--not-carried-check` was. The new `crate_map` rows make G3b read the six files' comments, so its count rises when it next runs |
| G-LOGFILE arm 1 | 3 undeclared files, 10 sites | declared in `LOG-PORTMAP.tsv` (allowance = full site count: 6, 2, 2); the arm now reads "declared port file does not exist" until the ports land. The other arms (dead `Termio.zig:717`, 19 off-line ABSENT rows, short files, the N20 comment) are not in T4's scope. |

Not applied mechanically, on purpose: Appendix A's UD250 `OpenGL.zig:312` and UD343 `test.yml` anchors are
history anchors (20abdb50a code that no longer exists), UD205's "began at :1557 there" is a 20abdb50a anchor,
and UD180/UD181's "first cited as" corrections are history; those were kept.

## P8-W0 T5: gate repair (N523, N524, N525)

G9 after the repair (`g9-after-gate-repair-at-a60e9e2a5.txt`, with its `--self-test`, PASS):

| kind | before (T3, regenerated surface) | after T5 | UNCOVERED, owner |
|---|---|---|---|
| config | 216/218 RED | 216/218 RED | `vt-xt-checksum-report`, `vt-xt-checksum-extension`: P8-W3 T2 |
| osc | 28/29 RED (`unknown` read covered) | 27/29 RED, structural | `unknown`: P8-W4 T2; `program_status`: P8-W4 T3 |
| csi | no ids | 42/43 RED, structural | `y` (DECRQCRA/XTCHECKSUM): P8-W3 T1 |
| esc | no ids | 24/24 | none |
| c0 | no ids | 13/13 | none |
| keybind, cli, terminfo, modes, sgr | covered | unchanged | none |

Total 772/777 (was 694/697). No CSI, ESC or C0 final was a spelling miss of the matcher; `y` is a real gap.
