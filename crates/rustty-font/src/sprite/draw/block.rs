//! Block Elements | U+2580...U+259F
//! https://en.wikipedia.org/wiki/Block_Elements
//!
//! ▀▁▂▃▄▅▆▇█▉▊▋▌▍▎▏
//! ▐░▒▓▔▕▖▗▘▙▚▛▜▝▞▟
//!

// The reference `pub fn`s keep their camelCase names (`draw2580_259F`,
// `blockShade`, `fullBlockShade`); the reference constant names below
// (`one_eighth`, etc.) are lowercase. Preserved verbatim rather than renamed.
#![allow(non_snake_case, non_upper_case_globals)]

// const std = @import("std");
// const Allocator = std.mem.Allocator;
//
// const common = @import("common.zig");
// const Shade = common.Shade;
// const Quads = common.Quads;
// const Alignment = common.Alignment;
// const fill = common.fill;
//
// const font = @import("../../main.zig");
//
// PORT NOTE: `common` is the sibling `super::common` module; `font =
// ../../main.zig` becomes the local crate (`crate::metrics::Metrics`,
// `crate::sprite::Canvas` / `Color`). `Allocator` is unused in this module.
use super::common::{fill, Alignment, Fraction, Quads, Shade};
use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color, Rect};

// Utility names for common fractions
// const one_eighth: f64 = 0.125;
const one_eighth: f64 = 0.125;
// const one_quarter: f64 = 0.25;
const one_quarter: f64 = 0.25;
// const one_third: f64 = (1.0 / 3.0);
//
// PORT NOTE: `one_third`/`two_thirds` are declared in the reference's set of
// "utility names for common fractions" but are not referenced by any prong of
// `draw2580_259F` (block elements have no thirds). Zig permits an unused
// file-level `const`; the Rust equivalent needs `#[allow(dead_code)]`.
#[allow(dead_code)]
const one_third: f64 = 1.0 / 3.0;
// const three_eighths: f64 = 0.375;
const three_eighths: f64 = 0.375;
// const half: f64 = 0.5;
const half: f64 = 0.5;
// const five_eighths: f64 = 0.625;
const five_eighths: f64 = 0.625;
// const two_thirds: f64 = (2.0 / 3.0);
#[allow(dead_code)]
const two_thirds: f64 = 2.0 / 3.0;
// const three_quarters: f64 = 0.75;
const three_quarters: f64 = 0.75;
// const seven_eighths: f64 = 0.875;
const seven_eighths: f64 = 0.875;

// pub fn draw2580_259F(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
#[allow(non_snake_case)]
pub fn draw2580_259F(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = width;
    let _ = width;
    // _ = height;
    let _ = height;

    // switch (cp) {
    match cp {
        // '▀' UPPER HALF BLOCK
        // 0x2580 => block(metrics, canvas, .upper, 1, half),
        0x2580 => block(metrics, canvas, Alignment::UPPER, 1.0, half),
        // '▁' LOWER ONE EIGHTH BLOCK
        // 0x2581 => block(metrics, canvas, .lower, 1, one_eighth),
        0x2581 => block(metrics, canvas, Alignment::LOWER, 1.0, one_eighth),
        // '▂' LOWER ONE QUARTER BLOCK
        // 0x2582 => block(metrics, canvas, .lower, 1, one_quarter),
        0x2582 => block(metrics, canvas, Alignment::LOWER, 1.0, one_quarter),
        // '▃' LOWER THREE EIGHTHS BLOCK
        // 0x2583 => block(metrics, canvas, .lower, 1, three_eighths),
        0x2583 => block(metrics, canvas, Alignment::LOWER, 1.0, three_eighths),
        // '▄' LOWER HALF BLOCK
        // 0x2584 => block(metrics, canvas, .lower, 1, half),
        0x2584 => block(metrics, canvas, Alignment::LOWER, 1.0, half),
        // '▅' LOWER FIVE EIGHTHS BLOCK
        // 0x2585 => block(metrics, canvas, .lower, 1, five_eighths),
        0x2585 => block(metrics, canvas, Alignment::LOWER, 1.0, five_eighths),
        // '▆' LOWER THREE QUARTERS BLOCK
        // 0x2586 => block(metrics, canvas, .lower, 1, three_quarters),
        0x2586 => block(metrics, canvas, Alignment::LOWER, 1.0, three_quarters),
        // '▇' LOWER SEVEN EIGHTHS BLOCK
        // 0x2587 => block(metrics, canvas, .lower, 1, seven_eighths),
        0x2587 => block(metrics, canvas, Alignment::LOWER, 1.0, seven_eighths),
        // '█' FULL BLOCK
        // 0x2588 => fullBlockShade(metrics, canvas, .on),
        0x2588 => fullBlockShade(metrics, canvas, Shade::ON),
        // '▉' LEFT SEVEN EIGHTHS BLOCK
        // 0x2589 => block(metrics, canvas, .left, seven_eighths, 1),
        0x2589 => block(metrics, canvas, Alignment::LEFT, seven_eighths, 1.0),
        // '▊' LEFT THREE QUARTERS BLOCK
        // 0x258a => block(metrics, canvas, .left, three_quarters, 1),
        0x258a => block(metrics, canvas, Alignment::LEFT, three_quarters, 1.0),
        // '▋' LEFT FIVE EIGHTHS BLOCK
        // 0x258b => block(metrics, canvas, .left, five_eighths, 1),
        0x258b => block(metrics, canvas, Alignment::LEFT, five_eighths, 1.0),
        // '▌' LEFT HALF BLOCK
        // 0x258c => block(metrics, canvas, .left, half, 1),
        0x258c => block(metrics, canvas, Alignment::LEFT, half, 1.0),
        // '▍' LEFT THREE EIGHTHS BLOCK
        // 0x258d => block(metrics, canvas, .left, three_eighths, 1),
        0x258d => block(metrics, canvas, Alignment::LEFT, three_eighths, 1.0),
        // '▎' LEFT ONE QUARTER BLOCK
        // 0x258e => block(metrics, canvas, .left, one_quarter, 1),
        0x258e => block(metrics, canvas, Alignment::LEFT, one_quarter, 1.0),
        // '▏' LEFT ONE EIGHTH BLOCK
        // 0x258f => block(metrics, canvas, .left, one_eighth, 1),
        0x258f => block(metrics, canvas, Alignment::LEFT, one_eighth, 1.0),

        // '▐' RIGHT HALF BLOCK
        // 0x2590 => block(metrics, canvas, .right, half, 1),
        0x2590 => block(metrics, canvas, Alignment::RIGHT, half, 1.0),
        // '░'
        // 0x2591 => fullBlockShade(metrics, canvas, .light),
        0x2591 => fullBlockShade(metrics, canvas, Shade::LIGHT),
        // '▒'
        // 0x2592 => fullBlockShade(metrics, canvas, .medium),
        0x2592 => fullBlockShade(metrics, canvas, Shade::MEDIUM),
        // '▓'
        // 0x2593 => fullBlockShade(metrics, canvas, .dark),
        0x2593 => fullBlockShade(metrics, canvas, Shade::DARK),
        // '▔' UPPER ONE EIGHTH BLOCK
        // 0x2594 => block(metrics, canvas, .upper, 1, one_eighth),
        0x2594 => block(metrics, canvas, Alignment::UPPER, 1.0, one_eighth),
        // '▕' RIGHT ONE EIGHTH BLOCK
        // 0x2595 => block(metrics, canvas, .right, one_eighth, 1),
        0x2595 => block(metrics, canvas, Alignment::RIGHT, one_eighth, 1.0),
        // '▖'
        // 0x2596 => quadrant(metrics, canvas, .{ .bl = true }),
        0x2596 => quadrant(
            metrics,
            canvas,
            Quads {
                bl: true,
                ..Default::default()
            },
        ),
        // '▗'
        // 0x2597 => quadrant(metrics, canvas, .{ .br = true }),
        0x2597 => quadrant(
            metrics,
            canvas,
            Quads {
                br: true,
                ..Default::default()
            },
        ),
        // '▘'
        // 0x2598 => quadrant(metrics, canvas, .{ .tl = true }),
        0x2598 => quadrant(
            metrics,
            canvas,
            Quads {
                tl: true,
                ..Default::default()
            },
        ),
        // '▙'
        // 0x2599 => quadrant(metrics, canvas, .{ .tl = true, .bl = true, .br = true }),
        0x2599 => quadrant(
            metrics,
            canvas,
            Quads {
                tl: true,
                bl: true,
                br: true,
                ..Default::default()
            },
        ),
        // '▚'
        // 0x259a => quadrant(metrics, canvas, .{ .tl = true, .br = true }),
        0x259a => quadrant(
            metrics,
            canvas,
            Quads {
                tl: true,
                br: true,
                ..Default::default()
            },
        ),
        // '▛'
        // 0x259b => quadrant(metrics, canvas, .{ .tl = true, .tr = true, .bl = true }),
        0x259b => quadrant(
            metrics,
            canvas,
            Quads {
                tl: true,
                tr: true,
                bl: true,
                ..Default::default()
            },
        ),
        // '▜'
        // 0x259c => quadrant(metrics, canvas, .{ .tl = true, .tr = true, .br = true }),
        0x259c => quadrant(
            metrics,
            canvas,
            Quads {
                tl: true,
                tr: true,
                br: true,
                ..Default::default()
            },
        ),
        // '▝'
        // 0x259d => quadrant(metrics, canvas, .{ .tr = true }),
        0x259d => quadrant(
            metrics,
            canvas,
            Quads {
                tr: true,
                ..Default::default()
            },
        ),
        // '▞'
        // 0x259e => quadrant(metrics, canvas, .{ .tr = true, .bl = true }),
        0x259e => quadrant(
            metrics,
            canvas,
            Quads {
                tr: true,
                bl: true,
                ..Default::default()
            },
        ),
        // '▟'
        // 0x259f => quadrant(metrics, canvas, .{ .tr = true, .bl = true, .br = true }),
        0x259f => quadrant(
            metrics,
            canvas,
            Quads {
                tr: true,
                bl: true,
                br: true,
                ..Default::default()
            },
        ),

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

// pub fn block(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime alignment: Alignment,
//     comptime width: f64,
//     comptime height: f64,
// ) void {
pub fn block(metrics: Metrics, canvas: &mut Canvas, alignment: Alignment, width: f64, height: f64) {
    // blockShade(metrics, canvas, alignment, width, height, .on);
    blockShade(metrics, canvas, alignment, width, height, Shade::ON);
}

// pub fn blockShade(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime alignment: Alignment,
//     comptime width: f64,
//     comptime height: f64,
//     comptime shade: Shade,
// ) void {
#[allow(non_snake_case)]
pub fn blockShade(
    metrics: Metrics,
    canvas: &mut Canvas,
    alignment: Alignment,
    width: f64,
    height: f64,
    shade: Shade,
) {
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;

    // const w: u32 = @intFromFloat(@round(float_width * width));
    let w: u32 = (float_width * width).round() as u32;
    // const h: u32 = @intFromFloat(@round(float_height * height));
    let h: u32 = (float_height * height).round() as u32;

    // const x = switch (alignment.horizontal) {
    //     .left => 0,
    //     .right => metrics.cell_width - w,
    //     .center => (metrics.cell_width - w) / 2,
    // };
    let x: u32 = match alignment.horizontal {
        super::common::Horizontal::Left => 0,
        super::common::Horizontal::Right => metrics.cell_width - w,
        super::common::Horizontal::Center => (metrics.cell_width - w) / 2,
    };
    // const y = switch (alignment.vertical) {
    //     .top => 0,
    //     .bottom => metrics.cell_height - h,
    //     .middle => (metrics.cell_height - h) / 2,
    // };
    let y: u32 = match alignment.vertical {
        super::common::Vertical::Top => 0,
        super::common::Vertical::Bottom => metrics.cell_height - h,
        super::common::Vertical::Middle => (metrics.cell_height - h) / 2,
    };

    // canvas.rect(.{
    //     .x = @intCast(x),
    //     .y = @intCast(y),
    //     .width = @intCast(w),
    //     .height = @intCast(h),
    // }, @as(font.sprite.Color, @enumFromInt(@intFromEnum(shade))));
    canvas.rect(
        Rect {
            x: x as i32,
            y: y as i32,
            width: w as i32,
            height: h as i32,
        },
        Color(shade.0),
    );
}

// pub fn fullBlockShade(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     shade: Shade,
// ) void {
#[allow(non_snake_case)]
pub fn fullBlockShade(metrics: Metrics, canvas: &mut Canvas, shade: Shade) {
    // canvas.box(
    //     0,
    //     0,
    //     @intCast(metrics.cell_width),
    //     @intCast(metrics.cell_height),
    //     @as(font.sprite.Color, @enumFromInt(@intFromEnum(shade))),
    // );
    canvas.r#box(
        0,
        0,
        metrics.cell_width as i32,
        metrics.cell_height as i32,
        Color(shade.0),
    );
}

// fn quadrant(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime quads: Quads,
// ) void {
fn quadrant(metrics: Metrics, canvas: &mut Canvas, quads: Quads) {
    // if (quads.tl) fill(metrics, canvas, .zero, .half, .zero, .half);
    if quads.tl {
        fill(
            metrics,
            canvas,
            Fraction::Zero,
            Fraction::Half,
            Fraction::Zero,
            Fraction::Half,
        );
    }
    // if (quads.tr) fill(metrics, canvas, .half, .full, .zero, .half);
    if quads.tr {
        fill(
            metrics,
            canvas,
            Fraction::Half,
            Fraction::Full,
            Fraction::Zero,
            Fraction::Half,
        );
    }
    // if (quads.bl) fill(metrics, canvas, .zero, .half, .half, .full);
    if quads.bl {
        fill(
            metrics,
            canvas,
            Fraction::Zero,
            Fraction::Half,
            Fraction::Half,
            Fraction::Full,
        );
    }
    // if (quads.br) fill(metrics, canvas, .half, .full, .half, .full);
    if quads.br {
        fill(
            metrics,
            canvas,
            Fraction::Half,
            Fraction::Full,
            Fraction::Half,
            Fraction::Full,
        );
    }
}
