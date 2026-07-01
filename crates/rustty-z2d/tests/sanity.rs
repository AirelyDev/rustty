// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2025 rustty contributors
//
// z2d-internal sanity tests (NOT part of the ghostty parity contract; z2d is a
// dependency). These anchor correctness of the ported pixel-producing core:
// building a `Surface`, driving pixels via `put_pixel`, and running the FILL
// scanline rasterizer over a `StaticPath` — then reading back the pixel buffer.

use rustty_z2d::options::AntiAliasMode;
use rustty_z2d::painter::{self, FillOptions};
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
