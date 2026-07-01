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
}
