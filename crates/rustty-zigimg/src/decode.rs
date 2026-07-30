//! Format sniff + decode — `zigimg` operations 1 and 2, re-homed onto `rustty-wuffs`.
//!
//! REFERENCE: `zigimg` `Image.zig:55` (`fromFilePath`) and `:65` (`fromMemory`), which
//! forward to `ImageUnmanaged.zig:162`/`:176`; both wrap the bytes in an `io.ReadStream`
//! and call `internalRead`, which walks every registered `FormatInterface` asking each
//! `formatDetect` whether it claims the stream, then runs that format's reader.
//!
//! Re-homed shape: sniff the magic bytes directly and hand the buffer to ghostty's own
//! decoder. `rustty_wuffs::{png,jpeg}::decode` return
//! `ImageData { width: u32, height: u32, data: RawBuf }` whose bytes are
//! **RGBA_NONPREMUL** — which *is* `zigimg`'s `rgba32` (straight alpha, R,G,B,A order,
//! one byte each). No conversion is needed or performed on the decode path; that
//! coincidence is what makes Option Z-C exact rather than approximate.
//!
//! THE DIVERGENCE, once more so it is impossible to miss: `zigimg` sniffs 15 formats,
//! this sniffs 2 (PNG, JPEG). See the crate header. It is a superset of what ghostty
//! itself decodes anywhere.

use allocator_api2::alloc::Global;

use crate::{Error, Image, PixelFormat};

/// PNG signature — `89 50 4E 47 0D 0A 1A 0A` (RFC 2083 §3.1). `zigimg`'s
/// `formats/png.zig` `formatDetect` compares the same eight bytes.
const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// JPEG SOI marker plus the first byte of the next marker — `FF D8 FF`. `zigimg`'s
/// `formats/jpeg.zig` `formatDetect` reads the `FFD8` SOI.
const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF];

/// The formats this crate decodes. Not `pub` as an enum in its own right — `libvaxis`
/// never names `zigimg.Image.Format`; it only ever asks for a decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Detected {
    Png,
    Jpeg,
}

/// `zigimg` `ImageUnmanaged.zig:157` — `detectFormatFromMemory`.
fn detect(bytes: &[u8]) -> Result<Detected, Error> {
    if bytes.starts_with(PNG_MAGIC) {
        Ok(Detected::Png)
    } else if bytes.starts_with(JPEG_MAGIC) {
        Ok(Detected::Jpeg)
    } else {
        Err(Error::UnsupportedFormat)
    }
}

/// `zigimg` `Image.zig:65` — `pub fn fromMemory(allocator, buffer) !Image`.
///
/// Operation 2. Reached from `Vaxis.zig:1027` (`.mem => |bytes| try
/// zigimg.Image.fromMemory(alloc, bytes)`).
pub fn from_memory(bytes: &[u8]) -> Result<Image, Error> {
    let detected = detect(bytes)?;
    let data = match detected {
        Detected::Png => rustty_wuffs::png::decode(Global, bytes).map_err(|_| Error::Decode)?,
        Detected::Jpeg => rustty_wuffs::jpeg::decode(Global, bytes).map_err(|_| Error::Decode)?,
    };

    let width = data.width as usize;
    let height = data.height as usize;
    // wuffs' output is RGBA_NONPREMUL == zigimg's rgba32; copy it into an owned Vec so
    // the `Image` is allocator-independent (the reference's `Image` owns its
    // `PixelStorage` via its own allocator; ours owns a `Vec`).
    let pixels = data.bytes().to_vec();
    Image::from_raw_pixels(width, height, pixels, PixelFormat::Rgba32)
}

/// `zigimg` `Image.zig:55` — `pub fn fromFilePath(allocator, file_path, read_buffer) !Image`.
///
/// Operation 1. Reached from `Vaxis.zig:1026`
/// (`.path => |path| try zigimg.Image.fromFilePath(alloc, path, &read_buffer)`).
///
/// PORT NOTE — the `read_buffer` parameter is PRESERVED, not dropped. Upstream threads
/// the caller's buffer into `io.ReadStream.initFile` as the stream's scratch/read window,
/// and `Vaxis.zig:1024` sizes it at exactly 1 MiB. That size is **observable**: it caps
/// what `load_image` can read off disk. Here it plays the same capping role — the file is
/// read into `read_buffer` and a file larger than the buffer is refused
/// (`Error::NoSpaceLeft`) rather than silently heap-growing to any size, which would
/// change what inputs succeed.
///
/// `path` is `&[u8]`, matching the Zig `[]const u8`; it is never round-tripped through a
/// lossy `String` (memory: zig-byteslice-to-rust-rawbytes).
pub fn from_file_path(path: &[u8], read_buffer: &mut [u8]) -> Result<Image, Error> {
    use std::io::Read;

    // A Zig []const u8 path is raw bytes. On Unix, OsStr is those bytes exactly.
    #[cfg(unix)]
    let os_path: std::path::PathBuf = {
        use std::os::unix::ffi::OsStrExt;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(path))
    };
    // Off Unix there is no byte-exact OsStr; fall back to a UTF-8 interpretation and
    // refuse non-UTF-8 rather than lossily mangling it.
    #[cfg(not(unix))]
    let os_path: std::path::PathBuf = {
        let s = std::str::from_utf8(path).map_err(|_| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "non-UTF-8 path on a non-Unix target",
            ))
        })?;
        std::path::PathBuf::from(s)
    };

    let mut file = std::fs::File::open(&os_path)?;

    // Fill read_buffer; a file that does not fit is refused, mirroring the bounded
    // stream buffer upstream.
    let mut filled = 0usize;
    loop {
        if filled == read_buffer.len() {
            // Is there more? If so the file exceeds the caller's buffer.
            let mut probe = [0u8; 1];
            match file.read(&mut probe) {
                Ok(0) => break,
                Ok(_) => return Err(Error::NoSpaceLeft),
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Error::Io(e)),
            }
        }
        match file.read(&mut read_buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Error::Io(e)),
        }
    }

    from_memory(&read_buffer[..filled])
}

#[cfg(test)]
mod tests {
    use super::*;

    // The sniff itself is pure logic, so these stay Miri-live (no FFI, no file I/O).

    #[test]
    fn detect_recognizes_png_and_jpeg_magic() {
        let mut png = PNG_MAGIC.to_vec();
        png.extend_from_slice(b"...rest of the stream...");
        assert_eq!(detect(&png).unwrap(), Detected::Png);

        let mut jpeg = JPEG_MAGIC.to_vec();
        jpeg.extend_from_slice(b"\xE0\x00\x10JFIF");
        assert_eq!(detect(&jpeg).unwrap(), Detected::Jpeg);
    }

    #[test]
    fn detect_rejects_unknown_and_truncated_magic() {
        assert!(matches!(
            detect(b"NOTANIMAGE"),
            Err(Error::UnsupportedFormat)
        ));
        assert!(matches!(detect(b""), Err(Error::UnsupportedFormat)));
        // A truncated PNG signature must NOT match — `starts_with` on a prefix of the
        // magic is the classic off-by-one here.
        assert!(matches!(
            detect(&PNG_MAGIC[..7]),
            Err(Error::UnsupportedFormat)
        ));
        // One wrong byte inside the signature must also be refused.
        let mut bad = PNG_MAGIC.to_vec();
        bad[3] = 0x00;
        assert!(matches!(detect(&bad), Err(Error::UnsupportedFormat)));
    }

    #[test]
    fn from_memory_rejects_unknown_magic_without_calling_a_codec() {
        // Stays Miri-live precisely because it never reaches the FFI.
        assert!(matches!(
            from_memory(b"NOTANIMAGE"),
            Err(Error::UnsupportedFormat)
        ));
    }
}
