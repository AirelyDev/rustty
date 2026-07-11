// PORT NOTE: ghostty src/font/glyf_rasterize_png_test.zig.
//
// The Zig test renders a visual atlas of the bubbletea glyph-protocol example
// outlines, exports it to a PNG via `z2d.png_exporter`, and byte-compares the
// generated PNG against the checked-in reference (`testdata/glyf_rasterize.png`),
// decoding the reference via `wuffs.png.decode` only when a mismatch is found.
//
// PORT NOTE: PNG *export* is not part of the ported z2d subset, so this port
// cannot reproduce the encoded-byte comparison. Instead — as the task
// prescribes — it decodes the checked-in reference PNG with the pure-Rust `png`
// dev-dependency (replacing `wuffs.png.decode`) and asserts the rendered alpha8
// atlas is PIXEL-EXACT against it. The reference is an 8-bit grayscale PNG whose
// per-pixel gray value is exactly the alpha8 coverage the atlas stores, so this
// is the same known-answer contract the Zig test enforces.

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

/// Decode a base64-encoded (standard alphabet, RFC 4648) glyf protocol payload
/// into an owned outline.
///
/// The payload is a complete simple-glyph `glyf` table entry. The returned
/// outline owns decoded point and contour storage and must be deinitialized by
/// the caller.
//
// PORT NOTE: `Outline` owns `Vec`s freed on drop, so there is no explicit
// deinit. The Zig helper uses `std.base64.standard.Decoder`; here we inline a
// small standard-alphabet decoder so the test pulls in no extra dependency
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

// PORT NOTE (REPLACED — the Zig `diffAtlas` helper): the reference compares the
// *encoded* generated PNG bytes to the checked-in reference and only decodes on
// mismatch to emit a diff image. PNG *export* is not part of the ported z2d
// subset, so that byte-compare + diff-image path is replaced by a direct decode
// of the reference and a pixel-exact assert (`decode_reference_png` +
// `bubbletea_glyph_protocol_examples_match_reference_image`). The reference
// `diffAtlas` doc, verbatim, for provenance:
//
//   /// Compare a generated atlas PNG against the checked-in reference image.
//   ///
//   /// On missing reference or mismatch, copy the generated PNG into the workspace
//   /// as `glyf_rasterize_test.png`. On pixel mismatch, also write
//   /// `glyf_rasterize_diff.png`, where red is reference-only coverage and green is
//   /// newly generated coverage. Returns true when a difference was found.

/// Decode the checked-in reference PNG to its 8-bit grayscale byte plane.
fn decode_reference_png() -> (u32, u32, Vec<u8>) {
    const REF: &[u8] = include_bytes!("../../res/glyf_rasterize.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(REF));
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

    // Decode the reference and assert a PIXEL-EXACT match against the atlas.
    let (ref_w, ref_h, ref_gray) = decode_reference_png();
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
}
