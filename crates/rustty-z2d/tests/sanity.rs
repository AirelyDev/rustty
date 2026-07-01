// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2025 rustty contributors
//
// z2d-internal sanity tests (NOT part of the ghostty parity contract; z2d is a
// dependency). These anchor correctness of the ported pixel-producing core:
// building a `Surface`, driving pixels via `put_pixel`, and running the FILL
// scanline rasterizer over a `StaticPath` — then reading back the pixel buffer.

use rustty_z2d::options::{AntiAliasMode, CapMode};
use rustty_z2d::painter::{self, FillOptions, StrokeOptions};
use rustty_z2d::pattern::{OpaquePattern, Pattern};
use rustty_z2d::pixel::{Alpha8, Pixel};
use rustty_z2d::static_path::StaticPath;
use rustty_z2d::surface::{Surface, SurfaceType};

/// Read the alpha value at (x, y) from an alpha8 surface.
fn alpha_at(sfc: &Surface, x: i32, y: i32) -> u8 {
    match sfc.get_pixel(x, y) {
        Some(Pixel::Alpha8(a)) => a.a,
        other => panic!("expected alpha8 pixel, got {other:?}"),
    }
}

#[test]
fn fill_a_rect() {
    // 10x10 alpha8 surface, transparent to start.
    let mut sfc = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 0 }), 10, 10).unwrap();

    // A closed rectangle from (2,2) to (8,8), filled opaque with no AA so the
    // coverage is exactly on/off (the `.none` scanline rasterizer).
    let mut path: StaticPath<6> = StaticPath::new();
    path.init();
    path.move_to(2.0, 2.0);
    path.line_to(8.0, 2.0);
    path.line_to(8.0, 8.0);
    path.line_to(2.0, 8.0);
    path.close();

    let pattern = Pattern::OpaquePattern(OpaquePattern {
        pixel: Pixel::Alpha8(Alpha8 { a: 255 }),
    });
    painter::fill(
        &mut sfc,
        &pattern,
        &path.wrapped_path.nodes,
        FillOptions {
            anti_aliasing_mode: AntiAliasMode::None,
            ..FillOptions::default()
        },
    )
    .unwrap();

    // Interior is filled.
    assert_eq!(255, alpha_at(&sfc, 4, 4));
    assert_eq!(255, alpha_at(&sfc, 5, 5));
    assert_eq!(255, alpha_at(&sfc, 3, 7));
    // Well outside the rectangle is untouched.
    assert_eq!(0, alpha_at(&sfc, 0, 0));
    assert_eq!(0, alpha_at(&sfc, 9, 9));
    assert_eq!(0, alpha_at(&sfc, 9, 0));

    // Some pixel in the interior must be lit, and some corner must be dark: a
    // coarse "the fill actually did something" check over the whole buffer.
    let buf = sfc.alpha8_buf();
    assert_eq!(100, buf.len());
    assert!(
        buf.iter().any(|p| p.a == 255),
        "expected some filled pixels"
    );
    assert!(buf.iter().any(|p| p.a == 0), "expected some empty pixels");

    sfc.deinit();
}

#[test]
fn fill_a_triangle() {
    let mut sfc = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 0 }), 20, 20).unwrap();

    let mut path: StaticPath<5> = StaticPath::new();
    path.init();
    path.move_to(10.0, 2.0);
    path.line_to(18.0, 18.0);
    path.line_to(2.0, 18.0);
    path.close();

    let pattern = Pattern::OpaquePattern(OpaquePattern {
        pixel: Pixel::Alpha8(Alpha8 { a: 255 }),
    });
    painter::fill(
        &mut sfc,
        &pattern,
        &path.wrapped_path.nodes,
        FillOptions {
            anti_aliasing_mode: AntiAliasMode::None,
            ..FillOptions::default()
        },
    )
    .unwrap();

    // The centroid-ish interior point is inside the triangle.
    assert_eq!(255, alpha_at(&sfc, 10, 12));
    // The top corners of the bounding box are outside the triangle.
    assert_eq!(0, alpha_at(&sfc, 2, 3));
    assert_eq!(0, alpha_at(&sfc, 18, 3));

    sfc.deinit();
}

#[test]
fn put_pixel_and_readback() {
    let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 4, 4).unwrap();
    // Everything starts transparent.
    assert!(sfc.alpha8_buf().iter().all(|p| p.a == 0));

    sfc.put_pixel(1, 2, Pixel::Alpha8(Alpha8 { a: 200 }));
    assert_eq!(200, alpha_at(&sfc, 1, 2));
    // Out-of-bounds writes are no-ops.
    sfc.put_pixel(4, 0, Pixel::Alpha8(Alpha8 { a: 111 }));
    sfc.put_pixel(0, 4, Pixel::Alpha8(Alpha8 { a: 111 }));
    assert!(sfc.get_pixel(4, 0).is_none());
    assert!(sfc.get_pixel(0, 4).is_none());

    // paint_stride fills a run on one row.
    sfc.paint_stride(0, 0, 4, Pixel::Alpha8(Alpha8 { a: 128 }));
    for x in 0..4 {
        assert_eq!(128, alpha_at(&sfc, x, 0));
    }
    // Row 1 is still clear.
    assert_eq!(0, alpha_at(&sfc, 0, 1));

    sfc.deinit();
}

#[test]
fn stroke_a_horizontal_line() {
    // 12x12 alpha8 surface. A horizontal line from (2,6) to (10,6), stroked
    // with thickness 2 (butt caps), no AA so coverage is exactly on/off. The
    // stroke tessellates to a rectangle centered on the line spanning y in
    // [5,7].
    let mut sfc = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 0 }), 12, 12).unwrap();

    let mut path: StaticPath<2> = StaticPath::new();
    path.init();
    path.move_to(2.0, 6.0);
    path.line_to(10.0, 6.0);

    let pattern = Pattern::OpaquePattern(OpaquePattern {
        pixel: Pixel::Alpha8(Alpha8 { a: 255 }),
    });
    painter::stroke(
        &mut sfc,
        &pattern,
        &path.wrapped_path.nodes,
        StrokeOptions {
            anti_aliasing_mode: AntiAliasMode::None,
            line_cap_mode: CapMode::Butt,
            line_width: 2.0,
            ..StrokeOptions::default()
        },
    )
    .unwrap();

    // Pixels on the stroked line are lit.
    assert_eq!(255, alpha_at(&sfc, 5, 5));
    assert_eq!(255, alpha_at(&sfc, 5, 6));
    assert_eq!(255, alpha_at(&sfc, 8, 6));
    // Well away from the line is untouched.
    assert_eq!(0, alpha_at(&sfc, 5, 0));
    assert_eq!(0, alpha_at(&sfc, 5, 11));
    assert_eq!(0, alpha_at(&sfc, 0, 0));

    let buf = sfc.alpha8_buf();
    assert!(buf.iter().any(|p| p.a == 255), "expected stroked pixels");
    assert!(buf.iter().any(|p| p.a == 0), "expected empty pixels");

    sfc.deinit();
}

#[test]
fn stroke_a_rect_outline() {
    // A closed rectangle from (3,3) to (13,13), stroked with thickness 2 and no
    // AA. Only the OUTLINE is drawn — the interior stays transparent.
    let mut sfc = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 0 }), 16, 16).unwrap();

    let mut path: StaticPath<6> = StaticPath::new();
    path.init();
    path.move_to(3.0, 3.0);
    path.line_to(13.0, 3.0);
    path.line_to(13.0, 13.0);
    path.line_to(3.0, 13.0);
    path.close();

    let pattern = Pattern::OpaquePattern(OpaquePattern {
        pixel: Pixel::Alpha8(Alpha8 { a: 255 }),
    });
    painter::stroke(
        &mut sfc,
        &pattern,
        &path.wrapped_path.nodes,
        StrokeOptions {
            anti_aliasing_mode: AntiAliasMode::None,
            line_cap_mode: CapMode::Butt,
            line_width: 2.0,
            ..StrokeOptions::default()
        },
    )
    .unwrap();

    // The center of the rectangle is hollow (outline stroke only).
    assert_eq!(0, alpha_at(&sfc, 8, 8));
    // A point on the top edge (centered on y=3, so y in [2,4]) is lit.
    assert_eq!(255, alpha_at(&sfc, 8, 3));
    // A point on the left edge (centered on x=3, so x in [2,4]) is lit.
    assert_eq!(255, alpha_at(&sfc, 3, 8));

    let buf = sfc.alpha8_buf();
    assert!(buf.iter().any(|p| p.a == 255), "expected outline pixels");

    sfc.deinit();
}

#[cfg_attr(miri, ignore)]
// Miri deliberately randomizes sin/cos/acos within 1 ULP (float non-determinism,
// not UB); the exact boundary pixels of the round-cap fan are verified natively.
#[test]
fn stroke_a_round_capped_line() {
    // Exercises the Pen / round-cap fan path. A short line stroked with round
    // caps must light up pixels beyond each butt endpoint (the semicircular
    // caps), and must not panic building the pen vertex fan.
    let mut sfc = Surface::init_pixel(Pixel::Alpha8(Alpha8 { a: 0 }), 20, 20).unwrap();

    let mut path: StaticPath<2> = StaticPath::new();
    path.init();
    path.move_to(6.0, 10.0);
    path.line_to(14.0, 10.0);

    let pattern = Pattern::OpaquePattern(OpaquePattern {
        pixel: Pixel::Alpha8(Alpha8 { a: 255 }),
    });
    painter::stroke(
        &mut sfc,
        &pattern,
        &path.wrapped_path.nodes,
        StrokeOptions {
            anti_aliasing_mode: AntiAliasMode::None,
            line_cap_mode: CapMode::Round,
            line_width: 6.0,
            ..StrokeOptions::default()
        },
    )
    .unwrap();

    // On the line body.
    assert_eq!(255, alpha_at(&sfc, 10, 10));
    // The round cap extends left of the p0 endpoint (x=6).
    assert_eq!(255, alpha_at(&sfc, 4, 10));
    // The round cap extends right of the p1 endpoint (x=14).
    assert_eq!(255, alpha_at(&sfc, 15, 10));

    let buf = sfc.alpha8_buf();
    assert!(buf.iter().any(|p| p.a == 255), "expected stroked pixels");

    sfc.deinit();
}
