// PORT NOTE: ghostty src/font/glyf_rasterize_png_test.zig.
//
// The Zig test renders a visual atlas of the bubbletea glyph-protocol example
// outlines, exports it to a PNG via `z2d.png_exporter`, and byte-compares the
// generated PNG against the checked-in reference (`testdata/glyf_rasterize.png`),
// decoding the reference via `wuffs.png.decode` only when a mismatch is found.
//
// PORT NOTE: PNG *export* is not part of the ported z2d subset; the pure-Rust
// `png` dev-dependency stands in for it on both sides — `z2d.png_exporter` for
// encoding (see `encode_alpha8_png`) and `wuffs.png.decode` for decoding — so
// the port reproduces the Zig test's full contract: encode the rendered alpha8
// atlas to PNG bytes, byte-compare them against the checked-in reference FIRST,
// and only decode + pixel-diff on mismatch, including the explicit
// bytes-differ-but-pixels-match failure arm. The reference is an 8-bit
// grayscale PNG whose per-pixel gray value is exactly the alpha8 coverage the
// atlas stores, re-encoded with this file's pinned `png` encoder settings so
// byte equality holds.

use super::test_metrics;
use crate::glyf_rasterize::{rasterize, Bitmap};
use crate::glyph::{self, Align, DesignMetrics, RenderOptions};
use crate::opentype::glyf;

use rustty_z2d as z2d;
use z2d::pixel::Alpha8;
use z2d::surface::{Surface, SurfaceType};

const TEST_GLYF_PAYLOADS: [&str; 5] = [
    // Nerd Font branch, folder, home, heart, and Rust cog outlines from:
    // https://github.com/raphamorim/glyph-protocol-examples/blob/main/bubbletea/main.go
    "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",
];

/// Decode a base64-encoded glyf protocol payload into an owned outline.
///
/// The payload is a complete simple-glyph `glyf` table entry. The returned
/// outline owns decoded point and contour storage and must be deinitialized by
/// the caller.
//
// PORT NOTE: `Outline` owns `Vec`s freed on drop, so there is no explicit
// deinit. The Zig helper uses `std.base64.standard.Decoder`; here we inline a
// small standard-alphabet (RFC 4648) decoder so the test pulls in no extra dependency
// beyond the `png` decoder used for the reference image.
fn decode_glyf_payload(payload: &str) -> glyf::Outline {
    let data = base64_decode_standard(payload);
    let entry = glyf::Entry::init(&data).unwrap();
    entry.decode().unwrap()
}

fn base64_decode_standard(s: &str) -> Vec<u8> {
    fn val(c: u8) -> u32 {
        match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => unreachable!("invalid base64 character"),
        }
    }

    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut chunk: [u8; 4] = [0; 4];
    let mut chunk_len = 0usize;
    for &c in bytes {
        if c == b'=' {
            break;
        }
        chunk[chunk_len] = c;
        chunk_len += 1;
        if chunk_len == 4 {
            let n = (val(chunk[0]) << 18)
                | (val(chunk[1]) << 12)
                | (val(chunk[2]) << 6)
                | val(chunk[3]);
            out.push((n >> 16) as u8);
            out.push((n >> 8) as u8);
            out.push(n as u8);
            chunk_len = 0;
        }
    }
    match chunk_len {
        0 => {}
        2 => {
            let n = (val(chunk[0]) << 18) | (val(chunk[1]) << 12);
            out.push((n >> 16) as u8);
        }
        3 => {
            let n = (val(chunk[0]) << 18) | (val(chunk[1]) << 12) | (val(chunk[2]) << 6);
            out.push((n >> 16) as u8);
            out.push((n >> 8) as u8);
        }
        _ => unreachable!("invalid base64 length"),
    }
    out
}

/// Copy a tightly packed alpha bitmap into the alpha atlas at `dst_x`, `dst_y`.
///
/// The destination rectangle must fit inside the atlas. This is a test helper,
/// so it trusts the hardcoded atlas layout rather than clipping.
fn blit_bitmap(atlas: &mut Surface, bm: &Bitmap, dst_x: usize, dst_y: usize) {
    let dst_width: usize = atlas.get_width() as usize;
    let dst = atlas.alpha8_buf_mut();
    for y in 0..bm.height as usize {
        let src_start = y * bm.width as usize;
        let dst_start = (dst_y + y) * dst_width + dst_x;
        for k in 0..bm.width as usize {
            dst[dst_start + k] = Alpha8 {
                a: bm.data[src_start + k],
            };
        }
    }
}

/// Draw faint terminal-cell outlines into one row of the alpha atlas.
///
/// The boxes make cell advance and placement behavior visible in the reference
/// PNG without overpowering the rendered glyph coverage.
fn draw_cell_boxes(atlas: &mut Surface, y: usize, cell_width: usize, cell_height: usize) {
    let width: usize = atlas.get_width() as usize;
    let dst = atlas.alpha8_buf_mut();
    let alpha: u8 = 64;

    let mut x: usize = 0;
    while x < width {
        let right = (x + cell_width - 1).min(width - 1);
        let bottom = y + cell_height - 1;

        for px in x..right + 1 {
            dst[y * width + px].a = dst[y * width + px].a.max(alpha);
            dst[bottom * width + px].a = dst[bottom * width + px].a.max(alpha);
        }
        for py in y..bottom + 1 {
            dst[py * width + x].a = dst[py * width + x].a.max(alpha);
            dst[py * width + right].a = dst[py * width + right].a.max(alpha);
        }

        x += cell_width;
    }
}

// PORT NOTE (the Zig `diffAtlas` helper): the byte-compare + decode-on-mismatch
// flow, including the explicit bytes-differ-but-pixels-match failure arm, is
// ported below (`diff_encoded_atlas` plus the failure arms in the test). The
// workspace copy of the generated PNG (`glyf_rasterize_test.png`) and the
// red/green `glyf_rasterize_diff.png` diff image are debugging aids with no
// analogue and are dropped; regeneration is instead performed in place via the
// `RUSTTY_REGEN_FONT_KAT_PNGS` env var. The reference `diffAtlas` doc,
// verbatim, for provenance:
//
//   /// Compare a generated atlas PNG against the checked-in reference image.
//   ///
//   /// On missing reference or mismatch, copy the generated PNG into the workspace
//   /// as `glyf_rasterize_test.png`. On pixel mismatch, also write
//   /// `glyf_rasterize_diff.png`, where red is reference-only coverage and green is
//   /// newly generated coverage. Returns true when a difference was found.

// const ref_path = "src/font/testdata/glyf_rasterize.png";
const REF_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/res/glyf_rasterize.png");

/// Encode the alpha8 atlas to PNG bytes: 8-bit grayscale, one gray byte per
/// pixel equal to the alpha8 coverage — the same shape the checked-in
/// reference uses.
//
// PORT NOTE: stands in for the reference's `z2d.png_exporter`; a deliberate
// mirror of the sprite `Face` KAT helper (ghostty likewise carries a separate
// `diffAtlas` per test family). The compression and filter settings are pinned
// explicitly because the checked-in reference bytes are coupled to them:
// changing them (or picking up a `png` crate release that changes encoder
// output) is a reference-regeneration event (`RUSTTY_REGEN_FONT_KAT_PNGS`),
// exactly as the reference treats z2d exporter changes.
fn encode_alpha8_png(atlas: &Surface) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut encoder = png::Encoder::new(
        &mut out,
        atlas.get_width() as u32,
        atlas.get_height() as u32,
    );
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Balanced);
    encoder.set_filter(png::Filter::Adaptive);
    let mut writer = encoder.write_header().unwrap();
    let gray: Vec<u8> = atlas.alpha8_buf().iter().map(|px| px.a).collect();
    writer.write_image_data(&gray).unwrap();
    writer.finish().unwrap();
    out
}

/// Decode the checked-in reference PNG to its 8-bit grayscale byte plane.
fn decode_reference_png(ref_bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(ref_bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(
        info.color_type,
        png::ColorType::Grayscale,
        "reference PNG must be 8-bit grayscale (one gray byte == one alpha8 coverage)"
    );
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

/// Outcome of comparing the encoded generated atlas against the reference PNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtlasDiff {
    /// Encoded PNG bytes are identical; nothing to decode.
    BytesMatch,
    /// Encoded bytes differ but every decoded pixel matches: possible binary
    /// smuggling in the reference file (or encoder drift). An explicit
    /// failure in the reference, and here.
    BytesDifferPixelsMatch,
    /// One or more decoded pixels differ from the rendered atlas.
    PixelsDiffer,
}

/// Comparison core of the KAT, split out so the byte-integrity contract itself
/// has direct unit coverage (see `diff_encoded_atlas_flags_byte_smuggle`).
fn diff_encoded_atlas(atlas: &Surface, generated_bytes: &[u8], ref_bytes: &[u8]) -> AtlasDiff {
    // if (std.mem.eql(u8, generated_bytes, ref_bytes)) return false;
    if generated_bytes == ref_bytes {
        return AtlasDiff::BytesMatch;
    }

    let (ref_w, ref_h, ref_gray) = decode_reference_png(ref_bytes);
    // if (ref_rgba.width != atlas.getWidth() or ref_rgba.height != atlas.getHeight()) { log; return true; }
    assert_eq!(
        ref_w as i32,
        atlas.get_width(),
        "reference width differs from generated atlas"
    );
    assert_eq!(
        ref_h as i32,
        atlas.get_height(),
        "reference height differs from generated atlas"
    );

    let atlas_buf = atlas.alpha8_buf();
    assert_eq!(atlas_buf.len(), ref_gray.len());
    let differs = atlas_buf
        .iter()
        .zip(ref_gray.iter())
        .any(|(px, r)| px.a != *r);

    if differs {
        AtlasDiff::PixelsDiffer
    } else {
        AtlasDiff::BytesDifferPixelsMatch
    }
}

#[test]
fn bubbletea_glyph_protocol_examples_match_reference_image() {
    // The generated atlas is a visual atlas for reading placement behavior.
    // Each column below is one terminal cell. The five payloads are rendered in
    // order: branch, folder, home, heart, rust.
    //
    // ```text
    // columns: 0 1 2 3 4 5 6 7 8 9
    //          ┌─┬─┬─┬─┬─┬─┬─┬─┬─┬─┐
    // row 0    │B│F│H│♥│R│ │ │ │ │ │  narrow/default: one-cell bitmap stride
    //          ├─┼─┼─┼─┼─┼─┼─┼─┼─┼─┤
    // row 1    │B  │F  │H  │♥  │R  │  width=2: same glyphs, two-cell bitmaps
    //          ├─┼─┼─┼─┼─┼─┼─┼─┼─┼─┤
    // row 2    │  B│  F│  H│  ♥│  R│  width=2 + horizontal center alignment
    //          ├─┼─┼─┼─┼─┼─┼─┼─┼─┼─┤
    // row 3    │B  │F  │H  │♥  │R  │  advance_width=2000: wider design box
    //          └─┴─┴─┴─┴─┴─┴─┴─┴─┴─┘
    // ```
    //
    // The faint grid lines in the PNG are these cell boundaries. Rows 2 and 3
    // are the design-metric/placement regression checks: row 2 centers a
    // one-cell-wide design box inside two cells, while row 3 centers a two-cell
    // design box so the visible glyph returns to the start of each span.
    let cell_width: u32 = 20;
    let cell_height: u32 = 20;
    let columns = TEST_GLYF_PAYLOADS.len() as u32;
    let narrow_stride_x = cell_width;
    let wide_stride_x = cell_width * 2;
    let row_count: u32 = 4;

    let mut atlas = Surface::init(
        SurfaceType::ImageSurfaceAlpha8,
        (wide_stride_x * columns) as i32,
        (cell_height * row_count) as i32,
    )
    .unwrap();

    for (i, payload) in TEST_GLYF_PAYLOADS.iter().enumerate() {
        let i = i as u32;
        let outline = decode_glyf_payload(payload);

        let narrow = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            RenderOptions {
                grid_metrics: test_metrics(cell_width, cell_height),
                ..RenderOptions::default()
            },
        )
        .unwrap();
        blit_bitmap(&mut atlas, &narrow, (i * narrow_stride_x) as usize, 0);

        let wide = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            RenderOptions {
                grid_metrics: test_metrics(cell_width, cell_height),
                cell_width: Some(2),
                ..RenderOptions::default()
            },
        )
        .unwrap();
        blit_bitmap(
            &mut atlas,
            &wide,
            (i * wide_stride_x) as usize,
            cell_height as usize,
        );

        let centered = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            RenderOptions {
                grid_metrics: test_metrics(cell_width, cell_height),
                cell_width: Some(2),
                constraint_width: 2,
                constraint: glyph::Constraint {
                    align_horizontal: Align::Center,
                    ..glyph::Constraint::none
                },
                ..RenderOptions::default()
            },
        )
        .unwrap();
        blit_bitmap(
            &mut atlas,
            &centered,
            (i * wide_stride_x) as usize,
            (cell_height * 2) as usize,
        );

        let designed_wide = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 2000,
                line_height: 1000,
            },
            RenderOptions {
                grid_metrics: test_metrics(cell_width, cell_height),
                cell_width: Some(2),
                constraint_width: 2,
                constraint: glyph::Constraint {
                    align_horizontal: Align::Center,
                    ..glyph::Constraint::none
                },
                ..RenderOptions::default()
            },
        )
        .unwrap();
        blit_bitmap(
            &mut atlas,
            &designed_wide,
            (i * wide_stride_x) as usize,
            (cell_height * 3) as usize,
        );
    }

    for row in 0..row_count {
        draw_cell_boxes(
            &mut atlas,
            (row * cell_height) as usize,
            cell_width as usize,
            cell_height as usize,
        );
    }

    // Encode the generated atlas and byte-compare against the checked-in
    // reference before any decoding — the binary-smuggling integrity gate —
    // then decode + pixel-diff only on mismatch.
    let generated_bytes = encode_alpha8_png(&atlas);
    let ref_bytes = std::fs::read(REF_PATH)
        .unwrap_or_else(|err| panic!("Can't open reference file {REF_PATH}: {err}"));

    let outcome = diff_encoded_atlas(&atlas, &generated_bytes, &ref_bytes);

    // Regeneration arm (no reference analogue): with
    // `RUSTTY_REGEN_FONT_KAT_PNGS` set, rewrite the checked-in reference from
    // the freshly rendered atlas — but only when the decoded pixels are exact,
    // so a regeneration can never smuggle a visual change past the KAT (see
    // the sprite `Face` KAT for the same arm).
    if std::env::var_os("RUSTTY_REGEN_FONT_KAT_PNGS").is_some()
        && outcome == AtlasDiff::BytesDifferPixelsMatch
    {
        std::fs::write(REF_PATH, &generated_bytes).unwrap();
        eprintln!("info(glyf_rasterize): regenerated reference file {REF_PATH}");
        return;
    }
    // BytesMatch needs no rewrite; PixelsDiffer falls through to the
    // failure arm below — regeneration never overwrites a visual change.

    match outcome {
        AtlasDiff::BytesMatch => {}
        // log.err("generated glyf rasterize PNG bytes differ from reference but pixels match; ...")
        AtlasDiff::BytesDifferPixelsMatch => panic!(
            "generated glyf rasterize PNG bytes differ from reference but pixels match; \
             either the `png` crate's encoder changed or binary data is being smuggled \
             in the reference! reference={REF_PATH}"
        ),
        AtlasDiff::PixelsDiffer => {
            // Decode again for a rich per-pixel failure message (this stands
            // in for the reference's red/green diff-image emission).
            let (ref_w, _ref_h, ref_gray) = decode_reference_png(&ref_bytes);
            let atlas_buf = atlas.alpha8_buf();
            for (i, (px, r)) in atlas_buf.iter().zip(ref_gray.iter()).enumerate() {
                assert_eq!(
                    px.a,
                    *r,
                    "glyf rasterize atlas differs from reference at pixel {} (x={}, y={}): got {}, want {}",
                    i,
                    i % ref_w as usize,
                    i / ref_w as usize,
                    px.a,
                    *r
                );
            }
            unreachable!(
                "diff_encoded_atlas reported a pixel diff but the per-pixel asserts all passed"
            );
        }
    }
}

/// W9-02 proving test: a reference re-encoded with an extra (valid) tEXt chunk
/// decodes to identical pixels but has different encoded bytes. The reference
/// explicitly fails this arm (`glyf_rasterize_png_test.zig` `diffAtlas`:
/// "bytes differ from reference but pixels match"); before the byte-compare
/// gate was ported, the KAT silently passed such a reference.
#[test]
fn diff_encoded_atlas_flags_byte_smuggle() {
    let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 8, 8).unwrap();
    for (j, px) in sfc.alpha8_buf_mut().iter_mut().enumerate() {
        px.a = (j * 11 % 251) as u8;
    }
    let generated_bytes = encode_alpha8_png(&sfc);

    // Same pixels, plus a smuggled tEXt chunk => different encoded bytes.
    let mut smuggled: Vec<u8> = Vec::new();
    let mut encoder = png::Encoder::new(&mut smuggled, 8, 8);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Balanced);
    encoder.set_filter(png::Filter::Adaptive);
    encoder
        .add_text_chunk("Comment".to_string(), "smuggled".to_string())
        .unwrap();
    let mut writer = encoder.write_header().unwrap();
    let gray: Vec<u8> = sfc.alpha8_buf().iter().map(|px| px.a).collect();
    writer.write_image_data(&gray).unwrap();
    writer.finish().unwrap();

    assert!(generated_bytes != smuggled);
    assert_eq!(
        diff_encoded_atlas(&sfc, &generated_bytes, &smuggled),
        AtlasDiff::BytesDifferPixelsMatch
    );

    // Control: byte-identical passes, a pixel edit is a pixel diff.
    assert_eq!(
        diff_encoded_atlas(&sfc, &generated_bytes, &generated_bytes),
        AtlasDiff::BytesMatch
    );
    let mut edited = Surface::init(SurfaceType::ImageSurfaceAlpha8, 8, 8).unwrap();
    for (j, px) in edited.alpha8_buf_mut().iter_mut().enumerate() {
        px.a = (j * 11 % 251) as u8;
    }
    edited.alpha8_buf_mut()[3].a ^= 0xFF;
    let edited_bytes = encode_alpha8_png(&edited);
    assert_eq!(
        diff_encoded_atlas(&sfc, &generated_bytes, &edited_bytes),
        AtlasDiff::PixelsDiffer
    );
}
