//! PORT: ghostty `src/font/backend.zig` — the `Backend` enum that names the
//! font discovery/rendering/shaping stack in use, plus the comptime/runtime
//! `has*` dependency predicates and the `default` selector.

// const std = @import("std");

/// The font backend in use.
// pub const Backend = enum {
//     const WasmTarget = @import("../os/wasm/target.zig").Target;
//     ...
// };
//
// PORT NOTE: the nested `WasmTarget` alias is only used by `default` below; the
// wasm target enum isn't carved into this tier yet, so `default`'s wasm arm is
// ported as a comment (see `default`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// FreeType for font rendering with no font discovery enabled.
    Freetype,

    /// FreeType for font rendering with a built-in Windows font directory
    /// scanner (C:\Windows\Fonts + %LOCALAPPDATA%\Microsoft\Windows\Fonts).
    /// Used when DirectWrite is not available; matches by family_name and
    /// SFNT name table without any external index.
    FreetypeWindows,

    /// Fontconfig for font discovery and FreeType for font rendering.
    FontconfigFreetype,

    /// CoreText for font discovery, rendering, and shaping (macOS).
    Coretext,

    /// CoreText for font discovery, FreeType for rendering, and
    /// HarfBuzz for shaping (macOS).
    CoretextFreetype,

    /// CoreText for font discovery and rendering, HarfBuzz for shaping
    CoretextHarfbuzz,

    /// CoreText for font discovery and rendering, no shaping.
    CoretextNoshape,

    /// Use the browser font system and the Canvas API (wasm). This limits
    /// the available fonts to browser fonts (anything Canvas natively
    /// supports).
    WebCanvas,
}

impl Backend {
    /// Returns the default backend for a build environment. This is
    /// meant to be called at comptime by the build.zig script. To get the
    /// backend look at build_options.
    // pub fn default(target: std.Target, wasm_target: WasmTarget) Backend { ... }
    //
    // PORT NOTE: the reference inspects a `std.Target`; rustty resolves the same
    // decision tree from `cfg` (there is no runtime `std.Target` here). The wasm
    // arm (`.browser => .web_canvas`) and Windows arm (`.freetype_windows`) are
    // preserved; the final line's Darwin/other split matches `cfg(target_os)`.
    pub const fn default() -> Backend {
        // if (target.cpu.arch == .wasm32) {
        //     return switch (wasm_target) { .browser => .web_canvas };
        // }
        //
        // if (target.os.tag == .windows) {
        //     // Avoid fontconfig on Windows because its libxml2 dependency
        //     // may not unpack due to symlinks. Use the FreeType-based
        //     // Windows font-directory scanner for discovery. A future
        //     // DirectWrite backend can replace this if needed.
        //     return .freetype_windows;
        // }
        //
        // // macOS also supports "coretext_freetype" but there is no scenario
        // // that is the default. It is only used by people who want to
        // // self-compile Ghostty and prefer the freetype aesthetic.
        // return if (target.os.tag.isDarwin()) .coretext else .fontconfig_freetype;
        #[cfg(target_os = "windows")]
        {
            Backend::FreetypeWindows
        }
        #[cfg(target_os = "macos")]
        {
            Backend::Coretext
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Backend::FontconfigFreetype
        }
    }

    // All the functions below can be called at comptime or runtime to
    // determine if we have a certain dependency.

    pub fn has_freetype(self) -> bool {
        match self {
            Backend::Freetype
            | Backend::FreetypeWindows
            | Backend::FontconfigFreetype
            | Backend::CoretextFreetype => true,

            Backend::Coretext
            | Backend::CoretextHarfbuzz
            | Backend::CoretextNoshape
            | Backend::WebCanvas => false,
        }
    }

    pub fn has_coretext(self) -> bool {
        match self {
            Backend::Coretext
            | Backend::CoretextFreetype
            | Backend::CoretextHarfbuzz
            | Backend::CoretextNoshape => true,

            Backend::Freetype
            | Backend::FreetypeWindows
            | Backend::FontconfigFreetype
            | Backend::WebCanvas => false,
        }
    }

    pub fn has_fontconfig(self) -> bool {
        match self {
            Backend::FontconfigFreetype => true,

            Backend::Freetype
            | Backend::FreetypeWindows
            | Backend::Coretext
            | Backend::CoretextFreetype
            | Backend::CoretextHarfbuzz
            | Backend::CoretextNoshape
            | Backend::WebCanvas => false,
        }
    }

    pub fn has_harfbuzz(self) -> bool {
        match self {
            Backend::Freetype
            | Backend::FreetypeWindows
            | Backend::FontconfigFreetype
            | Backend::CoretextFreetype
            | Backend::CoretextHarfbuzz => true,

            Backend::Coretext | Backend::CoretextNoshape | Backend::WebCanvas => false,
        }
    }
}
