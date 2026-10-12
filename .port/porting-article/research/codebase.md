# Research dossier: codebase shape and scale (Rustty vs Ghostty)

Snapshot: Rustty `1d22ba80d` (branch `phase-8-upstream-resync`, 2026-10-08, 2,968 commits since
`15c1a9bd7` on 2026-06-21). Oracle: Ghostty `a60e9e2a57f73e1eef2bd1cf2995a467f69e7fb0` (2026-10-07),
checked out at `/Users/gael/projects/ghostty` (`.port/ORACLE_SYNC.md`; `git -C ../ghostty rev-parse HEAD`
confirmed). Ported Zig dependency packages read from `/Users/gael/projects/ghostty/zig-pkg/` at the
hashes pinned in Ghostty's `build.zig.zon`.

Provenance tags: **MEASURED** (I computed it; the command or script is given), **DOCUMENTED** (read from a
repo doc; path given), **ESTIMATED**.

Scripts used (all under `.port/journey-article/research/tmp/`): `crate_stats.py` (per-crate tokei +
regex scan), `module_map.py` (Zig dir to crate mapping), plus inline Python recorded below. Every
LOC figure is tokei "code" lines (no comments, no blanks) unless stated otherwise.

---

## 1. Headline numbers

| Fact | Value | Provenance |
|---|---|---|
| Ghostty Zig, `src/` at the pin | 249,453 code lines, 56,319 comment lines, 599 files | MEASURED `tokei ../ghostty/src -t Zig` |
| Ghostty Zig, everything Rustty ports (src + pkg wuffs/oniguruma wrappers + libxev, libvaxis, z2d, zf) | 302,669 code lines | MEASURED `module_map.py` |
| Rustty Rust, 45 workspace crates + xtask (excl. vendored) | 507,628 code lines, 170,016 comment lines, 1,231 `.rs` files | MEASURED `crate_stats.py` |
| Rustty Rust incl. the 16 `-sys` crates | 511,728 code lines | MEASURED |
| Rust / Zig code ratio, like-for-like scope | 1.68x | MEASURED (507,628 / 302,669) |
| Rust / Zig ratio, VT engine only | 1.50x (174,265 vs 116,461) | MEASURED |
| Comment density, Zig src vs Rust crates | 0.23 vs 0.33 comment lines per code line | MEASURED |
| Workspace packages | 61 (45 crates + 16 `-sys`), 313 internal edges (287 normal, 5 build, 21 dev) | MEASURED `cargo metadata --no-deps --offline` |
| External crates.io packages in the resolved graph | 138 (37 distinct direct normal/build deps) | MEASURED `cargo metadata --offline` |
| Zig `test` blocks in Ghostty `src/` | 4,423 | MEASURED regex `^\s*test\s*("|\{)` over `src/**/*.zig` |
| Rust `#[test]` functions | 7,270 | MEASURED `crate_stats.py`; cross-checked `git grep -cE '^[[:space:]]*#\[test\]'` = 7,270 |
| Per-module test-count gate (211 mapped modules) | Zig 4,172 vs Rust 6,042, every module rust >= zig, GREEN | MEASURED `python3 .port/tools/gates/check_test_counts.py` |
| Distinctive Ghostty comments checked by G3b | 34,345 across 29 crates; 0 missing; 96 in "review" band | DOCUMENTED `.port/phase-8/w0/g3b-strict-finale-at-a60e9e2a5.txt` |
| Reference comments deliberately not carried | 79 rows (73 no-counterpart, 6 gag-prose) = 0.23% of checked | MEASURED `.port/parity-closure/COMMENT-NOT-CARRIED.tsv` |
| `unsafe` sites (Bun-method census pattern) | 22,311 (20,583 blocks, 1,217 fns, 386 extern, 124 impl, 1 trait) | MEASURED `.port/audits/count-unsafe.sh <tmp path>` |
| Share of `unsafe` sites inside test code | about 58% (13,017 of 22,311) | ESTIMATED (heuristic split at first `#[cfg(test)]`, see section 6) |
| Vendored C/C++ under `crates-sys/*/vendor` | 1,058,459 code lines over 16 libraries; Rust wrappers + build.rs 4,100 | MEASURED tokei |
| macOS Swift app | 26,843 lines (198 files) vs Ghostty 26,742 (197); per-area identical except `Tests` (+101) | MEASURED tokei |
| libghostty-vt header family | 35 headers, 208 functions, 79 enum sentinels on both sides | MEASURED |
| Embedding API entrypoints | 90 `GHOSTTY_API` entrypoints mapped 1:1 to `rustty_*` | DOCUMENTED `.port/tools/gates/check_symbol_manifest.sh`, `.port/audits/CABI_SYMBOLS.tsv` |
| Verification tooling (`.port/tools`) | 33,072 code lines (Python 17,632, Shell 12,183, JS 1,862, PowerShell 460); about 100 named gates in `ci.sh` | MEASURED tokei; `grep -oE 'G[0-9]...|G-[A-Z0-9-]+' .port/tools/ci.sh | sort -u` |

---

## 2. Zig module to Rust crate mapping (LOC and tests)

Data: `data/codebase-zig-vs-rust-per-module.csv` (MEASURED, `tmp/module_map.py`).

Ghostty's `src/` is one Zig module, not a set of packages. `.port/cycle-analysis.md` (DOCUMENTED)
measured its runtime import graph as **one 345-file strongly connected component**, with `global.zig`
as the main hub. Rust crates cannot be cyclic, so the port cut the tree into crates along
14 enumerated breaks (`docs/superpowers/specs/2026-06-21-rustty-crate-structure-design.md` section 4).
The mapping I used follows that spec. The one deliberate re-attribution: the VT input slice
(`src/input/{key,key_encode,key_mods,function_keys,kitty,mouse,mouse_encode,paste,config}.zig`, 4,859
Zig lines, 174 tests) was ported into `rustty-vt/src/input/` (7,746 Rust lines), so it is counted in the
VT engine row.

| Module | Zig code | Rust code | Ratio | Zig tests | Rust tests | Rust crates |
|---|---:|---:|---:|---:|---:|---|
| VT engine (`terminal/` + VT input slice) | 116,461 | 174,265 | 1.50 | 2,973 | 3,474 | rustty-vt |
| VT C API (`terminal/c` + `lib_vt.zig`) | 20,989 | 41,533 | 1.98 | 465 | 606 | rustty-vt-c |
| font | 21,349 | 36,697 | 1.72 | 171 | 292 | rustty-font + rustty-font-shaper |
| apprt (GTK, embedded, none) | 20,795 | 36,568 | 1.76 | 61 | 358 | rustty-apprt + -apprt-api + -apprt-c |
| libxev (dep) | 17,239 | 30,372 | 1.76 | 124 | 204 | rustty-xev |
| renderer | 9,364 | 27,249 | 2.91 | 35 | 254 | rustty-renderer + rustty-render-types |
| config | 8,734 | 19,948 | 2.28 | 184 | 283 | rustty-config + rustty-cfg-fmt |
| z2d (dep, subset) | 19,812 | 9,907 | 0.50 | 176 | 105 | rustty-z2d |
| cli | 7,502 | 15,212 | 2.03 | 109 | 214 | rustty-cli + rustty-args |
| libvaxis (dep) | 14,206 | 13,862 | 0.98 | 151 | 174 | rustty-vaxis |
| termio (+ `Command.zig`, `pty.zig`) | 5,971 | 13,942 | 2.33 | 46 | 138 | rustty-termio |
| core (`Surface.zig`, `App.zig`, `surface_mouse.zig`) | 4,996 | 13,206 | 2.64 | 3 | 91 | rustty-core |
| os | 3,393 | 9,531 | 2.81 | 50 | 143 | rustty-os |
| input (non-VT part) | 4,549 | 8,645 | 1.90 | 90 | 124 | rustty-input |
| build (`src/build/` + `build_config.zig`) | 4,646 | 7,772 | 1.67 | 7 | 133 | xtask + rustty-build-config |
| inspector | 4,193 | 7,086 | 1.69 | 1 | 23 | rustty-inspector |
| datastruct | 4,190 | 5,814 | 1.39 | 88 | 100 | rustty-datastruct |
| unicode | 566 | 5,386 | 9.52 | 11 | 23 | rustty-unicode |
| ... (smaller rows in the CSV) | | | | | | |
| Rust-only infra (zigstd, identity, version, zigimg) | 0 | 5,926 | - | 0 | 124 | |
| **Total** | **302,669** | **507,628** | **1.68** | **4,901** | **7,204** | |

Notes for the article:

- **Why Rust is bigger (ESTIMATED, with partial measurements).** (a) Comments are carried verbatim
  under the G3b gate *and* the port adds its own notes: Rust comment lines are 0.33 per code line vs
  0.23 in Zig (MEASURED). (b) Rust formatting is more vertical: lines that hold only closing
  delimiters are 20.0% of Rust non-comment lines vs 16.5% in Zig (MEASURED, inline regex over
  `ghostty/src` and `rustty/crates`). (c) Zig `comptime` generics and implicit pointer use turn into
  explicit const generics, trait impls and raw-pointer plumbing. (d) Test scaffolding is larger
  (7,270 vs 4,423 test functions).
- **Renderer 2.9x / core 2.6x / os 2.8x.** These rows hold the cross-thread seams that replace
  Ghostty's direct cross-module calls (renderer-surface trait, `RendererConfig` accessor trait,
  apprt-api), and in core the Zig file-level struct became module trees (`crates/rustty-core/src/surface/`).
- **Rows below 1.0x are subset ports.** `rustty-z2d` ports only the z2d subset Ghostty's sprite and glyf
  rasterizers use (the Cargo.toml comment says "z2d 0.10.0 subset"; Ghostty's pin now uses z2d 0.12.1,
  see Gaps). `rustty-vaxis` is close to 1:1 with libvaxis 0.6.0.
- **unicode 9.5x** is generated property tables (`crates/rustty-unicode/src/generated/props_data.rs`,
  1,823 lines) that replace build-time table generation from the `uucode` Zig package (8,891 Zig lines,
  not ported as code).
- **Tests.** The test-count gate (`check_test_counts.py`, GREEN at HEAD) maps 211 modules and
  requires `rust >= zig` per module: Zig 4,172 vs Rust 6,042 (MEASURED). Data:
  `data/codebase-test-counts-per-module.csv`. Largest: terminal 436/489, kitty 351/397, osc 345/387,
  pagelist 294/352, screen 225/242.

---

## 3. Crate graph and layering decisions

Data: `data/codebase-crate-graph.json` (nodes with tier, code LOC, tests, unsafe sites, longest-path
depth, in/out degree; edges with kind and `optional`; a `layering_decisions` array). MEASURED from
`cargo metadata --format-version 1 --no-deps --offline`.

- 61 nodes, 313 internal edges. Longest normal/build dependency chain: depth 18 (`xtask`), 17
  (`rustty`, `rustty-apprt-c`, `rustty-extra`).
- Most depended-on crates: `rustty-build-config` and `rustty-os` (17 dependents each),
  `rustty-datastruct`, `rustty-identity`, `rustty-zigstd` (16), `rustty-util`, `rustty-vt` (15).
- Acyclicity is enforced as a gate (R4, `.port/tools/cycle_check.py`, `cargo_cycle_check.py`);
  `Cargo.toml` lists members in strict leaves-first order.

Layering decisions, each verified against the measured edge set:

| Decision | What the graph shows | Evidence |
|---|---|---|
| D1: font is pure and below vt; the shaper is above vt | `rustty-vt -> rustty-font`; `rustty-font` has no edge to vt and no `-sys` edge; `rustty-font-shaper -> {rustty-vt, rustty-font, freetype-sys, harfbuzz-sys, fontconfig-sys}` | MEASURED; memory note `apc-glyf-font-layering-d1`; Cargo.toml comment at the `rustty-font-shaper` member |
| Renderer config via an accessor trait | `trait RendererConfig` at `crates/rustty-renderer/src/config.rs:42`, implemented in `crates/rustty-config/src/renderer_config.rs`; edge `rustty-config -> rustty-renderer` (optional); no renderer -> config edge | MEASURED |
| `Command.zig` lives in termio, not core | `crates/rustty-termio/src/command.rs`; `rustty-core -> rustty-termio` | MEASURED; memory `integration-tier-decisions` |
| `terminal/c` split out | `rustty-vt-c` depends on vt, lib, os, render-types, unicode only | MEASURED; spec break #4 |
| `global.zig` hub split | `rustty-global` is a leaf; `rustty-xev` depends only on it | MEASURED; `.port/cycle-analysis.md` |
| apprt interface extracted | `rustty-apprt-api` has 5 dependents; core depends on it, not on `rustty-apprt` | MEASURED |
| C ABIs are sinks | `rustty-vt-c`, `rustty-apprt-c`, `rustty-wasm`, `rustty-web-canvas` have no internal dependents | MEASURED |

Cycle-break cost (DOCUMENTED, `.port/cycle-analysis.md`): round 1 (inject `global`, move
`build_config` enums down, split `terminal/c`, and 3 more) broke the 345-file SCC into clean leaves plus a
167-file upper-half SCC; round 2 (apprt-api trait, font/config extraction, renderer/inspector/termio
surface traits, args extraction) broke that up too.

---

## 4. C API, embedding and WASM

Data: `data/codebase-c-api.csv` (MEASURED unless marked).

- **librustty-vt** (Ghostty's libghostty-vt): `include/rustty/vt/*.h` matches `include/ghostty/vt/*.h`:
  35 headers, 13,933 vs 13,926 lines, 208 vs 208 declared functions, 79 vs 79
  `_MAX_VALUE = *_ENUM_MAX_VALUE` sentinels (the AGENTS.md rule for pre-C23 int enum sizing).
  `rustty-vt-c` has 206 `#[no_mangle]`/`export_name` sites. `include/rustty.h` (3,027 lines) is
  cbindgen-generated from `rustty-vt-c`.
- **Embedding API** (Ghostty's `include/ghostty.h`, used by the macOS app): `include/rustty-embed.h`
  1,290 lines vs 1,286; 90 `GHOSTTY_API` entrypoints mapped 1:1 in `.port/audits/CABI_SYMBOLS.tsv`
  (91 data rows) and checked by `check_symbol_manifest.sh`. `rustty-apprt-c` has 80 `#[no_mangle]`
  sites and `rustty-config` 17 (config C-ABI); `include/rustty-apprt.h` (1,502 lines) is the cbindgen
  view of `rustty-apprt-c` alone.
- **`ghostty_*` aliases: no longer present.** The completion plan locked "`ghostty_*` aliases over a
  canonical `rustty_*` ABI" so the reused Swift app could stay byte-identical (memory
  `rustty-completion-plan`; commit `1395249e5` 2026-07-12 imported the Swift app byte-identical).
  The later rebrand deleted the alias forwarders: "all C exports: ghostty_* -> rustty_* (alias deleted,
  Task 1)", "the Swift app links `rustty_*` directly" (`.port/tools/PARITY_NAMING.md` lines 14-15, 26).
  `grep ghostty_ macos/Sources` returns 0; `rustty_` returns 546 (MEASURED). The article should describe
  aliases as the transition step, not the final state.
- **WASM**: Ghostty's `src/main_wasm.zig` is 17 code lines. Rustty has `rustty-wasm` (275 lines, the
  headless VT+font+config module) and `rustty-web-canvas` (263 lines, a `wasm32-unknown-unknown` cdylib
  exporting the VT C-ABI plus a browser-Canvas shaper C-ABI with zero WASI imports, per the
  `Cargo.toml` member comment). `rustty-font-shaper` carries 19 export sites for the `web_canvas`
  backend.

---

## 5. macOS Swift app

MEASURED with tokei: Rustty `macos/` 26,843 Swift code lines in 198 files; Ghostty `macos/` 26,742 in 197.
Per area the counts are identical (`Sources/Features` 11,536; `Sources/RusttyKit` = `Sources/Ghostty`
8,424; `Sources/Helpers` 2,546; `Sources/App` 1,028; UITests 603) except `Tests` (2,706 vs 2,605).
The app was imported byte-identical (`1395249e5`, 2026-07-12) and renamed (`406e399df` and the
later rebrand). 24 commits touch `macos/Sources` (MEASURED `git log --oneline -- macos/Sources | wc -l`).
For the article: the Swift layer is reused, not ported; the work was the C ABI underneath it.

---

## 6. Unsafe audit

Data: `data/codebase-unsafe-per-crate.csv`, `data/codebase-vt-unsafe-by-area.csv`,
`data/codebase-unsafe-hotspot-files.csv`, `data/codebase-unsafe-census-timeseries.csv`.

Method: the project's own census, `.port/audits/count-unsafe.sh` (pattern
`unsafe \{|unsafe fn|unsafe extern|unsafe impl|unsafe trait`, excluding `vendor/` and `cbits/`; the
header credits Bun's unsafe-audit method). I ran it with an output path under `research/tmp/` so no repo
file changed. Results (MEASURED):

- **22,311 sites**: 20,583 blocks, 1,217 fns, 386 extern, 124 impl, 1 trait. 26.66 per 1,000 total
  lines (the census counts all lines, 836,910).
- By lane tier (the census routes crates to the safety tool that covers them): pure-miri 34 crates,
  18,421 sites (29.98 per kLOC); ffi-asan 11 crates, 3,792 sites (17.67); sys 16 crates, 98 sites (12.41).
- Block spans: 15,363 single-line, 2,533 of 2-5 lines, 2,488 of 6-20, 134 of 21-50, 57 over 50
  (max 526).
- Strict `SAFETY(origin/dest)` tags: 17 of 22,311 (the census marks the tagging as advisory, not yet
  enforced). Free-form `// SAFETY:` comments: 2,880 (MEASURED, `crate_stats.py`).
- **Trend** (DOCUMENTED earlier census snapshots + this run): 11,145 sites / 412,464 lines (27.02)
  at `b4845f4` 2026-07-10; 18,645 / 662,381 (28.15) at `47b2dbbb` 2026-08-25; 18,826 / 675,517 (27.87)
  at `b0685c0d` 2026-08-25; 22,311 / 836,910 (26.66) at HEAD. Density stayed flat as the code doubled.
- **Most unsafe is in tests (ESTIMATED).** Treating everything after a file's first `#[cfg(test)]`, and
  `tests/`, `tests.rs`, `*_tests.rs` files, as test code: 13,017 sites in tests vs 9,293 in non-test code.
  In `rustty-vt` it is 6,270 vs 1,714: Ghostty's tests manipulate pages, rows and cells through pointers,
  and the ported tests keep that shape as raw-pointer access.
- **Per-crate density (sites per 1,000 code lines, all sites):** inspector 163 (Dear ImGui FFI calls;
  1,147 of 1,155 are non-test), vt-c 128, apprt-c 92, core 74, xev 67, vt 46, renderer 36, config 4.7,
  font 2.4; z2d, terminfo, extra, xtask 0.
- **Inside `rustty-vt`** (non-test sites per kLOC): `ref_counted_set` 100, `alloc` 58, `terminal` 30
  (480 sites, mostly `terminal/edit.rs` 176 and `terminal/print.rs` 172), `screen` 22, `render` 20,
  `page` 16, `pagelist` 10. `pagelist` has 234 non-test vs 2,439 test sites.
- **Non-test hotspot files:** `rustty-inspector/src/widgets/pagelist.rs` 306,
  `rustty-apprt-c/src/lib.rs` 221, `rustty-vt-c/src/render.rs` 194, `inspector/widgets/terminal.rs` 186,
  `rustty-vt/src/terminal/edit.rs` 176, `rustty-core/src/surface/mouse_input.rs` 175,
  `rustty-vt/src/terminal/print.rs` 172.

**Raw-pointer design (DOCUMENTED).** `.port/audits/GHOSTTY_LIFETIMES.tsv` (created 2026-07-10 in commit
`8f6d781dc`; the header cites "Bun LIFETIMES.tsv heritage") classifies 315 pointer fields of the Zig
source: BORROW_FIELD 68, BORROW_PARAM 49, OWNED 34, FFI 31, SHARED 27, INTRUSIVE 25, ARENA 19,
GHOSTTY_C 17, STATIC 11, BORROWED_PTR 10, BACKREF 9, others 14. 143 of them are in `src/terminal`.
Data: `data/codebase-lifetimes-classes.csv`. Memory note `rust-mut-args-strongly-protected`
records the rule that came out of it: `&mut Page/Row/[Cell]` arguments re-read across
`assert_integrity` are Stacked Borrows UB, so those functions take raw-pointer arguments.

**Miri coverage.**
- Lanes (DOCUMENTED `docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md`): G6 Stacked
  Borrows over every pure-Rust crate, G6c with debug assertions off over the
  `all(miri, debug_assertions)` set, G6t Tree Borrows over the pointer-heavy modules
  (vt page/pagelist/search/render/screen/terminal/snapshot/kitty, vt-c, renderer, xev, core). The doc
  states `rustty-vt` alone took about 8 h under the serial libtest G6 lane, which led to the move to
  `cargo miri nextest`. The 11 FFI crates (`FFI_CRATES` in `.port/tools/gates/check_leaks.sh:78`) go
  to ASan/LSan instead.
- Tests by lane (MEASURED): 6,016 `#[test]` in pure-miri crates, 1,188 in ffi-asan crates, 66 in sys.
- Opt-outs (MEASURED): 136 `cfg_attr(miri, ignore ...)` attributes in 43 files (1.9% of tests), mostly
  vendored C or file I/O (rustty-os 33, rustty-vt 31); 126 `all(miri, debug_assertions)` sites;
  `.port/phase-2/MIRI_EXCEPTIONS.md` lists the PageList tests excepted for runtime with four criteria
  (each must also pass natively and under ASan, and each method must be covered by a lighter test that
  does run under Miri).
- 33 standalone Miri reproduction harnesses under `.port/phase-*/mirihar-*` (27 in phase-2), used
  to prove aliasing fixes on isolated non-FFI code (memory `shaper-hook-sb-clean-interleave`).
- Real UB Miri found (DOCUMENTED, lanes design doc): the search subsystem's stale Screen aliases
  (9 tests), the N437 render-thread overlapping references, the WS7 Phase E uninitialized page mapping,
  N391, and 10 violations that blanket ignores had hidden (WS19 T6). Bug counts belong to the bugs
  dossier; listed here as context.

---

## 7. Comment fidelity

Data: `data/codebase-comment-coverage-g3b.csv`.

- **G3** (`check_comments_fidelity.py`) compares comment line counts per crate. **G3b**
  (`check_comment_coverage.py`) is content-aware: every distinctive Ghostty comment (5 or more word tokens
  after normalization and the ghostty -> rustty rename) must fuzzy-match a port comment (substring, or
  difflib ratio >= 0.60; below 0.45 is RED). G3b was added because G3's aggregate count let a dropped
  comment in `rustty-build-config` go unnoticed (script docstring).
- Finale at the pin (DOCUMENTED `.port/phase-8/w0/g3b-strict-finale-at-a60e9e2a5.txt`): **34,345**
  distinctive reference comments over 29 crates, **0 missing**, 96 in the review band. Largest: vt 14,929,
  font 3,365, config 2,906, apprt 2,623, renderer 1,847, termio 1,228, core 1,257, input 1,113.
- After an upstream advance the gate goes RED until the delta is ported: at the start of Phase 8
  `vt-c` showed 797 distinctive / 62 missing (`.port/phase-8/w0/g3b-strict-after-at-a60e9e2a5.txt`).
- **Not carried:** `.port/parity-closure/COMMENT-NOT-CARRIED.tsv` has 79 rows (0.23% of the checked
  set): 73 `no-counterpart` (the code they describe is a recorded non-port, for example Zig-only
  `std_options_debug_io` in `lib_vt.zig`, the Windows DLL global-constructor workaround, or an unsound
  `getMut` recorded as NON-PORT N281) and 6 `gag-prose` (jokes dropped; where a joke wrapped a fact, the
  fact is kept). By file: `lib/` 39, `main_c.zig` 15, `lib_vt.zig` 9, apprt/gtk 16. Every row is
  validated mechanically (`g3b-not-carried-finale-at-a60e9e2a5.txt`: "all 79 row(s) prove themselves").

---

## 8. Third-party code

Data: `data/codebase-third-party-sys.csv`, `data/codebase-language-mix.csv`.

- **Same C libraries, no pure-Rust substitutes (R3).** 16 `-sys` crates vendor the C/C++ sources Ghostty
  fetches as Zig packages: freetype 162,181 lines, libxml2 140,539, harfbuzz 115,927, highway 113,885,
  glslang 79,514, oniguruma 78,513, wuffs 73,304, dcimgui 68,384, spirv-cross 49,383, fontconfig 39,705,
  simdutf 38,536, libpng 24,413, breakpad 22,338, sentry 20,848, zlib 17,445, gettext/libintl 13,544.
  Total 1,058,459 C/C++ code lines vs 4,100 lines of Rust wrapper + `build.rs` (3,752 `build.rs` lines).
  Ghostty's `pkg/` wrappers for the same libraries are 6,078 Zig lines (MEASURED).
- Six `build.rs` files (fontconfig, dcimgui, gettext, libpng, oniguruma, libxml2) reference a
  `config.h` (MEASURED `grep -l config.h crates-sys/*/build.rs`). The pattern of generating a
  CMake-configured header into `OUT_DIR` is recorded in memory `sys-generated-config-header`; which
  crates use it was not checked one by one.
- **Zig dependency packages ported to Rust** (not bound over FFI, since they are Zig source):
  libxev to `rustty-xev` (30,372 Rust / 17,239 Zig), libvaxis 0.6.0 to `rustty-vaxis` (13,862 / 14,206),
  z2d subset to `rustty-z2d` (9,907 / 19,812; pure Rust, no deps, 0 `unsafe`, pixel-exact against
  Ghostty's sprite goldens; commit `0467b4db9` 2026-06-30), zf to `rustty-zf` (1,423 / 664),
  plus `rustty-wuffs` and `rustty-oniguruma` safe wrappers. `rustty-zigimg` re-homes the six zigimg
  operations libvaxis uses onto the existing wuffs and zlib codecs instead of porting zigimg's 15
  format modules (Cargo.toml comment, DOCUMENTED).
- crates.io dependencies are few: 37 distinct direct deps, mostly platform bindings (objc2-*, gtk4,
  wayland-*, gl, khronos-egl) plus `allocator-api2`, `log`, `libc`, `hashbrown`, `indexmap`,
  `bitflags`, `serde_json`; build: `cc`, `bindgen`, `pkg-config`, `gl_generator`, `wayland-scanner`.

---

## 9. Growth over time

Data: `data/codebase-growth-weekly.csv`, `data/codebase-growth-weekly-per-crate.csv` (MEASURED,
`git rev-list -1 --before=<day> HEAD` then `git grep -c '' <sha> -- crates/*.rs crates-sys/*.rs xtask/*.rs ':(exclude)*/vendor/*'`;
these are physical lines including comments, so they track the census line count, not tokei code).

| Week ending | Rust lines | `#[test]` | Crates with Rust |
|---|---:|---:|---:|
| 2026-06-21 | 33,040 | 314 | 30 |
| 2026-06-28 | 243,886 | 3,005 | 31 |
| 2026-07-05 | 376,276 | 3,471 | 37 |
| 2026-07-19 | 510,309 | 4,033 | 43 |
| 2026-08-02 | 584,068 | 4,752 | 44 |
| 2026-08-30 | 694,928 | 5,786 | 44 |
| 2026-09-27 | 791,380 | 6,837 | 45 |
| 2026-10-08 | 836,905 | 7,270 | 45 |

The first full week produced 244k lines and 3,005 tests: the VT engine came first (port order in
`Cargo.toml`, leaves first, vt at position 11). Later growth follows the upstream resync phases and the
GUI tiers.

---

## Proposed visualizations

1. **Treemap: where the Rust lives.** `data/codebase-treemap.json` (tier -> crate, value = code lines).
   Shows that one crate, `rustty-vt`, is 34% of the code, and that ported Zig dependencies (58k) are
   larger than the whole font or rendering tier. Gives the reader a map before the details.
2. **Paired horizontal bars (or dumbbell): Zig vs Rust per module.**
   `data/codebase-zig-vs-rust-per-module.csv` (`zig_code`, `rust_code`, sorted by size; annotate
   `code_ratio`). Shows the 1.5x-2.9x expansion and the subset-port exceptions (z2d 0.5x).
3. **Scatter on log axes: Zig tests vs Rust tests per module, with the y = x line.**
   `data/codebase-test-counts-per-module.csv`. All 211 points sit on or above the line, which is the
   test-count gate shown as a picture.
4. **Layered DAG: crate dependency graph.** `data/codebase-crate-graph.json`; x or y = `depth`, color =
   `tier`, node size = `rust_code`; highlight the `layering_decisions` edges. Pair it with the Ghostty
   side drawn as a single 345-file SCC blob (number from `.port/cycle-analysis.md`).
5. **Bar chart: unsafe density per crate, stacked test vs non-test.**
   `data/codebase-unsafe-per-crate.csv` (`sites_in_non_test_code`, `sites_in_test_code`,
   `sites_per_kloc_code`). Shows that FFI boundary crates (inspector, vt-c, apprt-c) carry the density
   and that most of vt's unsafe is test code.
6. **Stacked bars inside rustty-vt: unsafe by area.** `data/codebase-vt-unsafe-by-area.csv`. The
   page/pagelist/screen/terminal areas are where Ghostty's pointer-heavy design lives.
7. **Line chart: unsafe census over time.** `data/codebase-unsafe-census-timeseries.csv` (sites and
   total lines on two series, density as a flat third). The flat density as code doubled supports the
   "monotone density" rule (R9) in `count-unsafe.sh`.
8. **Area chart: codebase growth.** `data/codebase-growth-weekly.csv` (Rust lines and test functions per
   week) with a horizontal reference line at the Zig total (302,669 code lines, or about 360k physical
   lines). Stacked-per-crate variant from `data/codebase-growth-weekly-per-crate.csv`.
9. **Iceberg or proportional bar: vendored C vs Rust glue.** `data/codebase-third-party-sys.csv`:
   1.06M lines of C/C++ under 4,100 lines of Rust per library. Makes the "same C libraries" rule concrete.
10. **Coverage strip: comment fidelity.** `data/codebase-comment-coverage-g3b.csv`: 34,345 checked
    comments as a per-crate bar, with a 79-unit callout for not-carried (73 no-counterpart, 6 gag-prose).
11. **Parity table/pill row: C ABI.** `data/codebase-c-api.csv` (35/35 headers, 208/208 functions,
    79/79 sentinels, 90/90 embedding entrypoints).
12. **Side-by-side stacked bars: language mix.** `data/codebase-language-mix.csv` (Zig/Swift/C headers
    vs Rust/Swift/C headers/vendored C/verification tooling). Shows that the Swift app is the same size
    and that the port added 33k lines of verification tooling.
13. **Bar: pointer-field lifetime classes.** `data/codebase-lifetimes-classes.csv` (315 Zig pointer
    fields by ownership class). A good lead-in to the strategy section about LIFETIMES.tsv.

## Gaps

- **WASM binary size / C library sizes not measured.** No release artifacts were in `target/`, and I did
  not build (read-only brief). To get them: `cargo build -p rustty-vt-c --release --target wasm32-unknown-unknown`
  and the matching `zig build` in the oracle.
- **The z2d version header may be stale.** `rustty-z2d` says "z2d 0.10.0 subset"; Ghostty's pin uses
  z2d 0.12.1. The Zig LOC baseline I used is 0.12.1, so the 0.5x ratio partly reflects version drift.
  Check whether later phases resynced z2d before quoting a version.
- **Test vs non-test unsafe split is a heuristic** (first `#[cfg(test)]` onward). The total reconciles
  to the census within 1 site (22,310 vs 22,311).
- **The `#[test]` regex** counts only plain `#[test]` attributes at line start (7,270). Tests built with
  other harness macros, Swift XCTests and shell gate self-tests are not included.
- **The input/VT mapping is approximate.** The 9 `src/input` files assigned to the VT engine are the
  set in `crates/rustty-vt/src/input/`. Some inline test counts may sit in a different bucket than the
  gate's own mapping; the gate's per-module CSV is the authoritative test comparison.
- **The Zig LOC ratio excludes** Ghostty's `build.zig` at the repo root, `pkg/` wrappers for C
  libraries (6,078 Zig lines, compared separately in section 8) and `uucode`, which Rustty replaced
  with generated tables.
- **Miri pass/fail status at HEAD was not rerun** (multi-hour lanes). Lane structure and opt-out counts
  are measured or documented. `.port/phase-8/w0/gates-finale-summary-at-a60e9e2a5.txt` has no G6
  or Miri line, so the lane status must come from the phase-7 W12 long-run records (memory
  `phase-7-resync-in-progress`: "Miri-nextest + sanitizer matrix + G8 strict").

---

## Fact-check

Independent re-derivation, 2026-10-08, Rustty `1d22ba80d`, oracle `a60e9e2a5`. Methods differ from the
dossier where possible: tokei JSON per path (not `module_map.py`/`crate_stats.py`), perl/grep over
`git ls-files` (not the census script), `cargo metadata` re-parsed with my own edge/depth code, CSV/TSV
re-parsed directly.

| # | Fact | Verdict | Evidence / correction |
|---|---|---|---|
| 1 | 302,669 Zig vs 507,628 Rust (1.68x); VT 116,461 vs 174,265 | **Confirmed, one wording fix** | Zig: src 249,453 + libxev 17,239 + vaxis 14,206 + z2d 19,812 + zf 664 + pkg/oniguruma 649 + pkg/wuffs 646 = 302,669. VT: `src/terminal` 132,158 - `terminal/c` 20,556 + input slice 4,859 = 116,461. Rust: `tokei crates xtask -e vendor` = 507,628; `crates/rustty-vt` = 174,265. Scope wording: `crates/` holds **44** crates; with `xtask` that is 45 packages, so write "44 crates + xtask" (or "45 non-sys packages"), not "45 crates + xtask". Side note: tokei gives 168,505 comment lines / 1,196 files for that scope, not 170,016 / 1,231 (1,231 is the count incl. `-sys` wrappers). |
| 2 | Ghostty src: 249,453 code, 56,319 comments, 599 files | **Confirmed** | tokei JSON; `find src -name '*.zig' | wc -l` = 599. |
| 3 | One 345-file SCC, 14 enumerated breaks | **Confirmed** | `.port/cycle-analysis.md:23` ("one 345-file SCC"); spec section 4 lists breaks 1-14. |
| 4 | 61 packages (45+16 sys), 313 edges (287/5/21), depth 18, 138 external | **Confirmed** | Own parse of `cargo metadata --no-deps`: 61 pkgs, 16 `-sys`, 313 edges {normal 287, dev 21, build 5}; longest non-dev chain 18 (`xtask`), 17 (`rustty`, `rustty-apprt-c`, `rustty-extra`); full metadata: 138 crates.io packages. |
| 5 | 4,423 Zig tests vs 7,270 Rust `#[test]`; gate 211 modules Zig 4,172 vs Rust 6,042, all rust >= zig | **Confirmed, scope note** | Zig: 4,423 with the `test "`/`test {` regex. Rust: 7,270 = `crates/` + `xtask` (7,204) + `crates-sys` wrappers (66); the dossier's own section 2 total (7,204) is the 44 crates + xtask scope, so keep the two numbers' scopes explicit. Gate re-run GREEN; summed its rows myself: 211 modules, 4,172 vs 6,042, 0 below. |
| 6 | G3b 34,345 comments / 29 crates / 0 missing; 79 not carried (73+6) = 0.23% | **Confirmed** | Summed the 29 `G3b [crate]` lines in the evidence file: 34,345, missing 0. TSV: 73 `no-counterpart` + 6 `gag-prose` = 79; 79/34,345 = 0.230%. |
| 7 | unsafe 22,311 (20,583/1,217/386/124/1), 26.66 per kLOC; flat since 2026-07-10 (27.02) | **Corrected (by 1)** | perl count over tracked `.rs` (excl. vendor/cbits): **22,310**, `unsafe extern` **385**. The extra site is an untracked build artifact the census script swept up: `crates/rustty-vt/fuzz/target/aarch64-apple-darwin/release/build/gettext-sys-*/out/bindings.rs` (it is also the lone `unsafe_extern` charged to rustty-vt in the per-crate CSV; rustty-vt really has 0 and 7,984 sites). Density: the 26.66 is per 1,000 *physical* lines (836,910, also artifact-inflated), not code lines. History (`.port/audits/unsafe-census-*.json`): 27.02 (b4845f4, 2026-07-10, 412,464 lines) -> 28.15/27.87 (2026-08-25) -> 26.66; physical lines x2.03, so "flat while code doubled" holds. |
| 8 | ~58% of unsafe in test code (13,017); vt 6,270 test vs 1,714 | **Confirmed (heuristic-sensitive)** | Same split rule re-implemented: 13,041 / 22,310 = 58.5%; vt 6,270 / 1,714 exactly. Caveat: if only `tests/` dirs and the first `#[cfg(test)]` count (not `*test*.rs` file names), it falls to 52%. Say "52-58% depending on how test-only module files are classified". |
| 9 | Density: inspector 163, vt-c 128, apprt-c 92, core 74, xev 67, vt 46, z2d 0 | **Confirmed** | Own counts / tokei code: 162.9, 127.6, 92.1, 73.6, 67.4, 45.8, 0. Note these are per code kLOC, a different denominator from fact 7's 26.66 (physical); the census JSON's per-crate densities are physical (inspector 88.08). Label the unit on every chart. |
| 10 | 6,016 pure-miri tests, 1,188 in 11 ffi-asan crates; 136 `cfg_attr(miri, ignore)` (1.9%); 33 harnesses | **Confirmed** | Split by `FFI_CRATES` (11 names, `check_leaks.sh`): 6,016 / 1,188. grep: 136 attributes; 136/7,204 = 1.89%. `ls -d .port/*/mirihar-*` = 33. |
| 11 | 315 pointer fields classified (68/49/34/31/27/25...), 143 in src/terminal; commit 8f6d781dc 2026-07-10 | **Corrected** | HEAD ledger has **314** data rows (316 non-comment lines minus header and 1 blank); class counts 68/49/34/31/27/25 and 143 `src/terminal` rows match. But the ledger as created at `8f6d781dc` (2026-07-10) had **216** rows (BORROW_FIELD 56, BORROW_PARAM 26, INTRUSIVE 24, OWNED 24, FFI 23; 93 in src/terminal). It grew via prospective rows (e.g. P7-W1 T2/T4/T5 sections) over 67 commits. Write "216 retroactive rows on 2026-07-10, 314 today". |
| 12 | 35/35 headers, 208/208 functions, 79/79 sentinels; embedding API 90/90 | **Corrected (embedding count)** | Headers 35/35, unique `*_name(` functions 208/208, `_MAX_VALUE = *_ENUM_MAX_VALUE` 79/79 confirmed. Embedding API: the pinned `ghostty.h` has **91** `GHOSTTY_API` declarations (91 unique `ghostty_*(` names) and `CABI_SYMBOLS.tsv` has **91** rows. "90" is the gate's floor literal (`check_symbol_manifest.sh:11,134`), not the count at a60e9e2a5. Use 91/91. |
| 13 | Alias layer deleted; 0 `ghostty_`, 546 `rustty_` refs in macos/Sources | **Confirmed, unit note** | `PARITY_NAMING.md:14-15,26`. grep: 0 `ghostty_`; 546 = *lines* containing `rustty_` (560 occurrences, 26 files). Say "546 lines". |
| 14 | Swift 26,843 vs 26,742; identical per area except Tests (+101); imported byte-identical then renamed | **Confirmed** | tokei code lines (198 vs 197 files). Per-file physical diff: only renamed paths plus `Tests/Helpers` (293 vs 196) and `Tests/NSPasteboardTests.swift` (304 vs 255). Commit `1395249e5` 2026-07-12 "import the reused ghostty Swift app (Branch A, byte-identical)", 262 files. |
| 15 | 1,058,459 vendored C/C++ over 16 crates vs 4,100 Rust; Ghostty pkg wrappers 6,078 | **Confirmed, definition note** | tokei vendor dirs: C 479,854 + C Header 234,185 + C++ 257,251 + C++ Header 86,139 + **Objective-C++ 1,030** = 1,058,459 (state that Obj-C++ is included). `tokei crates-sys -e vendor` = 4,100 Rust code. The 16 matching `pkg/` dirs total 6,078 Zig (all of `pkg/` is 11,021). |
| 16 | libxev 17,239->30,372; vaxis 14,206->13,862; z2d 19,812->9,907, 0 unsafe; zf 664->1,423 | **Confirmed, version caveat** | tokei on the hashes pinned in `build.zig.zon` (a second vaxis 0.6.0 copy in zig-pkg is 14,092; the pinned `BWNV_Crb...` is 14,206). z2d crate: 0 unsafe matches. Caveat already in Gaps: Rust z2d subset tracks 0.10.0 while the pin's z2d is 0.12.1, so the 0.50 ratio compares different versions. |
| 17 | 33,040 lines 2026-06-21; 243,886 after week 1 (3,005 tests); 836,905 lines, 7,270 tests 2026-10-08 | **Confirmed, unit note** | `git grep -c ''` at the last commit of 2026-06-21 = 33,040 and 2026-06-28 (`3f8a6c83e`) = 243,886; HEAD `cat | wc -l` over tracked non-vendor `.rs` = 836,905. These are physical lines incl. `-sys` wrappers, not tokei code lines; do not plot them on the same axis as the 507,628 code-line figure. |
| 18 | ~100 named gates in ci.sh; tooling 33,072 (Py 17,632, Sh 12,183) | **Corrected (gate count)** | tokei `.port/tools`: 33,072, Python 17,632, Shell 12,183 confirmed. Distinct gate IDs in `ci.sh` (`G[0-9]...` / `G-...`): **87**, one of which is a regex artifact (`G6.`), so **86**, of which 22 are `-SELF` self-tests of other gates. Say "86 gate IDs (64 gates plus 22 self-tests)", not "about 100". |
| 19 | vt->font, no font->vt; font-shaper above vt; config->renderer via RendererConfig (config.rs:42), no renderer->config; Command in termio | **Confirmed** | Own edge query: vt->font normal, font->vt none; font-shaper->vt normal, reverse none; config->renderer normal, reverse none; core->termio, reverse none. `pub trait RendererConfig` at `crates/rustty-renderer/src/config.rs:42`; impl in `crates/rustty-config/src/renderer_config.rs:67`; `crates/rustty-termio/src/command.rs` exists. |

### Corrections to carry into the article

1. Embedding API: **91/91**, not 90/90.
2. Lifetimes ledger: **216 rows on 2026-07-10 -> 314 now**, not "315 (commit 8f6d781dc)".
3. unsafe census: **22,310** (385 extern); the script counted one untracked fuzz build artifact. Consider excluding `**/target/**` in `count-unsafe.sh`.
4. Gates: **86 IDs**, not "about 100".
5. Scope wording: **44 crates + xtask** (45 non-sys packages).
6. Always label units: tokei code lines vs physical lines; per-code-kLOC vs per-physical-kLOC densities; 546 *lines* with `rustty_`.
