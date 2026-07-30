# WS14 — wasm / WebGL Renderer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close every wasm/WebGL ledger item Phase 2 still carries — O13a (`main_wasm.zig`), O14 (`Atlas.Wasm` + its 4-assert test), O15 (`App.zig` Wasm block), O16 (the `renderer/WebGL.zig` backend) — by (a) porting the four reference hunks at R1 fidelity, (b) turning `crates/rustty-wasm` into a **real `wasm32-unknown-unknown` cdylib artifact** (`rustty_wasm.wasm`) that publishes the `Atlas.Wasm` + `config.Wasm` C-ABI/JS surface the reference's `main_wasm.zig` retains, and (c) proving glyph output on a **WebGL2 canvas in a real Chrome**, under a **falsifiable, scriptable, CI-runnable gate** whose two negative controls have already been observed to fail. No item is left deferred.

**Architecture:** Three seams, in dependency order.

1. **The renderer arm (O16).** The reference `src/renderer/WebGL.zig` is a **2-line placeholder** (`//! Renderer implementation for WebGL in the browser.` + `pub const WebGL = @This();`) — see PREMISE CORRECTION §1. `renderer.zig:38-42` comptime-selects it for `build_config.renderer == .webgl`, which `renderer/backend.zig` returns for `wasm32` + `WasmTarget.browser`. The port already has `Backend::Webgl` (`crates/rustty-build-config/src/renderer.rs:15`) and already carries the selection switch as a verbatim comment (`crates/rustty-renderer/src/lib.rs:137-153`). WS14 adds `crates/rustty-renderer/src/webgl.rs` (the faithful 2-line unit-struct port) and the `.webgl` arm of the selection, so the switch is complete for all three arms.
2. **The wasm module (O13a/O14/O15).** The reference `main_wasm.zig` is a pure *retention* file: `comptime { _ = os/wasm.zig; _ = font/main.zig; _ = terminal/main.zig; _ = config.zig.Wasm; _ = App.zig.Wasm; }` plus `std_options`. The port (`crates/rustty-wasm/src/lib.rs`, 128 lines) already models retention by crate edges + `#[used]` fn-pointer statics, but retains only **3 of the 5** blocks (`os/wasm`, `terminal/main`, `config.Wasm`); `font/main.zig` (⇒ `Atlas.Wasm`) is missing and `App.zig.Wasm` is a stand-in `rustty-core` edge. WS14 completes the retention set, makes the crate a `cdylib`, and shrinks the wasm link surface so the artifact actually links freestanding.
3. **The browser consumer.** Upstream's WebGL backend does not exist, and by design does not need to: libghostty-vt's model is that **wasm consumers render themselves** (DEFERRALS.md's WASM entry records this reframing). So the WebGL *drawing* code belongs in the harness page (`example/wasm-webgl/`), consuming `atlas_*` (the texture) + the VT C-ABI (the cells) out of `rustty_wasm.wasm`. That is not a shortcut: it is exactly the architecture the reference's own wasm deliverable implies, and it is what makes the pixel gate possible.

**Tech Stack:** Rust 1.93.1 (pinned), `wasm32-unknown-unknown` (installed; `rustup target list --installed` → `aarch64-apple-darwin`, `wasm32-unknown-unknown`), Google Chrome (`/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`), node v25.6.0 (built-in `WebSocket` + `fetch` — the CDP driver needs **no npm dependency**; puppeteer/playwright are NOT installed and are NOT required), `python3 -m http.server` bound to `127.0.0.1` for offline serving, oracle git repo at `/Users/gael/projects/ghostty` (working tree pinned at `4749c4e93`; read beyond-pin state with `git -C /Users/gael/projects/ghostty show origin/main:<path>` — **never** `git checkout` there).

## Global Constraints (inherited from `2026-07-23-rustty-completion-ROADMAP.md`)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e93`); reference-only, no build-time dependency (R3).
- R2 product identity: rustty naming, per-token casing. **Exception, preserved deliberately:** the wasm C-ABI symbols the reference exports are *unprefixed* (`config_new`, `atlas_new`, `atlas_reserve`, …) because the JS host binds them by bare name; `crates/rustty-config/src/wasm.rs:29-33` already records this decision. Keep them unprefixed. The rustty-only helper exports keep their `rustty_wc_*`/`rustty_wasm_*` prefixes.
- R4: the crate graph stays acyclic (`.port/tools/gates/check_acyclic.sh`). **`rustty-font` must never gain a `rustty-vt` or `rustty-font-shaper` edge** (memory: apc-glyf-font-layering-d1 — rustty-font is PURE and BELOW vt). Task 4 therefore declares its Canvas imports *locally* rather than depending on the shaper.
- R5: native/macOS artifacts must stay behavior-identical. Every change here is either `#[cfg(target_arch = "wasm32")]`, behind an off-by-default `wasm` cargo feature, or a **default-on** feature whose default keeps the native build byte-identical (Task 6).
- DG5: the C-ABI panic policy is **ABORT**, enforced by the ABI string (plain `extern "C"`, gated by `no_c_unwind_in_c_api_sources` in `crates/rustty-apprt-c/tests/panic_policy.rs:91`). Do **not** write per-function catch-unwind wrappers. Task 4 Step 6 extends that gate's crate list to the two crates gaining C-ABI surface.
- Out-of-range C integers are never transmuted into Rust enums: inbound params narrow via an explicit `match` (`atlas_new(size, format: u8)` — Task 4), values returned from C with no narrowing point use `repr(transparent)`.
- LTO stays banned. Do not add the `libc` crate. Do not add npm dependencies.
- Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String` (memory: zig-byteslice-to-rust-rawbytes). The atlas data path is `&[u8]`/`*const u8` throughout.
- Strip snark/gag editorializing when porting reference prose (memory: strip-ghostty-gag-prose). `Atlas.zig`'s `Wasm` block is dry; nothing to strip. Keep its "If you're copying this file out to a separate project…" comment — that is substantive, not gag.
- Ledger discipline: strike/repair `.port/phase-2/DEFERRALS.md` + `VERIFICATION_DEBT.md` rows in the same PR (Task 11). **Six** rows are wrong; all six are enumerated below with line numbers.
- Commit style: `subsystem: summary (WS14 T<n>)`. Branch `ws14-wasm-webgl`; merge to main only with `cargo build --workspace` + `cargo test --workspace` green on macOS **and** the wasm lane green.
- **Browser automation rule:** never trigger `alert`/`confirm`/`prompt` or any modal dialog from harness pages or injected scripts — a modal blocks the automation channel and wedges the session. The harness logs to a `<pre>` element and returns JSON; it must never call `window.alert`.

---

## PREMISE CORRECTION (read before Task 1)

The ROADMAP row says: *"Port the WebGL backend + wasm entry (`main_wasm`, `Atlas.Wasm` C-ABI/JS surface); build `wasm32` target green."* Four of its premises are wrong, and one ledger row will actively mislead a reviewer into thinking WS14 is already finished. All findings below were verified at plan-authoring time against the pinned oracle and against `main` @ `4069662`.

### §1 — "Port the WebGL backend" — there is no WebGL backend to port

`src/renderer/WebGL.zig` in the pinned oracle is **2 lines total**:

```
//! Renderer implementation for WebGL in the browser.
pub const WebGL = @This();
```

`wc -l /Users/gael/projects/ghostty/src/renderer/WebGL.zig` → `2`. It is **identical on the oracle's `origin/main`** (`git -C /Users/gael/projects/ghostty show origin/main:src/renderer/WebGL.zig` → the same 2 lines), so this is not a pin artifact. It is referenced in exactly two places (`src/renderer.zig:20`, `src/renderer.zig:41`) and has no methods, no `test {}`, no shaders, no draw path. `grep -rn webgl /Users/gael/projects/ghostty/src` returns 3 hits: `renderer.zig:41`, `backend.zig:8`, `backend.zig:16`.

**Consequence:** O16 is a ~15-line task (Task 2), not a backend implementation. Any plan that budgets a WebGL *renderer* port is porting code that does not exist — and writing a real WebGL backend from scratch would be an R1 **violation** (invention with no reference). The genuine WebGL work belongs in the browser *consumer* (Task 8), which is the reference's own stated model for wasm.

### §2 — `DEFERRALS.md:4363` "WASM/web target — ✅ DONE (VT C-ABI wasm), 2026-07-15" — true, but covers something else entirely

This is the row that can make a reviewer believe WS14 is finished. Precisely what it covered, per its own body (`.port/phase-2/DEFERRALS.md:4363-4459`), all of which was re-verified as still true:

| Claimed | Verified now |
|---|---|
| `target/wasm32-wasip1/release/rustty_vt_c.wasm` (the `ghostty-vt.wasm` analog), node-WASI instantiated, `rustty_type_json()` correct | build script present (`.port/tools/build-vt-wasm.sh`, `build-wasm.sh`); **`wasm32-wasip1` is NOT an installed rustup target on this machine** — this lane needs `rustup target add wasm32-wasip1` + the `.port/tools/zig-{cc,cxx,ar}-wasm.sh` shims |
| A **true freestanding** `target/wasm32-unknown-unknown/release/rustty_web_canvas.wasm`, "936 KB, 184 exports, imports modules `["env"]` only" | **REPRODUCED, green today.** `cargo build -p rustty-web-canvas --target wasm32-unknown-unknown --release` → `Finished in 8.59s`; artifact 1,119,476 bytes; `WebAssembly.Module.imports` → **14 imports, all module `env`, zero wasi**; **199 exports** (the row's 184 is stale-low, not wrong-in-kind) |
| `web_canvas.zig` face+shaper ported and "verified in real Chrome" | **REPRODUCED LIVE.** Served the repo over `127.0.0.1:8731`, loaded `example/wasm-web-canvas/index.html` in Chrome: 7 glyphs rasterized through the browser Canvas with distinct metrics (`R 21×25`, `u 18×19`, `s 18×20`, `t 18×24`, `y 20×26`, `! 14×25`), atlas blit 130×29, shaper 3 cells, `rustty_terminal_new(80×24)` + `rustty_terminal_vt_write(45 bytes)` both `rc=0`, zero console errors |
| The VT wasm export block (`rustty_wasm_*` allocator helpers), `xtask wasm-patch-growable-table` | present |

**What it did NOT cover, and never claimed to:** `renderer/WebGL.zig` (O16), `main_wasm.zig`'s artifact (O13a), `Atlas.Wasm` (O14), `App.zig`'s Wasm block (O15), and **any GPU/WebGL path at all** — every pixel in the existing harness is drawn by the 2D Canvas, not WebGL. The row is *narrowly* true and *broadly* misleading. Task 11 rewrites its heading to `WASM/web target — VT C-ABI wasm + web_canvas DONE (2026-07-15); WebGL/`main_wasm`/`Atlas.Wasm` closed by WS14` so a grep can no longer read as "wasm is finished".

### §3 — `main_wasm.zig` is "NOT ported / DEFERRED" (`DEFERRALS.md:3065`, restated at `:4067` as the C6 "app-core over-claim CORRECTION") — **stale in the *other* direction**

`crates/rustty-wasm/src/lib.rs` exists on `main`, 128 lines, header `PORTS: ghostty src/main_wasm.zig`, `PORT STATUS: ported (host-buildable rlib…)`. It ports the `comptime` retention block (as crate edges + a `#[used]` fn-pointer tuple over the four `config_*` exports), `std_options`, and the log-level switch. So `:4067`'s "They are NOT ported — they are DEFERRED" is **false for `main_wasm.zig`** (it remains correct for `main_gen.zig` / `main_build_data.zig`, which WS14 does not own).

The honest residual is narrower and sharper: **(a)** the retention set is incomplete — `_ = @import("font/main.zig")` has no counterpart edge (so `Atlas.Wasm` joins nothing) and `_ = @import("App.zig").Wasm` is modelled by a `rustty-core` dependency; **(b)** the crate is `rlib`-only, so **no `.wasm` artifact is ever produced**; **(c)** it does not build for `wasm32-unknown-unknown` at all. Verified:

```
$ cargo check -p rustty-wasm --target wasm32-unknown-unknown
error: no default backend for this target       --> crates/rustty-xev/src/backend.rs:79
error: no candidate backends for this target    --> crates/rustty-xev/src/backend.rs:119
error: failed to run custom build command for `zlib-sys`
```

### §4 — the `rustty-core` retention edge is not just heavy, it is *unfaithful*

The reference's `_ = @import("App.zig").Wasm` retains this (oracle `src/App.zig:577-604`):

```zig
pub const Wasm = if (!builtin.target.isWasm()) struct {} else struct {
    const wasm = @import("os/wasm.zig");
    const alloc = wasm.alloc;
    // export fn app_new(config: *Config) ?*App { ... }   ← ALL COMMENTED OUT
    // ...
};
```

An **empty struct with every export commented out**. Retaining it emits **zero symbols** and pulls **zero App implementation** into the wasm module. The port's `rustty-core` edge therefore over-retains by an entire application tier, and that edge is precisely what makes the wasm build impossible: `cargo tree -p rustty-wasm --target wasm32-unknown-unknown -i zlib-sys` →

```
zlib-sys ← freetype-sys ← dcimgui-sys ← rustty-inspector ← rustty-core ← rustty-wasm
```

i.e. the Dear ImGui inspector's FreeType/zlib C stack is being dragged into a freestanding wasm link because of a retention edge whose reference counterpart retains an empty struct. **Dropping the `rustty-core` edge is the *more* faithful choice**, and Task 5 closes O15 where it belongs — in `rustty-core/src/app.rs` (which already preserves the block verbatim at `app.rs:998-1007`) — as a *complete* port of an empty struct rather than a deferral.

### §5 — `Atlas.Wasm` (O14) cannot be ported "as written": the pinned oracle deleted its dependencies

`.port/phase-2/DEFERRALS.md:4209` ledgers the block's one test per-assertion and says porting it "MUST port this test with all 4 asserts live". Two of those asserts call `wasm.isHostOwned` / `wasm.toModuleOwned`, and `atlas_reserve` calls `wasm.toHostOwned`. **None of those functions exist in the pinned oracle.** `grep -rn 'toHostOwned\|isHostOwned\|toModuleOwned' /Users/gael/projects/ghostty/src` finds only *call sites* (`Atlas.zig:443,570,571`, `face/web_canvas.zig:557`) plus one doc-comment mention (`os/wasm.zig:19`) — no definitions. Oracle `src/os/wasm.zig` is 25 lines: `shared_mem` + `alloc`, nothing else.

Cause, pinned down: commit **`9dc2e5978` "lib-vt: enable freestanding wasm builds (#9301)" (2025-10-21)** deleted 90 lines from `src/os/wasm.zig` — the `allocs: AutoHashMapUnmanaged([*]u8, usize)` tracker, `malloc`, `free`, `toHostOwned`, `isHostOwned`, `toModuleOwned`, and their 2 tests. `Atlas.Wasm` and `face/web_canvas.zig` were left dangling and **do not compile upstream today**; nothing analyzes them because `main_wasm.zig` is referenced nowhere in `build.zig`.

This is the same archaeology the port already accepted once: DEFERRALS `:4402-4415` records `web_canvas.zig` as *resurrected on user request* and modernized. WS14 does the same, one layer down: **Task 3 resurrects the deleted tracker** into `rustty-os::wasm` from `git -C ../ghostty show 9dc2e5978^:src/os/wasm.zig`, with its 2 reference tests, and cites the commit in-source. Then Task 4's `Atlas.Wasm` port has real `is_host_owned`/`to_module_owned` and all 4 asserts run — natively, under `cargo test`, not only in a wasm build (the Rust tracker is target-agnostic; only the reference's `comptime` isWasm guard forced wasm-only analysis).

### §6 — the ~40 "Target: Phase-3" labels are NOT a wasm worklist

`grep -rn 'Target: Phase-3' .port/` hits are a **pre-WS codename** for the GPU / xev / inspector tiers, all of which landed (WS6, the D6 xev port, the GTK OpenGL3 inspector). They are unrelated to the new `.port/phase-3/` directory (upstream resync assessments). **Do not mine them for WS14 scope.** WS14's scope is exactly the four O-items plus the browser gate.

### §7 — the renderer tier's stance, and `VERIFICATION_DEBT.md:157`

`.port/phase-3/assessments/F-ui-tiers.md` establishes the renderer is **render-complete OFFSCREEN**, with only live-windowing `Renderer::init` outstanding (W-ATTACH, itself since RESOLVED at `DEFERRALS.md:4229`), and it disproves "GPU is unverifiable" by designing a real headless `MTLCreateSystemDefaultDevice` unit test. WS14 takes the identical stance for WebGL, and goes further: the gate is not merely headless-capable, it is **byte-identical between headless and live GPU Chrome** (Task 9 Step 5 evidence).

`VERIFICATION_DEBT.md:157` says *"Out of scope by decision (not debt): the WebGL/wasm apprt (owner decision, 2026-07)"*. That row is about the **browser `apprt`** (`apprt/browser.zig`, 4 loc — already ported at `crates/rustty-apprt/src/browser.rs`) growing a full App/Surface browser runtime. WS14 does **not** re-open that: no `apprt` browser runtime is built here. Task 11 sharpens the row's wording so it cannot be read as excluding WS14's scope.

### §8 — coordination, not duplication: `include/rustty.h` `Opaque`

`.port/phase-3/assessments/C-capi-headers.md:305-331` reports `include/rustty.h` does not compile as standalone C: `unknown type name 'Opaque'` at `:2455` / `:2457` (`rustty_wasm_alloc_opaque` / `rustty_wasm_free_opaque`), because `Opaque = *mut c_void` lives in a sibling crate that `crates/rustty-vt-c/cbindgen.toml`'s `after_includes` never pinned. **WS19 T3 owns this fix** (`docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md:405-459`, commit message already drafted there) and WS19 is **plan-only, unimplemented** (`main` @ `4069662` = "merge: Phase 2 closure audit + WS19 closeout plan"). WS14 must **not** touch `cbindgen.toml` or `include/rustty.h` for `Opaque`. Task 12 Step 2 only *asserts the current state* and records the cross-reference.

### Summary table

| ROADMAP / ledger premise | Reality | WS14 task |
|---|---|---|
| "Port the WebGL backend" | oracle `WebGL.zig` = 2-line placeholder, same on `origin/main` | **T2** (~15 lines) |
| `D:4363` "WASM/web target ✅ DONE" | true for VT C-ABI + web_canvas (reproduced live), silent on all four O-items and on WebGL | **T11** rewrite |
| `D:3065` / `D:4067` `main_wasm.zig` DEFERRED | **ported** (`crates/rustty-wasm`, 128 L); residual = incomplete retention set + no cdylib + no wasm32 build | **T5/T6/T7** |
| `D:1894` item 5 `App.zig` Wasm block deferred | reference block is an **empty struct**, all exports commented; verbatim-preserved at `app.rs:998-1007` | **T5** (complete the port, strike the deferral) |
| `D:4209` `Atlas.Wasm` "port with all 4 asserts live" | its `wasm.toHostOwned`/`isHostOwned`/`toModuleOwned` were **deleted upstream** by `9dc2e5978` | **T3** (resurrect) → **T4** |
| `D:571` WebGL backend "whole-backend follow-up port" | there is no backend | **T2** |
| `VERIFICATION_DEBT:157` "WebGL/wasm apprt out of scope" | about the browser *apprt* runtime, not WS14 | **T11** clarify |
| "~40 Target: Phase-3 labels" | pre-WS codename for GPU/xev/inspector (all landed) | **not scope** |
| `include/rustty.h` `Opaque` | WS19 T3 owns it | **T12** cross-ref only |

---

## De-risking probes already executed (the plan is written against these results)

Everything below was **run** at plan-authoring time on this machine. Reproduce Step-for-Step in Task 1; if any disagrees, STOP and re-scope.

1. **wasm32 freestanding build is green today.** `cargo build -p rustty-web-canvas --target wasm32-unknown-unknown --release` → `Finished` in 8.59s. Artifact: 1,119,476 bytes, **14 imports (all `env`, zero wasi)**, **199 exports** incl. `rustty_wc_atlas_{new,free,data,size}`, `rustty_wc_face_{new,free,render_glyph,debug_canvas}`, `rustty_wc_shaper_{new,free,shape}`, `rustty_wc_{alloc,free}`.
2. **`rustty-wasm` does NOT build for wasm32** — 3 errors (§3 above).
3. **The xev blocker is a 2-line cfg widening.** Patching `crates/rustty-xev/src/backend.rs`'s two `#[cfg(target_os = "wasi")]` arms (and the matching `not(any(...))` guard) to `#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]` makes `cargo check -p rustty-config --target wasm32-unknown-unknown` get **past xev entirely**. (Probe patch reverted; tree clean.)
4. **What remains after xev are five C/C++ `-sys` build scripts, and every one arrives through a single crate edge.** After the xev patch, `cargo check -p rustty-config --target wasm32-unknown-unknown` fails only on `wuffs-sys`, `spirv-cross-sys`, `oniguruma-sys`, `glslang-sys`, `sentry-sys`. Reverse-dependency chains (`cargo tree -i`):
   - `glslang-sys` ← `rustty-renderer` ← `rustty-config`
   - `spirv-cross-sys` ← `rustty-renderer` ← `rustty-config`
   - `wuffs-sys` ← `rustty-wuffs` ← `rustty-renderer` ← `rustty-config`
   - `sentry-sys` ← `rustty-crash` ← `rustty-renderer` ← `rustty-config`
   - `oniguruma-sys` ← `rustty-oniguruma` ← **`rustty-input`** ← `rustty-config` (also via `rustty-renderer`)

   So **two** edges gate all five: `rustty-config → rustty-renderer` and `rustty-input → rustty-oniguruma`. The first is confined to ONE file (`crates/rustty-config/src/renderer_config.rs` — the accessor-trait impl, memory: renderer-config-accessor-trait-acyclic); the second to ONE method (`crates/rustty-input/src/link.rs:92 oni_regex`). `rustty-config` already has an optional-`rustty-oniguruma` precedent (`Cargo.toml:32 oniguruma = ["dep:rustty-oniguruma"]`). Task 6 makes both edges default-on features. **No C is cross-compiled and no C dep is stubbed** — that is the whole point of choosing this design over teaching 5 build scripts to target wasm.
5. **Live Chrome works** (§2 above): harness page loads, 7 glyphs, 0 console errors, `WebGL2 available: true`.
6. **A WebGL2 pixel assertion against a CPU-computed expectation is exact.** Rendered the atlas as an `R8` texture, one NEAREST-filtered quad per glyph into a 128×64 canvas, `gl.readPixels`, counted ink (`R > 16`) per 32px cell column, and compared against ink counted directly in the atlas bytes for the same rects: **expectation `[217, 291]`, WebGL readback `[217, 291, 0]` — exact equality, `gl.getError() == 0`.**
7. **Both negative controls FAIL, as required** (numbers in the gate section below).
8. **The whole gate is scriptable headlessly with zero npm deps, and headless agrees exactly with the live GPU.** `--headless=new --remote-debugging-port=0 --user-data-dir=$(mktemp -d) --enable-unsafe-swiftshader`, port read from `DevToolsActivePort`, driven over node's built-in `WebSocket` via `Runtime.evaluate {returnByValue, awaitPromise}`. Headless result: `expect [217,291]`, `good [217,291,0]`, `offset [252,188,0]`, `black [0,0,0]`, `pass_good: true`, `neg_offset_fails: true`, `neg_black_fails: true`, renderer string `"WebKit WebGL"`. **Byte-identical to the live-GPU run.**

---

## The browser-validation gate (design + negative controls)

**Gate:** `.port/tools/gates/ws14_webgl_browser_gate.sh` (wrapper) → `.port/tools/gates/ws14_webgl_browser_gate.mjs` (driver) → `example/wasm-webgl/gate.js` (the in-page assertion body, shared verbatim with the harness page so the gate can never drift from what a human sees).

**Serving without network:** the driver spawns `python3 -m http.server <port> --bind 127.0.0.1` with `cwd` = repo root, so `fetch()` of the `.wasm` works (a `file://` page cannot). Nothing leaves loopback; Chrome runs with `--user-data-dir=$(mktemp -d)`, `--no-first-run`, `--no-default-browser-check`, `--disable-extensions`.

**What is asserted (not eyeballed):**

1. `WebAssembly.Module.imports(mod)` → every import's `module === "env"`; **zero** `/wasi/i` modules.
2. Every `atlas_reserve`/`atlas_set`/glyph-render call returns non-null / `rc == 0`.
3. `gl.getError() === 0` after the draw, and shader compile/link status is checked explicitly (an uncompiled shader throws with its info log rather than silently drawing nothing).
4. **The pixel assertion.** For each rendered cell *i*: `readPixels` ink count in cell column *i* **must equal** the ink count computed independently, on the CPU, from the atlas bytes for that glyph's `(ax, ay, w, h)` rect. Plus: the first unused cell column must have **exactly 0** ink.
   - The expectation is *computed at run time from the same atlas*, never hardcoded — so the gate is portable across hosts whose `monospace` font differs, while remaining exact.
   - `antialias: false`, `NEAREST` min/mag filter, integer-aligned quads, `UNPACK_ALIGNMENT = 1` ⇒ exactness is a design property, not luck. Observed exact on both the live GPU and SwiftShader.
   - Tolerance policy: **exact equality is the gate.** If a future host's readback deviates, the fix is to record the deviation and the host, not to widen the tolerance silently; any widening must be accompanied by a re-run of both negative controls proving they still FAIL under the widened rule (Task 9 Step 6 makes that a written rule in the gate's header).

**Negative controls — stated explicitly, and already observed to fail:**

| Control | Mutation | Expected | **Observed** |
|---|---|---|---|
| **NC1 — wrong glyph atlas offset** | add `+3` px to each glyph's atlas `(ax, ay)` when computing UVs (a plausible off-by-N packing bug) | assertion FAILS | ink `[252, 188, 0]` vs expectation `[217, 291]` → **FAIL** ✅ |
| **NC2 — shader outputs black** | fragment shader replaced with `o = vec4(0.,0.,0.,1.)` (a plausible "sampler unbound / wrong swizzle" bug) | assertion FAILS | ink `[0, 0, 0]` → **FAIL** ✅ |
| **NC3 — cell-3 purity** | (structural, always on) no glyph is drawn into cell column 2 | that column must be exactly 0 ink | `0` in every arm, including NC1/NC2 → the assertion is not trivially satisfied by "any ink anywhere" ✅ |

The gate **runs all three arms every time** and exits non-zero unless: `good` passes **and** NC1 fails **and** NC2 fails. A gate that only ran the positive arm would be exactly the class of vacuous assertion this program has found 5+ times; here the FAIL arms are proven reachable on every invocation, not once at authoring time.

**Cost and how it runs:**
- Local developer loop: `cargo build -p rustty-wasm --target wasm32-unknown-unknown --release` (~9s warm, ~2–3 min cold) then `.port/tools/gates/ws14_webgl_browser_gate.sh` (~6s: server up, Chrome up, CDP, three render arms, teardown).
- **CI-gateable: yes, proved** — headless, no npm, no GPU required (SwiftShader), deterministic, and byte-identical to the live GPU run. It is wired into `.port/tools/ci.sh` as an **opt-in lane** (`--webgl`) rather than the default lane, because it needs a `wasm32-unknown-unknown` toolchain component and a Chrome binary; the lane hard-FAILS (exit 3, never silently green) when either is missing, mirroring the A5(d) "must BLOCK if the oracle checkout is absent" rule from `.port/phase-3/assessments/C-capi-headers.md:437`.
- The live, visible (non-headless) Chrome pass in Task 10 is **corroboration for the record**, explicitly not the gate.

---

## Relevant files (all paths absolute-from-repo-root)

- `crates/rustty-renderer/src/webgl.rs` — **create** (T2). The `renderer/WebGL.zig` port.
- `crates/rustty-renderer/src/lib.rs` — **modify** (T2). `mod webgl;` + `pub use`, and the `.webgl` selection arm; un-comment the switch's third arm.
- `crates/rustty-os/src/wasm.rs` — **modify** (T3). Resurrect the `9dc2e5978`-deleted host-ownership tracker + its 2 tests.
- `crates/rustty-font/src/atlas.rs` — **modify** (T4). Port the `Wasm` block live under `#[cfg(feature = "wasm")]` + its `test "happy path"` with all 4 asserts.
- `crates/rustty-font/Cargo.toml` — **modify** (T4). `wasm = ["rustty-os/wasm-host-owned"]` (feature wiring, still no new crate edge beyond the existing `rustty-os`).
- `crates/rustty-core/src/app.rs` — **modify** (T5). Complete the `App.Wasm` port (empty struct + verbatim comments), strike the in-source deferral note.
- `crates/rustty-xev/src/backend.rs` — **modify** (T6). Freestanding-wasm arm of `default()`/`candidates()`.
- `crates/rustty-config/Cargo.toml`, `crates/rustty-config/src/lib.rs`, `crates/rustty-config/src/renderer_config.rs` — **modify** (T6). Default-on `renderer` feature.
- `crates/rustty-input/Cargo.toml`, `crates/rustty-input/src/link.rs` — **modify** (T6). Default-on `oniguruma` feature.
- `crates/rustty-wasm/Cargo.toml`, `crates/rustty-wasm/src/lib.rs` — **modify** (T7). cdylib, retention-set completion, `rustty-core` edge dropped, `rustty-font` edge added.
- `.port/tools/build-main-wasm.sh` — **create** (T7).
- `example/wasm-webgl/index.html`, `example/wasm-webgl/gate.js`, `example/wasm-webgl/glue.mjs` — **create** (T8).
- `.port/tools/gates/ws14_webgl_browser_gate.mjs`, `.port/tools/gates/ws14_webgl_browser_gate.sh` — **create** (T9).
- `.port/tools/ci.sh` — **modify** (T9). Opt-in `--webgl` lane.
- `.port/tools/gates/check_test_counts.py` — **modify** (T4). `font_atlas` apc_deferred `1 → 0`.
- `crates/rustty-apprt-c/tests/panic_policy.rs` — **modify** (T4). Extend the DG5 crate list.
- `.port/phase-2/DEFERRALS.md`, `.port/phase-2/VERIFICATION_DEBT.md`, `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` — **modify** (T11).
- `docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md` — **read only** (T12 cross-ref).

---

### Task 1: Branch and reproduce the baseline

**Files:** none (verification only).

**Interfaces:** Produces a `ws14-wasm-webgl` branch and a recorded baseline that confirms every PREMISE CORRECTION claim on the executing machine.

- [ ] **Step 1: Branch**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull && git log --oneline -1
git checkout -b ws14-wasm-webgl
git push -u origin ws14-wasm-webgl
```

Expected: HEAD at `4069662` or later; branch pushed.

- [ ] **Step 2: Confirm the native tree is green before touching anything**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -3
```

Expected: `Finished`. WS14 does not start on a red tree.

- [ ] **Step 3: Confirm the oracle's WebGL backend is a 2-line placeholder (§1)**

```bash
wc -l /Users/gael/projects/ghostty/src/renderer/WebGL.zig
cat /Users/gael/projects/ghostty/src/renderer/WebGL.zig
git -C /Users/gael/projects/ghostty show origin/main:src/renderer/WebGL.zig
```

Expected: `2`, the two lines, and the same two lines from `origin/main`. If `origin/main` has grown a real backend, STOP — O16 changes shape and this plan must be re-scoped.

- [ ] **Step 4: Confirm the deleted host-ownership tracker (§5)**

```bash
grep -rn 'toHostOwned\|isHostOwned\|toModuleOwned' /Users/gael/projects/ghostty/src
git -C /Users/gael/projects/ghostty show 9dc2e5978 --stat | grep 'os/wasm.zig'
git -C /Users/gael/projects/ghostty show 9dc2e5978^:src/os/wasm.zig | wc -l
```

Expected: only call sites + one doc mention (no `pub fn` definitions); `src/os/wasm.zig | 90 ---...`; `115`.

- [ ] **Step 5: Reproduce the wasm32 build + artifact shape (probe 1)**

```bash
cd /Users/gael/projects/rustty
cargo build -p rustty-web-canvas --target wasm32-unknown-unknown --release 2>&1 | tail -2
node -e 'const fs=require("fs");const m=new WebAssembly.Module(fs.readFileSync("target/wasm32-unknown-unknown/release/rustty_web_canvas.wasm"));const i=WebAssembly.Module.imports(m);console.log("imports",i.length,[...new Set(i.map(x=>x.module))],"exports",WebAssembly.Module.exports(m).length);'
```

Expected: `Finished`; `imports 14 [ 'env' ] exports 199`.

- [ ] **Step 6: Reproduce the three `rustty-wasm` wasm32 errors (§3)**

```bash
cd /Users/gael/projects/rustty && cargo check -p rustty-wasm --target wasm32-unknown-unknown 2>&1 | grep -E '^error' | head
```

Expected: the two `rustty-xev/src/backend.rs` `compile_error!`s and the `zlib-sys` build-script failure.

- [ ] **Step 7: Record the baseline**

Append the outputs of Steps 3-6 to the PR description (or a scratch note). These are the evidence the PREMISE CORRECTION rests on; the ledger edits in Task 11 cite them.

- [ ] **Step 8: Commit nothing** — Task 1 changes no files. Proceed.

---

### Task 2: Port `renderer/WebGL.zig` and complete the renderer selection switch (O16)

**Files:**
- Create: `crates/rustty-renderer/src/webgl.rs`
- Modify: `crates/rustty-renderer/src/lib.rs`

**Interfaces:**
- Consumes: `rustty_build_config::renderer::Backend::Webgl` (already exists, `crates/rustty-build-config/src/renderer.rs:15`).
- Produces: `rustty_renderer::webgl::WebGL`, and `rustty_renderer::Renderer` resolving on `wasm32`.

- [ ] **Step 1: Write the failing check first (TDD)**

Add to `crates/rustty-renderer/src/lib.rs`'s existing test module (or create `crates/rustty-renderer/tests/webgl_arm.rs` if the lib has no suitable `mod tests`):

```rust
/// O16: the reference `renderer.zig` switch has THREE arms; the port must name a
/// concrete type for `.webgl` too. Oracle: `src/renderer/WebGL.zig` (2 lines) —
/// a placeholder unit type with no methods, identical on the oracle's origin/main.
#[test]
fn webgl_arm_exists_and_is_a_unit_placeholder() {
    let _: crate::webgl::WebGL = crate::webgl::WebGL;
    assert_eq!(std::mem::size_of::<crate::webgl::WebGL>(), 0);
}
```

Run: `cargo test -p rustty-renderer --lib webgl_arm 2>&1 | tail -5`
Expected: **compile error** — `could not find webgl in the crate root`. That is the red state.

- [ ] **Step 2: Create `crates/rustty-renderer/src/webgl.rs`**

Faithful port. The reference is `//!` doc + `pub const WebGL = @This();` — a file-as-struct with no fields, i.e. a unit type. Content:

```rust
//! Renderer implementation for WebGL in the browser.
//!
//! PORTS: ghostty `src/renderer/WebGL.zig` (2 lines, 0 tests).
//!
//! PORT STATUS: ported — COMPLETE. The reference file is a two-line
//! PLACEHOLDER: a doc comment and `pub const WebGL = @This();` (the
//! file-as-struct idiom, i.e. an empty struct type). It has no fields, no
//! methods, no shaders, no draw path, and no `test {}`. It is named in exactly
//! two places, both in `src/renderer.zig` (`:20` the re-export, `:41` the
//! `.webgl` arm of the comptime `Renderer` switch). Verified identical on the
//! oracle's `origin/main` at plan time, so this is not an artifact of the pin.
//!
//! Consequently there is NOTHING deeper to port here, and inventing a WebGL
//! draw path would be an R1 violation (no reference to be faithful to). The
//! browser-side drawing that rustty DOES ship lives where the reference's own
//! wasm model puts it — in the consumer: `example/wasm-webgl/` renders the
//! `Atlas.Wasm` texture through WebGL2 against the wasm module's C-ABI. See
//! `docs/superpowers/plans/2026-07-23-ws14-wasm-webgl.md`.

// PORT: `pub const WebGL = @This();` — the file's own type. An empty Zig struct
// maps to a Rust unit struct (both zero-sized).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WebGL;
```

- [ ] **Step 3: Wire it into the crate root**

In `crates/rustty-renderer/src/lib.rs`, next to the other backend modules, add:

```rust
// PORT: `pub const WebGL = @import("renderer/WebGL.zig");` (renderer.zig:20).
pub mod webgl;
pub use webgl::WebGL;
```

Then complete the selection switch. The verbatim comment block at `lib.rs:137-153` documents all three arms; directly below the existing `#[cfg(target_os = "macos")]` / `#[cfg(target_os = "linux")]` `pub use generic::Renderer;` lines add the third arm:

```rust
// PORT: `.webgl => WebGL` (renderer.zig:41) — the ONLY arm that is NOT
// `GenericRenderer(Backend)`: the reference selects the bare `WebGL` type. With
// this arm the port's switch is complete for all three reference arms.
#[cfg(target_arch = "wasm32")]
pub use webgl::WebGL as Renderer;
```

Update the surrounding PORT NOTE so it no longer says "WebGL wasm-deferred" (three sites: `lib.rs:53`, `lib.rs:150`, and `crates/rustty-renderer/src/generic.rs:9`, `crates/rustty-renderer/src/metal.rs:7`, `:32` — grep `WebGL` in the crate and fix every "deferred" claim to "ported (2-line reference placeholder)").

- [ ] **Step 4: Green the test**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-renderer --lib webgl_arm 2>&1 | tail -5
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Confirm no native behavior change (R5)**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -2 && cargo test -p rustty-renderer 2>&1 | tail -3
```

Expected: `Finished`; the renderer suite's pass count is its previous count **+1**.

- [ ] **Step 6: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-renderer/src/webgl.rs crates/rustty-renderer/src/lib.rs crates/rustty-renderer/src/generic.rs crates/rustty-renderer/src/metal.rs
git commit -m "renderer: port renderer/WebGL.zig (2-line reference placeholder) + complete the .webgl selection arm (WS14 T2)"
```

---

### Task 3: Resurrect the deleted `os/wasm.zig` host-ownership tracker (unblocks O14)

**Files:** Modify `crates/rustty-os/src/wasm.rs`; modify `crates/rustty-os/Cargo.toml` (new default-off feature).

**Interfaces:**
- Consumes: nothing (leaf crate).
- Produces: `rustty_os::wasm::{malloc, free, to_host_owned, is_host_owned, to_module_owned}` behind a `wasm-host-owned` feature, host-testable.

**Why:** §5. `Atlas.Wasm` and the ledger's 4 required asserts depend on functions `9dc2e5978` deleted upstream. This is the same *user-requested resurrection* precedent as `web_canvas.zig` (`DEFERRALS.md:4402`).

- [ ] **Step 1: Read the deleted reference**

```bash
git -C /Users/gael/projects/ghostty show 9dc2e5978^:src/os/wasm.zig > /tmp/ws14_wasm_zig_pre9dc2e59.zig
sed -n '26,115p' /tmp/ws14_wasm_zig_pre9dc2e59.zig
```

Expected: the `allocs` map, `malloc`, `free`, `toHostOwned`, `isHostOwned`, `toModuleOwned`, and the 2 tests (`"basics"`, `"toHostOwned"`).

- [ ] **Step 2: Write the two reference tests FIRST (TDD)**

Append to `crates/rustty-os/src/wasm.rs`, under `#[cfg(all(test, feature = "wasm-host-owned"))] mod host_owned_tests`, faithful ports of both:

```rust
// test "basics"
#[test]
fn basics() {
    let buf = malloc(32).expect("malloc");
    assert_eq!(allocs_len(), 1);
    unsafe { free(buf) };
    assert_eq!(allocs_len(), 0);
}

// test "toHostOwned"
#[test]
fn to_host_owned_roundtrip() {
    #[derive(Default)]
    struct Point { x: u32, y: u32 }
    let p = Box::into_raw(Box::new(Point::default()));
    let ptr = unsafe { to_host_owned(p) }.expect("to_host_owned");
    assert_eq!(allocs_len(), 1);
    assert!(is_host_owned(p));
    assert!(is_host_owned(ptr));
    unsafe { free(ptr) };
    assert_eq!(allocs_len(), 0);
}
```

Run `cargo test -p rustty-os --features wasm-host-owned host_owned 2>&1 | tail -5` → expect **compile failure** (nothing exists yet). Red state.

- [ ] **Step 3: Port the tracker**

Add to `crates/rustty-os/src/wasm.rs` a `#[cfg(feature = "wasm-host-owned")] pub mod host_owned` (re-exported at the module root for the reference's flat namespace), with a full PORT header recording the archaeology:

```rust
// PORT NOTE — RESURRECTED REFERENCE CODE. The pinned oracle's `src/os/wasm.zig`
// no longer contains this block: commit `9dc2e5978` ("lib-vt: enable freestanding
// wasm builds (#9301)", 2025-10-21) deleted 90 lines — the `allocs` tracker,
// `malloc`, `free`, `toHostOwned`, `isHostOwned`, `toModuleOwned` and their two
// `test {}` blocks — leaving `font/Atlas.zig`'s `Wasm` block and
// `font/face/web_canvas.zig` referencing symbols that no longer exist (they are
// not analyzed upstream because `main_wasm.zig` is referenced nowhere in
// `build.zig`). rustty resurrects them, per the same user decision that
// resurrected `web_canvas.zig` (DEFERRALS.md, WASM/web target RESIDUAL #1), so
// that `Atlas.Wasm` and its 4-assert reference test can be ported LIVE.
// Reference text: `git -C ../ghostty show 9dc2e5978^:src/os/wasm.zig` lines 27-115.
```

Port rules to follow exactly:
- `allocs: std.AutoHashMapUnmanaged([*]u8, usize)` → a `HashMap<*mut u8, usize>` behind a module-private `OnceLock<Mutex<…>>` (or `thread_local!` `RefCell` — pick `Mutex` so the type is `Sync`; single-threaded wasm pays nothing). Document the divergence: Zig's global-var-with-explicit-allocator has no direct Rust analog; the observable contract (`putNoClobber` semantics ⇒ **panic/abort on a duplicate key**, `remove`, `get`, `contains`) is preserved 1:1.
- `malloc(len) ?[*]u8` → `pub extern "C" fn malloc(len: usize) -> *mut u8` returning null on failure. **Plain `extern "C"` (DG5 — abort, no wrappers).** Name: the reference exports it as bare `malloc`; that collides with libc on native. Resolve by `#[cfg_attr(target_arch = "wasm32", no_mangle)]` so the bare symbol is emitted **only** in the wasm artifact, and expose the Rust-callable name `malloc` regardless (tests call the Rust path). Record this in a PORT NOTE.
- `free(?[*]u8)` → `pub unsafe extern "C" fn free(ptr: *mut u8)`, same `#[cfg_attr]` treatment; null-tolerant; a pointer not in `allocs` is a no-op (faithful: the reference's `if (allocs.get(v)) |len|`).
- `toHostOwned(ptr: anytype) ![*]u8` → `pub unsafe fn to_host_owned<T>(ptr: *mut T) -> Result<*mut u8, HostOwnError>`; records `size_of::<T>()` (the reference's `@sizeOf(T)`).
- `isHostOwned`/`toModuleOwned` → `pub fn is_host_owned<T>(ptr: *const T) -> bool` / `pub unsafe fn to_module_owned<T>(ptr: *mut T)`.
- Add a test-only `allocs_len()` helper (the reference reads `allocs.size` directly from the same file, so this is the same access, not new API).
- Byte-slice discipline: everything is `*mut u8`/`&[u8]`; no `String` anywhere.

- [ ] **Step 4: Cargo feature**

In `crates/rustty-os/Cargo.toml`'s `[features]`:

```toml
# The pre-`9dc2e5978` `os/wasm.zig` host-ownership allocation tracker
# (malloc/free/toHostOwned/isHostOwned/toModuleOwned). Resurrected reference code,
# needed by `font/Atlas.zig`'s `Wasm` block; OFF by default so the desktop build is
# byte-identical. Host-buildable (the map is target-agnostic) so the two reference
# tests run under plain `cargo test`.
wasm-host-owned = []
```

- [ ] **Step 5: Green**

```bash
cd /Users/gael/projects/rustty
cargo test -p rustty-os --features wasm-host-owned host_owned 2>&1 | tail -5
cargo test -p rustty-os 2>&1 | tail -3
cargo build --workspace 2>&1 | tail -2
```

Expected: `2 passed` for the feature run; the default `rustty-os` suite unchanged; workspace `Finished`.

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-os/src/wasm.rs crates/rustty-os/Cargo.toml
git commit -m "os/wasm: resurrect the 9dc2e5978-deleted host-ownership tracker + its 2 reference tests, feature-gated (WS14 T3)"
```

---

### Task 4: Port `Atlas.Wasm` with all 4 asserts live (O14)

**Files:**
- Modify: `crates/rustty-font/src/atlas.rs`, `crates/rustty-font/Cargo.toml`
- Modify: `.port/tools/gates/check_test_counts.py`, `crates/rustty-apprt-c/tests/panic_policy.rs`

**Interfaces:**
- Consumes: `rustty_os::wasm` host-ownership tracker (T3); the Canvas `env` imports (declared locally — **no new crate edge**, R4/D1).
- Produces: the unprefixed `atlas_new`/`atlas_free`/`atlas_reserve`/`atlas_set`/`atlas_grow`/`atlas_clear`/`atlas_debug_canvas` C-ABI exports under `feature = "wasm"`.

- [ ] **Step 1: Read the reference block**

```bash
sed -n '405,578p' /Users/gael/projects/ghostty/src/font/Atlas.zig
```

Expected: `pub const Wasm = struct { … }` from the `/// The wasm-compatible API…` doc through `test "happy path"` and the closing `};`. The port's verbatim copy is already in `crates/rustty-font/src/atlas.rs:740-925` as comments — port *those* comment lines into live code, keeping each reference line as a `//`-prefixed PORT anchor above its Rust equivalent (the house style everywhere else in this file).

- [ ] **Step 2: Write the reference test first (TDD) — all 4 asserts live**

In `atlas.rs`, inside the new `#[cfg(feature = "wasm")] pub mod wasm_exports`:

```rust
// test "happy path"
#[cfg(test)]
#[test]
fn happy_path() {
    // const atlas = atlas_new(512, @intFromEnum(Format.grayscale)).?;
    let atlas = atlas_new(512, Format::Grayscale as u8);
    assert!(!atlas.is_null());
    // const reg = atlas_reserve(atlas, 2, 2).?;
    let reg = unsafe { atlas_reserve(atlas, 2, 2) };
    assert!(!reg.is_null());
    // try testing.expect(wasm.isHostOwned(reg));            [assert 1]
    assert!(rustty_os::wasm::is_host_owned(reg));
    // try testing.expect(reg.width > 0);                    [assert 2]
    assert!(unsafe { (*reg).width } > 0);
    let data = [1u8, 2, 3, 4];
    // try testing.expect(!atlas.modified);                  [assert 3]
    assert!(!unsafe { &*atlas }.modified_since(0 /* the file's own modified read */));
    // atlas_set(atlas, reg, data, data.len);
    unsafe { atlas_set(atlas, reg, data.as_ptr(), data.len()) };
    // try testing.expect(atlas.modified);                   [assert 4]
    assert!(unsafe { &*atlas }.modified_since(0));
    // defer wasm.toModuleOwned(reg); defer alloc.destroy(reg); defer atlas_free(atlas);
    unsafe { rustty_os::wasm::to_module_owned(reg) };
    unsafe { drop(Box::from_raw(reg)) };
    unsafe { atlas_free(atlas) };
}
```

**Note on asserts 3/4:** the reference's `atlas.modified` is an `Atomic(u32)` generation counter in the pinned tree (`test "writing data"` reads `atlas.modified.load(.monotonic)` and asserts `new > old`), while the older `Wasm` test predates that and treats it as a bool. Port faithfully to the **pinned** representation: capture `let before = atlas.modified.load(Ordering::Relaxed)` before `atlas_set` and assert `after > before`. Keep the reference's original two lines as PORT comments and add a one-line note that the bool→generation change is upstream's (`a83bd6a11`-era), not a port divergence. Use whatever accessor `atlas.rs` already exposes; do not add a new public accessor if `modified` is reachable in-crate.

Run `cargo test -p rustty-font --features wasm happy_path 2>&1 | tail -5` → **compile failure**. Red.

- [ ] **Step 3: Port the seven exports**

Each is `#[no_mangle] pub extern "C"` (plain `"C"` — DG5), unprefixed per the Global Constraints exception, and each maps 1:1:

| Reference | Rust |
|---|---|
| `atlas_new(size: u32, format: u8) ?*Atlas` | `atlas_new(size: u32, format: u8) -> *mut Atlas<Global>`; **narrow `format` with an explicit `match`** (`0 => Grayscale, 1 => Bgra, _ => return null()`) — never `transmute`/`from_repr` on an out-of-range C integer. The reference's `@enumFromInt(format)` is UB-on-invalid; the port's explicit `match` is the required divergence, documented in a PORT NOTE. |
| `atlas_free(ptr: ?*Atlas)` | null-tolerant `Box::from_raw` + `deinit` |
| `atlas_reserve(self, w, h) ?*Region` | heap `Region`, `to_host_owned` it, return; null on error (faithful `catch return null`) |
| `atlas_set(self, reg, data: [*]const u8, len)` | `slice::from_raw_parts(data, len)` → `&[u8]`; never `String` |
| `atlas_grow(self, size_new) bool` | `false` on error |
| `atlas_clear(self)` | direct |
| `atlas_debug_canvas(self) u32` | see Step 4 |

- [ ] **Step 4: `atlas_debug_canvas` — the Canvas/JS half, without a crate edge**

The reference uses `zig-js` to `document.createElement("canvas")`, size it, `getContext("2d")`, build an `ImageData` from wasm memory (BGRA→RGBA swap, or A8→RGBA expansion), `putImageData`, and return a JS ref id. rustty's equivalent JS bridge already exists as the 14-op `#[link(wasm_import_module = "env")]` Canvas FFI used by `rustty-font-shaper`'s `web_canvas` backend, with glue at `.port/tools/web-canvas-glue.mjs`.

**Constraint (memory: apc-glyf-font-layering-d1): `rustty-font` must NOT depend on `rustty-font-shaper` or `rustty-vt`.** So declare the needed imports **locally** in `atlas.rs` under `#[cfg(all(feature = "wasm", target_arch = "wasm32"))]`:

```rust
#[link(wasm_import_module = "env")]
extern "C" {
    fn rustty_wc_canvas_new() -> u32;
    fn rustty_wc_canvas_set_size(h: u32, w: u32, ht: u32);
    fn rustty_wc_canvas_get_context(h: u32) -> u32;
    // NEW op (Step 5): the reference's `ctx.call(void, "putImageData", …)`.
    fn rustty_wc_context_put_image_data(ch: u32, ptr: *const u8, len: usize, w: u32, h: u32, x: i32, y: i32);
}
```

PORT NOTE requirements: (a) wasm imports dedupe by `(module, name)`, so declaring the same import in two crates is legal **provided the signatures match byte-for-byte** — a mismatch is a hard link error, so any change to the shaper's decls must be mirrored here (state this explicitly, it is the maintenance hazard); (b) this keeps `rustty-font` pure and dependency-free, which is why it is preferred over an edge; (c) `wasm.shared_mem` (the reference's `SharedArrayBuffer` `arr.slice()` copy branch) maps to `rustty_os::wasm::shared_mem` — port the branch, with the copy performed on the JS side by the new op (it always copies out of wasm memory, which is the shared-mem-safe behavior; note that the non-shared branch's zero-copy `Uint8ClampedArray` view is therefore not reproduced, and why that is unobservable to the caller).

Also port both pixel conversions faithfully (BGRA: swap every R/B; grayscale: A8 → RGBA with the value in the alpha byte) into `Vec<u8>`, never `String`.

- [ ] **Step 5: Extend the shared glue with the one missing op**

`.port/tools/web-canvas-glue.mjs` currently exposes 14 ops (`rustty_wc_canvas_new/free/get_context/set_size`, `rustty_wc_context_clear/set_font/get_font/set_fill_style/set_text_baseline/fill_rect/fill_text/measure_text/get_image_data`, `rustty_wc_debug_append`) and has **no** `put_image_data`. Add:

```js
rustty_wc_context_put_image_data(ch, ptr, len, w, h, x, y) {
  const ctx = ctxs.get(ch);
  const img = new ImageData(new Uint8ClampedArray(u8().slice(ptr, ptr + len)), w, h);
  ctx.putImageData(img, x, y);
},
```

(match the file's existing handle-table and `u8()` helper names exactly).

- [ ] **Step 6: Feature wiring + the two gates**

`crates/rustty-font/Cargo.toml`: change `wasm = []` to `wasm = ["rustty-os/wasm-host-owned"]` and rewrite the comment (it currently says the block "is recorded as a verbatim commented reference" — now it is live code).

`.port/tools/gates/check_test_counts.py`: flip `"font_atlas": ([...], [...], 1)` → `0`, and rewrite the preceding comment block (`:266-276`) to record that the `Wasm` `test "happy path"` is now LIVE under `--features wasm` (note `rust_count` counts `#[test]` regardless of cfg, so 13 == 13).

`crates/rustty-apprt-c/tests/panic_policy.rs:96`: extend the crate list from `["rustty-apprt-c", "rustty-vt-c", "rustty-renderer"]` to include `"rustty-font"` and `"rustty-wasm"` (the two crates that now carry C-ABI exports), and extend its comment to say why.

- [ ] **Step 7: Green**

```bash
cd /Users/gael/projects/rustty
cargo test -p rustty-font --features wasm 2>&1 | tail -3
cargo test -p rustty-font 2>&1 | tail -3
cargo build -p rustty-font --features wasm --target wasm32-unknown-unknown 2>&1 | tail -2
python3 .port/tools/gates/check_test_counts.py 2>&1 | tail -5
cargo test -p rustty-apprt-c --test panic_policy 2>&1 | tail -3
cargo build --workspace 2>&1 | tail -2
```

Expected: font suite +1 with the feature on; default font suite unchanged; wasm target `Finished`; test-count gate green with `font_atlas` at 13/13; DG5 gate green; workspace green.

- [ ] **Step 8: Prove the DG5 gate can still fail (anti-vacuity)**

```bash
cd /Users/gael/projects/rustty
python3 - <<'EOF'
p='crates/rustty-font/src/atlas.rs'; s=open(p).read()
open(p,'w').write(s.replace('pub extern "C" fn atlas_clear','pub extern "C-unwind" fn atlas_clear',1))
EOF
cargo test -p rustty-apprt-c --test panic_policy 2>&1 | grep -c 'DG5 violation'
git checkout -- crates/rustty-font/src/atlas.rs
```

Expected: `1` (the gate FAILS naming `atlas.rs`), then the tree is restored. If it prints `0`, the crate-list edit in Step 6 did not take effect — fix before proceeding.

- [ ] **Step 9: Commit**

```bash
git add crates/rustty-font/src/atlas.rs crates/rustty-font/Cargo.toml \
        .port/tools/web-canvas-glue.mjs .port/tools/gates/check_test_counts.py \
        crates/rustty-apprt-c/tests/panic_policy.rs
git commit -m "font: port Atlas.Wasm live (7 C-ABI exports + debug canvas) with its 4-assert reference test; DG5 gate widened (WS14 T4)"
```

---

### Task 5: Complete the `App.zig` Wasm block port (O15)

**Files:** Modify `crates/rustty-core/src/app.rs`.

**Interfaces:** Produces a *complete* port of the reference's empty `App.Wasm` struct — and therefore a ledger strike, not a deferral.

- [ ] **Step 1: Re-read the reference and the port's verbatim copy**

```bash
sed -n '575,605p' /Users/gael/projects/ghostty/src/App.zig
sed -n '995,1010p' /Users/gael/projects/rustty/crates/rustty-core/src/app.rs
```

Expected: the reference block is `pub const Wasm = if (!builtin.target.isWasm()) struct {} else struct { const wasm = …; const alloc = …; /* app_new, app_new_, app_free ALL COMMENTED OUT */ };`, and `app.rs` already preserves it verbatim.

- [ ] **Step 2: Port it live**

Replace the commented block's *wrapper* (keeping every reference line as a `//` PORT anchor, including the commented-out `app_new`/`app_new_`/`app_free` bodies — they are commented **in the reference**, so commented-in-the-port is the faithful state) with a real, empty, wasm-gated item:

```rust
// Wasm API.
// PORT: `pub const Wasm = if (!builtin.target.isWasm()) struct {} else struct { … };`
//
// PORT STATUS: ported — COMPLETE, and deliberately EMPTY. On non-wasm targets the
// reference yields `struct {}`; on wasm it yields a struct whose ONLY live members
// are two private aliases (`wasm`, `alloc`) and whose three exports (`app_new`,
// `app_new_`, `app_free`) are COMMENTED OUT IN THE REFERENCE. Retaining it emits
// zero symbols and pulls no App implementation into the wasm module — which is why
// `crates/rustty-wasm` does NOT need a `rustty-core` dependency edge to be a
// faithful `main_wasm.zig` (WS14 T7). The reference's commented-out bodies are
// preserved verbatim below so a future un-comment is a diff, not archaeology.
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    // const wasm = @import("os/wasm.zig");
    // const alloc = wasm.alloc;
    #[allow(unused_imports)]
    use rustty_os::wasm as _wasm;

    // export fn app_new(config: *Config) ?*App { … }   [commented in the reference]
    // fn app_new_(config: *Config) !*App { … }         [commented in the reference]
    // export fn app_free(ptr: ?*App) void { … }        [commented in the reference]
}
```

Delete the in-source deferral wording (the "currently a documented-deferral" / "Target: the rustty wasm app-runtime" phrasing) — it is no longer true.

- [ ] **Step 3: Green**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -2 && cargo test -p rustty-core 2>&1 | tail -3
python3 .port/tools/gates/check_comment_coverage.py --crate rustty-core 2>&1 | tail -5
```

Expected: `Finished`; core suite unchanged at **15**; comment coverage no worse than before (the reference lines are all still present).

- [ ] **Step 4: Commit**

```bash
git add crates/rustty-core/src/app.rs
git commit -m "core/app: complete the App.Wasm port (empty struct, reference exports commented upstream) (WS14 T5)"
```

---

### Task 6: Make the wasm32 link surface buildable — three cfg/feature edits

**Files:**
- Modify: `crates/rustty-xev/src/backend.rs`
- Modify: `crates/rustty-config/Cargo.toml`, `crates/rustty-config/src/lib.rs`
- Modify: `crates/rustty-input/Cargo.toml`, `crates/rustty-input/src/link.rs`

**Interfaces:** Produces `cargo check -p rustty-config --no-default-features --features wasm --target wasm32-unknown-unknown` green, with **native defaults byte-identical**.

**Design note (why this rather than cross-compiling the C):** probe 4. Five `-sys` build scripts block the wasm link, and all five arrive through exactly two crate edges. `wasm32-unknown-unknown` has no libc sysroot, so those C/C++ libraries cannot be built for it at all (unlike the `wasm32-wasip1` lane, which has zig's wasi sysroot). Shrinking the link surface is therefore not a shortcut — it is the only correct option, and it is what `rustty-vt-c` already did for zlib/simdutf (`DEFERRALS.md:4415-4440`).

- [ ] **Step 1: xev — the freestanding-wasm backend arm**

In `crates/rustty-xev/src/backend.rs`, in **both** `default()` (`:66`) and `candidates()` (`:106`), widen the wasi arm and the matching `not(any(...))` guard:

```rust
// PORT NOTE: `.wasi => .wasi_poll`. The reference's target enum distinguishes
// `.wasi` from freestanding wasm; the port treats BOTH as the single-candidate
// wasi arm, because rustty's wasm builds (both wasip1 and the freestanding
// browser cdylib) run the SYNCHRONOUS C-ABI with no event loop — the same no-op
// Loop/Async seam either way. Without this, a `wasm32-unknown-unknown` build hits
// the `compile_error!` below even though nothing in the wasm module ever
// constructs a Loop.
#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
{
    Backend::WasiPoll
}
```

and in the guard replace `target_os = "wasi"` with `all(target_family = "wasm", not(target_os = "emscripten"))`.

Verify (this is probe 3, reproduced):

```bash
cd /Users/gael/projects/rustty && cargo check -p rustty-xev --target wasm32-unknown-unknown 2>&1 | tail -3
cargo test -p rustty-xev 2>&1 | tail -3
```

Expected: wasm32 `Finished`; the native xev suite (42 tests) unchanged.

- [ ] **Step 2: `rustty-config` — default-on `renderer` feature**

`Cargo.toml`: `rustty-renderer = { workspace = true, optional = true }`, and

```toml
[features]
default = ["renderer"]
# The `impl rustty_renderer::config::RendererConfig for Config` accessor-trait impl
# (memory: renderer-config-accessor-trait-acyclic) is the ONLY use of the renderer
# edge in this crate, and that edge transitively pulls glslang/spirv-cross/wuffs/
# sentry C++ — impossible on `wasm32-unknown-unknown` (no libc sysroot). Default-ON,
# so every native build is byte-identical; the wasm module builds with
# `--no-default-features --features wasm`.
renderer = ["dep:rustty-renderer"]
```

`src/lib.rs`: `#[cfg(feature = "renderer")] mod renderer_config;` (plus any `pub use`). **Do not** cfg individual items inside `renderer_config.rs` — gate the whole module; that file's every `use` names `rustty_renderer`.

Audit every dependent so no native build silently loses the impl:

```bash
cd /Users/gael/projects/rustty && grep -rn 'rustty-config' crates/*/Cargo.toml xtask/Cargo.toml | grep -i 'default-features'
```

Expected: **no** `default-features = false` on `rustty-config` anywhere (if one exists, add `features = ["renderer"]` to it explicitly and note it).

- [ ] **Step 3: `rustty-input` — default-on `oniguruma` feature**

`Cargo.toml`: `rustty-oniguruma = { workspace = true, optional = true }`, `default = ["oniguruma"]`, `oniguruma = ["dep:rustty-oniguruma"]` — mirroring the precedent `rustty-config/Cargo.toml:32` already sets. Gate the single consumer, `crates/rustty-input/src/link.rs:92 oni_regex` (and the `oni`-typed signature) with `#[cfg(feature = "oniguruma")]`, with a PORT NOTE: the reference `Link.oniRegex` is unconditional; the feature exists solely because the C library cannot target freestanding wasm, and the method is unreachable there (no renderer ⇒ no link highlighting).

Same dependent audit as Step 2 for `rustty-input`.

- [ ] **Step 4: Verify the wasm surface is now clean**

```bash
cd /Users/gael/projects/rustty
cargo check -p rustty-config --no-default-features --features wasm --target wasm32-unknown-unknown 2>&1 | tail -3
cargo check -p rustty-input --no-default-features --target wasm32-unknown-unknown 2>&1 | tail -3
```

Expected: `Finished` for both, **no** `-sys` build-script failures. If a *new* `-sys` crate appears, `cargo tree -i <crate>` it and extend this task rather than moving on.

- [ ] **Step 5: Verify native is unchanged (R5) and the graph is still acyclic (R4)**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -2
cargo test --workspace 2>&1 | tail -5
.port/tools/gates/check_acyclic.sh 2>&1 | tail -3
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
```

Expected: `Finished`; the workspace test totals **identical** to the Task 1 baseline plus WS14's new tests only; acyclic green; clippy clean.

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-xev/src/backend.rs crates/rustty-config/Cargo.toml crates/rustty-config/src/lib.rs \
        crates/rustty-input/Cargo.toml crates/rustty-input/src/link.rs
git commit -m "wasm32: freestanding xev backend arm + default-on renderer/oniguruma features so the wasm link drops 5 C -sys deps (WS14 T6)"
```

---

### Task 7: Turn `crates/rustty-wasm` into the real `main_wasm.zig` artifact (O13a)

**Files:** Modify `crates/rustty-wasm/Cargo.toml`, `crates/rustty-wasm/src/lib.rs`; create `.port/tools/build-main-wasm.sh`.

**Interfaces:**
- Consumes: T3 (tracker), T4 (`atlas_*` exports), T5 (`App.Wasm` is a no-symbol empty struct ⇒ no core edge), T6 (link surface).
- Produces: `target/wasm32-unknown-unknown/release/rustty_wasm.wasm` with the `config_*` + `atlas_*` + VT surface, zero wasi imports.

- [ ] **Step 1: Write the artifact test first (TDD)**

Create `crates/rustty-wasm/tests/artifact_shape.rs` — a host-side test that **fails loudly with instructions** when the artifact is absent, and otherwise asserts its shape by parsing the `.wasm` (pure Rust, no new deps: parse the module header + the import/export sections manually, or assert via the node driver in T9 if binary parsing is disproportionate — pick one and say so in the file header):

```
- the file exists at target/wasm32-unknown-unknown/release/rustty_wasm.wasm
- every import's module string is "env"; zero imports whose module contains "wasi"
- the export set CONTAINS: config_new, config_free, config_load_string,
  config_finalize, atlas_new, atlas_free, atlas_reserve, atlas_set, atlas_grow,
  atlas_clear
```

Mark it `#[ignore]` with a message naming `.port/tools/build-main-wasm.sh`, so `cargo test --workspace` does not require a wasm build; T9's gate runs it un-ignored.

- [ ] **Step 2: `Cargo.toml` — cdylib + the corrected retention edges**

```toml
[lib]
# cdylib: emits the `rustty_wasm.wasm` bearing the unmangled C-ABI exports (the
# `main_wasm.zig` artifact). rlib: keeps the retention logic host-buildable so
# native `cargo build --workspace` stays green.
crate-type = ["cdylib", "rlib"]

[dependencies]
# `_ = @import("os/wasm.zig");`
rustty-os = { workspace = true, features = ["wasm-host-owned"] }
# `_ = @import("config.zig").Wasm;` — the four `config_*` C-ABI exports. The
# renderer accessor-trait impl is NOT part of the wasm module (its C++ deps cannot
# target freestanding wasm), hence default-features = false (WS14 T6).
rustty-config = { workspace = true, default-features = false, features = ["wasm"] }
# `_ = @import("terminal/main.zig");`
rustty-vt = { workspace = true }
# `_ = @import("font/main.zig");` — the Atlas.Wasm C-ABI/JS surface (WS14 T4). This
# edge was MISSING: the reference retains font/main.zig, so `atlas_*` belongs in
# this artifact.
rustty-font = { workspace = true, features = ["wasm"] }
```

**Remove the `rustty-core` dependency**, replacing its comment with the §4 rationale:

```toml
# NOT depended on: `_ = @import("App.zig").Wasm;`. The reference's App.Wasm is an
# EMPTY struct whose three exports are commented out upstream (App.zig:577-604), so
# retaining it emits zero symbols and pulls no App implementation. A `rustty-core`
# edge would over-retain an entire application tier (and transitively drag
# rustty-inspector → dcimgui-sys → freetype-sys → zlib-sys into a freestanding wasm
# link). The block itself IS ported, in `rustty-core/src/app.rs` (WS14 T5).
```

- [ ] **Step 3: `src/lib.rs` — complete the retention set**

Extend the `mod retain` block with a second `#[used]` static pinning the `atlas_*` exports (same mechanism and same rationale as the existing `config_*` tuple), and rewrite the module header's PORT NOTE: it currently says `font/main.zig`'s wasm export block "is BLOCKED — B1" and that `App.zig.Wasm` is "currently a documented-deferral". Both statements are now false. State the new truth: 4 of the 5 reference retentions map to crate edges + `#[used]` statics; the 5th (`App.Wasm`) retains an empty struct and therefore needs no edge.

Also update `PORT STATUS:` from "ported (host-buildable rlib; the wasm32 cdylib link is gated on the B2 cross-workspace wasm32 gating)" to record the artifact and its build script.

- [ ] **Step 4: Build script**

Create `.port/tools/build-main-wasm.sh`, `chmod +x`, modelled exactly on `.port/tools/build-web-canvas-wasm.sh`:

```sh
#!/bin/sh
# Reproducible browser-freestanding build of the `main_wasm.zig` artifact:
# os/wasm + config.Wasm + terminal + font/Atlas.Wasm, as a PURE-RUST
# wasm32-unknown-unknown cdylib with ZERO wasi imports (only the `env` Canvas
# imports of Atlas.Wasm's debug canvas). No C is cross-compiled: the wasm link
# surface excludes the 5 C `-sys` deps by construction (WS14 T6).
#
# Usage:  .port/tools/build-main-wasm.sh [--release] [extra cargo args...]
set -e
exec cargo build -p rustty-wasm --target wasm32-unknown-unknown "$@"
```

- [ ] **Step 5: Build and inspect**

```bash
cd /Users/gael/projects/rustty
.port/tools/build-main-wasm.sh --release 2>&1 | tail -3
ls -la target/wasm32-unknown-unknown/release/rustty_wasm.wasm
node -e 'const fs=require("fs");const m=new WebAssembly.Module(fs.readFileSync("target/wasm32-unknown-unknown/release/rustty_wasm.wasm"));const i=WebAssembly.Module.imports(m);const e=WebAssembly.Module.exports(m).map(x=>x.name);console.log("imports",i.length,[...new Set(i.map(x=>x.module))]);console.log("wasi",i.filter(x=>/wasi/i.test(x.module)).length);console.log("has",["config_new","config_free","config_load_string","config_finalize","atlas_new","atlas_reserve","atlas_set","atlas_free","atlas_grow","atlas_clear"].filter(n=>e.includes(n)).join(" "));'
```

Expected: `Finished`; a `.wasm` on disk; `wasi 0`; all ten names listed by `has`. If any `config_*`/`atlas_*` is missing, the `#[used]` retention did not hold — fix the retention static (do **not** reach for `-C link-dead-code` or LTO; LTO is banned).

- [ ] **Step 6: Un-ignore-run the artifact test, and confirm native is untouched**

```bash
cd /Users/gael/projects/rustty
cargo test -p rustty-wasm -- --ignored 2>&1 | tail -5
cargo build --workspace 2>&1 | tail -2
```

Expected: artifact test passes; workspace `Finished`.

- [ ] **Step 7: Commit**

```bash
git add crates/rustty-wasm/Cargo.toml crates/rustty-wasm/src/lib.rs \
        crates/rustty-wasm/tests/artifact_shape.rs .port/tools/build-main-wasm.sh
git commit -m "wasm: rustty_wasm.wasm is now a real freestanding cdylib — complete retention set (font/Atlas.Wasm added, rustty-core edge dropped as unfaithful) (WS14 T7)"
```

---

### Task 8: The WebGL2 harness page (the browser consumer)

**Files:** Create `example/wasm-webgl/index.html`, `example/wasm-webgl/gate.js`, `example/wasm-webgl/glue.mjs`.

**Interfaces:**
- Consumes: `rustty_wasm.wasm`'s `atlas_*` + VT C-ABI + the `rustty_wc_*` Canvas imports.
- Produces: a page that renders glyphs through **WebGL2**, and `gate.js` — the single assertion body shared with T9's gate so the two can never diverge.

- [ ] **Step 1: `glue.mjs` — the `env` import object for `rustty_wasm.wasm`**

Re-export `makeWebCanvasEnv` from `../../.port/tools/web-canvas-glue.mjs` (which now includes `put_image_data` from T4 Step 5). If `rustty_wasm.wasm` imports a strict subset, still supply the full object — extra properties are ignored by `WebAssembly.instantiate`. Add a one-line header explaining the file exists so the page has a stable relative import and the gate and page load identical glue.

- [ ] **Step 2: `gate.js` — the assertion body (an async IIFE returning JSON)**

Structure it as an IIFE evaluating to a `Promise` of a plain JSON object, so it works verbatim from `Runtime.evaluate {returnByValue: true, awaitPromise: true}` **and** from the page's own `<script type="module">`. It must:

1. `compileStreaming(fetch('/target/wasm32-unknown-unknown/release/rustty_wasm.wasm'))`; assert every import module is `env`, zero `/wasi/i`.
2. Instantiate; allocate a `wasm`-side atlas via `atlas_new(1024, 0 /* grayscale */)`; assert non-null.
3. Rasterize a fixed glyph set (`"AB"` minimum; `"Rustty!"` for the visible page) into that atlas. **Source the coverage bitmaps through the wasm module** (the `web_canvas` Face path already exported by the shaper — if `rustty_wasm.wasm` does not export a face, have the page write known bitmaps through `atlas_set` instead and assert against those bytes; either way the expectation must be computed from the atlas bytes the module holds, never from a JS-side copy of the source pixels).
4. Read the atlas back (`atlas_data`-style accessor over `wasm.memory`) into a `Uint8Array` **copy** (`.slice()` — the buffer can be detached by a later `alloc`).
5. Compute, on the CPU, the ink count (`> 16`) inside each glyph's `(ax, ay, w, h)` atlas rect — **the expectation**.
6. Render: `webgl2` context with `{preserveDrawingBuffer: true, antialias: false}`; `R8` `texImage2D` of the atlas with `pixelStorei(UNPACK_ALIGNMENT, 1)`; `NEAREST` min/mag; one integer-aligned quad per glyph at a fixed 32px cell pitch; explicit shader compile/link status checks that throw the info log on failure.
7. `readPixels` and count ink per cell column; assert **equality** with the expectation, and **exactly 0** in the first unused column.
8. Accept a `mutate` parameter (`null | 'offset' | 'black'`) implementing NC1/NC2 (see the gate section) and return all three arms plus `pass_good`, `neg_offset_fails`, `neg_black_fails`, `glError`, and `gl.getParameter(gl.RENDERER)`.
9. **Never** call `alert`/`confirm`/`prompt`; log only into a `<pre id="log">`.

- [ ] **Step 3: `index.html` — the human-visible page**

Modelled on `example/wasm-web-canvas/index.html`: a short prose header explaining that this is the wasm consumer rendering the `Atlas.Wasm` texture through **WebGL2** (and that upstream's `renderer/WebGL.zig` is a placeholder, so this is where browser drawing lives), the serving instruction (`python3 -m http.server` from the repo root — `fetch()` of a `.wasm` needs http, not `file://`), the WebGL canvas appended at native size, a 5× nearest-neighbour upscale for legibility, and the `gate.js` result rendered as text (including the two negative-control arms, so a human sees the controls firing).

- [ ] **Step 4: Verify by hand once, over loopback**

```bash
cd /Users/gael/projects/rustty
python3 -m http.server 8731 --bind 127.0.0.1 >/tmp/ws14srv.log 2>&1 &
sleep 2 && curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8731/example/wasm-webgl/index.html
```

Expected: `200`. (Leave the server for T9/T10; kill it with `pkill -f 'http.server 8731'` when done.)

- [ ] **Step 5: Commit**

```bash
git add example/wasm-webgl
git commit -m "example/wasm-webgl: WebGL2 consumer page rendering the Atlas.Wasm texture from rustty_wasm.wasm (WS14 T8)"
```

---

### Task 9: The falsifiable browser gate (headless, scripted, CI-runnable)

**Files:** Create `.port/tools/gates/ws14_webgl_browser_gate.mjs` + `.sh`; modify `.port/tools/ci.sh`.

**Interfaces:**
- Consumes: `rustty_wasm.wasm` (T7), `example/wasm-webgl/gate.js` (T8).
- Produces: a gate that exits 0 only when the positive arm passes **and both negative controls fail**; exit 3 when the environment is missing; exit 1 on assertion failure.

- [ ] **Step 1: The driver**

`.port/tools/gates/ws14_webgl_browser_gate.mjs`, node-only, **zero npm deps** (node 25's built-in `WebSocket`/`fetch`):

1. Resolve repo root from `import.meta.dirname`; **exit 3** with a clear message if `target/wasm32-unknown-unknown/release/rustty_wasm.wasm` is absent (naming `.port/tools/build-main-wasm.sh --release`) or if the Chrome binary is absent. Never silently green.
2. `spawn('python3', ['-m','http.server', String(port), '--bind','127.0.0.1'], {cwd: repoRoot})` on an ephemeral port; poll `http://127.0.0.1:<port>/` until it answers.
3. `spawn(CHROME, ['--headless=new','--remote-debugging-port=0',`--user-data-dir=${mkdtempSync(...)}`,'--no-first-run','--no-default-browser-check','--disable-extensions','--enable-unsafe-swiftshader', pageUrl])`; read the port from `<user-data-dir>/DevToolsActivePort` (line 1), poll `\/json\/list` for the `page` target whose URL is the harness.
4. Open the target's `webSocketDebuggerUrl`, `Runtime.evaluate` the **contents of `example/wasm-webgl/gate.js`** with `{returnByValue: true, awaitPromise: true}`.
5. Also `Runtime.enable` + collect `Runtime.consoleAPICalled` / `Runtime.exceptionThrown`; **any** page exception or `console.error` is a gate failure (the existing harness's "0 console errors" claim becomes machine-checked).
6. Assert `pass_good === true && neg_offset_fails === true && neg_black_fails === true && glError === 0 && wasiImports === 0`. Print a compact table of `expect`, `good`, `offset`, `black`, plus the `RENDERER` string, then exit 0/1.
7. `finally`: close the socket, `kill` Chrome, `kill` the server, `rm -rf` the temp profile — even on throw.

Header comment must state, verbatim, the tolerance rule from the gate design: *exact equality is the gate; any future widening must be accompanied by a re-run proving NC1 and NC2 still FAIL under the widened rule.*

- [ ] **Step 2: The wrapper**

`.port/tools/gates/ws14_webgl_browser_gate.sh`, `chmod +x`:

```sh
#!/bin/sh
# WS14 gate: renders the Atlas.Wasm texture through WebGL2 in headless Chrome and
# asserts per-cell ink counts against a CPU expectation computed from the same
# atlas bytes — plus TWO negative controls (wrong atlas offset; black shader) that
# MUST fail on every run. Exit 0 = pass; 1 = assertion failure; 3 = environment
# missing (never silently green).
set -e
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
exec node "$ROOT/.port/tools/gates/ws14_webgl_browser_gate.mjs" "$@"
```

- [ ] **Step 3: Run it**

```bash
cd /Users/gael/projects/rustty
.port/tools/build-main-wasm.sh --release 2>&1 | tail -2
.port/tools/gates/ws14_webgl_browser_gate.sh; echo "exit=$?"
```

Expected: a table like the authoring-time observation — `expect [217,291]`, `good [217,291,0]`, `offset [252,188,0]`, `black [0,0,0]`, `pass_good: true`, `neg_offset_fails: true`, `neg_black_fails: true` — and `exit=0`. (Absolute ink counts depend on the host `monospace` font; the *equality* is what matters, which is why nothing is hardcoded.)

- [ ] **Step 4: Prove the gate's FAIL arm on the real thing (not just the built-in controls)**

Mutate **product** code, not the gate, and confirm a red gate:

```bash
cd /Users/gael/projects/rustty
python3 - <<'EOF'
p='crates/rustty-font/src/atlas.rs'; s=open(p).read()
# shift every reserved region by 1px in the returned Region -> the JS UVs go stale
open(p,'w').write(s.replace('pub extern "C" fn atlas_reserve', '#[allow(clippy::missing_safety_doc)]\npub extern "C" fn atlas_reserve', 1))
EOF
```

Then hand-edit that function to add `1` to the returned region's `x`, rebuild the wasm, and re-run the gate.

Expected: `exit=1` with the mismatch printed. Then:

```bash
git checkout -- crates/rustty-font/src/atlas.rs && .port/tools/build-main-wasm.sh --release 2>&1 | tail -1 && .port/tools/gates/ws14_webgl_browser_gate.sh; echo "exit=$?"
```

Expected: `exit=0`. Record both outputs in the PR — this is the evidence the gate reflects **product** correctness, not only its own injected mutations.

- [ ] **Step 5: Corroborate headless == live GPU**

Note in the gate's header the authoring-time finding that a live, GPU-backed Chrome and `--headless=new --enable-unsafe-swiftshader` produced **byte-identical** ink counts (`[217,291,0]`) on this host, so the headless lane is not a weaker proxy. Task 10 re-establishes this on the executing machine.

- [ ] **Step 6: Wire the opt-in CI lane**

In `.port/tools/ci.sh`, add a `--webgl` lane (following the file's existing lane conventions) that runs `.port/tools/build-main-wasm.sh --release` then the gate, and **hard-fails (exit 3)** if the `wasm32-unknown-unknown` target or Chrome is missing. It is NOT in the default lane set (heavyweight: a wasm build + a browser). Document the lane in `.port/tools/CI.md`, including the ~9s/~6s local timings and the exact two commands a developer runs.

- [ ] **Step 7: Commit**

```bash
git add .port/tools/gates/ws14_webgl_browser_gate.mjs .port/tools/gates/ws14_webgl_browser_gate.sh \
        .port/tools/ci.sh .port/tools/CI.md
git commit -m "gates: WS14 WebGL browser gate — headless Chrome pixel assertion + 2 always-run negative controls; opt-in ci.sh --webgl lane (WS14 T9)"
```

---

### Task 10: Live, visible Chrome verification (corroboration for the record)

**Files:** none (verification only; a screenshot is attached to the PR, not committed).

**Interfaces:** Consumes T8/T9. Produces the human-visible record and confirms headless == live on the executing machine.

- [ ] **Step 1: Serve**

```bash
cd /Users/gael/projects/rustty
python3 -m http.server 8731 --bind 127.0.0.1 >/tmp/ws14srv.log 2>&1 &
sleep 2 && curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8731/example/wasm-webgl/index.html
```

Expected: `200`.

- [ ] **Step 2: Load in the real browser**

Use the Chrome automation tools (`mcp__plugin_chrome-devtools-mcp_chrome-devtools__new_page` with `http://127.0.0.1:8731/example/wasm-webgl/index.html`, or the `claude-in-chrome` equivalents after loading them with `ToolSearch`). **Do not** trigger any dialog; the page must not call `alert`/`confirm`/`prompt`.

- [ ] **Step 3: Assert, don't admire**

`evaluate_script` a function that returns the same JSON `gate.js` produces (import `./gate.js` from the page, or read `window.__ws14_result` if the page stores it). Assert in the returned JSON: `pass_good === true`, `neg_offset_fails === true`, `neg_black_fails === true`, `glError === 0`, `wasiImports === 0`.

Expected: identical ink counts to Step 4 of Task 9 (same host font, same NEAREST path).

- [ ] **Step 4: Confirm a clean console**

`list_console_messages` with `types: ["error","warn"]`.

Expected: empty. Any entry is a defect to fix, not to note.

- [ ] **Step 5: Screenshot for the record**

`take_screenshot` of the WebGL canvas element. Attach to the PR, captioned *"corroboration only — the gate is the pixel assertion in Task 9, not this image."*

- [ ] **Step 6: Tear down**

```bash
pkill -f 'http.server 8731'
```

Close the tab via `close_page` (never leave an automation tab open).

- [ ] **Step 7: Record the live-vs-headless comparison**

Paste both JSON results side by side into the PR. If they differ at all, STOP and diagnose before merging — a divergence means the headless lane is not the gate it claims to be.

---

### Task 11: Ledger and roadmap repair (six wrong rows)

**Files:** Modify `.port/phase-2/DEFERRALS.md`, `.port/phase-2/VERIFICATION_DEBT.md`, `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`.

- [ ] **Step 1: `D:571` — the WebGL backend "STILL DEFERRED" bullet**

Replace the bullet (`.port/phase-2/DEFERRALS.md:571-573`) with:

```markdown
- ~~the **WebGL** backend (wasm) — out of the monomorphic Metal scope; a
  whole-backend follow-up port~~ **RESOLVED — WS14, 2026-07-29. The premise was
  wrong: there is no WebGL backend upstream.** The reference
  `src/renderer/WebGL.zig` is a **2-line placeholder** (a doc comment plus
  `pub const WebGL = @This();`) — no fields, no methods, no shaders, no draw path,
  no tests — and it is identical on the oracle's `origin/main`, so this is not a
  pin artifact. It is ported verbatim at `crates/rustty-renderer/src/webgl.rs`, and
  `renderer.zig`'s comptime switch now has all three arms
  (`#[cfg(target_arch = "wasm32")] pub use webgl::WebGL as Renderer`). Actual
  browser drawing lives where the reference's wasm model puts it — in the consumer
  (`example/wasm-webgl/`, WebGL2 over the `Atlas.Wasm` texture), gated by
  `.port/tools/gates/ws14_webgl_browser_gate.sh`.
```

- [ ] **Step 2: `D:1894` item 5 — the `App.zig` Wasm block**

Rewrite as RESOLVED: the reference block is an empty struct with all three exports commented out **upstream**; ported complete at `crates/rustty-core/src/app.rs`; and note the consequence that `crates/rustty-wasm` needs no `rustty-core` edge.

- [ ] **Step 3: `D:3065` and `D:4067` — the `main_wasm.zig` rows**

At `:3065` (the C6 bullet) split `main_wasm.zig` out of the three-file group: `main_gen.zig` / `main_build_data.zig` remain as they are; `main_wasm.zig` becomes RESOLVED with the artifact path, the build script, and the retention-set correction. At `:4067` (the app-core over-claim CORRECTION) add a dated correction-of-the-correction: the "NOT ported — DEFERRED" claim was **stale for `main_wasm.zig`**, which was already ported as an rlib at plan time (`crates/rustty-wasm/src/lib.rs`, 128 L) and is now a real cdylib artifact; keep the correction standing for the other two files.

- [ ] **Step 4: `D:4209` — the `Atlas.Wasm` per-test ledger**

Rewrite as RESOLVED, with the archaeology, because it is the single most confusing item in the ledger: the block's `wasm.toHostOwned`/`isHostOwned`/`toModuleOwned` were **deleted upstream by `9dc2e5978` (#9301, 2025-10-21)**, so "port the test with all 4 asserts live" required resurrecting the tracker (`crates/rustty-os/src/wasm.rs`, feature `wasm-host-owned`, with its own 2 reference tests). All 4 asserts now run natively under `cargo test -p rustty-font --features wasm`; `check_test_counts.py`'s `font_atlas` `apc_deferred` is `1 → 0` (13/13).

- [ ] **Step 5: `D:4363` — the misleading "WASM/web target ✅ DONE" heading**

Change the heading to:

```markdown
## WASM/web target — VT C-ABI wasm + web_canvas DONE (2026-07-15); WebGL / `main_wasm` / `Atlas.Wasm` closed by WS14 (2026-07-29)
```

and insert, immediately under it, a short scope note: what the 2026-07-15 pass covered (`rustty_vt_c.wasm` wasip1 + `rustty_web_canvas.wasm` freestanding + the `web_canvas` face/shaper, all browser-verified) and what it did **not** (any GPU/WebGL path — every pixel in that harness is 2D-Canvas — plus O13a/O14/O15/O16), with a pointer to this plan. Also correct the stale export count (`184 exports` → `199` as measured) so the row stays checkable, and note that the wasip1 lane needs `rustup target add wasm32-wasip1` on a fresh machine.

- [ ] **Step 6: `VERIFICATION_DEBT.md:157`**

Sharpen the "Out of scope by decision (not debt): the WebGL/wasm apprt" line to say *the browser **apprt runtime** (a full `apprt` App/Surface over the browser; `apprt/browser.zig` itself, 4 loc, is ported at `crates/rustty-apprt/src/browser.rs`)* — and add: *this exclusion does NOT cover WS14, which ports the WebGL placeholder + the wasm module + `Atlas.Wasm` and gates browser rendering with a falsifiable pixel assertion.*

- [ ] **Step 7: Add no new debt — or prove it**

WS14 introduces **no** verification debt: the wasm artifact builds, the browser gate runs headlessly and is CI-wireable, and both negative controls fire. If (and only if) the executing machine cannot run Chrome, add a `VERIFICATION_DEBT.md` row with the exact runnable recipe (`.port/tools/build-main-wasm.sh --release` + `.port/tools/gates/ws14_webgl_browser_gate.sh`) and the machine constraint — never a bare "unverifiable".

- [ ] **Step 8: ROADMAP row**

Replace the WS14 row in `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` with:

```
| **WS14 wasm/WebGL renderer** | `2026-07-23-ws14-wasm-webgl.md` | DONE. Premise corrected: upstream `renderer/WebGL.zig` is a 2-line placeholder (no backend exists to port) and `main_wasm.zig` was already ported as an rlib — the real residual was the artifact, the retention set, and verification. Shipped: the WebGL placeholder port + the third `Renderer` selection arm; the resurrected `os/wasm.zig` host-ownership tracker (deleted upstream by `9dc2e5978`) unblocking `Atlas.Wasm` with all 4 reference asserts live; `App.Wasm` completed; a real freestanding `rustty_wasm.wasm` cdylib (config_* + atlas_* + VT, zero wasi imports) enabled by an xev freestanding-wasm arm and default-on `renderer`/`oniguruma` features that drop 5 C `-sys` deps from the wasm link; and `example/wasm-webgl/` — a WebGL2 consumer gated by `.port/tools/gates/ws14_webgl_browser_gate.sh`, which asserts per-cell ink counts against a CPU expectation and runs two negative controls (wrong atlas offset; black shader) that must FAIL on every invocation. |
```

- [ ] **Step 9: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/DEFERRALS.md .port/phase-2/VERIFICATION_DEBT.md \
        docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md
git commit -m "parity: close O13a/O14/O15/O16; strike the misleading 'WASM DONE' framing and the stale main_wasm/Atlas.Wasm/WebGL rows (WS14 T11)"
```

---

### Task 12: Full gate sweep, WS19 cross-reference, merge

**Files:** none beyond the merge (plus any gate-mandated fix).

- [ ] **Step 1: Whole-tree gates**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -2
cargo test --workspace 2>&1 | tail -6
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all --check && echo FMT-OK
.port/tools/gates/check_acyclic.sh 2>&1 | tail -3
python3 .port/tools/gates/check_test_counts.py 2>&1 | tail -5
python3 .port/tools/gates/check_comment_coverage.py --crate rustty-font 2>&1 | tail -5
python3 .port/tools/gates/check_comment_coverage.py --crate rustty-renderer 2>&1 | tail -5
python3 .port/tools/gates/check_stale_comments.py 2>&1 | tail -5
cargo test -p rustty-apprt-c --test panic_policy 2>&1 | tail -3
```

Expected: all green. `check_stale_comments` matters here specifically: T2/T5/T7 removed "deferred"/"BLOCKED" claims that are no longer true, and this gate is what catches any that were missed. Do **not** run `.port/tools/gates/no_homebrew_gettext_gate.sh`.

- [ ] **Step 2: WS19 cross-reference (do not duplicate)**

```bash
cd /Users/gael/projects/rustty
grep -n 'Opaque' include/rustty.h | head
printf '#include <rustty.h>\nint main(void){return 0;}\n' | clang -fsyntax-only -Wall -Iinclude -x c - 2>&1 | head -5
```

Expected: still `unknown type name 'Opaque'` at `:2455`/`:2457`. That is **WS19 T3's** fix (`docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md:405-459`). WS14 must not touch `crates/rustty-vt-c/cbindgen.toml` or `include/rustty.h`. Record the state in the PR and note that WS14's new exports (`atlas_*`, `config_*`) are **wasm-only, unprefixed by design, and not cbindgen-exported**, so they add nothing to the A5 header surface. If WS19 has landed by then, re-run the compile probe and expect it clean; either way, do not re-fix.

- [ ] **Step 3: The wasm lane, end to end, from clean**

```bash
cd /Users/gael/projects/rustty
.port/tools/build-main-wasm.sh --release 2>&1 | tail -2
.port/tools/build-web-canvas-wasm.sh --release 2>&1 | tail -2
.port/tools/gates/ws14_webgl_browser_gate.sh; echo "exit=$?"
cargo test -p rustty-wasm -- --ignored 2>&1 | tail -4
```

Expected: both artifacts `Finished`; gate `exit=0`; artifact-shape test passing. Confirm the pre-existing `example/wasm-web-canvas` harness still works (it must not regress from the glue change in T4 Step 5) by loading it once in the browser as in Task 10.

- [ ] **Step 4: Merge**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull
git merge --no-ff ws14-wasm-webgl -m "merge: WS14 wasm/WebGL — WebGL placeholder port, Atlas.Wasm live (tracker resurrected), real rustty_wasm.wasm cdylib, falsifiable WebGL2 browser gate"
git push
```

- [ ] **Step 5: Verify the merge result**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -2
cargo test --workspace 2>&1 | tail -4
.port/tools/build-main-wasm.sh --release 2>&1 | tail -1
.port/tools/gates/ws14_webgl_browser_gate.sh; echo "exit=$?"
```

Expected: `Finished`, tests green, `exit=0`.

- [ ] **Step 6: Delete the branch and confirm a clean tree**

```bash
cd /Users/gael/projects/rustty
git branch -d ws14-wasm-webgl && git push origin --delete ws14-wasm-webgl
git status --short
```

Expected: no output.

---

## Success criteria

1. `crates/rustty-renderer/src/webgl.rs` exists and is a faithful port of the 2-line reference; `rustty_renderer::Renderer` resolves for `wasm32`; all three reference switch arms are live.
2. `cargo test -p rustty-font --features wasm` runs the `Atlas.Wasm` `test "happy path"` with **all 4 reference asserts live**; `check_test_counts.py` has `font_atlas` at `apc_deferred = 0`.
3. `crates/rustty-os/src/wasm.rs` carries the resurrected tracker plus its **2** reference tests, feature-gated, with `9dc2e5978` cited in-source.
4. `App.Wasm` is a complete (empty, upstream-commented) port; no in-source deferral wording remains.
5. `.port/tools/build-main-wasm.sh --release` produces `rustty_wasm.wasm` with **zero wasi imports** and the ten `config_*`/`atlas_*` exports present.
6. `.port/tools/gates/ws14_webgl_browser_gate.sh` exits 0 with `pass_good && neg_offset_fails && neg_black_fails`, and was demonstrated to exit 1 under a **product-code** mutation (T9 Step 4).
7. Live Chrome corroborates the headless numbers exactly, with an empty error/warn console (T10).
8. All six wrong ledger rows are repaired; `D:4363`'s heading can no longer be read as "wasm is finished"; **no new deferral is created**.
9. `cargo build --workspace`, `cargo test --workspace`, clippy `-D warnings`, `cargo fmt --check`, `check_acyclic.sh`, `check_test_counts.py`, `check_stale_comments.py`, and the DG5 `panic_policy` gate are all green — with native test totals unchanged except for WS14's additions.
