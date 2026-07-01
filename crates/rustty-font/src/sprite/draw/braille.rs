//! Braille Patterns | U+2800...U+28FF
//! https://en.wikipedia.org/wiki/Braille_Patterns
//!
//! (6 dot patterns)
//! ⠀ ⠁ ⠂ ⠃ ⠄ ⠅ ⠆ ⠇ ⠈ ⠉ ⠊ ⠋ ⠌ ⠍ ⠎ ⠏
//! ⠐ ⠑ ⠒ ⠓ ⠔ ⠕ ⠖ ⠗ ⠘ ⠙ ⠚ ⠛ ⠜ ⠝ ⠞ ⠟
//! ⠠ ⠡ ⠢ ⠣ ⠤ ⠥ ⠦ ⠧ ⠨ ⠩ ⠪ ⠫ ⠬ ⠭ ⠮ ⠯
//! ⠰ ⠱ ⠲ ⠳ ⠴ ⠵ ⠶ ⠷ ⠸ ⠹ ⠺ ⠻ ⠼ ⠽ ⠾ ⠿
//!
//! (8 dot patterns)
//! ⡀ ⡁ ⡂ ⡃ ⡄ ⡅ ⡆ ⡇ ⡈ ⡉ ⡊ ⡋ ⡌ ⡍ ⡎ ⡏
//! ⡐ ⡑ ⡒ ⡓ ⡔ ⡕ ⡖ ⡗ ⡘ ⡙ ⡚ ⡛ ⡜ ⡝ ⡞ ⡟
//! ⡠ ⡡ ⡢ ⡣ ⡤ ⡥ ⡦ ⡧ ⡨ ⡩ ⡪ ⡫ ⡬ ⡭ ⡮ ⡯
//! ⡰ ⡱ ⡲ ⡳ ⡴ ⡵ ⡶ ⡷ ⡸ ⡹ ⡺ ⡻ ⡼ ⡽ ⡾ ⡿
//! ⢀ ⢁ ⢂ ⢃ ⢄ ⢅ ⢆ ⢇ ⢈ ⢉ ⢊ ⢋ ⢌ ⢍ ⢎ ⢏
//! ⢐ ⢑ ⢒ ⢓ ⢔ ⢕ ⢖ ⢗ ⢘ ⢙ ⢚ ⢛ ⢜ ⢝ ⢞ ⢟
//! ⢠ ⢡ ⢢ ⢣ ⢤ ⢥ ⢦ ⢧ ⢨ ⢩ ⢪ ⢫ ⢬ ⢭ ⢮ ⢯
//! ⢰ ⢱ ⢲ ⢳ ⢴ ⢵ ⢶ ⢷ ⢸ ⢹ ⢺ ⢻ ⢼ ⢽ ⢾ ⢿
//! ⣀ ⣁ ⣂ ⣃ ⣄ ⣅ ⣆ ⣇ ⣈ ⣉ ⣊ ⣋ ⣌ ⣍ ⣎ ⣏
//! ⣐ ⣑ ⣒ ⣓ ⣔ ⣕ ⣖ ⣗ ⣘ ⣙ ⣚ ⣛ ⣜ ⣝ ⣞ ⣟
//! ⣠ ⣡ ⣢ ⣣ ⣤ ⣥ ⣦ ⣧ ⣨ ⣩ ⣪ ⣫ ⣬ ⣭ ⣮ ⣯
//! ⣰ ⣱ ⣲ ⣳ ⣴ ⣵ ⣶ ⣷ ⣸ ⣹ ⣺ ⣻ ⣼ ⣽ ⣾ ⣿
//!

// const std = @import("std");
// const assert = @import("../../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
//
// const font = @import("../../main.zig");
//
// PORT NOTE: `assert = quirks.inlineAssert` lives in `rustty-util`. `font =
// ../../main.zig` becomes the local crate (`crate::metrics::Metrics`,
// `crate::sprite::Canvas` / `Color`). `Allocator` is unused here (the reference
// draw signatures return `!void` but never allocate in this module).
use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color};
use rustty_util::quirks::inlineAssert as assert;

/// A braille pattern.
///
/// Mnemonic:
/// [t]op    - .       .
/// [u]pper  - .       .
/// [l]ower  - .       .
/// [b]ottom - .       .
///            |       |
///           [l]eft, [r]ight
///
/// Struct layout matches bit patterns of unicode codepoints.
// const Pattern = packed struct(u8) {
//     tl: bool,
//     ul: bool,
//     ll: bool,
//     tr: bool,
//     ur: bool,
//     lr: bool,
//     bl: bool,
//     br: bool,
//
//     fn from(cp: u32) Pattern {
//         return @bitCast(@as(u8, @truncate(cp)));
//     }
// };
//
// PORT NOTE: the reference is a `packed struct(u8)` whose fields map, LSB→MSB,
// onto the low 8 bits of the codepoint (`from` truncates `cp` to a `u8` and
// `@bitCast`s it). The Rust port carries the eight flags as `bool` fields and
// `from` extracts each bit explicitly, preserving the LSB→MSB order.
struct Pattern {
    tl: bool,
    ul: bool,
    ll: bool,
    tr: bool,
    ur: bool,
    lr: bool,
    bl: bool,
    br: bool,
}

impl Pattern {
    // fn from(cp: u32) Pattern {
    //     return @bitCast(@as(u8, @truncate(cp)));
    // }
    fn from(cp: u32) -> Pattern {
        let b = cp as u8;
        Pattern {
            tl: (b & (1 << 0)) != 0,
            ul: (b & (1 << 1)) != 0,
            ll: (b & (1 << 2)) != 0,
            tr: (b & (1 << 3)) != 0,
            ur: (b & (1 << 4)) != 0,
            lr: (b & (1 << 5)) != 0,
            bl: (b & (1 << 6)) != 0,
            br: (b & (1 << 7)) != 0,
        }
    }
}

// pub fn draw2800_28FF(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
#[allow(non_snake_case)]
pub fn draw2800_28FF(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = metrics;
    let _ = metrics;

    // var w: i32 = @intCast(@min(width / 4, height / 8));
    let mut w: i32 = (width / 4).min(height / 8) as i32;
    // var x_spacing: i32 = @intCast(width / 4);
    let mut x_spacing: i32 = (width / 4) as i32;
    // var y_spacing: i32 = @intCast(height / 8);
    let mut y_spacing: i32 = (height / 8) as i32;
    // var x_margin: i32 = @divFloor(x_spacing, 2);
    let mut x_margin: i32 = x_spacing.div_euclid(2);
    // var y_margin: i32 = @divFloor(y_spacing, 2);
    let mut y_margin: i32 = y_spacing.div_euclid(2);

    // var x_px_left: i32 =
    //     @as(i32, @intCast(width)) - 2 * x_margin - x_spacing - 2 * w;
    let mut x_px_left: i32 = width as i32 - 2 * x_margin - x_spacing - 2 * w;

    // var y_px_left: i32 =
    //     @as(i32, @intCast(height)) - 2 * y_margin - 3 * y_spacing - 4 * w;
    let mut y_px_left: i32 = height as i32 - 2 * y_margin - 3 * y_spacing - 4 * w;

    // First, try hard to ensure the DOT width is non-zero
    // if (x_px_left >= 2 and y_px_left >= 4 and w == 0) {
    if x_px_left >= 2 && y_px_left >= 4 && w == 0 {
        // w += 1;
        w += 1;
        // x_px_left -= 2;
        x_px_left -= 2;
        // y_px_left -= 4;
        y_px_left -= 4;
    }

    // Second, prefer a non-zero margin
    // if (x_px_left >= 2 and x_margin == 0) {
    if x_px_left >= 2 && x_margin == 0 {
        // x_margin = 1;
        x_margin = 1;
        // x_px_left -= 2;
        x_px_left -= 2;
    }
    // if (y_px_left >= 2 and y_margin == 0) {
    if y_px_left >= 2 && y_margin == 0 {
        // y_margin = 1;
        y_margin = 1;
        // y_px_left -= 2;
        y_px_left -= 2;
    }

    // Third, increase spacing
    // if (x_px_left >= 1) {
    if x_px_left >= 1 {
        // x_spacing += 1;
        x_spacing += 1;
        // x_px_left -= 1;
        x_px_left -= 1;
    }
    // if (y_px_left >= 3) {
    if y_px_left >= 3 {
        // y_spacing += 1;
        y_spacing += 1;
        // y_px_left -= 3;
        y_px_left -= 3;
    }

    // Fourth, margins (“spacing”, but on the sides)
    // if (x_px_left >= 2) {
    if x_px_left >= 2 {
        // x_margin += 1;
        x_margin += 1;
        // x_px_left -= 2;
        x_px_left -= 2;
    }
    // if (y_px_left >= 2) {
    if y_px_left >= 2 {
        // y_margin += 1;
        y_margin += 1;
        // y_px_left -= 2;
        y_px_left -= 2;
    }

    // Last - increase dot width
    // if (x_px_left >= 2 and y_px_left >= 4) {
    if x_px_left >= 2 && y_px_left >= 4 {
        // w += 1;
        w += 1;
        // x_px_left -= 2;
        x_px_left -= 2;
        // y_px_left -= 4;
        y_px_left -= 4;
    }

    // assert(x_px_left <= 1 or y_px_left <= 1);
    assert(x_px_left <= 1 || y_px_left <= 1);
    // assert(2 * x_margin + 2 * w + x_spacing <= width);
    assert(2 * x_margin + 2 * w + x_spacing <= width as i32);
    // assert(2 * y_margin + 4 * w + 3 * y_spacing <= height);
    assert(2 * y_margin + 4 * w + 3 * y_spacing <= height as i32);

    // const x = [2]i32{ x_margin, x_margin + w + x_spacing };
    let x = [x_margin, x_margin + w + x_spacing];
    // const y = y: {
    //     var y: [4]i32 = undefined;
    //     y[0] = y_margin;
    //     y[1] = y[0] + w + y_spacing;
    //     y[2] = y[1] + w + y_spacing;
    //     y[3] = y[2] + w + y_spacing;
    //     break :y y;
    // };
    let y = {
        let mut y: [i32; 4] = [0; 4];
        y[0] = y_margin;
        y[1] = y[0] + w + y_spacing;
        y[2] = y[1] + w + y_spacing;
        y[3] = y[2] + w + y_spacing;
        y
    };

    // assert(cp >= 0x2800);
    assert(cp >= 0x2800);
    // assert(cp <= 0x28ff);
    assert(cp <= 0x28ff);
    // const p: Pattern = .from(cp);
    let p: Pattern = Pattern::from(cp);

    // if (p.tl) canvas.box(x[0], y[0], x[0] + w, y[0] + w, .on);
    if p.tl {
        canvas.r#box(x[0], y[0], x[0] + w, y[0] + w, Color::ON);
    }
    // if (p.ul) canvas.box(x[0], y[1], x[0] + w, y[1] + w, .on);
    if p.ul {
        canvas.r#box(x[0], y[1], x[0] + w, y[1] + w, Color::ON);
    }
    // if (p.ll) canvas.box(x[0], y[2], x[0] + w, y[2] + w, .on);
    if p.ll {
        canvas.r#box(x[0], y[2], x[0] + w, y[2] + w, Color::ON);
    }
    // if (p.bl) canvas.box(x[0], y[3], x[0] + w, y[3] + w, .on);
    if p.bl {
        canvas.r#box(x[0], y[3], x[0] + w, y[3] + w, Color::ON);
    }
    // if (p.tr) canvas.box(x[1], y[0], x[1] + w, y[0] + w, .on);
    if p.tr {
        canvas.r#box(x[1], y[0], x[1] + w, y[0] + w, Color::ON);
    }
    // if (p.ur) canvas.box(x[1], y[1], x[1] + w, y[1] + w, .on);
    if p.ur {
        canvas.r#box(x[1], y[1], x[1] + w, y[1] + w, Color::ON);
    }
    // if (p.lr) canvas.box(x[1], y[2], x[1] + w, y[2] + w, .on);
    if p.lr {
        canvas.r#box(x[1], y[2], x[1] + w, y[2] + w, Color::ON);
    }
    // if (p.br) canvas.box(x[1], y[3], x[1] + w, y[3] + w, .on);
    if p.br {
        canvas.r#box(x[1], y[3], x[1] + w, y[3] + w, Color::ON);
    }

    Ok(())
}
