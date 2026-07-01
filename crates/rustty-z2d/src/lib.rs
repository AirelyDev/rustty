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
//! ## Ported in this subset
//!
//! * [`transformation::Transformation`] — the affine matrix (full).
//! * [`path::Path`] + [`static_path::StaticPath`] — the path builders
//!   (move/line/curve/close; `arc` excluded).
//! * [`pixel`] — `Format`/`Pixel` with the RGB(A) family + `Alpha8` (full).
//! * [`surface::Surface`] — memory-backed `ImageSurface` (argb/xrgb/rgb/rgba/
//!   alpha8); putPixel/paintStride/clearStride/getPixel/init/initPixel.
//! * [`painter::fill`] — the FILL operation for the non-AA (`.none`) scanline
//!   rasterizer (the pixel-producing core), via [`internal::tess::fill_plotter`]
//!   + [`internal::tess::polygon`] + [`internal::raster::direct`].
//! * [`pattern`], [`options`], [`compositor`] (`Operator`/`Precision`), and a
//!   minimal [`context::Context`].
//!
//! ## NOT yet ported (blockers for full ghostty pixel-parity)
//!
//! * The anti-aliased fill rasterizers (`.default`/`.multisample_4x`/
//!   `.supersample_4x`) and the full SIMD `compositor` (`color`/`color_vector`).
//! * The entire STROKE tessellation pipeline (`stroke_plotter` + `Pen`/`Face`/
//!   `Slope`/`Spline`-stroke/`Dasher` + `hairline`).
//! * `PackedImageSurface` (sub-8-bit alpha), gradients, dithering, PNG export.
//!
//! Those code paths are marked `unimplemented!` where present. See the task
//! blockers report for detail.

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
