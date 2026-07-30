//! rustty-zigimg — the six-operation `zigimg` surface `libvaxis` 0.5.1 instantiates,
//! re-homed onto ghostty's own vendored codecs.
//!
//! See `Cargo.toml`'s header for the full scope statement, the measured size of the
//! alternative (a faithful 29k-LOC zigimg port), and the one bounded divergence
//! (`from_*` accept PNG + JPEG rather than zigimg's 15 formats). WS12 Decision 2,
//! Option Z-C, ratified 2026-07-29.
//!
//! REFERENCE: `zigimg` 0.1.0 `src/Image.zig` + `src/ImageUnmanaged.zig`
//! (`ivanstepanovftw/zigimg` @ `d7b7ab0ba0899643831ef042bd73289510b39906`).
//!
//! Byte-slice discipline: pixels and PNG bytes are `Vec<u8>`/`&[u8]`, never a lossy
//! `String` — Zig `[]const u8` maps to raw bytes.

pub mod convert;
pub mod decode;
pub mod png_encode;

pub use decode::{from_file_path, from_memory};
pub use png_encode::write_to_memory;

/// The two `zigimg` pixel formats `libvaxis` names, and the only two.
///
/// PORT NOTE: `zigimg` `src/pixel_format.zig` defines 24 formats; `Vaxis.zig:983-1015`
/// reaches only `.rgb24` (`pixel_format.zig` — 3 bytes/px, R,G,B) and `.rgba32`
/// (4 bytes/px, R,G,B,A, straight/non-premultiplied alpha). The other 22 are out of
/// scope, not deferred: no `libvaxis` code path can construct them here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// `zigimg` `.rgb24` — `color.Rgb24 = RgbColor(u8)`, 3 bytes per pixel.
    Rgb24,
    /// `zigimg` `.rgba32` — `color.Rgba32 = RgbaColor(u8)`, 4 bytes per pixel. This is
    /// also exactly what `rustty_wuffs` decode returns (RGBA_NONPREMUL).
    Rgba32,
}

impl PixelFormat {
    /// Bytes per pixel. `zigimg` derives this from the packed struct's `@sizeOf`.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgb24 => 3,
            PixelFormat::Rgba32 => 4,
        }
    }
}

/// The error surface. Each variant exists because a `libvaxis` call path can produce it.
#[derive(Debug)]
pub enum Error {
    /// The input's magic bytes match none of the formats this crate decodes.
    ///
    /// PORT NOTE: `zigimg`'s `ImageUnmanaged.detectFormatFromMemory` returns
    /// `error.Unsupported` when no `FormatInterface.formatDetect` claims the stream.
    /// Same meaning here, over a smaller format set (see the crate header).
    UnsupportedFormat,
    /// The codec rejected the stream (a corrupt or truncated image), or the decoded
    /// dimensions are unusable.
    Decode,
    /// `writeToMemory`'s buffer was too small. **Load-bearing fidelity, not a bug to
    /// "fix":** `Vaxis.zig:996-998` sizes the PNG buffer at `img.imageByteSize()` — the
    /// *raw* pixel byte count — so a high-entropy image whose PNG encoding does not
    /// compress below its raw size legitimately overflows upstream too, surfacing as
    /// `error.NoSpaceLeft` from `io.WriteStream.initMemory`. Preserved deliberately.
    NoSpaceLeft,
    /// Compression failed (a malformed zlib init or a non-`Z_STREAM_END` result).
    Compress,
    /// `fromFilePath` could not read the file.
    Io(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnsupportedFormat => write!(f, "unsupported image format"),
            Error::Decode => write!(f, "image decode failed"),
            Error::NoSpaceLeft => write!(f, "no space left in the output buffer"),
            Error::Compress => write!(f, "DEFLATE compression failed"),
            Error::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// A decoded image: dimensions, a pixel format, and the raw pixel bytes.
///
/// PORT NOTE (`zigimg` `Image.zig:20-27`): the reference carries an `allocator`, a
/// `color.PixelStorage` tagged union, and an `animation`. Here the storage is a plain
/// `Vec<u8>` tagged by `pixel_format` — the union's only two reachable arms both hold a
/// flat byte buffer, so the tag plus the bytes is the whole state. `animation` is
/// dropped: `Vaxis.zig` never reads it (`isAnimation` is not among the six operations),
/// and a field nothing can observe is out of scope rather than faked.
///
/// PORT NOTE: **there is no `deinit`/`Drop`.** The reference's `img.deinit()`
/// (`Vaxis.zig:1030`, `defer img.deinit()`) frees the allocator-owned pixel storage;
/// Rust ownership does that when the `Image` drops. That is operation 6 of the six,
/// discharged by the language rather than by an API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels (`Image.zig:22`; `usize` in the reference).
    pub width: usize,
    /// Height in pixels (`Image.zig:23`).
    pub height: usize,
    /// The active pixel format — `zigimg`'s `std.meta.activeTag(self.pixels)`
    /// (`Image.zig:92`, `pixelFormat()`).
    pub pixel_format: PixelFormat,
    /// Raw pixel bytes, `width * height * bytes_per_pixel` long.
    pixels: Vec<u8>,
}

impl Image {
    /// Build an `Image` from raw pixel bytes.
    ///
    /// Mirrors `Image.zig:70-78` (`fromRawPixels`) minus the allocator dup — the caller
    /// hands ownership over. Returns `Error::Decode` if `pixels.len()` disagrees with
    /// `width * height * bytes_per_pixel`, which the reference enforces structurally via
    /// `PixelStorage.initRawPixels`.
    pub fn from_raw_pixels(
        width: usize,
        height: usize,
        pixels: Vec<u8>,
        pixel_format: PixelFormat,
    ) -> Result<Self, Error> {
        let want = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(pixel_format.bytes_per_pixel()))
            .ok_or(Error::Decode)?;
        if pixels.len() != want {
            return Err(Error::Decode);
        }
        Ok(Image {
            width,
            height,
            pixel_format,
            pixels,
        })
    }

    /// `zigimg` `Image.zig:92` — `pub fn pixelFormat(self: Image) PixelFormat`.
    pub fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    /// `zigimg` `Image.zig:97` — `pub fn rawBytes(self: Image) []const u8`.
    /// Operation 5. Byte slice, never a `String`.
    pub fn raw_bytes(&self) -> &[u8] {
        &self.pixels
    }

    /// `zigimg` `Image.zig:102` — `pub fn rowByteSize(self: Image) usize`, defined as
    /// `imageByteSize() / height`. Faithfully preserves the reference's divide-by-height,
    /// including that a zero-height image would trap there; guarded to 0 here because
    /// Rust integer division panics rather than trapping and `libvaxis` never constructs
    /// a zero-height image.
    pub fn row_byte_size(&self) -> usize {
        if self.height == 0 {
            0
        } else {
            self.image_byte_size() / self.height
        }
    }

    /// `zigimg` `Image.zig:107` — `pub fn imageByteSize(self: Image) usize`, defined as
    /// `rawBytes().len`. Operation 5, and the value `Vaxis.zig:996` uses to size its PNG
    /// buffer (see `Error::NoSpaceLeft`).
    pub fn image_byte_size(&self) -> usize {
        self.pixels.len()
    }

    /// `zigimg` `Image.zig:135` / `ImageUnmanaged.zig:280` — `convert`. Operation 4.
    ///
    /// The reference's first act is the identity guard
    /// `if (std.meta.activeTag(self.pixels) == destination_format) return;`
    /// (`ImageUnmanaged.zig:282`), preserved here — it matters because
    /// `transmit_image(..., Rgba)` on an already-`rgba32` decode result must not
    /// round-trip the pixels.
    pub fn convert(&mut self, destination_format: PixelFormat) {
        if self.pixel_format == destination_format {
            return;
        }
        self.pixels = match (self.pixel_format, destination_format) {
            (PixelFormat::Rgba32, PixelFormat::Rgb24) => convert::rgba32_to_rgb24(&self.pixels),
            (PixelFormat::Rgb24, PixelFormat::Rgba32) => convert::rgb24_to_rgba32(&self.pixels),
            // Unreachable: `PixelFormat` has exactly two variants and the identity pairs
            // returned above.
            (a, b) => unreachable!("no conversion arm for {a:?} -> {b:?}"),
        };
        self.pixel_format = destination_format;
    }

    /// `zigimg` `Image.zig:129` / `ImageUnmanaged.zig:268` — `writeToMemory` with
    /// `.{ .png = .{} }`. Operation 3. See [`png_encode::write_to_memory`].
    pub fn write_to_memory<'o>(&self, out: &'o mut [u8]) -> Result<&'o [u8], Error> {
        png_encode::write_to_memory(self, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_per_pixel_matches_zigimg_pixel_format() {
        assert_eq!(PixelFormat::Rgb24.bytes_per_pixel(), 3);
        assert_eq!(PixelFormat::Rgba32.bytes_per_pixel(), 4);
    }

    #[test]
    fn from_raw_pixels_rejects_a_length_mismatch() {
        // 2x2 rgba32 needs 16 bytes; 15 must be refused rather than silently accepted
        // (the reference gets this structurally from PixelStorage).
        assert!(matches!(
            Image::from_raw_pixels(2, 2, vec![0u8; 15], PixelFormat::Rgba32),
            Err(Error::Decode)
        ));
        assert!(Image::from_raw_pixels(2, 2, vec![0u8; 16], PixelFormat::Rgba32).is_ok());
        // And the rgb24 stride is genuinely different — a test that passed for both
        // would not be pinning bytes_per_pixel at all.
        assert!(matches!(
            Image::from_raw_pixels(2, 2, vec![0u8; 16], PixelFormat::Rgb24),
            Err(Error::Decode)
        ));
        assert!(Image::from_raw_pixels(2, 2, vec![0u8; 12], PixelFormat::Rgb24).is_ok());
    }

    #[test]
    fn size_accessors_mirror_the_reference_definitions() {
        // 4 wide x 2 high rgb24 => 24 bytes total, 12 bytes per row.
        let img = Image::from_raw_pixels(4, 2, vec![7u8; 24], PixelFormat::Rgb24).unwrap();
        assert_eq!(img.image_byte_size(), 24);
        assert_eq!(img.row_byte_size(), 12);
        assert_eq!(img.raw_bytes().len(), 24);
        assert_eq!(img.pixel_format(), PixelFormat::Rgb24);
    }

    #[test]
    fn convert_to_the_same_format_is_the_reference_identity_guard() {
        // ImageUnmanaged.zig:282. Not a no-op test for its own sake: without the guard,
        // `transmit_image(.., Rgba)` on an rgba32 decode would round-trip every pixel.
        let mut img = Image::from_raw_pixels(1, 1, vec![1, 2, 3, 4], PixelFormat::Rgba32).unwrap();
        let before = img.clone();
        img.convert(PixelFormat::Rgba32);
        assert_eq!(img, before);
    }
}
