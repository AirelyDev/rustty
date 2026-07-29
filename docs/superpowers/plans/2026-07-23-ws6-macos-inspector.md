# WS6 — macOS Terminal Inspector (Dear ImGui over Metal) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ⌘⌥I (`super+alt+i`) opens a LIVE, rendering Dear ImGui terminal inspector in `Rustty.app`, with the one remaining unported reference body filled, the ledgers made truthful, and the GTK/OpenGL3 inspector still building on Linux.

**Architecture:** WS6 is **NOT a fresh port** — see "Scope finding" below. The dcimgui `ImGui_ImplMetal` vendor+compile+FFI, the ~4.9k-LOC shared inspector widget render (`crates/rustty-inspector`), the core `Surface.activate/deactivateInspector` seams, `embedded::Inspector::{init,initMetal,renderMetal}`, the `rustty_inspector_metal_*` C exports, and the Swift `InspectorView`/`Rustty.Inspector` host all landed on `main` in the 2026-07-14 inspector series (commits `c6a68e5`…`c62ab07`). WS6 therefore (A) proves the Metal backend really rasterizes with a real-`MTLDevice` offscreen readback KAT, (B) closes the single genuinely unported reference body (`Inspector.updateContentScale`'s `ImGuiStyle` scaling) and proves the widget-render residual is zero via the G3b gate, (C) removes the now-false "Phase-3 leaf / crash-guard sentinel" claims from code comments and performs the never-completed live ⌘⌥I shakedown, and (D) runs both-platform build gates, strikes five stale ledger sections, and merges.

**Tech Stack:** Rust 1.93.1 (pinned), `crates-sys/dcimgui-sys` (Dear ImGui 1.92.5-docking + Dear Bindings 0.17, `cc` + `bindgen`), `imgui_impl_metal.mm` (ObjC++, macOS) / `imgui_impl_opengl3.cpp` (non-macOS), `objc2` 0.6 + `objc2-metal` 0.3, Xcode 26.5 (`macos/build-rustty.sh`), oracle at `/Users/gael/projects/ghostty`.

---

## Scope finding (evidence-based — READ BEFORE EXECUTING)

The roadmap's WS6 row ("port the ~4.4k-LOC inspector widget render; fill `Surface.activate/deactivateInspector` + `embedded::Inspector::{init,initMetal,renderMetal}` + `rustty_inspector_metal_*`; remove the no-op crash sentinel") describes work that is **already on `main`**. Evidence gathered at authoring time (2026-07-28, `HEAD` = `74226dd`):

| Roadmap item | Actual state | Evidence |
|---|---|---|
| dcimgui `ImGui_ImplMetal` vendor + compile | **DONE** | `crates-sys/dcimgui-sys/build.rs:180-219` compiles `vendor/imgui/backends/imgui_impl_metal.mm` as a separate ObjC++ `cc::Build` (`-x objective-c++ -fno-objc-arc`) into a `dcimgui_metal` archive and links `Metal`/`QuartzCore`/`Foundation`. Commit `c6a68e5`. |
| `ImGui_ImplMetal_*` FFI | **DONE** | `crates-sys/dcimgui-sys/src/lib.rs:202-241` (6 `extern "C"` decls, `#[cfg(target_os = "macos")]`) + link KAT `link_imgui_impl_metal_ffi` (lib.rs:355-373). Commit `9038a87`. |
| GTK OpenGL3 arm not regressed | **PRESENT** | Same files, `#[cfg(not(target_os = "macos"))]` block (lib.rs:253-281) + `opengl3_symbols_link` KAT. Commits `657f223`, `f780650`. |
| ~4.4k-LOC widget render | **DONE, SHARED, INHABITED** | Oracle `src/inspector/**` = **4873** Zig LOC across 13 files. Port = **11 625** Rust LOC across the same 13 modules (`crates/rustty-inspector/src/{Inspector,lib,units,widgets}.rs` + `widgets/{key,page,pagelist,renderer,screen,style,surface,terminal,termio}.rs`). Zero `unimplemented!()`/`todo!()` in the crate; the only three `TODO` strings are faithful copies of oracle TODOs. Commits `b344e09`…`d6be9b2`. |
| `Surface.activate/deactivateInspector` | **DONE (live)** | `crates/rustty-core/src/surface.rs:2873` / `:2967` — real bodies (heap-box the inspector, publish the `*mut dyn InspectorOverlay` + type-erased `inspector_ptr` under `renderer_state.mutex`, push `Message::Inspector(true/false)` to the renderer mailbox and termio). Commit `2dfcc32`. |
| `embedded::Inspector::{init,initMetal,renderMetal}` | **DONE (live)** | `crates/rustty-apprt/src/embedded.rs:1933` / `:2018` / `:2107` — real `ImGui_CreateContext`, `ImGui_ImplMetal_Init`, the double-frame loop + `renderCommandEncoderWithDescriptor:` objc2 msgSends + `ImGui_ImplMetal_RenderDrawData`. Commits `027f695`, `86b04b4`, `0b8f7af`. |
| `rustty_inspector_metal_*` exports | **DONE (live)** | `crates/rustty-apprt-c/src/lib.rs:2927` / `:2952` / `:2984`; declared in `include/rustty-embed.h:1197-1199`; called by `macos/Sources/RusttyKit/Rustty.Inspector.swift:84-95` and driven by `macos/Sources/RusttyKit/Surface View/InspectorView.swift`. |
| Crash-guard no-op sentinel | **ALREADY REMOVED FROM CODE; LEDGER STALE** | `grep -rn "not yet ported" crates/` has **zero** hits in `rustty-apprt`/`rustty-apprt-c`. `Inspector::init` is fully live. The DEFERRALS §"P2 crash-guard" section (line 4581) still claims it is guarded. **The removal WS6 owes is a ledger + code-comment removal, not a code change.** |
| ⌘⌥I keybind | **PORTED** | `crates/rustty-config/src/value_types.rs:4413` (`super+alt+i` macOS default). |
| Live ⌘⌥I shakedown | **NEVER PERFORMED** | The prior plan `docs/superpowers/plans/2026-07-14-inspector-port.md` Task 18 ("End-to-end shakedown — ⌘⌥I opens the inspector") has no corresponding commit, and every checkbox in that file is unticked. This is WS6's headline deliverable. |

**The genuine residual WS6 must produce:**

1. **One unported reference body:** `embedded::Inspector::updateContentScale` (`crates/rustty-apprt/src/embedded.rs:2285-2304`) elides the oracle's `ImGui_SetCurrentContext` + `ImGuiStyle_ImGuiStyle`/`ImGuiStyle_ScaleAllSizes`/`ImGui_GetStyle` block (oracle `src/apprt/embedded.zig:1111-1126`) behind a stale "dcimgui leaf — deferred until dcimgui-sys is wired" comment. dcimgui-sys **is** wired. On a HiDPI display the inspector's ImGui widget metrics never scale → real user-visible divergence.
2. **No proof the Metal backend rasterizes.** The existing `link_imgui_impl_metal_ffi` KAT only takes function addresses. There is no test that a real `MTLDevice` + `ImGui_ImplMetal_RenderDrawData` actually writes pixels.
3. **False code comments** in `rustty-apprt-c`/`rustty-apprt` asserting these paths are `unimplemented!()` Phase-3 leaves that "cannot happen this milestone".
4. **Five stale ledger sections** (`.port/phase-2/DEFERRALS.md` at ~954, ~1125, ~1817/1827, ~2107, ~4581; `.port/phase-2/MACOS_APP_GAPS.md` lines 14/66/75/99).
5. **The live shakedown + both-platform gate.**

Do NOT re-port anything in the table above. If a step finds a table row to be false, stop and report — do not silently widen scope.

---

## Global Constraints (inherited from `2026-07-23-rustty-completion-ROADMAP.md`)

- R1 full fidelity vs the ghostty oracle at `/Users/gael/projects/ghostty` (pinned `4749c4e`); the oracle is reference-only — **no build-time dependency on it** (R3). The inspector sources are already vendored under `crates-sys/dcimgui-sys/vendor/`.
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty, GHOSTTY_→RUSTTY_). The `ghostty_*` C-ABI alias layer was deleted upstream at `e4cc852` — **new/edited exports are `rustty_*` only**.
- R4/R5: acyclic crate graph; gates (`.port/tools/gates/check_acyclic.sh`, the A4 ABI gate `check_embed_h_abi.sh`, Miri lanes) are invariant — never weakened to pass.
- DG5: C-ABI panic policy is **ABORT** — no unwind across `extern "C"`. No `catch_unwind` in any `rustty_inspector_*` export; the drop-guard (`Defer`) discipline in `embedded.rs` relies on `panic=abort` (no unwinding path).
- **The GTK/Linux (OpenGL3) inspector must keep building.** Everything WS6 adds on the Metal side is `#[cfg(target_os = "macos")]`-gated or target-gated in `Cargo.toml`. Task 6 enforces this with a real VM build.
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: strike stale `.port/phase-2/DEFERRALS.md` / `MACOS_APP_GAPS.md` / `VERIFICATION_DEBT.md` entries in the same branch (Task 7).
- Commit style: `subsystem: summary (chunk-ref)` with chunk-ref `(WS6 T<n>)`. Branch per WS; merge to main only with both platforms green.
- Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String`.
- Raw-pointer discipline for C-ABI args: out-params are `*mut T`, never `&mut T` re-read across calls.
- Manual/visual verification is **screenshot-based**, never inferred from a process tree or a log line (Task 5).

## File Structure

- `crates-sys/dcimgui-sys/Cargo.toml` — modify: add a macOS-target `[target.'cfg(target_os = "macos")'.dev-dependencies]` table with `objc2` + `objc2-metal` (dev-only; no runtime dep, no graph change).
- `crates-sys/dcimgui-sys/tests/metal_offscreen.rs` — **create**: the real-`MTLDevice` offscreen render KAT for `ImGui_ImplMetal_*`. Sole responsibility: prove the vendored Metal backend rasterizes ImGui draw data into a texture. `#![cfg(target_os = "macos")]` — compiles to nothing on Linux.
- `crates/rustty-apprt/src/embedded.rs` — modify: fill `Inspector::updateContentScale` (`:2285-2304`); add the `inspector_style_tests` unit-test module; correct the stale "Phase-3 leaf" PORT NOTEs on `Inspector::init` / `Inspector::deinit`.
- `crates/rustty-apprt-c/src/lib.rs` — modify: correct the stale "Phase-3 leaf / cannot happen this milestone" PORT NOTEs on `rustty_surface_inspector`, `rustty_inspector_metal_init`, `rustty_inspector_metal_render`, `rustty_inspector_metal_shutdown`. No signature or behaviour change.
- `.port/phase-2/DEFERRALS.md` — modify: strike/resolve the five stale inspector sections.
- `.port/phase-2/MACOS_APP_GAPS.md` — modify: strike the four stale inspector rows.
- `.port/phase-2/VERIFICATION_DEBT.md` — modify: record the macOS inspector runtime sign-off (and keep the Linux one).
- `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` — modify: tick WS6.

## Phase map (each phase is independently testable)

| Phase | Tasks | Deliverable | Independent gate |
|---|---|---|---|
| **A — dcimgui-sys `ImGui_ImplMetal` FFI: verify, do not re-port** | T1 | Offscreen Metal render KAT | `cargo test -p dcimgui-sys` green on macOS; file compiles to nothing on Linux |
| **B — Shared widget-render residual** | T2, T3 | G3b zero-drop proof; `updateContentScale` filled + KAT | `check_comment_coverage.py --crate rustty-inspector --strict` GREEN; `cargo test -p rustty-apprt` green |
| **C — Embedded/C-export seams + sentinel removal + live shakedown** | T4, T5 | Truthful code comments; **screenshot of a live ⌘⌥I inspector** | `check_stale_comments.sh` green; screenshot artifact |
| **D — Gates, ledgers, merge** | T6, T7, T8 | Both-platform build, truthful ledgers, merged branch | macOS + `ssh rvm` builds green; merge |

---

## Task 0: Branch

- [ ] **Step 1: Create the WS6 branch**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull --ff-only
git checkout -b ws6-macos-inspector
```

- [ ] **Step 2: Confirm the scope-finding table is still true** (guards against drift since authoring)

```bash
cd /Users/gael/projects/rustty
grep -c "unimplemented!\|todo!" crates/rustty-inspector/src/*.rs crates/rustty-inspector/src/widgets/*.rs | grep -v ':0$' || echo "OK: zero unimplemented in rustty-inspector"
grep -rn "not yet ported" crates/rustty-apprt/src/embedded.rs crates/rustty-apprt-c/src/lib.rs || echo "OK: no crash-guard sentinel left in code"
grep -n "ImGui_ImplMetal_Init" crates-sys/dcimgui-sys/src/lib.rs
grep -n "imgui_impl_metal.mm" crates-sys/dcimgui-sys/build.rs
```

Expected: `OK: zero unimplemented in rustty-inspector`, `OK: no crash-guard sentinel left in code`, one hit each for the last two greps. If any expectation fails, STOP and report — the plan's scope finding has drifted.

---

# PHASE A — dcimgui-sys `ImGui_ImplMetal` FFI (verify, do not re-port)

**Phase boundary:** Phase A touches ONLY `crates-sys/dcimgui-sys`. It adds no runtime dependency and no non-test code. It ends when `cargo test -p dcimgui-sys` proves the vendored Metal backend rasterizes real pixels.

## Task 1: Real-`MTLDevice` offscreen render KAT for `ImGui_ImplMetal_*`

**Files:**
- Create: `crates-sys/dcimgui-sys/tests/metal_offscreen.rs`
- Modify: `crates-sys/dcimgui-sys/Cargo.toml` (add a macOS-gated `dev-dependencies` table)

**Interfaces:**
- Consumes: the already-declared `dcimgui_sys::{ImGui_CreateContext, ImGui_DestroyContext, ImGui_SetCurrentContext, ImGui_GetIO, ImGui_NewFrame, ImGui_Render, ImGui_GetDrawData, ImGui_Begin, ImGui_End, ImGui_Text, ImGui_SetNextWindowPos, ImGui_SetNextWindowSize, ImGui_StyleColorsDark, ImVec2, ImGui_ImplMetal_Init, ImGui_ImplMetal_NewFrame, ImGui_ImplMetal_RenderDrawData, ImGui_ImplMetal_Shutdown}`.
- Produces: the integration test `metal_offscreen_render_writes_pixels` (macOS only). Nothing depends on it; it is a gate.

- [ ] **Step 1: Write the failing test**

Create `crates-sys/dcimgui-sys/tests/metal_offscreen.rs`:

```rust
//! Offscreen Metal render KAT for the vendored Dear ImGui Metal backend
//! (`vendor/imgui/backends/imgui_impl_metal.mm`, compiled by `build.rs` into
//! the `dcimgui_metal` archive on macOS).
//!
//! The unit-test `link_imgui_impl_metal_ffi` in `src/lib.rs` only proves the
//! `ImGui_ImplMetal_*` symbols RESOLVE. This KAT proves the backend actually
//! RASTERIZES: it builds a real `MTLDevice`, an offscreen BGRA8 texture, a
//! render pass that clears it to opaque black, runs one full ImGui frame that
//! draws a window, encodes the resulting `ImDrawData` through
//! `ImGui_ImplMetal_RenderDrawData`, commits, waits, reads the texture back,
//! and asserts non-black pixels exist.
//!
//! This is the WS6 Phase-A evidence that ⌘⌥I can render at all. Metal is
//! Darwin-only, so the whole file compiles to nothing elsewhere (the GTK /
//! OpenGL3 inspector build is untouched).

#![cfg(target_os = "macos")]
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Mutex;

use objc2_metal::{
    MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice,
    MTLLoadAction, MTLOrigin, MTLPixelFormat, MTLRegion, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLSize, MTLStorageMode, MTLStoreAction, MTLTexture,
    MTLTextureDescriptor, MTLTextureUsage,
};

use dcimgui_sys::{
    ImGui_Begin, ImGui_CreateContext, ImGui_DestroyContext, ImGui_End, ImGui_GetDrawData,
    ImGui_GetIO, ImGui_ImplMetal_Init, ImGui_ImplMetal_NewFrame, ImGui_ImplMetal_RenderDrawData,
    ImGui_ImplMetal_Shutdown, ImGui_NewFrame, ImGui_Render, ImGui_SetCurrentContext,
    ImGui_SetNextWindowPos, ImGui_SetNextWindowSize, ImGui_StyleColorsDark, ImGui_Text, ImVec2,
};

/// Dear ImGui's current context is a plain global (`GImGui`); `cargo test`
/// runs each `#[test]` on its own thread. Serialize any test that creates a
/// context — same discipline as `src/lib.rs`'s `IMGUI_CONTEXT_TEST_LOCK`.
static IMGUI_CONTEXT_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Framebuffer edge length, in pixels, for the offscreen render target.
const DIM: usize = 256;

#[test]
fn metal_offscreen_render_writes_pixels() {
    let _guard = IMGUI_CONTEXT_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // --- Metal device + queue -------------------------------------------
    let Some(device) = MTLCreateSystemDefaultDevice() else {
        // No Metal device (should not happen on this hardware, but a headless
        // CI box without a GPU must not hard-fail the suite).
        eprintln!("skipping: MTLCreateSystemDefaultDevice() returned nil");
        return;
    };
    let queue = device
        .newCommandQueue()
        .expect("newCommandQueue returned nil");

    // --- Offscreen BGRA8 render target (Shared storage => CPU readback) ---
    let tex_desc = MTLTextureDescriptor::new();
    tex_desc.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    // SAFETY: objc2 marks the dimension setters unsafe; DIM is a plain size.
    unsafe {
        tex_desc.setWidth(DIM);
        tex_desc.setHeight(DIM);
    }
    tex_desc.setStorageMode(MTLStorageMode::Shared);
    tex_desc.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    let texture = device
        .newTextureWithDescriptor(&tex_desc)
        .expect("newTextureWithDescriptor returned nil");

    // --- Render pass: clear to opaque black, store the result ------------
    let pass = MTLRenderPassDescriptor::renderPassDescriptor();
    {
        let attachments = pass.colorAttachments();
        // SAFETY: index 0 is always valid on a render pass descriptor; objc2
        // marks the subscript unsafe because it is not bounds-checked.
        let a0 = unsafe { attachments.objectAtIndexedSubscript(0) };
        a0.setTexture(Some(&texture));
        a0.setLoadAction(MTLLoadAction::Clear);
        a0.setStoreAction(MTLStoreAction::Store);
        a0.setClearColor(objc2_metal::MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        });
    }

    // --- ImGui context + Metal backend -----------------------------------
    // SAFETY: single-threaded within this test (context lock held); every
    // pointer handed to Dear ImGui below is a live local.
    unsafe {
        let ctx = ImGui_CreateContext(std::ptr::null_mut());
        assert!(!ctx.is_null(), "ImGui_CreateContext(NULL) returned null");
        ImGui_SetCurrentContext(ctx);

        let io = ImGui_GetIO();
        assert!(!io.is_null(), "ImGui_GetIO() returned null");
        (*io).DisplaySize = ImVec2 {
            x: DIM as f32,
            y: DIM as f32,
        };
        (*io).DisplayFramebufferScale = ImVec2 { x: 1.0, y: 1.0 };
        (*io).DeltaTime = 1.0 / 60.0;
        ImGui_StyleColorsDark(std::ptr::null_mut());

        // `device` is an `id<MTLDevice>`; the FFI takes it untyped.
        let device_id: *mut c_void = objc2::rc::Retained::as_ptr(&device) as *mut c_void;
        assert!(
            ImGui_ImplMetal_Init(device_id),
            "ImGui_ImplMetal_Init returned false"
        );

        // --- One full frame ------------------------------------------------
        let pass_ptr: *mut c_void = objc2::rc::Retained::as_ptr(&pass) as *mut c_void;
        ImGui_ImplMetal_NewFrame(pass_ptr);
        ImGui_NewFrame();

        ImGui_SetNextWindowPos(ImVec2 { x: 8.0, y: 8.0 }, 0);
        ImGui_SetNextWindowSize(
            ImVec2 {
                x: (DIM as f32) - 16.0,
                y: (DIM as f32) - 16.0,
            },
            0,
        );
        let opened = ImGui_Begin(c"rustty-inspector-kat".as_ptr(), std::ptr::null_mut(), 0);
        assert!(opened, "ImGui_Begin returned false for a visible window");
        ImGui_Text(c"WS6 offscreen metal KAT".as_ptr());
        ImGui_End();

        ImGui_Render();
        let draw_data = ImGui_GetDrawData();
        assert!(!draw_data.is_null(), "ImGui_GetDrawData() returned null");

        // --- Encode + submit ------------------------------------------------
        let command_buffer = queue.commandBuffer().expect("commandBuffer returned nil");
        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&pass)
            .expect("renderCommandEncoderWithDescriptor returned nil");

        let cb_ptr: *mut c_void = objc2::rc::Retained::as_ptr(&command_buffer) as *mut c_void;
        let enc_ptr: *mut c_void = objc2::rc::Retained::as_ptr(&encoder) as *mut c_void;
        ImGui_ImplMetal_RenderDrawData(draw_data, cb_ptr, enc_ptr);

        encoder.endEncoding();
        command_buffer.commit();
        command_buffer.waitUntilCompleted();

        // --- Read the texture back ------------------------------------------
        let mut pixels = vec![0u8; DIM * DIM * 4];
        texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
            NonNull::new(pixels.as_mut_ptr().cast::<c_void>()).unwrap(),
            DIM * 4,
            MTLRegion {
                origin: MTLOrigin { x: 0, y: 0, z: 0 },
                size: MTLSize {
                    width: DIM,
                    height: DIM,
                    depth: 1,
                },
            },
            0,
        );

        // Known answer: the ImGui window drew SOMETHING over the black clear.
        // Count pixels whose BGR differs from the clear (0,0,0).
        let non_black = pixels
            .chunks_exact(4)
            .filter(|px| px[0] != 0 || px[1] != 0 || px[2] != 0)
            .count();
        assert!(
            non_black > 1000,
            "ImGui_ImplMetal_RenderDrawData wrote {non_black} non-black pixels \
             (expected > 1000 for a {DIM}x{DIM} window fill) — the Metal backend did not rasterize"
        );

        ImGui_ImplMetal_Shutdown();
        ImGui_DestroyContext(ctx);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd /Users/gael/projects/rustty
cargo test -p dcimgui-sys --test metal_offscreen 2>&1 | tail -20
```

Expected: FAIL at compile time — `error[E0432]: unresolved import 'objc2_metal'` (and `objc2`), because `dcimgui-sys` has no `objc2`/`objc2-metal` dev-dependency yet.

- [ ] **Step 3: Add the macOS-gated dev-dependencies**

Append to `crates-sys/dcimgui-sys/Cargo.toml` (at the end of the file):

```toml
# TEST-ONLY Metal bindings for `tests/metal_offscreen.rs` (the WS6 Phase-A KAT
# proving `imgui_impl_metal.mm` actually rasterizes). DEV-dependencies, so they
# never enter the runtime graph — dcimgui-sys stays a pure -sys leaf and the
# acyclic gate is untouched. Target-gated to macOS exactly like the Metal arm of
# build.rs; a Linux/GTK build resolves nothing here. Versions match the rest of
# the workspace (rustty-renderer/rustty-apprt/rustty-apprt-c all pin objc2 0.6 /
# objc2-metal 0.3); the per-type objc2-metal feature list mirrors
# rustty-renderer's, minus the features that crate needs only for its own
# pipeline/IOSurface paths.
[target.'cfg(target_os = "macos")'.dev-dependencies]
objc2 = { version = "0.6" }
objc2-metal = { version = "0.3", default-features = false, features = [
    "MTLAllocation",
    "MTLCommandBuffer",
    "MTLCommandEncoder",
    "MTLCommandQueue",
    "MTLDevice",
    "MTLPixelFormat",
    "MTLRenderCommandEncoder",
    "MTLRenderPass",
    "MTLResource",
    "MTLTexture",
    "MTLTypes",
    "block2",
    "dispatch2",
] }
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd /Users/gael/projects/rustty
cargo test -p dcimgui-sys --test metal_offscreen 2>&1 | tail -20
```

Expected: `test metal_offscreen_render_writes_pixels ... ok` / `test result: ok. 1 passed`.

If it fails with a **compile** error naming a missing `objc2-metal` feature, add exactly that feature name to the list above (this is the one place the plan tolerates a name fix — objc2-metal's feature gating is per-type; the fix is mechanical and the error message names the gate). If it fails on the `non_black > 1000` assertion, that is a REAL finding: the Metal backend is not rasterizing — stop and report before proceeding to Phase B.

- [ ] **Step 5: Verify the whole dcimgui-sys suite (including the pre-existing link KATs)**

```bash
cd /Users/gael/projects/rustty
cargo test -p dcimgui-sys 2>&1 | tail -15
```

Expected: all tests pass, including `kat_dcimgui_1_92_5_context_lifecycle`, `link_imgui_impl_metal_ffi`, `link_imgui_dock_builder_ffi`, `kat_dock_builder_add_node_ex_get_node_round_trip`, `kat_dock_builder_set_node_size_smoke`, and the new `metal_offscreen_render_writes_pixels`.

- [ ] **Step 6: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates-sys/dcimgui-sys/tests/metal_offscreen.rs crates-sys/dcimgui-sys/Cargo.toml Cargo.lock
git commit -m "dcimgui-sys: offscreen Metal render KAT — prove ImGui_ImplMetal rasterizes (WS6 T1)"
```

---

# PHASE B — Shared widget-render residual

**Phase boundary:** Phase B touches `crates/rustty-apprt/src/embedded.rs` only (plus a read-only gate run over `crates/rustty-inspector`). It ends when the G3b comment-coverage gate proves zero dropped reference comments in the shared widget crate AND `updateContentScale` matches the oracle byte-for-behaviour with a passing KAT.

## Task 2: Prove the shared widget-render residual is zero (G3b + oracle TODO check)

**Files:**
- Modify: none expected. (If the gate reports a RED drop, that file is modified — the gate output names it.)
- Test: `.port/tools/gates/check_comment_coverage.py --crate rustty-inspector --strict`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: the recorded evidence that Phase B has no widget-port work, which Task 7's ledger edit cites.

- [ ] **Step 1: Run the G3b content-aware reference-comment coverage gate**

```bash
cd /Users/gael/projects/rustty
python3 .port/tools/gates/check_comment_coverage.py --crate rustty-inspector --strict 2>&1 | tail -25
```

Expected: exit 0 — GREEN (or REVIEW-only). Exit 1 means a distinctive oracle comment was dropped: the report names the oracle file + the comment; restore it in the corresponding `crates/rustty-inspector/src/...` module (verbatim, with the ghostty→rustty rename) and re-run until green. Exit 2 = usage/blocked — read the script header and fix the invocation.

- [ ] **Step 2: Confirm the port carries no invented placeholders and its three `TODO`s are faithful oracle copies**

```bash
cd /Users/gael/projects/rustty
grep -rn "unimplemented!\|todo!()" crates/rustty-inspector/src/ || echo "OK: no unimplemented/todo in rustty-inspector"
grep -rn "TODO" crates/rustty-inspector/src/
grep -rn "TODO" /Users/gael/projects/ghostty/src/inspector/
```

Expected: the first prints `OK: ...`. The second prints exactly three hits — `Inspector.rs:131` ("This will have to be recalculated for different screen DPIs."), `widgets/termio.rs:249`, `widgets/termio.rs:253` ("TODO: Eventually"). The third must show the SAME TODO text in the oracle (`src/inspector/Inspector.zig`, `src/inspector/widgets/termio.zig`). If a port TODO has no oracle twin, it is an invented placeholder — port the real body from the oracle line it references before continuing.

- [ ] **Step 3: Confirm the module inventory matches the oracle 1:1**

```bash
cd /Users/gael/projects/rustty
ls crates/rustty-inspector/src crates/rustty-inspector/src/widgets
ls /Users/gael/projects/ghostty/src/inspector /Users/gael/projects/ghostty/src/inspector/widgets
```

Expected: the 9 widget submodules (`key page pagelist renderer screen style surface terminal termio`) plus `Inspector`, `units`, `widgets`, `lib`(=`main`) on both sides. Any oracle module with no Rust twin is a genuine gap — port it before continuing.

- [ ] **Step 4: Run the crate's own tests**

```bash
cd /Users/gael/projects/rustty
cargo test -p rustty-inspector 2>&1 | tail -8
```

Expected: green (the crate's existing `lib.rs` and `widgets/termio.rs` test modules).

- [ ] **Step 5: Commit (evidence-only; commits nothing if the gate was already green)**

```bash
cd /Users/gael/projects/rustty
git add -A && git commit -m "inspector: G3b zero-drop proof for the shared widget render (WS6 T2)" || echo "nothing to commit — residual confirmed zero"
```

## Task 3: Fill `Inspector::updateContentScale`'s ImGui style scaling (the one unported reference body)

**Files:**
- Modify: `crates/rustty-apprt/src/embedded.rs` (`Inspector::updateContentScale`, currently at `:2279-2304`)
- Test: `crates/rustty-apprt/src/embedded.rs` — new `#[cfg(test)] mod inspector_style_tests` appended after the existing `#[cfg(test)] mod host_view_tests`

**Interfaces:**
- Consumes: `dcimgui_sys::{ImGui_SetCurrentContext, ImGui_GetStyle, ImGuiStyle, ImGuiStyle_ScaleAllSizes, ImGuiStyle_ImGuiStyle, ImGui_CreateContext, ImGui_DestroyContext}` — all already exported by `dcimgui-sys` (`ImGuiStyle_ImGuiStyle` is the hand-declared `ext.cpp` shim in `crates-sys/dcimgui-sys/src/lib.rs:76`; `ImGuiStyle_ScaleAllSizes` and `ImGui_GetStyle` are bindgen-generated from `dcimgui.h`).
- Produces: `pub fn updateContentScale(&mut self, x: f64, y: f64)` — signature UNCHANGED (callers `rustty_inspector_set_content_scale` in `rustty-apprt-c` and `InspectorView.swift` are untouched); behaviour now includes the ImGui style rescale.

**Oracle (`/Users/gael/projects/ghostty/src/apprt/embedded.zig:1111-1126`):**

```zig
pub fn updateContentScale(self: *Inspector, x: f64, y: f64) void {
    _ = y;
    cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);

    // Cache our scale because we use it for cursor position calculations.
    self.content_scale = x;

    // Setup a new style and scale it appropriately. We must use the
    // ImGuiStyle constructor to get proper default values (e.g.,
    // CurveTessellationTol) rather than zero-initialized values.
    var style: cimgui.c.ImGuiStyle = undefined;
    cimgui.ext.ImGuiStyle_ImGuiStyle(&style);
    cimgui.c.ImGuiStyle_ScaleAllSizes(&style, @floatCast(x));
    const active_style = cimgui.c.ImGui_GetStyle();
    active_style.* = style;
}
```

- [ ] **Step 1: Write the failing test**

Append to `crates/rustty-apprt/src/embedded.rs` (after the closing `}` of `mod host_view_tests`):

```rust
#[cfg(test)]
mod inspector_style_tests {
    use super::*;
    use std::sync::Mutex;

    /// Dear ImGui's "current context" is a plain global (`GImGui`), and the
    /// default `cargo test` harness runs each `#[test]` on its own thread.
    /// Serialize every test that creates/destroys a context — same discipline
    /// as `dcimgui-sys`'s own `IMGUI_CONTEXT_TEST_LOCK`.
    static IMGUI_CONTEXT_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Known-answer test for `Inspector::updateContentScale` (oracle
    /// `embedded.zig:1111-1126`): the active ImGui style must be REPLACED with
    /// a default-constructed style scaled by `x`.
    ///
    /// Known answers are Dear ImGui 1.92.5's own `ImGuiStyle::ImGuiStyle()`
    /// defaults (`vendor/imgui/imgui.cpp`) run through `ScaleAllSizes`, which
    /// is `ImTrunc(value * scale_factor)`:
    ///   * `WindowPadding`  (8, 8)   -> (16, 16)
    ///   * `ScrollbarSize`   14.0    -> 28.0
    ///   * `GrabMinSize`     12.0    -> 24.0
    ///
    /// `updateContentScale` never dereferences `self.surface`, so the test
    /// constructs an `Inspector` with a null surface back-pointer.
    #[test]
    fn update_content_scale_rescales_active_imgui_style() {
        let _guard = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // SAFETY: single-threaded under the lock above; every pointer handed
        // to Dear ImGui is a live local, and the context is destroyed below.
        unsafe {
            let ig_ctx = dcimgui_sys::ImGui_CreateContext(core::ptr::null_mut());
            assert!(!ig_ctx.is_null(), "ImGui_CreateContext(NULL) returned null");

            let mut inspector = Inspector {
                surface: core::ptr::null_mut(),
                ig_ctx,
                backend: None,
                content_scale: 1.0,
                instant: None,
            };

            // Baseline: the fresh context carries the UNSCALED defaults.
            let style = dcimgui_sys::ImGui_GetStyle();
            assert!(!style.is_null(), "ImGui_GetStyle() returned null");
            assert_eq!((*style).WindowPadding.x, 8.0, "baseline WindowPadding.x");
            assert_eq!((*style).ScrollbarSize, 14.0, "baseline ScrollbarSize");
            assert_eq!((*style).GrabMinSize, 12.0, "baseline GrabMinSize");

            inspector.updateContentScale(2.0, 2.0);

            // The pure-Rust effect: the cached scale used by cursorPosCallback.
            assert_eq!(inspector.content_scale, 2.0, "cached content_scale");

            // The dcimgui effect: the ACTIVE style is the scaled default style.
            let style = dcimgui_sys::ImGui_GetStyle();
            assert_eq!((*style).WindowPadding.x, 16.0, "scaled WindowPadding.x");
            assert_eq!((*style).WindowPadding.y, 16.0, "scaled WindowPadding.y");
            assert_eq!((*style).ScrollbarSize, 28.0, "scaled ScrollbarSize");
            assert_eq!((*style).GrabMinSize, 24.0, "scaled GrabMinSize");

            dcimgui_sys::ImGui_DestroyContext(ig_ctx);
        }
    }

    /// Scaling is computed from a FRESH default style each call (the oracle
    /// default-constructs `style` every time), so calling twice with the same
    /// factor is idempotent — it must NOT compound to 4x.
    #[test]
    fn update_content_scale_is_idempotent_not_compounding() {
        let _guard = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // SAFETY: as above.
        unsafe {
            let ig_ctx = dcimgui_sys::ImGui_CreateContext(core::ptr::null_mut());
            assert!(!ig_ctx.is_null(), "ImGui_CreateContext(NULL) returned null");

            let mut inspector = Inspector {
                surface: core::ptr::null_mut(),
                ig_ctx,
                backend: None,
                content_scale: 1.0,
                instant: None,
            };

            inspector.updateContentScale(2.0, 2.0);
            inspector.updateContentScale(2.0, 2.0);

            let style = dcimgui_sys::ImGui_GetStyle();
            assert_eq!(
                (*style).WindowPadding.x, 16.0,
                "WindowPadding.x must stay 16 (not compound to 32)"
            );

            dcimgui_sys::ImGui_DestroyContext(ig_ctx);
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd /Users/gael/projects/rustty
cargo test -p rustty-apprt inspector_style_tests 2>&1 | tail -25
```

Expected: both tests FAIL on the post-call assertions — `assertion `left == right` failed: scaled WindowPadding.x / left: 8.0 / right: 16.0` — because the current `updateContentScale` only caches `content_scale` and never touches the style.

- [ ] **Step 3: Write the implementation**

In `crates/rustty-apprt/src/embedded.rs`, replace the entire `updateContentScale` PORT NOTE + body (from the line `// pub fn updateContentScale(self: *Inspector, x: f64, y: f64) void { ... }` through the closing `}` of the method) with:

```rust
    // pub fn updateContentScale(self: *Inspector, x: f64, y: f64) void { ... }
    //
    // PORT NOTE (WS6 T3 — Phase-3 leaf CLOSED): both halves are now LIVE. The
    // pure-Rust half caches `content_scale` (read by `cursorPosCallback`'s pixel
    // scaling); the dcimgui half default-constructs an `ImGuiStyle` through the
    // `ext.cpp` constructor shim (a zero-initialized struct is NOT a faithful
    // substitute — Dear ImGui's `ImGuiStyle::ImGuiStyle()` sets non-zero defaults
    // such as `CurveTessellationTol`), scales it, and overwrites the context's
    // active style. Scaling from a FRESH default style each call is what makes
    // repeated calls idempotent rather than compounding.
    pub fn updateContentScale(&mut self, x: f64, y: f64) {
        // _ = y;   (the vertical scale is unused — `x` drives both axes)
        let _ = y;
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        //
        // INV-11: every dcimgui touch is gated on this Inspector's own context.
        // SAFETY: `self.ig_ctx` is the live context created by `Inspector::init`
        // and destroyed only by `Inspector::deinit`.
        unsafe { dcimgui_sys::ImGui_SetCurrentContext(self.ig_ctx) };

        // Cache our scale because we use it for cursor position calculations.
        // self.content_scale = x;
        self.content_scale = x;

        // Setup a new style and scale it appropriately. We must use the
        // ImGuiStyle constructor to get proper default values (e.g.,
        // CurveTessellationTol) rather than zero-initialized values.
        // var style: cimgui.c.ImGuiStyle = undefined;
        // cimgui.ext.ImGuiStyle_ImGuiStyle(&style);
        // cimgui.c.ImGuiStyle_ScaleAllSizes(&style, @floatCast(x));
        // const active_style = cimgui.c.ImGui_GetStyle();
        // active_style.* = style;
        //
        // PORT NOTE: Zig's `var style: ImGuiStyle = undefined` -> `MaybeUninit`,
        // initialized in place by the `ext.cpp` constructor shim before any read
        // (so `assume_init` is sound). `@floatCast(x)` -> `x as f32`.
        // `active_style.* = style` -> a raw-pointer write; `ImGuiStyle` is a POD
        // C struct with no `Drop`, so overwriting through the pointer is the
        // exact Zig semantics.
        // SAFETY: the current context was set above, so `ImGui_GetStyle()`
        // returns this Inspector's live style; `style` is fully initialized by
        // the constructor shim before it is scaled and written back.
        unsafe {
            let mut style = core::mem::MaybeUninit::<dcimgui_sys::ImGuiStyle>::uninit();
            dcimgui_sys::ImGuiStyle_ImGuiStyle(style.as_mut_ptr());
            let mut style = style.assume_init();
            dcimgui_sys::ImGuiStyle_ScaleAllSizes(&raw mut style, x as f32);
            let active_style = dcimgui_sys::ImGui_GetStyle();
            core::ptr::write(active_style, style);
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd /Users/gael/projects/rustty
cargo test -p rustty-apprt inspector_style_tests 2>&1 | tail -15
```

Expected: `test result: ok. 2 passed`.

- [ ] **Step 5: Run the whole apprt suite + lints**

```bash
cd /Users/gael/projects/rustty
cargo test -p rustty-apprt 2>&1 | tail -8
cargo clippy -p rustty-apprt --all-targets -- -D warnings 2>&1 | tail -8
cargo fmt --check 2>&1 | tail -5
```

Expected: tests green, clippy clean, fmt clean.

- [ ] **Step 6: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-apprt/src/embedded.rs
git commit -m "apprt: fill Inspector::updateContentScale ImGuiStyle rescale + KATs (WS6 T3)"
```

---

# PHASE C — Embedded/C-export seams + sentinel removal + live shakedown

**Phase boundary:** Phase C changes no behaviour — Task 4 is comments only, Task 5 is a manual runtime verification. It ends with a screenshot proving a live inspector renders in `Rustty.app`.

## Task 4: Remove the false "Phase-3 leaf / crash-guard sentinel" claims from code comments

**Files:**
- Modify: `crates/rustty-apprt-c/src/lib.rs` (four PORT NOTEs: `rustty_surface_inspector` ~`:2300`, `rustty_inspector_metal_init` ~`:2918`, `rustty_inspector_metal_render` ~`:2943`, `rustty_inspector_metal_shutdown` ~`:2975`)
- Modify: `crates/rustty-apprt/src/embedded.rs` (two PORT NOTEs: `Inspector::init` ~`:1927`, `Inspector::deinit` ~`:1983`)

**Interfaces:**
- Consumes: nothing.
- Produces: nothing at the type level. This task exists because `.port/tools/gates/check_stale_comments.sh` treats resolved-deferral prose as a defect, and because a future reader must not be told a live path is an `unimplemented!()` leaf.

- [ ] **Step 1: Fix `rustty_surface_inspector`'s PORT NOTE** in `crates/rustty-apprt-c/src/lib.rs`

Replace:

```
    // PORT NOTE: `?*Inspector` → a nullable `*mut Inspector` (cbindgen emits the raw
    // pointer; `null` for the error case). `initInspector` reaches the Phase-3 dcimgui
    // `Inspector::init` leaf at RUNTIME (EMBEDDED_BLUEPRINT §1) — so a live inspector
    // cannot yet be built; this export links now and panics in that leaf when driven.
```

with:

```
    // PORT NOTE: `?*Inspector` → a nullable `*mut Inspector` (cbindgen emits the raw
    // pointer; `null` for the error case). `initInspector` -> `Inspector::init` is
    // FULLY LIVE (WS6): it creates the per-Surface Dear ImGui context, runs
    // `CoreInspector::setup()`, and activates the core inspector. The `null` return
    // is the reference's own init-failure path (the macOS host null-guards it in
    // SurfaceView_AppKit.swift), NOT a port placeholder.
```

- [ ] **Step 2: Fix `rustty_inspector_metal_init`'s PORT NOTE** in the same file

Replace:

```
// PORT NOTE: delegates to the E4 backend `Inspector::initMetal` (a Phase-3
// objc/Metal + dcimgui `ImGui_ImplMetal_Init` leaf; DEFERRALS.md E4 item 2). The
// reference `.fromId(device)` wraps the raw objc id in an `objc.Object`; rustty's
// `initMetal` takes the opaque `*mut c_void` directly (no objc dep this
// milestone). No new leaf in apprt-c — the panic is in the E4 backend leaf.
```

with:

```
// PORT NOTE: delegates to the backend `Inspector::initMetal`, which is LIVE
// (WS6): it sets the Inspector's ImGui context, tears down any prior backend,
// and calls dcimgui's `ImGui_ImplMetal_Init`. The reference `.fromId(device)`
// wraps the raw objc id in an `objc.Object`; rustty's `initMetal` takes the
// opaque `*mut c_void` directly (the id crosses the C ABI untyped) and releases
// the host's +1 retain on every exit path via its `Defer` guard.
```

- [ ] **Step 3: Fix `rustty_inspector_metal_render`'s PORT NOTE** in the same file

Replace:

```
// PORT NOTE: delegates to the E4 backend `Inspector::renderMetal` (a Phase-3
// objc/Metal render + dcimgui `ImplMetal_NewFrame`/`RenderDrawData` leaf;
// DEFERRALS.md E4 item 2), catching + logging its error exactly as the reference
// `catch |err| { log.err(...); return; }`. The two `.fromId(...)` objc-id wraps →
// opaque `*mut c_void`. No new leaf in apprt-c — the panic is in the E4 backend.
```

with:

```
// PORT NOTE: delegates to the backend `Inspector::renderMetal`, which is LIVE
// (WS6): the double-frame ImGui loop, the `rustty_inspector::Inspector::render`
// widget aggregate, the `MTLRenderCommandEncoder` construction, and
// `ImGui_ImplMetal_RenderDrawData`. Catches + logs its error exactly as the
// reference `catch |err| { log.err(...); return; }`. The two `.fromId(...)`
// objc-id wraps → opaque `*mut c_void`.
```

- [ ] **Step 4: Fix `rustty_inspector_metal_shutdown`'s PORT NOTE** in the same file

Replace:

```
// PORT NOTE: tears down the Metal backend if one was initialized. `Backend::deinit`
// is the E4 Phase-3 leaf (dcimgui `ImGui_ImplMetal_Shutdown`); it runs only when a
// backend was actually set, which cannot happen this milestone (`initMetal` is a
// leaf), so on a live inspector this is a no-op (`backend == None`). Faithful to
// the reference `if (ptr.backend) |v| { v.deinit(); ptr.backend = null; }`.
```

with:

```
// PORT NOTE: tears down the Metal backend if one was initialized.
// `Backend::deinit` is LIVE (WS6) and calls dcimgui's `ImGui_ImplMetal_Shutdown`
// on the `.metal` arm. Faithful to the reference
// `if (ptr.backend) |v| { v.deinit(); ptr.backend = null; }`.
//
// ABI NOTE (R1): the reference's own `ghostty.h` declares this `bool` while
// `embedded.zig`'s `export fn` returns `void`; `include/rustty-embed.h` mirrors
// that discrepancy verbatim rather than "fixing" it. The Swift host never calls
// this export, so nothing reads the return.
```

- [ ] **Step 5: Fix the two `embedded.rs` PORT NOTEs** in `crates/rustty-apprt/src/embedded.rs`

Replace (on `Inspector::init`):

```
    // `activateInspector` is a core `Surface` method
    // (still a Phase-3 leaf — Task 6 — so opening the inspector aborts THERE for now,
    // one step deeper than the previous init-sentinel guard; the reference catches
    // its error and continues, which Task 6 restores).
```

with:

```
    // `activateInspector` is a core `Surface` method
    // and is LIVE (it heap-boxes the core inspector, publishes it onto
    // `renderer_state` under the mutex, and notifies renderer + termio). Its
    // error is swallowed here exactly as the reference does.
```

Replace (on `Inspector::deinit`):

```
    // (shut down the Metal backend if one was initialized, then destroy the
    // context). Reached by [`Surface::freeInspector`]. `deactivateInspector` is a
    // core `Surface` method (still a Phase-3 leaf — Task 6).
```

with:

```
    // (shut down the Metal backend if one was initialized, then destroy the
    // context). Reached by [`Surface::freeInspector`]. `deactivateInspector` is a
    // core `Surface` method and is LIVE (it clears the renderer-state aliases
    // under the mutex, notifies renderer + termio, and frees the core inspector).
```

- [ ] **Step 6: Prove no crash-guard sentinel remains anywhere on the inspector path**

```bash
cd /Users/gael/projects/rustty
grep -rn "not yet ported\|TODO(port)" crates/rustty-apprt/src/embedded.rs crates/rustty-apprt-c/src/lib.rs || echo "OK: no sentinel"
grep -rn "Phase-3" crates/rustty-apprt-c/src/lib.rs | grep -i inspector || echo "OK: no stale Phase-3 inspector prose in apprt-c"
```

Expected: both print their `OK:` line.

- [ ] **Step 7: Run the stale-comment gate + build**

```bash
cd /Users/gael/projects/rustty
bash .port/tools/gates/check_stale_comments.sh 2>&1 | tail -15
cargo build --workspace 2>&1 | tail -3
```

Expected: gate green (or unchanged from its pre-WS6 baseline — if it was already reporting unrelated hits, WS6 must not ADD any); build green.

- [ ] **Step 8: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-apprt/src/embedded.rs crates/rustty-apprt-c/src/lib.rs
git commit -m "apprt/apprt-c: retire the Phase-3-leaf + crash-guard prose on the live inspector path (WS6 T4)"
```

## Task 5: MANUAL live shakedown — ⌘⌥I opens a rendering inspector in Rustty.app

**Files:** none (runtime verification). Any wiring defect this surfaces is fixed here and committed in Step 7.

**Interfaces:**
- Consumes: everything from Tasks 1–4.
- Produces: a screenshot artifact + the recorded evidence Task 7's `VERIFICATION_DEBT.md` edit cites.

> **THIS STEP IS MANUAL / SCREENSHOT-VERIFIED.** Do **not** claim success from a build log, an exit code, or a process listing. The claim "the inspector renders" is only valid with a screenshot showing ImGui panels. If you cannot obtain a screenshot, mark this task BLOCKED and report — do not proceed to Task 8.

- [ ] **Step 1: Build the workspace**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -3
```

Expected: `Finished` with no errors.

- [ ] **Step 2: Build the app bundle**

```bash
cd /Users/gael/projects/rustty
bash macos/build-rustty.sh 2>&1 | tail -15
```

Expected: `** BUILD SUCCEEDED **`. (The script runs `xtask xcframework` + `xtask resources` then `xcodebuild`.)

- [ ] **Step 3: Locate and launch the built app**

```bash
cd /Users/gael/projects/rustty
APP=$(xcodebuild -project macos/Rustty.xcodeproj -scheme Rustty -configuration Debug -showBuildSettings 2>/dev/null | awk -F' = ' '/ BUILT_PRODUCTS_DIR /{print $2}' | head -1)/Rustty.app
echo "$APP"
open "$APP"
```

Expected: the path exists and a Rustty terminal window appears with a live shell prompt.

- [ ] **Step 4: Open the inspector and screenshot it**

In the running Rustty window: type `ls -la` and press Return (so there is scrollback and VT traffic to inspect), then press **⌘⌥I**.

```bash
mkdir -p /tmp/ws6
screencapture -x /tmp/ws6/inspector-open.png
```

Then VIEW `/tmp/ws6/inspector-open.png` (read the image) and confirm ALL of:

1. The window is split — terminal on one side, the Dear ImGui inspector on the other.
2. The inspector shows its docked panels: **Terminal**, **Screen**, **Cursor**, **Keyboard**, **Termio (VT events)**, **Modes**, **Cell**.
3. The panels contain live values (non-zero cursor coordinates, real screen dimensions, populated page/pagelist stats) — not empty frames.

If the app **aborts** on ⌘⌥I, capture the crash:

```bash
ls -t ~/Library/Logs/DiagnosticReports/Rustty* 2>/dev/null | head -1 | xargs -I{} sh -c 'tail -60 "{}"'
```

and fix the named leaf before continuing. (Under `panic=abort`, any `unimplemented!()` reached on this path kills the process; the crash report names the frame.)

- [ ] **Step 5: Verify LIVE input reaches the inspector**

With the inspector open, click into the terminal pane and type `echo hello-ws6` + Return. Then:

```bash
screencapture -x /tmp/ws6/inspector-after-input.png
```

VIEW `/tmp/ws6/inspector-after-input.png` and confirm the **Keyboard** panel lists the newly-typed key events and the **Termio / VT events** panel lists the new VT stream events (they must differ from `inspector-open.png`). This is the INV-8/INV-9/INV-12 end-to-end proof: input → core → inspector ring → ImGui frame → Metal.

- [ ] **Step 6: Verify the HiDPI style rescale (the Task-3 deliverable) at runtime**

Drag the Rustty window from the built-in Retina display to a non-Retina external display (or vice versa; if only one display is available, toggle **System Settings → Displays → Resolution** between a HiDPI and a non-HiDPI mode). Then:

```bash
screencapture -x /tmp/ws6/inspector-rescaled.png
```

VIEW `/tmp/ws6/inspector-rescaled.png` and confirm the inspector's ImGui chrome (window padding, scrollbar width, grab handles) rescaled with the display — i.e. the panels are not rendered at half/double the correct metric density. Before Task 3 this was frozen at the 1.0 style.

- [ ] **Step 7: Commit any wiring fix this shakedown surfaced**

```bash
cd /Users/gael/projects/rustty
git add -A && git commit -m "inspector: end-to-end shakedown fixes — cmd+opt+i renders live in Rustty.app (WS6 T5)" || echo "nothing to commit — shakedown clean"
```

---

# PHASE D — Gates, ledgers, merge

**Phase boundary:** Phase D adds no product code. It ends with `main` carrying WS6 and truthful ledgers.

## Task 6: Both-platform build gate + workspace gates

**Files:** none (verification); fixes go on this branch.

**Interfaces:**
- Consumes: everything from Tasks 1–5.
- Produces: the green-on-both-platforms evidence the merge in Task 8 requires.

- [ ] **Step 1: macOS full build + full test suite**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -3
cargo test --workspace 2>&1 | tail -20
```

Expected: build `Finished`; tests green. (Pre-existing RED lanes documented in the ledgers — notably the vt search-subsystem Miri finding — are out of WS6 scope; WS6 must add no new failure.)

- [ ] **Step 2: Lints + format + acyclic + ABI gates on macOS**

```bash
cd /Users/gael/projects/rustty
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --check 2>&1 | tail -5
bash .port/tools/gates/check_acyclic.sh 2>&1 | tail -5
bash .port/tools/gates/check_embed_h_abi.sh 2>&1 | tail -8
```

Expected: clippy clean; fmt clean; acyclic PASS (the new `objc2`/`objc2-metal` edges are **dev**-dependencies of a leaf `-sys` crate — they must not appear in the runtime graph); A4 ABI gate PASS **with an unchanged ledger** — WS6 adds no export and changes no by-value C-ABI return, so any A4 delta is a defect (memory: embed-abi-byvalue-sret-gate).

- [ ] **Step 3: Linux (VM) build — the GTK/OpenGL3 inspector must not regress**

```bash
cd /Users/gael/projects/rustty
git push -u origin ws6-macos-inspector
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws6-macos-inspector && git pull --ff-only && cargo build --workspace' 2>&1 | tail -5
```

Expected: green. The Metal KAT file is `#![cfg(target_os = "macos")]` and its dev-deps are target-gated, so Linux resolves neither `objc2` nor `objc2-metal`.

- [ ] **Step 4: Linux (VM) — prove the OpenGL3 inspector arm still links and tests**

```bash
ssh rvm 'cd ~/projects/rustty && cargo test -p dcimgui-sys 2>&1 | tail -10'
ssh rvm 'cd ~/projects/rustty && cargo build -p rustty-apprt --features gtk 2>&1 | tail -3'
```

Expected: `opengl3_symbols_link` passes (and `link_imgui_impl_metal_ffi` is absent — it is macOS-gated); the `gtk`-featured apprt build is green.

- [ ] **Step 5: Commit any cross-platform fixup**

```bash
cd /Users/gael/projects/rustty
git add -A && git commit -m "inspector: WS6 gate sweep fixups — both-platform green (WS6 T6)" || echo "nothing to commit"
git push
```

## Task 7: Ledger strikes — make DEFERRALS / MACOS_APP_GAPS / VERIFICATION_DEBT truthful

**Files:**
- Modify: `.port/phase-2/DEFERRALS.md`
- Modify: `.port/phase-2/MACOS_APP_GAPS.md`
- Modify: `.port/phase-2/VERIFICATION_DEBT.md`

**Interfaces:**
- Consumes: the evidence produced by Tasks 1–6 (test names, commit refs, screenshot paths).
- Produces: ledgers a future wave can trust. (Standing lesson: prior waves repeatedly burned time on stale ledger entries.)

Re-locate every section by its heading text, not by line number — the line numbers below are as-of authoring.

- [ ] **Step 1: `DEFERRALS.md` — the core C1 section** (heading `## rustty-inspector C1 (core tier) — key-widget data surface; ImGui GUI deferred to Phase 3`, ~line 954)

Add a `**RESOLVED (WS6, 2026-07-…)**` paragraph immediately under the heading stating: `rustty_inspector::Inspector` is no longer an uninhabited enum — the crate is a fully inhabited 13-module port of oracle `src/inspector/**` (11 625 Rust LOC vs 4 873 Zig LOC), landed by the 2026-07-14 inspector series (`b344e09`…`d6be9b2`) and proven drop-free by `check_comment_coverage.py --crate rustty-inspector --strict` (WS6 T2). Core stores a live `Option<*mut rustty_inspector::Inspector>`.

- [ ] **Step 2: `DEFERRALS.md` — the core C6 section** (heading `## rustty-core C6 (core tier) — Surface init/deinit/close/draw lifecycle (inspector activate deferred verbatim)`, ~line 1125)

Mark its `activateInspector`/`deactivateInspector` bullet **RESOLVED (WS6)** with the evidence `crates/rustty-core/src/surface.rs:2873` / `:2967` are live bodies (commit `2dfcc32`), and the `keyCallback` inspector defer-block / `cursorPosCallback` Some-arm are reachable and exercised (the WS6 T5 shakedown proves keyboard + mouse events reach the inspector panels).

- [ ] **Step 3: `DEFERRALS.md` — the core roll-up** (~lines 1813-1835)

Two edits:
- In the `unimplemented!()` audit line for `surface.rs`, change `**2** … `activateInspector` / `deactivateInspector` (Phase-3 inspector, §4)` to `**0** — the two inspector entries closed by WS6; re-run the audit to confirm the current count`.
- Mark roll-up item **1. Inspector activate/deactivate** as `RESOLVED (WS6)` with the same evidence as Step 2.

Then re-run the audit to make the new number true:

```bash
cd /Users/gael/projects/rustty
grep -c "unimplemented!" crates/rustty-core/src/surface.rs
grep -n "unimplemented!" crates/rustty-core/src/surface.rs
```

Write whatever count that reports into the ledger line, listing each remaining site's purpose. (Do not assert a number you have not just measured.)

- [ ] **Step 4: `DEFERRALS.md` — the E4 section** (heading `## rustty-apprt embedded E4 (Inspector — dcimgui + Metal) — Phase-3 inhabited placeholder`, ~line 2107)

Add a `**RESOLVED IN FULL (WS6, 2026-07-…)**` paragraph under the heading and mark each of its four deferred items closed with evidence:
1. `Inspector::init` / `deinit` — live at `embedded.rs:1933` / `:1985` (commit `027f695`).
2. `initMetal` / `renderMetal` / `Backend::deinit` — live at `embedded.rs:2018` / `:2107` / `:1889` (commits `86b04b4`, `0b8f7af`); the Metal backend is proven to rasterize by `dcimgui-sys` `metal_offscreen_render_writes_pixels` (WS6 T1).
3. The six input callbacks + `newFrame` — live (commit `c62ab07`); proven end-to-end by the WS6 T5 shakedown (Keyboard/Termio panels populate on real input).
4. `Surface::initInspector` — live; no longer reaches a leaf.

Also correct the section's "**REAL (non-GPU)**" bullet, which still says the `updateContentScale` style scaling "is elided as a documented leaf": WS6 T3 filled it (`embedded.rs` `updateContentScale`, KATs `update_content_scale_rescales_active_imgui_style` / `update_content_scale_is_idempotent_not_compounding`).

- [ ] **Step 5: `DEFERRALS.md` — the crash-guard sentinel section** (heading `## P2 crash-guard: on-feature quicklook/inspector leaves [added 2026-07-12, panic=abort abort-policy pass]`, ~line 4581)

**This is the sentinel removal the roadmap assigns to WS6.** Replace the section body with a short RESOLVED note. The whole section (heading through the trailing `---` before `## rustty-termio ReadThread — PRESERVED REFERENCE QUIRK…`) becomes:

```markdown
## P2 crash-guard: on-feature quicklook/inspector leaves — RESOLVED [closed 2026-07-… by WS6]

All three guarded leaves are gone; there is no crash-guard sentinel left anywhere
in the tree (`grep -rn "not yet ported" crates/rustty-apprt/src/embedded.rs
crates/rustty-apprt-c/src/lib.rs` → no hits).

- `rustty_surface_quicklook_word` — un-deferred + fully ported in P2.1 (objc-free
  core/vt).
- `rustty_surface_quicklook_font` — un-deferred + fully ported in P2.2 (CoreText
  face read via objc2-core-text, returning the +1 CTFontRef the host releases, I16).
- `embedded::Inspector::init` — un-deferred by the 2026-07-14 inspector series
  (`027f695`): it creates the per-Surface Dear ImGui context, runs
  `CoreInspector::setup()`, and activates the core inspector. The `eprintln!`
  no-op sentinel and its `Err(ApprtError)` early return were removed with it; the
  export's `null` return is once again the reference's own init-failure path, not
  a placeholder. The downstream chain (`deinit`/`initMetal`/`renderMetal`,
  `Backend::deinit`, the `rustty_inspector_metal_*` exports, core
  `activate`/`deactivateInspector`, `queueInspectorRender`) is likewise live.

**Runtime sign-off (WS6):** ⌘⌥I opens a rendering inspector in Rustty.app with
live Terminal/Screen/Cursor/Keyboard/Termio/Modes/Cell panels; typed input
populates the Keyboard and VT-event panels; the ImGui style rescales across a
display-density change. Screenshots: `/tmp/ws6/inspector-open.png`,
`/tmp/ws6/inspector-after-input.png`, `/tmp/ws6/inspector-rescaled.png`.
Backend rasterization is additionally gated by the offscreen KAT
`dcimgui-sys::metal_offscreen_render_writes_pixels`.
```

- [ ] **Step 6: `DEFERRALS.md` — the Linux/GTK tail section** (heading `## GTK winproto (wayland/x11 arms) + dcimgui inspector — Linux/GTK tail [2026-07-18, final parity audit]`, ~line 4686)

Read the section; if any bullet still describes the dcimgui inspector as unported on Linux, mark it RESOLVED (the GTK OpenGL3 inspector landed 2026-07-19, commits `657f223`/`f780650`, with the `gtk_inspector_dump` framebuffer KAT). Leave the x11-winproto bullet alone — that is WS16.

- [ ] **Step 7: `MACOS_APP_GAPS.md` — strike the four stale inspector rows**

- Line ~14 (§3 "On-feature crash guards"): remove "**and the inspector**" from the list of features that abort under `panic=abort`; keep the quicklook history note as already-resolved.
- Line ~66 (the "**Inspector subsystem**" table row): replace its "Interim: no-op/return-null leaves. Full fix = dcimgui-sys + ImGui_ImplMetal (Phase-3)" cell with `RESOLVED (WS6): dcimgui-sys + ImGui_ImplMetal wired and proven rasterizing; ⌘⌥I opens a live inspector (screenshot-verified).`
- Line ~75 (renderer `overlayFeatures` bullet): update — the uninhabited-enum claim is obsolete; `overlay_features()` is a live trait method (`rustty-renderer/src/state.rs:48`) implemented by `rustty-inspector`'s `renderer::Info` (`widgets/renderer.rs:290`), and `rebuild_overlay` (`generic.rs:4322`) is a real implementation.
- Line ~99 ("**2b. Crash-guard inspector**" work item): mark DONE/superseded by WS6.

- [ ] **Step 8: `VERIFICATION_DEBT.md` — record the macOS runtime sign-off**

Add a macOS inspector sign-off entry next to the existing Linux one (~line 67, `Inspector (Ctrl+Shift+I, build with --features gtk,imgui)`): the macOS ⌘⌥I inspector is runtime-verified by WS6 T5 (three screenshots, listed by path), with the Metal backend additionally gated by `metal_offscreen_render_writes_pixels`. Leave the Linux frame-time-plot debt row open — that is a VM-track item.

- [ ] **Step 9: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/DEFERRALS.md .port/phase-2/MACOS_APP_GAPS.md .port/phase-2/VERIFICATION_DEBT.md
git commit -m "ledgers: strike the stale macOS-inspector deferrals + crash-guard sentinel (WS6 T7)"
git push
```

## Task 8: Merge

**Files:**
- Modify: `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`

- [ ] **Step 1: Re-run the both-platform gate on the final branch state**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -3
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws6-macos-inspector && git pull --ff-only && cargo build --workspace' 2>&1 | tail -3
```

Expected: both green. Do not merge otherwise.

- [ ] **Step 2: Tick WS6 in the roadmap checklist**

In `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`, replace the plan-authoring-checklist line

```
- [ ] WS6–WS17: authored at wave start
```

with

```
- [x] WS6 macOS inspector (research note: the dcimgui ImGui_ImplMetal FFI, the full 13-module shared widget render, the core activate/deactivate seams, the embedded init/initMetal/renderMetal path and the rustty_inspector_metal_* exports had ALL already landed in the 2026-07-14 inspector series; WS6's genuine residual was the `updateContentScale` ImGuiStyle rescale, an offscreen-Metal rasterization KAT, the never-performed ⌘⌥I shakedown, and five stale ledger sections)
- [ ] WS7–WS17: authored at wave start
```

- [ ] **Step 3: Commit the roadmap tick**

```bash
cd /Users/gael/projects/rustty
git add docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md
git commit -m "roadmap: tick WS6 macOS inspector (WS6 T8)"
git push
```

- [ ] **Step 4: Merge and clean up**

REQUIRED SUB-SKILL: `superpowers:finishing-a-development-branch`. Merge `ws6-macos-inspector` → `main` only with both platform builds green on the merge result, then delete the branch locally, on `origin`, and return the VM checkout to `main`:

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull --ff-only
git merge --no-ff ws6-macos-inspector -m "merge: WS6 macOS terminal inspector (live cmd+opt+i)"
cargo build --workspace 2>&1 | tail -3
git push
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout main && git pull --ff-only && cargo build --workspace' 2>&1 | tail -3
git branch -d ws6-macos-inspector
git push origin --delete ws6-macos-inspector
```

Expected: both post-merge builds green; branch deleted on both sides.

---

## Self-Review Notes (performed at authoring time)

**1. Spec coverage.** Every WS6 item named in the task brief maps to a task:

| Brief item | Task |
|---|---|
| dcimgui `ImGui_ImplMetal` FFI (mirror the GTK OpenGL3 precedent) | T0 Step 2 + **T1** (verified already landed; T1 adds the missing rasterization proof) |
| "How much of the ~4.4k-LOC widget render is already ported vs Metal-specific?" | Answered in **Scope finding** with LOC + commit evidence; enforced by **T2** |
| `Surface.activate/deactivateInspector` (DEFERRALS ~1827) | Verified live (Scope finding); ledger struck in **T7 Steps 2–3** |
| `embedded::Inspector::{init,initMetal,renderMetal}` + `rustty_inspector_metal_*` (DEFERRALS ~2112-2161) | Verified live; comments corrected in **T4**; ledger struck in **T7 Step 4** |
| Remove the crash-guard no-op sentinel (DEFERRALS ~4567-4615) | **T4 Step 6** (proves none remains in code) + **T7 Step 5** (removes the ledger claim) — the plan states explicitly that the code sentinel was already gone |
| Oracle fidelity (`src/inspector/**`, embedded.zig inspector fns, ImGui_ImplMetal usage) | Oracle body quoted verbatim in **T3**; oracle TODO/module cross-check in **T2 Steps 2–3** |
| C-ABI panic policy ABORT (DG5) | Global Constraints; **T4** keeps the exports free of `catch_unwind`; **T5 Step 4** treats an abort as a hard failure with a crash-report capture |
| GTK/Linux inspector must keep building | Global Constraints; enforced by **T6 Steps 3–4** (VM build + `opengl3_symbols_link` + `--features gtk`) |
| Acyclic crate graph gate | **T6 Step 2**, with the explicit note that T1's additions are dev-dependencies only |
| Inspector KATs following existing test patterns | **T1** follows `dcimgui-sys`'s `IMGUI_CONTEXT_TEST_LOCK` + KAT-doc style and the `gtk_inspector_dump` "assert the framebuffer is not a uniform clear" idea; **T3** follows the `host_view_tests` unit-module style in the same file |
| End-to-end: build workspace, `macos/build-rustty.sh`, launch, ⌘⌥I, confirm live render — MANUAL | **T5**, flagged MANUAL/screenshot in bold with an explicit "do not claim from a process tree" rule |
| Both-platform build gate incl. `ssh rvm '…cargo build --workspace'` | **T6 Step 3** |
| DEFERRALS ledger strikes incl. sentinel entry | **T7** (5 DEFERRALS sections + MACOS_APP_GAPS + VERIFICATION_DEBT) |
| Branch/merge flow per the roadmap | **T0** + **T8** |

**2. Placeholder scan.** No "TBD" / "implement later" / "add error handling" / "similar to Task N". Every code step carries complete, compilable code. The three places that could read as vague are bounded decision procedures with an explicit stop condition, not placeholders: T1 Step 4's objc2-metal feature-name fix (the compiler error names the exact gate), T2 Step 1's gate-RED remediation (the gate output names the file and the dropped comment), and T7 Step 3's "write whatever count you just measured" (a measurement, not a guess — deliberately not hard-coded, because asserting an unverified number in a ledger is the exact failure mode this WS is cleaning up).

**3. Type / name consistency.** Checked across tasks: `Inspector` disambiguated everywhere it appears (`embedded::Inspector` = the apprt struct with fields `surface`/`ig_ctx`/`backend`/`content_scale`/`instant`; `rustty_inspector::Inspector` = the shared widget aggregate; `Backend::Metal` the enum variant). FFI names used identically in T1 and T3 and matched against the generated `bindings.rs` and `src/lib.rs` at authoring time: `ImGui_CreateContext`, `ImGui_DestroyContext`, `ImGui_SetCurrentContext`, `ImGui_GetIO`, `ImGui_GetStyle`, `ImGui_NewFrame`, `ImGui_Render`, `ImGui_GetDrawData`, `ImGui_Begin`, `ImGui_End`, `ImGui_Text`, `ImGui_SetNextWindowPos`, `ImGui_SetNextWindowSize`, `ImGui_StyleColorsDark`, `ImGuiStyle_ImGuiStyle`, `ImGuiStyle_ScaleAllSizes`, `ImGui_ImplMetal_{Init,NewFrame,RenderDrawData,Shutdown}`, `ImVec2`. objc2-metal selectors verified against the in-repo precedent (`rustty-renderer/src/metal/{texture,render_pass,frame}.rs`) and the crate source: `MTLCreateSystemDefaultDevice`, `newCommandQueue`, `commandBuffer`, `MTLTextureDescriptor::new` + `setPixelFormat`/`setWidth`/`setHeight`/`setStorageMode`/`setUsage`, `newTextureWithDescriptor`, `MTLRenderPassDescriptor::renderPassDescriptor`, `colorAttachments`/`objectAtIndexedSubscript`/`setTexture`/`setLoadAction`/`setStoreAction`/`setClearColor`, `renderCommandEncoderWithDescriptor`, `endEncoding`, `commit`, `waitUntilCompleted`, `getBytes_bytesPerRow_fromRegion_mipmapLevel`. Test names are unique and referenced consistently in T7's ledger text: `metal_offscreen_render_writes_pixels`, `update_content_scale_rescales_active_imgui_style`, `update_content_scale_is_idempotent_not_compounding`.

**4. Known-answer sourcing.** T3's expected values are read from the vendored `crates-sys/dcimgui-sys/vendor/imgui/imgui.cpp` (`ImGuiStyle::ImGuiStyle()` defaults `WindowPadding=(8,8)`, `ScrollbarSize=14`, `GrabMinSize=12`) composed with `ImGuiStyle::ScaleAllSizes`'s `ImTrunc(v * scale)`. They are exact for `scale = 2.0` (no truncation ambiguity), which is why the KAT uses 2.0 rather than a fractional scale.
