//! dcimgui-sys — FFI bindings to the vendored Dear ImGui 1.92.5 (docking) +
//! Dear Bindings 0.17 C API, the reference's inspector UI toolkit
//! (`links = "dcimgui"`).
//!
//! PORT STATUS: vendored (Dear ImGui core + FreeType backend + Dear Bindings C
//! API; platform rendering backends deferred — see Cargo.toml/build.rs).
//!
//! R3: copies the SAME sources the reference uses (Dear ImGui 1.92.5 docking +
//! Dear Bindings 0.17) and builds them standalone (`cc` in C++17 mode +
//! `bindgen`; see `build.rs`). No pure-Rust substitution during parity phases.
//! The FFI is bindgen-generated from `wrapper.h` — the same public C header the
//! reference's `pkg/dcimgui/main.zig` @cIncludes (`dcimgui.h`).

// Force-link the vendored FreeType. Dear ImGui is built with
// `-DIMGUI_ENABLE_FREETYPE=1`, so `imgui_draw.cpp` references
// `ImGuiFreeType::GetFontLoader()` (in `misc/freetype/imgui_freetype.cpp`),
// which in turn references the FreeType API (`FT_Init_FreeType`, `FT_New_Face`,
// `FT_Load_Glyph`, …). Without an explicit reference to the `freetype-sys` rlib,
// rustc prunes it — and with it the `cargo:rustc-link-lib=static=freetype`
// directive — leaving those symbols undefined at link time. `extern crate`
// keeps the archive on the link line (mirrors the reference's
// `lib.linkLibrary(freetype_dep.artifact("freetype"))`). freetype-sys in turn
// keeps its bundled libpng + zlib on the line, so no separate libpng-sys dep is
// required (see Cargo.toml).
extern crate freetype_sys;

// The bindgen-generated module carries C naming conventions and machine-shaped
// code; suppress the whole surface (this is generated, not hand-authored — the
// clippy/rustc-lint gate applies to our code, not the generated bindings).
#[allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    dead_code,
    rustdoc::all
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;

// ---------------------------------------------------------------------------
// Dear Bindings extension shim FFI (`ext.cpp`).
//
// The reference (ghostty pkg/dcimgui/ext.cpp) ships a small C shim for
// functionality Dear Bindings does not yet expose — chiefly the default
// *constructors* for the value-type structs `ImFontConfig` and `ImGuiStyle`
// (dear_bindings issue #55). Dear ImGui's `ImFontConfig::ImFontConfig()` /
// `ImGuiStyle::ImGuiStyle()` set non-zero defaults (e.g. `OversampleH/V`,
// `RasterizerMultiply = 1.0`, `RasterizerDensity = 1.0`, `EllipsisChar`), so a
// zero-initialized struct is NOT a faithful substitute — the inspector's
// `Inspector.setup()` (Inspector.zig:44-45) and `updateContentScale`
// (embedded.zig:1121-1122) call these to obtain proper defaults before mutating
// a couple of fields.
//
// bindgen does not emit these: `wrapper.h` includes only the plain-C Dear
// Bindings surface (`dcimgui.h`), not `ext.cpp`'s C-ABI functions. But `ext.cpp`
// IS compiled into the `dcimgui` archive (build.rs) and exports both symbols
// with C linkage (`CIMGUI_API` == `extern "C"`), so — exactly as with the Metal
// backend block below — we declare them here directly. The `self` argument is a
// pointer to the corresponding bindgen value struct (ABI-identical to the C++
// type; ext.cpp `static_assert`s the size/alignment match).
extern "C" {
    /// Default-construct an `ImFontConfig` in place (Dear ImGui's
    /// `ImFontConfig::ImFontConfig()`), writing the proper non-zero defaults.
    /// (ext.cpp — `ImFontConfig_ImFontConfig(cimgui::ImFontConfig* self)`.)
    pub fn ImFontConfig_ImFontConfig(self_: *mut ImFontConfig);

    /// Default-construct an `ImGuiStyle` in place (Dear ImGui's
    /// `ImGuiStyle::ImGuiStyle()`), writing the proper non-zero defaults.
    /// (ext.cpp — `ImGuiStyle_ImGuiStyle(cimgui::ImGuiStyle* self)`.)
    pub fn ImGuiStyle_ImGuiStyle(self_: *mut ImGuiStyle);
}

// ---------------------------------------------------------------------------
// Dear ImGui internal DockBuilder API.
//
// The docking-branch DockBuilder functions live in `dcimgui_internal.h`, which
// the reference does NOT run through Dear Bindings (its bitfields cannot be
// C-translated), so bindgen — which parses only the public `dcimgui.h` surface —
// emits nothing for them. But they ARE compiled into the `dcimgui` archive
// (docking build) and exported with C linkage, so we declare them directly here,
// exactly mirroring the reference's own binding surface (ghostty
// pkg/dcimgui/main.zig:52-73, a manual `pub extern fn` block for the same
// reason). The inspector's `DetachableHeader`/`detachableHeader` (widgets.zig)
// drive window docking through the first three; `screen::Info.createDockSpace`
// (Inspector.rs) additionally needs `ImGui_DockBuilderGetNode` /
// `ImGui_DockBuilderAddNodeEx` to find-or-build the dockspace's root node. The
// wider DockBuilder set the reference declares can be added as later widget
// tasks need it.
//
// `ImGuiID` is a `c_uint` and `ImGuiDir` a `c_int` (bindgen type aliases); the
// out-params cross as `*mut ImGuiID`. `ImGuiDockNodeFlags` is `c_int` (a public
// Dear Bindings typedef, `dcimgui.h:298`) — bindgen already emits it as
// `pub type ImGuiDockNodeFlags = ::std::os::raw::c_int;`, so it is reused
// as-is (no local redeclaration needed).
//
// `surface::Inspector.createDockSpace` (widgets/surface.rs) additionally needs
// `ImGui_DockBuilderSetNodeSize` to size the root dockspace node to the main
// viewport before docking windows into it (ghostty pkg/dcimgui/main.zig does
// not need this one directly, but the vendored `dcimgui_internal.h` compiles
// and exports it exactly like the other five, so it is added here the same
// way — the "wider DockBuilder set... added as later widget tasks need it"
// extension the comment above already anticipated).
extern "C" {
    /// Split a dock node into two, returning the new node id and writing the
    /// two halves into the out-params. `split_dir` is an `ImGuiDir`,
    /// `size_ratio_for_node_at_dir` the fraction claimed by the `at_dir` half.
    /// (ghostty pkg/dcimgui/main.zig:71 — `ImGui_DockBuilderSplitNode`.)
    pub fn ImGui_DockBuilderSplitNode(
        node_id: ImGuiID,
        split_dir: ImGuiDir,
        size_ratio_for_node_at_dir: f32,
        out_id_at_dir: *mut ImGuiID,
        out_id_at_opposite_dir: *mut ImGuiID,
    ) -> ImGuiID;

    /// Dock the window named `window_name` into the node `node_id`.
    /// (ghostty pkg/dcimgui/main.zig:52 — `ImGui_DockBuilderDockWindow`.)
    pub fn ImGui_DockBuilderDockWindow(window_name: *const std::os::raw::c_char, node_id: ImGuiID);

    /// Finalize the dock layout built under `node_id`.
    /// (ghostty pkg/dcimgui/main.zig:73 — `ImGui_DockBuilderFinish`.)
    pub fn ImGui_DockBuilderFinish(node_id: ImGuiID);

    /// Look up the dock node with id `node_id`, or return null if it does not
    /// exist yet. `screen::Info.createDockSpace` uses the null-ness to decide
    /// whether the dockspace still needs to be built this run.
    /// (ghostty pkg/dcimgui/main.zig:53 — `ImGui_DockBuilderGetNode`, which
    /// the reference itself types as `?*anyopaque`; we return the opaque
    /// `ImGuiDockNode` pointer directly.)
    pub fn ImGui_DockBuilderGetNode(node_id: ImGuiID) -> *mut ImGuiDockNode;

    /// Create (or recreate) a dock node with the given id and flags, e.g. the
    /// private `ImGuiDockNodeFlagsPrivate::DockSpace` flag used to mark a node
    /// as a dockspace root. Returns the node's id.
    /// (ghostty pkg/dcimgui/main.zig:56 — `ImGui_DockBuilderAddNodeEx`.)
    pub fn ImGui_DockBuilderAddNodeEx(node_id: ImGuiID, flags: ImGuiDockNodeFlags) -> ImGuiID;

    /// Resize the dock node `node_id` to `size` (`dcimgui_internal.h:3653`).
    /// `surface::Inspector.createDockSpace` calls this once, on first setup, to
    /// size the root dockspace node to the main viewport's `Size` before
    /// docking the fixed window set into it.
    pub fn ImGui_DockBuilderSetNodeSize(node_id: ImGuiID, size: ImVec2);
}

/// Opaque handle for Dear ImGui's internal `ImGuiDockNode` (docking-branch,
/// `dcimgui_internal.h`). bindgen never sees its definition (it parses only
/// the public `dcimgui.h`, and `ImGuiDockNode` is internal-only with
/// bitfields Dear Bindings cannot translate), so — mirroring the reference's
/// own `?*anyopaque` typing (ghostty pkg/dcimgui/main.zig:53) — we model it as
/// a zero-sized opaque struct. It only ever crosses the FFI boundary as a raw
/// pointer (returned by `ImGui_DockBuilderGetNode`, null-checked, never
/// dereferenced on the Rust side).
#[repr(C)]
pub struct ImGuiDockNode {
    _unused: [u8; 0],
}

/// Private docking flag `ImGuiDockNodeFlagsPrivate_DockSpace` (Dear ImGui
/// `imgui_internal.h`'s `ImGuiDockNodeFlagsPrivate_` enum; not part of the
/// public Dear Bindings `dcimgui.h` surface, hence not bindgen-generated).
/// Marks a dock node as a dockspace root. Value mirrors the reference's own
/// manual redeclaration (ghostty pkg/dcimgui/main.zig:38 —
/// `ImGuiDockNodeFlagsPrivate.DockSpace = 1 << 10`), which in turn matches
/// Dear ImGui's `ImGuiDockNodeFlagsPrivate_DockSpace = 1 << 10` in
/// `dcimgui_internal.h`.
pub const IM_GUI_DOCK_NODE_FLAGS_PRIVATE_DOCK_SPACE: ImGuiDockNodeFlags = 1 << 10;

// ---------------------------------------------------------------------------
// Dear ImGui Metal rendering backend FFI (macOS).
//
// The inspector's `apprt.embedded.Inspector` drives Dear ImGui through the
// Metal backend (`vendor/imgui/backends/imgui_impl_metal.mm`, compiled by
// build.rs on macOS into the sibling `dcimgui_metal` archive). That backend's
// entry points are declared in `imgui_impl_metal.h` inside an `#ifdef __OBJC__`
// block using Objective-C `id<MTL...>` protocol types, so bindgen — which
// parses the plain-C Dear Bindings surface (`dcimgui.h`) with no Objective-C —
// never sees them and emits nothing for `ImGui_ImplMetal_*`.
//
// But the reference compiles the `.mm` with `IMGUI_IMPL_API = extern "C"` on
// non-Windows (see build.rs' Metal `cc::Build`), so every entry point is
// exported with plain C linkage, and Objective-C `id`/pointer arguments are
// ABI-transparent (an `id` is a pointer). We therefore declare the symbols
// directly here rather than via a C-ABI shim — exactly mirroring the
// reference's own binding surface (ghostty pkg/dcimgui/main.zig:23-27), which
// declares them as `pub extern fn ImGui_ImplMetal_*` with `*anyopaque` for the
// objc ids and `*c.ImDrawData` for the draw data.
//
// The objc handles cross as `*mut c_void`:
//   * `device`                     — `id<MTLDevice>`
//   * `render_pass_descriptor`     — `MTLRenderPassDescriptor*`
//   * `command_buffer`             — `id<MTLCommandBuffer>`
//   * `command_encoder`            — `id<MTLRenderCommandEncoder>`
// and the per-frame draw data (never stored — see PREPORT_INSPECTOR §1) crosses
// as `*mut ImDrawData`. These names are stable: the apprt-c
// `rustty_inspector_metal_{init,render,shutdown}` exports call them directly.
#[cfg(any(target_os = "macos", target_os = "ios"))] // isDarwin()
extern "C" {
    /// Initialize the Metal backend for the current ImGui context.
    /// `device` is an `id<MTLDevice>` objc handle. Returns false on failure.
    /// (imgui_impl_metal.h:32 — `ImGui_ImplMetal_Init(id<MTLDevice> device)`.)
    pub fn ImGui_ImplMetal_Init(device: *mut std::os::raw::c_void) -> bool;

    /// Tear down the Metal backend global state. Called from the inspector's
    /// `Backend.deinit` / `renderMetal` teardown.
    /// (imgui_impl_metal.h:33 — `ImGui_ImplMetal_Shutdown()`.)
    pub fn ImGui_ImplMetal_Shutdown();

    /// Begin a new Metal frame. `render_pass_descriptor` is a
    /// `MTLRenderPassDescriptor*`.
    /// (imgui_impl_metal.h:34 —
    /// `ImGui_ImplMetal_NewFrame(MTLRenderPassDescriptor*)`.)
    pub fn ImGui_ImplMetal_NewFrame(render_pass_descriptor: *mut std::os::raw::c_void);

    /// Encode the accumulated ImGui draw data into the given command buffer /
    /// render command encoder. `draw_data` is the per-frame `*ImDrawData`
    /// returned by `ImGui_GetDrawData()`; `command_buffer` is an
    /// `id<MTLCommandBuffer>` and `command_encoder` an
    /// `id<MTLRenderCommandEncoder>`.
    /// (imgui_impl_metal.h:35-37.)
    pub fn ImGui_ImplMetal_RenderDrawData(
        draw_data: *mut ImDrawData,
        command_buffer: *mut std::os::raw::c_void,
        command_encoder: *mut std::os::raw::c_void,
    );

    /// (Called by Init/NewFrame/Shutdown.) Create the backend's device
    /// objects for `device` (`id<MTLDevice>`). Returns false on failure.
    /// (imgui_impl_metal.h:40 — `ImGui_ImplMetal_CreateDeviceObjects`.)
    pub fn ImGui_ImplMetal_CreateDeviceObjects(device: *mut std::os::raw::c_void) -> bool;

    /// (Called by Init/NewFrame/Shutdown.) Destroy the backend's device
    /// objects. (imgui_impl_metal.h:41 —
    /// `ImGui_ImplMetal_DestroyDeviceObjects()`.)
    pub fn ImGui_ImplMetal_DestroyDeviceObjects();
}

// --- OpenGL3 backend (non-macOS / Linux+GTK) --------------------------------
//
// The GTK apprt renders the inspector with the OpenGL3 ImGui backend instead
// of Metal. These are the `ImGui_ImplOpenGL3_*` entry points from the vendored
// `imgui_impl_opengl3.cpp` (compiled by build.rs on non-macOS), mirroring the
// Metal block above. Like the Metal names, they are stable and are called
// directly by the GTK inspector's realize/render/unrealize glue.
//
// The per-frame draw data (never stored — see PREPORT_INSPECTOR §1) crosses as
// `*mut ImDrawData`, already in scope above.
#[cfg(not(any(target_os = "macos", target_os = "ios")))] // !isDarwin()
extern "C" {
    /// Initialize the OpenGL3 backend for the current ImGui context.
    /// `glsl_version` is a NUL-terminated `#version` string, or NULL to let the
    /// backend pick a default. Returns false on failure.
    /// (imgui_impl_opengl3.h — `ImGui_ImplOpenGL3_Init(const char* glsl_version)`.)
    pub fn ImGui_ImplOpenGL3_Init(glsl_version: *const core::ffi::c_char) -> bool;

    /// Tear down the OpenGL3 backend global state.
    /// (imgui_impl_opengl3.h — `ImGui_ImplOpenGL3_Shutdown()`.)
    pub fn ImGui_ImplOpenGL3_Shutdown();

    /// Begin a new OpenGL3 frame (lazily creates device objects).
    /// (imgui_impl_opengl3.h — `ImGui_ImplOpenGL3_NewFrame()`.)
    pub fn ImGui_ImplOpenGL3_NewFrame();

    /// Render the accumulated ImGui draw data with the current GL context.
    /// `draw_data` is the per-frame `*ImDrawData` returned by
    /// `ImGui_GetDrawData()`.
    /// (imgui_impl_opengl3.h — `ImGui_ImplOpenGL3_RenderDrawData(ImDrawData*)`.)
    pub fn ImGui_ImplOpenGL3_RenderDrawData(draw_data: *mut ImDrawData);

    /// Shut down the OpenGL3 backend and additionally unload the GL function
    /// loader (the `imgui_impl_opengl3_loader.h` dynamic loader). Used by the
    /// GTK inspector's `unrealize` path so the loader is re-resolved against a
    /// fresh GL context on the next realize. (ext.cpp:44 shim, auto-activated by
    /// `ZIGPKG_IMGUI_ENABLE_OPENGL3`.)
    pub fn ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use std::sync::Mutex;

    /// Dear ImGui keeps its "current context" in a plain (non-thread-local,
    /// non-atomic) global (`GImGui`). `cargo test`'s default parallel harness
    /// runs each `#[test]` fn on its own thread, so two tests concurrently
    /// calling `ImGui_CreateContext`/`ImGui_DestroyContext` race that global
    /// and corrupt its internal tables (observed: a `AddSettingsHandler`
    /// double-registration assert / abort). Every test below that touches a
    /// context takes this lock first, serializing them — same fix shape as
    /// e.g. `rustty-vt-c`'s `TEST_LOCK` / `rustty-os`'s `ENV_TEST_LOCK` for
    /// other non-thread-safe global C state.
    static IMGUI_CONTEXT_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// R3 known-answer test: prove the vendored Dear ImGui 1.92.5 + Dear
    /// Bindings 0.17 C API links and runs.
    ///
    /// Mirrors the reference's context lifecycle (pkg/dcimgui: create a
    /// context, access the `ImGuiIO` struct, mutate configuration, destroy the
    /// context) with no rendering backend. It:
    ///   * creates a context with a NULL shared font atlas
    ///     (`ImGui_CreateContext(NULL)` — Dear ImGui allocates a default shared
    ///     atlas internally) and asserts the handle is non-null;
    ///   * fetches the `ImGuiIO` structure of the (now-current) context
    ///     (`ImGui_GetIO()`) and asserts it is non-null;
    ///   * checks a fixed known answer: `io.DeltaTime` initializes to the Dear
    ///     ImGui default `1.0f/60.0f`;
    ///   * sets a field and reads it back (round-trip through the C ABI struct),
    ///     proving the bindgen struct layout matches the compiled archive;
    ///   * tears the context down (`ImGui_DestroyContext`).
    #[test]
    fn kat_dcimgui_1_92_5_context_lifecycle() {
        let _guard = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        unsafe {
            // Create a context (NULL => Dear ImGui builds a default shared font
            // atlas). Becomes the current context.
            let ctx = ImGui_CreateContext(ptr::null_mut());
            assert!(!ctx.is_null(), "ImGui_CreateContext(NULL) returned null");

            // Access the I/O configuration struct of the current context.
            let io = ImGui_GetIO();
            assert!(!io.is_null(), "ImGui_GetIO() returned null");

            // Known answer: DeltaTime defaults to 1/60 s (Dear ImGui's
            // ImGuiIO::ImGuiIO() initializer).
            assert_eq!(
                (*io).DeltaTime,
                1.0f32 / 60.0f32,
                "ImGuiIO::DeltaTime default"
            );

            // Set a field and read it back through the C ABI struct.
            (*io).DeltaTime = 0.25f32;
            assert_eq!((*io).DeltaTime, 0.25f32, "ImGuiIO::DeltaTime round-trip");

            // Tear down (frees the context and its default font atlas).
            ImGui_DestroyContext(ctx);
        }
    }

    /// Link test for the Dear ImGui Metal backend FFI surface
    /// (`ImGui_ImplMetal_*`). References each entry point so that a missing
    /// export from the compiled `dcimgui_metal` archive is a *link* error, not
    /// a silent runtime failure. We take each function's address (behind a
    /// `black_box` so the reference is not optimized away) rather than calling
    /// it — invoking the Metal backend would require a live `MTLDevice`, which
    /// is not available in a headless unit test. The exact set mirrors the
    /// inspector's calls (embedded.zig renderMetal / Backend.deinit +
    /// pkg/dcimgui/main.zig:23-27).
    #[cfg(any(target_os = "macos", target_os = "ios"))] // isDarwin()
    #[test]
    fn link_imgui_impl_metal_ffi() {
        // Coerce each `extern "C"` fn item to an opaque address; the linker
        // must resolve every symbol to build this test binary.
        let symbols: [*const (); 6] = [
            ImGui_ImplMetal_Init as *const (),
            ImGui_ImplMetal_Shutdown as *const (),
            ImGui_ImplMetal_NewFrame as *const (),
            ImGui_ImplMetal_RenderDrawData as *const (),
            ImGui_ImplMetal_CreateDeviceObjects as *const (),
            ImGui_ImplMetal_DestroyDeviceObjects as *const (),
        ];
        for sym in symbols {
            // black_box keeps the reference (and thus the link dependency)
            // from being elided; a resolved symbol is never null.
            assert!(!std::hint::black_box(sym).is_null());
        }
    }

    /// Link test for the `ImGui_ImplOpenGL3_*` FFI surface used by the GTK
    /// inspector's OpenGL3 backend — same shape as `link_imgui_impl_metal_ffi`
    /// above. Takes the address of each of the 5 entry points so a missing
    /// export from the compiled `imgui_impl_opengl3.cpp` TU is a *link* error
    /// at test-binary build time; no GL context is needed to force the link.
    #[cfg(not(any(target_os = "macos", target_os = "ios")))] // !isDarwin()
    #[test]
    fn opengl3_symbols_link() {
        // Coerce each `extern "C"` fn item to an opaque address; the linker
        // must resolve every symbol to build this test binary.
        let symbols: [*const (); 5] = [
            ImGui_ImplOpenGL3_Init as *const (),
            ImGui_ImplOpenGL3_Shutdown as *const (),
            ImGui_ImplOpenGL3_NewFrame as *const (),
            ImGui_ImplOpenGL3_RenderDrawData as *const (),
            ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup as *const (),
        ];
        for sym in symbols {
            // black_box keeps the reference (and thus the link dependency)
            // from being elided; a resolved symbol is never null.
            assert!(!std::hint::black_box(sym).is_null());
        }
    }

    /// Link test for the Dear ImGui internal DockBuilder FFI surface
    /// (`ImGui_DockBuilder*`), the same shape as `link_imgui_impl_metal_ffi`
    /// above: references each entry point so a missing export from the
    /// compiled `dcimgui` archive (docking build) is a *link* error, not a
    /// silent runtime failure. Includes `ImGui_DockBuilderGetNode` /
    /// `ImGui_DockBuilderAddNodeEx` (added for `screen::Info.createDockSpace`)
    /// and `ImGui_DockBuilderSetNodeSize` (added for
    /// `surface::Inspector.createDockSpace`) alongside the three already wired.
    #[test]
    fn link_imgui_dock_builder_ffi() {
        let symbols: [*const (); 6] = [
            ImGui_DockBuilderSplitNode as *const (),
            ImGui_DockBuilderDockWindow as *const (),
            ImGui_DockBuilderFinish as *const (),
            ImGui_DockBuilderGetNode as *const (),
            ImGui_DockBuilderAddNodeEx as *const (),
            ImGui_DockBuilderSetNodeSize as *const (),
        ];
        for sym in symbols {
            assert!(!std::hint::black_box(sym).is_null());
        }
    }

    /// Smoke test for `ImGui_DockBuilderSetNodeSize`: calls it on a node
    /// created by `AddNodeEx` and just confirms it doesn't crash/abort.
    /// `ImGuiDockNode` is opaque (no accessor exposes the size back out), so
    /// this cannot be a value round-trip KAT like the GetNode/AddNodeEx one
    /// above — it only proves the binding calls into the real symbol with the
    /// right ABI (an `ImVec2`-by-value struct arg is the one part of this
    /// binding shape not already exercised elsewhere).
    #[test]
    fn kat_dock_builder_set_node_size_smoke() {
        let _guard = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        unsafe {
            let ctx = ImGui_CreateContext(ptr::null_mut());
            assert!(!ctx.is_null(), "ImGui_CreateContext(NULL) returned null");

            let node_id: ImGuiID = 0xD0C5_0001;
            let created = ImGui_DockBuilderAddNodeEx(node_id, 0);
            assert_ne!(created, 0, "ImGui_DockBuilderAddNodeEx returned id 0");

            ImGui_DockBuilderSetNodeSize(created, ImVec2 { x: 800.0, y: 600.0 });

            ImGui_DestroyContext(ctx);
        }
    }

    /// Known-answer test for `ImGui_DockBuilderAddNodeEx` /
    /// `ImGui_DockBuilderGetNode`: proves a node created with `AddNodeEx`
    /// round-trips through `GetNode` as non-null (the null-check
    /// `screen::Info.createDockSpace` performs to decide whether the
    /// dockspace still needs to be built), and separately known-answers the
    /// private `DockSpace` flag's bit value.
    ///
    /// Deliberately passes `flags = 0` (a plain floating node), NOT the
    /// `DockSpace` flag: per Dear ImGui's own `DockBuilderAddNode` (imgui.cpp)
    /// the `ImGuiDockNodeFlags_DockSpace` bit routes through `ImGui::DockSpace()`
    /// — the widget call — which requires an active `NewFrame`/window scope.
    /// Every real call site (`screen::Info.createDockSpace`, widgets.rs'
    /// `DetachableHeader::window`) makes these calls from inside such a scope;
    /// a headless unit test has none, so exercising the flag here would
    /// segfault on a precondition the binding correctly forwards rather than
    /// violates.
    #[test]
    fn kat_dock_builder_add_node_ex_get_node_round_trip() {
        let _guard = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert_eq!(IM_GUI_DOCK_NODE_FLAGS_PRIVATE_DOCK_SPACE, 1 << 10);
        unsafe {
            let ctx = ImGui_CreateContext(ptr::null_mut());
            assert!(!ctx.is_null(), "ImGui_CreateContext(NULL) returned null");

            let node_id: ImGuiID = 0xD0C5_0000;
            let created = ImGui_DockBuilderAddNodeEx(node_id, 0);
            assert_ne!(created, 0, "ImGui_DockBuilderAddNodeEx returned id 0");

            let node = ImGui_DockBuilderGetNode(created);
            assert!(
                !node.is_null(),
                "ImGui_DockBuilderGetNode(created) returned null after AddNodeEx"
            );

            // A node id that was never added must look up as null.
            let missing = ImGui_DockBuilderGetNode(node_id.wrapping_add(0x1234));
            assert!(
                missing.is_null(),
                "ImGui_DockBuilderGetNode(<unknown id>) should return null"
            );

            ImGui_DestroyContext(ctx);
        }
    }
}
