# Completeness spec — WASM / web target (headless VT+font+config C-ABI lib)

PORTS the ghostty WASM module surface: `src/main_wasm.zig` (the headless C-ABI
library entry, NOT the GUI app) + its `comptime` imports (`os/wasm.zig`,
`font/main.zig`, `terminal/main.zig`, `config.zig.Wasm`, `App.zig.Wasm`) + the
browser Canvas font backend (`font/{face,shaper}/web_canvas.zig`) + the post-build
binary patcher (`src/build/wasm_patch_growable_table.zig`).

Status legend: DONE = ported & correct · BUG = ported but does not compile ·
COMMENT-DEFERRED = faithful body preserved as comments behind an off feature ·
UNPORTED = no rustty artifact · BLOCKED = cannot be faithfully ported now.

---

## 1. Oracle inventory (READ-ONLY, with LOC)

| Oracle file | LOC | rustty status |
|---|---|---|
| `src/main_wasm.zig` | 26 | UNPORTED — the module entry (needs `rustty-wasm` cdylib) |
| `src/os/wasm.zig` | 25 | DONE — `crates/rustty-os/src/wasm.rs` |
| `src/os/wasm/log.zig` | 45 | DONE — `crates/rustty-os/src/wasm/log.rs` |
| `src/os/wasm/target.zig` | 20 | DONE — `crates/rustty-os/src/wasm/target.rs` |
| `src/config/Wasm.zig` | 54 | **BUG** — `crates/rustty-config/src/wasm.rs` (does not compile; see §3.1) |
| `src/App.zig` Wasm block (~L578–604) | ~15 (all commented in oracle) | DONE — `crates/rustty-core/src/app.rs` L998+ (verbatim-comment port; the oracle body itself is fully commented out, so a comment port is R1-faithful) |
| `src/font/main.zig` L33–50 (wasm export block) | ~18 | COMMENT-DEFERRED (Atlas.Wasm / DeferredFace.Wasm, see below) |
| `src/font/Atlas.zig` `pub const Wasm` (L409+, has 1 test) | ~70 | COMMENT-DEFERRED — `crates/rustty-font/src/atlas.rs` L740+ |
| `src/font/DeferredFace.zig` `pub const Wasm` (L423+) | ~45 | COMMENT-DEFERRED — `crates/rustty-font-shaper/src/deferred_face.rs` L467 |
| `src/font/shaper/web_canvas.zig` | 305 | **BLOCKED** (B1) |
| `src/font/face/web_canvas.zig` | 561 | **BLOCKED** (B1 — the face the shaper needs) |
| `src/build/wasm_patch_growable_table.zig` | 269 (7 tests) | UNPORTED — clean host-testable win (→ xtask) |

Already-ported wasm C-ABI in rustty and audited complete:
- `rustty-os::wasm` (`shared_mem`, `alloc = std::alloc::System`), `::wasm::log`
  (`freestanding::log` → `extern "C" { #[link_name="log"] fn log }`, imported from
  the default wasm `env` module = faithful to Zig `extern "env" fn log`),
  `::wasm::target` (`Target::Browser`, `TARGET: Option<Target>`). All three are
  correct ports.
- `config/Wasm.zig` → `rustty-config/src/wasm.rs` behind the off-by-default
  `wasm` cargo feature. Faithful shape, but has a compile bug (§3.1).
- `App.zig.Wasm` → `rustty-core/src/app.rs` — the oracle body is 100% commented
  out (`app_new`/`app_free` are commented in ghostty itself), so the rustty
  verbatim-comment port is already complete & faithful.

---

## 2. Crate-structure decision

**Decision: a NEW thin `rustty-wasm` crate, `crate-type = ["cdylib", "rlib"]`,
as the wasm MODULE ENTRY (the Rust equivalent of `main_wasm.zig`).** It is a
top SINK (nothing depends on it), mirroring the `rustty-apprt-c` precedent. It
`[dependencies]`-references the crates whose C-ABI exports form the wasm surface
(`rustty-os`, `rustty-config` with `features=["wasm"]`, `rustty-vt`,
`rustty-core`; `rustty-font-shaper` deferred — see B1) and force-retains their
`#[no_mangle] extern "C"` exports so the wasm linker keeps them — the Rust map of
`main_wasm.zig`'s `comptime { _ = @import(...); }` block.

Rejected alternative: a `wasm` feature/target on an existing crate. There is no
natural host crate (the exports are spread across os/config/vt/core), and a
cdylib sink keeps the graph acyclic (R4) and the port order untouched.

**Force-retention seam (the `_ = @import` analog).** A `pub use` of a
`#[no_mangle]` fn does NOT re-export the symbol from a cdylib, and wasm
`--gc-sections`/`wasm-opt` drops unreferenced exports. Use a `#[used]` static
array of the exports' function pointers in `rustty-wasm/src/lib.rs` to force
retention across the link (documented, isolated seam). `std_options.logFn`
(main_wasm) → install `rustty_os::wasm::log::freestanding::log` as the global log
sink; `std_options.log_level` (Debug=.debug / ReleaseSmall=.warn / else .info) →
a `LOG_LEVEL` const gated on `cfg(debug_assertions)` / profile.

The crate builds as an **rlib on the host** (so the retention/logging logic is
unit-testable off-wasm); the cdylib + wasm-specific bits are
`cfg(target_arch="wasm32")`.

---

## 3. Work items

### 3.1 Fix `rustty-config/src/wasm.rs` compile bug (unblocked, do first)
The `wasm` feature does NOT currently compile. `config_load_string_` calls
`rustty_args::args::line_iterator(str_)` — no such function exists. The desktop
loader (`config_struct.rs::load_reader`) uses `LineIterator::with_filepath`; the
string form maps to `LineIterator::init`.
Fix: replace `rustty_args::args::line_iterator(str_)` with
`rustty_args::args::LineIterator::init(str_)`. Return type
`Result<(), rustty_args::args::ParseError>` is valid (`ParseError = args::Error`).
Acceptance: `cargo build -p rustty-config --features wasm` (host) is green.

### 3.2 Port `wasm_patch_growable_table.zig` → xtask (unblocked, clean win)
Pure LEB128/section-rewrite logic, 7 tests, no wasm target needed. Create
`xtask/src/wasm_patch_growable_table.rs` (`patch_table_growable`, `read_leb128`,
`uleb128_size` + the 3 test WASM byte fixtures) and wire a `main.rs` subcommand.
Port all 7 `test {}` assertion-for-assertion (`patches bounded table`, `already
growable unchanged`, `no table section unchanged`, `too short → InvalidWasm`,
`readLeb128 single/multi byte`, `uleb128Size`). Acceptance:
`cargo test -p xtask wasm_patch` = 7/7.

### 3.3 Author the `rustty-wasm` cdylib crate (skeleton, host-green)
- `crates/rustty-wasm/Cargo.toml`: `[lib] crate-type=["cdylib","rlib"]`; deps
  `rustty-os`, `rustty-config` (features `["wasm"]`), `rustty-vt`, `rustty-core`.
- `crates/rustty-wasm/src/lib.rs`: PORTS `main_wasm.zig` — the `#[used]`
  retention array over the config C-ABI exports, the `LOG_LEVEL` const, and the
  log-sink install. Add to workspace `members` + `[workspace.dependencies]`.
- Host acceptance: `cargo build -p rustty-wasm` (rlib) green.
- wasm32 acceptance is BLOCKED on B2 (below) — document it in the crate header.

### 3.4 (Optional, scoped-green milestone) Make `rustty-os` wasm32-clean
To prove the os/wasm helpers actually build for wasm32 in isolation, gate the 14
POSIX-only modules under `#[cfg(not(target_arch="wasm32"))]` in
`rustty-os/src/lib.rs`: desktop, file, homedir, hostname, locale, mach, open,
page_allocator, passwd, path, pipe, pty, resourcesdir, tempdir. Acceptance:
`cargo build -p rustty-os --target wasm32-unknown-unknown` green (exports only the
wasm helpers — faithful to Zig's lazy per-decl analysis of `os/wasm.zig`).

---

## 4. BLOCKED findings (precise follow-ups)

### B1 — `web_canvas` font shaper + face are UN-portable faithfully
`font/shaper/web_canvas.zig` is **stale/dead code in ghostty itself**:
- `Shaper.shape()` calls `run.group.group.faceFromIndex(run.font_index)`, but the
  current `run.zig` `TextRun` has NO `group` field (it holds `grid: *SharedGrid`
  + `font_index`), and `faceFromIndex` is DEFINED NOWHERE in the current ghostty
  font tree (`grep` for the def returns nothing). `Wasm.shaper_test_` also calls
  the stale 4-arg `self.runIterator(group, row, null, null)` (current signature is
  `runIterator(opts: RunOptions)`).
- The shaper needs `face.graphemeGlyphIndex`, which only exists on
  `font/face/web_canvas.zig` (561 LOC) — a browser Canvas font FACE that
  `@import("zig-js")` (an external Zig↔JS-interop package, NOT vendored under
  `~/.cache/zig/p`, with no Rust equivalent). `font/main.zig` even carries a TODO
  ("we need to modify the build config for wasm builds") on this path.

A faithful (R1) port would reference a `group.faceFromIndex` API that no longer
exists and a `zig-js` Canvas layer we do not have → it cannot type-check → fails
acceptance ("builds green for wasm32"). Making it compile requires FABRICATING
the deleted `Group`/`GroupCache`/`faceFromIndex` API and a `zig-js`-equivalent
Canvas FFI shim — a large R5-violating divergence.

**Follow-up decision (for the port stage):** (i) reconstruct the stale group API +
build a Canvas FFI shim — REJECT (R5 fabrication); (ii) keep font OUT of the wasm
module and wire `noop::Shaper` for wasm32 — a documented faithful-but-safe
divergence from the reference `.web_canvas` backend selection; (iii) leave
`web_canvas` permanently deferred (verbatim-comment port only, like Atlas.Wasm /
DeferredFace.Wasm already are). **Recommend (iii)**, with (ii) if/when a live
browser font path is actually required. Same blocker applies to `Atlas.Wasm`'s
`atlas_debug_canvas` and `DeferredFace.Wasm` (both touch the Canvas/`zig-js`
layer); their pure exports (`atlas_new/free/reserve/set/grow/clear`) are
portable, but the canvas ones are not.

### B2 — the wasm32 dependency tree does not compile whole-crate
The USEFUL wasm surface is `config_new`/`config_load_string`/`config_finalize`,
which need the full `Config` type → `rustty-config`, whose dep tree pulls
`rustty-renderer` (Metal/objc2/glslang-sys C++/spirv-cross-sys), `rustty-input`,
`rustty-font`, `rustty-vt`. In Zig, `@import` lazily analyzes only referenced
decls (App.Wasm is empty ⇒ almost nothing of App.zig is compiled for wasm). In
Rust, building a crate for `wasm32` compiles the ENTIRE crate. Measured:
`cargo build -p rustty-os --target wasm32-unknown-unknown` = 108 errors across 14
modules (libc, POSIX, mach, pty…). `rustty-config`/`rustty-core` are far worse
(renderer Metal/objc2/C `-sys`).

Proving `cargo build --target wasm32-unknown-unknown` green for the config/app
C-ABI surface therefore requires pervasive `#[cfg(not(target_arch="wasm32"))]`
gating across DOZENS of already-ported R1 files spanning os/config/renderer/
input/font/vt/core — a cross-workspace refactor well outside a clean isolated
wasm port, and a risk to those files' fidelity.

**Follow-up decision:** scope the wasm32-green target. Minimal, contained, and
achievable now = §3.4 (os-only wasm32-clean, proves the toolchain + os/wasm
helpers). The full config-surface wasm32 build is a separate, large,
explicitly-scoped workstream (gate renderer/input/font/vt/core off under wasm32)
and should be tracked as its own DEFERRALS entry, not smuggled into this port.

---

## 5. Exact build / test commands

```sh
# 3.1 config wasm feature compiles (host)
cargo build -p rustty-config --features wasm

# 3.2 the binary patcher, host unit tests (7/7)
cargo test -p xtask wasm_patch

# 3.3 the wasm module-entry crate, host rlib
cargo build -p rustty-wasm

# 3.4 (scoped-green) the os/wasm helpers actually build for wasm32
rustup target add wasm32-unknown-unknown   # already installed here
cargo build -p rustty-os --target wasm32-unknown-unknown

# FULL wasm surface — BLOCKED (B2), documented not-yet-green:
#   cargo build -p rustty-wasm --target wasm32-unknown-unknown
```

---

## 6. Hazards

- No SB/TB aliasing hazards in the portable items: config C-ABI (`Box::from_raw`
  round-trips), the LEB128 patcher (pure slices), os/wasm helpers (consts + an
  extern log call) — all Miri-clean on host.
- The web_canvas FACE (B1) uses `zig-js` extern JS canvas fns → an FFI path Miri
  can't execute (would be ASan-only) — moot, since B1 is not being ported.
- `#[no_mangle] extern "C"` retention via `#[used]` statics is a link-time
  concern, not a memory-safety one; verify by inspecting the emitted `.wasm`
  exports (`wasm-objdump -x`) once B2 unblocks the wasm32 build.

## 7. Summary

Deliver now (unblocked, all host-green): §3.1 config-wasm compile fix, §3.2 the
`wasm_patch_growable_table` xtask port (7 tests), §3.3 the `rustty-wasm` cdylib
skeleton (rlib host build), and optionally §3.4 (os-only wasm32-clean). Report
**BLOCKED** for B1 (web_canvas shaper+face — stale deleted `faceFromIndex`/`group`
API + un-vendored `zig-js` Canvas FFI) and B2 (whole-workspace wasm32-gating
required before the config C-ABI surface can build for wasm32), each with the
precise follow-up above.
