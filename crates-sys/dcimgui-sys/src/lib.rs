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
// drive window docking through these three; the wider DockBuilder set the
// reference declares can be added as later widget tasks need it.
//
// `ImGuiID` is a `c_uint` and `ImGuiDir` a `c_int` (bindgen type aliases); the
// out-params cross as `*mut ImGuiID`.
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
}

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
#[cfg(target_os = "macos")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

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
    #[cfg(target_os = "macos")]
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
}
