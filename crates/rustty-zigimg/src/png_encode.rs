//! PNG encode — `zigimg` operation 3, re-homed onto the vendored zlib in `zlib-sys`.
//!
//! REFERENCE: `zigimg` `Image.zig:129` (`writeToMemory(write_buffer, encoder_options)`)
//! → `ImageUnmanaged.zig:268`, which wraps the caller's buffer in
//! `io.WriteStream.initMemory` and dispatches to `formats/png.zig`'s writer.
//! `Vaxis.zig:993-999` is the only `libvaxis` caller and always passes
//! `.{ .png = .{} }`, i.e. the default PNG encoder options.
//!
//! WHY A LOCAL ENCODER: `rustty-wuffs` is a DECODER only — Wuffs, by design, does not
//! encode. Ghostty vendors no PNG encoder on any path this crate could borrow. So the
//! ~150 lines below are the one piece of genuinely new code Option Z-C needs, and they
//! are deliberately the most boring possible PNG: 8-bit, non-interlaced, one `IDAT`,
//! filter type 0 on every scanline.
//!
//! FIDELITY OF WHAT REACHES THE TERMINAL: PNG is lossless, so a decode of this encoder's
//! output is bit-identical in PIXELS to a decode of `zigimg`'s output. The compressed
//! bytes differ (different deflate parameters and filter choices), which is unobservable
//! to a terminal: the kitty-graphics protocol carries a PNG payload and the terminal
//! decodes it. Nothing in the escape sequence depends on the compressed length except the
//! chunking, which the already-LIVE `transmit_pre_encoded_image` handles generically.

use std::os::raw::{c_char, c_int, c_uint, c_ulong};

use crate::{Error, Image, PixelFormat};

/// PNG signature (RFC 2083 §3.1).
const SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// `zigimg` `Image.zig:129` / `ImageUnmanaged.zig:268` — `writeToMemory`.
///
/// Encodes `img` as a PNG into `out` and returns the written prefix — the faithful map of
/// the reference returning `write_stream.memory.buffered()`, i.e. a slice of the caller's
/// buffer rather than a fresh allocation.
///
/// Returns [`Error::NoSpaceLeft`] when the encoding does not fit. See that variant's doc:
/// this is a PRESERVED upstream quirk, because `Vaxis.zig:996` sizes the buffer at
/// `img.imageByteSize()` (the *raw* pixel count), so a high-entropy image can legitimately
/// fail to fit upstream too.
pub fn write_to_memory<'o>(img: &Image, out: &'o mut [u8]) -> Result<&'o [u8], Error> {
    // Colour type per RFC 2083 §4.1.1: 2 = truecolour (RGB), 6 = truecolour with alpha.
    let colour_type: u8 = match img.pixel_format {
        PixelFormat::Rgb24 => 2,
        PixelFormat::Rgba32 => 6,
    };
    let bpp = img.pixel_format.bytes_per_pixel();

    let width: u32 = u32::try_from(img.width).map_err(|_| Error::Decode)?;
    let height: u32 = u32::try_from(img.height).map_err(|_| Error::Decode)?;
    // RFC 2083: zero dimensions are invalid.
    if width == 0 || height == 0 {
        return Err(Error::Decode);
    }
    let stride = img.width.checked_mul(bpp).ok_or(Error::Decode)?;
    if img.raw_bytes().len() != stride.checked_mul(img.height).ok_or(Error::Decode)? {
        return Err(Error::Decode);
    }

    // Raw scanline stream: each row prefixed with filter type 0 ("None", RFC 2083 §6.2).
    let mut raw = Vec::with_capacity((stride + 1) * img.height);
    for row in img.raw_bytes().chunks_exact(stride) {
        raw.push(0u8);
        raw.extend_from_slice(row);
    }

    let idat = deflate_zlib(&raw)?;

    // Assemble, then copy into `out` only if it fits — so a NoSpaceLeft return leaves the
    // caller's buffer untouched rather than half-written.
    let mut png = Vec::with_capacity(SIGNATURE.len() + 12 * 3 + 13 + idat.len());
    png.extend_from_slice(&SIGNATURE);

    // IHDR (RFC 2083 §4.1.1): width, height, bit depth 8, colour type, compression 0
    // (deflate), filter method 0 (adaptive), interlace 0 (none).
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8);
    ihdr.push(colour_type);
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    write_chunk(&mut png, b"IHDR", &ihdr);

    // One IDAT. Splitting into several is legal but pointless here.
    write_chunk(&mut png, b"IDAT", &idat);
    write_chunk(&mut png, b"IEND", &[]);

    if png.len() > out.len() {
        return Err(Error::NoSpaceLeft);
    }
    out[..png.len()].copy_from_slice(&png);
    Ok(&out[..png.len()])
}

/// Append a PNG chunk: 4-byte big-endian length, 4-byte type, data, 4-byte CRC-32 over
/// type+data (RFC 2083 §3.2).
fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

// ---------------------------------------------------------------------------
// CRC-32 (ISO 3309 / ITU-T V.42), the polynomial PNG mandates.
//
// A local pure-Rust table on purpose: `zlib-sys` binds `adler32` (which zlib needs for
// its own stream trailer) but NOT `crc32`, and per the WS12 plan a new FFI binding must
// not be added for this. 40 lines of table-driven CRC is the cheaper, Miri-live option.
// ---------------------------------------------------------------------------

struct Crc32(u32);

impl Crc32 {
    fn new() -> Self {
        Crc32(0xFFFF_FFFF)
    }

    fn update(&mut self, data: &[u8]) {
        for &b in data {
            let idx = ((self.0 ^ u32::from(b)) & 0xFF) as usize;
            self.0 = CRC_TABLE[idx] ^ (self.0 >> 8);
        }
    }

    fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

/// The standard CRC-32 table for polynomial `0xEDB88320`, built at compile time so there
/// is no lazy-init state and nothing to get wrong at runtime.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0usize;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

// ---------------------------------------------------------------------------
// zlib-wrapped DEFLATE.
// ---------------------------------------------------------------------------

/// Compress `data` as a **zlib-wrapped** DEFLATE stream, which is what a PNG `IDAT`
/// requires (RFC 2083 §10.3: "the compressed data stream is a zlib stream").
///
/// `zlib-sys` already exposes `deflate_raw`, but that is `-MAX_WBITS` (raw, no zlib
/// header/trailer) because it exists to reproduce framegen. PNG needs the wrapper, i.e.
/// `windowBits = +MAX_WBITS`, so this drives the same `deflateInit2_`/`deflate`/
/// `deflateEnd` triple directly rather than reimplementing the 2-byte header and adler32
/// trailer by hand.
fn deflate_zlib(data: &[u8]) -> Result<Vec<u8>, Error> {
    // SAFETY: a fully zeroed z_stream is zlib's documented pre-init state; all pointer
    // fields are NULL so zlib installs its own allocator.
    let mut strm: zlib_sys::z_stream = unsafe { std::mem::zeroed() };

    // SAFETY: valid zeroed stream. windowBits = +MAX_WBITS selects the zlib wrapper
    // (the sign is the whole difference from zlib_sys::deflate_raw); the remaining
    // parameters are zlib's documented defaults.
    let rc = unsafe {
        zlib_sys::deflateInit2_(
            &mut strm,
            zlib_sys::Z_DEFAULT_COMPRESSION,
            zlib_sys::Z_DEFLATED,
            zlib_sys::MAX_WBITS,
            8,
            zlib_sys::Z_DEFAULT_STRATEGY,
            zlib_sys::ZLIB_VERSION.as_ptr() as *const c_char,
            std::mem::size_of::<zlib_sys::z_stream>() as c_int,
        )
    };
    if rc != zlib_sys::Z_OK {
        return Err(Error::Compress);
    }

    // Guard so deflateEnd always runs, including on the early returns below.
    struct Guard(*mut zlib_sys::z_stream);
    impl Drop for Guard {
        fn drop(&mut self) {
            // SAFETY: the stream was successfully initialized above.
            unsafe {
                zlib_sys::deflateEnd(self.0);
            }
        }
    }
    let _guard = Guard(&mut strm);

    // SAFETY: compressBound is a pure size computation.
    let bound = unsafe { zlib_sys::compressBound(data.len() as c_ulong) } as usize;
    let mut out: Vec<u8> = vec![0u8; bound];

    strm.next_in = data.as_ptr();
    strm.avail_in = data.len() as c_uint;
    strm.next_out = out.as_mut_ptr();
    strm.avail_out = out.len() as c_uint;

    // SAFETY: strm is initialized; the in/out buffers are valid and `out` is
    // compressBound-sized, so a single Z_FINISH completes the stream.
    let rc = unsafe { zlib_sys::deflate(&mut strm, zlib_sys::Z_FINISH) };
    if rc != zlib_sys::Z_STREAM_END {
        return Err(Error::Compress);
    }

    out.truncate(strm.total_out as usize);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The CRC table and chunk framing are pure logic and stay Miri-live.

    #[test]
    fn crc32_matches_published_vectors() {
        // The canonical CRC-32 check value: "123456789" => 0xCBF43926.
        let mut c = Crc32::new();
        c.update(b"123456789");
        assert_eq!(c.finish(), 0xCBF4_3926);

        // The empty input is the identity (0xFFFFFFFF ^ 0xFFFFFFFF).
        assert_eq!(Crc32::new().finish(), 0);

        // "IEND" with no data — the exact CRC every PNG in the world ends with.
        let mut c = Crc32::new();
        c.update(b"IEND");
        assert_eq!(c.finish(), 0xAE42_6082);
    }

    #[test]
    fn crc32_is_order_sensitive() {
        // A negative control against a CRC that accidentally ignores position (e.g. a
        // sum). "ab" and "ba" must differ.
        let mut a = Crc32::new();
        a.update(b"ab");
        let mut b = Crc32::new();
        b.update(b"ba");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn write_chunk_frames_length_type_data_crc() {
        let mut out = Vec::new();
        write_chunk(&mut out, b"IEND", &[]);
        assert_eq!(
            out,
            vec![0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]
        );
    }

    #[test]
    fn zero_dimensions_are_rejected() {
        // RFC 2083 forbids them, and a 0-byte image would otherwise produce a
        // structurally invalid PNG that the round-trip tests could not catch.
        let img = Image {
            width: 0,
            height: 1,
            pixel_format: PixelFormat::Rgb24,
            pixels: Vec::new(),
        };
        let mut out = [0u8; 128];
        assert!(matches!(
            write_to_memory(&img, &mut out),
            Err(Error::Decode)
        ));
    }
}
