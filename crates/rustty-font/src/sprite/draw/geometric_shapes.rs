//! Geometric Shapes | U+25A0...U+25FF
//! https://en.wikipedia.org/wiki/Geometric_Shapes_(Unicode_block)
//!
//! ■ □ ▢ ▣ ▤ ▥ ▦ ▧ ▨ ▩ ▪ ▫ ▬ ▭ ▮ ▯
//! ▰ ▱ ▲ △ ▴ ▵ ▶ ▷ ▸ ▹ ► ▻ ▼ ▽ ▾ ▿
//! ◀ ◁ ◂ ◃ ◄ ◅ ◆ ◇ ◈ ◉ ◊ ○ ◌ ◍ ◎ ●
//! ◐ ◑ ◒ ◓ ◔ ◕ ◖ ◗ ◘ ◙ ◚ ◛ ◜ ◝ ◞ ◟
//! ◠ ◡ ◢ ◣ ◤ ◥ ◦ ◧ ◨ ◩ ◪ ◫ ◬ ◭ ◮ ◯
//! ◰ ◱ ◲ ◳ ◴ ◵ ◶ ◷ ◸ ◹ ◺ ◻ ◼ ◽︎◾︎◿
//!
//! Only a subset of this block is viable for sprite drawing; filling
//! out this file to have full coverage of this block is not the goal.
//!

// The reference `pub fn`s keep their camelCase names (`draw25E2_25E5`,
// `cornerTriangleShade`, `cornerTriangleOutline`); preserved verbatim.
#![allow(non_snake_case)]

// const std = @import("std");
// const Allocator = std.mem.Allocator;
//
// const common = @import("common.zig");
// const Thickness = common.Thickness;
// const Corner = common.Corner;
// const Shade = common.Shade;
//
// const font = @import("../../main.zig");
//
// PORT NOTE: `common` is the sibling `super::common` module; `font =
// ../../main.zig` becomes the local crate (`crate::metrics::Metrics`,
// `crate::sprite::Canvas` / `Color`). `Allocator` is unused in this module.
// The z2d painter option/error types are pulled from `rustty-z2d` (the same
// `Canvas` uses) plus the canvas-local `InnerStrokeError`.
use super::common::{Corner, Shade, Thickness};
use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color, InnerStrokeError};
use rustty_z2d::options::CapMode;
use rustty_z2d::painter::{FillError, FillOptions, StrokeOptions};

/// ◢ ◣ ◤ ◥
// pub fn draw25E2_25E5(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw25E2_25E5(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // _ = width;
    let _ = width;
    // _ = height;
    let _ = height;
    // switch (cp) {
    match cp {
        // ◢
        // 0x25e2 => try cornerTriangleShade(metrics, canvas, .br, .on),
        0x25e2 => cornerTriangleShade(metrics, canvas, Corner::Br, Shade::ON)?,
        // ◣
        // 0x25e3 => try cornerTriangleShade(metrics, canvas, .bl, .on),
        0x25e3 => cornerTriangleShade(metrics, canvas, Corner::Bl, Shade::ON)?,
        // ◤
        // 0x25e4 => try cornerTriangleShade(metrics, canvas, .tl, .on),
        0x25e4 => cornerTriangleShade(metrics, canvas, Corner::Tl, Shade::ON)?,
        // ◥
        // 0x25e5 => try cornerTriangleShade(metrics, canvas, .tr, .on),
        0x25e5 => cornerTriangleShade(metrics, canvas, Corner::Tr, Shade::ON)?,

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

/// ◸ ◹ ◺
// pub fn draw25F8_25FA(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw25F8_25FA(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), InnerStrokeError> {
    // _ = width;
    let _ = width;
    // _ = height;
    let _ = height;
    // switch (cp) {
    match cp {
        // ◸
        // 0x25f8 => try cornerTriangleOutline(metrics, canvas, .tl),
        0x25f8 => cornerTriangleOutline(metrics, canvas, Corner::Tl)?,
        // ◹
        // 0x25f9 => try cornerTriangleOutline(metrics, canvas, .tr),
        0x25f9 => cornerTriangleOutline(metrics, canvas, Corner::Tr)?,
        // ◺
        // 0x25fa => try cornerTriangleOutline(metrics, canvas, .bl),
        0x25fa => cornerTriangleOutline(metrics, canvas, Corner::Bl)?,

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

/// ◿
// pub fn draw25FF(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw25FF(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), InnerStrokeError> {
    // _ = cp;
    let _ = cp;
    // _ = width;
    let _ = width;
    // _ = height;
    let _ = height;
    // try cornerTriangleOutline(metrics, canvas, .br);
    cornerTriangleOutline(metrics, canvas, Corner::Br)
}

// pub fn cornerTriangleShade(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime corner: Corner,
//     comptime shade: Shade,
// ) !void {
pub fn cornerTriangleShade(
    metrics: Metrics,
    canvas: &mut Canvas,
    corner: Corner,
    shade: Shade,
) -> Result<(), FillError> {
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;

    // const x0, const y0, const x1, const y1, const x2, const y2 =
    //     switch (corner) { ... };
    let (x0, y0, x1, y1, x2, y2): (f64, f64, f64, f64, f64, f64) = match corner {
        // .tl => .{ 0, 0, 0, float_height, float_width, 0 },
        Corner::Tl => (0.0, 0.0, 0.0, float_height, float_width, 0.0),
        // .tr => .{ 0, 0, float_width, float_height, float_width, 0 },
        Corner::Tr => (0.0, 0.0, float_width, float_height, float_width, 0.0),
        // .bl => .{ 0, 0, 0, float_height, float_width, float_height },
        Corner::Bl => (0.0, 0.0, 0.0, float_height, float_width, float_height),
        // .br => .{ 0, float_height, float_width, float_height, float_width, 0 },
        Corner::Br => (
            0.0,
            float_height,
            float_width,
            float_height,
            float_width,
            0.0,
        ),
    };

    // var path = canvas.staticPath(5); // nodes.len = 0
    let mut path = canvas.staticPath::<5>(); // nodes.len = 0
                                             // path.moveTo(x0, y0); // +1, nodes.len = 1
    path.move_to(x0, y0); // +1, nodes.len = 1
                          // path.lineTo(x1, y1); // +1, nodes.len = 2
    path.line_to(x1, y1); // +1, nodes.len = 2
                          // path.lineTo(x2, y2); // +1, nodes.len = 3
    path.line_to(x2, y2); // +1, nodes.len = 3
                          // path.close(); // +2, nodes.len = 5
    path.close(); // +2, nodes.len = 5

    // try canvas.fillPath(
    //     path.wrapped_path,
    //     .{},
    //     @enumFromInt(@intFromEnum(shade)),
    // );
    canvas.fillPath(&path.wrapped_path, FillOptions::default(), Color(shade.0))
}

// pub fn cornerTriangleOutline(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime corner: Corner,
// ) !void {
pub fn cornerTriangleOutline(
    metrics: Metrics,
    canvas: &mut Canvas,
    corner: Corner,
) -> Result<(), InnerStrokeError> {
    // const float_thick: f64 = @floatFromInt(Thickness.light.height(metrics.box_thickness));
    let float_thick: f64 = Thickness::Light.height(metrics.box_thickness) as f64;
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;

    // const x0, const y0, const x1, const y1, const x2, const y2 =
    //     switch (corner) { ... };
    let (x0, y0, x1, y1, x2, y2): (f64, f64, f64, f64, f64, f64) = match corner {
        // .tl => .{ 0, 0, 0, float_height, float_width, 0 },
        Corner::Tl => (0.0, 0.0, 0.0, float_height, float_width, 0.0),
        // .tr => .{ 0, 0, float_width, float_height, float_width, 0 },
        Corner::Tr => (0.0, 0.0, float_width, float_height, float_width, 0.0),
        // .bl => .{ 0, 0, 0, float_height, float_width, float_height },
        Corner::Bl => (0.0, 0.0, 0.0, float_height, float_width, float_height),
        // .br => .{ 0, float_height, float_width, float_height, float_width, 0 },
        Corner::Br => (
            0.0,
            float_height,
            float_width,
            float_height,
            float_width,
            0.0,
        ),
    };

    // var path = canvas.staticPath(5); // nodes.len = 0
    let mut path = canvas.staticPath::<5>(); // nodes.len = 0
                                             // path.moveTo(x0, y0); // +1, nodes.len = 1
    path.move_to(x0, y0); // +1, nodes.len = 1
                          // path.lineTo(x1, y1); // +1, nodes.len = 2
    path.line_to(x1, y1); // +1, nodes.len = 2
                          // path.lineTo(x2, y2); // +1, nodes.len = 3
    path.line_to(x2, y2); // +1, nodes.len = 3
                          // path.close(); // +2, nodes.len = 5
    path.close(); // +2, nodes.len = 5

    // try canvas.innerStrokePath(path.wrapped_path, .{
    //     .line_cap_mode = .butt,
    //     .line_width = float_thick,
    // }, .on);
    canvas.innerStrokePath(
        &path.wrapped_path,
        StrokeOptions {
            line_cap_mode: CapMode::Butt,
            line_width: float_thick,
            ..Default::default()
        },
        Color::ON,
    )
}
