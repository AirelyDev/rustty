//! rustty-font-shaper::face::web_canvas
//!
//! PORTS: ghostty `src/font/face/web_canvas.zig` (561 loc) — the browser/wasm
//! Canvas-backed font `Face` (`.web_canvas` backend). Live port, cfg-gated to
//! `wasm32` (the whole module is compiled only for the web_canvas backend).
//!
//! FAITHFUL DIVERGENCES (this is what makes the reference — which is internally
//! broken — actually compile + work in the modern tree):
//!
//!   1. `zig-js` → `extern "C"` Canvas FFI. The reference is built on the
//!      external `mitchellh/zig-js` package (`js.Object`/`js.global`/`canvas.call
//!      (…, "measureText"/"getImageData"/"createElement"/"getContext"/"clearRect"/
//!      "fillRect"/"fillText"/"append"/"set")`). We do NOT vendor zig-js. Instead
//!      this module declares a minimal `#[link(wasm_import_module = "env")]`
//!      import layer (`rustty_wc_*`) covering EXACTLY the Canvas ops web_canvas
//!      uses, passing primitives + pointer/len buffers only (no JS objects cross
//!      the boundary). Canvas/context objects live in an integer handle table on
//!      the JS side. See the `ffi` submodule + `.port/tools/web-canvas-glue.mjs`.
//!
//!   2. `font.Metrics` (hand-built) → `FaceMetrics` (modern pipeline). The
//!      reference `calcMetrics` builds a final `font.Metrics` directly. The
//!      modern `Collection`/`SharedGrid` pipeline instead reads `Face.getMetrics()
//!      -> FaceMetrics` (raw face metrics) and computes the final `Metrics` via
//!      the shared `Metrics.calc`. We surface the Canvas measurements as a
//!      `FaceMetrics` so the browser face flows through the same pipeline as every
//!      other backend (see `get_metrics`).
//!
//!   3. Extra modern-`Face`-surface methods the reference lacks (`synthetic_bold`
//!      /`synthetic_italic`/`name`/`set_variations`/`has_color`/`is_color_glyph`).
//!      `Collection`/`CodepointResolver` call these on the active `Face` on every
//!      backend; Canvas cannot synthesize styles or expose SFNT tables, so those
//!      are faithful `Unsupported`/derived stubs (documented at each site).
//!
//!   4. `atlas.format` RGBA arm -> RGBA->BGRA swizzle. The reference's
//!      `renderGlyph` stores the Canvas `getImageData` bytes UNCHANGED under an
//!      `.rgba` atlas format — a variant that does NOT exist in the pinned
//!      `font/Atlas.zig` enum (`grayscale`/`bgr`/`bgra`), i.e. that arm cannot
//!      compile against its own tree (the browser path is abandoned upstream).
//!      The modern enum's only 4-byte format is `bgra`, and the renderers upload
//!      `Format::Bgra` atlases as `BGRA`/`BGRA8Unorm_sRGB` textures, so storing
//!      Canvas RGBA bytes verbatim would render every color glyph channel-swapped
//!      (red -> blue). We therefore convert RGBA -> BGRA (`rgba_to_bgra`) so the
//!      bytes honor the format the `Atlas` declares. The grayscale (A8) arm is
//!      byte-identical to the reference.
//!
//!   5. `initNamed` validates that `raw` really is a CSS "font" value. The
//!      reference can only ever be handed a CSS string (its `Face` has NO
//!      bytes-based initializer, and `SharedGridSet` is never instantiated with
//!      the web_canvas face), so an invalid value is a browser-side no-op that it
//!      merely `log.warn`s about. The modern generic loader DOES call
//!      `Face::init(lib, source, opts)` with embedded TTF bytes, which the shim
//!      would otherwise hand to `ctx.font` as a binary "CSS" string: the browser
//!      rejects it and silently keeps the previous/fallback font. We reject
//!      non-CSS payloads up front with `Error::InvalidFontStr` (see
//!      `validate_css_font_str`) so the failure is loud rather than a silently
//!      wrong font, and keep the reference's post-set read-back check (see
//!      `context`) instead of discarding it.

use std::collections::HashMap;

use allocator_api2::alloc::Allocator;

use super::{DesiredSize, Options, Variation};
use crate::library::Library;
use crate::Presentation;
use rustty_font::atlas::{Atlas, Format};
use rustty_font::glyph::{Glyph, RenderOptions};
use rustty_font::metrics::FaceMetrics;

// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const js = @import("zig-js");
// const font = @import("../main.zig");
// const log = std.log.scoped(.font_face);
//
// The reference's `std.log.scoped(.font_face)` tag, carried through the `log`
// facade's `target:` (`.port/parity-closure/DECISION-logging.md` §6). Same scope
// as `face/coretext.rs` and `face/freetype.rs` — the reference's own choice.
const LOG_SCOPE: &str = "font_face";
use rustty_util::quirks::inlineAssert as assert;

/// The error set of the web_canvas `Face`.
// PORT NOTE: the reference threads an inferred error union (`Allocator.Error`,
// the JS-FFI errors, the `Atlas` errors, `error.UnsupportedAtlasFormat`,
// `error.UnknownGraphemeCluster`, `error.CanvasContainerNotFound`). Modeled as
// one enum with `From` conversions so `?` composes them; `AtlasFull` is named
// exactly (matched by `shared_grid.rs`'s `FaceError::AtlasFull`).
#[derive(Debug)]
pub enum Error {
    OutOfMemory,
    AtlasFull,
    UnsupportedAtlasFormat,
    UnknownGraphemeCluster,
    CanvasContainerNotFound,
    /// Codepoint outside the Unicode range (utf8 encode failure).
    CodepointTooLarge,
    /// The "raw" value handed to `init_named`/`init` is not a CSS "font" value
    /// (non-UTF-8, control bytes, or font-FILE bytes). See divergence 5.
    InvalidFontStr,
    /// Canvas cannot synthesize bold/italic or expose variations/SFNT names.
    Unsupported,
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

// ---------------------------------------------------------------------------
// The `extern "C"` Canvas FFI (replaces `zig-js`). The JS host implements these
// against a real (offscreen) Canvas; canvas/context objects are kept in an
// integer handle table on the JS side. All arguments are primitives or
// pointer/len buffers into wasm linear memory — no JS objects cross the boundary.
// ---------------------------------------------------------------------------
mod ffi {
    #[link(wasm_import_module = "env")]
    extern "C" {
        /// `document.createElement("canvas")` → integer handle.
        pub fn rustty_wc_canvas_new() -> u32;
        /// Drop a canvas handle.
        pub fn rustty_wc_canvas_free(canvas: u32);
        /// `canvas.getContext("2d")` → integer handle (stable per canvas).
        pub fn rustty_wc_canvas_get_context(canvas: u32) -> u32;
        /// Set `canvas.width`/`canvas.height` and the matching `style.width`/
        /// `style.height` (`"{n}px"`) in one call.
        pub fn rustty_wc_canvas_set_size(canvas: u32, width: u32, height: u32);

        /// `ctx.clearRect(0, 0, canvas.width, canvas.height)`.
        pub fn rustty_wc_context_clear(ctx: u32);
        /// `ctx.font = <utf8 string>`.
        pub fn rustty_wc_context_set_font(ctx: u32, ptr: *const u8, len: usize);
        /// Read `ctx.font` back into `out` (up to `cap`); returns the byte length.
        pub fn rustty_wc_context_get_font(ctx: u32, out: *mut u8, cap: usize) -> usize;
        /// `ctx.fillStyle = <utf8 string>`.
        pub fn rustty_wc_context_set_fill_style(ctx: u32, ptr: *const u8, len: usize);
        /// `ctx.textBaseline = <utf8 string>`.
        pub fn rustty_wc_context_set_text_baseline(ctx: u32, ptr: *const u8, len: usize);
        /// `ctx.fillRect(x, y, w, h)`.
        pub fn rustty_wc_context_fill_rect(ctx: u32, x: f64, y: f64, w: f64, h: f64);
        /// `ctx.fillText(<utf8 string>, x, y)`.
        pub fn rustty_wc_context_fill_text(ctx: u32, ptr: *const u8, len: usize, x: f64, y: f64);
        /// `ctx.measureText(<utf8 string>)`; writes 5 `f32` into `out`:
        /// `[width, actualBoundingBoxLeft, actualBoundingBoxRight,
        ///   actualBoundingBoxAscent, actualBoundingBoxDescent]`.
        pub fn rustty_wc_context_measure_text(ctx: u32, ptr: *const u8, len: usize, out: *mut f32);
        /// `ctx.getImageData(x, y, w, h).data`; copies the RGBA bytes into `out`
        /// (up to `cap`) and returns the true byte length (`w*h*4`).
        pub fn rustty_wc_context_get_image_data(
            ctx: u32,
            x: u32,
            y: u32,
            w: u32,
            h: u32,
            out: *mut u8,
            cap: usize,
        ) -> usize;
        /// `document.getElementById(id).append(canvas)`; 0 = ok, -1 = not found.
        pub fn rustty_wc_debug_append(canvas: u32, id_ptr: *const u8, id_len: usize) -> i32;
    }
}

/// Index into the `rustty_wc_context_measure_text` output buffer.
const M_WIDTH: usize = 0;
const M_BOX_LEFT: usize = 1;
const M_BOX_RIGHT: usize = 2;
const M_BOX_ASCENT: usize = 3;
const M_BOX_DESCENT: usize = 4;

/// Measure `text` on `ctx`, returning the 5 `TextMetrics` floats.
fn measure_text(ctx: u32, text: &[u8]) -> [f32; 5] {
    let mut out = [0.0f32; 5];
    unsafe {
        ffi::rustty_wc_context_measure_text(ctx, text.as_ptr(), text.len(), out.as_mut_ptr());
    }
    out
}

/// A JS-side canvas handle with RAII cleanup (`js.Object.deinit`).
struct Canvas {
    handle: u32,
}

impl Canvas {
    fn new() -> Canvas {
        Canvas {
            handle: unsafe { ffi::rustty_wc_canvas_new() },
        }
    }
    fn deinit(&mut self) {
        unsafe { ffi::rustty_wc_canvas_free(self.handle) };
    }
}

pub struct Face {
    // PORT NOTE: the reference holds `alloc: Allocator`; rustty uses the global
    // allocator everywhere, so no allocator handle is threaded on the struct.
    /// See `grapheme_to_glyph` for details.
    // const grapheme_start: u32 = 0x10FFFF + 1;
    // const grapheme_end: u32 = std.math.maxInt(u32);

    /// The CSS "font" attribute, excluding size.
    // font_str: []const u8,
    font_str: Vec<u8>,

    /// The size we currently have set.
    // size: font.face.DesiredSize,
    // PORT NOTE: `pub` to match the freetype/coretext backend faces — `Collection`
    // reads `face.size` when inheriting the size for synthetic styles.
    pub size: DesiredSize,

    /// The presentation for this font.
    // presentation: font.Presentation,
    presentation: Presentation,

    /// Face metrics computed from the Canvas measurements (see `calc_metrics`).
    // PORT NOTE (divergence 2): the reference stores a final `font.Metrics`; the
    // modern pipeline reads raw `FaceMetrics` via `get_metrics`, so we cache that.
    face_metrics: FaceMetrics,

    /// The canvas element that we will reuse to render glyphs.
    // canvas: js.Object,
    canvas: Canvas,

    /// The map to store multi-codepoint grapheme clusters that are rendered.
    /// We use 1 above the maximum unicode codepoint up to the max 32-bit
    /// unsigned integer to store the "glyph index" for graphemes.
    // grapheme_to_glyph: std.StringHashMapUnmanaged(u32) = .{},
    grapheme_to_glyph: HashMap<Vec<u8>, u32>,
    // glyph_to_grapheme: std.AutoHashMapUnmanaged(u32, []u8) = .{},
    glyph_to_grapheme: HashMap<u32, Vec<u8>>,
    // grapheme_next: u32 = grapheme_start,
    grapheme_next: u32,
}

/// We use 1 above the maximum unicode codepoint up to the max 32-bit unsigned
/// integer to store the "glyph index" for graphemes.
const GRAPHEME_START: u32 = 0x10FFFF + 1;

/// An internal (web-canvas-only) format for rendered glyphs since we do render
/// passes in multiple different situations.
// const RenderedGlyph = struct { ... };
struct RenderedGlyph {
    width: u32,
    height: u32,
    bitmap: Vec<u8>,
}

impl Face {
    /// Initialize a web canvas font with a "raw" value. The "raw" value can
    /// be any valid value for a CSS "font" property EXCLUDING the size. The
    /// size is always added via the `size` parameter.
    ///
    /// The raw value is copied so the caller can free it after it is gone.
    ///
    /// The presentation is given here directly because the browser gives
    /// us no easy way to determine the presentation we want for this font.
    /// Callers should just tell us what to expect.
    // pub fn initNamed(alloc, raw: []const u8, size, presentation) !Face {
    pub fn init_named(
        raw: &[u8],
        size: DesiredSize,
        presentation: Presentation,
    ) -> Result<Face, Error> {
        // PORT NOTE (divergence 5): the reference has no bytes-based initializer,
        // so `raw` is a CSS string by construction there. Here the generic
        // `Face::init` shim can be handed embedded font FILE bytes, which
        // `ctx.font` would reject while leaving the previous/fallback font in
        // place — a silently wrong font. Fail loudly instead.
        validate_css_font_str(raw)?;

        // Copy our font string because we're going to have to reuse it.
        // const font_str = try alloc.dupe(u8, raw);
        let font_str = raw.to_vec();

        // Create our canvas that we're going to continue to reuse.
        // const doc = try js.global.get(js.Object, "document");
        // const canvas = try doc.call(js.Object, "createElement", .{js.string("canvas")});
        let canvas = Canvas::new();

        let mut result = Face {
            font_str,
            size,
            presentation,
            canvas,
            // We're going to calculate these right after initialization.
            // .metrics = undefined,
            face_metrics: zero_face_metrics(),
            grapheme_to_glyph: HashMap::new(),
            glyph_to_grapheme: HashMap::new(),
            grapheme_next: GRAPHEME_START,
        };
        result.calc_metrics()?;

        // log.debug("face initialized: {s}", .{raw});
        //
        // PORT NOTE: `{s}` renders the raw CSS font-string bytes; the lossy
        // UTF-8 conversion is confined to the diagnostic.
        log::debug!(
            target: LOG_SCOPE,
            "face initialized: {}",
            String::from_utf8_lossy(raw)
        );
        Ok(result)
    }

    /// Modern generic loader (`Collection`/`SharedGridSet` call `Face::init(lib,
    /// source, opts)`). PORT NOTE (divergence): the reference web_canvas face has
    /// no bytes-based init — Canvas fonts are named CSS strings, not file bytes.
    /// The browser face therefore interprets `source` as the CSS "font" string
    /// (excluding size) and defaults the presentation to `.text`; `opts.size`
    /// carries the size. This keeps the modern loader call-site type-checking.
    ///
    /// PORT NOTE (divergence 5): `source` MUST therefore be a CSS "font" value.
    /// A caller that hands over real font FILE bytes (e.g.
    /// `rustty_font::embedded::VARIABLE`) gets `Error::InvalidFontStr` — the
    /// browser would otherwise reject the binary `ctx.font` assignment and keep
    /// its previous/fallback font, silently rendering a different font than the
    /// caller asked for. Wasm callers of the shared-grid pipeline pass a CSS
    /// family instead (see `shared_grid_set::embedded_face_source`).
    pub fn init(_lib: Library, source: &[u8], opts: Options) -> Result<Face, Error> {
        Face::init_named(source, opts.size, Presentation::Text)
    }

    // pub fn deinit(self: *Face) void {
    pub fn deinit(&mut self) {
        // self.alloc.free(self.font_str);
        // self.grapheme_to_glyph.deinit(self.alloc);
        // { var it = self.glyph_to_grapheme.valueIterator(); while … free; }
        // self.glyph_to_grapheme.deinit(self.alloc);
        self.grapheme_to_glyph.clear();
        self.glyph_to_grapheme.clear();
        // self.canvas.deinit();
        self.canvas.deinit();
        // self.* = undefined;
    }

    /// Resize the font in-place. If this succeeds, the caller is responsible
    /// for clearing any glyph caches, font atlas data, etc.
    // pub fn setSize(self: *Face, size: font.face.DesiredSize) !void {
    // PORT NOTE: modern callers pass `Options`; we take the `size` from it.
    pub fn set_size(&mut self, opts: Options) -> Result<(), Error> {
        // const old = self.size; self.size = size; errdefer self.size = old;
        let old = self.size;
        self.size = opts.size;
        match self.calc_metrics() {
            Ok(()) => Ok(()),
            Err(e) => {
                self.size = old;
                Err(e)
            }
        }
    }

    /// Returns the glyph index for the given Unicode code point. For canvas,
    /// we support every glyph and the ID is just the codepoint since we don't
    /// have access to the underlying tables anyways. We let the browser deal
    /// with bad codepoints.
    // pub fn glyphIndex(self: Face, cp: u32) ?u32 {
    pub fn glyph_index(&self, cp: u32) -> Option<u32> {
        // If this is a multi-codepoint grapheme then we only check if
        // we actually know about it.
        if cp >= GRAPHEME_START {
            // if (!self.glyph_to_grapheme.contains(cp)) return null;
            if !self.glyph_to_grapheme.contains_key(&cp) {
                return None;
            }
        }

        // Render the glyph to determine if it is colored or not. We have to do
        // this because the browser will always try to render whatever we give it
        // and we have no API to determine color.
        //
        // We don't want to say yes to the wrong presentation because it will go
        // into the wrong Atlas.
        // const p: font.Presentation = if (cp <= 255) .text else …;
        let p: Presentation = if cp <= 255 {
            Presentation::Text
        } else {
            match self.glyph_presentation(cp) {
                Ok(p) => p,
                // In this case, we assume we are unable to render this glyph and
                // therefore just say we don't support it.
                Err(_) => return None,
            }
        };
        if p != self.presentation {
            return None;
        }

        Some(cp)
    }

    /// This determines the presentation of the glyph by literally inspecting the
    /// image data to look for any color. This isn't super performant but we
    /// don't have a better choice given the canvas APIs.
    // fn glyphPresentation(self: Face, cp: u32) !font.Presentation {
    fn glyph_presentation(&self, cp: u32) -> Result<Presentation, Error> {
        // Render the glyph
        let render = self.render_glyph_internal(cp)?;

        // Inspect the image data for any non-zeros in the RGB value.
        // NOTE(perf): this is an easy candidate for SIMD.
        let mut i: usize = 0;
        while i < render.bitmap.len() {
            if render.bitmap[i] > 0 || render.bitmap[i + 1] > 0 || render.bitmap[i + 2] > 0 {
                return Ok(Presentation::Emoji);
            }
            i += 4;
        }

        Ok(Presentation::Text)
    }

    /// Returns the glyph index for the given grapheme cluster. The same cluster
    /// will always map to the same glyph index. This does not render the grapheme
    /// at this time, only reserves the index.
    // pub fn graphemeGlyphIndex(self: *Face, cluster: []const u8) error{OutOfMemory}!u32 {
    pub fn grapheme_glyph_index(&mut self, cluster: &[u8]) -> Result<u32, Error> {
        // If we already have this stored then return it.
        // const gop = try self.grapheme_to_glyph.getOrPut(self.alloc, cluster);
        // if (gop.found_existing) return gop.value_ptr.*;
        if let Some(&existing) = self.grapheme_to_glyph.get(cluster) {
            return Ok(existing);
        }

        // We don't have it stored. Ensure we have space to store. The next will
        // be "0" if we're out of space due to unsigned int wrapping.
        // if (self.grapheme_next == 0) return error.OutOfMemory;
        if self.grapheme_next == 0 {
            return Err(Error::OutOfMemory);
        }

        // Copy the cluster for our reverse mapping.
        // const copy = try self.alloc.dupe(u8, cluster);
        let copy = cluster.to_vec();

        // Store it (forward + reverse mapping).
        let idx = self.grapheme_next;
        self.grapheme_to_glyph.insert(cluster.to_vec(), idx);
        self.glyph_to_grapheme.insert(idx, copy);

        // self.grapheme_next +%= 1;
        self.grapheme_next = self.grapheme_next.wrapping_add(1);
        Ok(idx)
    }

    /// Render a glyph using the glyph index. The rendered glyph is stored in the
    /// given texture atlas.
    // pub fn renderGlyph(self, alloc, atlas: *font.Atlas, glyph_index, opts) !font.Glyph {
    pub fn render_glyph<A: Allocator + Copy>(
        &self,
        alloc: A,
        atlas: &mut Atlas<A>,
        glyph_index: u32,
        _opts: RenderOptions,
    ) -> Result<Glyph, Error> {
        // _ = opts;
        let render = self.render_glyph_internal(glyph_index)?;

        // Convert the format of the bitmap if necessary.
        // const bitmap_formatted: []u8 = switch (atlas.format) { … };
        let bitmap_formatted: Vec<u8> = match atlas.format() {
            // Bitmap is already in RGBA (canvas ImageData order) — but the atlas
            // says BGRA, and BGRA is what the renderers upload it as, so swap the
            // R and B channels. The reference stores the bitmap directly under an
            // `.rgba` atlas format that does NOT exist in `font/Atlas.zig`; see
            // faithful divergence 4 at the file head.
            // .rgba => render.bitmap,
            Format::Bgra => rgba_to_bgra(&render.bitmap),

            // Convert down to A8 (take the alpha channel).
            // .grayscale => a8: { … bitmap_a8[i] = render.bitmap[(i*4)+3]; }
            Format::Grayscale => {
                assert(render.bitmap.len() % 4 == 0);
                let mut bitmap_a8 = vec![0u8; render.bitmap.len() / 4];
                let mut i: usize = 0;
                while i < bitmap_a8.len() {
                    bitmap_a8[i] = render.bitmap[(i * 4) + 3];
                    i += 1;
                }
                bitmap_a8
            }

            // else => return error.UnsupportedAtlasFormat,
            Format::Bgr => return Err(Error::UnsupportedAtlasFormat),
        };

        // Put it in our atlas.
        // const region = try atlas.reserve(alloc, render.width, render.height);
        let region = atlas.reserve(alloc, render.width, render.height)?;
        // if (region.width > 0 and region.height > 0) atlas.set(region, bitmap_formatted);
        if region.width > 0 && region.height > 0 {
            atlas.set(region, &bitmap_formatted);
        }

        Ok(Glyph {
            width: render.width,
            height: render.height,
            // TODO: this can't be right
            offset_x: 0,
            offset_y: 0,
            atlas_x: region.x,
            atlas_y: region.y,
        })
    }

    /// Calculate the metrics associated with a given face.
    // fn calcMetrics(self: *Face) !void {
    fn calc_metrics(&mut self) -> Result<(), Error> {
        let ctx = self.context();

        // Cell width is the width of our M text.
        // const cell_width: f32 = cell_width: { … };
        let m = measure_text(ctx, b"M");
        // We prefer the bounding box since it is tighter but certain text such as
        // emoji do not have a bounding box set so we use the full run width.
        let cell_width: f32 = if m[M_BOX_RIGHT] > 0.0 {
            m[M_BOX_RIGHT]
        } else {
            m[M_WIDTH]
        };

        // To get the cell height we render a high and low character and get the
        // total of the ascent and descent. This should equal our pixel height but
        // this is a more surefire way to get it.
        // const height_metrics = try ctx.call(js.Object, "measureText", .{js.string("M_")});
        let hm = measure_text(ctx, b"M_");
        let asc = hm[M_BOX_ASCENT];
        let desc = hm[M_BOX_DESCENT];
        // const cell_height = asc + desc; const cell_baseline = desc;
        let cell_height = asc + desc;

        // PORT NOTE (divergence 2): the reference builds a final `font.Metrics`
        // here (including a hand-picked `underline_position = cell_height - 1`).
        // The modern pipeline instead consumes raw `FaceMetrics` and derives the
        // final `Metrics` (underline/strikethrough/cap-height/…) via `Metrics.calc`,
        // so we surface the Canvas measurements as `FaceMetrics` and leave the
        // derived fields `None` for the shared pipeline to compute:
        //   * px_per_em = the pixel size we set on the context (`{points}px`),
        //   * cell_width = measured "M" box width,
        //   * ascent = "M_" bounding-box ascent (+Y up),
        //   * descent = -(bounding-box descent) (FaceMetrics descent is negative),
        //   * line_gap = 0 (Canvas has no leading metric).
        self.face_metrics = FaceMetrics {
            px_per_em: self.size.points as f64,
            cell_width: cell_width as f64,
            ascent: asc as f64,
            descent: -(desc as f64),
            line_gap: 0.0,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: None,
            ex_height: None,
            // The measured height of the printable ASCII bounding box.
            ascii_height: Some(cell_height as f64),
            ic_width: None,
        };

        // log.debug("metrics font={s} value={}", .{ self.font_str, self.metrics });
        //
        // PORT NOTE: the reference's `{}` on `self.metrics` is Zig's default
        // struct formatting; `{:?}` is the Rust analogue. `{s}` on `font_str`
        // renders raw bytes (lossy conversion confined to the diagnostic).
        //
        // PORT NOTE (divergence 2, already recorded on `calc_metrics`): the port
        // stores `FaceMetrics`, not the reference's derived `font.Metrics`, so
        // the VALUE rendered here is the port's own metrics struct.
        log::debug!(
            target: LOG_SCOPE,
            "metrics font={} value={:?}",
            String::from_utf8_lossy(&self.font_str),
            self.face_metrics
        );
        Ok(())
    }

    /// Face metrics for the modern `Collection`/`SharedGrid` pipeline.
    // PORT NOTE (divergence 2): see `calc_metrics`.
    pub fn get_metrics(&self) -> FaceMetrics {
        self.face_metrics
    }

    /// Returns the 2d context configured for drawing.
    // fn context(self: Face) !js.Object {
    fn context(&self) -> u32 {
        // This will return the same context on subsequent calls so it is
        // important to reset it.
        // const ctx = try self.canvas.call(js.Object, "getContext", .{js.string("2d")});
        let ctx = unsafe { ffi::rustty_wc_canvas_get_context(self.canvas.handle) };

        // Clear the canvas.
        // { const width = …; const height = …; try ctx.call(void, "clearRect", …); }
        unsafe { ffi::rustty_wc_context_clear(ctx) };

        // Set our context font.
        // const font_val = try std.fmt.allocPrint(alloc, "{d}px {s}", .{size.points, font_str});
        let mut font_val: Vec<u8> = Vec::new();
        push_int(&mut font_val, self.size.points as i64);
        font_val.extend_from_slice(b"px ");
        font_val.extend_from_slice(&self.font_str);
        // try ctx.set("font", js.string(font_val));
        unsafe { ffi::rustty_wc_context_set_font(ctx, font_val.as_ptr(), font_val.len()) };

        // If the font property didn't change, then the font set didn't work. We
        // do this check because it is very easy to put an invalid font in and
        // this at least makes it show up in the logs.
        // const check = try ctx.getAlloc(js.String, self.alloc, "font");
        // if (!std.mem.eql(u8, font_val, check)) log.warn("canvas font didn't set…");
        let mut check = [0u8; 256];
        let n = unsafe { ffi::rustty_wc_context_get_font(ctx, check.as_mut_ptr(), check.len()) };
        let got = check.get(..n).unwrap_or(&[]);
        if font_val != got {
            font_did_not_set(&font_val, got);
        }

        ctx
    }

    /// Shared logic for rendering a glyph.
    // fn renderGlyphInternal(self: Face, alloc, glyph_index: u32) !RenderedGlyph {
    fn render_glyph_internal(&self, glyph_index: u32) -> Result<RenderedGlyph, Error> {
        // Encode our glyph to UTF-8 so we can build a JS string out of it.
        // var utf8: [4]u8 = undefined; const glyph_str = glyph_str: { … };
        let mut utf8_buf = [0u8; 4];
        let glyph_str: &[u8] = if glyph_index < GRAPHEME_START {
            // If we are a normal glyph then we are a single codepoint and we just
            // UTF8 encode it as-is.
            let ch = char::from_u32(glyph_index).ok_or(Error::CodepointTooLarge)?;
            ch.encode_utf8(&mut utf8_buf).as_bytes()
        } else {
            // We are a multi-codepoint glyph so we have to read the glyph from the
            // map and it is already utf8 encoded.
            // const slice = self.glyph_to_grapheme.get(glyph_index) orelse
            //     return error.UnknownGraphemeCluster;
            self.glyph_to_grapheme
                .get(&glyph_index)
                .ok_or(Error::UnknownGraphemeCluster)?
                .as_slice()
        };

        // Get our drawing context.
        let measure_ctx = self.context();

        // Get the width and height of the render.
        // const metrics = try measure_ctx.call(js.Object, "measureText", .{glyph_str});
        let metrics = measure_text(measure_ctx, glyph_str);
        // We prefer the bounding box since it is tighter but certain text such as
        // emoji do not have a bounding box set so we use the full run width.
        let width_f: f32 = if metrics[M_BOX_RIGHT] > 0.0 {
            metrics[M_BOX_RIGHT]
        } else {
            metrics[M_WIDTH]
        };
        let width: u32 = (width_f.ceil() as u32) + 1;

        let left = metrics[M_BOX_LEFT];
        let asc = metrics[M_BOX_ASCENT];
        let desc = metrics[M_BOX_DESCENT];

        // On Firefox on Linux, the bounding box is broken in some cases for
        // ideographic glyphs (such as emoji). We detect this and behave
        // differently.
        let broken_bbox = asc + desc < 0.001;

        // Height is our ascender + descender for this char.
        let height: u32 = if !broken_bbox {
            ((asc + desc).ceil() as u32) + 1
        } else {
            width
        };

        // Note: width and height both get "+ 1" added to them above. This is
        // important so that there is a 1px border around the glyph to avoid any
        // clipping in the atlas.

        // Resize canvas to match the glyph size exactly (canvas.width/height +
        // the matching `style.width`/`style.height` "{n}px" strings).
        unsafe { ffi::rustty_wc_canvas_set_size(self.canvas.handle, width, height) };

        // Reload our context since we resized the canvas.
        let ctx = self.context();

        // For the broken bounding box case we render ideographic baselines so that
        // we just render the glyph fully in the box with no offsets.
        if broken_bbox {
            unsafe { ffi::rustty_wc_context_set_text_baseline(ctx, b"ideographic".as_ptr(), 11) };
        }

        // Draw background.
        // try ctx.set("fillStyle", js.string("transparent"));
        unsafe { ffi::rustty_wc_context_set_fill_style(ctx, b"transparent".as_ptr(), 11) };
        // try ctx.call(void, "fillRect", .{ 0, 0, width, height });
        unsafe { ffi::rustty_wc_context_fill_rect(ctx, 0.0, 0.0, width as f64, height as f64) };

        // Draw glyph.
        // try ctx.set("fillStyle", js.string("black"));
        unsafe { ffi::rustty_wc_context_set_fill_style(ctx, b"black".as_ptr(), 5) };
        // try ctx.call(void, "fillText", .{ glyph_str, left + 1, if (!broken_bbox) asc + 1 else height });
        let y: f64 = if !broken_bbox {
            (asc + 1.0) as f64
        } else {
            height as f64
        };
        unsafe {
            ffi::rustty_wc_context_fill_text(
                ctx,
                glyph_str.as_ptr(),
                glyph_str.len(),
                (left + 1.0) as f64,
                y,
            )
        };

        // Read the image data and get it into a []u8 on our side.
        // const data = try ctx.call(js.Object, "getImageData", .{ 0, 0, width, height });
        // const src_array = try data.get(js.Object, "data"); // Uint8ClampedArray
        // const len = try src_array.get(u32, "length");
        // … copy src_array into our wasm memory via a Uint8Array view …
        let len = (width as usize) * (height as usize) * 4;
        let mut bitmap = vec![0u8; len];
        let copied = unsafe {
            ffi::rustty_wc_context_get_image_data(
                ctx,
                0,
                0,
                width,
                height,
                bitmap.as_mut_ptr(),
                bitmap.len(),
            )
        };
        // Defensive: the JS side reports the true length; trust our computed size.
        assert(copied == len);

        Ok(RenderedGlyph {
            width,
            height,
            bitmap,
        })
    }

    // -----------------------------------------------------------------------
    // Modern `Face` surface the reference web_canvas lacks (divergence 3). These
    // are called by `Collection`/`CodepointResolver` on every backend.
    // -----------------------------------------------------------------------

    /// Whether this face has color glyphs (emoji presentation). PORT NOTE
    /// (divergence 3): Canvas exposes no color-table API; a web_canvas face is
    /// either a text face or an emoji face by construction (`presentation`).
    pub fn has_color(&self) -> bool {
        self.presentation == Presentation::Emoji
    }

    /// Whether the given glyph renders in color. PORT NOTE: reuses the reference
    /// `glyphPresentation` image-inspection (emoji ⇒ color); on any error we
    /// conservatively report `false`.
    pub fn is_color_glyph(&self, glyph_id: u32) -> bool {
        matches!(self.glyph_presentation(glyph_id), Ok(Presentation::Emoji))
    }

    /// Synthesized bold. PORT NOTE (divergence 3): Canvas cannot synthesize a
    /// bold outline from a face; unsupported.
    pub fn synthetic_bold(&self, _opts: Options) -> Result<Face, Error> {
        Err(Error::Unsupported)
    }

    /// Synthesized italic. PORT NOTE (divergence 3): unsupported on Canvas.
    pub fn synthetic_italic(&self, _opts: Options) -> Result<Face, Error> {
        Err(Error::Unsupported)
    }

    /// Copy the font name into `buf`. PORT NOTE (divergence 3): Canvas has no SFNT
    /// name table; the CSS "font" string is the closest analog, so we return it.
    pub fn name<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], Error> {
        let n = self.font_str.len().min(buf.len());
        buf[..n].copy_from_slice(&self.font_str[..n]);
        Ok(&buf[..n])
    }

    /// Set font variations. PORT NOTE (divergence 3): Canvas has no
    /// `font-variation-settings` API reachable from the 2d context; noop.
    pub fn set_variations(&mut self, _vs: &[Variation], _opts: Options) -> Result<(), Error> {
        Ok(())
    }
}

/// An all-zero `FaceMetrics` placeholder used before `calc_metrics` runs (the
/// reference leaves `.metrics = undefined` and fills it immediately after).
fn zero_face_metrics() -> FaceMetrics {
    FaceMetrics {
        px_per_em: 0.0,
        cell_width: 0.0,
        ascent: 0.0,
        descent: 0.0,
        line_gap: 0.0,
        underline_position: None,
        underline_thickness: None,
        strikethrough_position: None,
        strikethrough_thickness: None,
        cap_height: None,
        ex_height: None,
        ascii_height: None,
        ic_width: None,
    }
}

/// Convert a Canvas `getImageData` RGBA buffer into the BGRA byte order the
/// `Atlas` `Format::Bgra` declares (swap byte 0 and byte 2 of every pixel; green
/// and alpha are untouched).
///
/// PORT NOTE (faithful divergence 4): the reference has no such conversion — it
/// stores the RGBA bytes under an `.rgba` atlas format that its own `Atlas` enum
/// does not define. `Format::Bgra` is the modern enum's only 4-byte format and is
/// uploaded as a BGRA texture by both the OpenGL and Metal renderers, so the
/// bytes must be swizzled to match the declared format.
fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    // `getImageData` always yields whole RGBA pixels, so the remainder is empty.
    for px in out.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    out
}

/// The font-FILE magics a CSS "font" value can never begin with: sfnt
/// (`0x00010000`), TrueType (`true`), TrueType collection (`ttcf`), CFF/OpenType
/// (`OTTO`), and WOFF/WOFF2 (`wOFF`/`wOF2`).
const FONT_FILE_MAGICS: [&[u8]; 6] = [
    &[0x00, 0x01, 0x00, 0x00],
    b"true",
    b"ttcf",
    b"OTTO",
    b"wOFF",
    b"wOF2",
];

/// Validate that `raw` is plausibly a CSS "font" value (see faithful divergence
/// 5). Rejects font FILE bytes, invalid UTF-8, and control characters — none of
/// which the browser can accept, and all of which `ctx.font` would silently
/// ignore (keeping the previous/fallback font).
fn validate_css_font_str(raw: &[u8]) -> Result<(), Error> {
    // Font file bytes (the modern generic `Face::init` loader's payload).
    for magic in FONT_FILE_MAGICS {
        if raw.starts_with(magic) {
            return Err(Error::InvalidFontStr);
        }
    }

    // A CSS value is text: it must be valid UTF-8 with no control characters.
    let s = core::str::from_utf8(raw).map_err(|_| Error::InvalidFontStr)?;
    if s.chars().any(|c| c.is_control()) {
        return Err(Error::InvalidFontStr);
    }

    Ok(())
}

/// The reference's `log.warn("canvas font didn't set, fonts may be broken,
/// expected={s} got={s}", …)` (`web_canvas.zig:316-321`).
///
/// PORT NOTE (task-14c-4): this used to reach `rustty-os`'s freestanding wasm
/// sink DIRECTLY, and only under the `ghostty-wasm-abi` feature — so in the
/// standalone `rustty_web_canvas.wasm` deliverable the message was dropped. It
/// now goes through the `log` facade like every other restored site in this
/// crate, and `rustty_wc_*`'s entry points install
/// `rustty_os::wasm::log::WasmLogger` (see `install_wasm_log_backend` below), so
/// BOTH wasm artifacts now carry it. `{s}` renders raw bytes; the lossy UTF-8
/// conversion is confined to the diagnostic.
#[cold]
#[inline(never)]
fn font_did_not_set(expected: &[u8], got: &[u8]) {
    log::warn!(
        target: LOG_SCOPE,
        "canvas font didn't set, fonts may be broken, expected={} got={}",
        String::from_utf8_lossy(expected),
        String::from_utf8_lossy(got)
    );
}

/// Append the base-10 ASCII of a non-negative integer to `out` (the `{d}` in the
/// reference `std.fmt.allocPrint`).
fn push_int(out: &mut Vec<u8>, mut v: i64) {
    if v < 0 {
        out.push(b'-');
        v = -v;
    }
    if v == 0 {
        out.push(b'0');
        return;
    }
    let start = out.len();
    while v > 0 {
        out.push(b'0' + (v % 10) as u8);
        v /= 10;
    }
    out[start..].reverse();
}

// ---------------------------------------------------------------------------
// The wasm-compatible C-ABI (`pub const Wasm = struct { … }`). Exported as
// `rustty_wc_face_*` `#[no_mangle] extern "C"` so a JS host can create a face and
// render glyphs against a real (offscreen) Canvas.
// ---------------------------------------------------------------------------

// The artifact-wide wasm log-backend installer lives at the CRATE ROOT
// (`crate::install_wasm_log_backend`) rather than here, so that
// `rustty-web-canvas` — which owns twelve more exports in the SAME
// `rustty_web_canvas.wasm` artifact — can call it on every target. See its doc
// comment for why the registration belongs at each entry point.
use crate::install_wasm_log_backend;

/// `export fn face_new(ptr, len, pts, p) ?*Face`
#[no_mangle]
pub extern "C" fn rustty_wc_face_new(
    ptr: *const u8,
    len: usize,
    pts: f32,
    presentation: u16,
) -> *mut Face {
    install_wasm_log_backend();
    // fn face_new_(...) !*Face { … } catch null
    let raw = unsafe { core::slice::from_raw_parts(ptr, len) };
    let pres = if presentation == 0 {
        Presentation::Text
    } else {
        Presentation::Emoji
    };
    let size = DesiredSize {
        points: pts,
        xdpi: super::DEFAULT_DPI,
        ydpi: super::DEFAULT_DPI,
    };
    match Face::init_named(raw, size, pres) {
        Ok(face) => Box::into_raw(Box::new(face)),
        Err(_) => core::ptr::null_mut(),
    }
}

/// `export fn face_free(ptr: ?*Face) void`
#[no_mangle]
pub extern "C" fn rustty_wc_face_free(ptr: *mut Face) {
    install_wasm_log_backend();
    if !ptr.is_null() {
        let mut face = unsafe { Box::from_raw(ptr) };
        face.deinit();
    }
}

/// `export fn face_render_glyph(face: *Face, atlas: *font.Atlas, codepoint: u32) ?*font.Glyph`
///
/// Renders `codepoint` into `atlas` and writes the resulting `Glyph` into
/// `out_glyph` (caller-allocated). Returns 0 on success, -1 on error. PORT NOTE:
/// the reference returns a heap `*Glyph` (`wasm.toHostOwned`); we write into a
/// caller buffer to avoid a bespoke free, matching the vt-c C-ABI convention.
#[no_mangle]
pub extern "C" fn rustty_wc_face_render_glyph(
    face: *mut Face,
    atlas: *mut Atlas,
    codepoint: u32,
    out_glyph: *mut Glyph,
) -> i32 {
    install_wasm_log_backend();
    if face.is_null() || atlas.is_null() || out_glyph.is_null() {
        return -1;
    }
    let face = unsafe { &*face };
    let atlas = unsafe { &mut *atlas };
    match face.render_glyph(
        allocator_api2::alloc::Global,
        atlas,
        codepoint,
        RenderOptions::default(),
    ) {
        Ok(glyph) => {
            unsafe { *out_glyph = glyph };
            0
        }
        // return face_render_glyph_(face, atlas, codepoint) catch |err| {
        //     log.warn("error rendering glyph err={}", .{err});
        //     return null;
        // };
        Err(err) => {
            log::warn!(target: LOG_SCOPE, "error rendering glyph err={err:?}");
            -1
        }
    }
}

/// `export fn face_debug_canvas(face: *Face) void` — append the face's canvas to
/// the DOM element with id `face-canvas` (`id_ptr`/`id_len` name the element).
#[no_mangle]
pub extern "C" fn rustty_wc_face_debug_canvas(
    face: *mut Face,
    id_ptr: *const u8,
    id_len: usize,
) -> i32 {
    install_wasm_log_backend();
    if face.is_null() {
        return -1;
    }
    let face = unsafe { &*face };
    // face_debug_canvas_(face) catch |err| {
    //     log.warn("error adding debug canvas err={}", .{err});
    // };
    //
    // PORT NOTE: the JS glue owns the `getElementById` + `append`, and returns
    // -1 for the reference's `error.CanvasContainerNotFound` — a REAL failure
    // arm, so this is not a diagnostic on a success path.
    let rc = unsafe { ffi::rustty_wc_debug_append(face.canvas.handle, id_ptr, id_len) };
    if rc != 0 {
        log::warn!(
            target: LOG_SCOPE,
            "error adding debug canvas err=error.CanvasContainerNotFound"
        );
    }
    rc
}

// ---------------------------------------------------------------------------
// The Ghostty-ABI wasm-compatible C-ABI (`pub const Wasm = struct { … }`,
// `web_canvas.zig:489-557`) — the UNPREFIXED export names retained by
// `font/main.zig`'s `comptime { _ = face.web_canvas.Wasm; }`. Feature-gated
// (`ghostty-wasm-abi`): only the `rustty_wasm.wasm` (`main_wasm.zig`) artifact
// carries these names; `rustty_web_canvas.wasm` keeps the `rustty_wc_*` surface
// above.
// ---------------------------------------------------------------------------

#[cfg(feature = "ghostty-wasm-abi")]
pub use ghostty_wasm_abi::{face_debug_canvas, face_free, face_new, face_render_glyph};

/// The wasm-compatible API.
// pub const Wasm = struct {
//     const wasm = @import("../../os/wasm.zig");
//     const alloc = wasm.alloc;
#[cfg(feature = "ghostty-wasm-abi")]
mod ghostty_wasm_abi {
    use super::{ffi, Face};
    use crate::face::{DesiredSize, DEFAULT_DPI};
    use crate::Presentation;
    use rustty_font::atlas::Atlas;
    use rustty_font::glyph::{Glyph, RenderOptions};

    /// `export fn face_new(ptr: [*]const u8, len: usize, pts: u16, p: u16) ?*Face`
    // PORT NOTE (`pts: u16`): the reference EXPORT takes the size as a `u16`
    // (points) and hands it to `initNamed`'s `f32` `DesiredSize.points`; the
    // widening is written out. `p` maps via the same total `@enumFromInt` map as
    // `rustty_wc_face_new` (0 => .text, else .emoji).
    #[no_mangle]
    pub extern "C" fn face_new(ptr: *const u8, len: usize, pts: u16, p: u16) -> *mut Face {
        // return face_new_(ptr, len, pts, p) catch null;
        let raw = unsafe { core::slice::from_raw_parts(ptr, len) };
        let pres = if p == 0 {
            Presentation::Text
        } else {
            Presentation::Emoji
        };
        // var face = try Face.initNamed(alloc, ptr[0..len], .{ .points = pts },
        //     @enumFromInt(presentation));
        let size = DesiredSize {
            points: f32::from(pts),
            xdpi: DEFAULT_DPI,
            ydpi: DEFAULT_DPI,
        };
        match Face::init_named(raw, size, pres) {
            // const result = try alloc.create(Face); result.* = face; return result;
            Ok(face) => Box::into_raw(Box::new(face)),
            // catch null
            Err(_) => core::ptr::null_mut(),
        }
    }

    /// `export fn face_free(ptr: ?*Face) void`
    #[no_mangle]
    pub extern "C" fn face_free(ptr: *mut Face) {
        // if (ptr) |v| { v.deinit(); alloc.destroy(v); }
        if !ptr.is_null() {
            let mut face = unsafe { Box::from_raw(ptr) };
            face.deinit();
        }
    }

    /// Resulting pointer must be freed using the global "free".
    // export fn face_render_glyph(face: *Face, atlas: *font.Atlas, codepoint: u32) ?*font.Glyph
    #[no_mangle]
    pub extern "C" fn face_render_glyph(
        face: *mut Face,
        atlas: *mut Atlas,
        codepoint: u32,
    ) -> *mut Glyph {
        // return face_render_glyph_(face, atlas, codepoint) catch |err| {
        //     log.warn("error rendering glyph err={}", .{err});   // log-as-comment
        //     return null;
        // };
        if face.is_null() || atlas.is_null() {
            return core::ptr::null_mut();
        }
        let face = unsafe { &*face };
        let atlas = unsafe { &mut *atlas };

        // fn face_render_glyph_(face, atlas, codepoint) !*font.Glyph {
        // const glyph = try face.renderGlyph(alloc, atlas, codepoint, .{});
        let glyph = match face.render_glyph(
            allocator_api2::alloc::Global,
            atlas,
            codepoint,
            RenderOptions::default(),
        ) {
            Ok(glyph) => glyph,
            Err(err) => {
                log::warn!(target: super::LOG_SCOPE, "error rendering glyph err={err:?}");
                return core::ptr::null_mut();
            }
        };

        // const result = try alloc.create(font.Glyph);
        // errdefer alloc.destroy(result);
        // _ = try wasm.toHostOwned(result);
        // result.* = glyph;
        // return result;
        //
        // PORT NOTE: `toHostOwned` is the resurrected pre-`9dc2e5978`
        // `os/wasm.zig` tracker (rustty-os `wasm-host-owned`): the host frees the
        // returned `*Glyph` with the module's exported `free`.
        let result = Box::into_raw(Box::new(glyph));
        match unsafe { rustty_os::wasm::to_host_owned(result) } {
            Ok(_) => result,
            Err(_err) => {
                // errdefer alloc.destroy(result);
                drop(unsafe { Box::from_raw(result) });
                core::ptr::null_mut()
            }
        }
    }

    /// `export fn face_debug_canvas(face: *Face) void`
    // PORT NOTE: the reference body (`face_debug_canvas_`) does
    // `document.getElementById("face-canvas").append(face.canvas)` via zig-js;
    // the Canvas FFI's `rustty_wc_debug_append` is that exact operation (the JS
    // glue owns the `getElementById` + `append`; -1 is the reference's
    // `error.CanvasContainerNotFound`, log-as-comment).
    #[no_mangle]
    pub extern "C" fn face_debug_canvas(face: *mut Face) {
        if face.is_null() {
            return;
        }
        let face = unsafe { &*face };
        const ID: &[u8] = b"face-canvas";
        // face_debug_canvas_(face) catch |err| { log.warn("error adding debug canvas err={}", …); };
        let rc = unsafe { ffi::rustty_wc_debug_append(face.canvas.handle, ID.as_ptr(), ID.len()) };
        if rc != 0 {
            log::warn!(
                target: super::LOG_SCOPE,
                "error adding debug canvas err=error.CanvasContainerNotFound"
            );
        }
    }
}
