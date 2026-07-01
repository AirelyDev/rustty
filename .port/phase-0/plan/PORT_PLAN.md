# rustty PORT_PLAN — strict topological, leaves-first port order

**Phase:** 0 (reference oracle & baseline) — authoritative file-level partition.
**Date:** 2026-06-21
**Reference:** ghostty @ `4749c4e` (Zig 0.15.2), `/Users/gael/projects/ghostty/src`
(READ-ONLY). 512 `.zig` files, **3234** `test {}` blocks total.
**Governs:** `.port/PORT_RULES.md` (R1–R5) + the crate design spec
(`docs/superpowers/specs/2026-06-21-rustty-crate-structure-design.md`).
**Tooling:** `.port/tools/cycle_check.py` (crate-level SCC gate) and
`.port/tools/reach_scc.py` (file reachability + SCC condensation).
**Inventory:** `.port/phase-0/inventory/tests-summary.json` (generated this phase;
file→crate mapping + exact per-file `test{}` counts).

---

## 0. How this plan was produced (no fabrication)

1. The authoritative **file→crate mapping** mirrors `cycle_check.py::crate_of()`,
   extended with the upper-tier carve (App/Surface/Command→`core`,
   `global.zig`→`global`, `config/key.zig`→`cfg-fmt`,
   `cli/CommaSplitter.zig`→`args`). Result: **all 512 files mapped, 0 unmapped.**
2. **`test{}` counts** are exact line-grep counts (`^\s*test\b[^{]*\{`). Verified:
   zero multiline test openers exist in the tree (every test brace is on the
   opener line), and the per-file sum equals the direct tree total **3234**.
3. **Acyclicity** is established by `reach_scc.py`: after the modeled breaks the
   345-file mega-SCC decomposes to per-crate SCCs (Section 6).
4. **Topological validity** of the order below is machine-checked against the
   design's crate-dependency edges (Section 5).

---

## 1. Crate count & ordered list (the answer)

**29 port-order crates** (the rustty-* DAG nodes that own ported reference code).
Plus supporting non-ordered work: **`-sys` crates** and **`xtask`** (independent
of the parity port order; built as needed), and the **`extra`/`resources`**
shell-completion+integration assets (see Section 4, ambiguity A4).

Strict leaves-first topological order (ties broken to match design Section 5):

```
 1. rustty-util
 2. rustty-build-config
 3. rustty-identity
 4. rustty-datastruct
 5. rustty-unicode
 6. rustty-simd
 7. rustty-lib
 8. rustty-global
 9. rustty-render-types
10. rustty-os
11. rustty-vt            ← crown jewel
12. rustty-vt-c
13. rustty-crash
14. rustty-cfg-fmt
15. rustty-args
16. rustty-apprt-api
17. rustty-font
18. rustty-input
19. rustty-inspector
20. rustty-renderer
21. rustty-config
22. rustty-terminfo
23. rustty-termio
24. rustty-core
25. rustty-apprt
26. rustty-cli
27. rustty           (bin)
28. rustty-bench
29. rustty-synthetic
```

Tier grouping (design Section 3): T0 leaves 1–8; T1 9,10,13; T2 11–12;
T3 14–16; T4 17–20; T5 21–23; T6 24–25; T7 26–29.

This matches design Section 5 except for one deliberate ordering decision on
`render-types` vs `vt` (resolved in Section 3, ambiguity A1) and the explicit
insertion of `identity` (T0) and `apprt-api` (T3) which the design lists but the
one-line Section-5 ordering abbreviates.

---

## 2. Topological-sort confirmation

Validated programmatically against the design's per-crate dependency edges
(Section 3 of the spec): **for every crate, all of its rustty-crate dependencies
appear earlier in the list.** No crate precedes any dependency. (See Section 5
for the full edge set and the one resolved tension.)

- Leaf set with no rustty deps: `util, build-config, identity, datastruct,
  unicode, simd, lib, global, render-types*, cfg-fmt, apprt-api`.
- `vt` (11) follows all 7 of its deps (datastruct, unicode, simd, os, lib, util,
  render-types). `vt-c` (12) follows `vt`. `core` (24) follows its 11 deps.
  `apprt` (25) follows `core`. `cli` (26) follows `apprt`+`core`. `bin` (27)
  follows `cli`. All confirmed.
- (*) `render-types` is treated as a leaf — see A1.

---

## 3. Per-crate plan

Legend: **Files** = reference `src/` files this crate owns (from the inventory).
**Deps** = its rustty-crate dependencies (down-edges; `-sys`/std omitted).
**Breaks** = cycle-breaks from the 14-item break-list it requires/embodies.
**Tests** = exact `test{}` block count it owns (contract under R1). **DoD** =
per-crate Definition-of-Done.

---

### 1. rustty-util — Tests: **4**
- **Files (5):** `math.zig`, `fastmem.zig`, `file_type.zig`, `tripwire.zig`,
  `quirks.zig`.
- **Deps:** none.
- **Breaks:** **#2 `quirks.zig→font`** — relocate the single font-quirk out of
  `quirks.zig` so this stays a true leaf (it is the assert/util leaf).
- **DoD:** all 5 files ported with every comment; 4 ported tests pass + leak
  check; the font-quirk relocation landed (the only `quirks→font` edge gone);
  `cycle_check.py` shows `util` has no out-edge to `font`.

### 2. rustty-build-config — Tests: **0**
- **Files (1):** `build_config.zig`.
- **Deps:** none.
- **Breaks:** **#3 `build_config→{apprt,font,renderer}`** — backend enums
  (app-runtime / renderer / font backends) are **defined here** (low), not pulled
  up; consumers import them downward.
- **DoD:** backend enums defined locally; no out-edge to apprt/font/renderer in
  `cycle_check.py`; comptime config values reproduced; build-feature flags
  (Section 7 of spec) represented. (0 tests; fidelity critic confirms 0.)

### 3. rustty-identity — Tests: **0** (new crate, no reference file)
- **Files:** none from `src/` — **new** R2 source-of-truth crate.
- **Deps:** none.
- **Breaks:** none (additive glue allowed by R1; adds nothing-lost).
- **DoD:** exports app name, version strings, `TERM`/`TERM_PROGRAM`, terminfo
  name, config dir, VT query responses (XTVERSION, Primary/Secondary DA); default
  = `rustty`; feature **`identity-ghostty`** flips every value to the reference's
  for byte-exact differential testing. **Flag:** this crate has no ported tests;
  identity values cross-checked against reference strings in the differential
  harness, not via ported `test{}`.

### 4. rustty-datastruct — Tests: **76**
- **Files (11):** `datastruct/` (`array_list_collection, blocking_queue,
  cache_table, circ_buf, comparison, intrusive_linked_list, lru, main,
  message_data, segmented_pool, split_tree`).
- **Deps:** none (becomes a clean leaf only after the `global` hub break #1).
- **Breaks:** benefits from **#1** (was pulled into the mesh by bottom-up
  `→global.zig`); no edge it must itself cut.
- **DoD:** 11 files ported; **76** ported tests pass (largest leaf test set:
  circ_buf 21, comparison 24, split_tree 18) + leak checks; no out-edge to any
  upper crate in `reach_scc.py`.

### 5. rustty-unicode — Tests: **5**
- **Files (8):** `unicode/` (`grapheme, lut, main, props, props_table,
  props_uucode, symbols_table, symbols_uucode`) + generated tables.
- **Deps:** none.
- **Breaks:** none.
- **DoD:** 8 files ported; the comptime/uucode-generated tables
  (`props_table`, `symbols_table`) reproduced **bit-for-bit** (R1); 5 ported
  tests pass.

### 6. rustty-simd — Tests: **23**
- **Files (6):** `simd/` (`base64, base64_scalar, codepoint_width, index_of,
  main, vt`). + `simdutf-sys`, `highway-sys`.
- **Deps:** none (rustty); links `simdutf-sys`/`highway-sys`.
- **Breaks:** none.
- **DoD:** 6 files ported; 23 ported tests pass (simd/vt 18); SIMD paths match
  reference outputs in the differential harness.

### 7. rustty-lib — Tests: **13**
- **Files (9):** `lib/` (`allocator`, `allocator/convenience`, `enum`, `main`,
  `string`, `struct`, `target`, `types`, `union`).
- **Deps:** none.
- **Breaks:** none. (Internal `lib/allocator ↔ allocator/convenience` 2-file SCC
  stays *inside* this crate — legal.)
- **DoD:** 9 files ported; 13 ported tests pass; allocator utilities back the
  tracking-allocator leak harness (spec Section 8).

### 8. rustty-global — Tests: **0**
- **Files (1):** `global.zig` (leaf state holder split from the master hub).
- **Deps:** none (leaf, OnceLock-style process-state holder).
- **Breaks:** **#1 `global.zig` → `rustty-global` (leaf state) + wiring in
  `core`** — *the master hub break*; the single most important refactor (frees
  the whole bottom-up mesh). High-level wiring/top-down imports move to `core`.
- **DoD:** only leaf state remains here (no top-down imports of
  apprt/cli/renderer/crash/font); `reach_scc.py` confirms no edge `global→*upper*`;
  injection/OnceLock glue reviewed for soundness (R1 additive). 0 tests.

### 9. rustty-render-types — Tests: **6** (+5 if A1-hoist adopted)
- **Files (1):** `renderer/size.zig`. **(A1: also hoist `terminal/size.zig`
  here — +5 tests → 11 — see ambiguity A1.)**
- **Deps:** `util` only (after A1 hoist). Without the hoist it depends on a vt
  size type (the design's literal "render-types→vt (terminal/size)").
- **Breaks:** **#5 `renderer/size.zig` → `rustty-render-types`** (breaks
  font↔renderer; also imported by config, input, vt-c).
- **DoD:** size types extracted into a crate everyone below renderer can depend
  on; 6 ported tests pass (11 with A1); `cycle_check.py` confirms render-types is
  acyclic w.r.t. font/renderer. **Flag A1.**

### 10. rustty-os — Tests: **58**
- **Files (34):** `os/` (34 incl. `wasm/`, `string_encoding`, `shell`, `xdg`,
  `path`, `i18n`, …) + `pty.zig`. (`pty.c` is a C source → `os`'s `-sys`/cc
  build.)
- **Deps:** `datastruct, util, global, build-config` (+ platform `-sys`).
- **Breaks:** **#14 `os/open→apprt`** — relocate/inject the one `os/open.zig →
  apprt` edge so `os` is a clean leaf-ish base.
- **DoD:** 34 files ported; 58 ported tests pass (string_encoding 18, shell 8,
  xdg 5); `os/open` no longer imports apprt (`reach_scc.py` os-SCC is os-only:
  the 7-file `{cf_release_thread, homedir, locale, main, passwd, xdg, pty}`
  cluster stays internal).

### 11. rustty-vt — Tests: **2001** — **crown jewel**
- **Files (95):** all of `terminal/` except `terminal/c/` (94 terminal files +
  root `terminal.zig`) **plus the VT input slice** that `lib_vt.zig` pulls in:
  `input/key.zig, input/key_encode.zig, input/mouse.zig, input/mouse_encode.zig,
  input/paste.zig` **plus** `stb/` font-rasterizer shim. (The 5 input files add
  130 of the 2001 tests.)
- **Deps:** `datastruct, unicode, simd, os, lib, util, render-types`.
- **Breaks:** depends on **#1** (global injected), **#4** (terminal/c carved out
  → vt-c), **#5** (render-types extracted). Its only residual leaks
  (`search/Thread→global`, `apc/glyph/Glossary→font`) stay behind the existing
  runtime-swappable **`sys` hooks** (no hard font/global dep), matching
  `lib_vt.zig`'s libghostty-vt build.
- **DoD:** 95 files ported; **2001** ported tests pass + leak checks (this single
  count is ~62% of the whole port's tests — the dominant fidelity contract);
  `reach_scc.py` confirms the 55-file `lib_vt`-reachable internal SCC is
  self-contained with **zero** LEAK lines to font/renderer/config/global except
  via `sys` hooks; differential + fuzz harness clean vs reference. **Flag A2**
  (95-file crate vs design's "55-file" figure — reconciled in A2).

### 12. rustty-vt-c — Tests: **307**
- **Files (28):** `terminal/c/` (`terminal` 89, `render` 38, `kitty_graphics`
  35, `selection_gesture` 19, …).
- **Deps:** `vt`.
- **Breaks:** **#4 `terminal/c` split → `rustty-vt-c`**.
- **DoD:** 28 files ported; **307** ported tests pass; C ABI emitted via
  **cbindgen** → `include/rustty.h` + `rustty/vt/*`, `rustty_*` symbols (R2);
  outputs `librustty-vt.{a,so,dylib}`, `rustty-vt.wasm`,
  `rustty-vt.xcframework`; the c-vt-* conformance programs (Phase-0 corpora)
  pass.

### 13. rustty-crash — Tests: **9**
- **Files (4):** `crash/` (`dir`, `main`, `sentry`, `sentry_envelope`).
- **Deps:** `os, util` (+ `sentry-sys`/breakpad); **global injected** (not a hard
  dep).
- **Breaks:** depends on **#1** (global injected rather than imported).
- **DoD:** 4 files ported; 9 ported tests pass (sentry_envelope 8); global is
  injected; links the same sentry/breakpad C libs (R3).

### 14. rustty-cfg-fmt — Tests: **9**
- **Files (2):** `config/formatter.zig`, `config/key.zig`.
- **Deps:** none (leaf-ish base extracted below both config and input).
- **Breaks:** **#6 `config/formatter.zig`(+`config/key.zig`) →
  `rustty-cfg-fmt`** (breaks config↔input).
- **DoD:** 2 files ported; 9 ported tests pass (formatter 8); `cycle_check.py`
  confirms the config↔input cycle through the formatter is gone. **Flag A3**
  (`config/key.zig` companion placement).

### 15. rustty-args — Tests: **64**
- **Files (3):** `cli/args.zig`, `cli/diagnostics.zig`, `cli/CommaSplitter.zig`.
- **Deps:** `os, util, build-config`.
- **Breaks:** **#7 `cli/args.zig`(+`CommaSplitter`,`diagnostics`) →
  `rustty-args`** (breaks config↔cli).
- **DoD:** 3 files ported; 64 ported tests pass (args 36, CommaSplitter 28);
  config can depend on args without pulling cli (`cycle_check.py` clean).

### 16. rustty-apprt-api — Tests: **20** (interface crate; trait extraction) — A5 RESOLVED (LANDED)
- **Files:** the apprt **interface** surface — `apprt/{action,ipc,runtime,
  surface,structs}.zig` carved out of `apprt/` (the bulk of `apprt/` stays in
  crate 25). No whole reference file is *moved* here intact; this is the trait
  seam. `action.zig` `Target`/`Action` + all payloads (action.rs) + its 17
  tests (action_tests.rs); `ipc.zig` (2 tests); `runtime.zig` re-export of
  `rustty_build_config::Runtime` + its 1 reify test; `surface.zig` `Message`/
  `ReportTitleStyle`/`ChildExited`/`NewSurfaceContext`/
  `shouldInheritWorkingDirectory` + the `AppMailbox`/`SurfaceMessageEnvelope`
  seam (0 tests); `structs.zig` value types (0 tests).
- **Deps:** rustty-{lib,build-config,config,vt,input,datastruct,renderer},
  allocator-api2 — ALL downstream (the acyclicity guarantee). `rustty-renderer`
  supplies `renderer::Health` (hoisted to the renderer crate root, like the
  render-types A1 `size` hoist) for `Action::renderer_health`; the edge is
  acyclic (nothing renderer deps reaches apprt-api). **No** edge to core/apprt.
- **Breaks:** **#8 apprt iface → `rustty-apprt-api` trait** (core depends on the
  iface, not on apprt). The seam: `CoreSurfaceHandle` (implemented by core's
  `Surface`) lets `Target::cval` reach the opaque `rt_surface` without naming
  core; `App`/`Surface` traits with a GENERIC `perform_action` (comptime
  single-backend monomorphization, NOT `dyn`).
- **DoD:** trait/types compile standalone (`cargo build -p rustty-apprt-api`,
  which Cargo refuses on a cycle — the cycle-break witness); `check_acyclic.sh`
  GREEN; 20 tests pass. **Flag A5 RESOLVED**: apprt-api owns 20 test_blocks
  (action 17, ipc 2, runtime 1); crate 25 `apprt` keeps the remaining **27**
  (47 − 20). Global test contract unchanged (**3234**). C-ABI byte-exact pinning
  of the `CValue` unions + the GObject `getGObjectType` registrations are
  DEFERRED to the apprt embedded C-API / gtk-backend milestones (documented in
  the crate; additive, R1-safe).

### 17. rustty-font — Tests: **178**
- **Files (50):** `font/` (Atlas, Collection, SharedGrid, SharedGridSet,
  CodepointResolver, discovery, embedded, sprite/, opentype/, …).
- **Deps:** `vt, os, datastruct, util, render-types` (+ `freetype-sys,
  harfbuzz-sys, fontconfig-sys`, CoreText); **config via DI**.
- **Breaks:** **#9 `font↔config`** (extract font-config types / DI); also relies
  on **#2** (quirks’ font-quirk now lives in/near font) and **#5**
  (render-types) and **#3** (build-config enums).
- **DoD:** 50 files ported; **178** ported tests pass; the `font` SCC
  (`reach_scc.py` round-2 SCC#3 = 35 files) is font-only; config injected (no
  hard config dep); same C font libs linked (R3).

### 18. rustty-input — Tests: **116**
- **Files (13):** `input/` **minus the 5-file VT slice owned by `vt`**:
  `input.zig, input/Binding.zig, KeymapDarwin.zig, KeymapNoop.zig, Link.zig,
  command.zig, config.zig, function_keys.zig, helpgen_actions.zig, key_mods.zig,
  keyboard.zig, keycodes.zig, kitty.zig`.
- **Deps:** `vt, datastruct, util, cfg-fmt, render-types`.
- **Breaks:** relies on **#6** (cfg-fmt) to avoid config↔input.
- **DoD:** 13 files ported; **116** ported tests pass; no edge to `config` except
  via `cfg-fmt`; `cycle_check.py` clean. (Note: the 5 VT-input files and their
  130 tests are counted under `vt`, not here — see A2.)

### 19. rustty-inspector — Tests: **1**
- **Files (13):** `inspector/` (Inspector, main, units, widgets, widgets/*).
- **Deps:** `vt, font, util` (+ `dcimgui-sys`); **core via refs/trait**.
- **Breaks:** **#11 `renderer↔inspector`** (inspector hook in render `State`) and
  **#12 `inspector→core`** (inspector takes refs/trait). **Fallback:** if #11–#13
  prove uneconomical, group `renderer+inspector+termio` into one
  `rustty-render` crate (design Section 4 fallback; reach_scc.py SCC#1 = 45 files
  is exactly this glued cluster).
- **DoD:** 13 files ported; 1 ported test passes; core taken by ref/trait (no
  hard core dep); render `State` hook in place. **Flag A6** (fallback path).

### 20. rustty-renderer — Tests: **21**
- **Files (35):** `renderer/` **minus `size.zig`** (owned by render-types):
  `renderer.zig, Metal, OpenGL, WebGL, Options, Overlay, State, Thread, backend,
  cell, cursor, generic, …`.
- **Deps:** `vt, os, datastruct, unicode, util, crash, font, render-types`
  (+ opengl/Metal, `glslang-sys, spirv-cross-sys, wuffs-sys, libpng-sys,
  zlib-sys`); **apprt via renderer-surface trait, inspector via hook**.
- **Breaks:** **#10 `renderer→apprt`** (renderer-surface trait), **#11
  `renderer↔inspector`** (State hook). Relies on **#5** (size out).
- **DoD:** 35 files ported; 21 ported tests pass; no hard apprt dep (surface
  trait); same GPU/image C libs (R3); perceptual render diff clean vs reference.
  **Flag A6** (may merge with inspector+termio under fallback).

### 21. rustty-config — Tests: **167**
- **Files (18):** `config/` **minus `formatter.zig`+`key.zig`** (cfg-fmt):
  `config.zig, Config, CApi, ClipboardCodepointMap, ErrorList,
  RepeatableStringMap, Wasm, c_get, command, conditional, edit, file_load, …`.
- **Deps:** `datastruct, os, util, vt, font, renderer, input, args, cfg-fmt`
  (+ `oniguruma-sys`, themes).
- **Breaks:** relies on **#6** (cfg-fmt), **#7** (args), **#9** (font↔config DI).
- **DoD:** 18 files ported; **167** ported tests pass; config parse-then-serialize
  matches reference (differential, via the cfg-fmt formatters); themes/oniguruma
  linked (R3); `config` SCC (round-2 SCC#5 minus input) is config-only.

### 22. rustty-terminfo — Tests: **4**
- **Files (3):** `terminfo/` (`Source`, `ghostty` → entry name `rustty`, `main`).
- **Deps:** `identity` (entry name `rustty` per R2).
- **Breaks:** none.
- **DoD:** 3 files ported; 4 ported tests pass; emits terminfo named `rustty`
  (R2; `identity-ghostty` feature emits `ghostty`/`xterm-ghostty` for the parity
  harness); terminfo corpus matches.

### 23. rustty-termio — Tests: **34**
- **Files (10):** `termio/` (`termio.zig, Exec, Options, Termio, Thread,
  backend, mailbox, message, shell_integration, stream_handler`).
- **Deps:** `vt, crash, datastruct, os, util, terminfo`; **renderer/apprt via
  trait**.
- **Breaks:** **#13 `termio→renderer/apprt`** (termio-surface trait).
- **DoD:** 10 files ported; 34 ported tests pass; no hard renderer/apprt dep
  (surface trait); `cycle_check.py` clean. **Flag A6** (fallback group member).

### 24. rustty-core — Tests: **11**
- **Files (4):** `App.zig, Surface.zig, Command.zig, surface_mouse.zig` (+ the
  high-level **wiring split from `global.zig`** per break #1 — additive glue,
  not a reference file).
- **Deps:** `vt, config, renderer, font, input, termio, os, crash, inspector,
  apprt-api, global`.
- **Breaks:** consumes **#1** (global wiring lands here), **#8** (apprt-api
  trait), **#12** (inspector by ref).
- **DoD:** 4 files ported; 11 ported tests pass (Command 10); depends on
  `apprt-api` (not `apprt`); global wiring reviewed (R1 additive); no edge to
  `apprt`/`cli` (`cycle_check.py` confirms core is below apprt).

### 25. rustty-apprt — Tests: **47**
- **Files (62):** `apprt/` (gtk/ 40+, embedded, none, browser, action, surface,
  ipc, structs, runtime, …) **minus** the apprt-api iface seam (crate 16).
- **Deps:** `core, apprt-api, vt, font, renderer, inspector, config, input, os`
  (+ gtk-rs/gobject, objc, wayland).
- **Breaks:** consumes **#8** (apprt-api), **#14** (os/open relocated here),
  **#10/#13** (renderer/termio surface traits point at apprt-api).
- **DoD:** 62 files ported; 47 ported tests pass; `apprt` SCC (round-2 SCC#2 = 43
  files; plus the 2-file `gtk/ext ↔ ext/actions` internal SCC#12 stays internal)
  is apprt-only; gtk-rs/objc bindings (not reimplementations, R3) wired.

### 26. rustty-cli — Tests: **45**
- **Files (25):** `cli/` **minus** `args.zig, diagnostics.zig, CommaSplitter.zig`
  (crate 15): `cli.zig, Pager, action, boo, crash_report, edit_config,
  explain_config, ghostty, help, list_actions, list_colors, list_fonts, …`.
- **Deps:** `config, vt, font, input, os, terminfo, args, apprt, core, crash`.
- **Breaks:** consumes **#7** (args).
- **DoD:** 25 files ported; 45 ported tests pass; `cli` SCC (round-2 SCC#4 = 18
  files) is cli-only; CLI command surface matches reference (renamed to `rustty`,
  R2).

### 27. rustty (bin) — Tests: **6**
- **Files (7):** `main.zig, main_ghostty.zig, main_c.zig, main_gen.zig,
  main_wasm.zig, main_build_data.zig, lib_vt.zig`.
- **Deps:** `cli, apprt, core` (entry points fan out only).
- **Breaks:** none (entry points are pure sources; nothing imports them).
- **DoD:** 7 entry files ported; 6 ported tests pass; produces the **`rustty`**
  binary (R2) + the `lib_vt` library root; `--version`/XTVERSION/DA report
  rustty.

### 28. rustty-bench — Tests: **8**
- **Files (13):** `benchmark/` (Benchmark, CApi, CodepointWidth, GraphemeBreak,
  IsSymbol, OscParser, ScreenClone, TerminalParser, TerminalStream, cli, main,
  options, …) + `main_bench.zig`.
- **Deps:** `vt, simd, unicode`.
- **Breaks:** none.
- **DoD:** 13 files ported; 8 ported tests pass; benchmarks run against the
  Phase-0 `BASELINE.json` (perf is Goal-2 gradient; parity unaffected).

### 29. rustty-synthetic — Tests: **12**
- **Files (9):** `synthetic/` (Bytes, Generator, Osc, Utf8, cli, cli/Ascii,
  cli/Osc, cli/Utf8, main).
- **Deps:** `vt, os` (fuzz/test data).
- **Breaks:** none. (Internal 4-file `synthetic/cli ↔ …` SCC stays internal.)
- **DoD:** 9 files ported; 12 ported tests pass; feeds the differential
  **fuzzing** harness (spec Section 8).

---

### Supporting (NOT in the parity port order)

- **`xtask` (37 files):** `build/` (34) + `helpgen.zig` + `input/helpgen_actions`
  is *kept in input*; xtask owns build orchestration — cbindgen headers, terminfo
  compilation, shell-integration/theme bundling, man pages, i18n, dist tarball,
  macOS xcframework/`.app`. **7 test blocks.** Independent of port order; some
  `build/` files form an internal 3-file SCC (`GhosttyDist, GhosttyFrameData,
  SharedDeps`) that stays inside xtask.
- **`-sys` crates (crates-sys/):** `freetype-, harfbuzz-, fontconfig-,
  oniguruma-, simdutf-, highway-, glslang-, spirv-cross-, wuffs-, zlib-, libpng-,
  sentry-, dcimgui-sys` (+ platform binding crates). Each vendors copied C
  sources + `build.rs` (cc) + bindgen (R3). Built on demand by the crate that
  links them; no port-order constraint.
- **`extra` / `resources` (5 files, 2 tests):** `extra/{bash,fish,zsh,vim,
  sublime}.zig` shell-completion generators (they `@import config/Config.zig` and
  `cli.zig`, consumed by `main_build_data.zig`/`main_ghostty.zig`). **Flag A4:**
  not named in the design DAG; assigned to xtask/cli completion-generation.

---

## 4. File→crate mapping ambiguities (flagged)

- **A1 — `render-types` vs `vt` ordering / `terminal/size.zig` placement
  (RESOLVED, needs sign-off).** The design *table* (Tier 1) says
  `render-types → vt (terminal/size)`, which would order render-types AFTER vt;
  but the design *port order* (Section 5) and the *vt dependency list* (Tier 2,
  `vt` deps include `render-types`) put render-types BEFORE vt. The literal table
  reading fails a topological sort. **Resolution adopted here:** hoist the tiny
  `terminal/size.zig` (227 LOC, 5 tests, imports only `std`+`quirks`; its *only*
  importer in the entire tree is `renderer/size.zig`) into `rustty-render-types`,
  making render-types a true leaf depending on `util` only. This both yields a
  valid topo sort and matches break #5's intent. Net effect: render-types owns 2
  files (11 tests); vt's terminal file count drops by 1 (94→93 terminal files,
  95→94 total). **Decision needed:** confirm the hoist (vs. co-developing the
  size type inside vt and exporting it).

- **A2 — `vt` file/test count vs design's "55-file" figure (reconciled).** The
  design calls vt "the 55-file terminal core." That 55 is the *minimal internal
  SCC* reachable from `lib_vt.zig` (`reach_scc.py` round-2 SCC#0 = 55). The
  *crate* `vt` legitimately owns more: all 95 mapped files = 94 `terminal/`
  files + root `terminal.zig` + the 5-file VT input slice + `stb/`. The extra
  files are acyclic terminal/input/stb code that belongs in the same crate. **No
  conflict, but flagged** because counts differ from the headline number. (With
  A1 hoist: 94 files, ~1996 tests.)

- **A3 — `config/key.zig` placement (low-risk).** Design break #6 names
  `config/formatter.zig (+ config/key.zig)` → cfg-fmt; the cycle tool's `EXTRACT`
  table lists only `config/formatter.zig`. Mapped `config/key.zig` → cfg-fmt per
  the spec text (1 extra test). Confirm cfg-fmt is the intended home (vs. config).

- **A4 — `extra/` shell-completion generators not in the DAG.** 5 files,
  2 tests, importing config+cli, consumed by build-data/main. Assigned to
  xtask/cli completion-generation; the design DAG does not enumerate them.
  Confirm placement.

- **A5 — `rustty-apprt-api` exact file/symbol partition — RESOLVED (LANDED).**
  The break landed by carving `apprt/{action,ipc,runtime,surface,structs}.zig`
  into `apprt-api` (crate 16). Partition of test_blocks: **apprt-api = 20**
  (action.zig 17, ipc.zig 2, runtime.zig 1; surface/structs 0), **apprt (crate
  25) = 27** (47 − 20); global test contract unchanged (**3234**). The seam is
  three traits — `CoreSurfaceHandle` (the cycle-break bridge to the opaque
  `rt_surface`, implemented by core's `Surface`), and `App`/`Surface` with a
  GENERIC (comptime-monomorphized, NOT `dyn`) `perform_action` — plus the
  `AppMailbox`/`SurfaceMessageEnvelope` replacement for `apprt.surface.Mailbox`.
  The concrete gtk/embedded backends, `newConfig`, the GObject `getGObjectType`
  registrations, and the byte-exact `CValue` C-ABI projection stay for crate 25 /
  the embedded C-API milestone (documented deferrals, verbatim-preserved).
  Crate deps: rustty-{lib,build-config,config,vt,input,datastruct,renderer} +
  allocator-api2 (all downstream — acyclic, `check_acyclic.sh` GREEN). The
  renderer dep is for `renderer::Health` (crate-root hoist).

- **A6 — fallback group `renderer+inspector+termio`.** Breaks #11–#13 are the
  hardest seams; `reach_scc.py` round-2 SCC#1 = **45 files** (renderer 20,
  inspector 11, termio 8, root 4, crash 2) is exactly the still-glued cluster.
  Design Section 4 pre-authorizes grouping these three into one `rustty-render`
  crate if the trait work proves uneconomical. If invoked, crates 19/20/23 merge
  into one node (tests 1+21+34 = 56); the topological position is unchanged
  (after font, before config/core).

---

## 5. Crate dependency edges used for topo validation

```
util:          —
build-config:  —
identity:      —
datastruct:    —
unicode:       —
simd:          —
lib:           —
global:        —
render-types:  (util)            # A1 leaf reading
os:            datastruct util global build-config
crash:         os util           # global injected
vt:            datastruct unicode simd os lib util render-types
vt-c:          vt
cfg-fmt:       —
args:          os util build-config
apprt-api:     —
font:          vt os datastruct util render-types        # config via DI
input:         vt datastruct util cfg-fmt render-types
inspector:     vt font util                              # core via ref/trait
renderer:      vt os datastruct unicode util crash font render-types  # apprt/inspector via trait/hook
config:        datastruct os util vt font renderer input args cfg-fmt
terminfo:      identity
termio:        vt crash datastruct os util terminfo      # renderer/apprt via trait
core:          vt config renderer font input termio os crash inspector apprt-api global
apprt:         core apprt-api vt font renderer inspector config input os
cli:           config vt font input os terminfo args apprt core crash
bin:           cli apprt core
bench:         vt simd unicode
synthetic:     vt os
```

Machine check result: **valid topological sort — no crate appears before any of
its dependencies** (under the A1 leaf reading of render-types). The only ordering
that fails is the literal "render-types→vt" table edge, which A1 resolves.

---

## 6. Acyclicity confirmation (gate evidence)

From `reach_scc.py` (runtime-only, test-block edges excluded — Cargo permits
cyclic dev-deps):

- **Before breaks:** `src/` is **one 345-file SCC** + 3 small clusters.
- **After Round-1 hub breaks** (#1 global, #2 quirks, #3 build_config enums, #5
  render-types/SharedGrid, #14 os/open, #4 terminal/c): mega-SCC shatters; `vt`
  (55), `os` (7), `vt-c` sub-clusters, `lib` (2) become clean; residual 167-file
  upper-half SCC.
- **After Round-2 seam breaks** (#8 apprt-api, #9 font↔config, #10/#11/#13
  surface traits, #7 args): **14 SCCs**, each a single crate except SCC#1 (the
  45-file `renderer+inspector+termio` fallback cluster — A6). Per-crate SCC sizes:
  vt 55, fallback-cluster 45, apprt 43, font 35, cli 18, config 7(+input2),
  os 6, vt-c sub-SCCs (5/2/2), synthetic 4, xtask/build 3, lib 2, apprt/gtk-ext 2.

All remaining SCCs are **intra-crate** (legal) once #11–#13 land or the A6
fallback is taken. `cycle_check.py` is the standing gate: re-run continuously;
fail on any unexpected cross-crate SCC.

---

## 7. Summary

- **Crate count (parity port order):** **29**. (+ `xtask`, the `-sys` crate set,
  and `extra`/`resources` assets, which are outside the topo order.)
- **Total reference files mapped:** 512/512, **0 unmapped**.
- **Total `test{}` blocks (R1 contract):** **3234** — dominated by `vt` (2001,
  62%), `vt-c` (307), `font` (178), `config` (167), `input` (116),
  `datastruct` (76), `args` (64), `os` (58).
- **Ordered list:** util → build-config → identity → datastruct → unicode → simd
  → lib → global → render-types → os → **vt** → vt-c → crash → cfg-fmt → args →
  apprt-api → font → input → inspector → renderer → config → terminfo → termio →
  core → apprt → cli → **rustty(bin)** → bench → synthetic.
- **Topological validity:** confirmed (Section 2/5) under the A1 resolution.
- **Ambiguities flagged:** A1 (render-types/terminal-size ordering — resolved,
  needs sign-off), A2 (vt 55-vs-95 count — reconciled), A3 (config/key.zig home),
  A4 (extra/ not in DAG), A5 (apprt-api file/symbol partition — least pinned),
  A6 (renderer+inspector+termio fallback group).

---
## Amendment (2026-06-22): Zig-stdlib dependency discovery

The Phase-0 dependency graph scanned only `src/` `@import`s and therefore MISSED
dependencies on the Zig STANDARD LIBRARY. `PageList` (and ~51 files via
`ArenaAllocator`, 10 via `MultiArrayList`, 8 via `AutoArrayHashMap`, 1 via
`std.heap.MemoryPool`) require faithful Rust equivalents of Zig stdlib types that
have no Rust-std counterpart. Resolution: a new foundational leaf crate
**`rustty-zigstd`** (additive infra; NOT in the 3234 test contract) provides them.
Shipped: `ArenaAllocator` (free_all/retain_capacity/retain_with_limit),
`MemoryPool`/`MemoryPoolAligned`, `AutoArrayHashMap`/Unmanaged. TODO as consumers
land: `MultiArrayList` (used by Screen/Terminal et al.), `BoundedArray` (0 users).
Crates needing the Zig memory model now depend on `rustty-zigstd`.
