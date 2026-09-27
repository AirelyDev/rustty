//! WS12 T8-T11 — KATs for the four formerly-seamed image-tier functions:
//! `Vaxis::transmit_image`, `Vaxis::load_image`, `Image::draw`, `Image::cell_size`.
//!
//! MIRI: the `draw`/`cell_size` geometry KATs are pure and stay Miri-live. The
//! `transmit_image(.., Png)` / `load_image` arms reach `rustty-zigimg`'s wuffs + zlib FFI
//! and carry `#[cfg_attr(miri, ignore)]`; so do the file-touching arms. Recorded in
//! `.port/phase-2/MIRI_EXCEPTIONS.md`. The `NoGraphicsCapability` negative controls stay
//! Miri-live because they return before touching any codec.
//!
//! Non-vacuity, stated per group:
//!   - `draw`: every one of the four `Scale` arms plus the `contain` early break is
//!     asserted to a DIFFERENT hand-computed `size`, and `Scale::None` is asserted to
//!     leave `size == None`. If the arms all produced one answer the group would be
//!     vacuous, and `image_draw_scale_none_leaves_size_none` is what rules that out.
//!   - `cell_size`: a geometry where `divCeil` and truncating `/` DISAGREE (100px over
//!     30px/col => 4, not 3), so an accidental `/` port is caught; plus the `try` arm and
//!     the `catch 0` arm asserted separately, proving they are genuinely different.
//!   - `transmit_image`: the COMPLETE expected byte string, not `is_ok()`; and a
//!     zero-bytes-written negative control.

use rustty_vaxis::image::{
    CellSize, DrawError, DrawOptions, Image as VxImage, ImageSize, Scale, Source, TransmitFormat,
};
use rustty_vaxis::screen::Screen;
use rustty_vaxis::vaxis::{ImageError, Vaxis};
use rustty_vaxis::window::Window;
use rustty_vaxis::Winsize;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// A `Screen` with explicit cell AND pixel geometry, plus a `Window` over all of it.
///
/// `width`/`height`/`width_pix`/`height_pix` are all `pub` (`screen.rs:21-25`), so the
/// geometry the two methods read can be set exactly.
fn screen_of(cols: u16, rows: u16, x_pixel: u16, y_pixel: u16) -> Screen<'static> {
    Screen::init(Winsize {
        rows,
        cols,
        x_pixel,
        y_pixel,
    })
}

fn window_over(screen: &mut Screen<'static>, width: u16, height: u16) -> Window<'static> {
    Window {
        x_off: 0,
        y_off: 0,
        parent_x_off: 0,
        parent_y_off: 0,
        width,
        height,
        screen: screen as *mut Screen<'static>,
    }
}

/// `draw` writes its `Placement` into the cell at (0,0); read it back out.
fn placement_size_at_0_0(screen: &Screen<'static>) -> Option<ImageSize> {
    screen.buf[0]
        .image
        .expect("draw must write a Placement")
        .options
        .size
}

fn placement_img_id_at_0_0(screen: &Screen<'static>) -> u32 {
    screen.buf[0]
        .image
        .expect("draw must write a Placement")
        .img_id
}

/// A `Vaxis` with the kitty-graphics capability forced on, and a `Vec<u8>` sink.
fn vaxis_with_kitty(enabled: bool) -> Vaxis<'static> {
    let mut vx = Vaxis::default();
    vx.caps.kitty_graphics = enabled;
    vx
}

// ---------------------------------------------------------------------------
// T10 — Image::draw
// ---------------------------------------------------------------------------

/// NEGATIVE CONTROL for the whole `draw` group: `.none` must leave `size` untouched.
/// If all four arms collapsed to one answer, this is the test that fails.
#[test]
fn image_draw_scale_none_leaves_size_none() {
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 80, 24);
    let img = VxImage {
        id: 7,
        width: 4000,
        height: 4000,
    };

    img.draw(
        win,
        DrawOptions {
            scale: Scale::None,
            ..Default::default()
        },
    )
    .unwrap();

    // "no scaling applied. the image may extend beyond the window" — and this image is
    // 4000px in a 800x480px window, so a scaling arm would definitely have set something.
    assert_eq!(placement_size_at_0_0(&screen), None);
}

#[test]
fn image_draw_scale_fill_sets_window_size() {
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    let img = VxImage {
        id: 1,
        width: 100,
        height: 100,
    };

    img.draw(
        win,
        DrawOptions {
            scale: Scale::Fill,
            ..Default::default()
        },
    )
    .unwrap();

    // .fill => size = { rows = win.height, cols = win.width } — BOTH set, unconditionally,
    // with no reference to the image's own dimensions.
    assert_eq!(
        placement_size_at_0_0(&screen),
        Some(ImageSize {
            rows: Some(12),
            cols: Some(30),
        })
    );
}

#[test]
fn image_draw_scale_fit_scales_by_height_when_only_y_does_not_fit() {
    // Geometry: 80x24 cells over 800x480 px => pix_per_col = 10, pix_per_row = 20.
    // Window 30x12 => win_width_pix = 300, win_height_pix = 240.
    // Image 200x400: fit_x = 300 >= 200 = true; fit_y = 240 >= 400 = false.
    // => the `fit_x and !fit_y` arm => size = { rows = win.height } and cols stays null.
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    let img = VxImage {
        id: 2,
        width: 200,
        height: 400,
    };

    img.draw(
        win,
        DrawOptions {
            scale: Scale::Fit,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        placement_size_at_0_0(&screen),
        Some(ImageSize {
            rows: Some(12),
            cols: None,
        })
    );
}

#[test]
fn image_draw_scale_fit_scales_by_width_when_only_x_does_not_fit() {
    // Same geometry; image 400x200: fit_x = 300 >= 400 = false; fit_y = 240 >= 200 = true.
    // => the `!fit_x and fit_y` arm => size = { cols = win.width }, rows null.
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    let img = VxImage {
        id: 3,
        width: 400,
        height: 200,
    };

    img.draw(
        win,
        DrawOptions {
            scale: Scale::Fit,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        placement_size_at_0_0(&screen),
        Some(ImageSize {
            rows: None,
            cols: Some(30),
        })
    );
}

#[test]
fn image_draw_scale_fit_neither_fits_picks_the_larger_difference() {
    // Same geometry (win 300x240 px). Image 1000x300:
    //   fit_x = 300 >= 1000 = false; fit_y = 240 >= 300 = false  => the !fit_x && !fit_y arm
    //   diff_x = 1000 - 300 = 700; diff_y = 300 - 240 = 60
    //   diff_x > diff_y => "Scale by width" => size = { cols = win.width }
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    img_draw_expect(
        &mut screen,
        win,
        VxImage {
            id: 4,
            width: 1000,
            height: 300,
        },
        Scale::Fit,
        Some(ImageSize {
            rows: None,
            cols: Some(30),
        }),
    );

    // And the mirror case, so the `diff_x > diff_y` branch is not the only one exercised.
    // Image 400x2000: diff_x = 100, diff_y = 1760 => diff_x <= diff_y => "Scale by height".
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    img_draw_expect(
        &mut screen,
        win,
        VxImage {
            id: 5,
            width: 400,
            height: 2000,
        },
        Scale::Fit,
        Some(ImageSize {
            rows: Some(12),
            cols: None,
        }),
    );
}

#[test]
fn image_draw_scale_fit_both_fit_picks_the_closer_direction() {
    // The final `else` (the branch guarded by the reference's three debug asserts).
    // win 300x240 px. Image 290x100: fit_x and fit_y both true, and scale == .fit so the
    // `.contain` early break does NOT apply.
    //   diff_x = 300 - 290 = 10; diff_y = 240 - 100 = 140
    //   diff_x < diff_y => "The width is closer in dimension. Scale by that" => cols.
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    img_draw_expect(
        &mut screen,
        win,
        VxImage {
            id: 6,
            width: 290,
            height: 100,
        },
        Scale::Fit,
        Some(ImageSize {
            rows: None,
            cols: Some(30),
        }),
    );

    // Mirror: image 100x230 => diff_x = 200, diff_y = 10 => NOT (diff_x < diff_y) => rows.
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    img_draw_expect(
        &mut screen,
        win,
        VxImage {
            id: 7,
            width: 100,
            height: 230,
        },
        Scale::Fit,
        Some(ImageSize {
            rows: Some(12),
            cols: None,
        }),
    );
}

/// The `break :contain` early exit — the ONLY behavioural difference between `.fit` and
/// `.contain`, and therefore the arm most likely to be silently dropped in a port.
#[test]
fn image_draw_scale_contain_fits_leaves_size_unset() {
    // win 300x240 px; image 290x100 fits in both directions.
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    let img = VxImage {
        id: 8,
        width: 290,
        height: 100,
    };

    img.draw(
        win,
        DrawOptions {
            scale: Scale::Contain,
            ..Default::default()
        },
    )
    .unwrap();

    // `break :contain` => size untouched. Contrast with
    // `image_draw_scale_fit_both_fit_picks_the_closer_direction`, which uses the SAME
    // geometry with `.fit` and gets `cols: Some(30)`. That pairing is what proves the
    // early break is real rather than the two enum values being aliases.
    assert_eq!(placement_size_at_0_0(&screen), None);

    // ...and a `.contain` image that does NOT fit must still scale, so the early break is
    // conditional rather than "contain never scales".
    let mut screen2 = screen_of(80, 24, 800, 480);
    let win2 = window_over(&mut screen2, 30, 12);
    img_draw_expect(
        &mut screen2,
        win2,
        VxImage {
            id: 9,
            width: 400,
            height: 200,
        },
        Scale::Contain,
        Some(ImageSize {
            rows: None,
            cols: Some(30),
        }),
    );
}

#[test]
fn image_draw_writes_placement_into_cell_0_0() {
    let mut screen = screen_of(80, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    let img = VxImage {
        id: 0xDEAD_BEEF,
        width: 10,
        height: 10,
    };

    // Cell (0,0) starts with no image at all — so the assertion below is about `draw`
    // having written, not about a pre-existing value.
    assert!(screen.buf[0].image.is_none());

    img.draw(win, DrawOptions::default()).unwrap();

    assert_eq!(placement_img_id_at_0_0(&screen), 0xDEAD_BEEF);
    // And ONLY (0,0) is written — the reference writes exactly one cell.
    assert!(screen.buf[1].image.is_none());
}

#[test]
fn image_draw_zero_screen_cells_errors() {
    // The reference's `try std.math.divCeil` propagates on a zero divisor. Only the
    // fit/contain arms divide, so `.fill`/`.none` must still succeed on the same screen —
    // asserted below, which is what makes this about the division rather than about the
    // screen being degenerate.
    let mut screen = screen_of(0, 24, 800, 480);
    let win = window_over(&mut screen, 30, 12);
    let img = VxImage {
        id: 1,
        width: 10,
        height: 10,
    };

    assert_eq!(
        img.draw(
            win,
            DrawOptions {
                scale: Scale::Fit,
                ..Default::default()
            }
        ),
        Err(DrawError::DivisionByZero)
    );
    assert_eq!(
        img.draw(
            win,
            DrawOptions {
                scale: Scale::Contain,
                ..Default::default()
            }
        ),
        Err(DrawError::DivisionByZero)
    );
    // `.fill` and `.none` never divide.
    assert!(img
        .draw(
            win,
            DrawOptions {
                scale: Scale::Fill,
                ..Default::default()
            }
        )
        .is_ok());
    assert!(img.draw(win, DrawOptions::default()).is_ok());
}

fn img_draw_expect(
    screen: &mut Screen<'static>,
    win: Window<'static>,
    img: VxImage,
    scale: Scale,
    expected: Option<ImageSize>,
) {
    img.draw(
        win,
        DrawOptions {
            scale,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(placement_size_at_0_0(screen), expected, "scale = {scale:?}");
}

// ---------------------------------------------------------------------------
// T11 — Image::cell_size
// ---------------------------------------------------------------------------

/// THE NEGATIVE CONTROL against an accidental `/` port: a geometry where `divCeil` and
/// truncating division give different answers.
#[test]
fn image_cell_size_div_ceil() {
    // 10 cols over 300 px => pix_per_col = 30. 10 rows over 300 px => pix_per_row = 30.
    // Image 100x100 px:
    //   divCeil(100, 30) = 4   <-- correct
    //   100 / 30          = 3   <-- what a `/` port would give
    let mut screen = screen_of(10, 10, 300, 300);
    let win = window_over(&mut screen, 10, 10);
    let img = VxImage {
        id: 1,
        width: 100,
        height: 100,
    };
    assert_eq!(
        img.cell_size(win).unwrap(),
        CellSize { rows: 4, cols: 4 },
        "divCeil(100, 30) must be 4, not the truncating 3"
    );
}

/// The `catch 0` arm: a zero `pix_per_col`/`pix_per_row` yields 0, it does NOT error.
#[test]
fn image_cell_size_zero_pix_per_col_yields_zero() {
    // 10 cols over 0 px => divCeil(0, 10) = 0 => pix_per_col = 0, and likewise for rows.
    // Then divCeil(width, 0) fails and is `catch 0`-ed.
    let mut screen = screen_of(10, 10, 0, 0);
    let win = window_over(&mut screen, 10, 10);
    let img = VxImage {
        id: 1,
        width: 100,
        height: 100,
    };
    assert_eq!(
        img.cell_size(win).unwrap(),
        CellSize { rows: 0, cols: 0 },
        "the reference's `catch 0` must swallow this, not propagate"
    );
}

/// The `try` arm — proving the two arms are genuinely DIFFERENT, which is the whole point
/// of the reference's asymmetry.
#[test]
fn image_cell_size_zero_screen_cells_errors() {
    // w == 0 => the FIRST divCeil fails, and that one is `try`.
    let mut screen = screen_of(0, 10, 300, 300);
    let win = window_over(&mut screen, 10, 10);
    let img = VxImage {
        id: 1,
        width: 100,
        height: 100,
    };
    assert_eq!(img.cell_size(win), Err(DrawError::DivisionByZero));

    // h == 0 => the SECOND divCeil fails, also `try`.
    let mut screen = screen_of(10, 0, 300, 300);
    let win = window_over(&mut screen, 10, 10);
    assert_eq!(img.cell_size(win), Err(DrawError::DivisionByZero));
}

/// Guards against a transposed `CellSize { rows, cols }` return.
#[test]
fn image_cell_size_rows_from_height() {
    // pix_per_col = divCeil(300, 10) = 30; pix_per_row = divCeil(600, 10) = 60.
    // Image 100 wide x 10 high:
    //   cols = divCeil(width  = 100, 30) = 4
    //   rows = divCeil(height =  10, 60) = 1
    // A transposed return would give rows = 4, cols = 1.
    let mut screen = screen_of(10, 10, 300, 600);
    let win = window_over(&mut screen, 10, 10);
    let img = VxImage {
        id: 1,
        width: 100,
        height: 10,
    };
    assert_eq!(img.cell_size(win).unwrap(), CellSize { rows: 1, cols: 4 });
}

// ---------------------------------------------------------------------------
// T8 — Vaxis::transmit_image
// ---------------------------------------------------------------------------

/// The complete expected escape for a fixed 2x2 RGB image — opener params, the exact
/// base64 payload, and the `ESC\` terminator. Not `is_ok()`.
#[test]
fn transmit_image_rgb_emits_exact_escape() {
    // Source pixels, rgba32, all fully opaque so the rgb24 conversion's alpha
    // premultiplication is the identity and the expected bytes are hand-checkable:
    //   (0,0) = 10,20,30   (1,0) = 40,50,60
    //   (0,1) = 70,80,90   (1,1) = 100,110,120
    let src = vec![
        10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
    ];
    let img = rustty_zigimg::Image::from_raw_pixels(2, 2, src, rustty_zigimg::PixelFormat::Rgba32)
        .unwrap();

    let mut vx = vaxis_with_kitty(true);
    let mut sink: Vec<u8> = Vec::new();
    let out = vx
        .transmit_image(&mut sink, &img, TransmitFormat::Rgb)
        .expect("transmit must succeed");

    // The converted rgb24 bytes are 10,20,30,40,50,60,70,80,90,100,110,120 (12 bytes),
    // whose standard base64 (no line breaks, `=` padding; 12 bytes is a multiple of 3 so
    // there is none) is:
    //   0a 14 1e 28 32 3c 46 50 5a 64 6e 78  ->  "ChQeKDI8RlBaZG54"
    let expected_payload = b"ChQeKDI8RlBaZG54";
    // `transmit_pre_encoded_image_inner`: f=24 for Rgb, s=width, v=height, i=id. The
    // payload is 16 base64 bytes, well under 4096, so the single-chunk (no `m=`) form.
    let mut expected: Vec<u8> = b"\x1b_Gf=24,s=2,v=2,i=1;".to_vec();
    expected.extend_from_slice(expected_payload);
    expected.extend_from_slice(b"\x1b\\");

    assert_eq!(sink, expected);
    // The returned handle carries the id the escape used, and the PIXEL dimensions.
    assert_eq!(out.id, 1);
    assert_eq!(out.width, 2);
    assert_eq!(out.height, 2);
    // `next_img_id` advanced exactly once (the reference's `defer self.next_img_id += 1`).
    assert_eq!(vx.next_img_id, 2);
    // libvaxis 0.6.0 takes `img: *const zigimg.Image` and converts a copy
    // (`var img_modifiable = img.*; try img_modifiable.convertNoFree(arena, .rgb24)`), so the
    // caller's image is left as it was (0.5.1 converted it in place).
    assert_eq!(img.pixel_format(), rustty_zigimg::PixelFormat::Rgba32);
}

#[test]
fn transmit_image_rgba_converts_then_emits() {
    let src = vec![
        10, 20, 30, 255, 40, 50, 60, 254, 70, 80, 90, 253, 100, 110, 120, 252,
    ];
    let img = rustty_zigimg::Image::from_raw_pixels(
        2,
        2,
        src.clone(),
        rustty_zigimg::PixelFormat::Rgba32,
    )
    .unwrap();

    let mut vx = vaxis_with_kitty(true);
    let mut sink: Vec<u8> = Vec::new();
    vx.transmit_image(&mut sink, &img, TransmitFormat::Rgba)
        .unwrap();

    // f=32 for Rgba.
    assert!(sink.starts_with(b"\x1b_Gf=32,s=2,v=2,i=1;"));
    assert!(sink.ends_with(b"\x1b\\"));

    // The payload is base64 of w*h*4 = 16 raw bytes => 16/3 rounded up * 4 = 24 chars.
    let header_len = b"\x1b_Gf=32,s=2,v=2,i=1;".len();
    let payload = &sink[header_len..sink.len() - 2];
    assert_eq!(payload.len(), 24, "base64 of 16 bytes is 24 chars");

    // The already-rgba32 image must NOT have been round-tripped: the reference's identity
    // guard (`activeTag == destination_format`) returns early, so the alpha channel — which
    // a spurious rgb24 detour would have destroyed — is intact.
    assert_eq!(img.pixel_format(), rustty_zigimg::PixelFormat::Rgba32);
    assert_eq!(img.raw_bytes(), &src[..]);
}

/// PNG arm. **Size matters here, and the reason is a real upstream constraint, not a
/// test-tuning detail.** `Vaxis.zig:996` sizes the PNG buffer at `img.imageByteSize()` —
/// the RAW pixel byte count — and a PNG has ~57 bytes of unavoidable fixed overhead
/// (8-byte signature + 25-byte IHDR + 12-byte IDAT framing + 12-byte IEND) before a single
/// compressed byte. So **a tiny image cannot be transmitted as PNG at all**: a 4x4 rgba32
/// image has only 64 raw bytes of budget and overflows. That is upstream's behaviour too,
/// faithfully preserved as `ImageError::Decode(NoSpaceLeft)` (this test was originally
/// written at 4x4 and hit exactly that). 32x32 = 4096 bytes of budget is comfortable.
#[test]
#[cfg_attr(miri, ignore = "PNG encode/decode crosses into zlib + wuffs FFI")]
fn transmit_image_png_round_trips() {
    let (w, h) = (32usize, 32usize);
    let mut px = Vec::new();
    for _ in 0..(w * h) {
        px.extend_from_slice(&[0x20, 0x40, 0x60, 0xFF]);
    }
    let img =
        rustty_zigimg::Image::from_raw_pixels(w, h, px.clone(), rustty_zigimg::PixelFormat::Rgba32)
            .unwrap();

    let mut vx = vaxis_with_kitty(true);
    let mut sink: Vec<u8> = Vec::new();
    vx.transmit_image(&mut sink, &img, TransmitFormat::Png)
        .unwrap();

    // f=100 for Png.
    let header = b"\x1b_Gf=100,s=32,v=32,i=1;";
    assert!(
        sink.starts_with(header),
        "got: {:?}",
        &sink[..30.min(sink.len())]
    );
    let payload = &sink[header.len()..sink.len() - 2];

    // Extract and decode the base64 payload, then decode the PNG: the pixels must come
    // back bit-exact. This is what proves the escape carries a REAL image rather than
    // plausible-looking bytes.
    let png = base64_decode(payload);
    let decoded = rustty_wuffs::png::decode(allocator_api2::alloc::Global, &png)
        .expect("the transmitted payload must be a decodable PNG");
    assert_eq!(decoded.width, 32);
    assert_eq!(decoded.height, 32);
    assert_eq!(decoded.bytes(), &px[..]);
}

/// NEGATIVE CONTROL: no capability => `Err` AND an untouched sink.
#[test]
fn transmit_image_without_kitty_graphics_writes_nothing() {
    let img = rustty_zigimg::Image::from_raw_pixels(
        1,
        1,
        vec![1, 2, 3, 4],
        rustty_zigimg::PixelFormat::Rgba32,
    )
    .unwrap();

    let mut vx = vaxis_with_kitty(false);
    let mut sink: Vec<u8> = Vec::new();
    let r = vx.transmit_image(&mut sink, &img, TransmitFormat::Rgb);

    assert!(matches!(r, Err(ImageError::NoGraphicsCapability)));
    assert!(sink.is_empty(), "no bytes may be written: {sink:?}");
    // And the guard runs BEFORE the id bump.
    assert_eq!(vx.next_img_id, 1);
    // ...and before any conversion.
    assert_eq!(img.pixel_format(), rustty_zigimg::PixelFormat::Rgba32);
}

// ---------------------------------------------------------------------------
// T9 — Vaxis::load_image
// ---------------------------------------------------------------------------

/// Encode a small PNG through `rustty-zigimg` for the `load_image` inputs.
#[cfg(not(miri))]
/// 32x32, not 4x4 — see `transmit_image_png_round_trips`: the reference's
/// `imageByteSize()`-sized PNG buffer cannot fit a PNG of a tiny image.
fn png_fixture() -> (Vec<u8>, Vec<u8>) {
    let (w, h) = (32usize, 32usize);
    let mut px = Vec::new();
    for i in 0..(w * h) {
        let i = i as u8;
        px.extend_from_slice(&[
            i.wrapping_mul(3),
            i.wrapping_mul(5),
            i.wrapping_mul(7),
            0xFF,
        ]);
    }
    let img =
        rustty_zigimg::Image::from_raw_pixels(w, h, px.clone(), rustty_zigimg::PixelFormat::Rgba32)
            .unwrap();
    let mut buf = vec![0u8; 64 * 1024];
    let png = rustty_zigimg::write_to_memory(&img, &mut buf)
        .unwrap()
        .to_vec();
    (png, px)
}

#[test]
#[cfg_attr(miri, ignore = "PNG encode/decode crosses into zlib + wuffs FFI")]
fn load_image_from_mem_png_transmits() {
    let (png, px) = png_fixture();

    let mut vx = vaxis_with_kitty(true);
    let mut sink: Vec<u8> = Vec::new();
    let out = vx.load_image(&mut sink, Source::Mem(&png)).unwrap();

    assert_eq!((out.width, out.height), (32, 32));
    let header = b"\x1b_Gf=100,s=32,v=32,i=1;";
    assert!(sink.starts_with(header));
    assert!(sink.ends_with(b"\x1b\\"));

    // The transmitted PNG must decode back to the original pixels: load_image really
    // decoded the input and re-encoded it, rather than passing bytes through.
    let payload = base64_decode(&sink[header.len()..sink.len() - 2]);
    let decoded = rustty_wuffs::png::decode(allocator_api2::alloc::Global, &payload).unwrap();
    assert_eq!(decoded.bytes(), &px[..]);
}

#[test]
#[cfg_attr(miri, ignore = "reads a file; PNG codecs cross into FFI")]
fn load_image_from_path_png_transmits_identically_to_mem() {
    let (png, _) = png_fixture();

    let path = std::env::temp_dir().join(format!(
        "rustty-vaxis-image-tier-{}.png",
        std::process::id()
    ));
    std::fs::write(&path, &png).unwrap();

    let mut vx_path = vaxis_with_kitty(true);
    let mut sink_path: Vec<u8> = Vec::new();
    let r = {
        vx_path.load_image(
            &mut sink_path,
            Source::Path(path.as_os_str().as_encoded_bytes()),
        )
    };
    let _ = std::fs::remove_file(&path);
    r.expect("load from path must succeed");

    let mut vx_mem = vaxis_with_kitty(true);
    let mut sink_mem: Vec<u8> = Vec::new();
    vx_mem.load_image(&mut sink_mem, Source::Mem(&png)).unwrap();

    // Byte-identical: the two Source arms differ only in where the bytes came from.
    assert_eq!(sink_path, sink_mem);
}

#[test]
fn load_image_rejects_non_image() {
    let mut vx = vaxis_with_kitty(true);
    let mut sink: Vec<u8> = Vec::new();
    let r = vx.load_image(&mut sink, Source::Mem(b"nope"));

    // The magic-byte sniff refuses before any codec runs, so this stays Miri-live.
    assert!(matches!(
        r,
        Err(ImageError::Decode(rustty_zigimg::Error::UnsupportedFormat))
    ));
    assert!(sink.is_empty(), "a rejected input must emit nothing");
    assert_eq!(vx.next_img_id, 1);
}

/// NEGATIVE CONTROL for `load_image`.
#[test]
fn load_image_without_kitty_graphics_writes_nothing() {
    let mut vx = vaxis_with_kitty(false);
    let mut sink: Vec<u8> = Vec::new();
    // Note the input is a NON-image: the capability guard must fire first, so this also
    // proves the guard precedes the decode (otherwise we would get an UnsupportedFormat).
    let r = vx.load_image(&mut sink, Source::Mem(b"not an image at all"));

    assert!(matches!(r, Err(ImageError::NoGraphicsCapability)));
    assert!(sink.is_empty());
    assert_eq!(vx.next_img_id, 1);
}

// ---------------------------------------------------------------------------
// test-local base64 decoder (the crate exports only the encoder, matching the reference)
// ---------------------------------------------------------------------------

fn base64_decode(input: &[u8]) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    let mut out = Vec::new();
    for &c in input {
        if c == b'=' {
            break;
        }
        let v = ALPHABET
            .iter()
            .position(|&a| a == c)
            .unwrap_or_else(|| panic!("non-base64 byte {c:#x} in payload")) as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

#[test]
fn base64_decode_helper_is_correct() {
    // The helper is test infrastructure, so pin it — a broken decoder would make the
    // round-trip assertions above fail for the wrong reason (or, worse, pass).
    assert_eq!(
        base64_decode(b"ChQeKDI8RlBaZG54"),
        vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120]
    );
    assert_eq!(base64_decode(b"Zm9vYmFy"), b"foobar".to_vec());
    assert_eq!(base64_decode(b"Zg=="), b"f".to_vec());
    assert_eq!(base64_decode(b"Zm8="), b"fo".to_vec());
}
