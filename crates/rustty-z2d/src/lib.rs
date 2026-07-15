// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//   Copyright © 2025 rustty contributors (Rust port)
//
// Ported to Rust for rustty from z2d 0.10.0 `src/z2d.zig` (R3 vendored dep).

//! rustty-z2d — a pure-Rust port of the `vancluever/z2d` 0.10.0 SUBSET that
//! ghostty's `font/sprite/canvas.zig` and `font/glyf_rasterize.zig` depend on.
//!
//! z2d is a pure-Zig 2D vector graphics library (MPL-2.0). Ghostty uses a
//! narrow slice of it — `Context` / `Path` / `StaticPath` / `Surface` /
//! `Transformation` / `painter.fill(+stroke)` plus the transitive pixel-
//! producing core (the path flattener + the polygon scanline rasterizer). This
//! crate ports that slice, faithfully to z2d's rasterization algorithm, so that
//! the ghostty sprite/glyf PNG known-answer tests (ported into `rustty-font`)
//! can pass pixel-for-pixel.
//!
//! PORT STATUS: done (the ghostty-exercised z2d subset is complete; arms
//! outside the subset — dashed strokes/polylines, supersample AA fill, the
//! out-of-subset surface formats — are guarded `unimplemented!()` by design;
//! marker added per VALIDATION_REPORT.md §3b).
//!
//! ## Ported in this subset
//!
//! * [`transformation::Transformation`] — the affine matrix (full).
//! * [`path::Path`] + [`static_path::StaticPath`] — the path builders
//!   (move/line/curve/close/arc, the last via [`internal::arc`]'s
//!   arc→bezier-spline decomposition).
//! * [`pixel`] — `Format`/`Pixel` with the RGB(A) family + `Alpha8` (full).
//! * [`surface::Surface`] — memory-backed `ImageSurface` (argb/xrgb/rgb/rgba/
//!   alpha8) AND the sub-8-bit little-endian bit-packed `PackedImageSurface`
//!   (alpha4/2/1); putPixel/paintStride/clearStride/getPixel/init/initPixel/
//!   downsample/compositeStride.
//! * [`painter::fill`] — the FILL operation for the non-AA (`.none`) scanline
//!   rasterizer ([`internal::raster::direct`]), the default anti-aliased
//!   (`.default`/`.multisample_4x`) rasterizer ([`internal::raster::multisample`]
//!   + [`internal::raster::sparse_coverage`]), AND the mask-and-downsample
//!   (`.supersample_4x`) rasterizer ([`internal::raster::supersample`]), via
//!   [`internal::tess::fill_plotter`] + [`internal::tess::polygon`].
//! * [`painter::stroke`] — the STROKE operation, including DASHED stroking
//!   ([`internal::tess::dashed_plotter`] + [`internal::tess::dasher`]), via
//!   [`internal::tess::stroke_plotter`] + [`internal::tess::pen`] /
//!   [`internal::tess::face`] / [`internal::tess::slope`] +
//!   `internal::tess::polygon::Contour`, fed into the `.none`/`.default`/
//!   `.supersample_4x` rasterizers (butt/round/square caps, miter/bevel/round
//!   joins). Plus the HAIRLINE (1px thin-line) fast-path —
//!   [`internal::tess::polyline_plotter`] (dashed and undashed) +
//!   [`internal::raster::hairline`] (Bresenham / Xiaolin-Wu).
//! * [`compositor`] — `Operator`/`Precision` plus the scalar integer single-pixel
//!   core (`run_pixel_t` + `IntegerOps`) used by `Surface::composite_stride` for
//!   anti-aliased partial-coverage spans.
//! * [`pattern`], [`options`], and the managed [`context::Context`] drawing API
//!   (`set_source`, `move_to`/`line_to`/`curve_to`/`arc`/`close_path`, `fill`,
//!   `stroke`), which delegates to the unmanaged `painter` core.
//!
//! ## NOT yet ported (blockers for full ghostty pixel-parity)
//!
//! * The SIMD-vectorized `StrideCompositor` / `SurfaceCompositor` /
//!   `RGBA16Vec` / `RGBAFloat` (`color`/`color_vector`) pipelines. Only the
//!   scalar integer single-pixel compositor is ported; the multi-step
//!   compositors are reproduced per-pixel through it (see
//!   [`internal::raster::supersample`] / [`internal::raster::shared`]), faithful
//!   for the opaque-pattern subset.
//! * The `get_stride`/`Stride` accessor path (feeds the SIMD `StrideCompositor`;
//!   unused by the rasterizers, which write via `paint_stride`/
//!   `composite_stride`).
//! * `Pattern` gradient/dither sources, PNG export.
//!
//! Those out-of-subset paths are the only remaining gaps; the anti-aliased
//! supersample rasterizer, dashed tessellation, and packed-alpha surfaces are
//! now fully ported.

pub mod compositor;
pub mod context;
pub mod internal;
pub mod options;
pub mod painter;
pub mod path;
pub mod pattern;
pub mod pixel;
pub mod static_path;
pub mod surface;
pub mod transformation;

// Re-exports mirroring the flat `z2d` namespace ghostty imports (`z2d.Surface`,
// `z2d.Path`, `z2d.Transformation`, `z2d.Context`, `z2d.StaticPath`, etc.).
pub use context::Context;
pub use path::Path;
pub use static_path::StaticPath;
pub use surface::Surface;
pub use transformation::Transformation;
