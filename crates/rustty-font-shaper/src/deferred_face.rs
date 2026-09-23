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
// exactly one is non-void per `options.backend`. ALL FOUR are ported here, each
// behind the `cfg` that models its `options.backend` (matching `Backend::default`):
// `coretext_payload` (macOS `coretext`), `fontconfig_payload` (Linux
// `fontconfig_freetype`), `windows_payload` (Windows `freetype_windows`, WS20), and
// `web_canvas_payload` (wasm32 `web_canvas`).
// `macos.text.*`/`macos.graphics.*`/`macos.foundation.*` map to the objc2
// framework crates (as in the coretext face).

// const log = std.log.scoped(.deferred_face);
//
// The reference's `std.log.scoped(.deferred_face)` tag, carried through the
// `log` facade's `target:` (`.port/parity-closure/DECISION-logging.md` §6).
//
// NOTE (task-14c-4): no LEDGER row named this file. Its three reference
// `log.warn` sites were found by enumerating the reference's own calls per file
// (PROCEDURE.md blind spot 3) rather than by trusting the rows' cited anchors.
//
// `#[allow(dead_code)]`: both of this file's live sites are inside
// `cfg(target_arch = "wasm32")` modules, so on a native build the const has no
// reader. Keeping it unconditional (rather than cfg-gating it) means the wasm
// and native trees read identically.
#[allow(dead_code)]
const LOG_SCOPE: &str = "deferred_face";

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

    /// Fontconfig (Linux default `fontconfig_freetype`).
    // fc: if (backend == .fontconfig_freetype) ?Fontconfig else void = ...,
    //
    // PORT NOTE: this is the freetype-active (`not(any(macos, wasm32))`) payload,
    // mirroring the macOS `ct` shape. `FontconfigData` is the port of the
    // reference `DeferredFace.Fontconfig` struct (`DeferredFace.zig:38-56`).
    #[cfg(all(
        not(target_os = "macos"),
        not(target_os = "windows"),
        not(target_arch = "wasm32")
    ))]
    pub fc: Option<FontconfigData>,

    /// Windows (FreeType directory scan).
    // win: if (backend == .freetype_windows) ?Windows else void = ...,
    //
    // PORT NOTE: this is the `freetype_windows` payload, mirroring the macOS `ct` /
    // Linux `fc` shape. `WindowsData` is the port of the reference
    // `DeferredFace.Windows` struct (`DeferredFace.zig:58-90`).
    #[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
    pub win: Option<WindowsData>,

    /// Canvas (wasm `web_canvas` backend).
    // wc: if (backend == .web_canvas) ?WebCanvas else void = ...,
    //
    // PORT NOTE: this is the wasm32 (`web_canvas`-active) payload, mirroring the
    // macOS `ct` / Linux `fc` shape. `WebCanvasData` is the port of the reference
    // `DeferredFace.WebCanvas` struct (`DeferredFace.zig:106-124`).
    #[cfg(target_arch = "wasm32")]
    pub wc: Option<WebCanvasData>,
}

// ---------------------------------------------------------------------------
// PORT NOTE (backend payloads): the Fontconfig / Windows / WebCanvas structs are
// recorded verbatim as reference comments here, and each is ported in the module
// named after it: `fontconfig_payload` (Linux), `windows_payload` (Windows, WS20),
// `web_canvas_payload` (wasm32).
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

    /// Raw CoreText entry point that objc2 0.3 wraps as non-null (it can
    /// legitimately return NULL, which the reference handles with `orelse`).
    // PORT NOTE (d166c05ed): the reference `pkg/macos/text/font.zig` wrapper is
    //     pub fn copyDisplayName(self: *Font) ?*foundation.String {
    //         return @ptrFromInt(@intFromPtr(c.CTFontCopyDisplayName(@ptrCast(self))));
    //     }
    // — nullable. objc2's `CTFont::display_name` marks the result non-null and
    // panics on NULL, so the raw symbol is declared here against the SAME
    // framework objc2 links (R3 — a binding, not a reimplementation; the
    // `discovery.rs` `CTFontCreateWithName` precedent).
    mod sys {
        use super::*;
        #[link(name = "CoreText", kind = "framework")]
        extern "C-unwind" {
            pub fn CTFontCopyDisplayName(font: &CTFont) -> *mut CFString;
        }
    }

    /// `ct.font.copyAttribute(.family_name)` — `None` when CoreText has no
    /// family name (`CTFontCopyAttribute(font, kCTFontFamilyNameAttribute)`,
    /// the reference's `pkg/macos` `copyAttribute`, which is nullable).
    fn ct_font_copy_family_name(font: &CTFont) -> Option<CFRetained<CFString>> {
        // SAFETY: `font` is a live retained CTFont and the key is CoreText's own
        // constant; the Copy rule gives the returned attribute +1, which the
        // `CFRetained` owns.
        let attr = unsafe { font.attribute(objc2_core_text::kCTFontFamilyNameAttribute) }?;
        attr.downcast::<CFString>().ok()
    }

    /// `ct.font.copyDisplayName()` — `None` when CoreText has no display name.
    fn ct_font_copy_display_name(font: &CTFont) -> Option<CFRetained<CFString>> {
        // SAFETY: `font` is a live retained CTFont.
        let raw = unsafe { sys::CTFontCopyDisplayName(font) };
        // SAFETY: CTFontCopyDisplayName follows the "Copy rule" (+1 reference);
        // `CFRetained::from_raw` takes that ownership (releases on drop).
        NonNull::new(raw).map(|p| unsafe { CFRetained::from_raw(p) })
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
                // PORT NOTE (S85-F2): the reference's `copyAttribute(.family_name)`
                // is nullable and a NULL takes `orelse return "unknown"`. objc2's
                // `CTFont::family_name` (`CTFontCopyFamilyName`) asserts non-null
                // and PANICKED instead; CoreText returns NULL for a font file with
                // no `name` table (measured, macOS 26.5.1). Use the same nullable
                // attribute copy the reference uses.
                let Some(family_name) = ct_font_copy_family_name(&ct.font) else {
                    return Ok(b"unknown");
                };
                // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD110.
                // The reference never releases this +1 copy (nor the display
                // name's in `name`, nor `Face.name`'s), on the `cstringPtr`
                // return or the `cstring(buf)` one: each call leaks a retain on
                // the name string (measured on a font loaded from data; a system
                // font's names are immortal). `CFRetained` releases it at scope
                // end; the bytes are copied into `buf` first, so nothing borrows
                // the string (LEDGER S85-F6 / S90-F3: the port therefore has no
                // zero-copy return for a name longer than `buf`).
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
            //     const display_name = ct.font.copyDisplayName() orelse
            //         return self.familyName(buf);
            //     return display_name.cstringPtr(.utf8) orelse unsupported: {
            //         // "NULL if the internal storage of theString does not allow
            //         // this to be returned efficiently." In this case, we need
            //         // to allocate. But we can't return an allocated string because
            //         // we don't have an allocator. Let's use the stack and log it.
            //         break :unsupported display_name.cstring(buf, .utf8) orelse
            //             return error.OutOfMemory;
            //     };
            // },
            if let Some(ct) = self.ct.as_ref() {
                // `CTFontCopyDisplayName` can return NULL (the reference `pkg/macos`
                // wrapper is `copyDisplayName(self: *Font) ?*foundation.String`);
                // fall back to the family name when it does (d166c05ed).
                // PORT NOTE: reference defect (R13), UD110 — the copy is
                // released at scope end (see `family_name`).
                let Some(display_name) = ct_font_copy_display_name(&ct.font) else {
                    // orelse return self.familyName(buf);
                    return self.family_name(buf);
                };
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
// `hasCodepoint` are recorded verbatim and land with their backends (the wasm
// `Wasm` C-ABI struct HAS landed — the `ghostty_wasm_abi` module below):
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
// The wasm `Wasm` C-ABI struct (`deferred_face_new`/`free`/`load`) is PORTED —
// see the `ghostty_wasm_abi` module below (feature `ghostty-wasm-abi`).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Fontconfig (Linux default `fontconfig_freetype`) DeferredFace payload.
//
// PORT NOTE: this materializes the reference's `fc` payload
// (`DeferredFace.zig:38-56`) and the `hasCodepoint` fontconfig arm
// (`:325-347`), which the discovery iterator (`discovery.rs`
// `DiscoverIterator::next`) constructs. The `familyName`/`name`/`load`
// (`loadFontconfig`) fontconfig arms are DEFERRED to L3 Task 7 and remain honest
// `unimplemented!()` leaves (matching Task 1's convention); they are not reached
// by any current caller (discovery does not name them).
#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "windows"),
    not(target_arch = "wasm32")
))]
pub use fontconfig_payload::FontconfigData;

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "windows"),
    not(target_arch = "wasm32")
))]
mod fontconfig_payload {
    use super::DeferredFace;
    use crate::face::{Face, FaceError, Options, Variation};
    use crate::fc;
    use crate::library::Library;
    use crate::Presentation;

    /// Fontconfig specific data. This is only present if building with fontconfig.
    // pub const Fontconfig = struct { ... };  (`DeferredFace.zig:38-56`)
    pub struct FontconfigData {
        /// The pattern for this font. This must be the "render prepared" pattern
        /// (i.e. from `FcFontRenderPrepare`). Owned here; destroyed in `deinit`.
        // pattern: *fontconfig.Pattern,
        pub pattern: *mut fc::Pattern,

        /// Charset and Langset are used for quick lookup if a codepoint and
        /// presentation style are supported. They can be derived from pattern
        /// but are cached since they're frequently used.
        // charset: *const fontconfig.CharSet,
        pub charset: *const fc::CharSet,
        // langset: *const fontconfig.LangSet,
        pub langset: *const fc::LangSet,

        /// Variations to apply to this font.
        // variations: []const font.face.Variation,
        pub variations: Vec<Variation>,
    }

    impl FontconfigData {
        // pub fn deinit(self: *Fontconfig) void { self.pattern.destroy(); self.* = undefined; }
        pub fn deinit(&mut self) {
            // SAFETY: `pattern` is the render-prepared pattern this payload owns
            // (from `FcFontRenderPrepare`); non-null and not yet destroyed.
            unsafe { (*self.pattern).destroy() };
            // self.* = undefined; — RAII (the raw handles are consumed by destroy).
        }
    }

    /// Build a backend-face `OutOfMemory` error (the reference `error.OutOfMemory`
    /// returned by `name`/`familyName` when the name can't be produced). Mirrors the
    /// CoreText payload's `oom()`.
    fn oom() -> FaceError {
        crate::face::freetype::Error::OutOfMemory
    }

    /// Copy `bytes` into `buf` and return the written sub-slice, or `OutOfMemory` if
    /// it doesn't fit. The reference returns a slice into pattern-owned memory
    /// directly; to unify lifetimes in Rust the bytes are copied into `buf` and a
    /// sub-slice of it is returned (identical bytes), exactly as the CoreText arm.
    fn copy_into<'a>(bytes: &[u8], buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
        if bytes.len() > buf.len() {
            return Err(oom());
        }
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(&buf[..bytes.len()])
    }

    impl DeferredFace {
        // pub fn deinit(self: *DeferredFace) void { ... if (self.fc) |*fc| fc.deinit() ... }
        pub fn deinit(&mut self) {
            // .fontconfig_freetype => if (self.fc) |*fc| fc.deinit(),
            if let Some(fc) = self.fc.as_mut() {
                fc.deinit();
            }
            // self.* = undefined; — RAII (the `Option<FontconfigData>` drops).
        }

        /// A `DeferredFace` with no payload. Used only where the reference passes
        /// `undefined` (the `add deferred without loading options` test), where the
        /// payload is guaranteed never to be accessed.
        pub fn undefined() -> DeferredFace {
            DeferredFace { fc: None }
        }

        /// Returns the family name of the font.
        // pub fn familyName(self: DeferredFace, buf: []u8) ![]const u8 {
        //   .fontconfig_freetype => if (self.fc) |fc|
        //       return (try fc.pattern.get(.family, 0)).string,
        //   ...
        //   return "";
        // }
        pub fn family_name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            if let Some(fc) = self.fc.as_ref() {
                // SAFETY: `pattern` is the render-prepared pattern this payload owns
                // (from `FcFontRenderPrepare`); non-null and not yet destroyed.
                let pattern = unsafe { &*fc.pattern };
                // return (try fc.pattern.get(.family, 0)).string,
                let name = match pattern.get(fc::Property::Family, 0) {
                    Ok(fc::Value::String(s)) => s,
                    // See the ERROR-ATOM note on `name` below.
                    _ => return Err(oom()),
                };
                return copy_into(name.to_bytes(), buf);
            }
            // return "";
            Ok(&[])
        }

        /// Returns the name of this face. The memory is always owned by the
        /// face so it doesn't have to be freed.
        // pub fn name(self: DeferredFace, buf: []u8) ![]const u8 {
        //   .fontconfig_freetype => if (self.fc) |fc|
        //       return (try fc.pattern.get(.fullname, 0)).string,
        //   ...
        //   return "";
        // }
        //
        // ERROR-ATOM NOTE (R1, the two-error-atoms gotcha): the reference's `try`
        // propagates the package-level `fontconfig.Error` (see `fc.rs`'s ERROR-ATOM
        // NOTE) out of `name`, whose inferred error set unions it with the backend
        // `Face` errors. This port pins `name`'s error to `FaceError` (= the backend
        // `freetype::Error`) so the signature is byte-identical to the macOS/CoreText
        // arm the callers bind against; `FaceError` carries no fontconfig atom, so a
        // failed/mistyped `get` is mapped to `FaceError::OutOfMemory` — the SAME
        // failure atom the CoreText `name` arm returns when it cannot materialize the
        // name (`DeferredFace.zig:193-194`). This is a non-panicking error return.
        //
        // CORRECTION (parity S86-F1): an earlier version of this note said "the
        // callers ... discard it, as the reference does". That is true of ONE
        // caller and not the other. `CodepointResolver.zig:203-206` does
        // `deferred_face.name(&buf) catch "<error>"` — discarded — and
        // `codepoint_resolver.rs:355-361` matches it. But
        // `SharedGridSet.zig:199-202` writes `try face.name(&name_buf)` inside
        // the `log.info` argument tuple, which Zig evaluates eagerly, so a name
        // error aborts `collection()` and therefore `ref()`: no grid is created.
        // Both `shared_grid_set.rs` sites propagate with `?` accordingly.
        //
        // A render-prepared pattern always carries `FC_FAMILY`; `FC_FULLNAME` is
        // present for effectively all real fonts, so this path is not reached in
        // practice.
        pub fn name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            if let Some(fc) = self.fc.as_ref() {
                // SAFETY: see `family_name`.
                let pattern = unsafe { &*fc.pattern };
                // return (try fc.pattern.get(.fullname, 0)).string,
                let name = match pattern.get(fc::Property::Fullname, 0) {
                    Ok(fc::Value::String(s)) => s,
                    _ => return Err(oom()),
                };
                return copy_into(name.to_bytes(), buf);
            }
            // return "";
            Ok(&[])
        }

        /// Load the deferred font face. This does nothing if the face is loaded.
        // fn loadFontconfig(self: *DeferredFace, lib: Library, opts: font.face.Options) !Face {
        //     const fc = self.fc.?;
        //     const filename = (try fc.pattern.get(.file, 0)).string;
        //     const face_index = (try fc.pattern.get(.index, 0)).integer;
        //     var face = try Face.initFile(lib, filename, face_index, opts);
        //     errdefer face.deinit();
        //     try face.setVariations(fc.variations, opts);
        //     return face;
        // }
        //
        // (See the ERROR-ATOM NOTE on `name`: `FC_FILE`/`FC_INDEX` are always present
        // on a render-prepared pattern, so the two `get`s never fail in practice; a
        // failure is mapped to `FaceError::OutOfMemory` rather than propagating the
        // fontconfig atom, which `FaceError` cannot represent.)
        pub fn load(&mut self, lib: Library, opts: Options) -> Result<Face, FaceError> {
            // const fc = self.fc.?;
            let fc = self.fc.as_ref().unwrap();
            // SAFETY: see `family_name`.
            let pattern = unsafe { &*fc.pattern };

            // Filename and index for our face so we can load it
            // const filename = (try fc.pattern.get(.file, 0)).string;
            let filename = match pattern.get(fc::Property::File, 0) {
                Ok(fc::Value::String(s)) => s,
                _ => return Err(oom()),
            };
            // const face_index = (try fc.pattern.get(.index, 0)).integer;
            let face_index = match pattern.get(fc::Property::Index, 0) {
                Ok(fc::Value::Integer(i)) => i,
                _ => return Err(oom()),
            };

            // var face = try Face.initFile(lib, filename, face_index, opts);
            let mut face = Face::init_file(lib, filename, face_index, opts)?;
            // errdefer face.deinit();
            // try face.setVariations(fc.variations, opts);
            match face.set_variations(&fc.variations, opts) {
                Ok(()) => Ok(face),
                Err(e) => {
                    face.deinit();
                    Err(e)
                }
            }
        }

        /// Returns true if this face can satisfy the given codepoint and
        /// presentation. If presentation is null, then it just checks if the
        /// codepoint is present at all.
        // pub fn hasCodepoint(self: DeferredFace, cp: u32, p: ?Presentation) bool {
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
        pub fn has_codepoint(&self, cp: u32, p: Option<Presentation>) -> bool {
            if let Some(fc) = self.fc.as_ref() {
                // Check if char exists
                // SAFETY: `charset` borrows the pattern-owned char set, valid for
                // the lifetime of `fc.pattern` (destroyed only in `deinit`).
                if !unsafe { (*fc.charset).has_char(cp) } {
                    return false;
                }

                // If we have a presentation, check it matches
                if let Some(desired) = p {
                    let emoji_lang = c"und-zsye";
                    // SAFETY: `langset` borrows the pattern-owned lang set, valid
                    // for the lifetime of `fc.pattern`.
                    let actual = if unsafe { (*fc.langset).has_lang(emoji_lang) } {
                        Presentation::Emoji
                    } else {
                        Presentation::Text
                    };
                    return desired == actual;
                }

                return true;
            }

            // This is unreachable because discovery mechanisms terminate, and
            // if we're not using a discovery mechanism, the face MUST be loaded.
            // unreachable;
            unreachable!()
        }
    }
}

// ---------------------------------------------------------------------------
// The Windows payload (`options.backend == .freetype_windows`). This is the live
// payload on `target_os = "windows"`, where `face::Face` IS
// `face::freetype::Face` and `discovery::Discover` IS `discovery::Windows`.
// Ported by WS20 (2026-07-30); it was previously the one payload left as a
// verbatim reference comment.
// ---------------------------------------------------------------------------

#[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
pub use windows_payload::WindowsData;

#[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
mod windows_payload {
    use super::DeferredFace;
    use crate::face::{Face, FaceError, Options, Variation};
    use crate::library::Library;
    use crate::Presentation;

    /// Windows specific data. Only present with the freetype_windows backend.
    ///
    /// Unlike Fontconfig/CoreText which carry lightweight descriptor handles,
    /// the Windows backend has no external descriptor service — the "deferred"
    /// metadata is the FreeType face itself. We keep a pre-loaded face (loaded
    /// at discovery time) to answer `hasCodepoint` cheaply without re-opening
    /// the file on every query, and remember the path so `load()` can open a
    /// fresh face at the caller's requested size/options.
    // pub const Windows = struct { ... };  (`DeferredFace.zig:58-90`)
    pub struct WindowsData {
        /// Path to the font file. Owned here.
        // path: [:0]const u8,
        //
        // PORT NOTE (byte-slice fidelity): a Windows font path is NOT guaranteed to
        // be valid UTF-8 (`OsStr` on Windows is WTF-8), and FreeType takes a
        // NUL-terminated byte path, so this is a `CString` — never a lossy `String`.
        pub path: std::ffi::CString,

        /// Face index within the file (for .ttc collections).
        // face_index: i32,
        pub face_index: i32,

        /// Variations to apply on load.
        // variations: []const font.face.Variation,
        pub variations: Vec<Variation>,

        /// Pre-loaded face used for cheap metadata queries (glyphIndex,
        /// hasColor). The size it was opened at is irrelevant for these
        /// queries since the CMap is size-independent. Deinit'd with us.
        // peek: Face,
        pub peek: Face,

        /// Whether the face presents as emoji (has color glyphs) or text.
        // presentation: Presentation,
        pub presentation: Presentation,
        // alloc: Allocator,
        //
        // PORT NOTE: the reference's `alloc` field exists only so `deinit` can free
        // the `path` it allocated. `path` is an owning `CString` here, so it frees
        // itself and the field has no counterpart (the same drop the CoreText
        // payload's `CFRetained` makes redundant).
    }

    impl WindowsData {
        // pub fn deinit(self: *Windows) void {
        //     self.peek.deinit();
        //     self.alloc.free(self.path);
        //     self.* = undefined;
        // }
        pub fn deinit(&mut self) {
            self.peek.deinit();
            // self.alloc.free(self.path); — RAII (the `CString` drops).
            // self.* = undefined; — RAII.
        }
    }

    impl DeferredFace {
        // pub fn deinit(self: *DeferredFace) void { ... if (self.win) |*w| w.deinit() ... }
        pub fn deinit(&mut self) {
            // .freetype_windows => if (self.win) |*w| w.deinit(),
            if let Some(w) = self.win.as_mut() {
                w.deinit();
            }
            // self.* = undefined; — RAII (the `Option<WindowsData>` drops).
        }

        /// A `DeferredFace` with no payload. Used only where the reference passes
        /// `undefined` (the `add deferred without loading options` test), where the
        /// payload is guaranteed never to be accessed.
        pub fn undefined() -> DeferredFace {
            DeferredFace { win: None }
        }

        /// Returns the family name of the font.
        // pub fn familyName(self: DeferredFace, buf: []u8) ![]const u8 {
        //   .freetype_windows => if (self.win) |w| return try w.peek.name(buf),
        //   ...
        //   return "";
        // }
        pub fn family_name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            if let Some(w) = self.win.as_ref() {
                return w.peek.name(buf);
            }
            // return "";
            Ok(&[])
        }

        /// Returns the name of this face. The memory is always owned by the
        /// face so it doesn't have to be freed.
        // pub fn name(self: DeferredFace, buf: []u8) ![]const u8 {
        //   .freetype_windows => if (self.win) |w| return try w.peek.name(buf),
        //   ...
        //   return "";
        // }
        //
        // (The reference's Windows arm is the SAME `peek.name(buf)` call as
        // `familyName`'s — FreeType exposes no separate "full name" here.)
        pub fn name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            if let Some(w) = self.win.as_ref() {
                return w.peek.name(buf);
            }
            // return "";
            Ok(&[])
        }

        /// Load the deferred font face. This does nothing if the face is loaded.
        // fn loadWindows(self: *DeferredFace, lib: Library, opts: font.face.Options) !Face {
        //     const w = self.win.?;
        //     var face = try Face.initFile(lib, w.path, w.face_index, opts);
        //     errdefer face.deinit();
        //     try face.setVariations(w.variations, opts);
        //     return face;
        // }
        pub fn load(&mut self, lib: Library, opts: Options) -> Result<Face, FaceError> {
            // const w = self.win.?;
            let w = self.win.as_ref().unwrap();

            // var face = try Face.initFile(lib, w.path, w.face_index, opts);
            let mut face = Face::init_file(lib, &w.path, w.face_index, opts)?;
            // errdefer face.deinit();
            // try face.setVariations(w.variations, opts);
            match face.set_variations(&w.variations, opts) {
                Ok(()) => Ok(face),
                Err(e) => {
                    face.deinit();
                    Err(e)
                }
            }
        }

        /// Returns true if this face can satisfy the given codepoint and
        /// presentation. If presentation is null, then it just checks if the
        /// codepoint is present at all.
        // pub fn hasCodepoint(self: DeferredFace, cp: u32, p: ?Presentation) bool {
        //   .freetype_windows => {
        //       // Use the pre-loaded peek face for a cheap CMap lookup.
        //       if (self.win) |w| {
        //           if (p) |desired| if (w.presentation != desired) return false;
        //           return w.peek.glyphIndex(cp) != null;
        //       }
        //   },
        pub fn has_codepoint(&self, cp: u32, p: Option<Presentation>) -> bool {
            // Use the pre-loaded peek face for a cheap CMap lookup.
            if let Some(w) = self.win.as_ref() {
                if let Some(desired) = p {
                    if w.presentation != desired {
                        return false;
                    }
                }
                return w.peek.glyph_index(cp).is_some();
            }

            // This is unreachable because discovery mechanisms terminate, and
            // if we're not using a discovery mechanism, the face MUST be loaded.
            // unreachable;
            unreachable!()
        }
    }
}

// ---------------------------------------------------------------------------
// The WebCanvas payload (`options.backend == .web_canvas`, wasm32). This is the
// live payload for the browser-freestanding deliverable (`rustty-web-canvas`),
// where `face::Face` IS `face::web_canvas::Face`.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub use web_canvas_payload::WebCanvasData;

#[cfg(target_arch = "wasm32")]
mod web_canvas_payload {
    use super::DeferredFace;
    use crate::face::{DesiredSize, Face, FaceError, Options, DEFAULT_DPI};
    use crate::library::Library;
    use crate::Presentation;

    /// WebCanvas specific data. This is only present when building with canvas.
    // pub const WebCanvas = struct {
    //     /// The allocator to use for fonts
    //     alloc: Allocator,
    //     /// The string to use for the "font" attribute for the canvas
    //     font_str: [:0]const u8,
    //     /// The presentation for this font.
    //     presentation: Presentation,
    // };
    //
    // PORT NOTE: the reference's `alloc` field is the allocator that owns
    // `font_str`; rustty's `Vec<u8>` owns its bytes (the global allocator is used
    // throughout this crate, exactly as `face::web_canvas::Face` documents), so no
    // allocator handle is threaded on the payload. `[:0]const u8` → `Vec<u8>`
    // (byte-slice fidelity; the CSS "font" string is bytes, never a lossy
    // `String`).
    pub struct WebCanvasData {
        /// The string to use for the "font" attribute for the canvas.
        // font_str: [:0]const u8,
        pub font_str: Vec<u8>,

        /// The presentation for this font.
        // presentation: Presentation,
        pub presentation: Presentation,
    }

    impl WebCanvasData {
        // pub fn deinit(self: *WebCanvas) void { self.alloc.free(self.font_str); self.* = undefined; }
        pub fn deinit(&mut self) {
            // self.alloc.free(self.font_str); — RAII: the `Vec<u8>` owns its bytes.
            self.font_str.clear();
            // self.* = undefined;
        }
    }

    /// Copy `bytes` into `buf`, mirroring the reference's "the memory is always
    /// owned by the face" contract for the returned slice. Same helper shape as
    /// the fontconfig payload's `copy_into`.
    fn copy_into<'a>(bytes: &[u8], buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
        if bytes.len() > buf.len() {
            return Err(FaceError::OutOfMemory);
        }
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(&buf[..bytes.len()])
    }

    impl DeferredFace {
        // pub fn deinit(self: *DeferredFace) void { ... .web_canvas => if (self.wc) |*wc| wc.deinit(), ... }
        pub fn deinit(&mut self) {
            // .web_canvas => if (self.wc) |*wc| wc.deinit(),
            if let Some(wc) = self.wc.as_mut() {
                wc.deinit();
            }
            // self.* = undefined; — RAII (the `Option<WebCanvasData>` drops).
        }

        /// A `DeferredFace` with no payload. Used only where the reference passes
        /// `undefined` (the `add deferred without loading options` test), where the
        /// payload is guaranteed never to be accessed.
        pub fn undefined() -> DeferredFace {
            DeferredFace { wc: None }
        }

        /// Returns the family name of the font.
        // pub fn familyName(self: DeferredFace, buf: []u8) ![]const u8 {
        //   ...
        //   .web_canvas => if (self.wc) |wc| return wc.font_str,
        //   }
        //   return "";
        // }
        pub fn family_name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            // .web_canvas => if (self.wc) |wc| return wc.font_str,
            if let Some(wc) = self.wc.as_ref() {
                return copy_into(&wc.font_str, buf);
            }
            // return "";
            Ok(&[])
        }

        /// Returns the name of this face. The memory is always owned by the
        /// face so it doesn't have to be freed.
        // pub fn name(self: DeferredFace, buf: []u8) ![]const u8 {
        //   ...
        //   .web_canvas => if (self.wc) |wc| return wc.font_str,
        //   }
        //   return "";
        // }
        pub fn name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            // .web_canvas => if (self.wc) |wc| return wc.font_str,
            if let Some(wc) = self.wc.as_ref() {
                return copy_into(&wc.font_str, buf);
            }
            // return "";
            Ok(&[])
        }

        /// Load the deferred font face. This does nothing if the face is loaded.
        // pub fn load(self: *DeferredFace, lib: Library, opts: font.face.Options) !Face {
        //     return switch (options.backend) { ... .web_canvas => try self.loadWebCanvas(opts), ... };
        // }
        //
        // fn loadWebCanvas(self: *DeferredFace, opts: font.face.Options) !Face {
        //     const wc = self.wc.?;
        //     return try .initNamed(wc.alloc, wc.font_str, opts, wc.presentation);
        // }
        //
        // PORT NOTE: `lib` is unused on this backend (Canvas has no FreeType
        // library handle) — the reference's `loadWebCanvas` likewise takes only
        // `opts`. `initNamed` takes the DESIRED SIZE, which `Options` carries.
        pub fn load(&mut self, _lib: Library, opts: Options) -> Result<Face, FaceError> {
            // const wc = self.wc.?;
            let wc = self.wc.as_ref().unwrap();
            // return try .initNamed(wc.alloc, wc.font_str, opts, wc.presentation);
            Face::init_named(&wc.font_str, opts.size, wc.presentation)
        }

        /// Returns true if this face can satisfy the given codepoint and
        /// presentation. If presentation is null, then it just checks if the
        /// codepoint is present at all.
        // pub fn hasCodepoint(self: DeferredFace, cp: u32, p: ?Presentation) bool {
        //   // Canvas always has the codepoint because we have no way of
        //   // really checking and we let the browser handle it.
        //   .web_canvas => if (self.wc) |wc| {
        //       // Fast-path if we have a specific presentation and we
        //       // don't match, then it is definitely not this face.
        //       if (p) |desired| if (wc.presentation != desired) return false;
        //
        //       // Slow-path: we initialize the font, render it, and check
        //       // if it works and the presentation matches.
        //       var face = Face.initNamed(wc.alloc, wc.font_str, .{ .points = 12 }, wc.presentation) catch |err| {
        //           log.warn("failed to init face for codepoint check face={s} err={}", .{ wc.font_str, err });
        //           return false;
        //       };
        //       defer face.deinit();
        //       return face.glyphIndex(cp) != null;
        //   },
        pub fn has_codepoint(&self, cp: u32, p: Option<Presentation>) -> bool {
            if let Some(wc) = self.wc.as_ref() {
                // Fast-path if we have a specific presentation and we
                // don't match, then it is definitely not this face.
                if let Some(desired) = p {
                    if wc.presentation != desired {
                        return false;
                    }
                }

                // Slow-path: we initialize the font, render it, and check
                // if it works and the presentation matches.
                //
                // PORT NOTE: `.{ .points = 12 }` relies on the reference
                // `DesiredSize` field defaults for `xdpi`/`ydpi` (`default_dpi`);
                // Rust has no field defaults on a literal, so they are written out.
                let mut face = match Face::init_named(
                    &wc.font_str,
                    DesiredSize {
                        points: 12.0,
                        xdpi: DEFAULT_DPI,
                        ydpi: DEFAULT_DPI,
                    },
                    wc.presentation,
                ) {
                    Ok(face) => face,
                    // log.warn("failed to init face for codepoint check " ++
                    //     "face={s} err={}", .{ wc.font_str, err });
                    //
                    // PORT NOTE: `{s}` renders the raw CSS font-string bytes;
                    // the lossy UTF-8 conversion is confined to the diagnostic.
                    Err(err) => {
                        log::warn!(
                            target: crate::deferred_face::LOG_SCOPE,
                            "failed to init face for codepoint check face={} err={:?}",
                            String::from_utf8_lossy(&wc.font_str),
                            err
                        );
                        return false;
                    }
                };
                // defer face.deinit();
                // return face.glyphIndex(cp) != null;
                let found = face.glyph_index(cp).is_some();
                face.deinit();
                return found;
            }

            // This is unreachable because discovery mechanisms terminate, and
            // if we're not using a discovery mechanism, the face MUST be loaded.
            // unreachable;
            unreachable!()
        }
    }
}

// ---------------------------------------------------------------------------
// The wasm-compatible API (`pub const Wasm = struct { … }`,
// `DeferredFace.zig:423-466`) — the UNPREFIXED Ghostty-ABI exports retained by
// `font/main.zig`'s `comptime { _ = DeferredFace.Wasm; }`. Feature-gated
// (`ghostty-wasm-abi`): only the `rustty_wasm.wasm` (`main_wasm.zig`) artifact
// carries these names; the separate `rustty_web_canvas.wasm` deliverable keeps
// its `rustty_wc_*` surface.
// ---------------------------------------------------------------------------

#[cfg(all(target_arch = "wasm32", feature = "ghostty-wasm-abi"))]
pub use ghostty_wasm_abi::{deferred_face_free, deferred_face_load, deferred_face_new};

/// The wasm-compatible API.
// pub const Wasm = struct {
//     const wasm = @import("../os/wasm.zig");
//     const alloc = wasm.alloc;
#[cfg(all(target_arch = "wasm32", feature = "ghostty-wasm-abi"))]
mod ghostty_wasm_abi {
    use super::{DeferredFace, WebCanvasData};
    use crate::face::{DesiredSize, Options, DEFAULT_DPI, FREETYPE_LOAD_FLAGS_DEFAULT};
    use crate::library::Library;
    use crate::Presentation;

    /// `export fn deferred_face_new(ptr: [*]const u8, len: usize, presentation: u16) ?*DeferredFace`
    // PORT NOTE (WAIVED site — `.port/parity-closure/ADJUDICATION.md#n4-deferred
    // -face-new`): `catch |err| { log.warn("error creating deferred face err={}",
    // …); return null; }`. The only reference failure mode is OOM from
    // `alloc.dupeZ`/`alloc.create`; in Rust `Vec`/`Box` ABORT on allocation
    // failure rather than returning an error, so there is NO reachable failure
    // arm here and no `null` return. A `log::warn!` at this site would have to
    // sit on the SUCCESS path, where the reference is silent. Do not "restore"
    // it.
    #[no_mangle]
    pub extern "C" fn deferred_face_new(
        ptr: *const u8,
        len: usize,
        presentation: u16,
    ) -> *mut DeferredFace {
        // fn deferred_face_new_(ptr, len, presentation) !*DeferredFace {
        // const font_str = try alloc.dupeZ(u8, ptr[0..len]);
        let font_str = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();

        // var face: DeferredFace = .{ .wc = .{ .alloc = alloc, .font_str = font_str,
        //     .presentation = @enumFromInt(presentation) } };
        //
        // PORT NOTE (`@enumFromInt`): same mapping the crate's `rustty_wc_face_new`
        // uses — 0 => .text, else .emoji (a value outside the enum is
        // safety-checked UB in the reference; the total mapping is the Rust map).
        let presentation = if presentation == 0 {
            Presentation::Text
        } else {
            Presentation::Emoji
        };
        let face = DeferredFace {
            wc: Some(WebCanvasData {
                font_str,
                presentation,
            }),
        };

        // const result = try alloc.create(DeferredFace); result.* = face; return result;
        Box::into_raw(Box::new(face))
    }

    /// `export fn deferred_face_free(ptr: ?*DeferredFace) void`
    #[no_mangle]
    pub extern "C" fn deferred_face_free(ptr: *mut DeferredFace) {
        // if (ptr) |v| { v.deinit(); alloc.destroy(v); }
        if !ptr.is_null() {
            let mut face = unsafe { Box::from_raw(ptr) };
            face.deinit();
        }
    }

    /// `export fn deferred_face_load(self: *DeferredFace, pts: f32) void`
    #[no_mangle]
    pub extern "C" fn deferred_face_load(self_: *mut DeferredFace, pts: f32) {
        if self_.is_null() {
            return;
        }
        let df = unsafe { &mut *self_ };
        // self.load(.{}, .{ .points = pts }) catch |err| {
        //     log.warn("error loading deferred face err={}", .{err});
        //     return;
        // };
        //
        // PORT NOTE: `.{ .points = pts }` relies on the reference `DesiredSize`
        // field defaults (`default_dpi`); written out here. The loaded `Face` the
        // reference discards is deinit'd (dropping it un-deinit'd would leak the
        // JS canvas handle); errors are log-as-comment.
        let lib = match Library::init() {
            Ok(lib) => lib,
            Err(_) => return,
        };
        match df.load(
            lib,
            Options {
                size: DesiredSize {
                    points: pts,
                    xdpi: DEFAULT_DPI,
                    ydpi: DEFAULT_DPI,
                },
                freetype_load_flags: FREETYPE_LOAD_FLAGS_DEFAULT,
            },
        ) {
            Ok(mut face) => face.deinit(),
            // log.warn("error loading deferred face err={}", .{err});
            Err(err) => {
                log::warn!(
                    target: super::LOG_SCOPE,
                    "error loading deferred face err={err:?}"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // test "fontconfig"
    //
    // PORT NOTE: the reference's runtime `if (options.backend != .fontconfig_freetype)
    // return error.SkipZigTest;` is expressed here as a compile-time `cfg` gate to
    // the fontconfig-active target class (mirroring the `coretext` test's
    // `#[cfg(target_os = "macos")]`), since the body names `discovery::Fontconfig`,
    // which only exists on that target. The runtime `!= fontconfig_freetype` skip is
    // additionally kept as an inverted `== fontconfig_freetype` guard for fidelity.
    #[cfg(all(
        not(target_os = "macos"),
        not(target_os = "windows"),
        not(target_arch = "wasm32")
    ))]
    #[test]
    fn fontconfig() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `fc::FC_GLOBAL`).
        let _fc = crate::fc::global_shared();
        // if (options.backend != .fontconfig_freetype) return error.SkipZigTest;
        if crate::options::BACKEND != crate::Backend::FontconfigFreetype {
            return;
        }

        use crate::discovery;
        use crate::face::DesiredSize;
        use crate::library::Library;

        // Load freetype
        // var lib = try Library.init(alloc);
        let mut lib = Library::init().unwrap();
        // defer lib.deinit();

        // Get a deferred face from fontconfig
        let mut def = {
            // var fc = discovery.Fontconfig.init(lib);
            let mut fc = discovery::Fontconfig::init(lib);
            // defer fc.deinit();
            // var it = try fc.discover(alloc, .{ .family = "monospace", .size = 12 });
            let mut it = fc
                .discover(discovery::Descriptor {
                    family: Some(b"monospace".to_vec()),
                    size: 12.0,
                    ..Default::default()
                })
                .unwrap();
            // defer it.deinit();
            // break :def (try it.next()).?;
            let d = it.next().unwrap().unwrap();
            it.deinit();
            fc.deinit();
            d
        };
        // defer def.deinit();

        // Verify we can get the name
        // var buf: [1024]u8 = undefined;
        // const n = try def.name(&buf);
        // try testing.expect(n.len > 0);
        let mut buf = [0u8; 1024];
        let n = def.name(&mut buf).unwrap();
        assert!(!n.is_empty());

        // Load it and verify it works
        // var face = try def.load(lib, .{ .size = .{ .points = 12 } });
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
        // defer face.deinit();
        // try testing.expect(face.glyphIndex(' ') != null);
        assert!(face.glyph_index(u32::from(' ')).is_some());

        face.deinit();
        def.deinit();
        lib.deinit();
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

    /// A CoreText `DeferredFace` for `bytes` (a font file in memory).
    #[cfg(target_os = "macos")]
    fn coretext_deferred(bytes: &[u8]) -> super::DeferredFace {
        use objc2_core_foundation::CFData;
        use objc2_core_text::{CTFont, CTFontManagerCreateFontDescriptorFromData};
        let data = CFData::from_bytes(bytes);
        let desc = unsafe { CTFontManagerCreateFontDescriptorFromData(&data) }
            .expect("CoreText accepts the font data");
        let font = unsafe { CTFont::with_font_descriptor(&desc, 12.0, std::ptr::null()) };
        super::DeferredFace {
            ct: Some(super::CoreTextData {
                font,
                variations: Vec::new(),
            }),
        }
    }

    /// UD110 (R13, task R13-D): the reference leaks the +1 name string on every
    /// `DeferredFace.name`/`familyName` and CoreText `Face.name` call. For a font
    /// loaded from data CoreText hands back the SAME string object on each copy
    /// (measured), so a leaked +1 shows up as a rising retain count. The port
    /// must leave it where it was after many calls.
    #[cfg(target_os = "macos")]
    #[test]
    fn coretext_name_lookups_release_the_copied_name_string() {
        use objc2_core_foundation::CFString;
        extern "C" {
            fn CFGetRetainCount(cf: *const core::ffi::c_void) -> isize;
        }
        let rc = |s: &CFString| unsafe { CFGetRetainCount((s as *const CFString).cast()) };
        // Independent copies (not the code under test): CTFontCopyAttribute
        // (family) and CTFontCopyDisplayName, each +1 in an owning handle.
        let copy_family = |f: &objc2_core_text::CTFont| {
            unsafe { f.attribute(objc2_core_text::kCTFontFamilyNameAttribute) }
                .unwrap()
                .downcast::<CFString>()
                .unwrap()
        };
        let copy_display = |f: &objc2_core_text::CTFont| unsafe { f.display_name() };

        let mut def = coretext_deferred(rustty_font::embedded::VARIABLE);
        let font = def.ct.as_ref().unwrap().font.clone();
        let family = copy_family(&font);
        let display = copy_display(&font);
        // Preconditions: each copy returns the one object we hold, and it is not
        // an immortal constant, so a leaked retain is visible in its count.
        assert!(std::ptr::eq(&*family, &*copy_family(&font)));
        assert!(std::ptr::eq(&*display, &*copy_display(&font)));
        let (family_rc, display_rc) = (rc(&family), rc(&display));
        assert!(family_rc < 1_000_000 && display_rc < 1_000_000);

        let mut buf = [0u8; 1024];
        for _ in 0..100 {
            assert_eq!(b"JetBrains Mono", def.family_name(&mut buf).unwrap());
            assert!(!def.name(&mut buf).unwrap().is_empty());
        }
        assert_eq!(family_rc, rc(&family), "familyName released its copy");
        assert_eq!(display_rc, rc(&display), "name released its copy");

        // The loaded CoreText `Face`'s own `name`.
        let face = crate::face::coretext::Face::init(
            crate::library::Library::init().unwrap(),
            rustty_font::embedded::VARIABLE,
            crate::face::Options {
                size: crate::face::DesiredSize {
                    points: 12.0,
                    xdpi: crate::face::DEFAULT_DPI,
                    ydpi: crate::face::DEFAULT_DPI,
                },
                freetype_load_flags: crate::face::FREETYPE_LOAD_FLAGS_DEFAULT,
            },
        )
        .unwrap();
        let face_family = copy_family(&face.font);
        let face_rc = rc(&face_family);
        assert!(face_rc < 1_000_000);
        for _ in 0..100 {
            assert_eq!(b"JetBrains Mono", face.name(&mut buf).unwrap());
        }
        assert_eq!(face_rc, rc(&face_family), "Face.name released its copy");

        drop((family, display, face_family));
        def.deinit();
    }

    /// S85-F2: a font whose CoreText family name is NULL (here: a font file with
    /// no `name` table) gives "unknown", the reference's `orelse return
    /// "unknown"`, where objc2's non-null `family_name()` panicked. `name` falls
    /// back to the family name (the display name is NULL too), so it reaches
    /// the same arm.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_coretext_font_without_a_family_name_is_unknown() {
        // Positive control: the unmodified font has its real family name.
        let mut def = coretext_deferred(rustty_font::embedded::VARIABLE);
        let mut buf = [0u8; 1024];
        assert_eq!(b"JetBrains Mono", def.family_name(&mut buf).unwrap());
        def.deinit();

        // The same font with its `name` table's directory tag renamed, so
        // CoreText finds no names at all.
        let mut bytes = rustty_font::embedded::VARIABLE.to_vec();
        let num_tables = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        let mut renamed = false;
        for i in 0..num_tables {
            let rec = 12 + 16 * i;
            if &bytes[rec..rec + 4] == b"name" {
                bytes[rec..rec + 4].copy_from_slice(b"zzzz");
                renamed = true;
            }
        }
        assert!(renamed, "the font has a name table to remove");

        let mut def = coretext_deferred(&bytes);
        let ct = def.ct.as_ref().unwrap();
        assert!(
            unsafe {
                ct.font
                    .attribute(objc2_core_text::kCTFontFamilyNameAttribute)
            }
            .is_none(),
            "precondition: CoreText has no family name for this font"
        );
        assert_eq!(b"unknown", def.family_name(&mut buf).unwrap());
        assert_eq!(b"unknown", def.name(&mut buf).unwrap());
        def.deinit();
    }
}
