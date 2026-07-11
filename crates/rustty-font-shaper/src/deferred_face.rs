//! A deferred face represents a single font face with all the information
//! necessary to load it, but defers loading the full face until it is
//! needed.
//!
//! This allows us to have many fallback fonts to look for glyphs, but
//! only load them if they're really needed.
// const DeferredFace = @This();

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const fontconfig = @import("fontconfig");
// const macos = @import("macos");
// const font = @import("main.zig");
// const options = @import("main.zig").options;
// const Library = @import("main.zig").Library;
// const Face = @import("main.zig").Face;
// const Presentation = @import("main.zig").Presentation;
//
// PORT NOTE (imports + backend scope): the reference `DeferredFace` carries four
// mutually-exclusive, comptime-conditional backend payloads — `fc` (Fontconfig),
// `ct` (CoreText), `win` (Windows FreeType scan), `wc` (WebCanvas) — of which
// exactly one is non-void per `options.backend`. rustty's parity targets are the
// two defaults (macOS `coretext`, Linux `fontconfig_freetype`); this port
// implements the CoreText payload (macOS), the active one for the gates. The
// Fontconfig/Windows/WebCanvas payloads (and the `familyName`/`name`/`load`/
// `hasCodepoint` arms that read them) are ported as verbatim reference comments +
// DEFERRED notes and land with those backends. `macos.text.*`/`macos.graphics.*`/
// `macos.foundation.*` map to the objc2 framework crates (as in the coretext
// face). `fontconfig` (Linux discovery) is DEFERRED per the batch scope.

// const log = std.log.scoped(.deferred_face);

/// The DeferredFace itself. On the macOS coretext backend the only live payload
/// is `ct`.
// PORT NOTE: the reference is a file-struct with the four conditional fields:
//   fc: if (backend == .fontconfig_freetype) ?Fontconfig else void = ...,
//   ct: if (Discover == discovery.CoreText) ?CoreText else void = ...,
//   win: if (backend == .freetype_windows) ?Windows else void = ...,
//   wc: if (backend == .web_canvas) ?WebCanvas else void = ...,
// Only the `ct` field is materialized here (macOS). The others are void on this
// backend and are omitted.
pub struct DeferredFace {
    /// CoreText
    // ct: if (font.Discover == font.discovery.CoreText) ?CoreText else void = ...,
    #[cfg(target_os = "macos")]
    pub ct: Option<CoreTextData>,

    /// Non-macOS placeholder (Fontconfig/Windows/WebCanvas payloads are DEFERRED).
    #[cfg(not(target_os = "macos"))]
    pub _deferred: (),
}

// ---------------------------------------------------------------------------
// PORT NOTE (DEFERRED backend payloads): the Fontconfig / Windows / WebCanvas structs
// are recorded verbatim as reference comments; their ports land with those
// backends.
//
// /// Fontconfig specific data. This is only present if building with fontconfig.
// pub const Fontconfig = struct {
//     /// The pattern for this font. This must be the "render prepared" pattern.
//     /// (i.e. call FcFontRenderPrepare).
//     pattern: *fontconfig.Pattern,
//
//     /// Charset and Langset are used for quick lookup if a codepoint and
//     /// presentation style are supported. They can be derived from pattern
//     /// but are cached since they're frequently used.
//     charset: *const fontconfig.CharSet,
//     langset: *const fontconfig.LangSet,
//
//     /// Variations to apply to this font.
//     variations: []const font.face.Variation,
//
//     pub fn deinit(self: *Fontconfig) void { self.pattern.destroy(); self.* = undefined; }
// };
//
// /// Windows specific data. Only present with the freetype_windows backend.
// ///
// /// Unlike Fontconfig/CoreText which carry lightweight descriptor handles,
// /// the Windows backend has no external descriptor service — the "deferred"
// /// metadata is the FreeType face itself. We keep a pre-loaded face (loaded
// /// at discovery time) to answer `hasCodepoint` cheaply without re-opening
// /// the file on every query, and remember the path so `load()` can open a
// /// fresh face at the caller's requested size/options.
// pub const Windows = struct {
//     /// Path to the font file. Owned here.
//     path: [:0]const u8,
//
//     /// Face index within the file (for .ttc collections).
//     face_index: i32,
//
//     /// Variations to apply on load.
//     variations: []const font.face.Variation,
//
//     /// Pre-loaded face used for cheap metadata queries (glyphIndex,
//     /// hasColor). The size it was opened at is irrelevant for these
//     /// queries since the CMap is size-independent. Deinit'd with us.
//     peek: Face,
//
//     /// Whether the face presents as emoji (has color glyphs) or text.
//     presentation: Presentation,
//
//     /// Allocator that owns `path`.
//     alloc: Allocator,
//
//     pub fn deinit(self: *Windows) void { self.peek.deinit(); self.alloc.free(self.path); self.* = undefined; }
// };
//
// /// WebCanvas specific data. This is only present when building with canvas.
// pub const WebCanvas = struct {
//     /// The allocator to use for fonts
//     alloc: Allocator,
//
//     /// The string to use for the "font" attribute for the canvas
//     font_str: [:0]const u8,
//
//     /// The presentation for this font.
//     presentation: Presentation,
//
//     pub fn deinit(self: *WebCanvas) void { self.alloc.free(self.font_str); self.* = undefined; }
// };
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub use coretext_payload::{new_cfstring, new_cfstring_bytes, CoreTextData};

#[cfg(target_os = "macos")]
mod coretext_payload {
    use super::DeferredFace;
    use crate::face::{Face, FaceError, Options, Variation};
    use crate::library::Library;
    use crate::Presentation;
    use core::ptr::NonNull;
    use objc2_core_foundation::{CFRetained, CFString, CFStringBuiltInEncodings};
    use objc2_core_graphics::CGGlyph;
    use objc2_core_text::{CTFont, CTFontSymbolicTraits};

    /// CoreText specific data. This is only present when building with CoreText.
    // pub const CoreText = struct { font: *macos.text.Font, variations: []const Variation };
    pub struct CoreTextData {
        /// The initialized font
        // PORT NOTE: `*macos.text.Font` is a manually retained/released `CTFont`;
        // modeled as `CFRetained<CTFont>`, whose `Drop` performs the release the
        // reference's `deinit` does. `variations` is owned here (`Vec`) rather than
        // borrowed — the reference borrows into the search descriptor, but for the
        // discovered/fallback faces the variation set is transient/empty, so owning
        // it is behavior-preserving and avoids a self-referential borrow.
        pub font: CFRetained<CTFont>,

        /// Variations to apply to this font. We apply the variations to the
        /// search descriptor but sometimes when the font collection is
        /// made the variation axes are reset so we have to reapply them.
        pub variations: Vec<Variation>,
    }

    impl CoreTextData {
        // pub fn deinit(self: *CoreText) void { self.font.release(); self.* = undefined; }
        //
        // PORT NOTE: RAII — `font` (`CFRetained<CTFont>`) releases on `Drop`,
        // reproducing the reference's manual `release`. No-op here.
        pub fn deinit(&mut self) {}
    }

    /// UTF-32 -> UTF-16 surrogate decomposition (see the coretext face for the
    /// verbatim `CFStringGetSurrogatePairForLongCharacter` body this reproduces).
    fn surrogate_pair(character: u32, surrogates: &mut [u16; 2]) -> bool {
        if (character > 0xFFFF) && (character < 0x110000) {
            let character = character - 0x10000;
            surrogates[0] = ((character >> 10) + 0xD800) as u16;
            surrogates[1] = ((character & 0x3FF) + 0xDC00) as u16;
            true
        } else {
            surrogates[0] = character as u16;
            false
        }
    }

    /// Create a UTF-8 `CFString`.
    pub fn new_cfstring(s: &str) -> Option<CFRetained<CFString>> {
        unsafe {
            CFString::with_bytes(
                None,
                s.as_ptr(),
                s.len() as isize,
                CFStringBuiltInEncodings::EncodingUTF8.0,
                false,
            )
        }
    }

    /// Create a UTF-8 `CFString` from raw bytes.
    // PORT NOTE: `macos.foundation.String.createWithBytes(bytes, .utf8, false)` — the
    // `discovery.Descriptor` family/style/codepoint strings are byte slices
    // (`[:0]const u8`), not necessarily valid `str`.
    pub fn new_cfstring_bytes(bytes: &[u8]) -> Option<CFRetained<CFString>> {
        unsafe {
            CFString::with_bytes(
                None,
                bytes.as_ptr(),
                bytes.len() as isize,
                CFStringBuiltInEncodings::EncodingUTF8.0,
                false,
            )
        }
    }

    impl DeferredFace {
        // pub fn deinit(self: *DeferredFace) void { switch (backend) { ... ct.deinit() ... } self.* = undefined; }
        pub fn deinit(&mut self) {
            // .coretext, .coretext_freetype, .coretext_harfbuzz, .coretext_noshape
            // => if (self.ct) |*ct| ct.deinit(),
            if let Some(ct) = self.ct.as_mut() {
                ct.deinit();
            }
            // self.* = undefined; — RAII (the `Option<CoreTextData>` drops).
        }

        /// A `DeferredFace` with no payload. Used only where the reference passes
        /// `undefined` (the `add deferred without loading options` test), where the
        /// payload is guaranteed never to be accessed.
        pub fn undefined() -> DeferredFace {
            DeferredFace { ct: None }
        }

        /// Returns the family name of the font.
        // pub fn familyName(self: DeferredFace, buf: []u8) ![]const u8 {
        pub fn family_name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            // .coretext* => if (self.ct) |ct| {
            //     const family_name = ct.font.copyAttribute(.family_name) orelse return "unknown";
            //     return family_name.cstringPtr(.utf8) orelse family_name.cstring(buf, .utf8) ...;
            // },
            if let Some(ct) = self.ct.as_ref() {
                let family_name = unsafe { ct.font.family_name() };
                let s = family_name.to_string();
                let bytes = s.as_bytes();
                if bytes.len() > buf.len() {
                    return Err(oom());
                }
                buf[..bytes.len()].copy_from_slice(bytes);
                return Ok(&buf[..bytes.len()]);
            }

            // return "";
            Ok(&[])
        }

        /// Returns the name of this face. The memory is always owned by the
        /// face so it doesn't have to be freed.
        // pub fn name(self: DeferredFace, buf: []u8) ![]const u8 {
        pub fn name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            // .coretext* => if (self.ct) |ct| {
            //     const display_name = ct.font.copyDisplayName();
            //     return display_name.cstringPtr(.utf8) orelse
            //         // "NULL if the internal storage of theString does not allow
            //         // this to be returned efficiently." In this case, we need
            //         // to allocate. But we can't return an allocated string because
            //         // we don't have an allocator. Let's use the stack and log it.
            //         display_name.cstring(buf, .utf8) orelse return error.OutOfMemory;
            // },
            if let Some(ct) = self.ct.as_ref() {
                let display_name = unsafe { ct.font.display_name() };
                let s = display_name.to_string();
                let bytes = s.as_bytes();
                if bytes.len() > buf.len() {
                    return Err(oom());
                }
                buf[..bytes.len()].copy_from_slice(bytes);
                return Ok(&buf[..bytes.len()]);
            }

            // return "";
            Ok(&[])
        }

        /// Load the deferred font face. This does nothing if the face is loaded.
        // pub fn load(self: *DeferredFace, lib: Library, opts: font.face.Options) !Face {
        pub fn load(&mut self, lib: Library, opts: Options) -> Result<Face, FaceError> {
            // return switch (options.backend) {
            //     .fontconfig_freetype => try self.loadFontconfig(lib, opts),
            //     .freetype_windows => try self.loadWindows(lib, opts),
            //     .coretext, .coretext_harfbuzz, .coretext_noshape => try self.loadCoreText(lib, opts),
            //     .coretext_freetype => try self.loadCoreTextFreetype(lib, opts),
            //     .web_canvas => try self.loadWebCanvas(opts),
            //
            //     // Unreachable because we must be already loaded or have the
            //     // proper configuration for one of the other deferred mechanisms.
            //     .freetype => unreachable,
            // };
            self.load_coretext(lib, opts)
        }

        // fn loadCoreText(self, lib, opts) !Face {
        fn load_coretext(&mut self, _lib: Library, opts: Options) -> Result<Face, FaceError> {
            // _ = lib;
            let ct = self.ct.as_ref().unwrap();
            let mut face = Face::init_font_copy(&ct.font, opts)?;
            // errdefer face.deinit();
            face.set_variations(&ct.variations, opts)?;
            Ok(face)
        }

        /// Returns true if this face can satisfy the given codepoint and
        /// presentation. If presentation is null, then it just checks if the
        /// codepoint is present at all.
        ///
        /// This should not require the face to be loaded IF we're using a
        /// discovery mechanism (i.e. fontconfig). If no discovery is used,
        /// the face is always expected to be loaded.
        // pub fn hasCodepoint(self: DeferredFace, cp: u32, p: ?Presentation) bool {
        pub fn has_codepoint(&self, cp: u32, p: Option<Presentation>) -> bool {
            // .coretext* => {
            //     // If we are using coretext, we check the loaded CT font.
            //     if (self.ct) |ct| { ... }
            // },
            if let Some(ct) = self.ct.as_ref() {
                // This presentation check isn't as detailed as isColorGlyph
                // because forced presentation modes are only used for emoji and
                // emoji should always have color glyphs set. This can be
                // more correct by using the isColorGlyph logic but I'd want
                // to find a font that actually requires this so we can write
                // a test for it before changing it.
                if let Some(desired_p) = p {
                    let traits = unsafe { ct.font.symbolic_traits() };
                    let actual_p: Presentation =
                        if traits.0 & CTFontSymbolicTraits::TraitColorGlyphs.0 != 0 {
                            Presentation::Emoji
                        } else {
                            Presentation::Text
                        };
                    if actual_p != desired_p {
                        return false;
                    }
                }

                // Turn UTF-32 into UTF-16 for CT API
                let mut unichars: [u16; 2] = [0, 0];
                let pair = surrogate_pair(cp, &mut unichars);
                let len: usize = if pair { 2 } else { 1 };

                // Get our glyphs
                let mut glyphs: [CGGlyph; 2] = [0, 0];
                return unsafe {
                    ct.font.glyphs_for_characters(
                        NonNull::new(unichars.as_mut_ptr()).unwrap(),
                        NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                        len as isize,
                    )
                };
            }

            // This is unreachable because discovery mechanisms terminate, and
            // if we're not using a discovery mechanism, the face MUST be loaded.
            // unreachable;
            unreachable!()
        }
    }

    /// Build a backend-face `OutOfMemory` error (the reference `error.OutOfMemory`
    /// returned by `name`/`familyName` when the name doesn't fit the buffer).
    fn oom() -> FaceError {
        crate::face::coretext::Error::OutOfMemory
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE (DEFERRED): the non-CoreText arms of `familyName`/`name`/`load`/
// `hasCodepoint` and the wasm `Wasm` C-ABI struct are recorded verbatim and land
// with their backends:
//
//   .freetype_windows => if (self.win) |w| return try w.peek.name(buf),
//   .fontconfig_freetype => if (self.fc) |fc| return (try fc.pattern.get(.fullname, 0)).string,
//
// fn loadFontconfig(self, lib, opts) !Face {
//     const fc = self.fc.?;
//     // Filename and index for our face so we can load it
//     const filename = (try fc.pattern.get(.file, 0)).string;
//     const face_index = (try fc.pattern.get(.index, 0)).integer;
//     var face = try Face.initFile(lib, filename, face_index, opts);
//     errdefer face.deinit();
//     try face.setVariations(fc.variations, opts);
//     return face;
// }
//
// fn loadWindows(self, lib, opts) !Face {
//     const w = self.win.?;
//     var face = try Face.initFile(lib, w.path, w.face_index, opts);
//     errdefer face.deinit();
//     try face.setVariations(w.variations, opts);
//     return face;
// }
//
// fn loadCoreTextFreetype(self, lib, opts) !Face {
//     const ct = self.ct.?;
//     // Get the URL for the font so we can get the filepath
//     const url = ct.font.copyAttribute(.url) orelse return error.FontHasNoFile;
//     defer url.release();
//     // Get the path from the URL
//     const path = url.copyPath() orelse return error.FontHasNoFile;
//     defer path.release();
//     // URL decode the path
//     const blank = try macos.foundation.String.createWithBytes("", .utf8, false);
//     defer blank.release();
//     const decoded = try macos.foundation.URL.createStringByReplacingPercentEscapes(path, blank);
//     defer decoded.release();
//     // Decode into a c string. 1024 bytes should be enough for anybody.
//     var buf: [1024]u8 = undefined;
//     const path_slice = decoded.cstring(buf[0..1023], .utf8) orelse return error.FontPathCantDecode;
//     // Freetype requires null-terminated. We always leave space at
//     // the end for a zero so we set that up here.
//     buf[path_slice.len] = 0;
//     // Face index 0 is not always correct. We don't ship this configuration
//     // in a release build. Users should use the pure CoreText builds.
//     //std.log.warn("path={s}", .{path_slice});
//     var face = try Face.initFile(lib, buf[0..path_slice.len :0], 0, opts);
//     errdefer face.deinit();
//     try face.setVariations(ct.variations, opts);
//     return face;
// }
//
// fn loadWebCanvas(self, opts) !Face {
//     const wc = self.wc.?;
//     return try .initNamed(wc.alloc, wc.font_str, opts, wc.presentation);
// }
//
// hasCodepoint (fontconfig arm):
//   .fontconfig_freetype => {
//       // If we are using fontconfig, use the fontconfig metadata to
//       // avoid loading the face.
//       if (self.fc) |fc| {
//           // Check if char exists
//           if (!fc.charset.hasChar(cp)) return false;
//           // If we have a presentation, check it matches
//           if (p) |desired| {
//               const emoji_lang = "und-zsye";
//               const actual: Presentation = if (fc.langset.hasLang(emoji_lang)) .emoji else .text;
//               return desired == actual;
//           }
//           return true;
//       }
//   },
//
// hasCodepoint (freetype_windows arm):
//   .freetype_windows => {
//       // Use the pre-loaded peek face for a cheap CMap lookup.
//       if (self.win) |w| {
//           if (p) |desired| if (w.presentation != desired) return false;
//           return w.peek.glyphIndex(cp) != null;
//       }
//   },
//
// hasCodepoint (web_canvas arm):
//   // Canvas always has the codepoint because we have no way of
//   // really checking and we let the browser handle it.
//   .web_canvas => if (self.wc) |wc| {
//       // Fast-path if we have a specific presentation and we
//       // don't match, then it is definitely not this face.
//       if (p) |desired| if (wc.presentation != desired) return false;
//       // Slow-path: we initialize the font, render it, and check
//       // if it works and the presentation matches.
//       var face = Face.initNamed(wc.alloc, wc.font_str, .{ .points = 12 }, wc.presentation) catch |err| {
//           log.warn("failed to init face for codepoint check face={s} err={}", .{ wc.font_str, err });
//           return false;
//       };
//       defer face.deinit();
//       return face.glyphIndex(cp) != null;
//   },
//
//   pub const Wasm = struct { export fn deferred_face_new/free/load ... };
// ---------------------------------------------------------------------------

// Non-macOS DeferredFace stub methods (payloads DEFERRED); present so higher
// tiers type-check on Linux until the Fontconfig backend lands.
#[cfg(not(target_os = "macos"))]
impl DeferredFace {
    pub fn undefined() -> DeferredFace {
        DeferredFace { _deferred: () }
    }
    pub fn deinit(&mut self) {}
    pub fn has_codepoint(&self, _cp: u32, _p: Option<crate::Presentation>) -> bool {
        unimplemented!("Fontconfig DeferredFace is DEFERRED (Linux backend)")
    }
    pub fn name<'a>(&self, _buf: &'a mut [u8]) -> Result<&'a [u8], crate::face::FaceError> {
        unimplemented!("Fontconfig DeferredFace is DEFERRED (Linux backend)")
    }
    pub fn load(
        &mut self,
        _lib: crate::library::Library,
        _opts: crate::face::Options,
    ) -> Result<crate::face::Face, crate::face::FaceError> {
        unimplemented!("Fontconfig DeferredFace is DEFERRED (Linux backend)")
    }
}

#[cfg(test)]
mod tests {
    // test "fontconfig"
    #[test]
    fn fontconfig() {
        // if (options.backend != .fontconfig_freetype) return error.SkipZigTest;
        //
        // PORT NOTE: on the macOS/coretext gate this test always skips (its body
        // exercises the Fontconfig discoverer, which is DEFERRED). The full body
        // lands with the Fontconfig backend. The `!= fontconfig_freetype` skip is
        // expressed as an inverted `== fontconfig_freetype` guard (clippy-clean).
        if crate::options::BACKEND == crate::Backend::FontconfigFreetype {
            // (body DEFERRED with the Fontconfig backend)
        }
    }

    // test "coretext"
    #[cfg(target_os = "macos")]
    #[test]
    fn coretext() {
        // if (options.backend != .coretext) return error.SkipZigTest;
        if crate::options::BACKEND != crate::Backend::Coretext {
            return;
        }

        use crate::discovery;
        use crate::face::DesiredSize;
        use crate::library::Library;

        // Load freetype
        let mut lib = Library::init().unwrap();

        // Get a deferred face from fontconfig
        let mut def = {
            let fc = discovery::CoreText::init(lib);
            let mut it = fc
                .discover(discovery::Descriptor {
                    family: Some(b"Monaco".to_vec()),
                    size: 12.0,
                    ..Default::default()
                })
                .unwrap();
            // defer it.deinit();
            let d = it.next().unwrap().unwrap();
            it.deinit();
            d
        };

        assert!(def.has_codepoint(u32::from(' '), None));

        // Verify we can get the name
        let mut buf = [0u8; 1024];
        let n = def.name(&mut buf).unwrap();
        assert!(!n.is_empty());

        // Load it and verify it works
        let mut face = def
            .load(
                lib,
                crate::face::Options {
                    size: DesiredSize {
                        points: 12.0,
                        xdpi: crate::face::DEFAULT_DPI,
                        ydpi: crate::face::DEFAULT_DPI,
                    },
                    freetype_load_flags: crate::face::FREETYPE_LOAD_FLAGS_DEFAULT,
                },
            )
            .unwrap();
        assert!(face.glyph_index(u32::from(' ')).is_some());

        face.deinit();
        def.deinit();
        lib.deinit();
    }
}
