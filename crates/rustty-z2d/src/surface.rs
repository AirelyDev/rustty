// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/surface.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): the memory-backed `ImageSurface` variants (argb, xrgb,
// rgb, rgba, alpha8) and the sub-8-bit little-endian bit-packed
// `PackedImageSurface` (alpha4/2/1) are both ported — the ghostty sprite/glyf
// rasterizers use `alpha8` (plus rgb/rgba scratch surfaces), and the packed
// alpha formats round out the `Surface` union so the supersample rasterizer's
// alpha4/2/1 mask arm is a real, reachable type. The `get_stride`/`Stride` path
// is NOT part of this subset (it feeds the full SIMD `StrideCompositor` and is
// unused by the rasterizers, which write via `paint_stride`/`composite_stride`);
// the packed reference tests' `getStride` sub-blocks are correspondingly
// omitted. The compositor-backed `composite` / `composite_stride` paths ARE
// ported, but as scalar reproductions that route each pixel through the integer
// `run_pixel_t` core rather than the SIMD `SurfaceCompositor`/`StrideCompositor`
// (integer operators only); see the crate-level blockers doc.

//! Surface types represented by the library.

use crate::compositor::Operator;
use crate::pixel::{self, Alpha1, Alpha2, Alpha4, Alpha8, Argb, Format, Pixel, Rgb, Rgba, Xrgb};
use core::marker::PhantomData;

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

/// The sub-8-bit alpha pixel trait implemented by `Alpha4`/`Alpha2`/`Alpha1`,
/// which the `PackedImageSurface` generic is parameterized over. Faithful to the
/// `comptime T` interface z2d's `PackedImageSurface(T)` requires (`format`,
/// `Opaque`, `fromPixel`/`asPixel`, `average`, and the packed-integer bit
/// width). `value`/`from_value` expose the raw channel integer so the packed
/// `_get`/`_set` bit math can round-trip it (the Zig `@bitCast` to/from the
/// struct's backing integer).
pub trait PackedAlpha: Copy + PartialEq + Default {
    /// The packed channel bit width (`@bitSizeOf(T)`): 4, 2 or 1.
    const NUM_BITS: usize;
    /// The maximum (fully-opaque) channel value: `(1 << NUM_BITS) - 1`.
    const MAX_INT: u8;
    /// The format descriptor for this pixel format.
    const FORMAT: Format;
    /// Shorthand for a fully-opaque pixel in this format (`T.Opaque`).
    fn opaque() -> Self;
    /// The raw packed channel integer.
    fn value(self) -> u8;
    /// Reconstructs the pixel from a raw packed channel integer.
    fn from_value(v: u8) -> Self;
    /// Returns the pixel translated to this alpha format (`T.fromPixel`).
    fn from_pixel(p: Pixel) -> Self;
    /// Returns this pixel as an interface (`T.asPixel`).
    fn as_pixel(self) -> Pixel;
    /// Returns an average of the pixels in the supplied slice (`T.average`).
    fn average(ps: &[Self]) -> Self;
    /// The `SurfaceType` interface tag corresponding to this pixel format.
    fn surface_type() -> SurfaceType;
}

impl PackedAlpha for Alpha4 {
    const NUM_BITS: usize = Alpha4::NUM_BITS;
    const MAX_INT: u8 = Alpha4::MAX_INT;
    const FORMAT: Format = Alpha4::FORMAT;
    fn opaque() -> Self {
        Alpha4::OPAQUE
    }
    fn value(self) -> u8 {
        self.a
    }
    fn from_value(v: u8) -> Self {
        Alpha4 { a: v }
    }
    fn from_pixel(p: Pixel) -> Self {
        Alpha4::from_pixel(p)
    }
    fn as_pixel(self) -> Pixel {
        Alpha4::as_pixel(self)
    }
    fn average(ps: &[Self]) -> Self {
        Alpha4::average(ps)
    }
    fn surface_type() -> SurfaceType {
        SurfaceType::ImageSurfaceAlpha4
    }
}

impl PackedAlpha for Alpha2 {
    const NUM_BITS: usize = Alpha2::NUM_BITS;
    const MAX_INT: u8 = Alpha2::MAX_INT;
    const FORMAT: Format = Alpha2::FORMAT;
    fn opaque() -> Self {
        Alpha2::OPAQUE
    }
    fn value(self) -> u8 {
        self.a
    }
    fn from_value(v: u8) -> Self {
        Alpha2 { a: v }
    }
    fn from_pixel(p: Pixel) -> Self {
        Alpha2::from_pixel(p)
    }
    fn as_pixel(self) -> Pixel {
        Alpha2::as_pixel(self)
    }
    fn average(ps: &[Self]) -> Self {
        Alpha2::average(ps)
    }
    fn surface_type() -> SurfaceType {
        SurfaceType::ImageSurfaceAlpha2
    }
}

impl PackedAlpha for Alpha1 {
    const NUM_BITS: usize = Alpha1::NUM_BITS;
    const MAX_INT: u8 = Alpha1::MAX_INT;
    const FORMAT: Format = Alpha1::FORMAT;
    fn opaque() -> Self {
        Alpha1::OPAQUE
    }
    fn value(self) -> u8 {
        self.a
    }
    fn from_value(v: u8) -> Self {
        Alpha1 { a: v }
    }
    fn from_pixel(p: Pixel) -> Self {
        Alpha1::from_pixel(p)
    }
    fn as_pixel(self) -> Pixel {
        Alpha1::as_pixel(self)
    }
    fn average(ps: &[Self]) -> Self {
        Alpha1::average(ps)
    }
    fn surface_type() -> SurfaceType {
        SurfaceType::ImageSurfaceAlpha1
    }
}

/// A specialized surface for pixel types that are smaller than 8 bits in size,
/// packing the data into the buffer in little-endian fashion. Faithful port of
/// `surface.zig` `PackedImageSurface(comptime T)`.
///
/// ## Accessing pixel data
///
/// The buffer is stored as a `Vec<u8>` (the Zig `[]u8`) in little-endian
/// fashion, consistent with the layout of the packed pixel structs. As an
/// example, for an 8x2 Alpha1 surface, the first row is stored in `buf[0]` and
/// the second row in `buf[1]`, with the lowest column index for each in the LSB
/// (i.e., x=0,y=0 in mask=0x1 of `buf[0]`, and x=0,y=1 in mask=0x1 of `buf[1]`).
///
/// Because every supported bit width (4, 2, 1) divides 8, no packed pixel ever
/// straddles a byte boundary, so `readPackedInt`/`writePackedInt` reduce to a
/// single-byte shift-and-mask (still little-endian: index `i` lives at bit
/// offset `i * NUM_BITS`).
#[derive(Debug, Clone, PartialEq)]
pub struct PackedImageSurface<T: PackedAlpha> {
    /// The width of the surface.
    pub width: i32,
    /// The height of the surface.
    pub height: i32,
    /// The underlying buffer as a densely-packed `[]u8`. See the type
    /// documentation for details on how to interpret the buffer.
    pub buf: Vec<u8>,
    _marker: PhantomData<T>,
}

impl<T: PackedAlpha> PackedImageSurface<T> {
    /// Initializes a surface. If non-null, the surface is initialized with the
    /// supplied `initial_px`.
    pub fn init(
        width: i32,
        height: i32,
        initial_px: Option<T>,
    ) -> Result<PackedImageSurface<T>, Error> {
        if width < 1 {
            return Err(Error::InvalidWidth);
        }
        if height < 1 {
            return Err(Error::InvalidHeight);
        }
        let h_usize = height.max(0) as usize;
        let w_usize = width.max(0) as usize;
        // WS19 (audit row N11): `clippy::manual_div_ceil` wants `.div_ceil(8)`. Kept as
        // the reference's `(n + 7) / 8` ceil-div idiom verbatim (R1) — the two forms are
        // numerically identical for these non-negative `usize` operands, so the rewrite
        // buys nothing and costs fidelity on a byte-layout computation that must stay
        // trivially comparable to the reference.
        #[allow(clippy::manual_div_ceil)]
        let len = (h_usize * w_usize * T::NUM_BITS + 7) / 8;
        let mut buf = vec![0u8; len];
        if let Some(px) = initial_px {
            Self::paint_pixel_buf(&mut buf, px);
        }
        Ok(PackedImageSurface {
            width,
            height,
            buf,
            _marker: PhantomData,
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
                let mut pixels = vec![T::default(); scale * scale];
                for i in 0..scale {
                    for j in 0..scale {
                        let idx = (y * scale + i) * width_orig_u + (x * scale + j);
                        pixels[i * scale + j] = self._get(idx);
                    }
                }
                self._set(y * width + x, T::average(&pixels));
            }
        }
        self.height = height as i32;
        self.width = width as i32;
    }

    /// Resizes the buffer to the dimensions set within the surface.
    pub fn resize_buffer(&mut self) {
        let height = self.height.max(0) as usize;
        let width = self.width.max(0) as usize;
        // WS19 (audit row N11): see the `manual_div_ceil` note in `init_pixel` above —
        // the reference's ceil-div idiom is kept verbatim (R1).
        #[allow(clippy::manual_div_ceil)]
        let new_len = (height * width * T::NUM_BITS + 7) / 8;
        if self.buf.len() == new_len {
            return;
        }
        self.buf.truncate(new_len);
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
        Some(self._get(idx).as_pixel())
    }

    /// Puts a single pixel at the `x` and `y` co-ordinates. No-ops if the pixel
    /// is out of range.
    pub fn put_pixel(&mut self, x: i32, y: i32, px: Pixel) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let idx = (self.width as isize * y as isize + x as isize).max(0) as usize;
        self._set(idx, T::from_pixel(px));
    }

    /// Replaces the surface with the supplied pixel.
    pub fn paint_pixel(&mut self, px: Pixel) {
        Self::paint_pixel_buf(&mut self.buf, T::from_pixel(px));
    }

    /// Copies a single pixel to the range starting at `(x, y)` and proceeding
    /// for `len`. Out-of-range start co-ordinates cause a no-op.
    pub fn paint_stride(&mut self, x: i32, y: i32, len: usize, px: Pixel) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        // Because we are doing a partial paint here, we need to be a bit more
        // careful than we would be with paint_pixel. We can do the majority
        // still with our internal `paint_pixel_buf` routine, but we need to
        // slice off the contiguous part of memory to do so.
        //
        // We've already properly aligned our buffer, so we can check the start
        // and end to make sure they're evenly divisible against our bit size.
        // The remainders are the leftovers we need to individually set, if they
        // exist.
        let src_px = T::from_pixel(px);
        let scale = 8 / T::NUM_BITS;
        let w_usize = self.width.max(0) as usize;
        let y_usize = y.max(0) as usize;
        let x_usize = x.max(0) as usize;
        let start = w_usize * y_usize + x_usize;
        let end = start + len;
        let slice_start = start / scale;
        let slice_end = end / scale;
        if slice_start > slice_end {
            panic!("invalid range for paint (start > end). this is a bug, please report it");
        } else if slice_start == slice_end {
            // There's nothing we can memset, just set the range individually.
            for idx in start..end {
                self._set(idx, src_px);
            }
            return;
        }
        let start_rem = start % scale;
        let slice_offset = if start_rem > 0 { 1 } else { 0 };
        // Set our contiguous range
        Self::paint_pixel_buf(&mut self.buf[slice_start + slice_offset..slice_end], src_px);
        // Set the ends.
        // Note that the subtractions here should be safe; start_rem is
        // (start % scale), the result of which will always be less than scale,
        // so worst case it is (scale - (max(0, scale - 1))). Worst case for
        // (end - end % scale) is always 0 when end < scale, and positive
        // otherwise.
        let l_low: usize = start; // start of left of non-contiguous range
        let l_high: usize = start + (scale - start_rem); // end of left of non-contiguous range
        let r_low: usize = end - end % scale; // start of right non-contiguous range
        let r_high: usize = end; // end of right of non-contiguous range
        for idx in l_low..l_high {
            self._set(idx, src_px);
        }
        for idx in r_low..r_high {
            self._set(idx, src_px);
        }
    }

    /// Composites a single pixel using the specified operation, with an optional
    /// opacity, to the range starting at `(x, y)` for `len`. Integer operators
    /// only. Faithful port of `PackedImageSurface(T).compositeStride`.
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
            let out =
                crate::compositor::run_pixel_t(self._get(idx).as_pixel(), src.as_pixel(), operator);
            self._set(idx, T::from_pixel(out));
        }
    }

    /// Utility to get a packed pixel out of the buffer at `index`. Because
    /// `NUM_BITS` divides 8, the value lives entirely within one byte at bit
    /// offset `(index * NUM_BITS) % 8` (little-endian). Faithful to
    /// `readPackedInt(IntType, buf, index * bits, .little)`.
    fn _get(&self, index: usize) -> T {
        let bit = index * T::NUM_BITS;
        let byte = self.buf[bit / 8];
        let shift = (bit % 8) as u32;
        let val = (byte >> shift) & T::MAX_INT;
        T::from_value(val)
    }

    /// Utility to set a packed pixel in the buffer at `index`. See `_get`.
    /// Faithful to `writePackedInt(IntType, buf, index * bits, px, .little)`.
    fn _set(&mut self, index: usize, value: T) {
        let bit = index * T::NUM_BITS;
        let shift = (bit % 8) as u32;
        let b = &mut self.buf[bit / 8];
        *b = (*b & !(T::MAX_INT << shift)) | ((value.value() & T::MAX_INT) << shift);
    }

    /// Sets an entire (sub)buffer to a single packed pixel. Faithful port of
    /// `_paintPixel`: builds one byte packed with as many copies of the pixel
    /// as fit, then fills the slice with it (a zero pixel short-circuits to a
    /// zero fill). This may fill past the end of the canvas into the excess in
    /// the last byte, but that space is undefined in our implementation anyway.
    fn paint_pixel_buf(buf: &mut [u8], px: T) {
        if px == T::default() {
            // Short-circuit to writing zeroes if the pixel we're setting is zero
            for b in buf.iter_mut() {
                *b = 0;
            }
            return;
        }

        let px_u8: u8 = px.value();
        let mut packed_px: u8 = 0;
        let mut sh: usize = 0;
        while sh <= 8 - T::NUM_BITS {
            packed_px |= px_u8 << sh;
            sh += T::NUM_BITS;
        }

        for b in buf.iter_mut() {
            *b = packed_px;
        }
    }
}

/// Represents an interface as an enum of the pixel formats.
#[derive(Debug, Clone, PartialEq)]
pub enum Surface {
    ImageSurfaceArgb(ImageSurface<Argb>),
    ImageSurfaceXrgb(ImageSurface<Xrgb>),
    ImageSurfaceRgb(ImageSurface<Rgb>),
    ImageSurfaceRgba(ImageSurface<Rgba>),
    ImageSurfaceAlpha8(ImageSurface<Alpha8>),
    ImageSurfaceAlpha4(PackedImageSurface<Alpha4>),
    ImageSurfaceAlpha2(PackedImageSurface<Alpha2>),
    ImageSurfaceAlpha1(PackedImageSurface<Alpha1>),
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
            Surface::ImageSurfaceAlpha4($s) => $body,
            Surface::ImageSurfaceAlpha2($s) => $body,
            Surface::ImageSurfaceAlpha1($s) => $body,
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
            SurfaceType::ImageSurfaceAlpha4 => Ok(Surface::ImageSurfaceAlpha4(
                PackedImageSurface::init(width, height, None)?,
            )),
            SurfaceType::ImageSurfaceAlpha2 => Ok(Surface::ImageSurfaceAlpha2(
                PackedImageSurface::init(width, height, None)?,
            )),
            SurfaceType::ImageSurfaceAlpha1 => Ok(Surface::ImageSurfaceAlpha1(
                PackedImageSurface::init(width, height, None)?,
            )),
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
            Pixel::Alpha4(px) => Ok(Surface::ImageSurfaceAlpha4(PackedImageSurface::init(
                width,
                height,
                Some(px),
            )?)),
            Pixel::Alpha2(px) => Ok(Surface::ImageSurfaceAlpha2(PackedImageSurface::init(
                width,
                height,
                Some(px),
            )?)),
            Pixel::Alpha1(px) => Ok(Surface::ImageSurfaceAlpha1(PackedImageSurface::init(
                width,
                height,
                Some(px),
            )?)),
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
            Surface::ImageSurfaceAlpha4(_) => Format::Alpha4,
            Surface::ImageSurfaceAlpha2(_) => Format::Alpha2,
            Surface::ImageSurfaceAlpha1(_) => Format::Alpha1,
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

    /// Composites `src` onto this surface using `operator`, with the source
    /// bounded to its own dimensions and oriented at `(dst_x, dst_y)`. Any parts
    /// of the source outside the destination are ignored.
    //
    // PORT NOTE: faithful scalar port of `Surface.composite`, which in z2d is a
    // one-operation call into `SurfaceCompositor.run` with the source `Param`
    // being a `.surface`. The SIMD `SurfaceCompositor`/`StrideCompositor` are not
    // in the ghostty subset; instead we reproduce `SurfaceCompositor.run`'s
    // geometry (negative-offset reflection, destination clamping, per-scanline
    // stride bounds) exactly and composite each destination pixel through the
    // scalar integer `run_pixel_t` (the same core `StrideCompositor` uses via
    // `runOperator`). For the single-surface-source op, `StrideCompositor.run`
    // reduces to `_dst = runOperator(_dst_from_dst, _src_from_src, operator)`
    // written back to the destination stride — i.e. `run_pixel_t(dst, src, op)`
    // per pixel. The computed `scanline_width` keeps every access within both
    // buffers, so no stride wrapping can occur. The `opts.precision` parameter is
    // omitted: this subset is integer-only, matching `composite_stride`.
    pub fn composite(&mut self, src: &Surface, operator: Operator, dst_x: i32, dst_y: i32) {
        // No-op if we'd just be drawing out of bounds of the surface.
        if dst_x >= self.get_width() || dst_y >= self.get_height() {
            return;
        }

        // The bounding info is taken from the (surface) source.
        let src_width = src.get_width();
        let src_height = src.get_height();
        let dst_width = self.get_width();
        let dst_height = self.get_height();

        // "Reflect" our x/y offsets if they are negative, which indicates
        // where we start on the source, otherwise we start at 0.
        let src_start_x: i32 = if dst_x < 0 {
            dst_x.unsigned_abs() as i32
        } else {
            0
        };
        let src_start_y: i32 = if dst_y < 0 {
            dst_y.unsigned_abs() as i32
        } else {
            0
        };

        // Compute our actual drawing dimensions.
        let width = if src_width + dst_x > dst_width {
            dst_width - dst_x
        } else {
            src_width
        };
        let height = if src_height + dst_y > dst_height {
            dst_height - dst_y
        } else {
            src_height
        };

        if src_start_x >= width || src_start_y >= height {
            // Our calculated source offsets are completely outside of the draw
            // area itself. This can happen when the source is offset above or
            // to the left enough that it does not overlap with the destination
            // at all (due to the reflection of the large negative offset in
            // this scenario). Return, as nothing will be drawn.
            return;
        }

        let src_scan_low: i32 = src_start_y; // limited to >= 0 above
        let src_scan_high: i32 = src_scan_low.max(height);
        let dst_start_x: i32 = src_start_x + dst_x;
        let scanline_width: usize = (width - src_start_x).max(0) as usize;

        // Assert that the dst_start_x (calculated above) and absolute minimum
        // dst_start_y (calculated per scanline below) are not below zero. This
        // should be ultimately handled above, and negative co-ordinates are
        // safe to pass into getStride (in this case, a zero-length stride is
        // returned), but it's not intended and this exists as a final
        // safeguard against that.
        {
            let dst_start_y: i32 = src_scan_low + dst_y;
            if dst_start_x < 0 || dst_start_y < 0 {
                panic!("invalid initial effective offsets. this is a bug, please report it");
            }
        }

        for src_y in src_scan_low.max(0)..src_scan_high.max(0) {
            let dst_start_y: i32 = src_y + dst_y;
            // Composite this scanline: the destination stride starts at
            // `(dst_start_x, dst_start_y)`, the source stride at
            // `(src_start_x, src_y)`, both `scanline_width` long.
            for k in 0..scanline_width as i32 {
                let dpx = self.get_pixel(dst_start_x + k, dst_start_y);
                let spx = src.get_pixel(src_start_x + k, src_y);
                if let (Some(dpx), Some(spx)) = (dpx, spx) {
                    let out = crate::compositor::run_pixel_t(dpx, spx, operator);
                    self.put_pixel(dst_start_x + k, dst_start_y, out);
                }
            }
        }
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

    // Port of `surface.zig` `test "PackedImageSurface, alpha4"`. The `getStride`
    // sub-block is omitted — `get_stride`/`Stride` is not part of the rustty
    // subset (see the module SUBSET NOTE).
    #[test]
    fn packed_image_surface_alpha4() {
        {
            // basic layout test, 3x3, puts pixels in the following layout:
            //
            // 5  0  0
            // 0  10 0
            // 0  0  15
            //
            // In this instance, we expect [5]u8 as the buffer, with the
            // appropriate bits set, endian defined. Our padding (last 6 bits in
            // the last byte) is all zeroes.
            let mut sfc = PackedImageSurface::<Alpha4>::init(3, 3, None).unwrap();
            sfc.put_pixel(0, 0, Pixel::Alpha4(Alpha4 { a: 5 }));
            sfc.put_pixel(1, 1, Pixel::Alpha4(Alpha4 { a: 10 }));
            sfc.put_pixel(2, 2, Pixel::Alpha4(Alpha4 { a: 15 }));

            assert_eq!(5, sfc.buf.len());
            assert_eq!(5, sfc.buf[0]);
            assert_eq!(0, sfc.buf[1]);
            assert_eq!(10, sfc.buf[2]);
            assert_eq!(0, sfc.buf[3]);
            assert_eq!(15, sfc.buf[4]);
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 5 })), sfc.get_pixel(0, 0));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(1, 0));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(2, 0));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(0, 1));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 10 })), sfc.get_pixel(1, 1));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(2, 1));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(0, 2));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(1, 2));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(2, 2));
            assert_eq!(None, sfc.get_pixel(0, 3));
            sfc.deinit();
        }

        {
            // Initial pixel initialization. We expect this to fill the whole 3
            // bytes in the buffer even though the buffer is supposed to only be
            // 18 bits (bounds checks are a product of HxW on part of the
            // consumer).
            let mut sfc = PackedImageSurface::<Alpha4>::init(3, 3, Some(Alpha4 { a: 15 })).unwrap();

            assert_eq!(5, sfc.buf.len());
            assert_eq!(0xFF, sfc.buf[0]);
            assert_eq!(0xFF, sfc.buf[1]);
            assert_eq!(0xFF, sfc.buf[2]);
            assert_eq!(0xFF, sfc.buf[3]);
            assert_eq!(0xFF, sfc.buf[4]);
            sfc.deinit();
        }

        {
            // Downsampling
            let mut sfc = PackedImageSurface::<Alpha4>::init(4, 4, None).unwrap();
            sfc.put_pixel(0, 0, Pixel::Alpha4(Alpha4 { a: 15 }));
            sfc.put_pixel(0, 1, Pixel::Alpha4(Alpha4 { a: 15 }));
            sfc.put_pixel(0, 2, Pixel::Alpha4(Alpha4 { a: 15 }));
            sfc.put_pixel(0, 3, Pixel::Alpha4(Alpha4 { a: 15 }));
            sfc.put_pixel(1, 0, Pixel::Alpha4(Alpha4 { a: 15 }));
            sfc.put_pixel(1, 1, Pixel::Alpha4(Alpha4 { a: 15 }));
            sfc.put_pixel(1, 2, Pixel::Alpha4(Alpha4 { a: 15 }));
            sfc.put_pixel(1, 3, Pixel::Alpha4(Alpha4 { a: 15 }));

            sfc.downsample();

            assert_eq!(1, sfc.width);
            assert_eq!(1, sfc.height);
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 7 })), sfc.get_pixel(0, 0));
            sfc.deinit();
        }

        {
            // paintStride, testing contiguous ranges. Unlike paintPixel, we
            // want to make sure only a very specific range of pixels were
            // touched.
            let mut sfc = PackedImageSurface::<Alpha4>::init(3, 3, None).unwrap();

            sfc.paint_stride(1, 0, 6, Pixel::Alpha4(Alpha4 { a: 15 }));

            assert_eq!(5, sfc.buf.len());
            assert_eq!(0xF0, sfc.buf[0]);
            assert_eq!(0xFF, sfc.buf[1]);
            assert_eq!(0xFF, sfc.buf[2]);
            assert_eq!(0x0F, sfc.buf[3]);
            assert_eq!(0x00, sfc.buf[4]);
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(0, 0));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(1, 0));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(2, 0));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(0, 1));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(1, 1));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(2, 1));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(0, 2));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(1, 2));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(2, 2));

            sfc.paint_pixel(Pixel::Alpha4(Alpha4 { a: 0 }));
            sfc.paint_stride(0, 1, 6, Pixel::Alpha4(Alpha4 { a: 15 }));
            assert_eq!(0x00, sfc.buf[0]);
            assert_eq!(0xF0, sfc.buf[1]);
            assert_eq!(0xFF, sfc.buf[2]);
            assert_eq!(0xFF, sfc.buf[3]);
            assert_eq!(0x0F, sfc.buf[4]);
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(0, 0));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(1, 0));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 0 })), sfc.get_pixel(2, 0));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(0, 1));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(1, 1));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(2, 1));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(0, 2));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(1, 2));
            assert_eq!(Some(Pixel::Alpha4(Alpha4 { a: 15 })), sfc.get_pixel(2, 2));
            sfc.deinit();
        }
    }

    // Port of `surface.zig` `test "PackedImageSurface, alpha2"` (getStride
    // sub-block omitted, see SUBSET NOTE).
    #[test]
    fn packed_image_surface_alpha2() {
        {
            // basic layout test, 3x3:
            //
            // 1 0 0
            // 0 2 0
            // 0 0 3
            let mut sfc = PackedImageSurface::<Alpha2>::init(3, 3, None).unwrap();
            sfc.put_pixel(0, 0, Pixel::Alpha2(Alpha2 { a: 1 }));
            sfc.put_pixel(1, 1, Pixel::Alpha2(Alpha2 { a: 2 }));
            sfc.put_pixel(2, 2, Pixel::Alpha2(Alpha2 { a: 3 }));

            assert_eq!(3, sfc.buf.len());
            assert_eq!(1, sfc.buf[0]);
            assert_eq!(2, sfc.buf[1]);
            assert_eq!(3, sfc.buf[2]);
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 1 })), sfc.get_pixel(0, 0));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(1, 0));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(2, 0));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(0, 1));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 2 })), sfc.get_pixel(1, 1));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(2, 1));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(0, 2));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(1, 2));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(2, 2));
            assert_eq!(None, sfc.get_pixel(0, 3));
            sfc.deinit();
        }

        {
            // Initial pixel initialization.
            let mut sfc = PackedImageSurface::<Alpha2>::init(3, 3, Some(Alpha2 { a: 3 })).unwrap();

            assert_eq!(3, sfc.buf.len());
            assert_eq!(0xFF, sfc.buf[0]);
            assert_eq!(0xFF, sfc.buf[1]);
            assert_eq!(0xFF, sfc.buf[2]);
            sfc.deinit();
        }

        {
            // Downsampling
            let mut sfc = PackedImageSurface::<Alpha2>::init(4, 4, None).unwrap();
            sfc.put_pixel(0, 0, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(0, 1, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(0, 2, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(0, 3, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(1, 0, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(1, 1, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(1, 2, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(1, 3, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(2, 0, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(2, 1, Pixel::Alpha2(Alpha2 { a: 3 }));
            sfc.put_pixel(2, 2, Pixel::Alpha2(Alpha2 { a: 3 }));

            sfc.downsample();

            assert_eq!(1, sfc.width);
            assert_eq!(1, sfc.height);
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 2 })), sfc.get_pixel(0, 0));
            sfc.deinit();
        }

        {
            // paintStride, testing contiguous ranges.
            let mut sfc = PackedImageSurface::<Alpha2>::init(3, 3, None).unwrap();

            sfc.paint_stride(1, 0, 6, Pixel::Alpha2(Alpha2 { a: 3 }));

            assert_eq!(3, sfc.buf.len());
            assert_eq!(0xFC, sfc.buf[0]);
            assert_eq!(0x3F, sfc.buf[1]);
            assert_eq!(0x00, sfc.buf[2]);
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(0, 0));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(1, 0));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(2, 0));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(0, 1));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(1, 1));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(2, 1));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(0, 2));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(1, 2));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(2, 2));

            sfc.paint_pixel(Pixel::Alpha2(Alpha2 { a: 0 }));
            sfc.paint_stride(0, 1, 6, Pixel::Alpha2(Alpha2 { a: 3 }));
            assert_eq!(0xC0, sfc.buf[0]);
            assert_eq!(0xFF, sfc.buf[1]);
            assert_eq!(0x03, sfc.buf[2]);
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(0, 0));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(1, 0));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 0 })), sfc.get_pixel(2, 0));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(0, 1));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(1, 1));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(2, 1));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(0, 2));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(1, 2));
            assert_eq!(Some(Pixel::Alpha2(Alpha2 { a: 3 })), sfc.get_pixel(2, 2));
            sfc.deinit();
        }
    }

    // Port of `surface.zig` `test "PackedImageSurface, alpha1"` (getStride
    // sub-block omitted, see SUBSET NOTE).
    #[test]
    fn packed_image_surface_alpha1() {
        {
            // basic layout test, 3x3:
            //
            // 1 0 0
            // 0 1 0
            // 0 0 1
            let mut sfc = PackedImageSurface::<Alpha1>::init(3, 3, None).unwrap();
            sfc.put_pixel(0, 0, Pixel::Alpha1(Alpha1 { a: 1 }));
            sfc.put_pixel(1, 1, Pixel::Alpha1(Alpha1 { a: 1 }));
            sfc.put_pixel(2, 2, Pixel::Alpha1(Alpha1 { a: 1 }));

            assert_eq!(2, sfc.buf.len());
            assert_eq!(0b10001, sfc.buf[0]);
            assert_eq!(1, sfc.buf[1]);
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 0));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(1, 0));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(2, 0));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(0, 1));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(1, 1));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(2, 1));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(0, 2));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(1, 2));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(2, 2));
            assert_eq!(None, sfc.get_pixel(0, 3));
            sfc.deinit();
        }

        {
            // Initial pixel initialization.
            let mut sfc = PackedImageSurface::<Alpha1>::init(3, 3, Some(Alpha1 { a: 1 })).unwrap();

            assert_eq!(2, sfc.buf.len());
            assert_eq!(0xFF, sfc.buf[0]);
            assert_eq!(0xFF, sfc.buf[1]);
            sfc.deinit();
        }

        {
            // Downsampling. Note that we don't really use downsampling on
            // Alpha1 as it's kind of pointless (a pixel is either just off or
            // on).
            let mut sfc = PackedImageSurface::<Alpha1>::init(4, 4, Some(Alpha1 { a: 1 })).unwrap();

            sfc.downsample();

            assert_eq!(1, sfc.width);
            assert_eq!(1, sfc.height);
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 0));
            sfc.deinit();
        }

        {
            // Downsampling with a pixel off (will render the whole downsampled
            // pixel off as per the Alpha1 implementation).
            let mut sfc = PackedImageSurface::<Alpha1>::init(4, 4, Some(Alpha1 { a: 1 })).unwrap();

            sfc.put_pixel(0, 0, Pixel::Alpha1(Alpha1 { a: 0 }));
            sfc.downsample();

            assert_eq!(1, sfc.width);
            assert_eq!(1, sfc.height);
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(0, 0));
            sfc.deinit();
        }

        {
            // paintStride, testing contiguous ranges.
            let mut sfc = PackedImageSurface::<Alpha1>::init(3, 3, None).unwrap();

            sfc.paint_stride(1, 0, 6, Pixel::Alpha1(Alpha1 { a: 1 }));

            assert_eq!(2, sfc.buf.len());
            assert_eq!(0b1111110, sfc.buf[0]);
            assert_eq!(0, sfc.buf[1]);
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(0, 0));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(1, 0));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(2, 0));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 1));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(1, 1));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(2, 1));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 2));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(1, 2));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(2, 2));

            sfc.paint_pixel(Pixel::Alpha1(Alpha1 { a: 0 }));
            sfc.paint_stride(0, 1, 6, Pixel::Alpha1(Alpha1 { a: 1 }));
            assert_eq!(0b11111000, sfc.buf[0]);
            assert_eq!(1, sfc.buf[1]);
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(0, 0));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(1, 0));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 0 })), sfc.get_pixel(2, 0));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 1));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(1, 1));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(2, 1));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 2));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(1, 2));
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(2, 2));
            sfc.deinit();
        }
    }

    // Port of `surface.zig` `test "PackedImageSurface, dimension validation"`.
    // NOTE: the Zig error cases invoke `ImageSurface(pixel.Alpha1).init` (the
    // non-packed generic); rustty's `ImageSurface<T>` requires a `SurfacePixel`
    // and does not carry the packed alpha types, so we invoke
    // `PackedImageSurface::<Alpha1>::init` for all cases — the InvalidWidth/
    // InvalidHeight assertions (raised before any allocation) are identical.
    #[test]
    fn packed_image_surface_dimension_validation() {
        {
            let mut sfc = PackedImageSurface::<Alpha1>::init(1, 1, Some(Alpha1 { a: 1 })).unwrap();
            sfc.deinit();
        }
        assert_eq!(
            Err(Error::InvalidWidth),
            PackedImageSurface::<Alpha1>::init(0, 1, Some(Alpha1 { a: 1 }))
        );
        assert_eq!(
            Err(Error::InvalidWidth),
            PackedImageSurface::<Alpha1>::init(-1, 1, Some(Alpha1 { a: 1 }))
        );
        assert_eq!(
            Err(Error::InvalidHeight),
            PackedImageSurface::<Alpha1>::init(1, 0, Some(Alpha1 { a: 1 }))
        );
        assert_eq!(
            Err(Error::InvalidHeight),
            PackedImageSurface::<Alpha1>::init(1, -1, Some(Alpha1 { a: 1 }))
        );
    }

    // Port of `surface.zig` `test "PackedImageSurface.downsample, edge cases"`.
    #[test]
    fn packed_image_surface_downsample_edge_cases() {
        {
            // Width not divisible by 4
            let mut sfc = PackedImageSurface::<Alpha1>::init(7, 4, Some(Alpha1 { a: 1 })).unwrap();
            sfc.downsample();
            assert_eq!(1, sfc.width);
            assert_eq!(1, sfc.height);
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 0));
            sfc.deinit();
        }
        {
            // Height not divisible by 4
            let mut sfc = PackedImageSurface::<Alpha1>::init(4, 7, Some(Alpha1 { a: 1 })).unwrap();
            sfc.downsample();
            assert_eq!(1, sfc.width);
            assert_eq!(1, sfc.height);
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 0));
            sfc.deinit();
        }
        {
            // Width too small
            let mut sfc = PackedImageSurface::<Alpha1>::init(3, 4, Some(Alpha1 { a: 1 })).unwrap();
            sfc.downsample();
            assert_eq!(3, sfc.width);
            assert_eq!(4, sfc.height);
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 0));
            sfc.deinit();
        }
        {
            // Height too small
            let mut sfc = PackedImageSurface::<Alpha1>::init(4, 3, Some(Alpha1 { a: 1 })).unwrap();
            sfc.downsample();
            assert_eq!(4, sfc.width);
            assert_eq!(3, sfc.height);
            assert_eq!(Some(Pixel::Alpha1(Alpha1 { a: 1 })), sfc.get_pixel(0, 0));
            sfc.deinit();
        }
    }

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

    #[test]
    fn composite_alpha8_src() {
        // `.src` copies the source alpha onto the destination, ignoring the
        // existing destination pixel.
        let mut dst = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 200 }), 4, 3).unwrap();
        let src = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 100 }), 4, 3).unwrap();
        dst.composite(&src, Operator::Src, 0, 0);
        for y in 0..3 {
            for x in 0..4 {
                assert_eq!(Some(Pixel::Alpha8(Alpha8 { a: 100 })), dst.get_pixel(x, y));
            }
        }
    }

    #[test]
    fn composite_alpha8_src_over() {
        // `.src_over`: out.a = src.a + dst.a - mul(src.a, dst.a)
        //            = 100 + 200 - (100 * 200 / 255) = 300 - 78 = 222.
        let mut dst = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 200 }), 4, 3).unwrap();
        let src = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 100 }), 4, 3).unwrap();
        dst.composite(&src, Operator::SrcOver, 0, 0);
        for y in 0..3 {
            for x in 0..4 {
                assert_eq!(Some(Pixel::Alpha8(Alpha8 { a: 222 })), dst.get_pixel(x, y));
            }
        }
    }

    #[test]
    fn composite_alpha8_offset_clipped() {
        // A 2x2 source placed at (3, 2) on a 4x3 destination: only the top-left
        // source pixel lands (at dst (3, 2)); the rest is clipped by the
        // destination bounds. `.src` overwrites just that pixel.
        let mut dst = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 0 }), 4, 3).unwrap();
        let src = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 128 }), 2, 2).unwrap();
        dst.composite(&src, Operator::Src, 3, 2);
        for y in 0..3 {
            for x in 0..4 {
                let want = if x == 3 && y == 2 { 128 } else { 0 };
                assert_eq!(Some(Pixel::Alpha8(Alpha8 { a: want })), dst.get_pixel(x, y));
            }
        }
    }

    #[test]
    fn composite_alpha8_negative_offset_reflects() {
        // A 4x4 source placed at (-2, -1): the negative offset reflects the
        // source start to (2, 1), so dst (0, 0) receives src (2, 1). With a
        // uniform source and `.src` the whole (overlapping) destination is set.
        let mut dst = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 0 }), 4, 3).unwrap();
        let src = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 77 }), 4, 4).unwrap();
        dst.composite(&src, Operator::Src, -2, -1);
        // Overlap region: dst x in 0..2 (src cols 2..4), dst y in 0..3 (src rows 1..4).
        for y in 0..3 {
            for x in 0..4 {
                let want = if x < 2 { 77 } else { 0 };
                assert_eq!(Some(Pixel::Alpha8(Alpha8 { a: want })), dst.get_pixel(x, y));
            }
        }
    }
}
