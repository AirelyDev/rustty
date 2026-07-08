//! Test-only image fixtures.
//!
//! The reference `pkg/wuffs/src/{1x1#000000,1x1#FFFFFF}.png` + `too_big.png`
//! `@embedFile`s are ABSENT from the ghostty checkout (F5), so the PNG fixtures
//! are GENERATED here (via the pure-Rust `png` encoder dev-dep — the same
//! stand-in rustty-font uses) instead of checked in. JPEG fixtures cannot be
//! generated (no pure-Rust JPEG encoder dep), so those are minimal checked-in
//! `fixtures/*.jpg` files (`include_bytes!`-ed at their use sites).

/// Encode a 1×1 solid-color RGB PNG (8-bit, no alpha) → exact-decodable bytes.
/// The decode path re-expands it to RGBA_NONPREMUL `[r, g, b, 255]`.
pub(crate) fn encode_1x1_png(rgb: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 1, 1);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(&rgb).expect("png data");
        writer.finish().expect("png finish");
    }
    out
}

/// A minimal PNG whose IHDR DECLARES 65536×65536 (⇒ `65536·65536·4 = 16 GiB`,
/// past the 4 GiB `maximum_image_size`). `decode_image_config` reads the IHDR
/// dimensions BEFORE any frame decode, so the overflow guard fires on a
/// few-dozen-byte input (F5 — no multi-MB blob). wuffs' `decode_image_config`
/// reads past IHDR to the first IDAT before it reports the config complete (an
/// IHDR-only stream reads as "truncated input"), so a token IDAT + IEND follow;
/// their content is never decompressed by `decode_image_config`.
pub(crate) fn overflow_png() -> Vec<u8> {
    // PNG signature.
    let mut out: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    // IHDR data: width=65536, height=65536, bit depth 8, color type 6 (RGBA),
    // compression 0, filter 0, interlace 0.
    let mut ihdr: Vec<u8> = Vec::new();
    ihdr.extend_from_slice(b"IHDR");
    ihdr.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // width  = 65536
    ihdr.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // height = 65536
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type: RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    push_chunk(&mut out, &ihdr);

    // A token IDAT (a minimal zlib stream: 0x78 0x01 header + a stored empty
    // deflate block) so `decode_image_config` sees the config is complete and
    // returns OK (it does not decompress IDAT). Then IEND.
    let mut idat: Vec<u8> = Vec::new();
    idat.extend_from_slice(b"IDAT");
    idat.extend_from_slice(&[
        0x78, 0x01, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x01,
    ]);
    push_chunk(&mut out, &idat);

    let mut iend: Vec<u8> = Vec::new();
    iend.extend_from_slice(b"IEND");
    push_chunk(&mut out, &iend);
    out
}

/// Append a PNG chunk: `length(4) | type+data | CRC(4)`. `type_and_data` is the
/// 4-byte type followed by the chunk data; the CRC covers exactly those bytes.
fn push_chunk(out: &mut Vec<u8>, type_and_data: &[u8]) {
    let data_len = (type_and_data.len() - 4) as u32;
    out.extend_from_slice(&data_len.to_be_bytes());
    out.extend_from_slice(type_and_data);
    out.extend_from_slice(&crc32(type_and_data).to_be_bytes());
}

/// CRC-32/IEEE (the PNG chunk CRC) — a tiny table-free implementation for the
/// hand-built `overflow_png` IHDR chunk.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
