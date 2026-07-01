// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/surface.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): only the memory-backed `ImageSurface` variants (argb,
// xrgb, rgb, rgba, alpha8) are ported — the ghostty sprite/glyf rasterizers use
// `alpha8` (plus rgb/rgba scratch surfaces). The sub-8-bit `PackedImageSurface`
// (alpha4/2/1), and the compositor-backed `composite` / `composite_stride` /
// `get_stride` paths, are NOT part of this subset (they pull in the full SIMD
// compositor). Those methods are present but unimplemented, and calling them
// panics with a clear message; see the crate-level blockers doc.

//! Surface types represented by the library.

use crate::compositor::Operator;
use crate::pixel::{self, Alpha8, Argb, Format, Pixel, Rgb, Rgba, Xrgb};

pub const SUPERSAMPLE_SCALE: i32 = 4;

/// Errors associated with surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// An invalid height was passed to surface initialization.
    InvalidHeight,
    /// An invalid width was passed to surface initialization.
    InvalidWidth,
}

/// Interface tags for surface types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceType {
    ImageSurfaceArgb,
    ImageSurfaceXrgb,
    ImageSurfaceRgb,
    ImageSurfaceRgba,
    ImageSurfaceAlpha8,
    ImageSurfaceAlpha4,
    ImageSurfaceAlpha2,
    ImageSurfaceAlpha1,
}

/// The pixel-buffer element trait implemented by each concrete surface pixel
/// type. Faithful to the `RGBA_T` / `Alpha` `comptime` interface z2d generates.
pub trait SurfacePixel: Copy + PartialEq {
    const FORMAT: Format;
    fn from_pixel(p: Pixel) -> Self;
    fn as_pixel(self) -> Pixel;
    fn average(ps: &[Self]) -> Self;
    fn zero() -> Self;
}

impl SurfacePixel for Alpha8 {
    const FORMAT: Format = Format::Alpha8;
    fn from_pixel(p: Pixel) -> Self {
        Alpha8::from_pixel(p)
    }
    fn as_pixel(self) -> Pixel {
        Alpha8::as_pixel(self)
    }
    fn average(ps: &[Self]) -> Self {
        Alpha8::average(ps)
    }
    fn zero() -> Self {
        Alpha8 { a: 0 }
    }
}

macro_rules! impl_surface_pixel_rgb {
    ($t:ty, $fmt:expr, $avg_is_rgba:expr) => {
        impl SurfacePixel for $t {
            const FORMAT: Format = $fmt;
            fn from_pixel(p: Pixel) -> Self {
                <$t>::from_pixel(p)
            }
            fn as_pixel(self) -> Pixel {
                <$t>::as_pixel(self)
            }
            fn average(ps: &[Self]) -> Self {
                rgb_average::<$t>(ps, $avg_is_rgba)
            }
            fn zero() -> Self {
                <$t>::default()
            }
        }
    };
}

impl_surface_pixel_rgb!(Xrgb, Format::Xrgb, false);
impl_surface_pixel_rgb!(Rgb, Format::Rgb, false);
impl_surface_pixel_rgb!(Argb, Format::Argb, true);
impl_surface_pixel_rgb!(Rgba, Format::Rgba, true);

/// Shared RGB(A) average, mirroring `RGBA_T.average`. We route through `Pixel`
/// to reuse the field extraction for each concrete type.
fn rgb_average<T: SurfacePixel>(ps: &[T], is_rgba: bool) -> T {
    debug_assert!(ps.len() <= 256);
    if ps.is_empty() {
        return T::zero();
    }
    let (mut r, mut g, mut b, mut a): (u16, u16, u16, u16) = (0, 0, 0, 0);
    for p in ps {
        let (pr, pg, pb, pa) = match p.as_pixel() {
            Pixel::Xrgb(q) => (q.r as u16, q.g as u16, q.b as u16, 0),
            Pixel::Rgb(q) => (q.r as u16, q.g as u16, q.b as u16, 0),
            Pixel::Argb(q) => (q.r as u16, q.g as u16, q.b as u16, q.a as u16),
            Pixel::Rgba(q) => (q.r as u16, q.g as u16, q.b as u16, q.a as u16),
            _ => (0, 0, 0, 0),
        };
        r += pr;
        g += pg;
        b += pb;
        if is_rgba {
            a += pa;
        }
    }
    let n = ps.len() as u16;
    let px = if is_rgba {
        // Reconstruct via the RGBA-shaped pixel then narrow to T.
        Pixel::Rgba(Rgba {
            r: (r / n) as u8,
            g: (g / n) as u8,
            b: (b / n) as u8,
            a: (a / n) as u8,
        })
    } else {
        Pixel::Rgb(Rgb {
            r: (r / n) as u8,
            g: (g / n) as u8,
            b: (b / n) as u8,
        })
    };
    T::from_pixel(px)
}

/// A memory-backed image surface. The pixel format is the type parameter `T`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSurface<T: SurfacePixel> {
    /// The width of the surface.
    pub width: i32,
    /// The height of the surface.
    pub height: i32,
    /// The underlying buffer, initialized to `width` * `height`.
    pub buf: Vec<T>,
}

impl<T: SurfacePixel> ImageSurface<T> {
    /// Initializes a surface. If non-null, the surface is initialized with the
    /// supplied `initial_px`.
    pub fn init(width: i32, height: i32, initial_px: Option<T>) -> Result<ImageSurface<T>, Error> {
        if width < 1 {
            return Err(Error::InvalidWidth);
        }
        if height < 1 {
            return Err(Error::InvalidHeight);
        }
        let h_usize = height.max(0) as usize;
        let w_usize = width.max(0) as usize;
        let fill = initial_px.unwrap_or_else(T::zero);
        Ok(ImageSurface {
            width,
            height,
            buf: vec![fill; h_usize * w_usize],
        })
    }

    /// De-allocates the surface buffer (Rust: drops the backing `Vec`).
    pub fn deinit(&mut self) {
        self.buf = Vec::new();
    }

    /// Downsamples a buffer in place, using simple pixel averaging.
    pub fn downsample_buffer(&mut self) {
        if self.width < SUPERSAMPLE_SCALE || self.height < SUPERSAMPLE_SCALE {
            return;
        }
        let scale = SUPERSAMPLE_SCALE as usize;
        let height = (self.height / SUPERSAMPLE_SCALE).max(0) as usize;
        let width = (self.width / SUPERSAMPLE_SCALE).max(0) as usize;
        let width_orig_u = self.width.max(0) as usize;

        for y in 0..height {
            for x in 0..width {
                let mut pixels = vec![T::zero(); scale * scale];
                for i in 0..scale {
                    for j in 0..scale {
                        let idx = (y * scale + i) * width_orig_u + (x * scale + j);
                        pixels[i * scale + j] = self.buf[idx];
                    }
                }
                self.buf[y * width + x] = T::average(&pixels);
            }
        }
        self.height = height as i32;
        self.width = width as i32;
    }

    /// Resizes the buffer to the dimensions set within the surface.
    pub fn resize_buffer(&mut self) {
        let height = self.height.max(0) as usize;
        let width = self.width.max(0) as usize;
        if self.buf.len() == height * width {
            return;
        }
        self.buf.truncate(height * width);
        self.buf.shrink_to_fit();
    }

    /// Downsamples the image, then resizes the buffer. See `downsample_buffer`.
    pub fn downsample(&mut self) {
        self.downsample_buffer();
        self.resize_buffer();
    }

    /// Gets the pixel data at the co-ordinates specified. Returns `None` if the
    /// co-ordinates are out of range.
    pub fn get_pixel(&self, x: i32, y: i32) -> Option<Pixel> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        let idx = (self.width as isize * y as isize + x as isize).max(0) as usize;
        Some(self.buf[idx].as_pixel())
    }

    /// Puts a single pixel at the `x` and `y` co-ordinates. No-ops if the pixel
    /// is out of range.
    pub fn put_pixel(&mut self, x: i32, y: i32, px: Pixel) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let idx = (self.width as isize * y as isize + x as isize).max(0) as usize;
        self.buf[idx] = T::from_pixel(px);
    }

    /// Replaces the surface with the supplied pixel.
    pub fn paint_pixel(&mut self, px: Pixel) {
        let v = T::from_pixel(px);
        for e in self.buf.iter_mut() {
            *e = v;
        }
    }

    /// Copies a single pixel to the range starting at `(x, y)` and proceeding
    /// for `len`. Out-of-range start co-ordinates cause a no-op.
    pub fn paint_stride(&mut self, x: i32, y: i32, len: usize, px: Pixel) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let w_usize = self.width.max(0) as usize;
        let y_usize = y.max(0) as usize;
        let x_usize = x.max(0) as usize;
        let start = w_usize * y_usize + x_usize;
        let v = T::from_pixel(px);
        for e in self.buf[start..start + len].iter_mut() {
            *e = v;
        }
    }

    /// Composites a single pixel using the specified operation, with an optional
    /// opacity, to the range starting at `(x, y)` for `len`.
    //
    // PORT NOTE: faithful port of `ImageSurface(T).compositeStride`. `RGBA` is
    // the base intermediary pixel for the source; if opacity < 255 the source is
    // pre-multiplied via a `dst_in` against an `Alpha8` mask. Each destination
    // pixel is then composited with `operator` via the scalar integer
    // `run_pixel_t`. See `compositor.rs` for why routing through `Pixel` is
    // equivalent to the Zig `runPixelT(T, ...)`.
    pub fn composite_stride(
        &mut self,
        x: i32,
        y: i32,
        len: usize,
        px: Pixel,
        operator: Operator,
        opacity: u8,
    ) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let w_usize = self.width.max(0) as usize;
        let y_usize = y.max(0) as usize;
        let x_usize = x.max(0) as usize;
        let start = w_usize * y_usize + x_usize;

        // We use RGBA as our base intermediary pixel for the source.
        let mut src: Rgba = Rgba::from_pixel(px);
        if opacity < 255 {
            src = Rgba::from_pixel(crate::compositor::run_pixel_t(
                src.as_pixel(),
                Pixel::Alpha8(Alpha8 { a: opacity }),
                Operator::DstIn,
            ));
        }

        for idx in start..start + len {
            self.buf[idx] = T::from_pixel(crate::compositor::run_pixel_t(
                self.buf[idx].as_pixel(),
                src.as_pixel(),
                operator,
            ));
        }
    }
}

/// Represents an interface as an enum of the (memory-backed) pixel formats.
#[derive(Debug, Clone, PartialEq)]
pub enum Surface {
    ImageSurfaceArgb(ImageSurface<Argb>),
    ImageSurfaceXrgb(ImageSurface<Xrgb>),
    ImageSurfaceRgb(ImageSurface<Rgb>),
    ImageSurfaceRgba(ImageSurface<Rgba>),
    ImageSurfaceAlpha8(ImageSurface<Alpha8>),
}

/// Dispatches an expression over each concrete surface variant, binding `s` to
/// the inner `ImageSurface`.
macro_rules! dispatch {
    ($self:expr, $s:ident => $body:expr) => {
        match $self {
            Surface::ImageSurfaceArgb($s) => $body,
            Surface::ImageSurfaceXrgb($s) => $body,
            Surface::ImageSurfaceRgb($s) => $body,
            Surface::ImageSurfaceRgba($s) => $body,
            Surface::ImageSurfaceAlpha8($s) => $body,
        }
    };
}

impl Surface {
    /// Initializes a surface of the specific `surface_type`. The surface buffer
    /// is initialized with the zero value for the pixel type.
    pub fn init(surface_type: SurfaceType, width: i32, height: i32) -> Result<Surface, Error> {
        match surface_type {
            SurfaceType::ImageSurfaceArgb => Ok(Surface::ImageSurfaceArgb(ImageSurface::init(
                width, height, None,
            )?)),
            SurfaceType::ImageSurfaceXrgb => Ok(Surface::ImageSurfaceXrgb(ImageSurface::init(
                width, height, None,
            )?)),
            SurfaceType::ImageSurfaceRgb => Ok(Surface::ImageSurfaceRgb(ImageSurface::init(
                width, height, None,
            )?)),
            SurfaceType::ImageSurfaceRgba => Ok(Surface::ImageSurfaceRgba(ImageSurface::init(
                width, height, None,
            )?)),
            SurfaceType::ImageSurfaceAlpha8 => Ok(Surface::ImageSurfaceAlpha8(ImageSurface::init(
                width, height, None,
            )?)),
            SurfaceType::ImageSurfaceAlpha4
            | SurfaceType::ImageSurfaceAlpha2
            | SurfaceType::ImageSurfaceAlpha1 => {
                unimplemented!(
                    "PackedImageSurface (sub-8-bit alpha) is not part of the ghostty z2d subset"
                )
            }
        }
    }

    /// Initializes a surface with the buffer set to the supplied `initial_px`.
    /// The surface type is inferred from it.
    pub fn init_pixel(initial_px: Pixel, width: i32, height: i32) -> Result<Surface, Error> {
        match initial_px {
            Pixel::Argb(px) => Ok(Surface::ImageSurfaceArgb(ImageSurface::init(
                width,
                height,
                Some(px),
            )?)),
            Pixel::Xrgb(px) => Ok(Surface::ImageSurfaceXrgb(ImageSurface::init(
                width,
                height,
                Some(px),
            )?)),
            Pixel::Rgb(px) => Ok(Surface::ImageSurfaceRgb(ImageSurface::init(
                width,
                height,
                Some(px),
            )?)),
            Pixel::Rgba(px) => Ok(Surface::ImageSurfaceRgba(ImageSurface::init(
                width,
                height,
                Some(px),
            )?)),
            Pixel::Alpha8(px) => Ok(Surface::ImageSurfaceAlpha8(ImageSurface::init(
                width,
                height,
                Some(px),
            )?)),
            Pixel::Alpha4(_) | Pixel::Alpha2(_) | Pixel::Alpha1(_) => {
                unimplemented!(
                    "PackedImageSurface (sub-8-bit alpha) is not part of the ghostty z2d subset"
                )
            }
        }
    }

    /// Releases the underlying surface memory.
    pub fn deinit(&mut self) {
        dispatch!(self, s => s.deinit());
    }

    /// Downsamples the image, using simple pixel averaging.
    pub fn downsample(&mut self) {
        dispatch!(self, s => s.downsample());
    }

    /// Gets the width of the surface.
    pub fn get_width(&self) -> i32 {
        dispatch!(self, s => s.width)
    }

    /// Gets the height of the surface.
    pub fn get_height(&self) -> i32 {
        dispatch!(self, s => s.height)
    }

    /// Gets the pixel format of the surface.
    pub fn get_format(&self) -> Format {
        match self {
            Surface::ImageSurfaceArgb(_) => Format::Argb,
            Surface::ImageSurfaceXrgb(_) => Format::Xrgb,
            Surface::ImageSurfaceRgb(_) => Format::Rgb,
            Surface::ImageSurfaceRgba(_) => Format::Rgba,
            Surface::ImageSurfaceAlpha8(_) => Format::Alpha8,
        }
    }

    /// Gets the pixel data at the co-ordinates specified.
    pub fn get_pixel(&self, x: i32, y: i32) -> Option<Pixel> {
        dispatch!(self, s => s.get_pixel(x, y))
    }

    /// Puts a single pixel at the x and y co-ordinates.
    pub fn put_pixel(&mut self, x: i32, y: i32, px: Pixel) {
        dispatch!(self, s => s.put_pixel(x, y, px));
    }

    /// Replaces the surface with the supplied pixel.
    pub fn paint_pixel(&mut self, px: Pixel) {
        dispatch!(self, s => s.paint_pixel(px));
    }

    /// Copies a single pixel to the range starting at `(x, y)` for `len`.
    pub fn paint_stride(&mut self, x: i32, y: i32, len: usize, px: Pixel) {
        dispatch!(self, s => s.paint_stride(x, y, len, px));
    }

    /// Effectively clears the stride, equivalent to `paint_stride` using a
    /// zero-value RGBA pixel.
    pub fn clear_stride(&mut self, x: i32, y: i32, len: usize) {
        self.paint_stride(
            x,
            y,
            len,
            Pixel::Rgba(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            }),
        );
    }

    /// Composites a single pixel using the specified operation, with an
    /// optional opacity, to the range starting at `(x, y)` for `len`.
    //
    // PORT NOTE: dispatches to the concrete `ImageSurface(T).composite_stride`.
    // This is the partial-coverage write path used by the multisample fill
    // rasterizer for anti-aliased spans.
    pub fn composite_stride(
        &mut self,
        x: i32,
        y: i32,
        len: usize,
        px: Pixel,
        operator: Operator,
        opacity: u8,
    ) {
        dispatch!(self, s => s.composite_stride(x, y, len, px, operator, opacity));
    }

    /// Runs the single compositor operation described by `operator`.
    ///
    /// SUBSET: requires the full compositor; unimplemented.
    pub fn composite(&mut self, _src: &Surface, _operator: Operator, _dst_x: i32, _dst_y: i32) {
        unimplemented!("Surface::composite requires the full compositor (not in the z2d subset)")
    }

    /// Convenience accessor for the alpha8 backing buffer (immutable).
    pub fn alpha8_buf(&self) -> &[Alpha8] {
        match self {
            Surface::ImageSurfaceAlpha8(s) => &s.buf,
            _ => panic!("surface is not alpha8"),
        }
    }

    /// Convenience accessor for the alpha8 backing buffer (mutable).
    pub fn alpha8_buf_mut(&mut self) -> &mut [Alpha8] {
        match self {
            Surface::ImageSurfaceAlpha8(s) => &mut s.buf,
            _ => panic!("surface is not alpha8"),
        }
    }
}

// Silence "unused import" for `pixel` module alias in configs where only the
// re-exported concrete types are referenced.
const _: Option<pixel::Format> = None;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_surface_init_deinit() {
        let mut sfc = ImageSurface::<Rgba>::init(10, 20, None).unwrap();
        assert_eq!(20, sfc.height);
        assert_eq!(10, sfc.width);
        assert_eq!(200, sfc.buf.len());
        assert_eq!(
            vec![
                Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0
                };
                200
            ],
            sfc.buf
        );
        sfc.deinit();
    }

    #[test]
    fn image_surface_get_pixel() {
        let mut sfc = ImageSurface::<Rgba>::init(20, 10, None).unwrap();
        {
            // OK
            let x: i32 = 7;
            let y: i32 = 5;
            let rgba = Rgba {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
                a: 0xDD,
            };
            sfc.buf[(y * 20 + x) as usize] = rgba;
            assert_eq!(Some(Pixel::Rgba(rgba)), sfc.get_pixel(x, y));
        }
        {
            // Out of bounds
            assert_eq!(None, sfc.get_pixel(20, 9));
            assert_eq!(None, sfc.get_pixel(19, 10));
        }
    }

    #[test]
    fn image_surface_put_pixel() {
        let mut sfc = ImageSurface::<Rgba>::init(20, 10, None).unwrap();
        let rgba = Rgba {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
            a: 0xDD,
        };
        let pix_rgba = rgba.as_pixel();
        {
            // OK
            let x: i32 = 7;
            let y: i32 = 5;
            sfc.put_pixel(x, y, pix_rgba);
            assert_eq!(rgba, sfc.buf[(y * 20 + x) as usize]);
        }
        {
            // Error, out of bounds
            let orig = sfc.buf.clone();
            sfc.put_pixel(20, 9, pix_rgba);
            assert_eq!(orig, sfc.buf);
            sfc.put_pixel(19, 10, pix_rgba);
            assert_eq!(orig, sfc.buf);
        }
        {
            // Different pixel type (copySrc is used)
            let rgb = Rgb {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
            };
            let pix_rgb = rgb.as_pixel();
            sfc.put_pixel(1, 1, pix_rgb);
            assert_eq!(
                Rgba {
                    r: 0xAA,
                    g: 0xBB,
                    b: 0xCC,
                    a: 0xFF
                },
                sfc.buf[21]
            );
        }
    }
}
