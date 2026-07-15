# rustty completion plan + Sub-project #1: macOS on-screen windowing — Design

_Date: 2026-07-08 · Status: approved (design), pending spec review · Author: autonomous port session_

## 1. Context & goal

`rustty` is a full-fidelity Zig→Rust port of ghostty (Zig 1.3.2-dev), governed by
`.port/PORT_RULES.md` (R1 100% parity, R2 rustty identity, R3 self-contained C deps,
R4 acyclic crates, R5 gates-invariant, R6 Miri/ASan). The goal of THIS effort is a
**full and complete port** — every target in scope: **macOS native, Linux (GTK4/libadwaita),
Windows, and WASM/web + iOS** — **reusing non-Zig code** (the Swift macOS/iOS app, dcimgui
C++, GLSL/Metal shaders, GTK Blueprint XML, vendored C/C++ libs) rather than reimplementing it.

The complete gap analysis (14-subsystem inventory + 3 completeness critics + synthesis, from
the `full-gap-analysis` workflow) lives in **`.port/phase-2/GAP_ANALYSIS.md`** (readable) and
**`.port/phase-2/gap-analysis-raw.json`** (per-unit data). This document is the actionable
plan on top of that inventory: the master sequence, the two locked product forks, and the
detailed design for the FIRST sub-project.

## 2. Reconciled current state

~78% of the port is done. Fully complete subsystems: **terminal-vt, config-input, os-platform**.
Mostly done (headless logic ported; seams gated on windowing/threads): app-core, apprt-embedded,
termio, font, cli, deps-runtime, misc-support. Not started: **apprt-gtk** (zero counterpart).
Partial: renderer (offscreen done + golden-tested; present pending), inspector (dcimgui FFI done;
widgets pending), macos-swift-app (reuse pending C-ABI glue).

**Key reconciliation (corrects the synthesis roadmap's over-scoping):** the "runtime-thread / D6"
workstream is **already done and proven**. `crates/rustty-termio/tests/live_shell.rs` runs a real
`/bin/sh` whose output round-trips into the `Screen` through the real Exec + rustty-xev kqueue loop
+ ReadThread + `Termio::processOutput`. `termio/{thread,exec,termio_impl}.rs` carry **no live
seams** on the posix/macOS path. D6's only true remainders are (a) the **renderer `Thread` event
loop**, which is inseparable from the GPU present path, and (b) a small **`cf_release_thread`
rewire** (a local placeholder `xev` module — `crates/rustty-os/src/cf_release_thread.rs:339…373`
— that predates rustty-xev existing). Both are absorbed into Sub-project #1.

## 3. Locked product decisions (forks)

### 3.1 Sequencing — macOS-first (to a shipping app)

Critical path: **macOS windowing → macOS Swift-app reuse**, with the renderer `Thread` loop and
`cf_release_thread` folded into windowing (D6 is otherwise done). Rationale: earliest runnable
window; highest reuse-per-effort (~32k LOC Swift app for ~L of C-ABI glue); smallest new windowing
surface (offscreen Metal already done); validates the shared core/renderer/termio **before** GTK
(the XL chunk) inherits it. iOS then largely falls out of the same Swift/Metal work. The low-risk
headless tails (TUI-CLI, unused generators, Linux fontconfig, ledger hygiene) run as **parallel
fill**, never as a front-loaded blocking phase.

### 3.2 C-ABI symbol identity — `ghostty_*` aliases, `rustty_*` canonical

librustty exports `rustty_*` as the real, documented ABI (`include/rustty.h`) — satisfying R2 —
**plus** a *generated* thin alias layer adding `ghostty_* → rustty_*` symbols and a `ghostty.h`
compat view over the identical struct/enum layouts. Rationale: the reused Swift app (~150 files,
~163 hard-coded `ghostty_` symbols) stays **byte-identical to upstream**, so re-syncing ghostty's
ongoing Swift development is a plain copy; 100% of the adaptation lives in one generated file in
rustty's tree; zero Swift source touched. This mirrors the blessed `identity-oracle` build-feature
precedent (reference-compat as an isolated, supported profile). Detailed design deferred to
Sub-project #3.

## 4. Master sequence (all targets in scope)

Each numbered item is its own **spec → plan → implement** cycle. D6 is done, so #1 begins at windowing.

1. **macOS on-screen windowing** (this doc) — Metal windowed present + renderer `Thread` loop +
   IOSurfaceLayer + `renderer_seam`→real Renderer + apprt-c macOS objc leaves + `cf_release_thread`
   rewire. Delivers: first pixels → a Rust harness showing a live terminal frame.
2. **macOS Swift-app reuse** — `ghostty_*` alias layer + config cbindgen header + umbrella
   modulemap + xtask packaging librustty into a signed `.xcframework` + identity rebrand
   (io.airely.rustty). Delivers: a launchable rustty.app. **iOS** widens the Darwin gate off this.
3. *Parallel fill (off critical path, low-risk):* **ledger hygiene** (§7) → **TUI-CLI**
   (boo/list_colors/list_keybinds/list_themes over a Rust vaxis+zf substrate) → **unused
   generators** (synthetic/bench/src-extra) → **Linux fontconfig** (GTK prereq).
4. **GTK/Linux** frontend (~23k Zig LOC) + OpenGL renderer backend + a from-scratch gtk4-rs/adwaita
   binding stack + reused `.blp`/`.css` assets + GLSL shaders. (XL — on the proven core.)
5. **Windows** — ConPTY spawn + Windows pty + win32 shims (core + CLI only; no upstream GUI frontend).
6. **Inspector** (dcimgui developer overlay) — Zig widget logic + imgui_impl_metal/opengl backends;
   dcimgui C++ already vendored. Slots after a live layer exists; parallelizable.
7. **WASM/web + iOS finish** — web_canvas font+shaper, WebGL renderer, os/wasm, the browser apprt.
   Most isolated, largest net-new, last.

## 5. Sub-project #1 — macOS on-screen windowing (detailed design)

### 5.1 Architecture (the key insight)

The offscreen renderer already draws each frame into an **IOSurface-backed `MTLTexture`** (`Target`,
golden-image tested). On-screen present is therefore **not** a drawable/swapchain rework — it is:

```
generic.drawFrame → Frame.complete(sync) → bufferCompleted → present
    → IOSurfaceLayer.setSurface(iosurface)   // set the IOSurface as the CALayer's `contents`
    → (layer is attached to the host NSView)
```

This is faithful to ghostty's actual design (`GPU_RD_BLUEPRINT.md` §1, §F6). The deferred surface
is small and self-contained: `IOSurfaceLayer.zig` in full, the CVDisplayLink block, and ~10
layer-attach/present lines in `Metal.zig`/`generic.zig`, plus the renderer `Thread` loop.

### 5.2 Scope (units)

- **A. IOSurfaceLayer + dep.** Add `objc2-quartz-core` (CAMetalLayer/CALayer; not yet in lock). Port
  `src/renderer/metal/IOSurfaceLayer.zig` (183 LOC) → `crates/rustty-renderer/src/metal/iosurface_layer.rs`:
  the `CALayer` subclass (`allocateClassPair`), `init`/`release`, `setSurface` (main-thread
  `dispatch`), `setSurfaceSync`, `setDisplayCallback`. *The present primitive.*
- **B. Metal.zig on-screen delta.** `crates/rustty-renderer/src/metal/metal_impl.rs:334,384`
  (`init`/`loop_enter`): add the `layer` field, NSView attach (`wantsLayer`/`addSublayer`/
  `clipsToBounds`), `surfaceSize` from `layer.bounds`, `present`/`presentLastTarget`.
- **C. generic.rs R5 present + vsync.** `crates/rustty-renderer/src/generic.rs:1795,1841` (the two
  R5 windowed-present seams) + the CVDisplayLink block (`display_link`/`hasVsync`/`setMacOSDisplayID`/
  `displayLinkCallback`) + `loopEnter`/`loopExit` + `surfaceSize`-from-layer coupling. `metal/frame.rs:198`
  (async `addCompletedHandler` present).
- **D. Renderer `Thread` event loop.** Port `src/renderer/Thread.zig` over the real rustty-xev:
  `wakeup`/`draw_now` `Async` + `render_h`/`draw_h`/`cursor_h` `Timer` + `threadMain` + the render
  `Mailbox` drain + `wakeupCallback`/`drawCallback`. Replaces the `renderer_seam::Thread<A>`
  placeholder. Lands in `crates/rustty-renderer/src/thread.rs` (currently only `DerivedConfig`).
- **E. Real Renderer in core.** Replace `crates/rustty-core/src/renderer_seam.rs`'s `Renderer`
  placeholder (`surfaceInit`/`init`/`finalizeSurfaceInit`/`threadEnter`/`drawFrame`/`deinit`,
  currently `unimplemented!()`) with the real `rustty_renderer::Renderer`; wire `Surface::draw →
  drawFrame`. Preserve acyclicity (R4) — core already deps renderer; no new edge.
- **F. apprt-c macOS host leaves.** `crates/rustty-apprt-c/src/lib.rs` + `crates/rustty-apprt/src/embedded.rs`
  macOS `#[cfg(target_os="macos")]` leaves needed to host a window: the NSView/CAMetalLayer host
  handshake from the C-ABI (the embedded `Surface` receives the host layer), window blur
  (`CGSSetWindowBackgroundBlurRadius`), `display_id`, content-scale. (quicklook/secure-input/inspector
  leaves stay deferred to #6.)
- **G. cf_release_thread rewire.** `crates/rustty-os/src/cf_release_thread.rs` — replace the local
  placeholder `xev` mod (`:339…373` `unimplemented!()`) with the real rustty-xev `Loop`/`Async`.

### 5.3 Internal chunking (each independently testable, its own gate-green commit)

| chunk | deliverable | test / proof |
|---|---|---|
| **W0** | `objc2-quartz-core` dep + `cf_release_thread` rewire + `IOSurfaceLayer` port | unit: layer sets an IOSurface as `contents`; `cf_release_thread` runs on the real xev loop |
| **W1** | Metal on-screen delta (B) + generic R5 present (C, present half) | `Metal::init` binds a `CAMetalLayer` to a test `NSView`; `generic` presents ONE real frame of terminal state on-screen — **FIRST PIXELS** |
| **W2** | renderer `Thread` loop over rustty-xev (D) + CVDisplayLink (C, vsync half) | the render thread draws on vsync; a bounded-pump integration test (live_shell.rs pattern) ticks the render loop and observes a present |
| **W3** | swap `renderer_seam` → real Renderer/Thread in core (E) | core `Surface::draw` drives a live `drawFrame`; workspace build + acyclic gate green |
| **W4** | apprt-c macOS host handshake + blur/display_id/scale (F) | a Rust integration harness opens an `NSWindow`, hosts the layer, and shows a live terminal frame — no Swift app yet |

### 5.4 Testing strategy

macOS-gated (`#[cfg(target_os = "macos")]`) integration tests following the `live_shell.rs` pattern:
build a real `CAMetalLayer`/`NSWindow`, assert `present` succeeds, and **reuse the offscreen
golden-image gate** for pixel parity (render the same state on-screen and compare the IOSurface
readback to the committed golden). objc FFI is Miri-excluded (foreign calls) but the render-thread
raw-pointer aliasing is **proven SB/TB-clean on the `mirihar` harness** (`.port/phase-2/mirihar-chunk3b`)
before shipping, and recorded in `.port/phase-2/MIRI_EXCEPTIONS.md`. All standing gates stay green:
build, test, clippy `-D warnings`, fmt, `check_acyclic.sh`, `check_test_counts.py`,
`check_comment_coverage.py`, Miri-subset (pure), ASan (FFI).

### 5.5 Technical decisions (made; follow established patterns)

- **objc2-quartz-core** for `CAMetalLayer`/`CALayer` — same objc2-\* FFI path already used for
  CoreText/Metal (R3-consistent; replaces ghostty `pkg/macos`+`pkg/objc`).
- **IOSurface-as-`contents` present** (not `CAMetalDrawable`/swapchain) — faithful to ghostty; reuses
  the done, golden-tested `Target`.
- **Render-thread ownership: construct-then-spawn / pid-only-Box-owned / raw-self helpers** — the
  render thread owns the `Renderer`; the main thread holds only a `*const Mailbox` (Box pointee,
  outside the struct's SB coverage) + `Async` notifiers captured before spawn. This is the
  [ADV-FIX F1/F2] + search construct-then-spawn discipline the user approved for D6; proven on the
  `mirihar` harness (a `renderthread_naive`=UB / `renderthread_shipped`=CLEAN candidate pair) before
  shipping W2. No `&mut *self` whole-struct reborrow while the render thread holds a raw alias.
- **Subprocess/arena divergence** (sanctioned): where the reference stores raw pointers into a
  temporary arena (`&mut`-alias UB under Rust SB/TB), use the faithful-but-safe pid-only / Box-owned
  substitution.

### 5.6 Acyclicity & identity notes

`rustty-core` already depends on `rustty-renderer` (renderer_seam is core-local), so E adds **no new
crate edge** — `check_acyclic.sh` stays green. The `RendererConfig` accessor-trait (config↔renderer
back-edge) is already landed; the live Renderer reads config through it, never naming a config type.

### 5.7 Out of scope for #1 (explicitly deferred)

Inspector/dcimgui leaves (→ #6), quicklook/secure-input, the Swift app + xcframework + `ghostty_*`
aliases (→ #2), GTK OpenGL backend (→ #4), Windows/WASM present. The `generic.rs:4064` inspector
draw seam stays `unimplemented!()` until #6.

## 6. Risks

- **High:** objc2-quartz-core CAMetalLayer/CVDisplayLink surface may have gaps vs the mapped API
  (mitigation: the blueprint already verified objc2-metal/io-surface/core-graphics; quartz-core is
  the one unverified add — spike it in W0 and flag any gap + workaround per the blueprint's §2.138
  discipline). **High:** render-thread aliasing soundness (mitigation: mirihar proof gates W2).
- **Med:** the NSView/layer host handshake across the C-ABI (who owns the layer, main-thread
  affinity) — mitigation: W4 uses a Rust-owned NSWindow harness to isolate it from the Swift app.

## 7. Ledger hygiene (fold into Sub-project #3 parallel fill — tracked here so it isn't lost)

The completeness critics found **no untracked orphan seams in reachable core logic**, but
`DEFERRALS.md` has drifted. Add entries for: synthetic generators, benchmark harness, `src/extra`
generators, mdgen/webgen, stb_image_resize, the `ghostty_*` alias layer, the `.xcframework`
repackaging step. Correct the app-core over-claims (`main_gen`/`main_wasm`/`main_build_data` are
ledger-deferred, not done). Strip the stale `PORT STATUS` skeleton comments in
`rustty-vt/src/{terminal,screen}.rs` (bodies are implemented; only the dcimgui `imguiKey` seam
remains). Leave the z2d (supersample/dashed) and wuffs (swizzle) subset seams as
documented-unreachable (the sole consumer never constructs those input variants).

## 8. References

- `.port/phase-2/GAP_ANALYSIS.md` — full 14-subsystem inventory + 10-workstream roadmap.
- `.port/phase-2/GPU_RD_BLUEPRINT.md` — the offscreen/on-screen split, objc2 FFI mapping, deferred surface.
- `.port/phase-2/EMBEDDED_BLUEPRINT.md` — apprt embedded C-ABI + macOS-first strategy.
- `.port/PORT_RULES.md` — R1–R6. `.port/phase-2/MIRI_EXCEPTIONS.md` — SB/TB exception ledger.
- `crates/rustty-termio/tests/live_shell.rs` — the proven D6 live-shell harness (the testing pattern).
- Reference (read-only): `/Users/gael/projects/ghostty/src/renderer/{Thread,Metal,generic}.zig`,
  `src/renderer/metal/{IOSurfaceLayer,Frame,Target}.zig`.
