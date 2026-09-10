//! CoreText font face.
// PORTS: ghostty src/font/face/coretext.zig (the CoreText face: a `CTFont` built
// from a descriptor, `glyphIndex`, `renderGlyph` via a `CGBitmapContext` -> Atlas,
// and font metrics read from the raw OpenType tables + CoreText getters).
//
// PORT NOTE (imports): the reference imports are
//   const std = @import("std");
//   const builtin = @import("builtin");
//   const assert = @import("../../quirks.zig").inlineAssert;
//   const Allocator = std.mem.Allocator;
//   const macos = @import("macos");
//   const harfbuzz = @import("harfbuzz");
//   const font = @import("../main.zig");
//   const opentype = @import("../opentype.zig");
//   const quirks = @import("../../quirks.zig");
//   const log = std.log.scoped(.font_face);
// Mapped per memory `apc-glyf-font-layering-d1` + the F3 kickoff:
//   * `macos.foundation.*` -> `objc2_core_foundation` (CFData/CFString/CFNumber),
//   * `macos.graphics.*`   -> `objc2_core_graphics` (CGContext/CGColorSpace/
//     CGBitmapContext/CGAffineTransform/CGGlyph) plus the two classic C entry
//     points objc2 0.3 doesn't wrap (`CGBitmapContextCreate`,
//     `CFStringGetSurrogatePairForLongCharacter`), declared in `sys` below,
//   * `macos.text.*`       -> `objc2_core_text` (CTFont/CTFontDescriptor/
//     CTFontManager).
//   * `assert = quirks.inlineAssert` is `rustty_util::quirks::inlineAssert`.
//   * `harfbuzz` is unused here: the macOS default backend is `coretext`, whose
//     `backend.hasHarfbuzz()` is false, so `harfbuzz_shaper` is comptime-false and
//     every `if (comptime harfbuzz_shaper)` arm is ported as a comment (the
//     `coretext_harfbuzz` backend's Harfbuzz font lands with that backend).
//   * `font.*` shared types (`Atlas`, `Glyph`, `Metrics`, glyph `RenderOptions`)
//     and the `opentype.*` table parsers live in the PURE `rustty-font` leaf.
//   * `log` is a scoped logger; it maps onto the `log` facade crate with the
//     reference's `std.log.scoped` tag carried as `target: LOG_SCOPE`
//     (`.port/parity-closure/DECISION-logging.md` §6). Reference `log.*` calls
//     are kept as verbatim comments (R1) with the live emission beneath them.

use allocator_api2::alloc::Allocator;
use core::ffi::c_void;

// const log = std.log.scoped(.font_face);
//
// The reference's `std.log.scoped(.font_face)` tag, carried through the `log`
// facade's `target:`. `face/coretext.zig`, `face/freetype.zig` and
// `face/web_canvas.zig` all use this ONE scope in the reference; that is the
// reference's own choice and is ported as-is.
const LOG_SCOPE: &str = "font_face";

/// The reference guards its variation-axes dump with
/// `comptime builtin.mode == .Debug`; `cfg!(debug_assertions)` is the direct
/// Rust analog.
const DEBUG_MODE: bool = cfg!(debug_assertions);
use core::ptr::{self, NonNull};

use objc2_core_foundation::{kCFAllocatorNull, CFData, CFNumber, CFNumberType, CFRetained};
use objc2_core_foundation::{CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGGlyph;
use objc2_core_graphics::{
    kCGColorSpaceDisplayP3, kCGColorSpaceLinearGray, CGColorSpace, CGContext, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGTextDrawingMode,
};
use objc2_core_text::{
    CTFont, CTFontManagerCreateFontDescriptorFromData, CTFontOrientation, CTFontSymbolicTraits,
    CTFontTableOptions,
};

use rustty_font::atlas::Atlas;
use rustty_font::glyph::{ConstraintSize, Glyph, RenderOptions, Size};
use rustty_font::metrics::FaceMetrics;
use rustty_font::opentype::head::Head;
use rustty_font::opentype::hhea::Hhea;
use rustty_font::opentype::os2::OS2;
use rustty_font::opentype::post::Post;
use rustty_font::opentype::svg::SVG;
use rustty_util::quirks::inlineAssert as assert;

use crate::hb;

use super::{DesiredSize, Options, Variation};

/// Raw C entry points that objc2 0.3 does not wrap.
// PORT NOTE: `CGBitmapContextCreate` (the classic bitmap-context constructor) is a
// stable CoreGraphics symbol the reference calls directly; objc2 0.3.2 omits it
// (only the "adaptive" variant is wrapped). It is declared here against the SAME
// framework objc2 links (R3 — a binding, not a reimplementation).
mod sys {
    use super::*;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C-unwind" {
        pub fn CGBitmapContextCreate(
            data: *mut c_void,
            width: usize,
            height: usize,
            bits_per_component: usize,
            bytes_per_row: usize,
            space: *const CGColorSpace,
            bitmap_info: u32,
        ) -> *mut CGContext;
    }
}

/// UTF-32 -> UTF-16 surrogate decomposition, matching CoreFoundation.
// PORT NOTE: `macos.foundation.stringGetSurrogatePairForLongCharacter` calls
// `CFStringGetSurrogatePairForLongCharacter`, which is a `CF_INLINE` function in
// `<CoreFoundation/CFString.h>` (NOT an exported dynamic symbol — objc2 leaves it
// as a TODO for exactly this reason). Its header body is reproduced verbatim here:
// a non-BMP scalar (0xFFFF < c < 0x110000) is split into a high/low surrogate and
// returns `true`; otherwise the single code unit is written and it returns `false`.
fn string_get_surrogate_pair_for_long_character(character: u32, surrogates: &mut [u16; 2]) -> bool {
    if (character > 0xFFFF) && (character < 0x110000) {
        // Non-BMP character
        let character = character - 0x10000;
        surrogates[0] = ((character >> 10) + 0xD800) as u16;
        surrogates[1] = ((character & 0x3FF) + 0xDC00) as u16;
        true
    } else {
        surrogates[0] = character as u16;
        false
    }
}

/// The error set for the CoreText face.
// PORT NOTE: the reference errors are the union of `error.FontInitFailure`,
// `Allocator.Error` (`OutOfMemory`), `error.InvalidAtlasFormat`, the `Atlas`
// reserve error, and `ColorState.Error` (`InvalidSVGTable`). Modeled here by one
// enum with `From` conversions so `?` composes them.
#[derive(Debug)]
pub enum Error {
    FontInitFailure,
    OutOfMemory,
    InvalidAtlasFormat,
    AtlasFull,
    InvalidSVGTable,
}

impl From<rustty_font::atlas::OutOfMemory> for Error {
    fn from(_: rustty_font::atlas::OutOfMemory) -> Self {
        Error::OutOfMemory
    }
}
impl From<rustty_font::atlas::ReserveError> for Error {
    fn from(e: rustty_font::atlas::ReserveError) -> Self {
        match e {
            rustty_font::atlas::ReserveError::OutOfMemory => Error::OutOfMemory,
            rustty_font::atlas::ReserveError::AtlasFull => Error::AtlasFull,
        }
    }
}

/// The matrix applied to a regular font to auto-italicize it.
// pub const italic_skew = macos.graphics.AffineTransform{ .a=1, .b=0, .c=0.267949, .d=1, .tx=0, .ty=0 };
#[allow(non_upper_case_globals)]
pub const italic_skew: CGAffineTransform = CGAffineTransform {
    a: 1.0,
    b: 0.0,
    c: 0.267949, // approx. tan(15)
    d: 1.0,
    tx: 0.0,
    ty: 0.0,
};

pub struct Face {
    /// Our font face
    // PORT NOTE: the reference `font: *macos.text.Font` is a manually
    // retained/released `CTFont`; modeled as `CFRetained<CTFont>`, whose `Drop`
    // performs the single `CFRelease` the reference's `deinit`/`errdefer` do.
    pub font: CFRetained<CTFont>,

    /// Harfbuzz font corresponding to this face. We only use this
    /// if we're using Harfbuzz.
    // hb_font: if (harfbuzz_shaper) harfbuzz.Font else void,
    //
    // PORT NOTE: this crate models the macOS `coretext_harfbuzz` backend for the
    // shaper tier (`harfbuzz_shaper == true`, see `HARFBUZZ_SHAPER` below), so the
    // face carries a real `hb_font` built from the `CTFont` via
    // `harfbuzz.coretext.createFont`. `hb::Font` is a raw-pointer handle (no
    // `Drop`); it is destroyed in `deinit`, exactly as the FreeType face does.
    pub hb_font: hb::Font,

    /// Set quirks.disableDefaultFontFeatures
    pub quirks_disable_default_font_features: bool,

    /// True if this font face should be rasterized with a synthetic bold
    /// effect. This is used for fonts that don't have a bold variant.
    pub synthetic_bold: Option<f64>,

    /// If the face can possibly be colored, then this is the state
    /// used to check for color information. This is null if the font
    /// can't possibly be colored (i.e. doesn't have SVG, sbix, etc
    /// tables).
    pub color: Option<ColorState>,

    /// The current size this font is set to.
    pub size: DesiredSize,
}

impl Face {
    /// True if our build is using Harfbuzz. If we're not, we can avoid
    /// some Harfbuzz-specific code paths.
    // const harfbuzz_shaper = font.options.backend.hasHarfbuzz();
    //
    // PORT NOTE: the macOS default backend is `coretext`, whose `hasHarfbuzz()` is
    // false; this const is reproduced as `false` so the comptime-gated Harfbuzz
    // arms below collapse to comments.
    const HARFBUZZ_SHAPER: bool = true;

    /// Initialize a CoreText-based font from a TTF/TTC in memory.
    // PORT NOTE: the reference signature is `init(lib: font.Library, source, opts)`
    // and immediately does `_ = lib` (CoreText needs no library state). `lib` is
    // the backend-switched `crate::Library` (= `NoopLibrary` on the macOS coretext
    // backend); it is accepted for call-shape fidelity and ignored.
    pub fn init(
        _lib: crate::library::Library,
        source: &[u8],
        opts: Options,
    ) -> Result<Face, Error> {
        // _ = lib;

        let data = new_data_no_copy(source).ok_or(Error::OutOfMemory)?;
        // defer data.release(); — `data` is `CFRetained` and releases on drop.

        let desc = unsafe { CTFontManagerCreateFontDescriptorFromData(&data) }
            // orelse return error.FontInitFailure;
            .ok_or(Error::FontInitFailure)?;
        // defer desc.release();

        let ct_font = unsafe { CTFont::with_font_descriptor(&desc, 12.0, ptr::null()) };
        // defer ct_font.release();

        Face::init_font_copy(&ct_font, opts)
    }

    /// Initialize a CoreText-based face from another initialized font face
    /// but with a new size. This is often how CoreText fonts are initialized
    /// because the font is loaded at a default size during discovery, and then
    /// adjusted to the final size for final load.
    pub fn init_font_copy(base: &CTFont, opts: Options) -> Result<Face, Error> {
        // Create a copy. The copyWithAttributes docs say the size is in points,
        // but we need to scale the points by the DPI and to do that we use our
        // function called "pixels".
        let ct_font =
            unsafe { base.copy_with_attributes(opts.size.pixels() as f64, ptr::null(), None) };
        // errdefer ct_font.release();

        Face::init_font(ct_font, opts)
    }

    /// Initialize a face with a CTFont. This will take ownership over
    /// the CTFont. This does NOT copy or retain the CTFont.
    pub fn init_font(ct_font: CFRetained<CTFont>, opts: Options) -> Result<Face, Error> {
        let traits = unsafe { ct_font.symbolic_traits() };

        // var hb_font = if (comptime harfbuzz_shaper) font: {
        //     var hb_font = try harfbuzz.coretext.createFont(ct_font);
        //     const pixels: opentype.sfnt.F26Dot6 = .from(opts.size.pixels());
        //     hb_font.setScale(@bitCast(pixels), @bitCast(pixels));
        //     break :font hb_font;
        // } else {};
        // errdefer if (comptime harfbuzz_shaper) hb_font.destroy();
        //
        // PORT NOTE (`harfbuzz_shaper == true`): build the HarfBuzz font from the
        // `CTFont`. `opentype.sfnt.F26Dot6.from(pixels)` rounds `pixels*64` to a
        // 26.6 fixed-point `i32` and `@bitCast`s it to the `u32` scale argument;
        // reproduced as `(pixels*64).round() as i32 as u32`.
        let _ = Self::HARFBUZZ_SHAPER;
        let hb_font = {
            let ct_font_ptr = CFRetained::as_ptr(&ct_font).as_ptr() as *const core::ffi::c_void;
            let hb_font =
                hb::create_font_coretext(ct_font_ptr).map_err(|_| Error::FontInitFailure)?;
            let pixels = opts.size.pixels();
            let scale = (pixels * 64.0).round() as i32 as u32;
            hb_font.set_scale(scale, scale);
            hb_font
        };
        // errdefer hb_font.destroy(); — the only fallible op below is
        // `ColorState::init`; on its error the face is not yet built, so destroy
        // `hb_font` explicitly to avoid a leak (mirrors the reference `errdefer`).

        let color: Option<ColorState> = if traits.0 & CTFontSymbolicTraits::TraitColorGlyphs.0 != 0
        {
            match ColorState::init(&ct_font) {
                Ok(cs) => Some(cs),
                // errdefer hb_font.destroy();
                Err(e) => {
                    hb_font.destroy();
                    return Err(e);
                }
            }
        } else {
            None
        };
        // errdefer if (color) |v| v.deinit(); — `ColorState` owns its data and
        // drops normally on the error paths below (there are none after this).

        let mut result = Face {
            font: ct_font,
            hb_font,
            color,
            size: opts.size,
            quirks_disable_default_font_features: false,
            synthetic_bold: None,
        };
        result.quirks_disable_default_font_features = quirks_disable_default_font_features(&result);

        // In debug mode, we output information about available variation axes,
        // if they exist.
        //
        // if (comptime builtin.mode == .Debug) { ... }
        //
        // Ported live (S90-F2 / S91-F8): the body is `log_variation_axes`, which
        // takes the mode flag as a parameter so the release arm is testable; the
        // ONLY production call site passes `DEBUG_MODE`, the direct port of
        // `comptime builtin.mode == .Debug`.
        result.log_variation_axes(DEBUG_MODE)?;

        Ok(result)
    }

    pub fn deinit(&mut self) {
        // self.font.release();
        // if (comptime harfbuzz_shaper) self.hb_font.destroy();
        // if (self.color) |v| v.deinit();
        // self.* = undefined;
        //
        // PORT NOTE: `self.font` (`CFRetained<CTFont>`) and `self.color`
        // (`ColorState`, which owns its bytes) release/free on `Drop`, reproducing
        // the reference's manual `release`/`deinit` + `self.* = undefined`. The
        // HarfBuzz font is a raw-pointer handle (no `Drop`), so — exactly as the
        // FreeType face does — it is destroyed here (`harfbuzz_shaper == true`).
        // All faces are torn down through `deinit` (the Collection walks entries
        // calling `deinit`; the face tests call it explicitly), so this runs once.
        #[cfg(test)]
        crate::face::test_probe::note_face_deinit();
        self.hb_font.destroy();
    }

    /// Return a new face that is the same as this but has a transformation
    /// matrix applied to italicize it.
    pub fn synthetic_italic(&self, opts: Options) -> Result<Face, Error> {
        let ct_font = unsafe { self.font.copy_with_attributes(0.0, &italic_skew, None) };
        // errdefer ct_font.release();
        Face::init_font(ct_font, opts)
    }

    /// Return a new face that is the same as this but applies a synthetic
    /// bold effect to it. This is useful for fonts that don't have a bold
    /// variant.
    pub fn synthetic_bold(&self, opts: Options) -> Result<Face, Error> {
        let ct_font = unsafe { self.font.copy_with_attributes(0.0, ptr::null(), None) };
        // errdefer ct_font.release();
        let mut face = Face::init_font(ct_font, opts)?;

        // To determine our synthetic bold line width we get a multiplier
        // from the font size in points. This is a heuristic that is based
        // on the fact that a line width of 1 looks good to me at a certain
        // point size. We want to scale that up roughly linearly with the
        // font size.
        let points_f64: f64 = opts.size.points as f64;
        let line_width = (points_f64 / 14.0).max(1.0);
        // log.debug("synthetic bold line width={}", .{line_width});
        face.synthetic_bold = Some(line_width);

        Ok(face)
    }

    /// The reference's `if (comptime builtin.mode == .Debug)` variation-axes
    /// dump from `init` (`face/coretext.zig:118-162`), extracted into a method
    /// so its two arms are independently testable. `debug_mode` is the port of
    /// `comptime builtin.mode == .Debug`; production passes `DEBUG_MODE`.
    ///
    /// Purely diagnostic: it emits `log.debug` records and has no effect on the
    /// face. The `try result.name(&buf)` in the reference propagates, so this
    /// returns `Result` and `init` uses `?` on it, matching that.
    fn log_variation_axes(&self, debug_mode: bool) -> Result<(), Error> {
        if !debug_mode {
            return Ok(());
        }

        // if (ct_font.copyAttribute(.variation_axes)) |axes| { ... }
        // defer axes.release(); — `CFRetained` releases on drop.
        let axes = match unsafe {
            self.font
                .attribute(objc2_core_text::kCTFontVariationAxesAttribute)
        } {
            Some(a) => a,
            None => return Ok(()),
        };
        let axes = match axes.downcast::<objc2_core_foundation::CFArray>() {
            Ok(a) => a,
            // The attribute is documented as an array of dictionaries; anything
            // else is not something the reference's typed `getValueAtIndex`
            // would produce, so there is nothing to dump.
            Err(_) => return Ok(()),
        };

        // var buf: [1024]u8 = undefined;
        let mut buf = [0u8; 1024];
        // log.debug("variation axes font={s}", .{try result.name(&buf)});
        //
        // PORT NOTE: `{s}` renders the raw name bytes; the lossy UTF-8
        // conversion is confined to the diagnostic.
        log::debug!(
            target: LOG_SCOPE,
            "variation axes font={}",
            String::from_utf8_lossy(self.name(&mut buf)?)
        );

        // const len = axes.getCount();
        let len = axes.count();
        for i in 0..len {
            // const dict = axes.getValueAtIndex(macos.foundation.Dictionary, i);
            let dict =
                unsafe { axes.value_at_index(i) } as *const objc2_core_foundation::CFDictionary;
            if dict.is_null() {
                continue;
            }
            // SAFETY: the array is retained for the body of this loop and CF
            // guarantees its elements outlive it.
            let dict = unsafe { &*dict };

            // const Key = macos.text.FontVariationAxisKey;
            // const cf_* = dict.getValue(Key.*.Value(), Key.*.key()).?;
            //
            // PORT NOTE: the reference's `.?` asserts each of the five keys is
            // present. CoreText documents all five on every axis dictionary; a
            // malformed dictionary skips the axis here rather than panicking,
            // since this is a diagnostic-only path.
            let get = |key: &objc2_core_foundation::CFString| -> *const c_void {
                unsafe { dict.value(key as *const _ as *const c_void) }
            };
            let cf_name = get(unsafe { objc2_core_text::kCTFontVariationAxisNameKey });
            let cf_id = get(unsafe { objc2_core_text::kCTFontVariationAxisIdentifierKey });
            let cf_min = get(unsafe { objc2_core_text::kCTFontVariationAxisMinimumValueKey });
            let cf_max = get(unsafe { objc2_core_text::kCTFontVariationAxisMaximumValueKey });
            let cf_def = get(unsafe { objc2_core_text::kCTFontVariationAxisDefaultValueKey });
            if cf_name.is_null()
                || cf_id.is_null()
                || cf_min.is_null()
                || cf_max.is_null()
                || cf_def.is_null()
            {
                continue;
            }

            // const namestr = cf_name.cstring(&buf, .utf8) orelse "";
            let namestr =
                unsafe { &*(cf_name as *const objc2_core_foundation::CFString) }.to_string();

            // var id_raw: c_int = 0;
            // _ = cf_id.getValue(.int, &id_raw);
            // const id: font.face.Variation.Id = @bitCast(id_raw);
            let mut id_raw: core::ffi::c_int = 0;
            let _ = unsafe {
                (*(cf_id as *const CFNumber)).value(
                    CFNumberType::IntType,
                    &mut id_raw as *mut core::ffi::c_int as *mut c_void,
                )
            };
            // `Variation.Id` is `packed struct(u32) { d, c, b, a }`, so `d` is
            // the LEAST significant byte (see `face::Variation::Id::to_u32`).
            let id_bits = id_raw as u32;
            let id = crate::face::Id {
                d: (id_bits & 0xff) as u8,
                c: ((id_bits >> 8) & 0xff) as u8,
                b: ((id_bits >> 16) & 0xff) as u8,
                a: ((id_bits >> 24) & 0xff) as u8,
            };

            let read_double = |p: *const c_void| -> f64 {
                let mut v: f64 = 0.0;
                let _ = unsafe {
                    (*(p as *const CFNumber))
                        .value(CFNumberType::DoubleType, &mut v as *mut f64 as *mut c_void)
                };
                v
            };
            // var min/max/def: f64 = 0; _ = cf_*.getValue(.double, &*);
            let min = read_double(cf_min);
            let max = read_double(cf_max);
            let def = read_double(cf_def);

            // log.debug("variation axis: name={s} id={s} min={} max={} def={}", ...);
            //
            // PORT NOTE: `id.str()` is a `[4]u8` tag rendered by `{s}` as raw
            // bytes; the lossy conversion is confined to the diagnostic.
            log::debug!(
                target: LOG_SCOPE,
                "variation axis: name={} id={} min={} max={} def={}",
                namestr,
                String::from_utf8_lossy(&id.str()),
                min,
                max,
                def
            );
        }

        Ok(())
    }

    /// Returns the font name. If allocation is required, buf will be used,
    /// but sometimes allocation isn't required and a static string is
    /// returned.
    // PORT NOTE: the reference returns `[]const u8` that may point at `buf` or at
    // CoreFoundation-owned memory (`cstringPtr`). To unify lifetimes in Rust the
    // family name is decoded (via `CFString`'s UTF-8 `Display`) and copied into
    // `buf`, returning a sub-slice of it (identical bytes); over-long names that
    // don't fit `buf` map the reference's `orelse error.OutOfMemory`.
    pub fn name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], Error> {
        let family_name = unsafe { self.font.family_name() };
        let s = family_name.to_string();
        let bytes = s.as_bytes();
        if bytes.len() > buf.len() {
            // "NULL if the internal storage of theString does not allow
            // this to be returned efficiently." In this case, we need
            // to allocate.
            return Err(Error::OutOfMemory);
        }
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(&buf[..bytes.len()])
    }

    /// Resize the font in-place. If this succeeds, the caller is responsible
    /// for clearing any glyph caches, font atlas data, etc.
    pub fn set_size(&mut self, opts: Options) -> Result<(), Error> {
        // Test-only fail-point (W9F-02): lets unit tests force the fallible
        // `setSize` arm so its error-path cleanup can be proven.
        #[cfg(test)]
        if crate::face::test_probe::set_size_failpoint_should_fail() {
            return Err(Error::FontInitFailure);
        }

        // We just create a copy and replace ourself
        let face = Face::init_font_copy(&self.font, opts)?;
        self.deinit();
        *self = face;
        Ok(())
    }

    /// Set the variation axes for this font. This will modify this font
    /// in-place.
    pub fn set_variations(&mut self, vs: &[Variation], opts: Options) -> Result<(), Error> {
        // If we have no variations, we don't need to do anything.
        if vs.is_empty() {
            return Ok(());
        }

        // Create a new font descriptor with all the variations set.
        let mut desc = unsafe { self.font.font_descriptor() };
        // defer desc.release(); — `desc` is `CFRetained` and releases on drop.
        for v in vs {
            // PORT NOTE: `Number.create(.int, @ptrCast(&v.id))` reads the packed
            // `Variation.Id` (a `u32`) as a `c_int`; on the little-endian targets
            // rustty builds for that value is exactly `v.id.to_u32()`.
            let id_raw: i32 = v.id.to_u32() as i32;
            let id = unsafe {
                CFNumber::new(
                    None,
                    CFNumberType::IntType,
                    &id_raw as *const i32 as *const c_void,
                )
            }
            .ok_or(Error::OutOfMemory)?;
            // defer id.release();
            let next = unsafe { desc.copy_with_variation(&id, v.value) };
            // desc.release(); desc = next;
            desc = next;
        }

        // Put our current size in the opts so that we don't change size.
        let mut new_opts = opts;
        new_opts.size = self.size;

        // Initialize a font based on these attributes.
        let ct_font = unsafe {
            self.font
                .copy_with_attributes(0.0, ptr::null(), Some(&desc))
        };
        // errdefer ct_font.release();
        let face = Face::init_font(ct_font, new_opts)?;
        self.deinit();
        *self = face;
        Ok(())
    }

    /// Returns true if the face has any glyphs that are colorized.
    /// To determine if an individual glyph is colorized you must use
    /// isColorGlyph.
    pub fn has_color(&self) -> bool {
        self.color.is_some()
    }

    /// Returns true if the given glyph ID is colorized.
    pub fn is_color_glyph(&self, glyph_id: u32) -> bool {
        let c = match &self.color {
            Some(c) => c,
            None => return false,
        };
        c.is_color_glyph(glyph_id)
    }

    /// Returns the glyph index for the given Unicode code point. If this
    /// face doesn't support this glyph, null is returned.
    pub fn glyph_index(&self, cp: u32) -> Option<u32> {
        // Turn UTF-32 into UTF-16 for CT API
        let mut unichars: [u16; 2] = [0, 0];
        let pair = string_get_surrogate_pair_for_long_character(cp, &mut unichars);
        let len: usize = if pair { 2 } else { 1 };

        // Get our glyphs
        let mut glyphs: [CGGlyph; 2] = [0, 0];
        if !unsafe {
            self.font.glyphs_for_characters(
                NonNull::new(unichars.as_mut_ptr()).unwrap(),
                NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                len as isize,
            )
        } {
            return None;
        }

        // We can have pairs due to chars like emoji but we expect all of them
        // to decode down into exactly one glyph ID.
        if pair {
            assert(glyphs[1] == 0);
        }

        Some(glyphs[0] as u32)
    }

    pub fn render_glyph<A: Allocator + Copy>(
        &self,
        alloc: A,
        atlas: &mut Atlas<A>,
        glyph_index: u32,
        opts: RenderOptions,
    ) -> Result<Glyph, Error> {
        let mut glyphs: [CGGlyph; 1] = [glyph_index as CGGlyph];

        // Get the bounding rect for rendering this glyph.
        // This is in a coordinate space with (0.0, 0.0)
        // in the bottom left and +Y pointing up.
        let mut rect = unsafe {
            self.font.bounding_rects_for_glyphs(
                CTFontOrientation::Horizontal,
                NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                ptr::null_mut(),
                1,
            )
        };

        // Determine whether this is a color glyph.
        let is_color = self.is_color_glyph(glyph_index);
        // And whether it's (probably) a bitmap (sbix).
        let sbix = is_color && self.color.is_some() && self.color.as_ref().unwrap().sbix;

        // If we're rendering a synthetic bold then we will gain 50% of
        // the line width on every edge, which means we should increase
        // our width and height by the line width and subtract half from
        // our origin points.
        //
        // We don't add extra size if it's a sbix color font though,
        // since bitmaps aren't affected by synthetic bold.
        if !sbix {
            if let Some(line_width) = self.synthetic_bold {
                rect.size.width += line_width;
                rect.size.height += line_width;
                rect.origin.x -= line_width / 2.0;
                rect.origin.y -= line_width / 2.0;
            }
        }

        // If our rect is smaller than a quarter pixel in either axis
        // then it has no outlines or they're too small to render.
        //
        // In this case we just return 0-sized glyph struct.
        if rect.size.width < 0.25 || rect.size.height < 0.25 {
            return Ok(Glyph {
                width: 0,
                height: 0,
                offset_x: 0,
                offset_y: 0,
                atlas_x: 0,
                atlas_y: 0,
            });
        }

        let metrics = opts.grid_metrics;
        let cell_width: f64 = metrics.cell_width as f64;
        let cell_height: f64 = metrics.cell_height as f64;

        // Next we apply any constraints to get the final size of the glyph.
        let constraint = opts.constraint;

        // We need to add the baseline position before passing to the constrain
        // function since it operates on cell-relative positions, not baseline.
        let cell_baseline: f64 = metrics.cell_baseline as f64;

        let glyph_size = constraint.constrain(
            Size {
                width: rect.size.width,
                height: rect.size.height,
                x: rect.origin.x,
                y: rect.origin.y + cell_baseline,
            },
            metrics,
            opts.constraint_width,
        );

        let mut x = glyph_size.x;
        let mut y = glyph_size.y;
        let mut width = glyph_size.width;
        let mut height = glyph_size.height;

        // We center all glyphs within the pixel-rounded and adjusted
        // cell width if it's larger than the face width, so that they
        // aren't weirdly off to the left.
        //
        // We don't do this if the glyph has a stretch constraint,
        // since in that case the position was already calculated with the
        // new cell width in mind.
        if constraint.size != ConstraintSize::Stretch {
            // We add half the difference to re-center.
            let dx = (cell_width - metrics.face_width) / 2.0;
            x += dx;
            if dx < 0.0 {
                // For negative diff (cell narrower than advance), we remove the
                // integer part and only keep the fractional adjustment needed
                // for consistent subpixel positioning.
                x -= dx.trunc();
            }
        }

        // If this is a bitmap glyph, it will always render as full pixels,
        // not fractional pixels, so we need to quantize its position and
        // size accordingly to align to full pixels so we get good results.
        if sbix {
            width = cell_width - (cell_width - width - x).round() - x.round();
            height = cell_height - (cell_height - height - y).round() - y.round();
            x = x.round();
            y = y.round();
        }

        // We make an assumption that font smoothing ("thicken")
        // adds no more than 1 extra pixel to any edge. We don't
        // add extra size if it's a sbix color font though, since
        // bitmaps aren't affected by smoothing.
        let canvas_padding: u32 = if opts.thicken && !sbix { 1 } else { 0 };

        // Our whole-pixel bearings for the final glyph.
        // The fractional portion will be included in the rasterized position.
        let px_x = (x.floor() as i32) - (canvas_padding as i32);
        let px_y = (y.floor() as i32) - (canvas_padding as i32);

        // We keep track of the fractional part of the pixel bearings, which
        // we will add as an offset when rasterizing to make sure we get the
        // correct sub-pixel position.
        let frac_x = x - x.floor();
        let frac_y = y - y.floor();

        // Add the fractional pixel to the width and height and take
        // the ceiling to get a canvas size that will definitely fit
        // our drawn glyph, including the fractional offset and font smoothing.
        let px_width = ((width + frac_x).ceil() as u32) + (2 * canvas_padding);
        let px_height = ((height + frac_y).ceil() as u32) + (2 * canvas_padding);

        // Settings that are specific to if we are rendering text or emoji.
        // PORT NOTE: the reference's anonymous struct `color` holds the color
        // flag, bit depth, color space, and `CGBitmapContext` option bits; modeled
        // here as separate locals (`is_color_ctx`, `depth`, `space`,
        // `context_opts`). `space` is a `CFRetained<CGColorSpace>` (releases on
        // drop, matching `defer color.space.release()`).
        let (is_color_ctx, depth, space, context_opts): (bool, u32, CFRetained<CGColorSpace>, u32) =
            if !is_color {
                (
                    false,
                    1,
                    CGColorSpace::with_name(Some(unsafe { kCGColorSpaceLinearGray }))
                        .ok_or(Error::OutOfMemory)?,
                    CGImageAlphaInfo::Only.0,
                )
            } else {
                (
                    true,
                    4,
                    CGColorSpace::with_name(Some(unsafe { kCGColorSpaceDisplayP3 }))
                        .ok_or(Error::OutOfMemory)?,
                    CGImageByteOrderInfo::Order32Little.0 | CGImageAlphaInfo::PremultipliedFirst.0,
                )
            };

        // This is just a safety check.
        if atlas.format().depth() as u32 != depth {
            // log.warn("font atlas color depth doesn't equal font color depth atlas={} font={}", .{
            //     atlas.format.depth(), color.depth,
            // });
            log::warn!(
                target: LOG_SCOPE,
                "font atlas color depth doesn't equal font color depth atlas={} font={}",
                atlas.format().depth(),
                depth
            );
            return Err(Error::InvalidAtlasFormat);
        }

        // Our buffer for rendering. We could cache this but glyph rasterization
        // usually stabilizes pretty quickly and is very infrequent so I think
        // the allocation overhead is acceptable compared to the cost of
        // caching it forever or having to deal with a cache lifetime.
        let n = (px_width as usize) * (px_height as usize) * (depth as usize);
        let mut buf = allocator_api2::vec::Vec::<u8, A>::with_capacity_in(n, alloc);
        buf.resize(n, 0);
        // defer alloc.free(buf); — `buf` drops at end of scope.
        // @memset(buf, 0) — `resize(n, 0)` above zero-fills.

        let ctx_raw = unsafe {
            sys::CGBitmapContextCreate(
                buf.as_mut_ptr() as *mut c_void,
                px_width as usize,
                px_height as usize,
                8,
                (px_width * depth) as usize,
                &*space,
                context_opts,
            )
        };
        let ctx: CFRetained<CGContext> =
            unsafe { CFRetained::from_raw(NonNull::new(ctx_raw).ok_or(Error::OutOfMemory)?) };
        // defer context.release(ctx); — `ctx` is `CFRetained` and releases on drop.
        let c = Some(&*ctx);

        // Perform an initial fill. This ensures that we don't have any
        // uninitialized pixels in the bitmap.
        if is_color_ctx {
            CGContext::set_rgb_fill_color(c, 0.0, 0.0, 0.0, 0.0);
        } else {
            CGContext::set_gray_fill_color(c, 0.0, 0.0);
        }
        CGContext::fill_rect(
            c,
            CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: px_width as f64,
                    height: px_height as f64,
                },
            },
        );

        // "Font smoothing" is what we call "thickening", it's an attempt
        // to compensate for optical thinning of fonts, but at this point
        // it's just something that makes the text look closer to system
        // applications if users want that.
        CGContext::set_allows_font_smoothing(c, true);
        CGContext::set_should_smooth_fonts(c, opts.thicken);

        // Subpixel positioning allows glyphs to be placed at non-integer
        // coordinates. We need this for our alignment.
        CGContext::set_allows_font_subpixel_positioning(c, true);
        CGContext::set_should_subpixel_position_fonts(c, true);

        // We don't want subpixel quantization, since we very carefully
        // manage the position of our glyphs ourselves, and dont want to
        // mess that up.
        CGContext::set_allows_font_subpixel_quantization(c, false);
        CGContext::set_should_subpixel_quantize_fonts(c, false);

        // Anti-aliasing is self explanatory.
        CGContext::set_allows_antialiasing(c, true);
        CGContext::set_should_antialias(c, true);

        // Set our color for drawing
        if is_color_ctx {
            CGContext::set_rgb_fill_color(c, 1.0, 1.0, 1.0, 1.0);
            CGContext::set_rgb_stroke_color(c, 1.0, 1.0, 1.0, 1.0);
        } else {
            let strength: f64 = opts.thicken_strength as f64;
            CGContext::set_gray_fill_color(c, strength / 255.0, 1.0);
            CGContext::set_gray_stroke_color(c, strength / 255.0, 1.0);
        }

        // If we are drawing with synthetic bold then use a fill stroke
        // which strokes the outlines of the glyph making a more bold look.
        if let Some(line_width) = self.synthetic_bold {
            CGContext::set_text_drawing_mode(c, CGTextDrawingMode::FillStroke);
            CGContext::set_line_width(c, line_width);
        }

        // Translate our drawing context so that when we draw our
        // glyph the bottom/left edge is at the correct sub-pixel
        // position. The bottom/left edges are guaranteed to be at
        // exactly [0, 0] relative to this because when we call to
        // `drawGlyphs`, we pass the negated bearings.
        CGContext::translate_ctm(
            c,
            frac_x + (canvas_padding as f64),
            frac_y + (canvas_padding as f64),
        );

        // Scale the drawing context so that when we draw
        // our glyph it's stretched to the constrained size.
        CGContext::scale_ctm(c, width / rect.size.width, height / rect.size.height);

        // Draw our glyph.
        //
        // We offset the position by the negated bearings so that the
        // glyph is drawn at exactly [0, 0], which is then offset to
        // the appropriate fractional position by the translation we
        // did before scaling.
        let mut positions: [CGPoint; 1] = [CGPoint {
            x: -rect.origin.x,
            y: -rect.origin.y,
        }];
        unsafe {
            self.font.draw_glyphs(
                NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                NonNull::new(positions.as_mut_ptr()).unwrap(),
                1,
                &ctx,
            );
        }

        // Write our rasterized glyph to the atlas.
        let region = atlas.reserve(alloc, px_width, px_height)?;
        atlas.set(region, &buf);

        // This should be the distance from the bottom of
        // the cell to the top of the glyph's bounding box.
        let offset_y: i32 = px_y + (px_height as i32);

        // This should be the distance from the left of
        // the cell to the left of the glyph's bounding box.
        let offset_x: i32 = px_x;

        Ok(Glyph {
            width: px_width,
            height: px_height,
            offset_x,
            offset_y,
            atlas_x: region.x,
            atlas_y: region.y,
        })
    }

    /// Get the `FaceMetrics` for this face.
    pub fn get_metrics(&self) -> FaceMetrics {
        let ct_font = &self.font;

        // Read the 'head' table out of the font data.
        let head_: Option<Head> = {
            // macOS bitmap-only fonts use a 'bhed' tag rather than 'head', but
            // the table format is byte-identical to the 'head' table, so if we
            // can't find 'head' we try 'bhed' instead before failing.
            //
            // ref: https://fontforge.org/docs/techref/bitmaponlysfnt.html
            match copy_table_data(ct_font, b"head").or_else(|| copy_table_data(ct_font, b"bhed")) {
                None => None,
                Some(data) => {
                    let slice = unsafe { data.as_bytes_unchecked() };
                    // catch |err| { log.warn("error parsing head table: {}", .{err}); break :head null; }
                    match Head::init(slice) {
                        Ok(v) => Some(v),
                        Err(err) => {
                            log::warn!(target: LOG_SCOPE, "error parsing head table: {err:?}");
                            None
                        }
                    }
                }
            }
        };

        // Read the 'post' table out of the font data.
        let post_: Option<Post> = match copy_table_data(ct_font, b"post") {
            None => None,
            Some(data) => {
                let slice = unsafe { data.as_bytes_unchecked() };
                // catch |err| { log.warn("error parsing post table: {}", .{err}); break :post null; }
                match Post::init(slice) {
                    Ok(v) => Some(v),
                    Err(err) => {
                        log::warn!(target: LOG_SCOPE, "error parsing post table: {err:?}");
                        None
                    }
                }
            }
        };

        // Read the 'OS/2' table out of the font data if it's available.
        let os2_: Option<OS2> = match copy_table_data(ct_font, b"OS/2") {
            None => None,
            Some(data) => {
                let slice = unsafe { data.as_bytes_unchecked() };
                // catch |err| { log.warn("error parsing OS/2 table: {}", .{err}); break :os2 null; }
                match OS2::init(slice) {
                    Ok(v) => Some(v),
                    Err(err) => {
                        log::warn!(target: LOG_SCOPE, "error parsing OS/2 table: {err:?}");
                        None
                    }
                }
            }
        };

        // Read the 'hhea' table out of the font data.
        let hhea_: Option<Hhea> = match copy_table_data(ct_font, b"hhea") {
            None => None,
            Some(data) => {
                let slice = unsafe { data.as_bytes_unchecked() };
                // catch |err| { log.warn("error parsing hhea table: {}", .{err}); break :hhea null; }
                match Hhea::init(slice) {
                    Ok(v) => Some(v),
                    Err(err) => {
                        log::warn!(target: LOG_SCOPE, "error parsing hhea table: {err:?}");
                        None
                    }
                }
            }
        };

        let units_per_em: f64 = if let Some(head) = head_ {
            head.unitsPerEm as f64
        } else {
            unsafe { ct_font.units_per_em() as f64 }
        };
        let px_per_em: f64 = unsafe { ct_font.size() };
        let px_per_unit: f64 = px_per_em / units_per_em;

        let (ascent, descent, line_gap): (f64, f64, f64) = 'vertical_metrics: {
            // If we couldn't get the hhea table, rely on metrics from CoreText.
            let hhea = match hhea_ {
                Some(h) => h,
                None => {
                    break 'vertical_metrics (
                        unsafe { self.font.ascent() },
                        -unsafe { self.font.descent() },
                        unsafe { self.font.leading() },
                    );
                }
            };

            let hhea_ascent: f64 = hhea.ascender as f64;
            let hhea_descent: f64 = hhea.descender as f64;
            let hhea_line_gap: f64 = hhea.lineGap as f64;

            // If our font has no OS/2 table, then we just
            // blindly use the metrics from the hhea table.
            let os2 = match os2_ {
                Some(o) => o,
                None => {
                    break 'vertical_metrics (
                        hhea_ascent * px_per_unit,
                        hhea_descent * px_per_unit,
                        hhea_line_gap * px_per_unit,
                    );
                }
            };

            let os2_ascent: f64 = os2.sTypoAscender as f64;
            let os2_descent: f64 = os2.sTypoDescender as f64;
            let os2_line_gap: f64 = os2.sTypoLineGap as f64;

            // If the font says to use typo metrics, trust it.
            if os2.fsSelection.use_typo_metrics {
                break 'vertical_metrics (
                    os2_ascent * px_per_unit,
                    os2_descent * px_per_unit,
                    os2_line_gap * px_per_unit,
                );
            }

            // Otherwise we prefer the height metrics from 'hhea' if they
            // are available, or else OS/2 sTypo* metrics, and if all else
            // fails then we use OS/2 usWin* metrics.
            //
            // This is not "standard" behavior, but it's our best bet to
            // account for fonts being... just weird. It's pretty much what
            // FreeType does to get its generic ascent and descent metrics.

            if hhea.ascender != 0 || hhea.descender != 0 {
                break 'vertical_metrics (
                    hhea_ascent * px_per_unit,
                    hhea_descent * px_per_unit,
                    hhea_line_gap * px_per_unit,
                );
            }

            if os2_ascent != 0.0 || os2_descent != 0.0 {
                break 'vertical_metrics (
                    os2_ascent * px_per_unit,
                    os2_descent * px_per_unit,
                    os2_line_gap * px_per_unit,
                );
            }

            let win_ascent: f64 = os2.usWinAscent as f64;
            let win_descent: f64 = os2.usWinDescent as f64;
            break 'vertical_metrics (
                win_ascent * px_per_unit,
                // usWinDescent is *positive* -> down unlike sTypoDescender
                // and hhea.Descender, so we flip its sign to fix this.
                -win_descent * px_per_unit,
                0.0,
            );
        };

        let (underline_position, underline_thickness): (Option<f64>, Option<f64>) = 'ul: {
            let post = match post_ {
                Some(p) => p,
                None => break 'ul (None, None),
            };

            // Some fonts have degenerate 'post' tables where the underline
            // thickness (and often position) are 0. We consider them null
            // if this is the case and use our own fallbacks when we calculate.
            let has_broken_underline = post.underlineThickness == 0;

            // If the underline position isn't 0 then we do use it,
            // even if the thickness is't properly specified.
            let pos: Option<f64> = if has_broken_underline && post.underlinePosition == 0 {
                None
            } else {
                Some((post.underlinePosition as f64) * px_per_unit)
            };

            let thick: Option<f64> = if has_broken_underline {
                None
            } else {
                Some((post.underlineThickness as f64) * px_per_unit)
            };

            (pos, thick)
        };

        // Similar logic to the underline above.
        let (strikethrough_position, strikethrough_thickness): (Option<f64>, Option<f64>) = 'st: {
            let os2 = match os2_ {
                Some(o) => o,
                None => break 'st (None, None),
            };

            let has_broken_strikethrough = os2.yStrikeoutSize == 0;

            let pos: Option<f64> = if has_broken_strikethrough && os2.yStrikeoutPosition == 0 {
                None
            } else {
                Some((os2.yStrikeoutPosition as f64) * px_per_unit)
            };

            let thick: Option<f64> = if has_broken_strikethrough {
                None
            } else {
                Some((os2.yStrikeoutSize as f64) * px_per_unit)
            };

            (pos, thick)
        };

        // We fall back to whatever CoreText does if the
        // OS/2 table doesn't specify a cap or ex height.
        let (cap_height, ex_height): (f64, f64) = 'heights: {
            let os2 = match os2_ {
                Some(o) => o,
                None => {
                    break 'heights (unsafe { ct_font.cap_height() }, unsafe {
                        ct_font.x_height()
                    });
                }
            };

            (
                if let Some(s_cap_height) = os2.sCapHeight {
                    (s_cap_height as f64) * px_per_unit
                } else {
                    unsafe { ct_font.cap_height() }
                },
                if let Some(sx_height) = os2.sxHeight {
                    (sx_height as f64) * px_per_unit
                } else {
                    unsafe { ct_font.x_height() }
                },
            )
        };

        // Cell width is calculated by calculating the widest width of the
        // visible ASCII characters. Usually 'M' is widest but we just take
        // whatever is widest.
        //
        // ASCII height is calculated as the height of the overall bounding
        // box of the same characters.
        let (cell_width, ascii_height): (f64, f64) = {
            // Build a comptime array of all the ASCII chars
            // PORT NOTE: `len = 127 - 32 = 95`.
            let unichars: [u16; 95] = {
                let mut result = [0u16; 95];
                let mut i: u16 = 32;
                while i < 127 {
                    result[(i - 32) as usize] = i;
                    i += 1;
                }
                result
            };

            // Get our glyph IDs for the ASCII chars
            let mut glyphs: [CGGlyph; 95] = [0; 95];
            let _ = unsafe {
                ct_font.glyphs_for_characters(
                    NonNull::new(unichars.as_ptr() as *mut u16).unwrap(),
                    NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                    95,
                )
            };

            // Get all our advances
            let mut advances: [CGSize; 95] = [CGSize {
                width: 0.0,
                height: 0.0,
            }; 95];
            let _ = unsafe {
                ct_font.advances_for_glyphs(
                    CTFontOrientation::Horizontal,
                    NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                    advances.as_mut_ptr(),
                    95,
                )
            };

            // Find the maximum advance
            let mut max: f64 = 0.0;
            let mut i: usize = 0;
            while i < advances.len() {
                max = advances[i].width.max(max);
                i += 1;
            }

            // Get the overall bounding rect for the glyphs
            let rect = unsafe {
                ct_font.bounding_rects_for_glyphs(
                    CTFontOrientation::Horizontal,
                    NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                    ptr::null_mut(),
                    95,
                )
            };

            (max, rect.size.height)
        };

        // Measure "水" (CJK water ideograph, U+6C34) for our ic width.
        let ic_width: Option<f64> = 'ic_width: {
            let glyph = match self.glyph_index('水' as u32) {
                Some(g) => g,
                None => break 'ic_width None,
            };

            let mut glyphs: [CGGlyph; 1] = [glyph as CGGlyph];
            let advance = unsafe {
                ct_font.advances_for_glyphs(
                    CTFontOrientation::Horizontal,
                    NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                    ptr::null_mut(),
                    1,
                )
            };

            let bounds = unsafe {
                ct_font.bounding_rects_for_glyphs(
                    CTFontOrientation::Horizontal,
                    NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                    ptr::null_mut(),
                    1,
                )
            };

            // If the advance of the glyph is less than the width of the actual
            // glyph then we just treat it as invalid since it's probably wrong
            // and using it for size normalization will instead make the font
            // way too big.
            //
            // This can sometimes happen if there's a CJK font that has been
            // patched with the nerd fonts patcher and it butchers the advance
            // values so the advance ends up half the width of the actual glyph.
            if bounds.size.width > advance {
                // var buf: [1024]u8 = undefined;
                // const font_name = self.name(&buf) catch "<Error getting font name>";
                // log.warn(
                //     "(getMetrics) Width of glyph '水' for font \"{s}\" is greater than its advance ({d} > {d}), discarding ic_width metric.",
                //     .{ font_name, bounds.size.width, advance },
                // );
                //
                // PORT NOTE: `{s}` renders the raw name bytes; the lossy UTF-8
                // conversion is confined to the diagnostic.
                let mut buf = [0u8; 1024];
                let font_name = self
                    .name(&mut buf)
                    .map(String::from_utf8_lossy)
                    .unwrap_or(std::borrow::Cow::Borrowed("<Error getting font name>"));
                log::warn!(
                    target: LOG_SCOPE,
                    "(getMetrics) Width of glyph '水' for font \"{}\" is greater than its advance ({} > {}), discarding ic_width metric.",
                    font_name,
                    bounds.size.width,
                    advance
                );
                break 'ic_width None;
            }

            Some(advance)
        };

        FaceMetrics {
            px_per_em,

            cell_width,

            ascent,
            descent,
            line_gap,

            underline_position,
            underline_thickness,

            strikethrough_position,
            strikethrough_thickness,

            cap_height: Some(cap_height),
            ex_height: Some(ex_height),
            ascii_height: Some(ascii_height),
            ic_width,
        }
    }

    /// Copy the font table data for the given tag.
    pub fn copy_table<A: Allocator + Copy>(
        &self,
        _alloc: A,
        tag: &[u8; 4],
    ) -> Result<Option<Vec<u8>>, Error> {
        // PORT NOTE: the reference threads `alloc` into the buffer allocation; the
        // table buffer here is a global-allocator `Vec<u8>` (the allocator choice
        // is not observable), so `_alloc` is accepted for call-shape fidelity only.
        let data = match copy_table_data(&self.font, tag) {
            None => return Ok(None),
            Some(d) => d,
        };
        // defer data.release(); — `data` is `CFRetained` and releases on drop.

        let bytes = unsafe { data.as_bytes_unchecked() };
        Ok(Some(bytes.to_vec()))
    }
}

/// The state associated with a font face that may have colorized glyphs.
/// This is used to determine if a specific glyph ID is colorized.
pub struct ColorState {
    /// True if there is an sbix font table. For now, the mere presence
    /// of an sbix font table causes us to assume the glyph is colored.
    /// We can improve this later.
    pub sbix: bool,

    /// The SVG font table data (if any), which we can use to determine
    /// if a glyph is present in the SVG table.
    // PORT NOTE: the reference holds a parsed `?SVG` (its `records` slice borrows
    // into the retained `svg_data: ?*Data`). Rust can't store that borrow next to
    // its owner without self-reference, so `ColorState` owns the raw table bytes
    // (`svg_data: Option<Vec<u8>>`, copied out of the `CFData`) and re-parses them
    // via `SVG::init` in `is_color_glyph`; `init` already validated they parse.
    // This owns the same bytes the reference kept alive and yields identical
    // `hasGlyph` results.
    pub svg_data: Option<Vec<u8>>,
}

impl ColorState {
    pub fn init(f: &CTFont) -> Result<ColorState, Error> {
        // sbix is true if the table exists in the font data at all.
        // In the future we probably want to actually parse it and
        // check for glyphs.
        let sbix: bool = match copy_table_data(f, b"sbix") {
            None => false,
            Some(data) => {
                // data.release();
                // break :sbix data.getLength() > 0;
                // PORT NOTE: `!is_empty()` is clippy's preferred spelling of the
                // reference's `getLength() > 0`.
                !unsafe { data.as_bytes_unchecked() }.is_empty()
            }
        };

        // Read the SVG table out of the font data.
        let svg_data: Option<Vec<u8>> = match copy_table_data(f, b"SVG ") {
            None => None,
            Some(data) => {
                // errdefer data.release();
                let bytes = unsafe { data.as_bytes_unchecked() };
                match SVG::init(bytes) {
                    Ok(_svg) => Some(bytes.to_vec()),
                    Err(_err) => {
                        // return switch (err) {
                        //     error.EndOfStream, error.SVGVersionNotSupported,
                        //     => error.InvalidSVGTable,
                        // };
                        return Err(Error::InvalidSVGTable);
                    }
                }
            }
        };

        Ok(ColorState { sbix, svg_data })
    }

    // pub fn deinit(self: *const ColorState) void { if (self.svg_data) |v| v.release(); }
    //
    // PORT NOTE: `svg_data` is an owned `Vec<u8>` that frees on `Drop`; no explicit
    // deinit is needed.

    /// Returns true if the given glyph ID is colored.
    pub fn is_color_glyph(&self, glyph_id: u32) -> bool {
        // Our font system uses 32-bit glyph IDs for special values but
        // actual fonts only contain 16-bit glyph IDs so if we can't cast
        // into it it must be false.
        let glyph_u16: u16 = match u16::try_from(glyph_id) {
            Ok(v) => v,
            Err(_) => return false,
        };

        // sbix is always true for now
        if self.sbix {
            return true;
        }

        // if we have svg data, check it
        if let Some(bytes) = &self.svg_data {
            if let Ok(svg) = SVG::init(bytes) {
                if svg.has_glyph(glyph_u16) {
                    return true;
                }
            }
        }

        false
    }
}

/// If true, the default font features should be disabled for the given face.
// PORT NOTE: `quirks.disableDefaultFontFeatures`.
fn quirks_disable_default_font_features(face: &Face) -> bool {
    let _ = face;

    // This function used to do something, but we integrated the logic
    // we checked for directly into our shaping algorithm. It's likely
    // there are other broken fonts for other reasons so I'm keeping this
    // around so its easy to add more checks in the future.
    false
}

/// Build a big-endian FourCC `CTFontTableTag` from a 4-byte tag.
// PORT NOTE: `macos.text.FontTableTag.init(v)` = `@bitCast(FourCharCode.init(v))`, i.e.
// `(v[0]<<24) | (v[1]<<16) | (v[2]<<8) | v[3]`.
fn fourcc(v: &[u8; 4]) -> u32 {
    ((v[0] as u32) << 24) | ((v[1] as u32) << 16) | ((v[2] as u32) << 8) | (v[3] as u32)
}

/// Copy the raw font-table `CFData` for `tag`, or `None` if absent.
// PORT NOTE: `ct_font.copyTable(FontTableTag.init(tag))`.
// (crate-visible: `discovery`'s `Score.score` reads the 'head'/'OS/2' tables the
// same way.)
pub(crate) fn copy_table_data(font: &CTFont, tag: &[u8; 4]) -> Option<CFRetained<CFData>> {
    unsafe { font.table(fourcc(tag), CTFontTableOptions::NoOptions) }
}

/// Create a no-copy `CFData` view over `source` (kept alive by the caller).
// PORT NOTE: `macos.foundation.Data.createWithBytesNoCopy(source)` — the `null`
// allocator + `kCFAllocatorNull` deallocator mean CoreFoundation neither copies
// nor frees `source`.
fn new_data_no_copy(source: &[u8]) -> Option<CFRetained<CFData>> {
    unsafe {
        CFData::with_bytes_no_copy(
            None,
            source.as_ptr(),
            source.len() as isize,
            kCFAllocatorNull,
        )
    }
}

/// Create a UTF-8 `CFString` for the given bytes (test helper).
// PORT NOTE: `macos.foundation.String.createWithBytes(bs, .utf8, false)`.
#[cfg(test)]
fn new_string_utf8(bs: &[u8]) -> Option<CFRetained<objc2_core_foundation::CFString>> {
    unsafe {
        objc2_core_foundation::CFString::with_bytes(
            None,
            bs.as_ptr(),
            bs.len() as isize,
            objc2_core_foundation::CFStringBuiltInEncodings::EncodingUTF8.0,
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;
    use objc2_core_text::CTFontDescriptor;
    use rustty_font::atlas::Format;
    use rustty_font::embedded;
    use rustty_font::metrics::Metrics;

    // PORT NOTE: `font.face.Options` on the coretext backend has
    // `freetype_load_flags: void`; this helper builds it from just the size,
    // matching the reference tests' `.{ .size = ... }`.
    fn opts(points: f32) -> Options {
        Options {
            size: DesiredSize {
                points,
                xdpi: super::super::DEFAULT_DPI,
                ydpi: super::super::DEFAULT_DPI,
            },
            freetype_load_flags: super::super::FREETYPE_LOAD_FLAGS_DEFAULT,
        }
    }

    /// Build a CTFont for a named system font at the given size (test helper).
    // PORT NOTE: the shared `String -> FontDescriptor -> Font` dance the tests repeat.
    fn named_font(name: &str, size: f64) -> CFRetained<CTFont> {
        let cf_name = new_string_utf8(name.as_bytes()).unwrap();
        let desc = unsafe { CTFontDescriptor::with_name_and_size(&cf_name, size) };
        unsafe { CTFont::with_font_descriptor(&desc, size, ptr::null()) }
    }

    // test {} (unnamed): Monaco, render all visible ASCII.
    #[test]
    fn render_ascii() -> Result<(), Error> {
        let alloc = Global;

        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        // defer atlas.deinit(alloc);

        let ct_font = named_font("Monaco", 12.0);
        // defer ...release();

        let mut face = Face::init_font_copy(&ct_font, opts(12.0))?;
        // defer face.deinit();

        // Generate all visible ASCII
        let mut i: u8 = 32;
        while i < 127 {
            assert!(face.glyph_index(i as u32).is_some());
            face.render_glyph(
                alloc,
                &mut atlas,
                face.glyph_index(i as u32).unwrap(),
                RenderOptions {
                    grid_metrics: Metrics::calc(face.get_metrics()),
                    ..Default::default()
                },
            )?;
            i += 1;
        }

        face.deinit();
        atlas.deinit(alloc);
        Ok(())
    }

    // test "name"
    #[test]
    fn name() -> Result<(), Error> {
        let ct_font = named_font("Menlo", 12.0);
        // defer ...release();

        let mut face = Face::init_font_copy(&ct_font, opts(12.0))?;
        // defer face.deinit();

        let mut buf = [0u8; 1024];
        let font_name = face.name(&mut buf)?;
        assert_eq!(font_name, b"Menlo");

        face.deinit();
        Ok(())
    }

    // test "emoji"
    #[test]
    fn emoji() -> Result<(), Error> {
        let ct_font = named_font("Apple Color Emoji", 18.0);
        // defer ...release();

        let mut face = Face::init_font_copy(&ct_font, opts(18.0))?;
        // defer face.deinit();

        // Glyph index check
        {
            let id = face.glyph_index('🥸' as u32).unwrap();
            assert!(face.is_color_glyph(id));
        }

        face.deinit();
        Ok(())
    }

    // test "in-memory"
    #[test]
    fn in_memory() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::REGULAR;

        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        // defer atlas.deinit(alloc);

        // var lib = try font.Library.init(alloc); defer lib.deinit();
        let mut face = Face::init(
            crate::library::Library::init().unwrap(),
            test_font,
            opts(12.0),
        )?;
        // defer face.deinit();

        // Generate all visible ASCII
        let mut i: u8 = 32;
        while i < 127 {
            assert!(face.glyph_index(i as u32).is_some());
            face.render_glyph(
                alloc,
                &mut atlas,
                face.glyph_index(i as u32).unwrap(),
                RenderOptions {
                    grid_metrics: Metrics::calc(face.get_metrics()),
                    ..Default::default()
                },
            )?;
            i += 1;
        }

        face.deinit();
        atlas.deinit(alloc);
        Ok(())
    }

    // test "variable"
    #[test]
    fn variable() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::VARIABLE;

        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        // defer atlas.deinit(alloc);

        let mut face = Face::init(
            crate::library::Library::init().unwrap(),
            test_font,
            opts(12.0),
        )?;
        // defer face.deinit();

        // Generate all visible ASCII
        let mut i: u8 = 32;
        while i < 127 {
            assert!(face.glyph_index(i as u32).is_some());
            face.render_glyph(
                alloc,
                &mut atlas,
                face.glyph_index(i as u32).unwrap(),
                RenderOptions {
                    grid_metrics: Metrics::calc(face.get_metrics()),
                    ..Default::default()
                },
            )?;
            i += 1;
        }

        face.deinit();
        atlas.deinit(alloc);
        Ok(())
    }

    // test "variable set variation"
    #[test]
    fn variable_set_variation() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::VARIABLE;

        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        // defer atlas.deinit(alloc);

        let mut face = Face::init(
            crate::library::Library::init().unwrap(),
            test_font,
            opts(12.0),
        )?;
        // defer face.deinit();

        face.set_variations(
            &[Variation {
                id: super::super::Id::init(b"wght"),
                value: 400.0,
            }],
            opts(12.0),
        )?;

        // Generate all visible ASCII
        let mut i: u8 = 32;
        while i < 127 {
            assert!(face.glyph_index(i as u32).is_some());
            face.render_glyph(
                alloc,
                &mut atlas,
                face.glyph_index(i as u32).unwrap(),
                RenderOptions {
                    grid_metrics: Metrics::calc(face.get_metrics()),
                    ..Default::default()
                },
            )?;
            i += 1;
        }

        face.deinit();
        atlas.deinit(alloc);
        Ok(())
    }

    // test "svg font table"
    #[test]
    fn svg_font_table() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::JULIA_MONO;

        let mut face = Face::init(
            crate::library::Library::init().unwrap(),
            test_font,
            opts(12.0),
        )?;
        // defer face.deinit();

        let table = face.copy_table(alloc, b"SVG ")?.unwrap();
        // defer alloc.free(table);

        assert!(!table.is_empty());

        face.deinit();
        Ok(())
    }

    // test "glyphIndex colored vs text"
    #[test]
    fn glyph_index_colored_vs_text() -> Result<(), Error> {
        let test_font = embedded::JULIA_MONO;

        let mut face = Face::init(
            crate::library::Library::init().unwrap(),
            test_font,
            opts(12.0),
        )?;
        // defer face.deinit();

        {
            let glyph = face.glyph_index('A' as u32).unwrap();
            assert_eq!(4, glyph);
            assert!(!face.is_color_glyph(glyph));
        }

        {
            let glyph = face.glyph_index(0xE800).unwrap();
            assert_eq!(11482, glyph);
            assert!(face.is_color_glyph(glyph));
        }

        face.deinit();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // task-14c-4 (S90-F2, and fix round 1 F7): the restored
    // `if (comptime builtin.mode == .Debug)` variation-axes dump at
    // `face/coretext.zig:118-162`. This is THE site the ledger row named as
    // elided; the freetype sibling test covers the other backend's copy.
    //
    // No device or environment is required (CoreText is always present on the
    // macOS host these tests build for), so neither has a skip arm and no
    // `RUSTTY_REQUIRE_*` strict-mode pivot applies.
    // -----------------------------------------------------------------------

    /// The RELEASE arm. `debug_mode == false` is the port of
    /// `comptime builtin.mode == .Debug` being false, and it must emit NOTHING
    /// even for a font that HAS variation axes.
    #[test]
    fn log_variation_axes_is_silent_when_not_debug_mode() -> Result<(), Error> {
        let mut face = Face::init(
            crate::library::Library::init().unwrap(),
            embedded::VARIABLE,
            opts(14.0),
        )?;

        let cap = crate::log_capture::start();
        face.log_variation_axes(false)?;
        let messages = cap.messages();
        drop(cap);

        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
        face.deinit();
        Ok(())
    }

    /// The DEBUG arm, positive control: the header debug plus exactly one debug
    /// per axis, at the reference's `.font_face` scope.
    ///
    /// Asserts the FULL captured vector. The per-axis payload is checked
    /// STRUCTURALLY rather than as a byte-for-byte golden: CoreText resolves an
    /// axis's `name` through the OS's localized font-name tables, so pinning the
    /// literal string would make this test a hostage to the host's locale and
    /// macOS version. The record COUNT, the level, the target, and the message
    /// PREFIX are all pinned, which is what the restoration is actually about.
    #[test]
    fn log_variation_axes_emits_the_header_and_one_record_per_axis() -> Result<(), Error> {
        let mut face = Face::init(
            crate::library::Library::init().unwrap(),
            embedded::VARIABLE,
            opts(14.0),
        )?;

        let cap = crate::log_capture::start();
        face.log_variation_axes(true)?;
        let messages = cap.messages();
        drop(cap);

        // The header, byte for byte.
        assert_eq!(
            (
                log::Level::Debug,
                "font_face".to_string(),
                "variation axes font=JetBrains Mono".to_string()
            ),
            messages[0],
            "captured: {messages:?}"
        );
        // JetBrainsMono-Variable has exactly one axis (`wght`), the same count
        // the freetype sibling test pins.
        assert_eq!(2, messages.len(), "captured: {messages:?}");
        for record in &messages[1..] {
            assert_eq!(log::Level::Debug, record.0);
            assert_eq!("font_face", record.1);
            assert!(
                record.2.starts_with("variation axis: name=") && record.2.contains(" id=wght "),
                "unexpected axis record: {record:?}"
            );
        }

        face.deinit();
        Ok(())
    }
}
