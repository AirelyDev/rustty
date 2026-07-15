# rustty — Crate Structure Design

**Date:** 2026-06-21
**Status:** Approved (alignment complete; pending final spec review)
**Scope:** The Cargo workspace / crate structure for the Zig→Rust port of Ghostty
(codename *rustty*). Defines Phase-1's workspace; Phase-0 finalizes the exact
file-level partition. Governed by the Parity Constitution + `.port/PORT_RULES.md`.

---

## 1. Goal & constraints

Port Ghostty (terminal emulator) from Zig to Rust at **100% functional parity,
zero regressions** (Goal 1), then optimize (Goal 2) without breaking parity.
Reference: `/Users/gael/projects/ghostty` @ `4749c4e…` (READ-ONLY, Zig 0.15.2).
Port target: `/Users/gael/projects/rustty`.

Binding amendments (see `.port/PORT_RULES.md`):

- **R1 — 100% full fidelity:** every line of code, **every comment** (preserved;
  retargeted to rustty names only where it names a ghostty-specific identifier),
  every `test{}` block (assertion-for-assertion), every assertion/edge case.
  No-loss, not no-addition: cycle-breaking glue may be added, never reference
  content removed/weakened.
- **R2 — Identity:** published artifacts are `rustty` (binary, `rustty_*` symbols,
  `rustty.h`, `TERM=xterm-rustty`, terminfo `rustty`, `~/.config/rustty`). A
  `identity-oracle` build feature flips all identity to the reference's values
  for byte-exact differential testing only.
- **R3 — Self-contained:** copy all needed C sources into rustty, build via
  `cc`+`bindgen`; zero dep on the ghostty tree (it's an external oracle only).
  Same C libraries; no pure-Rust substitution during parity phases.
- **R4 — Crate graph acyclic**, enforced by `.port/tools/cycle_check.py` as a gate.
- **R5 — Gates are invariant; never weaken to pass.**

---

## 2. Why a hybrid (not 1:1 with the reference)

The reference is effectively **one Zig module** (rooted at `src/main.zig`) plus
the `ghostty-vt` library module (rooted at `src/lib_vt.zig`). Directories under
`src/` are file-organization, not modules, so Zig permits pervasive cross-file
cycles. Measured (runtime imports, test-block edges excluded — Cargo permits
cyclic dev-deps): **`src/` is one 345-file strongly-connected component.** Rust
crates cannot be cyclic, so the port must cut acyclic boundaries. We follow the
grain: cut where the file graph already (nearly) separates, and break a bounded,
enumerated set of bottom-up "leak" edges. See `.port/cycle-analysis.md`.

---

## 3. The crate DAG

`rustty-*` names only. Edges point downward (a crate depends only on lower ones).
**FIRM** = proven cleanly separable. **TARGET** = carved incrementally as its
break (Section 4) lands, under the acyclicity gate, leaves-first.

### Tier 0 — leaves (FIRM)
| crate | from `src/` | notes |
|---|---|---|
| `rustty-util` | `quirks.zig, math.zig, fastmem.zig, file_type.zig, tripwire.zig` | assert/util leaf (after `quirks→font` break) |
| `rustty-build-config` | `build_config.zig` | comptime config + backend enums (defined here, not pulled up) |
| `rustty-identity` | (new) | single source of truth for R2 identity; `identity-oracle` feature |
| `rustty-datastruct` | `datastruct/` | |
| `rustty-unicode` | `unicode/` + generated tables | parity-replicated tables |
| `rustty-simd` | `simd/` | + `simdutf-sys`, `highway-sys` |
| `rustty-lib` | `lib/` | allocator utilities |
| `rustty-global` | (split from `global.zig`) | leaf process-state holder (OnceLock-style); high-level wiring moves to `core` |

### Tier 1 (FIRM)
| crate | deps |
|---|---|
| `rustty-os` | datastruct, util, global, build-config (+ platform `-sys`) |
| `rustty-render-types` | (extracted `renderer/size.zig`) → vt (terminal/size) |
| `rustty-crash` | os, util (+ `sentry-sys`/breakpad); global injected |

### Tier 2 — VT core (FIRM) — **crown jewel**
| crate | deps |
|---|---|
| `rustty-vt` | datastruct, unicode, simd, os, lib, util, render-types. All of `terminal/` + VT input slice (`key, key_encode, mouse, mouse_encode, paste, focus`). Regex/image/URL **and** the few app-mode terminal paths (`search/Thread→global`, `apc/glyph/Glossary→font`) go through runtime-swappable `sys` hooks — no hard oniguruma/png/font/global dep — matching `lib_vt.zig`. (Phase-0 confirms whether any such file is hoisted out instead.) |
| `rustty-vt-c` | vt. C ABI via **cbindgen** → `include/rustty.h` + `rustty/vt/*`, `rustty_*` symbols; outputs `librustty-vt.{a,so,dylib}`, `rustty-vt.wasm`, `rustty-vt.xcframework`. |

### Tier 3 — extracted bases (TARGET)
| crate | from | breaks |
|---|---|---|
| `rustty-cfg-fmt` | `config/formatter.zig` (+ `config/key.zig`) | config↔input |
| `rustty-args` | `cli/args.zig` (+ `CommaSplitter`, `diagnostics`) | config↔cli |
| `rustty-apprt-api` | apprt interface (`action.zig` types, surface target/msg traits) | apprt↔core |

### Tier 4 — font/render (TARGET)
| crate | deps |
|---|---|
| `rustty-font` | vt, os, datastruct, util, render-types (+ `freetype-sys, harfbuzz-sys, fontconfig-sys`, CoreText) ; config via DI |
| `rustty-input` | vt, datastruct, util, cfg-fmt, render-types |
| `rustty-inspector` | vt, font, util (+ `dcimgui-sys`); core via refs/trait |
| `rustty-renderer` | vt, os, datastruct, unicode, util, crash, font, render-types (+ opengl/Metal, `glslang-sys, spirv-cross-sys, wuffs-sys, libpng-sys, zlib-sys`); apprt via renderer-surface trait, inspector via hook |

### Tier 5 — config/io (TARGET)
| crate | deps |
|---|---|
| `rustty-config` | datastruct, os, util, vt, font, renderer, input, args, cfg-fmt (+ `oniguruma-sys`, themes) |
| `rustty-terminfo` | identity (entry name `rustty`) |
| `rustty-termio` | vt, crash, datastruct, os, util, terminfo; renderer/apprt via trait |

### Tier 6 — core + runtime (TARGET)
| crate | deps |
|---|---|
| `rustty-core` | `Surface.zig, App.zig, Command.zig, surface_mouse.zig` → vt, config, renderer, font, input, termio, os, crash, inspector, apprt-api, global |
| `rustty-apprt` | `apprt/` backends (gtk, embedded, macos) → core, apprt-api, vt, font, renderer, inspector, config, input, os (+ gtk-rs/gobject, objc, wayland) |

### Tier 7 — cli + binary + tools (TARGET)
| crate | deps |
|---|---|
| `rustty-cli` | `cli/` commands → config, vt, font, input, os, terminfo, args, apprt, core, crash |
| `rustty` (bin) | `main*.zig` entry → cli, apprt, core → produces the **`rustty`** binary |
| `rustty-synthetic` | `synthetic/` → vt, os (fuzz/test data) |
| `rustty-bench` | `benchmark/` → vt, simd, unicode |

### Supporting
- **`crates-sys/`**: `freetype-sys, harfbuzz-sys, fontconfig-sys, oniguruma-sys,
  simdutf-sys, highway-sys, glslang-sys, spirv-cross-sys, wuffs-sys, zlib-sys,
  libpng-sys, sentry-sys, dcimgui-sys` (+ platform binding crates). Each vendors
  copied C sources + `build.rs` (cc) + bindgen. (R3)
- **`xtask/`**: replaces `src/build/` orchestration — cbindgen headers, terminfo
  compilation, shell-integration/theme bundling, man pages, i18n, dist tarball,
  macOS xcframework/`.app`.

---

## 4. Cycle break-list (the contract)

Each break is a behavior-preserving structural change (R1: redistributes code +
adds minimal glue, never alters logic/comments/tests). Full evidence in
`.port/cycle-analysis.md`.

1. **`global.zig` → `rustty-global`** (leaf state) + wiring in `core`. *Master hub.*
2. `quirks.zig→font` — relocate the one font-quirk.
3. `build_config→{apprt,font,renderer}` — define backend enums in `build-config`.
4. `terminal/c` → `rustty-vt-c` crate.
5. `renderer/size.zig` → `rustty-render-types`.
6. `config/formatter.zig`(+`config/key.zig`) → `rustty-cfg-fmt`.
7. `cli/args.zig`(+`CommaSplitter`,`diagnostics`) → `rustty-args`.
8. apprt iface → `rustty-apprt-api` trait (core depends on iface).
9. `font↔config` — extract font-config types / DI.
10. `renderer→apprt` — renderer-surface trait.
11. `renderer↔inspector` — inspector hook in render `State`.
12. `inspector→core` — inspector takes refs/trait.
13. `termio→renderer/apprt` — termio-surface trait.
14. `os/open→apprt` — relocate / inject.

**Fallback (pre-agreed):** if breaks 11–13 prove uneconomical, group
`renderer+inspector+termio` into one `rustty-render` crate rather than force them.

---

## 5. Topological port order (leaves first)

`util, build-config, identity` → `datastruct, unicode, simd, lib, global` →
`os` → `render-types` → **`vt`** → `vt-c` → `crash` →
`cfg-fmt, args, apprt-api` → `font` → `input` → `inspector` → `renderer` →
`config` → `terminfo, termio` → `core` → `apprt` → `cli` →
`rustty`(bin)`, bench, synthetic`. (`xtask`, `-sys` crates: independent.)

Phase-0 emits the authoritative file-level `PORT_PLAN.md` from this.

---

## 6. Identity dual-profile (R2)

`rustty-identity` exports: app name, version strings, `TERM`/`TERM_PROGRAM`,
terminfo name, config dir, and VT query responses (XTVERSION, Primary/Secondary
DA). Default = `rustty`. Feature `identity-oracle` (parity harness only) →
reference values, so differential output matches byte-for-byte with no
canonicalization. Identity-equivalence gates dropped; behavioral gates retained.

---

## 7. Build features (from `src/build/Config.zig`)

`app-runtime-{none,gtk}`, `renderer-{opengl,metal,software}`,
`font-{freetype,coretext,fontconfig}`, `simd`, `sentry`, `i18n`, `wayland`,
`x11`, `identity-oracle`. Mirror reference defaults per target.

---

## 8. Parity harness (the three oracles)

1. **Ported assertions** — every reference `test{}` → Rust test (R1).
2. **Differential testing (primary)** — identical input to reference (external
   oracle, `identity-oracle` build / golden corpora) and port; diff canonical
   output via the vt formatters (Screen/PageList), config parse-then-serialize,
   input encode, render perceptual diff. Differential **fuzzing** stood up early.
3. **Conformance & goldens** — vttest/esctest, the c-vt-* programs, snapshot
   fixtures, terminfo.

Leak detection: tracking global allocator in tests and/or Miri (mirrors
`std.testing.allocator`).

---

## 9. Gates (must stay green)

- **Acyclicity:** `cycle_check.py` / `reach_scc.py` — no unexpected cross-crate SCC.
- **Ported tests + leak checks** per crate, ≥ reference coverage.
- **Differential + fuzz** clean vs reference.
- **Perf/memory/size/startup** not worse than `BASELINE.json` (Phase-0).
- **Fidelity critic (R1):** per-module comment/test/assertion inventory diff = 0.

---

## 10. Next step

This spec defines the Phase-1 workspace. Proceed to **Phase 0** (reference oracle
& baseline): build the reference, extract the full test/comment/assertion
inventory, assemble golden corpora, stand up the differential harness, capture
`BASELINE.json`, and emit the file-level `PORT_PLAN.md` confirming these crate
boundaries before any porting begins.
