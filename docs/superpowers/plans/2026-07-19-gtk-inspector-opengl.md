# GTK Terminal Inspector (Dear ImGui over OpenGL) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (or a dynamic Workflow) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `toggle-inspector` (Ctrl+Shift+I) open a functional Dear ImGui terminal inspector window in the GTK build, rendered with the OpenGL3 ImGui backend — GTK parity with the already-shipping macOS/Metal inspector.

**Architecture:** The shared `rustty-inspector` crate + core `activate/deactivateInspector` seam are already done and GPU-backend-agnostic (macOS proves it via a Metal wrapper). This plan adds the missing GTK wrapper: (1) the OpenGL3 backend in `dcimgui-sys` (mirror of the Metal wiring), (2) three gtk4-rs GObject classes ported faithfully from ghostty (`ImguiWidget` → `InspectorWidget` → `InspectorWindow`), and (3) the action wiring, all owning the per-surface ImGui context (INV-11/INV-12/R6). On-by-default in the `gtk` build.

**Tech Stack:** Rust, `gtk4`/`libadwaita` (gtk4-rs, `glib::subclass`), `dcimgui-sys` (Dear ImGui C FFI), `imgui_impl_opengl3.cpp` (C++ GL backend), `cc` crate, the existing `rustty-inspector` crate, `rustty-vt` input mapping, `rustty-renderer` OpenGL framebuffer readback (for the KAT).

## Global Constraints

- **R1 fidelity** — port every line of each oracle `.zig`/`.blp`; preserve its comments as ported comments. Oracle: `/home/gael/projects/ghostty` @ `4749c4e93` (READ-ONLY).
- **INV-11** — the ImGui context is per-Surface/per-widget; `ImGui_SetCurrentContext(ctx)` MUST precede every ImGui call; `Inspector::setup()` mutates the shared IO exactly once (docking + sRGB + no ini/log + embedded font @ 16*2px).
- **INV-12** — a rendered inspector frame forces a terminal rebuild via `wakeup.notify()` (already inside `rustty_inspector::Inspector::render`).
- **R6** — all dcimgui/ImGui calls are `!Send`, main-thread-affine, and context-scoped; set-current-context at exactly the points the oracle does.
- **Main-thread only** — the GLArea `render`/`realize`/`unrealize`/input handlers all run on the GTK main thread; never touch the ImGui context off-thread.
- **On-by-default gating** — the `gtk` feature enables `rustty-vt/imgui`; the inspector is wired unconditionally in the gtk build. No new per-file `#[cfg(feature)]` gates.
- **Scale handling (match exactly or fonts blur)** — `io.DisplaySize` unscaled, `io.DisplayFramebufferScale = (1,1)`, scaling applied by rebuilding `ImGuiStyle` + `ScaleAllSizes(scale_factor)`; mouse positions pre-multiplied by the widget scale factor; font hardcoded at 2×.
- **Build gate** — `cargo build -p rustty-apprt --features gtk` stays green after every task; `cargo build --workspace` green at phase ends.
- **Git** — explicit paths; retry on `index.lock` (sleep 3, ≤10×); never push/reset/stash. Commit after each task with the message shown.

---

## File Structure

**Create:**

- `crates-sys/dcimgui-sys/vendor/imgui/backends/imgui_impl_opengl3.cpp` — vendored GL backend (git-tracked).
- `crates-sys/dcimgui-sys/vendor/imgui/backends/imgui_impl_opengl3.h` — vendored header.
- `crates-sys/dcimgui-sys/vendor/imgui/backends/imgui_impl_opengl3_loader.h` — vendored GL loader.
- `crates/rustty-apprt/src/gtk/class/imgui_widget.rs` — generic ImGui-over-GLArea host (port of `imgui_widget.zig`).
- `crates/rustty-apprt/src/gtk/class/inspector_widget.rs` — inspector subclass (port of `inspector_widget.zig`).
- `crates/rustty-apprt/src/gtk/class/inspector_window.rs` — inspector window (port of `inspector_window.zig`).
- `crates/rustty-apprt/tests/gtk_inspector_dump.rs` — headed framebuffer→PPM KAT.

**Modify:**

- `crates-sys/dcimgui-sys/build.rs` — non-macOS OpenGL3 compile branch + `ZIGPKG_IMGUI_ENABLE_OPENGL3` define.
- `crates-sys/dcimgui-sys/src/lib.rs` — `ImGui_ImplOpenGL3_*` FFI decls (non-macOS).
- `crates/rustty-vt/src/input/key.rs:792` — fill `imgui_key` from the verbatim body at `:1002`.
- `crates/rustty-apprt/Cargo.toml` — `gtk` feature enables `rustty-vt/imgui`.
- `crates/rustty-apprt/src/gtk/class/mod.rs` (or the class module index) — declare + register the 3 new classes.
- `crates/rustty-apprt/src/gtk/class/window.rs:1154` — real `toggle_inspector`.
- `crates/rustty-apprt/src/gtk/class/surface.rs:1835` — real `control_inspector` (window construct/present/destroy).
- `crates/rustty-apprt/src/gtk/surface.rs:259` — real `redraw_inspector`.

**Reused as-is (do NOT modify):** `crates/rustty-inspector/*` (the shared inspector), `crates/rustty-core/src/surface.rs` (activate/deactivate seam), the 3 `.blp` templates in `crates/rustty-apprt/assets/ui/1.5/` (already tracked), `crates/rustty-apprt/src/embedded.rs` (macOS Metal path).

---

## Phase 1 — OpenGL3 backend (dcimgui-sys)

### Task 1: Vendor + compile `imgui_impl_opengl3` into dcimgui-sys

**Files:**

- Create (git add): `crates-sys/dcimgui-sys/vendor/imgui/backends/imgui_impl_opengl3.{cpp,h}`, `imgui_impl_opengl3_loader.h`
- Modify: `crates-sys/dcimgui-sys/build.rs`

**Interfaces:**

- Produces: a `dcimgui-sys` that compiles `imgui_impl_opengl3.cpp` on non-macOS and defines `ZIGPKG_IMGUI_ENABLE_OPENGL3` for the `ext.cpp` TU (activating the `ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup` shim already at `ext.cpp:44`).

- [ ] **Step 1: Source the vendored files at the matching imgui version.** Read the vendored imgui version: `grep IMGUI_VERSION_NUM crates-sys/dcimgui-sys/vendor/imgui/imgui.h`. Copy the three backend files from ghostty's vendored imgui (same version the Metal files came from) — locate with `find /home/gael/projects/ghostty -path '*backends/imgui_impl_opengl3*'` (ghostty's dcimgui pkg cache under `.zig-cache`/`zig-out` or the pkg source). If not present there, fetch from the upstream imgui repo at the exact matching tag. Verify the copied `imgui_impl_opengl3.cpp` `#include "imgui_impl_opengl3_loader.h"` and the version guard matches. `git add` all three.
- [ ] **Step 2: Add the compile branch in `build.rs`.** Mirror the Metal branch (`build.rs:155-194`) but non-macOS and no ObjC++. In the main `ext.cpp`/core build, when `target_os != "macos"`, add `build.define("ZIGPKG_IMGUI_ENABLE_OPENGL3", "1");` (mirrors the Metal `build.define("ZIGPKG_IMGUI_ENABLE_METAL", "1")` at `build.rs:115`). Compile `imgui_impl_opengl3.cpp` — it can join the core TU list (`build.rs:119-137`) gated on non-macOS, `.include("vendor/imgui")`. No extra system libs needed: the vendored `imgui_impl_opengl3_loader.h` is a self-contained GL loader (no libepoxy/GLEW link). Keep it `#[cfg(not(target_os = "macos"))]` in build.rs so macOS still builds Metal-only.
- [ ] **Step 3: Build.** `cargo build -p dcimgui-sys` on this Linux box → compiles the `.cpp` clean. Expected: green. Confirm the symbol is emitted: `nm -D --defined-only target/debug/build/dcimgui-sys-*/out/*.o 2>/dev/null | grep -i ImplOpenGL3` OR inspect the build output dir; at minimum the crate builds without "undefined ZIGPKG_IMGUI_ENABLE_OPENGL3" and `ext.cpp`'s shim now compiles (it was dead-gated before).
- [ ] **Step 4: Commit** (`inspector(gtk): vendor + compile imgui_impl_opengl3 into dcimgui-sys (Task 1)`).

### Task 2: Expose the ImGui_ImplOpenGL3 FFI in dcimgui-sys

**Files:**

- Modify: `crates-sys/dcimgui-sys/src/lib.rs` (mirror the Metal FFI block at `:202-241`, but `#[cfg(not(target_os = "macos"))]`)

**Interfaces:**

- Consumes: the compiled backend (Task 1).
- Produces (exact decls):

  ```rust
  #[cfg(not(target_os = "macos"))]
  extern "C" {
      /// glsl_version = null → backend default #version. (imgui_widget.zig:236 passes null.)
      pub fn ImGui_ImplOpenGL3_Init(glsl_version: *const core::ffi::c_char) -> bool;
      pub fn ImGui_ImplOpenGL3_Shutdown();
      pub fn ImGui_ImplOpenGL3_NewFrame();
      pub fn ImGui_ImplOpenGL3_RenderDrawData(draw_data: *mut ImDrawData);
      /// ext.cpp:44 — Shutdown + zero the imgl3w loader proc table.
      pub fn ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup();
  }
  ```

- [ ] **Step 1: Add the FFI block** to `src/lib.rs`, `#[cfg(not(target_os = "macos"))]`, adjacent to the Metal block. `ImDrawData` is already in scope (used by the Metal decls). Use `core::ffi::c_char`.
- [ ] **Step 2: Link test.** Add a `#[cfg(not(target_os = "macos"))] #[test] fn opengl3_symbols_link() { let _ = ImGui_ImplOpenGL3_Init as *const (); let _ = ImGui_ImplOpenGL3_Shutdown as *const (); let _ = ImGui_ImplOpenGL3_NewFrame as *const (); let _ = ImGui_ImplOpenGL3_RenderDrawData as *const (); let _ = ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup as *const (); }` — referencing each symbol so a missing export is a link error (no GL context needed).
- [ ] **Step 3: Build + test.** `cargo test -p dcimgui-sys opengl3_symbols_link` → PASS (links). Expected: green; a missing symbol would be an unresolved-external link error.
- [ ] **Step 4: Commit** (`inspector(gtk): dcimgui-sys ImGui_ImplOpenGL3 FFI surface (Task 2)`).

---

## Phase 2 — GTK wrapper (widget → subclass → window → wiring)

### Task 3: Fill `Key::imgui_key` + enable `imgui` in the gtk build

**Files:**

- Modify: `crates/rustty-vt/src/input/key.rs:792` (replace `unimplemented!()`), using the verbatim reference body preserved at `key.rs:1002`
- Modify: `crates/rustty-apprt/Cargo.toml` (the `gtk` feature enables `rustty-vt/imgui`)

**Interfaces:**

- Produces: `rustty_vt::input::Key::imgui_key(self) -> Option<c_int>` returning the `ImGuiKey_*` constant for a `Key` (used by `ImguiWidget::key_event` in Task 4). Compiled whenever `feature = "imgui"` is on.

- [ ] **Step 1: Port the body.** Read the verbatim reference body at `key.rs:1002` (the `imguiKey` `switch` preserved as a comment) and the oracle `src/input/key.zig:545`. Replace the `unimplemented!("...")` at `key.rs:796` with the real `match self { … => dcimgui_sys::ImGuiKey_… as c_int, … }`, returning `Some(constant)` for mapped keys and `None` otherwise. Preserve the reference comment lines.
- [ ] **Step 2: Enable the feature for gtk.** In `crates/rustty-apprt/Cargo.toml`, add `rustty-vt/imgui` to the `gtk` feature's enable list (e.g. `gtk = ["dep:...", "rustty-vt/imgui", ...]`). Confirm `rustty-vt` is already a dep; if the `imgui` feature needs `dcimgui-sys`, wire `imgui = ["dep:dcimgui-sys"]` in `rustty-vt/Cargo.toml` (it is currently `imgui = []` — check whether `key.rs` references `dcimgui_sys` directly and add the dep-feature if so).
- [ ] **Step 3: Build.** `cargo build -p rustty-vt --features imgui` → green (imgui_key compiles). `cargo build -p rustty-apprt --features gtk` → green (feature propagates).
- [ ] **Step 4: Commit** (`inspector(gtk): fill Key::imgui_key + enable imgui in the gtk build (Task 3)`).

### Task 4: `ImguiWidget` — generic ImGui-over-GLArea GObject

**Files:**

- Create: `crates/rustty-apprt/src/gtk/class/imgui_widget.rs`
- Modify: the class module index (`crates/rustty-apprt/src/gtk/class/mod.rs` or wherever classes are declared — follow the existing `surface`/`window` registration pattern) to add `pub mod imgui_widget;`
- Oracle: `src/apprt/gtk/class/imgui_widget.zig` (551 L) + `assets/ui/1.5/imgui-widget.blp` (already tracked)

**Interfaces:**

- Consumes: `dcimgui_sys` (`ImGui_CreateContext`/`DestroyContext`/`SetCurrentContext`/`GetIO`/`NewFrame`/`Render`/`GetDrawData`/`GetStyle`, the ext `ImGuiStyle_ImGuiStyle` + `ImGuiStyle_ScaleAllSizes`), the Task 2 `ImGui_ImplOpenGL3_*` FFI, `rustty_vt::input::Key::imgui_key` (Task 3).
- Produces: `class::imgui_widget::ImguiWidget` (glib subclass of `adw::Bin`, GType `"RusttyImguiWidget"`), template child `gl_area: gtk4::GLArea`, `im_context: gtk4::IMMulticontext`; a subclassable virtual seam `imgui_setup(&self)` / `imgui_render(&self)` (default impls log a "should be overridden" warning); public `queue_render(&self)` → `gl_area.queue_render()`; `set_current_context(&self)`.

- [ ] **Step 1: Scaffold the GObject** using the existing gtk4-rs pattern in `crates/rustty-apprt/src/gtk/class/` (`#[glib::object_subclass]`, `ObjectSubclass`, `#[template]` bound to `gresource.blueprint(1,5,"imgui-widget")`; bind template children `gl_area`, `im_context`; wire the `.blp` callback names via `#[template_callback]`). Private fields: `ig_context: Cell<*mut dcimgui_sys::ImGuiContext>` (null init), `instant: Cell<Option<Instant>>`, `tick_callback_id: Cell<Option<TickCallbackId>>`, `last_render_time: Cell<Option<Instant>>`. Establish the **virtual seam**: define a `pub trait ImguiWidgetImpl: ObjectSubclass` (or use the gtk4-rs parent-class-override idiom) with `imgui_setup(&self)` / `imgui_render(&self)`; `ImguiWidget` calls them via its subclass — this replaces the oracle's hand-rolled vtable (imgui_widget.zig:21-23,40-50).
- [ ] **Step 2: Port realize/unrealize** (imgui_widget.zig:214-272). realize: `gl_area.make_current()` + `gl_area.error()` check → `ImGui_CreateContext(null)` → `set_current_context` → `io.BackendPlatformName = "rustty_gtk"` → `ImGui_ImplOpenGL3_Init(null)` → `self.imgui_setup()` → `add_tick_callback`. unrealize: remove tick callback → `make_current()` → `set_current_context` → `ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup()` → `ImGui_DestroyContext` + null the field. Ported comments (esp. the "unrealize may lack a current context" note at :260-261).
- [ ] **Step 3: Port the render handler** (imgui_widget.zig:296-324, the two-pass loop) + `new_frame` (`:139-153`, `io.DeltaTime` from `Instant`) + `resize` (`:275-293`, DisplaySize/style-scale). Do NOT call `make_current` in render (GTK guarantees current). End render with `glClearColor(0x28,0x2C,0x34)/glClear/ImGui_ImplOpenGL3_RenderDrawData(ImGui_GetDrawData())`. Use the GL calls the renderer crate already links (or raw `gl` fn pointers via `epoxy`/the existing GL loader used by `rustty-renderer`); match how the terminal GLArea issues GL.
- [ ] **Step 4: Port all input handlers** (imgui_widget.zig:326-475): focus enter/leave (`AddFocusEvent`), `key_event` (queue render; 4 modifier key events via `key::translate_mods`; `Key::imgui_key` → `AddKeyEvent`; forward the GDK event to `im_context.filter_keypress`), gesture click pressed/released (`AddMouseButtonEvent`, button map via `translate_mouse_button` :192-200), motion (`AddMousePosEvent(x*scale,y*scale)`), scroll (`AddMouseWheelEvent(x,-y)`), IM commit (`AddInputCharactersUTF8`), and the 30-FPS `tick_callback` (`:452-475`) that throttles `queue_render`. Use `rustty_vt::input` for `translate_mods`/`key_from_keyval` equivalents already ported in the apprt input layer.
- [ ] **Step 5: Build.** `cargo build -p rustty-apprt --features gtk` → green. (No headed test yet — exercised in Task 8.)
- [ ] **Step 6: Commit** (`inspector(gtk): ImguiWidget — ImGui-over-GLArea host (Task 4)`).

### Task 5: `InspectorWidget : ImguiWidget`

**Files:**

- Create: `crates/rustty-apprt/src/gtk/class/inspector_widget.rs`
- Modify: class module index (`pub mod inspector_widget;`)
- Oracle: `src/apprt/gtk/class/inspector_widget.zig` (245 L) + `assets/ui/1.5/inspector-widget.blp`

**Interfaces:**

- Consumes: `ImguiWidget` (Task 4) as parent; `rustty_inspector::Inspector::{setup, render}`; the gtk `Surface` class `core()` accessor + core `CoreSurface::{activateInspector, deactivateInspector, inspector}`.
- Produces: `class::inspector_widget::InspectorWidget` (subclass of `ImguiWidget`, GType `"RusttyInspectorWidget"`) with a `surface` GObject property (custom getter/setter) and the two virtual overrides.

- [ ] **Step 1: Scaffold the subclass** (`#[template]` bound to `inspector-widget` blueprint; parent = `ImguiWidget`; `glib::ensure_type::<ImguiWidget>()` in class_init before template). Private: `surface: Cell<*mut SurfaceGObj>` (raw, no strong ref — weak-notify managed) per inspector_widget.zig:45-49.
- [ ] **Step 2: Override the virtuals** (`imgui_setup` → `rustty_inspector::Inspector::setup()`; `imgui_render` → the `:85-93` body: `surface → core() → core.inspector → inspector.render(surface_data, wakeup)`. Build the `SurfaceData` (the R4 seam type `rustty_inspector::widgets::surface::SurfaceData`) from the core surface exactly as the macOS path does in `embedded.rs` renderMetal — reuse that construction).
- [ ] **Step 3: Port `set_surface` + weak lifecycle** (inspector_widget.zig:124-203): old → `deactivateInspector` + weak-unref; new → `activateInspector` + `weak_ref`(notify) + store + `queue_render`. `dispose` → `set_surface(null)`. `surface_weak_notify` → `deactivateInspector` + null + notify the `surface` pspec. Use `glib::object::ObjectExt::add_weak_ref_notify`. Preserve the dispose-vs-finalize lifecycle comments (:185-196).
- [ ] **Step 4: Build.** `cargo build -p rustty-apprt --features gtk` → green.
- [ ] **Step 5: Commit** (`inspector(gtk): InspectorWidget subclass wiring the shared inspector (Task 5)`).

### Task 6: `InspectorWindow`

**Files:**

- Create: `crates/rustty-apprt/src/gtk/class/inspector_window.rs`
- Modify: class module index (`pub mod inspector_window;`)
- Oracle: `src/apprt/gtk/class/inspector_window.zig` (218 L) + `assets/ui/1.5/inspector-window.blp`

**Interfaces:**

- Consumes: `InspectorWidget` (Task 5).
- Produces: `class::inspector_window::InspectorWindow` (subclass of `adw::ApplicationWindow`, GType `"RusttyInspectorWindow"`), `new(surface: &Surface) -> Self`, `present(&self)`, `queue_render(&self)` (→ embedded widget), a `surface` property bound down to the widget (`surface: bind template.surface` in the `.blp`), and a `debug` property.

- [ ] **Step 1: Scaffold the window** (`#[template]` bound to `inspector-window` blueprint; parent = `adw::ApplicationWindow`; `glib::ensure_type::<InspectorWidget>()` + the `DebugWarning` type if the `.blp` references `$RusttyDebugWarning`). Template child `inspector_widget`. Private: `surface: glib::WeakRef<SurfaceGObj>`.
- [ ] **Step 2: Port `new`/`present`/`queue_render`/`set_surface`** (inspector_window.zig:117-140) + the `notify::surface` handler `prop_inspector_surface` (`:160-171`): when the widget's surface becomes null, `self.destroy()` (auto-close on terminal close). `dispose` → `set_surface(null)` (clear weak refs — the oracle flags Valgrind-found corruption otherwise, :96-101). Add the `"devel"` CSS class in debug builds; set the window icon/title (`"Rustty: Terminal Inspector"`).
- [ ] **Step 3: Build.** `cargo build -p rustty-apprt --features gtk` → green.
- [ ] **Step 4: Commit** (`inspector(gtk): InspectorWindow host + auto-close teardown (Task 6)`).

### Task 7: Wire the actions (toggle / control / redraw) + type registration

**Files:**

- Modify: `crates/rustty-apprt/src/gtk/class/window.rs:1154` (`toggle_inspector`), `crates/rustty-apprt/src/gtk/class/surface.rs:1835` (`control_inspector`), `crates/rustty-apprt/src/gtk/surface.rs:259` (`redraw_inspector`)
- Oracle: `src/apprt/gtk/class/surface.zig:885-913` (`controlInspector` + `redrawInspector`), `window.zig:2069-2083`

**Interfaces:**

- Consumes: `InspectorWindow` (Task 6).
- Produces: a functional `win.toggle-inspector` action path; the surface's `inspector` weak backref (`gtk/class/surface.rs:396`) now holds an `InspectorWindow`.

- [ ] **Step 1: `control_inspector`** (surface.rs:1835) — port `surface.zig:885-907`: if a window exists → `.toggle/.hide` destroys it; else `.toggle/.show` → `InspectorWindow::new(self)` + `present()` + `weak_ref`(notify nulls the backref) + store on `self.imp().inspector`. Return `true`.
- [ ] **Step 2: `toggle_inspector`** (window.rs:1154) — replace the `eprintln!` warn with: resolve the active surface, call its `control_inspector(InspectorMode::Toggle)` (the accelerator sync at `application.rs:764` already maps `win.toggle-inspector` → `Action::Inspector(Toggle)`; the core-action seam `control_inspector` at `application.rs:3335` already forwards to the surface — this un-stubs the surface leaf).
- [ ] **Step 3: `redraw_inspector`** (gtk/surface.rs:259) — port `surface.zig:909-913`: `if let Some(win) = self.inspector() { win.queue_render() }`. (Note: the widget self-drives via its 30-FPS tick, so this is parity-completeness.)
- [ ] **Step 4: Build + smoke.** `cargo build -p rustty-apprt --features gtk` → green; `cargo build --workspace` → green (macOS Metal path untouched, cfg-gated). Verify no acyclic-dep regression if the check gate exists (`.port/tools/gates/check_acyclic.sh`).
- [ ] **Step 5: Commit** (`inspector(gtk): wire toggle/control/redraw inspector actions (Task 7)`).

---

## Phase 3 — Verification

### Task 8: `gtk_inspector_dump.rs` — headed framebuffer KAT

**Files:**

- Create: `crates/rustty-apprt/tests/gtk_inspector_dump.rs`
- Reference: `crates/rustty-apprt/tests/gtk_glyph_dump.rs` (the readback/PPM harness to reuse)

**Interfaces:**

- Consumes: the full inspector path (Tasks 1-7), `rustty_renderer` `read_bound_framebuffer_rgba` + the `write_ppm` helper pattern.

- [ ] **Step 1: Write the KAT** (`#[ignore]`, `--exact`, env `RUSTTY_DUMP_PPM`): construct a headed GTK surface (as `gtk_glyph_dump.rs` does), open the inspector (`control_inspector(Toggle)`), pump the main loop so the InspectorWidget realizes + renders, feed a synthetic key event into the ImguiWidget, render again. Read back the inspector GLArea framebuffer (`read_bound_framebuffer_rgba(w,h)`), `write_ppm`. Assert: (a) the framebuffer is NOT uniformly the `#282C34` clear color (panels drew), and (b) needle-scan the keyboard-panel region for a pixel-delta between the pre-key and post-key frames (proves live input reaches the panel), mirroring the grid needle-scan at `gtk_glyph_dump.rs:209`.
- [ ] **Step 2: Run it.** `RUSTTY_DUMP_PPM=/home/gael/tmp/inspector.ppm cargo test -p rustty-apprt --features gtk --test gtk_inspector_dump -- --ignored --exact inspector_dump` → PASS. Convert + eyeball: `magick /home/gael/tmp/inspector.ppm /home/gael/tmp/inspector.png`.
- [ ] **Step 3: Commit** (`inspector(gtk): gtk_inspector_dump KAT — framebuffer + live-input assert (Task 8)`).

### Task 9: End-to-end shakedown — Ctrl+Shift+I opens a live inspector (native Xorg screenshot)

**Files:** none (verification); may fix wiring surfaced by the live build.

- [ ] **Step 1:** `cargo build -p rustty-apprt --features gtk` green. Launch the dev build (per the `rustty-gtk-dev-run` recipe: `RUSTTY_RESOURCES_DIR`, `setsid`, pidfile) on the native-Xorg session.
- [ ] **Step 2: Screenshot-verify** (do NOT claim from the process tree). Press Ctrl+Shift+I → a separate 1000×600 "Rustty: Terminal Inspector" window opens. `import -window root` (native Xorg captures the GLArea; XAUTHORITY=/run/user/1000/gdm/Xauthority). Confirm the panels render (terminal/screen/cursor/keyboard/termio/modes/cell). Type input → the keyboard-events panel populates; run shell output → the termio VT-events panel populates.
- [ ] **Step 3:** Confirm INV-12 (an inspector edit triggers a terminal re-render) and, if feasible, INV-3 (a scrolling-region edit in the terminal panel takes effect). Kill the instance by pid (never `pkill -f target/debug/rustty`). Commit any wiring fixes (`inspector(gtk): end-to-end shakedown — Ctrl+Shift+I inspector functional (Task 9)`).

- [ ] **Step 4: Update the ledger.** Mark the dcimgui inspector deferral resolved in `.port/phase-2/DEFERRALS.md` (the G3b / R-d GTK rows) and `docs/manual-test-plan-gtk.md`; update the parity memory (`rustty-gtk-parity-status`) — the last GTK deferral is closed. Commit (`docs: GTK inspector implemented — close the last Linux/GTK deferral`).

---

## Self-Review

**1. Spec coverage** (against the design doc):

- OpenGL3 backend (vendor + build + FFI) → Tasks 1-2. ✅
- `ImguiWidget` (context lifecycle, two-pass render, scale, all input, 30-FPS tick, virtual seam) → Task 4. ✅
- `InspectorWidget` (setup/render overrides, surface weak lifecycle) → Task 5. ✅
- `InspectorWindow` (host, auto-close teardown) → Task 6. ✅
- Wiring (toggle/control/redraw) + `Key::imgui_key` + on-by-default gating → Tasks 3, 7. ✅
- Verification KAT + live screenshot → Tasks 8, 9. ✅
- Invariants INV-11 (Task 4 realize/render + Task 5 setup), INV-12 (inside `Inspector::render`, exercised Task 5/8), R6 (Task 4 set-current-context discipline). ✅

**2. Placeholder scan:** This is an R1 port — each task cites the exact oracle file + line ranges + the verbatim call sequences (from the design doc's component map); the novel/non-mechanical pieces (build.rs branch, FFI signatures, the vtable→subclass translation, the action wiring) carry concrete Rust. No "TBD"/"handle edge cases". Acceptable for a port plan (same precedent as `2026-07-14-inspector-port.md`).

**3. Type/name consistency:** `ImguiWidget`/`InspectorWidget`/`InspectorWindow` (GTypes `Rustty*`), `imgui_setup`/`imgui_render` (virtual seam, Task 4→5), `set_surface`/`surface_weak_notify` (Task 5), `queue_render` (Task 4→6→7), `control_inspector`/`redraw_inspector`/`toggle_inspector` (Task 7), `ImGui_ImplOpenGL3_*` FFI (Task 2→4), `Key::imgui_key` (Task 3→4), `SurfaceData` (Task 5, from `rustty_inspector::widgets::surface`). Consistent.

**Sequencing for the Workflow:** Task 1 → Task 2 (FFI needs the compiled backend). Task 3 is independent (parallel with T1/T2). Task 4 needs T2 + T3. Then strictly sequential: T4 → T5 → T6 → T7 (each builds on the prior GObject). T8 needs T7; T9 needs everything (and T8's confidence). So the parallel front is {T1→T2, T3}; the tail T4→T5→T6→T7→T8→T9 is a chain. Each task touches distinct files; the only shared file is the class module index (append-only `pub mod` lines across T4/T5/T6 — sequence them or have the wiring task add all three).
