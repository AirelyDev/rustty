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
//   * `log` is a scoped logger; rustty has no logging facade wired into this crate,
//     so all `log.*` calls are ported as verbatim comments and are no-ops.

use allocator_api2::alloc::Allocator;
use core::ffi::c_void;
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

    // /// Harfbuzz font corresponding to this face. We only use this
    // /// if we're using Harfbuzz.
    // hb_font: if (harfbuzz_shaper) harfbuzz.Font else void,
    //
    // PORT NOTE: `harfbuzz_shaper == false` on the `coretext` backend, so this
    // field is `void` and is omitted; it returns with the `coretext_harfbuzz`
    // backend port.
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
    const HARFBUZZ_SHAPER: bool = false;

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
        // PORT NOTE: `harfbuzz_shaper == false` — no Harfbuzz font is created.
        let _ = Self::HARFBUZZ_SHAPER;

        let color: Option<ColorState> = if traits.0 & CTFontSymbolicTraits::TraitColorGlyphs.0 != 0
        {
            Some(ColorState::init(&ct_font)?)
        } else {
            None
        };
        // errdefer if (color) |v| v.deinit(); — `ColorState` owns its data and
        // drops normally on the error paths below (there are none after this).

        let mut result = Face {
            font: ct_font,
            color,
            size: opts.size,
            quirks_disable_default_font_features: false,
            synthetic_bold: None,
        };
        result.quirks_disable_default_font_features = quirks_disable_default_font_features(&result);

        // In debug mode, we output information about available variation axes,
        // if they exist.
        //
        // PORT NOTE (log-as-comment): this `if (comptime builtin.mode == .Debug)`
        // block is purely diagnostic (it logs the font's variation axes via
        // `log.debug`, reading them through `copyAttribute(.variation_axes)` and a
        // `Dictionary`/`FontVariationAxisKey` walk) and has no behavioral effect.
        // Ported verbatim as a comment:
        //
        // if (comptime builtin.mode == .Debug) {
        //     if (ct_font.copyAttribute(.variation_axes)) |axes| {
        //         defer axes.release();
        //         var buf: [1024]u8 = undefined;
        //         log.debug("variation axes font={s}", .{try result.name(&buf)});
        //         const len = axes.getCount();
        //         for (0..len) |i| {
        //             const dict = axes.getValueAtIndex(macos.foundation.Dictionary, i);
        //             const Key = macos.text.FontVariationAxisKey;
        //             const cf_name = dict.getValue(Key.name.Value(), Key.name.key()).?;
        //             const cf_id = dict.getValue(Key.identifier.Value(), Key.identifier.key()).?;
        //             const cf_min = dict.getValue(Key.minimum_value.Value(), Key.minimum_value.key()).?;
        //             const cf_max = dict.getValue(Key.maximum_value.Value(), Key.maximum_value.key()).?;
        //             const cf_def = dict.getValue(Key.default_value.Value(), Key.default_value.key()).?;
        //             const namestr = cf_name.cstring(&buf, .utf8) orelse "";
        //             var id_raw: c_int = 0;
        //             _ = cf_id.getValue(.int, &id_raw);
        //             const id: font.face.Variation.Id = @bitCast(id_raw);
        //             var min: f64 = 0;
        //             _ = cf_min.getValue(.double, &min);
        //             var max: f64 = 0;
        //             _ = cf_max.getValue(.double, &max);
        //             var def: f64 = 0;
        //             _ = cf_def.getValue(.double, &def);
        //             log.debug("variation axis: name={s} id={s} min={} max={} def={}", .{
        //                 namestr, id.str(), min, max, def,
        //             });
        //         }
        //     }
        // }

        Ok(result)
    }

    pub fn deinit(&mut self) {
        // self.font.release();
        // if (comptime harfbuzz_shaper) self.hb_font.destroy();
        // if (self.color) |v| v.deinit();
        // self.* = undefined;
        //
        // PORT NOTE: RAII — `self.font` (`CFRetained<CTFont>`) and `self.color`
        // (`ColorState`, which owns its bytes) release/free on `Drop`, reproducing
        // the reference's manual `release`/`deinit` + `self.* = undefined`. This is
        // a no-op; the storage is freed when the `Face` is dropped or overwritten.
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
                    // PORT NOTE (log-as-comment): the reference
                    // `catch |err| { log.warn("error parsing head table: {}", .{err}); break :head null; }`
                    // discards a parse failure as `null`; the warning is a no-op here.
                    Head::init(slice).ok()
                }
            }
        };

        // Read the 'post' table out of the font data.
        let post_: Option<Post> = match copy_table_data(ct_font, b"post") {
            None => None,
            Some(data) => {
                let slice = unsafe { data.as_bytes_unchecked() };
                // PORT NOTE (log-as-comment): `catch |err| { log.warn("error parsing post table: {}", .{err}); break :post null; }`.
                Post::init(slice).ok()
            }
        };

        // Read the 'OS/2' table out of the font data if it's available.
        let os2_: Option<OS2> = match copy_table_data(ct_font, b"OS/2") {
            None => None,
            Some(data) => {
                let slice = unsafe { data.as_bytes_unchecked() };
                // PORT NOTE (log-as-comment): `catch |err| { log.warn("error parsing OS/2 table: {}", .{err}); break :os2 null; }`.
                OS2::init(slice).ok()
            }
        };

        // Read the 'hhea' table out of the font data.
        let hhea_: Option<Hhea> = match copy_table_data(ct_font, b"hhea") {
            None => None,
            Some(data) => {
                let slice = unsafe { data.as_bytes_unchecked() };
                // PORT NOTE (log-as-comment): `catch |err| { log.warn("error parsing hhea table: {}", .{err}); break :hhea null; }`.
                Hhea::init(slice).ok()
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
// PORT: `quirks.disableDefaultFontFeatures`.
fn quirks_disable_default_font_features(face: &Face) -> bool {
    let _ = face;

    // This function used to do something, but we integrated the logic
    // we checked for directly into our shaping algorithm. It's likely
    // there are other broken fonts for other reasons so I'm keeping this
    // around so its easy to add more checks in the future.
    false
}

/// Build a big-endian FourCC `CTFontTableTag` from a 4-byte tag.
// PORT: `macos.text.FontTableTag.init(v)` = `@bitCast(FourCharCode.init(v))`, i.e.
// `(v[0]<<24) | (v[1]<<16) | (v[2]<<8) | v[3]`.
fn fourcc(v: &[u8; 4]) -> u32 {
    ((v[0] as u32) << 24) | ((v[1] as u32) << 16) | ((v[2] as u32) << 8) | (v[3] as u32)
}

/// Copy the raw font-table `CFData` for `tag`, or `None` if absent.
// PORT: `ct_font.copyTable(FontTableTag.init(tag))`.
// (crate-visible: `discovery`'s `Score.score` reads the 'head'/'OS/2' tables the
// same way.)
pub(crate) fn copy_table_data(font: &CTFont, tag: &[u8; 4]) -> Option<CFRetained<CFData>> {
    unsafe { font.table(fourcc(tag), CTFontTableOptions::NoOptions) }
}

/// Create a no-copy `CFData` view over `source` (kept alive by the caller).
// PORT: `macos.foundation.Data.createWithBytesNoCopy(source)` — the `null`
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
// PORT: `macos.foundation.String.createWithBytes(bs, .utf8, false)`.
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
    // PORT: the shared `String -> FontDescriptor -> Font` dance the tests repeat.
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
}
