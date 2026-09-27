//! WS12 T7 — `rustty-zigimg` codec KATs: PNG encode, decode, and their round trip.
//!
//! ALL tests here are `#[cfg_attr(miri, ignore)]` because every one of them crosses into
//! C: encode calls the vendored zlib (`deflateInit2_`/`deflate`/`deflateEnd`) and decode
//! calls the vendored Wuffs PNG/JPEG decoders. Miri fails only when a foreign function is
//! actually *called*, so this is the narrowest possible exception — the crate's non-FFI
//! logic (magic-byte sniff, the rgba32<->rgb24 converter, PNG chunk framing, the CRC-32
//! table) is covered by `#[cfg(test)]` unit tests inside `src/` that stay Miri-live.
//! Recorded in `.port/phase-2/MIRI_EXCEPTIONS.md`.
//!
//! Falsifiability: the round-trip tests are guarded by `png_round_trip_detects_corruption`
//! below, which flips one byte inside the IDAT payload and asserts the decode does NOT
//! reproduce the original pixels. Without it, "decode(encode(x)) == x" could be comparing
//! a value to itself through a decoder that ignores its input.

use allocator_api2::alloc::Global;
use rustty_zigimg::{Error, Image, PixelFormat};

/// A deterministic gradient with per-channel variation, so a channel swap or a stride
/// error shows up rather than cancelling out.
fn gradient(width: usize, height: usize, format: PixelFormat) -> Image {
    let bpp = format.bytes_per_pixel();
    let mut px = Vec::with_capacity(width * height * bpp);
    for y in 0..height {
        for x in 0..width {
            px.push((x * 40 + 5) as u8); // R
            px.push((y * 70 + 11) as u8); // G
            px.push((x * 13 + y * 29 + 3) as u8); // B
            if bpp == 4 {
                px.push(0xFF); // A — opaque, so the rgb24/rgba32 arms are comparable
            }
        }
    }
    Image::from_raw_pixels(width, height, px, format).unwrap()
}

/// Encode into a generously sized buffer and return the PNG bytes.
fn encode(img: &Image) -> Vec<u8> {
    // Deliberately NOT `imageByteSize()`: that is the (faithfully preserved) upstream
    // buffer size that can legitimately overflow, and it is exercised on purpose by
    // `write_to_memory_no_space_left`. Here we want the encoder's output, not its
    // overflow behaviour.
    let mut buf = vec![0u8; img.image_byte_size() * 4 + 1024];
    let out = img.write_to_memory(&mut buf).expect("encode must succeed");
    out.to_vec()
}

#[test]
#[cfg_attr(miri, ignore = "calls the vendored zlib + Wuffs via FFI")]
fn png_encode_decode_round_trip_rgba32() {
    let img = gradient(3, 2, PixelFormat::Rgba32);
    let png = encode(&img);

    // Structural pins: signature and the IEND terminator.
    assert_eq!(&png[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");

    let decoded = rustty_wuffs::png::decode(Global, &png).expect("wuffs must decode our PNG");
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);
    // wuffs emits RGBA_NONPREMUL == zigimg rgba32, so this is a BIT-EXACT comparison.
    assert_eq!(decoded.bytes(), img.raw_bytes());
}

#[test]
#[cfg_attr(miri, ignore = "calls the vendored zlib + Wuffs via FFI")]
fn png_encode_decode_round_trip_rgb24() {
    let img = gradient(3, 2, PixelFormat::Rgb24);
    let png = encode(&img);

    let decoded = rustty_wuffs::png::decode(Global, &png).expect("wuffs must decode our PNG");
    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);

    // wuffs always outputs RGBA; our source was RGB, so compare against the source with
    // alpha = 255 inserted (colour type 2 has no alpha, so a decoder must synthesise
    // opaque).
    let mut expected = Vec::new();
    for px in img.raw_bytes().chunks_exact(3) {
        expected.extend_from_slice(&[px[0], px[1], px[2], 0xFF]);
    }
    assert_eq!(decoded.bytes(), &expected[..]);
}

/// THE NEGATIVE CONTROL for both round trips above.
#[test]
#[cfg_attr(miri, ignore = "calls the vendored zlib + Wuffs via FFI")]
fn png_round_trip_detects_corruption() {
    let img = gradient(8, 8, PixelFormat::Rgba32);
    let png = encode(&img);

    // Locate the IDAT payload and flip a byte inside it. (Chunk layout: 4-byte length,
    // 4-byte type, payload, 4-byte CRC.)
    let idat_type = png
        .windows(4)
        .position(|w| w == b"IDAT")
        .expect("the encoder must emit an IDAT chunk");
    let len = u32::from_be_bytes([
        png[idat_type - 4],
        png[idat_type - 3],
        png[idat_type - 2],
        png[idat_type - 1],
    ]) as usize;
    assert!(len > 8, "IDAT payload must be non-trivial, got {len}");
    let payload_start = idat_type + 4;

    let mut corrupt = png.clone();
    let target = payload_start + len / 2;
    corrupt[target] ^= 0xFF;

    // A corrupted IDAT must either fail to decode (bad zlib/adler, or the chunk CRC) or
    // decode to different pixels. What it must NOT do is reproduce the original — that
    // would mean the decode is not actually reading the payload.
    match rustty_wuffs::png::decode(Global, &corrupt) {
        Err(_) => {}
        Ok(decoded) => assert_ne!(
            decoded.bytes(),
            img.raw_bytes(),
            "a corrupted IDAT decoded back to the ORIGINAL pixels — the round-trip tests \
             above are therefore vacuous"
        ),
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls the vendored zlib via FFI")]
fn write_to_memory_no_space_left() {
    let img = gradient(4, 4, PixelFormat::Rgba32);
    let mut tiny = [0u8; 1];
    assert!(matches!(
        img.write_to_memory(&mut tiny),
        Err(Error::NoSpaceLeft)
    ));

    // A buffer one byte short of the real encoding must also fail — proves the check is
    // a real length comparison, not just "buffer is absurdly small".
    let exact = encode(&img);
    let mut short = vec![0u8; exact.len() - 1];
    assert!(matches!(
        img.write_to_memory(&mut short),
        Err(Error::NoSpaceLeft)
    ));
    // ...and an exactly-sized buffer must succeed, so the boundary is off-by-none.
    let mut fits = vec![0u8; exact.len()];
    assert_eq!(img.write_to_memory(&mut fits).unwrap().len(), exact.len());
}

#[test]
fn from_memory_rejects_unknown_magic() {
    // No FFI reached (the sniff refuses first), so this one is Miri-live.
    assert!(matches!(
        rustty_zigimg::from_memory(b"NOTANIMAGE"),
        Err(Error::UnsupportedFormat)
    ));
}

#[test]
#[cfg_attr(miri, ignore = "calls the vendored zlib + Wuffs via FFI")]
fn from_memory_decodes_our_own_png_to_rgba32() {
    let img = gradient(5, 3, PixelFormat::Rgba32);
    let png = encode(&img);

    let decoded = rustty_zigimg::from_memory(&png).expect("decode must succeed");
    assert_eq!(decoded.width, 5);
    assert_eq!(decoded.height, 3);
    assert_eq!(decoded.pixel_format(), PixelFormat::Rgba32);
    assert_eq!(decoded.raw_bytes(), img.raw_bytes());
}

#[test]
#[cfg_attr(miri, ignore = "calls the vendored zlib + Wuffs via FFI")]
fn from_memory_rejects_a_truncated_png() {
    let img = gradient(5, 3, PixelFormat::Rgba32);
    let png = encode(&img);
    // Keep the signature (so the sniff accepts) but cut the stream in half.
    assert!(rustty_zigimg::from_memory(&png[..png.len() / 2]).is_err());
}

#[test]
#[cfg_attr(
    miri,
    ignore = "reads a file and calls the vendored zlib + Wuffs via FFI"
)]
fn from_file_path_matches_from_memory() {
    let img = gradient(4, 2, PixelFormat::Rgba32);
    let png = encode(&img);

    let path = std::env::temp_dir().join(format!(
        "rustty-zigimg-kat-{}-{}.png",
        std::process::id(),
        line!()
    ));
    std::fs::write(&path, &png).unwrap();

    let mut read_buffer = vec![0u8; 1024 * 1024];
    let from_path = {
        rustty_zigimg::from_file_path(path.as_os_str().as_encoded_bytes(), &mut read_buffer)
            .expect("from_file_path must succeed")
    };
    let from_mem = rustty_zigimg::from_memory(&png).unwrap();

    let _ = std::fs::remove_file(&path);

    assert_eq!(from_path, from_mem);
    assert_eq!(from_path.raw_bytes(), img.raw_bytes());
}

#[test]
#[cfg_attr(miri, ignore = "reads a file")]
fn from_file_path_refuses_a_file_larger_than_the_read_buffer() {
    // PORT NOTE pin: `read_buffer` is the reference's bounded stream buffer
    // (`Vaxis.zig:1024` sizes it at 1 MiB), so it CAPS the loadable file size. A port that
    // silently heap-grew instead would change which inputs succeed.
    let img = gradient(4, 2, PixelFormat::Rgba32);
    let png = encode(&img);

    let path = std::env::temp_dir().join(format!(
        "rustty-zigimg-kat-{}-{}.png",
        std::process::id(),
        line!()
    ));
    std::fs::write(&path, &png).unwrap();

    let mut tiny = vec![0u8; 4];
    let r = { rustty_zigimg::from_file_path(path.as_os_str().as_encoded_bytes(), &mut tiny) };
    let _ = std::fs::remove_file(&path);
    assert!(matches!(r, Err(Error::NoSpaceLeft)));
}

#[test]
#[cfg_attr(miri, ignore = "reads a file")]
fn from_file_path_surfaces_a_missing_file_as_io() {
    let mut buf = vec![0u8; 64];
    let r =
        rustty_zigimg::from_file_path(b"/nonexistent/rustty-zigimg/definitely-not-here", &mut buf);
    assert!(matches!(r, Err(Error::Io(_))));
}

#[test]
fn convert_rgba32_to_rgb24_drops_to_the_exact_expected_bytes() {
    // Miri-live: pure arithmetic, no FFI. Note the TRANSLUCENT pixel — the reference
    // premultiplies alpha rather than discarding it (see src/convert.rs), and only a
    // non-opaque case distinguishes the two.
    let mut img = Image::from_raw_pixels(
        2,
        1,
        vec![10, 20, 30, 255, 200, 100, 50, 64],
        PixelFormat::Rgba32,
    )
    .unwrap();
    img.convert(PixelFormat::Rgb24);
    assert_eq!(img.pixel_format(), PixelFormat::Rgb24);
    assert_eq!(img.raw_bytes(), &[10, 20, 30, 50, 25, 13]);
    assert_eq!(img.image_byte_size(), 6);
}

#[test]
fn convert_rgb24_to_rgba32_inserts_opaque_alpha() {
    let mut img =
        Image::from_raw_pixels(2, 1, vec![10, 20, 30, 40, 50, 60], PixelFormat::Rgb24).unwrap();
    img.convert(PixelFormat::Rgba32);
    assert_eq!(img.pixel_format(), PixelFormat::Rgba32);
    assert_eq!(img.raw_bytes(), &[10, 20, 30, 0xFF, 40, 50, 60, 0xFF]);
}
