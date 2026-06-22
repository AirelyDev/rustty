//! Font backend selection.
//!
//! BREAK #3: the `Backend` enum is DEFINED HERE (ported from the reference
//! `src/font/backend.zig`) so that `rustty-font` imports it from build-config
//! rather than the reverse (R4). The reference file has no `test {}` block.

use crate::os::{Target, WasmTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub const fn default(target: Target, wasm_target: WasmTarget) -> Backend {
        if target.arch.is_wasm() {
            return match wasm_target {
                WasmTarget::Browser => Backend::WebCanvas,
            };
        }

        if matches!(target.os, crate::os::Os::Windows) {
            // Avoid fontconfig on Windows because its libxml2 dependency
            // may not unpack due to symlinks. Use the FreeType-based
            // Windows font-directory scanner for discovery. A future
            // DirectWrite backend can replace this if needed.
            return Backend::FreetypeWindows;
        }

        // macOS also supports "coretext_freetype" but there is no scenario
        // that is the default. It is only used by people who want to
        // self-compile Ghostty and prefer the freetype aesthetic.
        if target.os.is_darwin() {
            Backend::Coretext
        } else {
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
