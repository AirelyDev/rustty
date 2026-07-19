# GTK Terminal Inspector (Dear ImGui over OpenGL) — Design

**Date:** 2026-07-19
**Status:** Approved (design)
**Oracle:** `/home/gael/projects/ghostty` @ `4749c4e93` (READ-ONLY) — `src/apprt/gtk/class/{imgui_widget,inspector_widget,inspector_window}.zig` + `src/apprt/gtk/ui/1.5/{imgui-widget,inspector-widget,inspector-window}.blp` + `src/apprt/gtk/{Surface,App}.zig` inspector methods.

## Goal

Make the Rustty GTK terminal inspector functional: pressing the `toggle-inspector` accelerator (Ctrl+Shift+I) opens a separate window showing the live Dear ImGui inspector — the same panels that already work on macOS (terminal/screen/cursor/keyboard/termio/modes/cell) — rendered with the **OpenGL3** ImGui backend instead of Metal.

## Why this is the last GTK deferral, and what "already works on macOS" means

The inspector is ~90% shared and already ported:

- **`rustty-inspector` crate** (~11.6k lines, all 13 widget files) is fully inhabited with real Dear ImGui draw code. `Inspector::render(surface_data, wakeup)` (`Inspector.rs:355`) is **deliberately GPU-backend-agnostic and context-agnostic** — it draws into whatever ImGui context is current and leaves context selection + the GPU backend to the apprt wrapper (`Inspector.rs:350-353`).
- **Core seam** — `Surface::activateInspector` / `deactivateInspector` (`rustty-core/src/surface.rs:2873/2967`), the `inspector` field, and the keyCallback/mouse recording paths are all live.
- **Data-model invariants** INV-1..10, INV-13 and hazards R1..R5 are realized inside the crate — backend-independent, reused verbatim.

macOS works because `crates/rustty-apprt/src/embedded.rs` wraps that shared core with a **Metal** backend (`ImGui_ImplMetal_*`, `Backend::Metal`). GTK has no equivalent wrapper. That wrapper — the OpenGL3 backend plus the GTK widget/window glue that owns the per-surface ImGui context — is the entire remaining gap.

The only invariants still "live" for this work are the ones the wrapper owns:
- **INV-11** — per-Surface ImGui context; `ImGui_SetCurrentContext(ctx)` must precede every ImGui call; `Inspector::setup()` mutates the shared IO once (docking + sRGB + no ini/log + embedded font @ 16*2px).
- **INV-12** — a rendered frame forces a terminal rebuild via `wakeup.notify()` (already inside `Inspector::render`).
- **R6** — dcimgui calls are `!Send` / context-scoped; set-current-context at the same points the reference does.

## Decisions

1. **Gating: on by default in the `gtk` build (ghostty parity).** The `gtk` feature pulls in `rustty-vt/imgui` so `Key::imgui_key` compiles; the inspector is wired unconditionally. Rationale: the imgui stack already compiles in every build (gating saves nothing), macOS ships the inspector on-by-default (symmetry), ghostty has no such feature (fidelity), and per-file `#[cfg]` gating would add a second CI config and a fallback stub path for no benefit. Runtime cost when the inspector is never opened is zero (the GLArea + ImGui context only realize when the window is shown).
2. **Verification: KAT + live screenshot.** A CI-portable framebuffer→PPM KAT that drives a key event and needle-scans the keyboard-panel region for a change (not merely "non-blank"), **plus** a live native-Xorg screenshot showing populated panels. The inspector is the highest-risk-of-silent-render-failure feature in the port (new GPU backend + font atlas + input plumbing all compile clean but can render blank); belt-and-suspenders matches how every other GTK feature was landed.

## Architecture

Faithful port of ghostty's three-class GTK layering into idiomatic gtk4-rs (`glib::subclass`), plus the OpenGL3 backend in `dcimgui-sys`:

```
InspectorWindow (adw::ApplicationWindow, 1000x600)      class/inspector_window.rs
   └─ InspectorWidget : ImguiWidget                     class/inspector_widget.rs
        └─ ImguiWidget : adw::Bin                        class/imgui_widget.rs
             └─ adw::Bin → gtk4::GLArea                  (drives Dear ImGui via OpenGL3)
```

### Component 1 — `dcimgui-sys` OpenGL3 backend

Mirror of the existing Metal wiring. Vendor `imgui_impl_opengl3.cpp`, `imgui_impl_opengl3.h`, `imgui_impl_opengl3_loader.h` (matching the pinned imgui version) into `crates-sys/dcimgui-sys/vendor/imgui/backends/` and git-track them. In `build.rs`, add a non-macOS (Linux/GTK) branch that compiles `imgui_impl_opengl3.cpp` and defines `ZIGPKG_IMGUI_ENABLE_OPENGL3` for the `ext.cpp` TU — this auto-activates the already-staged `ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup` shim (`ext.cpp:44`). Expose the FFI in `src/lib.rs` (mirroring the macOS Metal block): `ImGui_ImplOpenGL3_Init(glsl_version: *const c_char) -> bool`, `_NewFrame()`, `_RenderDrawData(*mut ImDrawData)`, `_Shutdown()`, and `ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup()`.

### Component 2 — `ImguiWidget` (generic ImGui-over-GLArea host)

Port of `imgui_widget.zig` (551 L). `adw::Bin` wrapping a `gtk4::GLArea` (from the existing `imgui-widget.blp` template). Owns one ImGui context per widget. Behaviors, preserving exact ordering:

- **realize:** `make_current()` + error check → `ImGui_CreateContext(null)` → `SetCurrentContext` → `io.BackendPlatformName = "rustty_gtk"` → `ImGui_ImplOpenGL3_Init(null)` → virtual `setup()` → add a frame-clock tick callback.
- **unrealize:** remove tick callback → `make_current()` → `SetCurrentContext` → `ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup()` → `ImGui_DestroyContext`.
- **render** (context is guaranteed current by GTK; do NOT call `make_current`): loop twice { `ImGui_ImplOpenGL3_NewFrame`; `newFrame()` (compute `io.DeltaTime`); `ImGui_NewFrame`; virtual `render()`; `ImGui_Render` }; `glClearColor(#282C34)`; `glClear`; `ImGui_ImplOpenGL3_RenderDrawData(ImGui_GetDrawData())`.
- **resize:** `io.DisplaySize = (w,h)` unscaled; `io.DisplayFramebufferScale = (1,1)`; rebuild `ImGuiStyle` via the ext constructor + `ScaleAllSizes(scale_factor)`; assign to `ImGui_GetStyle()`. (Scaling via style, not framebuffer scale, for crisp fonts — match exactly or fonts blur.)
- **Input controllers** (all from the `.blp`): focus enter/leave → `AddFocusEvent`; key pressed/released → `keyEvent` (queue render; add LeftShift/Ctrl/Alt/Super mod events every keystroke; `Key::imgui_key` → `AddKeyEvent`; forward the GDK event to a `gtk4::IMMulticontext.filter_keypress`); GestureClick → `AddMouseButtonEvent` (button map 1/2/3→L/M/R); motion → `AddMousePosEvent(x*scale, y*scale)`; scroll → `AddMouseWheelEvent(x, -y)`; IM `commit` → `AddInputCharactersUTF8`.
- **Self-driving repaint:** frame-clock tick throttled to 30 FPS calls `queue_render`; input handlers also `queue_render`.
- **Virtuals `setup`/`render`:** ghostty hand-rolls a vtable (zig-gobject limitation). In gtk4-rs these become a real subclassable trait (e.g. an `ImguiWidgetImpl` with `imgui_setup`/`imgui_render`, dispatched through the subclass's parent-class chain) — the single biggest structural simplification vs the oracle.

### Component 3 — `InspectorWidget : ImguiWidget`

Port of `inspector_widget.zig` (245 L). Overrides `setup` → `rustty_inspector::Inspector::setup()`; `render` → fetch its `surface` → `core()` → `core.inspector` → `inspector.render(surface_data, wakeup)`. Holds the surface via a weak-notify (no strong ref). `set_surface(new)`: on old → `deactivateInspector()` + weak-unref; on new → `activateInspector()` + weak-ref + `queue_render`. `dispose` → `set_surface(null)`. The weak-notify nulls the surface and notifies the `surface` pspec so the window can self-destroy.

### Component 4 — `InspectorWindow`

Port of `inspector_window.zig` (218 L). `adw::ApplicationWindow`, 1000×600, title "Rustty: Terminal Inspector", from the `inspector-window.blp` template; hosts one `InspectorWidget` with `surface: bind template.surface`. Holds the surface via a `glib::WeakRef`. `notify::surface` on the embedded widget → when the surface goes away, destroy the window (auto-close when the terminal closes). `dispose` → `set_surface(null)` (clear weak refs — the oracle notes real Valgrind-found corruption otherwise).

### Component 5 — Wiring (un-stub the seams)

- `Key::imgui_key` (`rustty-vt/src/input/key.rs:792`) — replace `unimplemented!()` with the verbatim reference body preserved at `key.rs:1002`.
- `window.rs:1154` `toggle_inspector` — replace the `warn` with the real path.
- `surface.rs:1835` `control_inspector` — construct/present/destroy the `InspectorWindow` per `.toggle/.show/.hide` (ports `surface.zig:885-907`), tracking it on the surface's `inspector` weak backref.
- `gtk/surface.rs:259` `redraw_inspector` — forward to the window's `queue_render`.
- Register the new GObject types; add `inspector-widget` / `inspector-window` to the gresource build if not already emitted (`imgui-widget` is at gresource.rs:177).

### Component 6 — Verification

- **KAT** `crates/rustty-apprt/tests/gtk_inspector_dump.rs` (headed, `--ignored`, `RUSTTY_DUMP_PPM`): open the inspector on a live surface, drive `Inspector::render`, feed a key event, and needle-scan the keyboard-panel region of the framebuffer (readback via `renderer.api.read_bound_framebuffer_rgba` + `write_ppm`, reusing `gtk_glyph_dump.rs`) for a non-background change. Regression anchor.
- **Live screenshot** on the native-Xorg session: launch, Ctrl+Shift+I, `import` the inspector window; type input → keyboard panel populates; run output → termio VT-events panel populates. Confirm INV-12 (edits trigger re-render) and, if feasible, INV-3 (a scrolling-region edit takes effect).

## Out of scope

- Metal/embedded path (done). No changes to `Backend::Metal`.
- Any new inspector widget/panel functionality (the shared crate is faithful and frozen).
- Wayland-specific handling — the GLArea path is backend-agnostic; validated under both Wayland (XWayland GL) and native Xorg.
- The GTK built-in debugger (`app.show-gtk-inspector`) — unrelated.

## Verification of the design against the oracle

Every component cites its exact oracle file + line ranges (above). The port is R1-faithful (port every line, preserve comments as ported comments). The three `.blp` templates already exist and are tracked; only the Rust GObject classes behind them are new.
