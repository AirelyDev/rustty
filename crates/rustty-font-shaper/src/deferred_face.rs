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
    //
    // PORT NOTE: `Discover == CoreText` exactly on the `hasCoretext()` backends
    // (`rustty_font_has_coretext`): `coretext`, `coretext_freetype`,
    // `coretext_harfbuzz`, `coretext_noshape`.
    #[cfg(rustty_font_has_coretext)]
    pub ct: Option<CoreTextData>,

    /// Fontconfig (Linux default `fontconfig_freetype`).
    // fc: if (backend == .fontconfig_freetype) ?Fontconfig else void = ...,
    //
    // PORT NOTE: this is the freetype-active (`not(any(macos, wasm32))`) payload,
    // mirroring the macOS `ct` shape. `FontconfigData` is the port of the
    // reference `DeferredFace.Fontconfig` struct (`DeferredFace.zig:38-56`).
    #[cfg(rustty_font_has_fontconfig)]
    pub fc: Option<FontconfigData>,

    /// Windows (FreeType directory scan).
    // win: if (backend == .freetype_windows) ?Windows else void = ...,
    //
    // PORT NOTE: this is the `freetype_windows` payload, mirroring the macOS `ct` /
    // Linux `fc` shape. `WindowsData` is the port of the reference
    // `DeferredFace.Windows` struct (`DeferredFace.zig:58-90`).
    #[cfg(rustty_font_backend = "freetype_windows")]
    pub win: Option<WindowsData>,

    /// Canvas (wasm `web_canvas` backend).
    // wc: if (backend == .web_canvas) ?WebCanvas else void = ...,
    //
    // PORT NOTE: this is the wasm32 (`web_canvas`-active) payload, mirroring the
    // macOS `ct` / Linux `fc` shape. `WebCanvasData` is the port of the reference
    // `DeferredFace.WebCanvas` struct (`DeferredFace.zig:106-124`).
    #[cfg(rustty_font_backend = "web_canvas")]
    pub wc: Option<WebCanvasData>,
}
// PORT NOTE (S-14, N266): each payload field is keyed on the backend predicate
// the reference declaration names (`options.backend == ...` / `Discover ==
// CoreText`), not on the target OS. On `.freetype` ("no font discovery") all
// four are `void`, so the struct is empty — see `no_payload` below.

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

#[cfg(rustty_font_has_coretext)]
pub use coretext_payload::{new_cfstring, new_cfstring_bytes, CoreTextData};

#[cfg(rustty_font_has_coretext)]
mod coretext_payload {
    use super::DeferredFace;
    use crate::face::coretext::{cstring_ptr_utf8, cstring_utf8};
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

        /// The family and display names `family_name` / `name` return, each
        /// copied from CoreText on its first use and held with the font
        /// (`None` inside: CoreText has none). Rustty-only: see
        /// [`crate::face::coretext::Face::family_name`] (S85-F6, UD110) for why
        /// holding the copy stands in for the reference's never-released one.
        pub family_name_str: std::sync::OnceLock<Option<CFRetained<CFString>>>,
        pub display_name_str: std::sync::OnceLock<Option<CFRetained<CFString>>>,
    }

    impl CoreTextData {
        /// `.{ .font = font, .variations = variations }`.
        pub fn new(font: CFRetained<CTFont>, variations: Vec<Variation>) -> CoreTextData {
            CoreTextData {
                font,
                variations,
                family_name_str: std::sync::OnceLock::new(),
                display_name_str: std::sync::OnceLock::new(),
            }
        }

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
        pub fn family_name<'a>(&'a self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            // .coretext* => if (self.ct) |ct| {
            //     const family_name = ct.font.copyAttribute(.family_name) orelse return "unknown";
            //     return family_name.cstringPtr(.utf8) orelse unsupported: {
            //         break :unsupported family_name.cstring(buf, .utf8) orelse
            //             return error.OutOfMemory;
            //     };
            // },
            if let Some(ct) = self.ct.as_ref() {
                // PORT NOTE (S85-F2): the reference's `copyAttribute(.family_name)`
                // is nullable and a NULL takes `orelse return "unknown"`. objc2's
                // `CTFont::family_name` (`CTFontCopyFamilyName`) asserts non-null
                // and PANICKED instead; CoreText returns NULL for a font file with
                // no `name` table (measured, macOS 26.5.1). Use the same nullable
                // attribute copy the reference uses.
                //
                // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD110.
                // The reference never releases this +1 copy (nor the display
                // name's in `name`, nor `Face.name`'s), on the `cstringPtr`
                // return or the `cstring(buf)` one: each call leaks a retain on
                // the name string (measured on a font loaded from data). A
                // discovered font's names are immortal constants (measured,
                // including a non-system font installed in /Library/Fonts), and
                // the reference's `DeferredFace.ct` only comes from discovery, so
                // there this is a retain on an immortal string, not an observed
                // leak; the real leak is `Face.name` on an embedded face. The
                // port copies the string once, holds it in `ct` and releases it
                // with the face, so the `cstringPtr` return can borrow it.
                // CORRECTION (task T-F, S85-F6): the copy used to be released
                // at scope end after copying the bytes into `buf`, so a name
                // longer than `buf` was an error where the reference returned
                // the `cstringPtr` pointer.
                let family_name = ct
                    .family_name_str
                    .get_or_init(|| ct_font_copy_family_name(&ct.font));
                let Some(family_name) = family_name.as_deref() else {
                    return Ok(b"unknown");
                };
                return match cstring_ptr_utf8(family_name) {
                    Some(s) => Ok(s),
                    None => cstring_utf8(family_name, buf).ok_or_else(oom),
                };
            }

            // return "";
            Ok(&[])
        }

        /// Returns the name of this face. The memory is always owned by the
        /// face so it doesn't have to be freed.
        // pub fn name(self: DeferredFace, buf: []u8) ![]const u8 {
        pub fn name<'a>(&'a self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
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
                // PORT NOTE: reference defect (R13), UD110 — the one copy is
                // held in `ct` and released with the face (see `family_name`).
                let display_name = ct
                    .display_name_str
                    .get_or_init(|| ct_font_copy_display_name(&ct.font));
                let Some(display_name) = display_name.as_deref() else {
                    // orelse return self.familyName(buf);
                    return self.family_name(buf);
                };
                return match cstring_ptr_utf8(display_name) {
                    Some(s) => Ok(s),
                    None => cstring_utf8(display_name, buf).ok_or_else(oom),
                };
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
            //
            // PORT NOTE: this impl is the `hasCoretext()` payload, so only the
            // two CoreText arms can be live; `options.backend` picks between them
            // at compile time exactly as the comptime `switch` does.
            #[cfg(rustty_font_backend = "coretext_freetype")]
            return self.load_coretext_freetype(lib, opts);
            #[cfg(not(rustty_font_backend = "coretext_freetype"))]
            return self.load_coretext(lib, opts);
        }

        // fn loadCoreText(self, lib, opts) !Face {
        #[cfg(not(rustty_font_backend = "coretext_freetype"))]
        fn load_coretext(&mut self, _lib: Library, opts: Options) -> Result<Face, FaceError> {
            // _ = lib;
            let ct = self.ct.as_ref().unwrap();
            let mut face = Face::init_font_copy(&ct.font, opts)?;
            // errdefer face.deinit();
            // try face.setVariations(ct.variations, opts);
            //
            // S85-F3: the `errdefer` was a comment only, so a failing
            // `setVariations` dropped the face without its `deinit` (the
            // fontconfig and Windows arms below call it).
            if let Err(e) = face.set_variations(&ct.variations, opts) {
                face.deinit();
                return Err(e);
            }
            Ok(face)
        }

        // fn loadCoreTextFreetype(self, lib, opts) !Face {
        //
        // PORT NOTE (S-14, N266): ported live for `-Dfont-backend=
        // coretext_freetype` (CoreText discovery, FreeType faces); it was a
        // quoted comment while the option was folded to `.coretext`. `Face` is
        // `freetype::Face` in that build, so `FaceError` is `freetype::Error`,
        // which carries the two local errors in that build only.
        #[cfg(rustty_font_backend = "coretext_freetype")]
        fn load_coretext_freetype(
            &mut self,
            lib: Library,
            opts: Options,
        ) -> Result<Face, FaceError> {
            use objc2_core_foundation::CFURL;

            // const ct = self.ct.?;
            let ct = self.ct.as_ref().unwrap();

            // Get the URL for the font so we can get the filepath
            // const url = ct.font.copyAttribute(.url) orelse
            //     return error.FontHasNoFile;
            // defer url.release();
            //
            // PORT NOTE: `CTFontCopyAttribute(font, kCTFontURLAttribute)`, nullable
            // (the reference `pkg/macos` `copyAttribute`); the `CFRetained` is the
            // `defer url.release()`.
            // SAFETY: `ct.font` is a live retained CTFont and the key is
            // CoreText's own constant; the Copy rule gives +1, owned here.
            let url = unsafe { ct.font.attribute(objc2_core_text::kCTFontURLAttribute) }
                .and_then(|attr| attr.downcast::<CFURL>().ok())
                .ok_or(FaceError::FontHasNoFile)?;

            // Get the path from the URL
            // const path = url.copyPath() orelse return error.FontHasNoFile;
            // defer path.release();
            let path = url.path().ok_or(FaceError::FontHasNoFile)?;

            // URL decode the path
            // const blank = try macos.foundation.String.createWithBytes("", .utf8, false);
            // defer blank.release();
            let blank = new_cfstring("").ok_or_else(oom)?;
            // const decoded = try macos.foundation.URL.createStringByReplacingPercentEscapes(
            //     path,
            //     blank,
            // );
            // defer decoded.release();
            let decoded =
                CFURL::new_string_by_replacing_percent_escapes(None, Some(&path), Some(&blank))
                    .ok_or_else(oom)?;

            // Decode into a c string. 1024 bytes should be enough for anybody.
            // var buf: [1024]u8 = undefined;
            // const path_slice = decoded.cstring(buf[0..1023], .utf8) orelse
            //     return error.FontPathCantDecode;
            let mut buf = [0u8; 1024];
            let path_len = cstring_utf8(&decoded, &mut buf[0..1023])
                .ok_or(FaceError::FontPathCantDecode)?
                .len();

            // Freetype requires null-terminated. We always leave space at
            // the end for a zero so we set that up here.
            // buf[path_slice.len] = 0;
            buf[path_len] = 0;

            // Face index 0 is not always correct. We don't ship this configuration
            // in a release build. Users should use the pure CoreText builds.
            //std.log.warn("path={s}", .{path_slice});
            // var face = try Face.initFile(lib, buf[0..path_slice.len :0], 0, opts);
            // errdefer face.deinit();
            //
            // PORT NOTE: `cstring` stops at the first NUL (`sliceTo`), so
            // `buf[..=path_len]` holds exactly one NUL, at its end.
            let c_path = core::ffi::CStr::from_bytes_with_nul(&buf[..=path_len])
                .expect("cstring stops at the first NUL");
            // PORT NOTE: reference defect (R13 / the user's evident-intent rule),
            // UPSTREAM-DEFECTS.md UD280. The reference always opens face index 0,
            // which its own comment calls "not always correct": for a font file
            // holding several faces (a `.ttc` collection such as the system's
            // `Menlo.ttc`) it loads the collection's FIRST face instead of the
            // one CoreText discovered, so e.g. `font-family = Menlo` renders its
            // bold style with Menlo Regular. Reachable by any config that names a
            // font shipped as a collection, in a `coretext_freetype` build. The
            // port opens the face whose PostScript name is the CoreText font's
            // (`collection_face_index`), and keeps the reference's index 0 when
            // the file is not a collection or no face matches.
            let face_index = collection_face_index(lib, c_path, &ct.font);
            let mut face = Face::init_file(lib, c_path, face_index, opts)?;
            // try face.setVariations(ct.variations, opts);
            if let Err(e) = face.set_variations(&ct.variations, opts) {
                face.deinit();
                return Err(e);
            }

            // return face;
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

    /// UD280 (see `load_coretext_freetype`): the index, in the font file at
    /// `path`, of the face CoreText resolved `font` to — the first face whose
    /// PostScript name equals the CoreText font's. `0` (the reference's index)
    /// when the file holds one face, when no face matches, or when a probe
    /// fails (the real open that follows then reports that failure itself).
    /// RUSTTY-ONLY.
    #[cfg(rustty_font_backend = "coretext_freetype")]
    fn collection_face_index(lib: Library, path: &core::ffi::CStr, font: &CTFont) -> i32 {
        let want = unsafe { font.post_script_name() }.to_string();
        // "Mutex to be held any time the library is being used to create or
        // destroy a face" (`FreetypeLibrary.mutex`).
        // SAFETY: `lib.mutex` is the library's live boxed mutex.
        let _guard = unsafe { (*lib.mutex).lock().unwrap() };
        let probe = |index: i32| -> Option<(i64, bool)> {
            let face = lib.lib.init_face(path, index).ok()?;
            // SAFETY: `face.handle` is the live `FT_Face` just opened; the
            // PostScript name (NULL-able) is owned by the face.
            let (num_faces, matches) = unsafe {
                let ps = freetype_sys::FT_Get_Postscript_Name(face.handle);
                (
                    (*face.handle).num_faces as i64,
                    !ps.is_null() && core::ffi::CStr::from_ptr(ps).to_bytes() == want.as_bytes(),
                )
            };
            face.deinit();
            Some((num_faces, matches))
        };
        let Some((num_faces, first_matches)) = probe(0) else {
            return 0;
        };
        if num_faces <= 1 || first_matches {
            return 0;
        }
        for index in 1..num_faces {
            let Ok(index) = i32::try_from(index) else {
                break;
            };
            if let Some((_, true)) = probe(index) {
                return index;
            }
        }
        0
    }

    /// Build a backend-face `OutOfMemory` error (the reference `error.OutOfMemory`
    /// returned by `name`/`familyName` when the name doesn't fit the buffer).
    // PORT NOTE (S-14): `FaceError` — the active face's error set, which is
    // `freetype::Error` in a `coretext_freetype` build; both have the variant.
    fn oom() -> FaceError {
        FaceError::OutOfMemory
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE: the non-CoreText arms of `familyName`/`name`/`load`/
// `hasCodepoint` are recorded verbatim here; the Fontconfig, Windows and
// WebCanvas arms are ported in their backends' impls below, and the wasm
// `Wasm` C-ABI struct in the `ghostty_wasm_abi` module. The `coretext_freetype`
// arm (`loadCoreTextFreetype`) is ported live in `coretext_payload` above
// (S-14, N266; it was the quoted copy kept below while `options::BACKEND`
// resolved only the per-OS default).
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
// `DiscoverIterator::next`) constructs, and the `familyName`/`name`/`load`
// (`loadFontconfig`) fontconfig arms below. (CORRECTION, 2026-09-24, parity
// tail S-8a: this note still called those three arms DEFERRED
// `unimplemented!()` leaves; they are ported.)
#[cfg(rustty_font_has_fontconfig)]
pub use fontconfig_payload::FontconfigData;

#[cfg(rustty_font_has_fontconfig)]
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

#[cfg(rustty_font_backend = "freetype_windows")]
pub use windows_payload::WindowsData;

#[cfg(rustty_font_backend = "freetype_windows")]
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
        // CORRECTION (S-14, N287): `&'a self`, as the coretext payload has it —
        // `peek.name(buf)` may return the FreeType face's own name bytes
        // (`face/freetype.rs` `name<'a>(&'a self, ...)`), so the result borrows
        // `self` too. With `&self` this arm did not type-check for
        // `x86_64-pc-windows-*` (found by S-14's cross check).
        pub fn family_name<'a>(&'a self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
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
        // (`&'a self`: see `family_name`, S-14 / N287.)
        pub fn name<'a>(&'a self, buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
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

// ---------------------------------------------------------------------------
// `.freetype` ("FreeType for font rendering with no font discovery enabled"):
// every payload is `void`, so each `switch (options.backend)` in
// `DeferredFace.zig` takes its `.freetype` arm. Ported live by S-14 (N266).
// ---------------------------------------------------------------------------
#[cfg(rustty_font_backend = "freetype")]
mod no_payload {
    use super::DeferredFace;
    use crate::face::{Face, FaceError, Options};
    use crate::library::Library;
    use crate::Presentation;

    impl DeferredFace {
        // pub fn deinit(self: *DeferredFace) void {
        //     switch (options.backend) { ... .freetype => {}, ... }
        //     self.* = undefined;
        // }
        pub fn deinit(&mut self) {}

        /// A `DeferredFace` with no payload. Used only where the reference passes
        /// `undefined` (the `add deferred without loading options` test), where the
        /// payload is guaranteed never to be accessed.
        pub fn undefined() -> DeferredFace {
            DeferredFace {}
        }

        /// Returns the family name of the font.
        // pub fn familyName(self: DeferredFace, buf: []u8) ![]const u8 {
        //     switch (options.backend) { .freetype => {}, ... }
        //     return "";
        // }
        pub fn family_name<'a>(&self, _buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            Ok(&[])
        }

        /// Returns the name of this face. The memory is always owned by the
        /// face so it doesn't have to be freed.
        // pub fn name(self: DeferredFace, buf: []u8) ![]const u8 {
        //     switch (options.backend) { .freetype => {}, ... }
        //     return "";
        // }
        pub fn name<'a>(&self, _buf: &'a mut [u8]) -> Result<&'a [u8], FaceError> {
            Ok(&[])
        }

        /// Load the deferred font face. This does nothing if the face is loaded.
        // pub fn load(self: *DeferredFace, lib: Library, opts: font.face.Options) !Face {
        //     return switch (options.backend) {
        //         ...
        //         // Unreachable because we must be already loaded or have the
        //         // proper configuration for one of the other deferred mechanisms.
        //         .freetype => unreachable,
        //     };
        // }
        //
        // PORT NOTE (R7): an invariant `unreachable`: with no discovery there is
        // no way to obtain a deferred face (every `DeferredFace` comes from a
        // discoverer; the only other constructor is the test-only `undefined`,
        // whose payload is never accessed), so this is checked, not R13.
        pub fn load(&mut self, _lib: Library, _opts: Options) -> Result<Face, FaceError> {
            unreachable!()
        }

        /// Returns true if this face can satisfy the given codepoint and
        /// presentation. If presentation is null, then it just checks if the
        /// codepoint is present at all.
        ///
        /// This should not require the face to be loaded IF we're using a
        /// discovery mechanism (i.e. fontconfig). If no discovery is used,
        /// the face is always expected to be loaded.
        // pub fn hasCodepoint(self: DeferredFace, cp: u32, p: ?Presentation) bool {
        //     switch (options.backend) { ... .freetype => {}, ... }
        //     // This is unreachable because discovery mechanisms terminate, and
        //     // if we're not using a discovery mechanism, the face MUST be loaded.
        //     unreachable;
        // }
        pub fn has_codepoint(&self, _cp: u32, _p: Option<Presentation>) -> bool {
            // This is unreachable because discovery mechanisms terminate, and
            // if we're not using a discovery mechanism, the face MUST be loaded.
            unreachable!()
        }
    }
}

#[cfg(rustty_font_backend = "web_canvas")]
pub use web_canvas_payload::WebCanvasData;

#[cfg(rustty_font_backend = "web_canvas")]
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
    // which only exists on that target. That `cfg` is the whole skip: a runtime
    // `!= fontconfig_freetype` guard inside it could never fire, and an early
    // `return` would be a silent pass anyway (R14).
    #[cfg(rustty_font_backend = "fontconfig_freetype")]
    #[test]
    fn fontconfig() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `fc::FC_GLOBAL`).
        let _fc = crate::fc::global_shared();
        // if (options.backend != .fontconfig_freetype) return error.SkipZigTest;
        // (the `#[cfg]` above IS that skip, keyed on the backend itself; R14 —
        // no silent early return)

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
    #[cfg(rustty_font_has_coretext)]
    #[cfg_attr(
        not(rustty_font_backend = "coretext"),
        ignore = "SkipZigTest: `options.backend != .coretext` (R14)"
    )]
    #[test]
    fn coretext() {
        // if (options.backend != .coretext) return error.SkipZigTest;
        // (the `cfg_attr(.., ignore)` above IS that skip, made visible — R14, no
        // silent early return. The `#[cfg]` scopes the body to the builds where
        // `discovery.CoreText` exists, the `hasCoretext()` backends.)

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
    #[cfg(rustty_font_has_coretext)]
    fn coretext_deferred(bytes: &[u8]) -> super::DeferredFace {
        use objc2_core_foundation::CFData;
        use objc2_core_text::{CTFont, CTFontManagerCreateFontDescriptorFromData};
        let data = CFData::from_bytes(bytes);
        let desc = unsafe { CTFontManagerCreateFontDescriptorFromData(&data) }
            .expect("CoreText accepts the font data");
        let font = unsafe { CTFont::with_font_descriptor(&desc, 12.0, std::ptr::null()) };
        super::DeferredFace {
            ct: Some(super::CoreTextData::new(font, Vec::new())),
        }
    }

    /// S85-F6 / S90-F3: the CoreText name lookups return the `cstringPtr`
    /// pointer when CoreFoundation has one (`DeferredFace.zig:159-162,
    /// 186-195`, `face/coretext.zig:211-212`), so a name longer than the
    /// caller's buffer is returned whole, not an error. Here the buffer holds 4
    /// bytes; the names are ASCII, for which `CFStringGetCStringPtr` succeeds
    /// (asserted as a precondition, so the pointer path is the one tested).
    #[cfg(rustty_font_has_coretext)]
    #[test]
    fn coretext_names_longer_than_the_buffer_are_returned_whole() {
        use crate::face::coretext::cstring_ptr_utf8;
        let mut buf = [0u8; 4];

        // A discovered font (the reference's only DeferredFace source) and a
        // font loaded from data.
        let menlo = {
            let name = super::new_cfstring("Menlo").unwrap();
            let font = unsafe { objc2_core_text::CTFont::with_name(&name, 12.0, std::ptr::null()) };
            super::DeferredFace {
                ct: Some(super::CoreTextData::new(font, Vec::new())),
            }
        };
        let jbm = coretext_deferred(rustty_font::embedded::VARIABLE);
        for (mut def, family, display) in [
            (menlo, &b"Menlo"[..], &b"Menlo Regular"[..]),
            (jbm, b"JetBrains Mono", b"JetBrains Mono Regular"),
        ] {
            assert_eq!(family, def.family_name(&mut buf).unwrap());
            assert_eq!(display, def.name(&mut buf).unwrap());
            let ct = def.ct.as_ref().unwrap();
            let held = ct.family_name_str.get().unwrap().as_deref().unwrap();
            assert!(
                cstring_ptr_utf8(held).is_some(),
                "precondition: pointer path"
            );
            let held = ct.display_name_str.get().unwrap().as_deref().unwrap();
            assert!(
                cstring_ptr_utf8(held).is_some(),
                "precondition: pointer path"
            );
            def.deinit();
        }

        // The loaded CoreText `Face`'s `name`.
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
        assert_eq!(b"JetBrains Mono", face.name(&mut buf).unwrap());
        let held = face.family_name.get().unwrap().as_deref().unwrap();
        assert!(
            cstring_ptr_utf8(held).is_some(),
            "precondition: pointer path"
        );
    }

    /// `String.cstring(buf, .utf8)`, the fallback when CoreFoundation has no
    /// pointer (`pkg/macos/foundation/string.zig:60-68`): the bytes and a NUL
    /// must fit in `buf`, else null (the callers' `error.OutOfMemory`). A
    /// non-ASCII string is stored as UTF-16, so it has no UTF-8 pointer.
    #[cfg(rustty_font_has_coretext)]
    #[test]
    fn cstring_needs_room_for_the_nul() {
        use crate::face::coretext::{cstring_ptr_utf8, cstring_utf8};
        let s = objc2_core_foundation::CFString::from_str("Ünïcødé");
        assert!(cstring_ptr_utf8(&s).is_none(), "precondition: no pointer");
        let utf8 = "Ünïcødé".as_bytes();
        let mut exact = vec![0u8; utf8.len()];
        assert_eq!(cstring_utf8(&s, &mut exact), None, "no room for the NUL");
        let mut room = vec![0u8; utf8.len() + 1];
        assert_eq!(cstring_utf8(&s, &mut room), Some(utf8));
    }

    /// S85-F3: `loadCoreText`'s `errdefer face.deinit()` (`DeferredFace.zig:
    /// 248-258`): when `setVariations` fails, the freshly created face is torn
    /// down through `deinit` before the error returns. The fail-point makes
    /// `set_variations` fail; the probe counts `Face::deinit` calls.
    // Scoped to the builds whose `load` takes `loadCoreText` (not
    // `coretext_freetype`, which loads by file path — see the
    // `load_coretext_freetype_*` tests below).
    #[cfg(all(
        rustty_font_has_coretext,
        not(rustty_font_backend = "coretext_freetype")
    ))]
    #[test]
    fn load_coretext_deinits_the_face_when_set_variations_fails() {
        use crate::face::test_probe;
        let opts = crate::face::Options {
            size: crate::face::DesiredSize {
                points: 12.0,
                xdpi: crate::face::DEFAULT_DPI,
                ydpi: crate::face::DEFAULT_DPI,
            },
            freetype_load_flags: crate::face::FREETYPE_LOAD_FLAGS_DEFAULT,
        };
        let lib = crate::library::Library::init().unwrap();
        let mut def = coretext_deferred(rustty_font::embedded::VARIABLE);
        def.ct.as_mut().unwrap().variations = vec![crate::face::Variation {
            id: crate::face::Id::init(b"wght"),
            value: 700.0,
        }];

        // Positive control: with the fail-point disarmed the load succeeds.
        // (`set_variations` itself deinits the face it replaces, once.)
        let before = test_probe::face_deinits();
        let mut face = def.load(lib, opts).expect("load with variations");
        assert_eq!(test_probe::face_deinits(), before + 1);
        face.deinit();

        test_probe::arm_set_variations_failpoint();
        let before = test_probe::face_deinits();
        assert!(def.load(lib, opts).is_err(), "set_variations failed");
        // The fail-point was consumed, so set_variations really ran.
        assert!(!test_probe::set_variations_failpoint_should_fail());
        assert_eq!(
            test_probe::face_deinits(),
            before + 1,
            "the face was deinit'ed on the error path"
        );
        def.deinit();
    }

    /// S-14 (N266) fixture: a CoreText font backed by a FILE, as discovery
    /// yields (a data-backed font has no `kCTFontURLAttribute`, so
    /// `loadCoreTextFreetype` could not load it). The file is the embedded
    /// variable JetBrains Mono, written under a directory whose name needs
    /// percent-escaping (a space and a literal `%41`), so `CFURLCopyPath`'s
    /// escaped path must be decoded — `createStringByReplacingPercentEscapes` —
    /// before FreeType can open it. Returns the deferred face and the temp dir
    /// (removed by the caller).
    #[cfg(rustty_font_backend = "coretext_freetype")]
    fn coretext_freetype_file_deferred() -> (super::DeferredFace, std::path::PathBuf) {
        use core::ptr::NonNull;
        use objc2_core_foundation::{CFArray, CFRetained, CFURL};
        use objc2_core_text::{
            CTFont, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromURL,
        };

        let dir = std::env::temp_dir().join(format!(
            "rustty s14 %41 {}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("JetBrains Mono.ttf");
        std::fs::write(&path, rustty_font::embedded::VARIABLE).unwrap();

        let bytes = path.to_str().unwrap().as_bytes();
        // SAFETY: `bytes` is readable for its length.
        let url = unsafe {
            CFURL::from_file_system_representation(
                None,
                bytes.as_ptr(),
                bytes.len() as isize,
                false,
            )
        }
        .expect("file URL");
        // SAFETY: `url` is a live file URL.
        let descs = unsafe { CTFontManagerCreateFontDescriptorsFromURL(&url) }
            .expect("CoreText reads the font file");
        assert!(CFArray::count(&descs) > 0, "one descriptor per face");
        // SAFETY: index 0 < count; the element is a CTFontDescriptor, retained
        // here because the array is released at scope end.
        let desc: CFRetained<CTFontDescriptor> = unsafe {
            let p = CFArray::value_at_index(&descs, 0);
            CFRetained::retain(NonNull::new(p as *mut CTFontDescriptor).unwrap())
        };
        let font = unsafe { CTFont::with_font_descriptor(&desc, 12.0, std::ptr::null()) };
        (
            super::DeferredFace {
                ct: Some(super::CoreTextData::new(font, Vec::new())),
            },
            dir,
        )
    }

    #[cfg(rustty_font_backend = "coretext_freetype")]
    fn ft_opts() -> crate::face::Options {
        crate::face::Options {
            size: crate::face::DesiredSize {
                points: 12.0,
                xdpi: crate::face::DEFAULT_DPI,
                ydpi: crate::face::DEFAULT_DPI,
            },
            freetype_load_flags: crate::face::FREETYPE_LOAD_FLAGS_DEFAULT,
        }
    }

    /// S-14 (N266): `loadCoreTextFreetype` (`DeferredFace.zig:260-302`) loads
    /// the CoreText-discovered font's FILE into a FreeType face: URL -> path ->
    /// percent-decoded C string -> `Face.initFile(lib, path, 0, opts)`. The
    /// fixture's path only opens if the decode ran (negative control: skipping
    /// `createStringByReplacingPercentEscapes` makes FreeType fail to open the
    /// still-escaped path).
    #[cfg(rustty_font_backend = "coretext_freetype")]
    #[test]
    fn load_coretext_freetype_loads_the_font_file_with_freetype() {
        let mut lib = crate::library::Library::init().unwrap();
        let (mut def, dir) = coretext_freetype_file_deferred();

        let mut face = def.load(lib, ft_opts()).expect("loadCoreTextFreetype");
        // It IS a FreeType face (the `Face` of this build), holding the file's
        // glyphs.
        let _: &crate::face::freetype::Face = &face;
        assert!(face.glyph_index(u32::from('A')).is_some());
        assert!(
            face.face.has_multiple_masters(),
            "the variable font, not a fallback"
        );

        face.deinit();
        def.deinit();
        lib.deinit();
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// UD280 (R13-style, S-14): `loadCoreTextFreetype` opens face index 0 of
    /// the discovered font's file ("Face index 0 is not always correct"). In a
    /// collection file that is the wrong face: CoreText's "Menlo Bold" lives in
    /// `Menlo.ttc`, whose face 0 is Menlo Regular, so a bold style loaded
    /// regular. The port opens the face whose PostScript name is the CoreText
    /// font's. Precondition asserted: the file really is a multi-face
    /// collection and Menlo Bold is not its face 0. Negative control: forcing
    /// index 0 (the reference) loads "Menlo-Regular" and fails.
    #[cfg(rustty_font_backend = "coretext_freetype")]
    #[test]
    fn load_coretext_freetype_opens_the_discovered_face_of_a_collection() {
        use objc2_core_text::CTFont;
        let mut lib = crate::library::Library::init().unwrap();
        let name = super::new_cfstring("Menlo-Bold").unwrap();
        let font = unsafe { CTFont::with_name(&name, 12.0, core::ptr::null()) };
        assert_eq!(
            "Menlo-Bold",
            unsafe { font.post_script_name() }.to_string(),
            "precondition: CoreText resolved Menlo Bold"
        );
        let mut def = super::DeferredFace {
            ct: Some(super::CoreTextData::new(font, Vec::new())),
        };

        let mut face = def.load(lib, ft_opts()).expect("loadCoreTextFreetype");
        // SAFETY: `face.face` is the live FT face the load opened.
        let (num_faces, index, ps) = unsafe {
            let h = face.face.handle;
            let ps = freetype_sys::FT_Get_Postscript_Name(h);
            (
                (*h).num_faces,
                (*h).face_index,
                std::ffi::CStr::from_ptr(ps).to_string_lossy().into_owned(),
            )
        };
        assert!(
            num_faces > 1,
            "precondition: Menlo's file is a collection ({num_faces})"
        );
        assert_eq!(
            "Menlo-Bold", ps,
            "the discovered face, not face 0 (index {index})"
        );
        assert_ne!(
            0, index,
            "precondition: Menlo Bold is not the collection's face 0"
        );

        face.deinit();
        def.deinit();
        lib.deinit();
    }

    /// S-14 (N266): a font with no file (`copyAttribute(.url)` is null — a
    /// data-backed CoreText font) is `error.FontHasNoFile`.
    #[cfg(rustty_font_backend = "coretext_freetype")]
    #[test]
    fn load_coretext_freetype_without_a_file_is_font_has_no_file() {
        let mut lib = crate::library::Library::init().unwrap();
        let mut def = coretext_deferred(rustty_font::embedded::VARIABLE);
        let got = def.load(lib, ft_opts());
        assert!(
            matches!(got, Err(crate::face::FaceError::FontHasNoFile)),
            "no URL -> FontHasNoFile"
        );
        def.deinit();
        lib.deinit();
    }

    /// S-14 (N266): `loadCoreTextFreetype`'s `errdefer face.deinit()` — when
    /// `setVariations` fails, the FreeType face it opened is torn down through
    /// `deinit` before the error returns (the S85-F3 fail-point + probe).
    #[cfg(rustty_font_backend = "coretext_freetype")]
    #[test]
    fn load_coretext_freetype_deinits_the_face_when_set_variations_fails() {
        use crate::face::test_probe;
        let mut lib = crate::library::Library::init().unwrap();
        let (mut def, dir) = coretext_freetype_file_deferred();
        def.ct.as_mut().unwrap().variations = vec![crate::face::Variation {
            id: crate::face::Id::init(b"wght"),
            value: 700.0,
        }];

        // Positive control: disarmed, the load (with its variations) succeeds
        // and nothing is deinit'ed.
        let before = test_probe::face_deinits();
        let mut face = def.load(lib, ft_opts()).expect("load with variations");
        assert_eq!(test_probe::face_deinits(), before);
        face.deinit();

        test_probe::arm_set_variations_failpoint();
        let before = test_probe::face_deinits();
        assert!(def.load(lib, ft_opts()).is_err(), "set_variations failed");
        // The fail-point was consumed, so set_variations really ran.
        assert!(!test_probe::set_variations_failpoint_should_fail());
        assert_eq!(
            test_probe::face_deinits(),
            before + 1,
            "the face was deinit'ed on the error path"
        );

        def.deinit();
        lib.deinit();
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// UD110 (R13, task R13-D): the reference leaks the +1 name string on every
    /// `DeferredFace.name`/`familyName` and CoreText `Face.name` call. For a font
    /// loaded from data CoreText hands back the SAME string object on each copy
    /// (measured), so a leaked +1 shows up as a rising retain count. The port
    /// must leave it where it was after many calls. Only `Face.name` on a
    /// data-loaded (embedded) face leaks in the reference: a discovered font's
    /// names are immortal, and the reference's `DeferredFace` is always
    /// discovered. The data-backed `DeferredFace` here exists only so that a
    /// leaked retain would be visible; it proves the port releases its copies.
    ///
    /// Since task T-F (S85-F6 / S90-F3) the port holds ONE copy per face, so
    /// that a `cstringPtr` result can borrow it: the first call adds exactly
    /// one retain, later calls add none, and dropping the held copy gives it
    /// back (the face then holds nothing else).
    #[cfg(rustty_font_has_coretext)]
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

        // The name strings of one font file are one process-wide object, so a
        // retain held by another test running in parallel on the same font
        // would move these counts. Each half therefore uses its own copy of
        // JetBrains Mono, renamed so no other test shares its strings.
        let def_font = renamed_jetbrains_mono(b"RusttyUd110Def");
        let face_font = renamed_jetbrains_mono(b"RusttyUd110Fce");

        let mut def = coretext_deferred(&def_font);
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
        assert_eq!(b"RusttyUd110Def", def.family_name(&mut buf).unwrap());
        assert!(!def.name(&mut buf).unwrap().is_empty());
        assert_eq!(family_rc + 1, rc(&family), "familyName holds one copy");
        assert_eq!(display_rc + 1, rc(&display), "name holds one copy");
        for _ in 0..100 {
            assert_eq!(b"RusttyUd110Def", def.family_name(&mut buf).unwrap());
            assert!(!def.name(&mut buf).unwrap().is_empty());
        }
        assert_eq!(family_rc + 1, rc(&family), "familyName took no more");
        assert_eq!(display_rc + 1, rc(&display), "name took no more");
        let ct = def.ct.as_mut().unwrap();
        ct.family_name_str = std::sync::OnceLock::new();
        ct.display_name_str = std::sync::OnceLock::new();
        assert_eq!(family_rc, rc(&family), "the held copy was the only retain");
        assert_eq!(
            display_rc,
            rc(&display),
            "the held copy was the only retain"
        );
        def.deinit();

        // The loaded CoreText `Face`'s own `name`.
        let face = crate::face::coretext::Face::init(
            crate::library::Library::init().unwrap(),
            &face_font,
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
        // `init` may already have named the face (its debug variation-axes log
        // does, when a logger is installed); start from an empty cache.
        let mut face = face;
        face.family_name = std::sync::OnceLock::new();
        let face_family = copy_family(&face.font);
        let face_rc = rc(&face_family);
        assert!(face_rc < 1_000_000);
        for _ in 0..100 {
            assert_eq!(b"RusttyUd110Fce", face.name(&mut buf).unwrap());
        }
        assert_eq!(face_rc + 1, rc(&face_family), "Face.name holds one copy");
        // Dropping the held copy (not the font: CoreText may keep the name
        // alive in its own caches once the font goes) gives the retain back.
        face.family_name = std::sync::OnceLock::new();
        assert_eq!(
            face_rc,
            rc(&face_family),
            "the held copy was the only retain"
        );
        drop(face);

        drop((family, display, face_family));
    }

    /// JetBrains Mono with every "JetBrains Mono" in the file (the `name`
    /// table's Mac Roman and UTF-16BE records) replaced by `to`, so CoreText
    /// reports `to` as its family name. CoreText does not check the table
    /// checksums (as the S85-F2 fixture below also relies on).
    #[cfg(rustty_font_has_coretext)]
    fn renamed_jetbrains_mono(to: &[u8; 14]) -> Vec<u8> {
        let mut bytes = rustty_font::embedded::VARIABLE.to_vec();
        let from = b"JetBrains Mono";
        let from16: Vec<u8> = from.iter().flat_map(|&b| [0, b]).collect();
        let to16: Vec<u8> = to.iter().flat_map(|&b| [0, b]).collect();
        let mut n = 0;
        for (pat, rep) in [(&from[..], &to[..]), (&from16[..], &to16[..])] {
            let mut i = 0;
            while i + pat.len() <= bytes.len() {
                if &bytes[i..i + pat.len()] == pat {
                    bytes[i..i + pat.len()].copy_from_slice(rep);
                    n += 1;
                    i += pat.len();
                } else {
                    i += 1;
                }
            }
        }
        assert!(n > 0, "the font names itself JetBrains Mono");
        bytes
    }

    /// S85-F2: a font whose CoreText family name is NULL (here: a font file with
    /// no `name` table) gives "unknown", the reference's `orelse return
    /// "unknown"`, where objc2's non-null `family_name()` panicked. `name` falls
    /// back to the family name (the display name is NULL too), so it reaches
    /// the same arm.
    #[cfg(rustty_font_has_coretext)]
    #[test]
    fn a_coretext_font_without_a_family_name_is_unknown() {
        // The body runs in a child process (`crate::test_child`): after a test
        // builds a variation instance of the same font data, CoreText can name
        // this nameless font "JetBrains Mono" for the rest of the process.
        const CHILD: &str = "RUSTTY_T_F2_NAMELESS_DEFERRED_CHILD";
        if std::env::var_os(CHILD).is_some() {
            a_coretext_font_without_a_family_name_is_unknown_child();
            return;
        }
        crate::test_child::run_isolated(
            "deferred_face::tests::a_coretext_font_without_a_family_name_is_unknown",
            CHILD,
        );
    }

    #[cfg(rustty_font_has_coretext)]
    fn a_coretext_font_without_a_family_name_is_unknown_child() {
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
