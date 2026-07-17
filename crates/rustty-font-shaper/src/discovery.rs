//! Font discovery.
// PORTS: ghostty `src/font/discovery.zig` (1349 loc, 8 tests) — font discovery.
// The active backend on macOS is the CoreText discoverer (`CTFontDescriptor` /
// `CTFontCollection`), fully ported here. The Fontconfig discoverer (the Linux
// default) is DEFERRED and recorded as verbatim reference comments; likewise the
// Windows FreeType-scan discoverer (`freetype_windows` backend). `Descriptor`
// (the search descriptor) and both conversions — `toCoreTextDescriptor` and
// `toFcPattern` (Fontconfig, gated to the fontconfig backend class) — are ported.
//
// PORT NOTE (imports): the reference imports are
//   const std = @import("std");
//   const builtin = @import("builtin");
//   const Allocator = std.mem.Allocator;
//   const assert = @import("../quirks.zig").inlineAssert;
//   const fontconfig = @import("fontconfig");
//   const macos = @import("macos");
//   const opentype = @import("opentype.zig");
//   const options = @import("main.zig").options;
//   const Collection = @import("main.zig").Collection;
//   const DeferredFace = @import("main.zig").DeferredFace;
//   const Face = @import("main.zig").Face;
//   const Library = @import("main.zig").Library;
//   const Presentation = @import("main.zig").Presentation;
//   const Variation = @import("main.zig").face.Variation;
//   const log = std.log.scoped(.discovery);
// Mapped per memory `apc-glyf-font-layering-d1` + the F3 kickoff:
//   * `macos.foundation.*` -> `objc2_core_foundation`
//     (CFString/CFNumber/CFDictionary/CFArray/CFCharacterSet/CFRange),
//   * `macos.text.*`       -> `objc2_core_text`
//     (CTFontDescriptor/CTFontCollection/CTFont + the attribute-key statics),
//   * `opentype.*`         -> the PURE `rustty-font` leaf (`Head`/`OS2` parsers),
//   * `fontconfig`         -> Linux discovery, DEFERRED (recorded as comments),
//   * `assert`             -> `rustty_util::quirks::inlineAssert`,
//   * `log`                -> no logging facade wired into this crate; every
//     `log.*` call is ported as a verbatim comment and is a no-op.

use crate::face::Variation;

/// Discover implementation for the compile options.
// pub const Discover = switch (options.backend) {
//     .freetype => void, // no discovery
//     .freetype_windows => Windows,
//     .fontconfig_freetype => Fontconfig,
//     .web_canvas => void, // no discovery
//     .coretext,
//     .coretext_freetype,
//     .coretext_harfbuzz,
//     .coretext_noshape,
//     => CoreText,
// };
//
// PORT NOTE: the `switch` above is preserved verbatim as a reference; it is
// modeled here with `cfg(target_os)` matching `backend.zig`'s `Backend.default`
// (macOS default `coretext` → `CoreText`; Linux default `fontconfig_freetype` →
// `Fontconfig`). Parity targets both defaults.
/// The active backend discoverer. macOS default `coretext` → `CoreText`.
#[cfg(target_os = "macos")]
pub type Discover = CoreText;

/// Deferred discoverer stub for non-macOS targets (the Fontconfig discoverer is
/// DEFERRED). Present only so `CodepointResolver`/`Collection` type-check; the
/// real Fontconfig discoverer lands with the full Fontconfig backend.
#[cfg(not(target_os = "macos"))]
pub type Discover = Fontconfig;

// ---------------------------------------------------------------------------
// Fontconfig (Linux default `fontconfig_freetype`) discoverer — typed stub.
//
// PORT NOTE: the real Fontconfig discoverer is DEFERRED (its full reference body
// is recorded verbatim in the comment block further below, and it lands in L3
// Task 6/7). This module exists so the crate TYPE-CHECKS on Linux: `Collection`,
// `CodepointResolver` and `SharedGridSet` name `Discover::init`/`.deinit`/
// `.discover`/`.discover_fallback` and a `DiscoverIterator{next,deinit}`
// unconditionally (the reference comptime-elides them when `Discover == void`,
// but Rust type-checks them on every target).
//
// Unlike the wasm arm below — which is a FUNCTIONAL noop, because `.web_canvas
// => void` genuinely means "no discovery" — Linux discovery is a real,
// not-yet-written feature. So these bodies are HONEST `unimplemented!()` leaves
// rather than silent `Ok(None)`s: a Linux caller that reaches them gets a loud
// panic naming the deferral, matching the convention already used by the
// non-macOS `DeferredFace` stubs (`deferred_face.rs`). The SIGNATURES are real
// and mirror the reference exactly, so call sites bind against the final shape.
#[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32")))]
pub use fontconfig_impl::{DiscoverIterator, Error, Fontconfig};

#[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32")))]
mod fontconfig_impl {
    use super::Descriptor;
    use crate::collection::Collection;
    use crate::deferred_face::DeferredFace;
    use crate::face::FaceError;
    use crate::library::Library;

    /// The error set for Fontconfig discovery.
    // PORT NOTE: the reference errors are the inferred union of `fontconfig.Error`
    // (`error.FontConfigFailed` from `fontSort`, plus the `pattern.get` failures
    // threaded through `DiscoverIterator.next`), `Allocator.Error`, and the backend
    // `Face`/`Collection` errors threaded through `discoverFallback`. Modeled as one
    // enum with `From` conversions so `?` composes them — the same shape as the
    // macOS `coretext_impl::Error`, so `SharedGridSet`'s `Discovery(discovery::Error)`
    // variant and its `From<discovery::Error>` compose identically on both targets.
    // The FontConfig-specific variants are fleshed out in L3 Task 6.
    #[derive(Debug)]
    pub enum Error {
        /// `error.FontConfigFailed` — the `fontSort` call did not return `.match`.
        FontConfigFailed,
        /// `Allocator.Error`.
        OutOfMemory,
        Face(FaceError),
        Collection(crate::collection::Error),
    }

    impl From<FaceError> for Error {
        fn from(e: FaceError) -> Self {
            Error::Face(e)
        }
    }

    impl From<crate::collection::Error> for Error {
        fn from(e: crate::collection::Error) -> Self {
            Error::Collection(e)
        }
    }

    /// The Fontconfig discoverer (Linux default backend).
    // pub const Fontconfig = struct { fc_config: *fontconfig.Config, ... };
    //
    // PORT NOTE: the reference's `fc_config: *fontconfig.Config` field lands with
    // the real discoverer (L3 Task 6/7); the stub is fieldless until then.
    pub struct Fontconfig;

    impl Fontconfig {
        // pub fn init(lib: Library) Fontconfig
        pub fn init(_lib: Library) -> Fontconfig {
            unimplemented!("Fontconfig discovery is DEFERRED to L3 Task 6/7")
        }

        // pub fn deinit(self: *Fontconfig) void
        pub fn deinit(&mut self) {
            unimplemented!("Fontconfig discovery is DEFERRED to L3 Task 6/7")
        }

        /// Discover fonts from a descriptor. This returns an iterator that can
        /// be used to build up the deferred fonts.
        // pub fn discover(self: *const Fontconfig, alloc: Allocator, desc: Descriptor) !DiscoverIterator
        //
        // PORT NOTE: the reference threads `alloc`; rustty uses the global
        // allocator, so `alloc` is omitted (matching `CoreText::discover`).
        pub fn discover(&self, _desc: Descriptor) -> Result<DiscoverIterator, Error> {
            unimplemented!("Fontconfig discovery is DEFERRED to L3 Task 6/7")
        }

        // pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator
        pub fn discover_fallback(
            &self,
            _collection: *mut Collection,
            _desc: Descriptor,
        ) -> Result<DiscoverIterator, Error> {
            unimplemented!("Fontconfig discovery is DEFERRED to L3 Task 6/7")
        }
    }

    /// The Fontconfig discovery iterator.
    // pub const DiscoverIterator = struct { config, pattern, set, fonts, variations, i };
    //
    // PORT NOTE: the reference's fields are all `*fontconfig.*` handles; they land
    // with the real discoverer (L3 Task 6/7).
    pub struct DiscoverIterator;

    impl DiscoverIterator {
        // pub fn deinit(self: *DiscoverIterator) void
        pub fn deinit(&mut self) {
            unimplemented!("Fontconfig discovery is DEFERRED to L3 Task 6/7")
        }

        // pub fn next(self: *DiscoverIterator) fontconfig.Error!?DeferredFace
        // (Named `next` to match the reference iterator; not `Iterator::next`,
        // whose signature — no `Result`, no external `deinit` — doesn't fit.)
        #[allow(clippy::should_implement_trait)]
        pub fn next(&mut self) -> Result<Option<DeferredFace>, Error> {
            unimplemented!("Fontconfig discovery is DEFERRED to L3 Task 6/7")
        }
    }
}

// ---------------------------------------------------------------------------
// WASM (web_canvas backend) discoverer — a functional NOOP.
//
// PORT NOTE: the reference `Discover` selector maps the `.web_canvas` backend to
// `void` ("no discovery" — see the `switch` comment above): the browser has no
// font-file discovery, fonts are named CSS strings resolved by the Canvas API.
// Rust cannot alias `Discover` to `void`/`()` because `CodepointResolver` and
// `SharedGridSet` name `Discover::init`/`.discover`/`.discover_fallback` and a
// `DiscoverIterator{next,deinit}` unconditionally (they are comptime-elided in
// the reference when `Discover == void`, but Rust type-checks them on every
// target). So on wasm `Discover` is a concrete NOOP discoverer whose iterators
// always yield `None` — behaviourally identical to the reference's "no discovery"
// (`self.discover` is always `null` in practice, so the resolver's discovery
// branch short-circuits before calling these). This is the faithful modelling of
// the `.web_canvas => void` arm under Rust's non-lazy type system.
#[cfg(target_arch = "wasm32")]
pub use wasm_noop::{DiscoverIterator, Error, Fontconfig};

#[cfg(target_arch = "wasm32")]
mod wasm_noop {
    use super::Descriptor;
    use crate::collection::Collection;
    use crate::deferred_face::DeferredFace;
    use crate::face::FaceError;
    use crate::library::Library;

    /// NOOP discoverer for the wasm `web_canvas` backend (`.web_canvas => void`).
    pub struct Fontconfig;

    /// The (never-produced) discovery error set, mirroring the macOS `Error`
    /// shape enough that `SharedGridSet`'s `From<discovery::Error>` composes. The
    /// NOOP never errors, so no variant is ever constructed.
    #[derive(Debug)]
    pub enum Error {
        Face(FaceError),
        Collection(crate::collection::Error),
    }

    impl From<FaceError> for Error {
        fn from(e: FaceError) -> Self {
            Error::Face(e)
        }
    }
    impl From<crate::collection::Error> for Error {
        fn from(e: crate::collection::Error) -> Self {
            Error::Collection(e)
        }
    }

    impl Fontconfig {
        // pub fn init(lib: Library) Fontconfig
        pub fn init(_lib: Library) -> Fontconfig {
            Fontconfig
        }

        // pub fn deinit(self: *Fontconfig) void
        pub fn deinit(&mut self) {}

        // pub fn discover(self, alloc, desc) !DiscoverIterator
        pub fn discover(&self, _desc: Descriptor) -> Result<DiscoverIterator, Error> {
            Ok(DiscoverIterator)
        }

        // pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator
        pub fn discover_fallback(
            &self,
            _collection: *mut Collection,
            _desc: Descriptor,
        ) -> Result<DiscoverIterator, Error> {
            Ok(DiscoverIterator)
        }
    }

    /// NOOP discovery iterator: the browser exposes no enumerable font list, so
    /// this always yields `None` (no fallback faces).
    pub struct DiscoverIterator;

    impl DiscoverIterator {
        // pub fn deinit(self: *DiscoverIterator) void
        pub fn deinit(&mut self) {}

        // pub fn next(self: *DiscoverIterator) !?DeferredFace
        pub fn next(&mut self) -> Result<Option<DeferredFace>, Error> {
            Ok(None)
        }
    }
}

/// Descriptor is used to search for fonts. The only required field
/// is "family". The rest are ignored unless they're set to a non-zero
/// value.
// pub const Descriptor = struct { ... };
#[derive(Clone, Debug, Default)]
pub struct Descriptor {
    /// Font family to search for. This can be a fully qualified font
    /// name such as "Fira Code", "monospace", "serif", etc. Memory is
    /// owned by the caller and should be freed when this descriptor
    /// is no longer in use. The discovery structs will never store the
    /// descriptor.
    ///
    /// On systems that use fontconfig (Linux), this can be a full
    /// fontconfig pattern, such as "Fira Code-14:bold".
    // family: ?[:0]const u8 = null,
    pub family: Option<Vec<u8>>,

    /// Specific font style to search for. This will filter the style
    /// string the font advertises. The "bold/italic" booleans later in this
    /// struct filter by the style trait the font has, not the string, so
    /// these can be used in conjunction or not.
    // style: ?[:0]const u8 = null,
    pub style: Option<Vec<u8>>,

    /// A codepoint that this font must be able to render.
    // codepoint: u32 = 0,
    pub codepoint: u32,

    /// Font size in points that the font should support. For conversion
    /// to pixels, we will use 72 DPI for Mac and 96 DPI for everything else.
    /// (If pixel conversion is necessary, i.e. emoji fonts)
    // size: f32 = 0,
    pub size: f32,

    /// True if we want to search specifically for a font that supports
    /// specific styles.
    // bold: bool = false,
    pub bold: bool,
    // italic: bool = false,
    pub italic: bool,
    // monospace: bool = false,
    pub monospace: bool,

    /// Variation axes to apply to the font. This also impacts searching
    /// for fonts since fonts with the ability to set these variations
    /// will be preferred, but not guaranteed.
    // variations: []const Variation = &.{},
    pub variations: Vec<Variation>,
}

impl Descriptor {
    /// Hash the descriptor with the given hasher.
    // pub fn hash(self: Descriptor, hasher: anytype) void {
    //
    // (Named `hash` to match the reference; not the std `Hash::hash` trait method.)
    #[allow(clippy::should_implement_trait)]
    pub fn hash<H: core::hash::Hasher>(&self, hasher: &mut H) {
        use core::hash::Hash;
        // const autoHash = std.hash.autoHash;
        // const autoHashStrat = std.hash.autoHashStrat;
        // autoHashStrat(hasher, self.family, .Deep);
        self.family.hash(hasher);
        // autoHashStrat(hasher, self.style, .Deep);
        self.style.hash(hasher);
        // autoHash(hasher, self.codepoint);
        self.codepoint.hash(hasher);
        // autoHash(hasher, @as(u32, @bitCast(self.size)));
        self.size.to_bits().hash(hasher);
        // autoHash(hasher, self.bold);
        self.bold.hash(hasher);
        // autoHash(hasher, self.italic);
        self.italic.hash(hasher);
        // autoHash(hasher, self.monospace);
        self.monospace.hash(hasher);
        // autoHash(hasher, self.variations.len);
        self.variations.len().hash(hasher);
        // for (self.variations) |variation| {
        for variation in &self.variations {
            // autoHash(hasher, variation.id);
            variation.id.to_u32().hash(hasher);

            // This is not correct, but we don't currently depend on the
            // hash value being different based on decimal values of variations.
            // autoHash(hasher, @as(i64, @intFromFloat(variation.value)));
            (variation.value as i64).hash(hasher);
        }
    }

    /// Returns a hash code that can be used to uniquely identify this
    /// action.
    // pub fn hashcode(self: Descriptor) u64 {
    pub fn hashcode(&self) -> u64 {
        // var hasher = std.hash.Wyhash.init(0);
        //
        // PORT NOTE: the reference seeds `std.hash.Wyhash`; rustty has no Wyhash
        // port in this tier, so the stdlib `DefaultHasher` is used. The descriptor
        // cache keyed by this hashcode is an internal cache key (never serialized
        // or compared cross-implementation), so the exact hash algorithm is not
        // observable; the ported tests only assert non-zero / inequality. A full
        // Wyhash port lands if a differential cache-key comparison ever needs it.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // self.hash(&hasher);
        self.hash(&mut hasher);
        // return hasher.final();
        core::hash::Hasher::finish(&hasher)
    }

    /// Deep copy of the struct. The given allocator is expected to
    /// be an arena allocator of some sort since the descriptor
    /// itself doesn't support fine-grained deallocation of fields.
    // pub fn clone(self: *const Descriptor, alloc: Allocator) !Descriptor {
    //
    // PORT NOTE: rustty uses owned `Vec` fields, so `clone` is the derived deep
    // clone; `alloc` is not threaded (fields free themselves).
    //
    // We can't do any errdefer cleanup in here. As documented we
    // expect the allocator to be an arena so any errors should be
    // cleaned up somewhere else.
    pub fn clone_desc(&self) -> Descriptor {
        // var copy = self.*;
        // copy.family = if (self.family) |src| try alloc.dupeZ(u8, src) else null;
        // copy.style = if (self.style) |src| try alloc.dupeZ(u8, src) else null;
        // copy.variations = try alloc.dupe(Variation, self.variations);
        // return copy;
        self.clone()
    }

    /// Convert to Fontconfig pattern to use for lookup. The pattern does
    /// not have defaults filled/substituted (Fontconfig thing) so callers
    /// must still do this.
    ///
    /// The returned raw pointer is owned by the caller (matching the reference's
    /// manually managed `*fontconfig.Pattern`); the caller must `destroy` it.
    // PORT NOTE: `discovery.zig:117-157` `toFcPattern`. `assert = quirks.inlineAssert`
    // (imported crate-wide as in `face/freetype.rs:41`); each `pat.add`/`cs.addChar`
    // is `assert`ed exactly as the reference. Gated to the fontconfig backend class
    // (`not(any(macos, wasm32))`) — the coretext path uses `to_coretext_descriptor`,
    // and wasm has no discovery.
    // pub fn toFcPattern(self: Descriptor) *fontconfig.Pattern {
    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    pub fn to_fc_pattern(&self) -> *mut crate::fc::Pattern {
        use crate::fc;
        use rustty_util::quirks::inlineAssert as assert;
        use std::ffi::CString;

        // const pat = fontconfig.Pattern.create();
        let pat_ptr = fc::Pattern::create();
        // SAFETY: `create` returns a freshly allocated, non-null pattern which this
        // function owns and hands back to the caller.
        let pat = unsafe { &mut *pat_ptr };

        // if (self.family) |family| {
        //     assert(pat.add(.family, .{ .string = family }, false));
        // }
        if let Some(family) = &self.family {
            // The reference `family` is a `[:0]const u8` (a C string); rustty stores
            // the raw bytes, so NUL-terminate here. Interior NUL violates the field
            // contract (a fontconfig family/pattern string).
            let family = CString::new(family.as_slice())
                .expect("Descriptor::family must not contain an interior NUL");
            assert(pat.add(fc::Property::Family, fc::Value::String(&family), false));
        }

        // if (self.style) |style| {
        //     assert(pat.add(.style, .{ .string = style }, false));
        // }
        if let Some(style) = &self.style {
            let style = CString::new(style.as_slice())
                .expect("Descriptor::style must not contain an interior NUL");
            assert(pat.add(fc::Property::Style, fc::Value::String(&style), false));
        }

        // if (self.codepoint > 0) {
        //     const cs = fontconfig.CharSet.create();
        //     defer cs.destroy();
        //     assert(cs.addChar(self.codepoint));
        //     assert(pat.add(.charset, .{ .char_set = cs }, false));
        // }
        if self.codepoint > 0 {
            let cs_ptr = fc::CharSet::create();
            // SAFETY: `create` returns a freshly allocated, non-null char set.
            let cs = unsafe { &mut *cs_ptr };
            assert(cs.add_char(self.codepoint));
            // `add` (FcPatternAdd) copies the char set into pattern-owned memory, so
            // `cs` is destroyed below (the reference's `defer cs.destroy()`).
            assert(pat.add(
                fc::Property::Charset,
                fc::Value::CharSet(cs_ptr.cast_const()),
                false,
            ));
            cs.destroy();
        }

        // if (self.size > 0) assert(pat.add(
        //     .size,
        //     .{ .integer = @intFromFloat(@round(self.size)) },
        //     false,
        // ));
        if self.size > 0.0 {
            assert(pat.add(
                fc::Property::Size,
                fc::Value::Integer(self.size.round() as i32),
                false,
            ));
        }

        // if (self.bold) assert(pat.add(
        //     .weight,
        //     .{ .integer = @intFromEnum(fontconfig.Weight.bold) },
        //     false,
        // ));
        if self.bold {
            assert(pat.add(
                fc::Property::Weight,
                fc::Value::Integer(fc::Weight::Bold as i32),
                false,
            ));
        }

        // if (self.italic) assert(pat.add(
        //     .slant,
        //     .{ .integer = @intFromEnum(fontconfig.Slant.italic) },
        //     false,
        // ));
        if self.italic {
            assert(pat.add(
                fc::Property::Slant,
                fc::Value::Integer(fc::Slant::Italic as i32),
                false,
            ));
        }

        // For fontconfig, we always add monospace in the pattern. Since
        // fontconfig sorts by closeness to the pattern, this doesn't fully
        // exclude non-monospace but helps prefer it.
        // assert(pat.add(
        //     .spacing,
        //     .{ .integer = @intFromEnum(fontconfig.Spacing.mono) },
        //     false,
        // ));
        assert(pat.add(
            fc::Property::Spacing,
            fc::Value::Integer(fc::Spacing::Mono as i32),
            false,
        ));

        // return pat;
        pat_ptr
    }
}

/// Bridge a `CodepointMap` descriptor (the thin seam in `rustty-font`, carrying
/// only `family`/`monospace`) into a full discovery `Descriptor`. The reference
/// `CodepointMap.get` returns a `discovery.Descriptor` directly (they are the same
/// type); rustty seams the below-vt `CodepointMap` with a two-field descriptor, so
/// this bridge fills the remaining defaults.
impl From<rustty_font::codepoint_map::Descriptor> for Descriptor {
    fn from(d: rustty_font::codepoint_map::Descriptor) -> Self {
        Descriptor {
            family: d.family,
            monospace: d.monospace,
            ..Descriptor::default()
        }
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE (DEFERRED — Fontconfig backend): `pub const Fontconfig = struct { ... }`
// is the Linux-default discoverer. It lands with the Fontconfig backend; recorded
// here verbatim:
//
// pub const Fontconfig = struct {
//     fc_config: *fontconfig.Config,
//
//     pub fn init(lib: Library) Fontconfig {
//         _ = lib;
//         // safe to call multiple times and concurrently
//         _ = fontconfig.init();
//         return .{ .fc_config = fontconfig.initLoadConfigAndFonts() };
//     }
//
//     pub fn deinit(self: *Fontconfig) void {
//         self.fc_config.destroy();
//     }
//
//     /// Discover fonts from a descriptor. This returns an iterator that can
//     /// be used to build up the deferred fonts.
//     pub fn discover(self, alloc, desc) !DiscoverIterator {
//         _ = alloc;
//         // Build our pattern that we'll search for
//         const pat = desc.toFcPattern();
//         errdefer pat.destroy();
//         assert(self.fc_config.substituteWithPat(pat, .pattern));
//         pat.defaultSubstitute();
//         // Search
//         const res = self.fc_config.fontSort(pat, false, null);
//         if (res.result != .match) return error.FontConfigFailed;
//         errdefer res.fs.destroy();
//         return .{
//             .config = self.fc_config,
//             .pattern = pat,
//             .set = res.fs,
//             .fonts = res.fs.fonts(),
//             .variations = desc.variations,
//             .i = 0,
//         };
//     }
//
//     pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator {
//         _ = collection;
//         return try self.discover(alloc, desc);
//     }
//
//     pub const DiscoverIterator = struct {
//         config: *fontconfig.Config,
//         pattern: *fontconfig.Pattern,
//         set: *fontconfig.FontSet,
//         fonts: []*fontconfig.Pattern,
//         variations: []const Variation,
//         i: usize,
//
//         pub fn deinit(self: *DiscoverIterator) void {
//             self.set.destroy();
//             self.pattern.destroy();
//             self.* = undefined;
//         }
//
//         pub fn next(self: *DiscoverIterator) fontconfig.Error!?DeferredFace {
//             if (self.i >= self.fonts.len) return null;
//             // Get the copied pattern from our fontset that has the
//             // attributes configured for rendering.
//             const font_pattern = try self.config.fontRenderPrepare(
//                 self.pattern,
//                 self.fonts[self.i],
//             );
//             errdefer font_pattern.destroy();
//             // Increment after we return
//             defer self.i += 1;
//             return DeferredFace{
//                 .fc = .{
//                     .pattern = font_pattern,
//                     .charset = (try font_pattern.get(.charset, 0)).char_set,
//                     .langset = (try font_pattern.get(.lang, 0)).lang_set,
//                     .variations = self.variations,
//                 },
//             };
//         }
//     };
// };
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub use coretext_impl::{CoreText, DiscoverIterator, Error};

#[cfg(target_os = "macos")]
mod coretext_impl {
    use super::Descriptor;
    use crate::collection::{Collection, Index};
    use crate::deferred_face::{new_cfstring_bytes, CoreTextData, DeferredFace};
    use crate::face::coretext::copy_table_data;
    use crate::face::{Face, FaceError, Variation};
    use crate::library::Library;
    use crate::Style;
    use core::ffi::c_void;
    use core::ptr::{self, NonNull};
    use objc2_core_foundation::{
        kCFNull, kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks,
        kCFTypeDictionaryValueCallBacks, CFArray, CFCharacterSet, CFDictionary,
        CFMutableDictionary, CFNumber, CFNumberType, CFRange, CFRetained, CFString, CFType,
    };
    use objc2_core_graphics::CGGlyph;
    use objc2_core_text::{
        kCTFontCharacterSetAttribute, kCTFontFamilyNameAttribute, kCTFontSizeAttribute,
        kCTFontStyleNameAttribute, kCTFontSymbolicTrait, kCTFontTraitsAttribute,
        kCTFontVariationAttribute, kCTFontVariationAxesAttribute,
        kCTFontVariationAxisDefaultValueKey, kCTFontVariationAxisIdentifierKey,
        kCTFontVariationAxisNameKey, CTFont, CTFontCollection, CTFontDescriptor,
        CTFontSymbolicTraits,
    };
    use rustty_font::opentype::head::Head;
    use rustty_font::opentype::os2::OS2;
    use rustty_util::quirks::inlineAssert as assert;

    /// The error set for CoreText discovery.
    // PORT NOTE: the reference errors are the inferred union of the CoreFoundation
    // allocation failures, `Allocator.Error`, `error.CodepointTooLarge` (utf8
    // encode), and the backend `Face` errors threaded through `getFace`. Modeled
    // as one enum with a `From<FaceError>` conversion so `?` composes them.
    #[derive(Debug)]
    pub enum Error {
        CoreTextFailure,
        OutOfMemory,
        CodepointTooLarge,
        Face(FaceError),
        Collection(crate::collection::Error),
    }

    impl From<FaceError> for Error {
        fn from(e: FaceError) -> Self {
            Error::Face(e)
        }
    }

    // `discoverCodepoint` threads `collection.getFace`'s error into the inferred
    // error set; here that backend-collection error composes via `?`.
    impl From<crate::collection::Error> for Error {
        fn from(e: crate::collection::Error) -> Self {
            Error::Collection(e)
        }
    }

    /// Raw CoreText entry points that objc2 0.3 wraps as non-null (they can
    /// legitimately return NULL, which the reference handles with `orelse`).
    // PORT NOTE: `CTFontCreateForString` is documented as returning the best
    // substitute font, but objc2's wrapper `CTFont::for_string` marks it non-null
    // and panics on NULL. The reference's `original.font.createForString(...) orelse
    // return null` relies on the nullable result, so the raw symbol is declared here
    // against the SAME framework objc2 links (R3 — a binding, not a reimplementation).
    mod sys {
        use super::*;
        #[link(name = "CoreText", kind = "framework")]
        extern "C-unwind" {
            pub fn CTFontCreateForString(
                current_font: &CTFont,
                string: &CFString,
                range: CFRange,
            ) -> *mut CTFont;
        }
    }

    /// Create a fresh empty mutable `CFDictionary` with CoreFoundation type
    /// key/value callbacks (retain/release/equal/hash for CF objects).
    // PORT NOTE: `macos.foundation.MutableDictionary.create(0)`.
    fn new_mutable_dictionary() -> CFRetained<CFMutableDictionary> {
        unsafe {
            CFMutableDictionary::new(
                None,
                0,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            )
        }
        .expect("CFDictionaryCreateMutable returned NULL")
    }

    /// Set `attrs[key] = value` where both are CoreFoundation objects.
    // PORT NOTE: `attrs.setValue(key, value)`.
    fn dict_set(attrs: &CFMutableDictionary, key: &CFString, value: &CFType) {
        unsafe {
            CFMutableDictionary::set_value(
                Some(attrs),
                key as *const CFString as *const c_void,
                value as *const CFType as *const c_void,
            );
        }
    }

    /// Look up `dict[key]` as a borrowed CF object (no retain), or `None`.
    // PORT NOTE: `dict.getValue(T, key)` — CFDictionaryGetValue returns a borrowed ref.
    unsafe fn dict_get(dict: &CFDictionary, key: *const c_void) -> Option<&CFType> {
        let p = unsafe { dict.value(key) };
        if p.is_null() {
            None
        } else {
            Some(unsafe { &*(p as *const CFType) })
        }
    }

    /// Read a CFNumber as an f64 (`getValue(.double, ...)`).
    fn number_as_f64(num: &CFNumber) -> f64 {
        let mut out: f64 = 0.0;
        unsafe {
            num.value(
                CFNumberType::DoubleType,
                &mut out as *mut f64 as *mut c_void,
            );
        }
        out
    }

    /// ASCII case-insensitive equality (`std.ascii.eqlIgnoreCase`).
    fn ascii_eql_ignore_case(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
    }

    /// ASCII case-insensitive substring search (`std.ascii.indexOfIgnoreCase`).
    fn ascii_index_of_ignore_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            return Some(0);
        }
        if needle.len() > haystack.len() {
            return None;
        }
        (0..=haystack.len() - needle.len())
            .find(|&i| haystack[i..i + needle.len()].eq_ignore_ascii_case(needle))
    }

    /// UTF-32 -> UTF-16 surrogate decomposition, matching CoreFoundation
    /// (`macos.foundation.stringGetSurrogatePairForLongCharacter`).
    fn string_get_surrogate_pair_for_long_character(
        character: u32,
        surrogates: &mut [u16; 2],
    ) -> bool {
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

    impl Descriptor {
        /// Convert to Core Text font descriptor to use for lookup or
        /// conversion to a specific font.
        // pub fn toCoreTextDescriptor(self: Descriptor) !*macos.text.FontDescriptor {
        pub fn to_core_text_descriptor(&self) -> Result<CFRetained<CTFontDescriptor>, Error> {
            // const attrs = try macos.foundation.MutableDictionary.create(0);
            let attrs = new_mutable_dictionary();
            // defer attrs.release(); — `attrs` is `CFRetained` and releases on drop.

            // Family
            // if (self.family) |family_bytes| {
            if let Some(family_bytes) = &self.family {
                // const family = try macos.foundation.String.createWithBytes(family_bytes, .utf8, false);
                let family = new_cfstring_bytes(family_bytes).ok_or(Error::OutOfMemory)?;
                // defer family.release();
                // attrs.setValue(macos.text.FontAttribute.family_name.key(), family);
                dict_set(&attrs, unsafe { kCTFontFamilyNameAttribute }, &family);
            }

            // Style
            // if (self.style) |style_bytes| {
            if let Some(style_bytes) = &self.style {
                // const style = try macos.foundation.String.createWithBytes(style_bytes, .utf8, false);
                let style = new_cfstring_bytes(style_bytes).ok_or(Error::OutOfMemory)?;
                // defer style.release();
                // attrs.setValue(macos.text.FontAttribute.style_name.key(), style);
                dict_set(&attrs, unsafe { kCTFontStyleNameAttribute }, &style);
            }

            // Codepoint support
            // if (self.codepoint > 0) {
            if self.codepoint > 0 {
                // const cs = try macos.foundation.CharacterSet.createWithCharactersInRange(.{
                //     .location = self.codepoint,
                //     .length = 1,
                // });
                let cs = unsafe {
                    CFCharacterSet::with_characters_in_range(
                        None,
                        CFRange {
                            location: self.codepoint as isize,
                            length: 1,
                        },
                    )
                }
                .ok_or(Error::OutOfMemory)?;
                // defer cs.release();
                // attrs.setValue(macos.text.FontAttribute.character_set.key(), cs);
                dict_set(&attrs, unsafe { kCTFontCharacterSetAttribute }, &cs);
            }

            // Set our size attribute if set
            // if (self.size > 0) {
            if self.size > 0.0 {
                // const size32: i32 = @intFromFloat(@round(self.size));
                let size32: i32 = self.size.round() as i32;
                // const size = try macos.foundation.Number.create(.sint32, &size32);
                let size = unsafe {
                    CFNumber::new(
                        None,
                        CFNumberType::SInt32Type,
                        &size32 as *const i32 as *const c_void,
                    )
                }
                .ok_or(Error::OutOfMemory)?;
                // defer size.release();
                // attrs.setValue(macos.text.FontAttribute.size.key(), size);
                dict_set(&attrs, unsafe { kCTFontSizeAttribute }, &size);
            }

            // Build our traits. If we set any, then we store it in the attributes
            // otherwise we do nothing. We determine this by setting up the packed
            // struct, converting to an int, and checking if it is non-zero.
            // const traits: macos.text.FontSymbolicTraits = .{
            //     .bold = self.bold, .italic = self.italic, .monospace = self.monospace,
            // };
            // const traits_cval: u32 = @bitCast(traits);
            let mut traits_cval: u32 = 0;
            if self.bold {
                traits_cval |= CTFontSymbolicTraits::TraitBold.0;
            }
            if self.italic {
                traits_cval |= CTFontSymbolicTraits::TraitItalic.0;
            }
            if self.monospace {
                traits_cval |= CTFontSymbolicTraits::TraitMonoSpace.0;
            }
            // if (traits_cval > 0) {
            if traits_cval > 0 {
                // Setting traits is a pain. We have to create a nested dictionary
                // of the symbolic traits value, and set that in our attributes.
                // const traits_num = try macos.foundation.Number.create(
                //     .sint32,
                //     @as(*const i32, @ptrCast(&traits_cval)),
                // );
                let traits_num = unsafe {
                    CFNumber::new(
                        None,
                        CFNumberType::SInt32Type,
                        &traits_cval as *const u32 as *const c_void,
                    )
                }
                .ok_or(Error::OutOfMemory)?;
                // defer traits_num.release();

                // const traits_dict = try macos.foundation.MutableDictionary.create(0);
                let traits_dict = new_mutable_dictionary();
                // defer traits_dict.release();
                // traits_dict.setValue(macos.text.FontTraitKey.symbolic.key(), traits_num);
                dict_set(&traits_dict, unsafe { kCTFontSymbolicTrait }, &traits_num);

                // attrs.setValue(macos.text.FontAttribute.traits.key(), traits_dict);
                dict_set(&attrs, unsafe { kCTFontTraitsAttribute }, &traits_dict);
            }

            // return try macos.text.FontDescriptor.createWithAttributes(@ptrCast(attrs));
            Ok(unsafe { CTFontDescriptor::with_attributes(&attrs) })
        }
    }

    /// The CoreText discoverer. Holds no state.
    // pub const CoreText = struct { ... };
    pub struct CoreText;

    impl CoreText {
        // pub fn init(lib: Library) CoreText {
        pub fn init(_lib: Library) -> CoreText {
            // _ = lib;
            // Required for the "interface" but does nothing for CoreText.
            CoreText
        }

        // pub fn deinit(self: *CoreText) void { _ = self; }
        pub fn deinit(&mut self) {}

        /// Discover fonts from a descriptor. This returns an iterator that can
        /// be used to build up the deferred fonts.
        // pub fn discover(self: *const CoreText, alloc: Allocator, desc: Descriptor) !DiscoverIterator {
        //
        // PORT NOTE: the reference threads `alloc` for the matching-descriptor
        // slice; rustty uses a `Vec` (global allocator), so `alloc` is omitted.
        pub fn discover(&self, desc: Descriptor) -> Result<DiscoverIterator, Error> {
            // _ = self;

            // Build our pattern that we'll search for
            // const ct_desc = try desc.toCoreTextDescriptor();
            let ct_desc = desc.to_core_text_descriptor()?;
            // defer ct_desc.release();

            // Our descriptors have to be in an array
            // var ct_desc_arr = [_]*const macos.text.FontDescriptor{ct_desc};
            // const desc_arr = try macos.foundation.Array.create(macos.text.FontDescriptor, &ct_desc_arr);
            let mut ct_desc_arr: [*const c_void; 1] =
                [&*ct_desc as *const CTFontDescriptor as *const c_void];
            let desc_arr =
                unsafe { CFArray::new(None, ct_desc_arr.as_mut_ptr(), 1, &kCFTypeArrayCallBacks) }
                    .ok_or(Error::CoreTextFailure)?;
            // defer desc_arr.release();

            // Build our collection
            // const set = try macos.text.FontCollection.createWithFontDescriptors(desc_arr);
            let set = unsafe { CTFontCollection::with_font_descriptors(Some(&desc_arr), None) };
            // defer set.release();
            // const list = set.createMatchingFontDescriptors();
            let list = unsafe { set.matching_font_descriptors() };
            // defer list.release();

            // Sort our descriptors
            // const zig_list = try copyMatchingDescriptors(alloc, list);
            let zig_list = copy_matching_descriptors(list);
            // errdefer alloc.free(zig_list);
            // sortMatchingDescriptors(&desc, zig_list);
            let zig_list = sort_matching_descriptors(&desc, zig_list);

            Ok(DiscoverIterator {
                list: zig_list,
                variations: desc.variations,
                i: 0,
            })
        }

        // pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator {
        pub fn discover_fallback(
            &self,
            collection: *mut Collection,
            desc: Descriptor,
        ) -> Result<DiscoverIterator, Error> {
            // If we have a codepoint within the CJK unified ideographs block
            // then we fallback to macOS to find a font that supports it because
            // there isn't a better way manually with CoreText that I can find that
            // properly takes into account system locale.
            //
            // References:
            // - http://unicode.org/charts/PDF/U4E00.pdf
            // - https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/fonts/LocaleInFonts.md#unified-han-ideographs
            // if (desc.codepoint >= 0x4E00 and desc.codepoint <= 0x9FFF) han: {
            if desc.codepoint >= 0x4E00 && desc.codepoint <= 0x9FFF {
                // const han = try self.discoverCodepoint(collection, desc) orelse break :han;
                if let Some(han) = self.discover_codepoint(collection, &desc)? {
                    // This is silly but our discover iterator needs a slice so
                    // we allocate here. This isn't a performance bottleneck but
                    // this is something we can optimize very easily...
                    // const list = try alloc.alloc(*macos.text.FontDescriptor, 1);
                    // errdefer alloc.free(list);
                    // list[0] = han;
                    let list = vec![han];

                    return Ok(DiscoverIterator {
                        list,
                        variations: desc.variations,
                        i: 0,
                    });
                }
            }

            // const it = try self.discover(alloc, desc);
            let it = self.discover(desc.clone())?;

            // If our normal discovery doesn't find anything and we have a specific
            // codepoint, then fallback to using CTFontCreateForString to find a
            // matching font CoreText wants to use. See:
            // https://github.com/ghostty-org/ghostty/issues/2499
            // if (it.list.len == 0 and desc.codepoint > 0) codepoint: {
            if it.list.is_empty() && desc.codepoint > 0 {
                // const ct_desc = try self.discoverCodepoint(collection, desc) orelse break :codepoint;
                if let Some(ct_desc) = self.discover_codepoint(collection, &desc)? {
                    // const list = try alloc.alloc(*macos.text.FontDescriptor, 1);
                    // errdefer alloc.free(list);
                    // list[0] = ct_desc;
                    let list = vec![ct_desc];

                    return Ok(DiscoverIterator {
                        list,
                        variations: desc.variations,
                        i: 0,
                    });
                }
            }

            // return it;
            Ok(it)
        }

        /// Discover a font for a specific codepoint using the CoreText
        /// CTFontCreateForString API.
        // fn discoverCodepoint(self, collection: *Collection, desc: Descriptor) !?*macos.text.FontDescriptor {
        fn discover_codepoint(
            &self,
            collection: *mut Collection,
            desc: &Descriptor,
        ) -> Result<Option<CFRetained<CTFontDescriptor>>, Error> {
            // _ = self;

            // if (comptime options.backend.hasFreetype()) {
            //     // If we have freetype, we can't use CoreText to find a font
            //     // that supports a specific codepoint because we need to
            //     // have a CoreText font to be able to do so.
            //     return null;
            // }
            //
            // PORT NOTE: on the macOS default `coretext` backend `hasFreetype()`
            // is false, so this early-return does not apply and is a no-op here.

            // assert(desc.codepoint > 0);
            assert(desc.codepoint > 0);

            // SAFETY: `collection` is an externally-owned collection pointer.
            let collection = unsafe { &mut *collection };

            // Get our original font. This is dependent on the requested style
            // from the descriptor.
            // const original = original: {
            //     // In all the styles below, we try to match it but if we don't
            //     // we always fall back to some other option. The order matters
            //     // here.
            let original: *mut Face = {
                // if (desc.bold and desc.italic) {
                //     const entries = collection.faces.get(.bold_italic);
                //     if (entries.count() > 0) {
                //         break :original try collection.getFace(.{ .style = .bold_italic });
                //     }
                // }
                if desc.bold && desc.italic && collection.style_face_count(Style::BoldItalic) > 0 {
                    collection.get_face(Index::new(Style::BoldItalic, 0))?
                }
                // if (desc.bold) {
                //     const entries = collection.faces.get(.bold);
                //     if (entries.count() > 0) {
                //         break :original try collection.getFace(.{ .style = .bold });
                //     }
                // }
                else if desc.bold && collection.style_face_count(Style::Bold) > 0 {
                    collection.get_face(Index::new(Style::Bold, 0))?
                }
                // if (desc.italic) {
                //     const entries = collection.faces.get(.italic);
                //     if (entries.count() > 0) {
                //         break :original try collection.getFace(.{ .style = .italic });
                //     }
                // }
                else if desc.italic && collection.style_face_count(Style::Italic) > 0 {
                    collection.get_face(Index::new(Style::Italic, 0))?
                }
                // break :original try collection.getFace(.{ .style = .regular });
                else {
                    collection.get_face(Index::new(Style::Regular, 0))?
                }
            };

            // We need it in utf8 format
            // var buf: [4]u8 = undefined;
            // const len = try std.unicode.utf8Encode(@intCast(desc.codepoint), &buf);
            let ch = char::from_u32(desc.codepoint).ok_or(Error::CodepointTooLarge)?;
            let mut buf = [0u8; 4];
            let encoded = ch.encode_utf8(&mut buf);
            let len = encoded.len();

            // We need a CFString
            // const str = try macos.foundation.String.createWithBytes(buf[0..len], .utf8, false);
            let str = new_cfstring_bytes(&buf[0..len]).ok_or(Error::OutOfMemory)?;
            // defer str.release();

            // Get our range length for CTFontCreateForString. It looks like
            // the range uses UTF-16 codepoints and not UTF-32 codepoints.
            // const range_len: usize = range_len: {
            //     var unichars: [2]u16 = undefined;
            //     const pair = macos.foundation.stringGetSurrogatePairForLongCharacter(desc.codepoint, &unichars);
            //     break :range_len if (pair) 2 else 1;
            // };
            let range_len: isize = {
                let mut unichars: [u16; 2] = [0, 0];
                let pair =
                    string_get_surrogate_pair_for_long_character(desc.codepoint, &mut unichars);
                if pair {
                    2
                } else {
                    1
                }
            };

            // Get our font
            // const font = original.font.createForString(
            //     str,
            //     macos.foundation.Range.init(0, range_len),
            // ) orelse return null;
            let font_ptr = unsafe {
                sys::CTFontCreateForString(
                    &(*original).font,
                    &str,
                    CFRange {
                        location: 0,
                        length: range_len,
                    },
                )
            };
            let font: CFRetained<CTFont> = match NonNull::new(font_ptr) {
                Some(nn) => unsafe { CFRetained::from_raw(nn) },
                None => return Ok(None),
            };
            // defer font.release();

            // Do not allow the last resort font to go through. This is the
            // last font used by CoreText if it can't find anything else and
            // only contains replacement characters.
            // last_resort: {
            'last_resort: {
                // const name_str = font.copyPostScriptName();
                let name_str = unsafe { font.post_script_name() };
                // defer name_str.release();

                // If the name doesn't fit in our buffer, then it can't
                // be the last resort font so we break out.
                // var name_buf: [64]u8 = undefined;
                // const name: []const u8 = name_str.cstring(&name_buf, .utf8) orelse break :last_resort;
                let name = name_str.to_string();
                let name = name.as_bytes();
                // PORT NOTE: `cstring(&[64]u8, .utf8)` needs room for a trailing NUL,
                // so a name of >= 64 bytes returns null → break out (not last resort).
                if name.len() >= 64 {
                    break 'last_resort;
                }

                // If the name is "LastResort" then we don't want to use it.
                // if (std.mem.eql(u8, "LastResort", name)) return null;
                if b"LastResort" == name {
                    return Ok(None);
                }
            }

            // Get the descriptor
            // return font.copyDescriptor();
            Ok(Some(unsafe { font.font_descriptor() }))
        }
    }

    // fn copyMatchingDescriptors(alloc, list: *macos.foundation.Array) ![]*macos.text.FontDescriptor {
    fn copy_matching_descriptors(
        list: Option<CFRetained<CFArray>>,
    ) -> Vec<CFRetained<CTFontDescriptor>> {
        // var result = try alloc.alloc(*macos.text.FontDescriptor, list.getCount());
        // errdefer alloc.free(result);
        let mut result: Vec<CFRetained<CTFontDescriptor>> = Vec::new();
        // PORT NOTE: `createMatchingFontDescriptors` returns null when nothing
        // matches; the reference `list` is always non-null (empty array), so a
        // null result maps to the empty slice.
        let list = match list {
            Some(l) => l,
            None => return result,
        };
        // for (0..result.len) |i| {
        let count = CFArray::count(&list);
        for i in 0..count {
            // result[i] = list.getValueAtIndex(macos.text.FontDescriptor, i);
            let ptr = unsafe { CFArray::value_at_index(&list, i) };
            // We need to retain because once the list is freed it will
            // release all its members.
            // result[i].retain();
            let d: CFRetained<CTFontDescriptor> =
                unsafe { CFRetained::retain(NonNull::new_unchecked(ptr as *mut CTFontDescriptor)) };
            result.push(d);
        }
        // return result;
        result
    }

    // fn sortMatchingDescriptors(desc: *const Descriptor, list: []*macos.text.FontDescriptor) void {
    //
    // PORT NOTE: the reference sorts `list` in place with `std.mem.sortUnstable`,
    // whose comparator recomputes `Score.score` for each side on every comparison.
    // `score` is pure (deterministic), so rustty precomputes each descriptor's
    // score once and sorts the (score, descriptor) pairs — behavior-identical
    // ordering with O(n) scoring instead of O(n log n). Higher score sorts earlier.
    fn sort_matching_descriptors(
        desc: &Descriptor,
        list: Vec<CFRetained<CTFontDescriptor>>,
    ) -> Vec<CFRetained<CTFontDescriptor>> {
        let mut scored: Vec<(u32, CFRetained<CTFontDescriptor>)> = list
            .into_iter()
            .map(|d| (Score::score(desc, &d).int(), d))
            .collect();
        // fn lessThan(desc, lhs, rhs) bool {
        //     const lhs_score: Score = .score(desc, lhs);
        //     const rhs_score: Score = .score(desc, rhs);
        //     // Higher score is "less" (earlier)
        //     return lhs_score.int() > rhs_score.int();
        // }
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, d)| d).collect()
    }

    /// We represent our sorting score as a packed struct so that we
    /// can compare scores numerically but build scores symbolically.
    ///
    /// Note that packed structs store their fields from least to most
    /// significant, so the fields here are defined in increasing order
    /// of precedence.
    // const Score = packed struct {
    #[derive(Default)]
    struct Score {
        // const Backing = @typeInfo(@This()).@"struct".backing_integer.?;
        //
        // PORT NOTE: the fields sum to u16 + u8 + 5 bools = 29 bits, so the packed
        // backing integer is `u32`. `int()` reproduces that little-endian bit
        // layout (first field = least-significant bits).
        /// Number of glyphs in the font, if two fonts have identical
        /// scores otherwise then we prefer the one with more glyphs.
        ///
        /// (Number of glyphs clamped at u16 intmax)
        // glyph_count: u16 = 0,
        glyph_count: u16,
        /// A fuzzy match on the style string, less important than
        /// an exact match, and less important than trait matches.
        // fuzzy_style: u8 = 0,
        fuzzy_style: u8,
        /// Whether the bold-ness of the font matches the descriptor.
        /// This is less important than italic because a font that's italic
        /// when it shouldn't be or not italic when it should be is a bigger
        /// problem (subjectively) than being the wrong weight.
        // bold: bool = false,
        bold: bool,
        /// Whether the italic-ness of the font matches the descriptor.
        /// This is less important than an exact match on the style string
        /// because we want users to be allowed to override trait matching
        /// for the bold/italic/bold italic styles if they want.
        // italic: bool = false,
        italic: bool,
        /// An exact (case-insensitive) match on the style string.
        // exact_style: bool = false,
        exact_style: bool,
        /// Whether the font is monospace, this is more important than any of
        /// the other fields unless we're looking for a specific codepoint,
        /// in which case that is the most important thing.
        // monospace: bool = false,
        monospace: bool,
        /// If we're looking for a codepoint, whether this font has it.
        // codepoint: bool = false,
        codepoint: bool,
    }

    impl Score {
        // pub fn int(self: Score) Backing {
        //     return @bitCast(self);
        // }
        fn int(&self) -> u32 {
            (self.glyph_count as u32)
                | ((self.fuzzy_style as u32) << 16)
                | ((self.bold as u32) << 24)
                | ((self.italic as u32) << 25)
                | ((self.exact_style as u32) << 26)
                | ((self.monospace as u32) << 27)
                | ((self.codepoint as u32) << 28)
        }

        // fn score(desc: *const Descriptor, ct_desc: *const macos.text.FontDescriptor) Score {
        // (Named `score` to match the reference associated fn on `Score`; the
        // clippy `self_named_constructors` lint fires on the type/fn name match,
        // but this is not a constructor — it computes a score, not a `Score::new`.)
        #[allow(clippy::self_named_constructors)]
        fn score(desc: &Descriptor, ct_desc: &CTFontDescriptor) -> Score {
            // var self: Score = .{};
            let mut this = Score::default();

            // We always load the font if we can since some things can only be
            // inspected on the font itself. Fonts that can't be loaded score
            // 0 automatically because we don't want a font we can't load.
            // const font: *macos.text.Font = macos.text.Font.createWithFontDescriptor(ct_desc, 12) catch return self;
            //
            // PORT NOTE: objc2's `with_font_descriptor` returns a non-null font
            // (CoreText substitutes rather than failing), so the `catch return self`
            // 0-score path is effectively unreachable; the call cannot fail here.
            let font = unsafe { CTFont::with_font_descriptor(ct_desc, 12.0, ptr::null()) };
            // defer font.release();

            // We prefer fonts with more glyphs, all else being equal.
            {
                // const Type = @TypeOf(self.glyph_count); // u16
                // self.glyph_count = std.math.cast(Type, font.getGlyphCount()) orelse std.math.maxInt(Type);
                let gc = unsafe { font.glyph_count() };
                this.glyph_count = u16::try_from(gc).unwrap_or(u16::MAX);
            }

            // If we're searching for a codepoint, then we
            // prioritize fonts that have that codepoint.
            // if (desc.codepoint > 0) {
            if desc.codepoint > 0 {
                // Turn UTF-32 into UTF-16 for CT API
                // var unichars: [2]u16 = undefined;
                // const pair = macos.foundation.stringGetSurrogatePairForLongCharacter(desc.codepoint, &unichars);
                // const len: usize = if (pair) 2 else 1;
                let mut unichars: [u16; 2] = [0, 0];
                let pair =
                    string_get_surrogate_pair_for_long_character(desc.codepoint, &mut unichars);
                let len: usize = if pair { 2 } else { 1 };

                // Get our glyphs
                // var glyphs = [2]macos.graphics.Glyph{ 0, 0 };
                // self.codepoint = font.getGlyphsForCharacters(unichars[0..len], glyphs[0..len]);
                let mut glyphs = [0 as CGGlyph; 2];
                this.codepoint = unsafe {
                    font.glyphs_for_characters(
                        NonNull::new(unichars.as_mut_ptr()).unwrap(),
                        NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                        len as isize,
                    )
                };
            }

            // Get our symbolic traits for the descriptor so we can
            // compare boolean attributes like bold, monospace, etc.
            // const symbolic_traits: macos.text.FontSymbolicTraits = traits: {
            //     const traits = ct_desc.copyAttribute(.traits) orelse break :traits .{};
            //     defer traits.release();
            //     const key = macos.text.FontTraitKey.symbolic.key();
            //     const symbolic = traits.getValue(macos.foundation.Number, key) orelse break :traits .{};
            //     break :traits macos.text.FontSymbolicTraits.init(symbolic);
            // };
            let symbolic_traits: CTFontSymbolicTraits = 'traits: {
                let traits = match unsafe { ct_desc.attribute(kCTFontTraitsAttribute) } {
                    Some(t) => t,
                    None => break 'traits CTFontSymbolicTraits(0),
                };
                let traits = match traits.downcast_ref::<CFDictionary>() {
                    Some(d) => d,
                    None => break 'traits CTFontSymbolicTraits(0),
                };
                let key = unsafe { kCTFontSymbolicTrait };
                let symbolic =
                    match unsafe { dict_get(traits, key as *const CFString as *const c_void) }
                        .and_then(|v| v.downcast_ref::<CFNumber>())
                    {
                        Some(n) => n,
                        None => break 'traits CTFontSymbolicTraits(0),
                    };
                // FontSymbolicTraits.init(symbolic) — read the int value.
                let mut raw: i32 = 0;
                unsafe {
                    symbolic.value(CFNumberType::IntType, &mut raw as *mut i32 as *mut c_void);
                }
                break 'traits CTFontSymbolicTraits(raw as u32);
            };

            // self.monospace = symbolic_traits.monospace;
            this.monospace = symbolic_traits.0 & CTFontSymbolicTraits::TraitMonoSpace.0 != 0;

            // We try to derived data from the font itself, which is generally
            // more reliable than only using the symbolic traits for this.
            // const is_bold: bool, const is_italic: bool = derived: {
            let (is_bold, is_italic): (bool, bool) = 'derived: {
                // We start with initial guesses based on the symbolic traits,
                // but refine these with more information if we can get it.
                // var is_italic = symbolic_traits.italic;
                // var is_bold = symbolic_traits.bold;
                let mut is_italic = symbolic_traits.0 & CTFontSymbolicTraits::TraitItalic.0 != 0;
                let mut is_bold = symbolic_traits.0 & CTFontSymbolicTraits::TraitBold.0 != 0;

                // Read the 'head' table out of the font data if it's available.
                // if (head: {
                //     const tag = macos.text.FontTableTag.init("head");
                //     const data = font.copyTable(tag) orelse break :head null;
                //     defer data.release();
                //     const ptr = data.getPointer();
                //     const len = data.getLength();
                //     break :head opentype.Head.init(ptr[0..len]) catch |err| {
                //         log.warn("error parsing head table: {}", .{err});
                //         break :head null;
                //     };
                // }) |head_| {
                //     const head: opentype.Head = head_;
                //     is_bold = is_bold or (head.macStyle & 1 == 1);
                //     is_italic = is_italic or (head.macStyle & 2 == 2);
                // }
                let head_: Option<Head> = match copy_table_data(&font, b"head") {
                    None => None,
                    // PORT NOTE (log-as-comment): the reference's
                    // `catch |err| { log.warn("error parsing head table: {}", .{err}); break :head null; }`
                    // discards a parse failure as `null`; the warning is a no-op here.
                    Some(data) => Head::init(unsafe { data.as_bytes_unchecked() }).ok(),
                };
                if let Some(head) = head_ {
                    is_bold = is_bold || (head.macStyle & 1 == 1);
                    is_italic = is_italic || (head.macStyle & 2 == 2);
                }

                // Read the 'OS/2' table out of the font data if it's available.
                // if (os2: { ... opentype.OS2.init ... }) |os2| {
                //     is_bold = is_bold or os2.fsSelection.bold;
                //     is_italic = is_italic or os2.fsSelection.italic;
                // }
                let os2_: Option<OS2> = match copy_table_data(&font, b"OS/2") {
                    None => None,
                    // PORT NOTE (log-as-comment): `catch |err| { log.warn("error parsing OS/2 table: {}", .{err}); break :os2 null; }`.
                    Some(data) => OS2::init(unsafe { data.as_bytes_unchecked() }).ok(),
                };
                if let Some(os2) = os2_ {
                    is_bold = is_bold || os2.fsSelection.bold;
                    is_italic = is_italic || os2.fsSelection.italic;
                }

                // Check if we have variation axes in our descriptor, if we
                // do then we can derive weight italic-ness or both from them.
                // if (font.copyAttribute(.variation_axes)) |axes| variations: {
                if let Some(axes) = unsafe { font.attribute(kCTFontVariationAxesAttribute) } {
                    'variations: {
                        // defer axes.release();
                        let axes = match axes.downcast_ref::<CFArray>() {
                            Some(a) => a,
                            None => break 'variations,
                        };

                        // Copy the variation values for this instance of the font.
                        // if there are none then we just break out immediately.
                        // const values: *macos.foundation.Dictionary = font.copyAttribute(.variation) orelse break :variations;
                        // defer values.release();
                        let values_ret = match unsafe { font.attribute(kCTFontVariationAttribute) }
                        {
                            Some(v) => v,
                            None => break 'variations,
                        };
                        let values = match values_ret.downcast_ref::<CFDictionary>() {
                            Some(d) => d,
                            None => break 'variations,
                        };

                        // var buf: [1024]u8 = undefined;

                        // If we see the 'ital' value then we ignore 'slnt'.
                        // var ital_seen = false;
                        let mut ital_seen = false;

                        // const len = axes.getCount();
                        let len = CFArray::count(axes);
                        // for (0..len) |i| {
                        for i in 0..len {
                            // const dict = axes.getValueAtIndex(macos.foundation.Dictionary, i);
                            let dict_ptr = unsafe { CFArray::value_at_index(axes, i) };
                            if dict_ptr.is_null() {
                                continue;
                            }
                            let dict: &CFDictionary =
                                unsafe { &*(dict_ptr as *const CFDictionary) };
                            // const Key = macos.text.FontVariationAxisKey;
                            // const cf_id = dict.getValue(Key.identifier.Value(), Key.identifier.key()).?;
                            // const cf_name = dict.getValue(Key.name.Value(), Key.name.key()).?;
                            // const cf_def = dict.getValue(Key.default_value.Value(), Key.default_value.key()).?;
                            let cf_id = unsafe {
                                dict_get(
                                    dict,
                                    kCTFontVariationAxisIdentifierKey as *const CFString
                                        as *const c_void,
                                )
                            }
                            .expect("variation axis missing identifier");
                            let cf_name = unsafe {
                                dict_get(
                                    dict,
                                    kCTFontVariationAxisNameKey as *const CFString as *const c_void,
                                )
                            }
                            .and_then(|v| v.downcast_ref::<CFString>())
                            .expect("variation axis missing name");
                            let cf_def = unsafe {
                                dict_get(
                                    dict,
                                    kCTFontVariationAxisDefaultValueKey as *const CFString
                                        as *const c_void,
                                )
                            }
                            .and_then(|v| v.downcast_ref::<CFNumber>())
                            .expect("variation axis missing default value");

                            // const name_str = cf_name.cstring(&buf, .utf8) orelse "";
                            let name_string = cf_name.to_string();
                            // PORT NOTE: `cstring(&[1024]u8, .utf8)` returns "" when
                            // the name doesn't fit its buffer (needs a NUL byte).
                            let name_str: &[u8] = if name_string.len() < 1024 {
                                name_string.as_bytes()
                            } else {
                                b""
                            };

                            // Default value
                            // var def: f64 = 0;
                            // _ = cf_def.getValue(.double, &def);
                            let def: f64 = number_as_f64(cf_def);
                            // Value in this font
                            // var val: f64 = def;
                            // if (values.getValue(macos.foundation.Number, cf_id)) |cf_val| _ = cf_val.getValue(.double, &val);
                            let mut val: f64 = def;
                            if let Some(cf_val) =
                                unsafe { dict_get(values, cf_id as *const CFType as *const c_void) }
                                    .and_then(|v| v.downcast_ref::<CFNumber>())
                            {
                                val = number_as_f64(cf_val);
                            }

                            // if (std.mem.eql(u8, "wght", name_str)) {
                            if name_str == b"wght" {
                                // Somewhat subjective threshold, we consider fonts
                                // bold if they have a 'wght' set greater than 600.
                                is_bold = val > 600.0;
                                continue;
                            }
                            // if (std.mem.eql(u8, "ital", name_str)) {
                            if name_str == b"ital" {
                                is_italic = val > 0.5;
                                ital_seen = true;
                                continue;
                            }
                            // if (!ital_seen and std.mem.eql(u8, "slnt", name_str)) {
                            if !ital_seen && name_str == b"slnt" {
                                // Arbitrary threshold of anything more than a 5
                                // degree clockwise slant is considered italic.
                                is_italic = val <= -5.0;
                                continue;
                            }
                        }
                    }
                }

                // break :derived .{ is_bold, is_italic };
                break 'derived (is_bold, is_italic);
            };

            // self.bold = desc.bold == is_bold;
            // self.italic = desc.italic == is_italic;
            this.bold = desc.bold == is_bold;
            this.italic = desc.italic == is_italic;

            // Get the style string from the font.
            // var style_str_buf: [128]u8 = undefined;
            // const style_str: []const u8 = style_str: {
            //     const style = ct_desc.copyAttribute(.style_name) orelse break :style_str "";
            //     defer style.release();
            //     break :style_str style.cstring(&style_str_buf, .utf8) orelse "";
            // };
            let style_string: String = match unsafe { ct_desc.attribute(kCTFontStyleNameAttribute) }
                .and_then(|v| v.downcast::<CFString>().ok())
            {
                Some(s) => s.to_string(),
                None => String::new(),
            };
            // PORT NOTE: `cstring(&[128]u8, .utf8)` returns "" when the style name
            // doesn't fit its buffer (needs a NUL byte).
            let style_str: &[u8] = if style_string.len() < 128 {
                style_string.as_bytes()
            } else {
                b""
            };

            // The first string in this slice will be used for the exact match,
            // and for the fuzzy match, all matching substrings will increase
            // the rank.
            // const desired_styles: []const [:0]const u8 = desired: {
            //     if (desc.style) |s| break :desired &.{s};
            //
            //     // If we don't have an explicitly desired style name, we base
            //     // it on the bold and italic properties, this isn't ideal since
            //     // fonts may use style names other than these, but it helps in
            //     // some edge cases.
            //     if (desc.bold) {
            //         if (desc.italic) break :desired &.{ "bold italic", "bold", "italic", "oblique" };
            //         break :desired &.{ "bold", "upright" };
            //     } else if (desc.italic) {
            //         break :desired &.{ "italic", "regular", "oblique" };
            //     }
            //     break :desired &.{ "regular", "upright" };
            // };
            let desired_styles: Vec<&[u8]> = if let Some(s) = &desc.style {
                vec![s.as_slice()]
            }
            // If we don't have an explicitly desired style name, we base
            // it on the bold and italic properties, this isn't ideal since
            // fonts may use style names other than these, but it helps in
            // some edge cases.
            else if desc.bold {
                if desc.italic {
                    vec![
                        &b"bold italic"[..],
                        &b"bold"[..],
                        &b"italic"[..],
                        &b"oblique"[..],
                    ]
                } else {
                    vec![&b"bold"[..], &b"upright"[..]]
                }
            } else if desc.italic {
                vec![&b"italic"[..], &b"regular"[..], &b"oblique"[..]]
            } else {
                vec![&b"regular"[..], &b"upright"[..]]
            };

            // self.exact_style = std.ascii.eqlIgnoreCase(style_str, desired_styles[0]);
            this.exact_style = ascii_eql_ignore_case(style_str, desired_styles[0]);
            // Our "fuzzy match" score is 0 if the desired style isn't present
            // in the string, otherwise we give higher priority for styles that
            // have fewer characters not in the desired_styles list.
            // const fuzzy_type = @TypeOf(self.fuzzy_style); // u8
            // self.fuzzy_style = @intCast(style_str.len);
            this.fuzzy_style = style_str.len() as u8;
            // for (desired_styles) |s| {
            for s in &desired_styles {
                // if (std.ascii.indexOfIgnoreCase(style_str, s) != null) {
                if ascii_index_of_ignore_case(style_str, s).is_some() {
                    // self.fuzzy_style -|= @intCast(s.len);
                    this.fuzzy_style = this.fuzzy_style.saturating_sub(s.len() as u8);
                }
            }
            // self.fuzzy_style = std.math.maxInt(fuzzy_type) -| self.fuzzy_style;
            this.fuzzy_style = u8::MAX.saturating_sub(this.fuzzy_style);

            // return self;
            this
        }
    }

    /// Iterator over the matching descriptors, yielding `DeferredFace`s.
    // pub const DiscoverIterator = struct {
    pub struct DiscoverIterator {
        // alloc: Allocator,  (rustty uses a `Vec` — global allocator; no `alloc`.)
        // list: []const *macos.text.FontDescriptor,
        list: Vec<CFRetained<CTFontDescriptor>>,
        // variations: []const Variation,
        variations: Vec<Variation>,
        // i: usize,
        i: usize,
    }

    impl DiscoverIterator {
        // pub fn deinit(self: *DiscoverIterator) void {
        //     for (self.list) |desc| desc.release();
        //     self.alloc.free(self.list);
        //     self.* = undefined;
        // }
        //
        // PORT NOTE: RAII — each `CFRetained<CTFontDescriptor>` releases on drop and
        // the `Vec` frees its backing store; `clear()` drops them now, matching the
        // reference's explicit release + free.
        pub fn deinit(&mut self) {
            self.list.clear();
        }

        // pub fn next(self: *DiscoverIterator) !?DeferredFace {
        // (Named `next` to match the reference iterator; not `Iterator::next`,
        // whose signature — no `Result`, no external `deinit` — doesn't fit.)
        #[allow(clippy::should_implement_trait)]
        pub fn next(&mut self) -> Result<Option<DeferredFace>, Error> {
            // if (self.i >= self.list.len) return null;
            if self.i >= self.list.len() {
                return Ok(None);
            }

            // Get our descriptor. We need to remove the character set
            // limitation because we may have used that to filter but we
            // don't want it anymore because it'll restrict the characters
            // available.
            // const desc = desc: {
            //     // We create a copy, overwriting the character set attribute.
            //     const attrs = try macos.foundation.MutableDictionary.create(0);
            //     defer attrs.release();
            //     attrs.setValue(macos.text.FontAttribute.character_set.key(), macos.c.kCFNull);
            //     break :desc try macos.text.FontDescriptor.createCopyWithAttributes(self.list[self.i], @ptrCast(attrs));
            // };
            // defer desc.release();
            let desc = {
                let attrs = new_mutable_dictionary();
                let null_val = unsafe { kCFNull }.expect("kCFNull is NULL");
                unsafe {
                    CFMutableDictionary::set_value(
                        Some(&attrs),
                        kCTFontCharacterSetAttribute as *const CFString as *const c_void,
                        null_val as *const _ as *const c_void,
                    );
                }
                unsafe { self.list[self.i].copy_with_attributes(&attrs) }
            };

            // Create our font. We need a size to initialize it so we use size
            // 12 but we will alter the size later.
            // const font = try macos.text.Font.createWithFontDescriptor(desc, 12);
            // errdefer font.release();
            let font: CFRetained<CTFont> =
                unsafe { CTFont::with_font_descriptor(&desc, 12.0, ptr::null()) };

            // Increment after we return
            // defer self.i += 1;
            self.i += 1;

            // return DeferredFace{ .ct = .{ .font = font, .variations = self.variations } };
            Ok(Some(DeferredFace {
                ct: Some(CoreTextData {
                    font,
                    variations: self.variations.clone(),
                }),
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE (DEFERRED — Windows FreeType-scan backend `freetype_windows`): the
// `Windows` discoverer walks the system + per-user font directories, opening
// candidate files with FreeType only as needed and matching by `family_name`
// (with an SFNT-name-table fallback). It lands with the Windows backend; recorded
// here verbatim:
//
// /// Windows font discovery. Enumerates font files in the system and
// /// per-user font directories and matches them to a descriptor via
// /// FreeType's family_name field (with a fallback to the SFNT name
// /// table when family_name is missing).
// ///
// /// No external service is used; each discover() call walks the
// /// directories, opening candidate files with FreeType only as needed.
// /// For typical Windows installations (~300 fonts) a name query is in
// /// the tens of milliseconds. A codepoint fallback query may be
// /// noticeably slower because every candidate has to be opened to
// /// probe its CMap.
// pub const Windows = struct {
//     lib: Library,
//
//     pub fn init(lib: Library) Windows { return .{ .lib = lib }; }
//     pub fn deinit(self: *Windows) void { _ = self; }
//
//     pub fn discover(self, alloc, desc) !DiscoverIterator {
//         return .{
//             .alloc = alloc, .lib = self.lib, .desc = desc,
//             .variations = desc.variations, .state = .system,
//             .dir = null, .iter = null, .system_path = null, .user_path = null,
//         };
//     }
//
//     pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator {
//         _ = collection;
//         return self.discover(alloc, desc);
//     }
//
//     pub const DiscoverIterator = struct {
//         alloc: Allocator,
//         lib: Library,
//         desc: Descriptor,
//         variations: []const Variation,
//         state: State,
//         dir: ?std.fs.Dir,
//         iter: ?std.fs.Dir.Iterator,
//         system_path: ?[:0]const u8,
//         user_path: ?[:0]const u8,
//
//         const State = enum { system, user, done };
//
//         pub fn deinit(self: *DiscoverIterator) void {
//             if (self.dir) |*d| d.close();
//             if (self.system_path) |p| self.alloc.free(p);
//             if (self.user_path) |p| self.alloc.free(p);
//             self.* = undefined;
//         }
//
//         pub fn next(self: *DiscoverIterator) !?DeferredFace {
//             while (true) {
//                 // Ensure we have a directory iterator for the current state.
//                 if (self.iter == null) {
//                     switch (self.state) {
//                         .system => {
//                             const path = self.systemFontsPath() orelse { self.state = .user; continue; };
//                             self.system_path = path;
//                             self.dir = std.fs.openDirAbsoluteZ(path, .{ .iterate = true }) catch { self.state = .user; continue; };
//                             self.iter = self.dir.?.iterate();
//                         },
//                         .user => {
//                             const path = self.userFontsPath() orelse { self.state = .done; continue; };
//                             self.user_path = path;
//                             self.dir = std.fs.openDirAbsoluteZ(path, .{ .iterate = true }) catch { self.state = .done; continue; };
//                             self.iter = self.dir.?.iterate();
//                         },
//                         .done => return null,
//                     }
//                 }
//                 const entry = (self.iter.?.next() catch null) orelse {
//                     // Finished this directory; advance state.
//                     if (self.dir) |*d| d.close();
//                     self.dir = null;
//                     self.iter = null;
//                     self.state = switch (self.state) { .system => .user, .user => .done, .done => .done };
//                     continue;
//                 };
//                 if (entry.kind != .file) continue;
//                 if (!isFontFile(entry.name)) continue;
//                 if (try self.tryMatch(entry.name)) |face| return face;
//             }
//         }
//
//         /// Build the system fonts directory from %SYSTEMROOT%. Returns null
//         /// if SYSTEMROOT is unset, which shouldn't happen on a healthy
//         /// Windows install but we just skip the directory rather than
//         /// falling back to a hardcoded drive letter.
//         fn systemFontsPath(self: *DiscoverIterator) ?[:0]const u8 {
//             const systemroot = std.process.getEnvVarOwned(self.alloc, "SYSTEMROOT") catch return null;
//             defer self.alloc.free(systemroot);
//             return std.fmt.allocPrintSentinel(self.alloc, "{s}\\Fonts", .{systemroot}, 0) catch null;
//         }
//
//         fn userFontsPath(self: *DiscoverIterator) ?[:0]const u8 {
//             const local_appdata = std.process.getEnvVarOwned(self.alloc, "LOCALAPPDATA") catch return null;
//             defer self.alloc.free(local_appdata);
//             return std.fmt.allocPrintSentinel(self.alloc, "{s}\\Microsoft\\Windows\\Fonts", .{local_appdata}, 0) catch null;
//         }
//
//         fn tryMatch(self: *DiscoverIterator, name: []const u8) !?DeferredFace {
//             const dir_path = switch (self.state) {
//                 .system => self.system_path.?,
//                 .user => self.user_path.?,
//                 .done => return null,
//             };
//             var path_buf: [std.fs.max_path_bytes]u8 = undefined;
//             const full_path = std.fmt.bufPrintZ(&path_buf, "{s}\\{s}", .{ dir_path, name }) catch return null;
//             const is_ttc = std.ascii.endsWithIgnoreCase(name, ".ttc");
//             const max_faces: i32 = if (is_ttc) 16 else 1;
//             // Probe each face in the file.
//             var face_index: i32 = 0;
//             while (face_index < max_faces) : (face_index += 1) {
//                 var face = Face.initFile(self.lib, full_path, face_index, .{ .size = .{ .points = 12 } }) catch break;
//                 if (self.matches(&face)) return try self.makeDeferred(face, full_path, face_index);
//                 face.deinit();
//             }
//             return null;
//         }
//
//         /// Check whether the given face matches the descriptor.
//         fn matches(self: *const DiscoverIterator, face: *Face) bool {
//             if (self.desc.family) |family| { if (!familyMatches(face, family)) return false; }
//             if (self.desc.codepoint != 0) { if (face.glyphIndex(self.desc.codepoint) == null) return false; }
//             return true;
//         }
//
//         fn makeDeferred(self: *DiscoverIterator, face: Face, full_path: []const u8, face_index: i32) !DeferredFace {
//             const path_owned = try self.alloc.dupeZ(u8, full_path);
//             errdefer self.alloc.free(path_owned);
//             const presentation: Presentation = if (face.hasColor()) .emoji else .text;
//             return DeferredFace{ .win = .{
//                 .path = path_owned, .face_index = face_index, .variations = self.variations,
//                 .peek = face, .presentation = presentation, .alloc = self.alloc,
//             } };
//         }
//     };
//
//     fn isFontFile(name: []const u8) bool {
//         return std.ascii.endsWithIgnoreCase(name, ".ttf") or
//             std.ascii.endsWithIgnoreCase(name, ".ttc") or
//             std.ascii.endsWithIgnoreCase(name, ".otf");
//     }
//
//     /// Compare a face's family against a requested family name. Checks
//     /// FreeType's family_name first, then falls back to the SFNT name
//     /// table entry.
//     fn familyMatches(face: *Face, family: [:0]const u8) bool {
//         const ft_family: ?[*:0]const u8 = face.face.handle.*.family_name;
//         if (ft_family) |f| { if (std.ascii.eqlIgnoreCase(std.mem.span(f), family)) return true; }
//         var buf: [256]u8 = undefined;
//         const sfnt = face.name(&buf) catch "";
//         return sfnt.len > 0 and std.ascii.eqlIgnoreCase(sfnt, family);
//     }
// };
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // test "descriptor hash" {
    #[test]
    fn descriptor_hash() {
        // const testing = std.testing;
        // var d: Descriptor = .{};
        let d: Descriptor = Descriptor::default();
        // try testing.expect(d.hashcode() != 0);
        assert!(d.hashcode() != 0);
    }

    // test "descriptor hash family names" {
    #[test]
    fn descriptor_hash_family_names() {
        // const testing = std.testing;
        // var d1: Descriptor = .{ .family = "A" };
        let d1 = Descriptor {
            family: Some(b"A".to_vec()),
            ..Descriptor::default()
        };
        // var d2: Descriptor = .{ .family = "B" };
        let d2 = Descriptor {
            family: Some(b"B".to_vec()),
            ..Descriptor::default()
        };
        // try testing.expect(d1.hashcode() != d2.hashcode());
        assert!(d1.hashcode() != d2.hashcode());
    }

    /// Compile-gate: on every target that is neither macOS nor wasm32 (the
    /// `fontconfig_freetype` backend class), the `Discover` seam that
    /// `CodepointResolver`/`SharedGridSet` name unconditionally must type-check
    /// against the reference signatures (`discovery.zig`'s `Fontconfig`).
    ///
    /// PORT NOTE: this mirrors the `refAllDecls` convention used across this crate
    /// (e.g. `face/mod.rs`'s `ref_all_decls`): the methods are coerced to function
    /// pointers with their exact expected signatures rather than CALLED, because
    /// their bodies are honest `unimplemented!()` leaves until the real Fontconfig
    /// discoverer lands (L3 Task 6/7). Coercion still fails to compile if a
    /// signature drifts, which is exactly the guarantee this test exists to hold.
    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    #[test]
    fn discover_impl_typechecks() {
        use crate::collection::Collection;
        use crate::deferred_face::DeferredFace;
        use crate::library::Library;

        // The active discoverer on this backend class is the Fontconfig one.
        let _: fn(Library) -> Fontconfig = Fontconfig::init;
        let _: fn(&mut Fontconfig) = Fontconfig::deinit;
        let _: fn(&Fontconfig, Descriptor) -> Result<DiscoverIterator, Error> =
            Fontconfig::discover;
        let _: fn(&Fontconfig, *mut Collection, Descriptor) -> Result<DiscoverIterator, Error> =
            Fontconfig::discover_fallback;

        // `SharedGridSet`/`CodepointResolver` drive the iterator via `next`/`deinit`.
        let _: fn(&mut DiscoverIterator) -> Result<Option<DeferredFace>, Error> =
            DiscoverIterator::next;
        let _: fn(&mut DiscoverIterator) = DiscoverIterator::deinit;

        // `Discover` (the backend selector) must resolve to that discoverer.
        let _: fn(Library) -> Discover = Discover::init;

        // `SharedGridSet::Error` composes the discovery error via `From`.
        let _: fn(crate::face::FaceError) -> Error = Error::from;
        let _: fn(crate::collection::Error) -> Error = Error::from;
    }

    // test "fontconfig" {
    #[test]
    fn fontconfig() {
        // if (options.backend != .fontconfig_freetype) return error.SkipZigTest;
        //
        // PORT NOTE: on the macOS/coretext gate this test always skips (its body
        // exercises the Fontconfig discoverer, which is DEFERRED). The full body
        // lands with the Fontconfig backend. The `!= fontconfig_freetype` skip is
        // expressed as an inverted `== fontconfig_freetype` guard (clippy-clean).
        if crate::options::BACKEND == crate::Backend::FontconfigFreetype {
            // var lib = try Library.init(alloc); defer lib.deinit();
            // var fc = Fontconfig.init(lib); defer fc.deinit();
            // var it = try fc.discover(alloc, .{ .family = "monospace", .size = 12 });
            // defer it.deinit();
            // (body DEFERRED with the Fontconfig backend)
        }
    }

    // test "fontconfig codepoint" {
    #[test]
    fn fontconfig_codepoint() {
        // if (options.backend != .fontconfig_freetype) return error.SkipZigTest;
        //
        // PORT NOTE: DEFERRED with the Fontconfig backend (skips on the coretext
        // gate). Full body recorded below:
        //
        // var lib = try Library.init(alloc); defer lib.deinit();
        // var fc = Fontconfig.init(lib); defer fc.deinit();
        // var it = try fc.discover(alloc, .{ .codepoint = 'A', .size = 12 });
        // defer it.deinit();
        // // The first result should have the codepoint. Later ones may not
        // // because fontconfig returns all fonts sorted.
        // var face = (try it.next()).?;
        // defer face.deinit();
        // try testing.expect(face.hasCodepoint('A', null));
        // // Should have other codepoints too
        // try testing.expect(face.hasCodepoint('B', null));
        if crate::options::BACKEND == crate::Backend::FontconfigFreetype {
            // (body DEFERRED with the Fontconfig backend)
        }
    }

    // PORT NOTE: unit tests for `Descriptor::to_fc_pattern` (the port of
    // `discovery.zig:117-157` `toFcPattern`). The reference has no dedicated test
    // for `toFcPattern` (it is exercised indirectly by the `fontconfig`/`fontconfig
    // codepoint` tests, whose bodies discover real fonts); these directly assert
    // the built pattern's contents, and are gated to the fontconfig backend class
    // (the only class where `to_fc_pattern` is compiled).
    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    #[test]
    fn descriptor_to_fc_pattern_family_size_mono() {
        use crate::fc::{Property, Spacing, Value};

        let desc = Descriptor {
            family: Some(b"monospace".to_vec()),
            size: 12.0,
            ..Descriptor::default()
        };
        // SAFETY: `to_fc_pattern` returns a freshly created, non-null pattern that
        // this test owns and destroys below.
        let pat = unsafe { &mut *desc.to_fc_pattern() };

        let Value::String(family) = pat.get(Property::Family, 0).unwrap() else {
            panic!("family value was not a string");
        };
        assert_eq!(family, c"monospace");

        let Value::Integer(size) = pat.get(Property::Size, 0).unwrap() else {
            panic!("size value was not an integer");
        };
        assert_eq!(size, 12);

        // Spacing is added UNCONDITIONALLY (the reference always biases toward
        // monospace matches, `discovery.zig:147-154`).
        let Value::Integer(spacing) = pat.get(Property::Spacing, 0).unwrap() else {
            panic!("spacing value was not an integer");
        };
        assert_eq!(spacing, Spacing::Mono as i32);

        pat.destroy();
    }

    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    #[test]
    fn descriptor_to_fc_pattern_codepoint() {
        use crate::fc::{Property, Value};

        let desc = Descriptor {
            codepoint: u32::from('A'),
            ..Descriptor::default()
        };
        // SAFETY: as above — owned, non-null, destroyed below.
        let pat = unsafe { &mut *desc.to_fc_pattern() };

        let Value::CharSet(cs) = pat.get(Property::Charset, 0).unwrap() else {
            panic!("charset value was not a char set");
        };
        // SAFETY: `cs` borrows the pattern-owned char set; valid until `pat` is
        // destroyed.
        assert!(unsafe { (*cs).has_char(u32::from('A')) });

        pat.destroy();
    }

    // test "coretext" {
    #[cfg(target_os = "macos")]
    #[test]
    fn coretext() {
        // if (options.backend != .coretext and options.backend != .coretext_freetype)
        //     return error.SkipZigTest;
        if crate::options::BACKEND != crate::Backend::Coretext
            && crate::options::BACKEND != crate::Backend::CoretextFreetype
        {
            return;
        }

        use crate::library::Library;

        // var lib = try Library.init(alloc); defer lib.deinit();
        let mut lib = Library::init().unwrap();

        // var ct = CoreText.init(lib); defer ct.deinit();
        let mut ct = CoreText::init(lib);
        // var it = try ct.discover(alloc, .{ .family = "Monaco", .size = 12 });
        let mut it = ct
            .discover(Descriptor {
                family: Some(b"Monaco".to_vec()),
                size: 12.0,
                ..Descriptor::default()
            })
            .unwrap();
        // defer it.deinit();
        // var count: usize = 0;
        let mut count: usize = 0;
        // while (try it.next()) |_| { count += 1; }
        while it.next().unwrap().is_some() {
            count += 1;
        }
        // try testing.expect(count > 0);
        assert!(count > 0);

        it.deinit();
        ct.deinit();
        lib.deinit();
    }

    // test "coretext codepoint" {
    #[cfg(target_os = "macos")]
    #[test]
    fn coretext_codepoint() {
        // if (options.backend != .coretext and options.backend != .coretext_freetype)
        //     return error.SkipZigTest;
        if crate::options::BACKEND != crate::Backend::Coretext
            && crate::options::BACKEND != crate::Backend::CoretextFreetype
        {
            return;
        }

        use crate::library::Library;

        // var lib = try Library.init(alloc); defer lib.deinit();
        let mut lib = Library::init().unwrap();

        // var ct = CoreText.init(lib); defer ct.deinit();
        let mut ct = CoreText::init(lib);
        // var it = try ct.discover(alloc, .{ .codepoint = 'A', .size = 12 });
        let mut it = ct
            .discover(Descriptor {
                codepoint: u32::from('A'),
                size: 12.0,
                ..Descriptor::default()
            })
            .unwrap();
        // defer it.deinit();

        // The first result should have the codepoint. Later ones may not
        // because fontconfig returns all fonts sorted.
        // const face = (try it.next()).?;
        let face = it.next().unwrap().unwrap();
        // try testing.expect(face.hasCodepoint('A', null));
        assert!(face.has_codepoint(u32::from('A'), None));

        // Should have other codepoints too
        // try testing.expect(face.hasCodepoint('B', null));
        assert!(face.has_codepoint(u32::from('B'), None));

        it.deinit();
        ct.deinit();
        lib.deinit();
    }

    // test "coretext sorting" {
    #[cfg(target_os = "macos")]
    #[test]
    fn coretext_sorting() {
        // if (options.backend != .coretext and options.backend != .coretext_freetype)
        //     return error.SkipZigTest;
        if crate::options::BACKEND != crate::Backend::Coretext
            && crate::options::BACKEND != crate::Backend::CoretextFreetype
        {
            return;
        }

        // !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!//
        // FIXME: Disabled for now because SF Pro is not available in CI
        //        The solution likely involves directly testing that the
        //        `sortMatchingDescriptors` function sorts a bundled test
        //        font correctly, instead of relying on the system fonts.
        // if (true) return error.SkipZigTest;
        // !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!//
        #[allow(clippy::needless_return)]
        if true {
            return;
        }

        // The remainder is unreachable (the `if (true) return` above always skips);
        // it is ported verbatim as reference below and never executes:
        //
        // var lib = try Library.init(alloc); defer lib.deinit();
        // var ct = CoreText.init(lib); defer ct.deinit();
        //
        // We try to get a Regular, Italic, Bold, & Bold Italic version of SF Pro,
        // which should be installed on all Macs, and has many styles which makes
        // it a good test, since there will be many results for each discovery.
        //
        // Regular
        // {
        //     var it = try ct.discover(alloc, .{ .family = "SF Pro", .size = 12 });
        //     defer it.deinit();
        //     const res = (try it.next()).?;
        //     var buf: [1024]u8 = undefined;
        //     const name = try res.name(&buf);
        //     try testing.expectEqualStrings("SF Pro Regular", name);
        // }
        //
        // Regular Italic
        // NOTE: This makes sure that we don't accidentally prefer "Thin Italic",
        //       which we previously did, because it has a shorter name.
        // {
        //     var it = try ct.discover(alloc, .{ .family = "SF Pro", .size = 12, .italic = true });
        //     defer it.deinit();
        //     const res = (try it.next()).?;
        //     var buf: [1024]u8 = undefined;
        //     const name = try res.name(&buf);
        //     try testing.expectEqualStrings("SF Pro Regular Italic", name);
        // }
        //
        // Bold
        // {
        //     var it = try ct.discover(alloc, .{ .family = "SF Pro", .size = 12, .bold = true });
        //     defer it.deinit();
        //     const res = (try it.next()).?;
        //     var buf: [1024]u8 = undefined;
        //     const name = try res.name(&buf);
        //     try testing.expectEqualStrings("SF Pro Bold", name);
        // }
        //
        // Bold Italic
        // {
        //     var it = try ct.discover(alloc, .{ .family = "SF Pro", .size = 12, .bold = true, .italic = true });
        //     defer it.deinit();
        //     const res = (try it.next()).?;
        //     var buf: [1024]u8 = undefined;
        //     const name = try res.name(&buf);
        //     try testing.expectEqualStrings("SF Pro Bold Italic", name);
        // }
    }

    // test "windows" {
    #[test]
    fn windows() {
        // if (options.backend != .freetype_windows) return error.SkipZigTest;
        //
        // PORT NOTE: DEFERRED with the Windows backend (always skips on the coretext
        // gate). Full body recorded below:
        //
        // var lib = try Library.init(alloc); defer lib.deinit();
        // var win = Windows.init(lib); defer win.deinit();
        // // Arial ships on every stock Windows install.
        // var it = try win.discover(alloc, .{ .family = "Arial", .size = 12 });
        // defer it.deinit();
        // var face = (try it.next()) orelse return error.TestFontNotFound;
        // defer face.deinit();
        // try testing.expect(face.hasCodepoint('A', null));
        if crate::options::BACKEND == crate::Backend::FreetypeWindows {
            // (body DEFERRED with the Windows backend)
        }
    }
}
