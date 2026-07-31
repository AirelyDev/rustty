//! Powerline + Powerline Extra Symbols | U+E0B0...U+E0D4
//! https://github.com/ryanoasis/powerline-extra-symbols
//!
//!                
//!                  
//!      
//!
//! We implement the more geometric glyphs here, but not the stylized ones.
//!

// The reference `pub fn`s keep their camelCase names (`drawE0B0`, ...);
// preserved verbatim.
#![allow(non_snake_case)]

// const std = @import("std");
// const Allocator = std.mem.Allocator;
//
// const common = @import("common.zig");
// const Thickness = common.Thickness;
// const Shade = common.Shade;
//
// const box = @import("box.zig");
//
// const font = @import("../../main.zig");
// const Quad = font.sprite.Canvas.Quad;
//
// PORT NOTE: `common` and `box` are the sibling modules; `font = ../../main.zig`
// becomes the local crate (`crate::metrics::Metrics`, `crate::sprite::Canvas` /
// `Color` / `Triangle` / `Point`). `Allocator` is unused. `Shade` and `Quad`
// are imported in the reference but not referenced by any draw function in this
// module (colors flow through `Color.on`); they are preserved here as commented
// references rather than as unused Rust `use`s:
//   // const Shade = common.Shade;
//   // const Quad = font.sprite.Canvas.Quad;
use super::common::Thickness;
use super::r#box;
use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color, InnerStrokeError, Point, Triangle};
use rustty_z2d::options::CapMode;
use rustty_z2d::painter::{FillError, FillOptions, StrokeError, StrokeOptions};

/// 
// pub fn drawE0B0(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn drawE0B0(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // _ = cp;
    let _ = cp;
    // _ = metrics;
    let _ = metrics;
    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;
    // try canvas.triangle(.{
    //     .p0 = .{ .x = 0, .y = 0 },
    //     .p1 = .{ .x = float_width, .y = float_height / 2 },
    //     .p2 = .{ .x = 0, .y = float_height },
    // }, .on);
    canvas.triangle(
        Triangle {
            p0: Point { x: 0.0, y: 0.0 },
            p1: Point {
                x: float_width,
                y: float_height / 2.0,
            },
            p2: Point {
                x: 0.0,
                y: float_height,
            },
        },
        Color::ON,
    )
}

/// 
// pub fn drawE0B2(...) !void {
pub fn drawE0B2(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // _ = cp;
    let _ = cp;
    // _ = metrics;
    let _ = metrics;
    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;
    // try canvas.triangle(.{
    //     .p0 = .{ .x = float_width, .y = 0 },
    //     .p1 = .{ .x = 0, .y = float_height / 2 },
    //     .p2 = .{ .x = float_width, .y = float_height },
    // }, .on);
    canvas.triangle(
        Triangle {
            p0: Point {
                x: float_width,
                y: 0.0,
            },
            p1: Point {
                x: 0.0,
                y: float_height / 2.0,
            },
            p2: Point {
                x: float_width,
                y: float_height,
            },
        },
        Color::ON,
    )
}

/// 
// pub fn drawE0B8(...) !void {
pub fn drawE0B8(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // _ = cp;
    let _ = cp;
    // _ = metrics;
    let _ = metrics;
    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;
    // try canvas.triangle(.{
    //     .p0 = .{ .x = 0, .y = 0 },
    //     .p1 = .{ .x = float_width, .y = float_height },
    //     .p2 = .{ .x = 0, .y = float_height },
    // }, .on);
    canvas.triangle(
        Triangle {
            p0: Point { x: 0.0, y: 0.0 },
            p1: Point {
                x: float_width,
                y: float_height,
            },
            p2: Point {
                x: 0.0,
                y: float_height,
            },
        },
        Color::ON,
    )
}

/// 
// pub fn drawE0B9(...) !void {
pub fn drawE0B9(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;
    // _ = width;
    let _ = width;
    // _ = height;
    let _ = height;
    // box.lightDiagonalUpperLeftToLowerRight(metrics, canvas);
    r#box::lightDiagonalUpperLeftToLowerRight(metrics, canvas);
    Ok(())
}

/// 
// pub fn drawE0BA(...) !void {
pub fn drawE0BA(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // _ = cp;
    let _ = cp;
    // _ = metrics;
    let _ = metrics;
    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;
    // try canvas.triangle(.{
    //     .p0 = .{ .x = float_width, .y = 0 },
    //     .p1 = .{ .x = float_width, .y = float_height },
    //     .p2 = .{ .x = 0, .y = float_height },
    // }, .on);
    canvas.triangle(
        Triangle {
            p0: Point {
                x: float_width,
                y: 0.0,
            },
            p1: Point {
                x: float_width,
                y: float_height,
            },
            p2: Point {
                x: 0.0,
                y: float_height,
            },
        },
        Color::ON,
    )
}

/// 
// pub fn drawE0BB(...) !void {
pub fn drawE0BB(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;
    // _ = width;
    let _ = width;
    // _ = height;
    let _ = height;
    // box.lightDiagonalUpperRightToLowerLeft(metrics, canvas);
    r#box::lightDiagonalUpperRightToLowerLeft(metrics, canvas);
    Ok(())
}

/// 
// pub fn drawE0BC(...) !void {
pub fn drawE0BC(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // _ = cp;
    let _ = cp;
    // _ = metrics;
    let _ = metrics;
    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;
    // try canvas.triangle(.{
    //     .p0 = .{ .x = 0, .y = 0 },
    //     .p1 = .{ .x = float_width, .y = 0 },
    //     .p2 = .{ .x = 0, .y = float_height },
    // }, .on);
    canvas.triangle(
        Triangle {
            p0: Point { x: 0.0, y: 0.0 },
            p1: Point {
                x: float_width,
                y: 0.0,
            },
            p2: Point {
                x: 0.0,
                y: float_height,
            },
        },
        Color::ON,
    )
}

/// 
// pub fn drawE0BD(...) !void {
pub fn drawE0BD(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;
    // _ = width;
    let _ = width;
    // _ = height;
    let _ = height;
    // box.lightDiagonalUpperRightToLowerLeft(metrics, canvas);
    r#box::lightDiagonalUpperRightToLowerLeft(metrics, canvas);
    Ok(())
}

/// 
// pub fn drawE0BE(...) !void {
pub fn drawE0BE(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // _ = cp;
    let _ = cp;
    // _ = metrics;
    let _ = metrics;
    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;
    // try canvas.triangle(.{
    //     .p0 = .{ .x = 0, .y = 0 },
    //     .p1 = .{ .x = float_width, .y = 0 },
    //     .p2 = .{ .x = float_width, .y = float_height },
    // }, .on);
    canvas.triangle(
        Triangle {
            p0: Point { x: 0.0, y: 0.0 },
            p1: Point {
                x: float_width,
                y: 0.0,
            },
            p2: Point {
                x: float_width,
                y: float_height,
            },
        },
        Color::ON,
    )
}

/// 
// pub fn drawE0BF(...) !void {
pub fn drawE0BF(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;
    // _ = width;
    let _ = width;
    // _ = height;
    let _ = height;
    // box.lightDiagonalUpperLeftToLowerRight(metrics, canvas);
    r#box::lightDiagonalUpperLeftToLowerRight(metrics, canvas);
    Ok(())
}

/// 
// pub fn drawE0B1(...) !void {
pub fn drawE0B1(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), StrokeError> {
    // _ = cp;
    let _ = cp;
    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;

    // var path = canvas.staticPath(3);
    let mut path = canvas.staticPath::<3>();
    // path.moveTo(0, 0);
    path.move_to(0.0, 0.0);
    // path.lineTo(float_width, float_height / 2);
    path.line_to(float_width, float_height / 2.0);
    // path.lineTo(0, float_height);
    path.line_to(0.0, float_height);

    // try canvas.strokePath(
    //     path.wrapped_path,
    //     .{
    //         .line_cap_mode = .butt,
    //         .line_width = @floatFromInt(
    //             Thickness.light.height(metrics.box_thickness),
    //         ),
    //     },
    //     .on,
    // );
    canvas.strokePath(
        &path.wrapped_path,
        StrokeOptions {
            line_cap_mode: CapMode::Butt,
            line_width: Thickness::Light.height(metrics.box_thickness) as f64,
            ..Default::default()
        },
        Color::ON,
    )
}

/// 
// pub fn drawE0B3(...) !void {
pub fn drawE0B3(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), StrokeError> {
    // try drawE0B1(cp, canvas, width, height, metrics);
    drawE0B1(cp, canvas, width, height, metrics)?;
    // try canvas.flipHorizontal();
    canvas.flipHorizontal();
    Ok(())
}

/// 
// pub fn drawE0B4(...) !void {
pub fn drawE0B4(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // _ = cp;
    let _ = cp;
    // _ = metrics;
    let _ = metrics;

    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;

    // Coefficient for approximating a circular arc.
    // const c: f64 = (std.math.sqrt2 - 1.0) * 4.0 / 3.0;
    let c: f64 = (std::f64::consts::SQRT_2 - 1.0) * 4.0 / 3.0;

    // const radius: f64 = @min(float_width, float_height / 2);
    let radius: f64 = float_width.min(float_height / 2.0);

    // var path = canvas.staticPath(6);
    let mut path = canvas.staticPath::<6>();
    // path.moveTo(0, 0);
    path.move_to(0.0, 0.0);
    // path.curveTo(
    //     radius * c,
    //     0,
    //     radius,
    //     radius - radius * c,
    //     radius,
    //     radius,
    // );
    path.curve_to(radius * c, 0.0, radius, radius - radius * c, radius, radius);
    // path.lineTo(radius, float_height - radius);
    path.line_to(radius, float_height - radius);
    // path.curveTo(
    //     radius,
    //     float_height - radius + radius * c,
    //     radius * c,
    //     float_height,
    //     0,
    //     float_height,
    // );
    path.curve_to(
        radius,
        float_height - radius + radius * c,
        radius * c,
        float_height,
        0.0,
        float_height,
    );
    // path.close();
    path.close();

    // try canvas.fillPath(path.wrapped_path, .{}, .on);
    canvas.fillPath(&path.wrapped_path, FillOptions::default(), Color::ON)
}

/// 
// pub fn drawE0B5(...) !void {
pub fn drawE0B5(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), InnerStrokeError> {
    // _ = cp;
    let _ = cp;

    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;

    // Coefficient for approximating a circular arc.
    // const c: f64 = (std.math.sqrt2 - 1.0) * 4.0 / 3.0;
    let c: f64 = (std::f64::consts::SQRT_2 - 1.0) * 4.0 / 3.0;

    // const radius: f64 = @min(float_width, float_height / 2);
    let radius: f64 = float_width.min(float_height / 2.0);

    // Note on the first/final lineTo moves: this is to enforce horizontal
    // lines on both ends. This ensures that when the path is offset and
    // stroked, the ends are butt-capped entirely perpendicularly; curveTo
    // lerping would produce very slight slopes otherwise.
    // var path = canvas.staticPath(6);
    let mut path = canvas.staticPath::<6>();
    // path.moveTo(0, 0);
    path.move_to(0.0, 0.0);
    // path.lineTo(1, 0);
    path.line_to(1.0, 0.0);
    // path.curveTo(
    //     radius * c,
    //     0,
    //     radius,
    //     radius - radius * c,
    //     radius,
    //     radius,
    // );
    path.curve_to(radius * c, 0.0, radius, radius - radius * c, radius, radius);
    // path.lineTo(radius, float_height - radius);
    path.line_to(radius, float_height - radius);
    // path.curveTo(
    //     radius,
    //     float_height - radius + radius * c,
    //     radius * c,
    //     float_height,
    //     1,
    //     float_height,
    // );
    path.curve_to(
        radius,
        float_height - radius + radius * c,
        radius * c,
        float_height,
        1.0,
        float_height,
    );
    // path.lineTo(0, float_height);
    path.line_to(0.0, float_height);

    // try canvas.innerStrokePath(path.wrapped_path, .{
    //     .line_width = @floatFromInt(metrics.box_thickness),
    //     .line_cap_mode = .butt,
    // }, .on);
    canvas.innerStrokePath(
        &path.wrapped_path,
        StrokeOptions {
            line_width: metrics.box_thickness as f64,
            line_cap_mode: CapMode::Butt,
            ..Default::default()
        },
        Color::ON,
    )
}

/// 
// pub fn drawE0B6(...) !void {
pub fn drawE0B6(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // try drawE0B4(cp, canvas, width, height, metrics);
    drawE0B4(cp, canvas, width, height, metrics)?;
    // try canvas.flipHorizontal();
    canvas.flipHorizontal();
    Ok(())
}

/// 
// pub fn drawE0B7(...) !void {
pub fn drawE0B7(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), InnerStrokeError> {
    // try drawE0B5(cp, canvas, width, height, metrics);
    drawE0B5(cp, canvas, width, height, metrics)?;
    // try canvas.flipHorizontal();
    canvas.flipHorizontal();
    Ok(())
}

/// 
// pub fn drawE0D2(...) !void {
pub fn drawE0D2(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // _ = cp;
    let _ = cp;

    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;
    // const float_thick: f64 = @floatFromInt(metrics.box_thickness);
    let float_thick: f64 = metrics.box_thickness as f64;

    // Top piece
    {
        // var path = canvas.staticPath(6);
        let mut path = canvas.staticPath::<6>();
        // path.moveTo(0, 0);
        path.move_to(0.0, 0.0);
        // path.lineTo(float_width, 0);
        path.line_to(float_width, 0.0);
        // path.lineTo(float_width / 2, float_height / 2 - float_thick / 2);
        path.line_to(float_width / 2.0, float_height / 2.0 - float_thick / 2.0);
        // path.lineTo(0, float_height / 2 - float_thick / 2);
        path.line_to(0.0, float_height / 2.0 - float_thick / 2.0);
        // path.close();
        path.close();

        // try canvas.fillPath(path.wrapped_path, .{}, .on);
        canvas.fillPath(&path.wrapped_path, FillOptions::default(), Color::ON)?;
    }

    // Bottom piece
    {
        // var path = canvas.staticPath(6);
        let mut path = canvas.staticPath::<6>();
        // path.moveTo(0, float_height);
        path.move_to(0.0, float_height);
        // path.lineTo(float_width, float_height);
        path.line_to(float_width, float_height);
        // path.lineTo(float_width / 2, float_height / 2 + float_thick / 2);
        path.line_to(float_width / 2.0, float_height / 2.0 + float_thick / 2.0);
        // path.lineTo(0, float_height / 2 + float_thick / 2);
        path.line_to(0.0, float_height / 2.0 + float_thick / 2.0);
        // path.close();
        path.close();

        // try canvas.fillPath(path.wrapped_path, .{}, .on);
        canvas.fillPath(&path.wrapped_path, FillOptions::default(), Color::ON)?;
    }

    Ok(())
}

/// 
// pub fn drawE0D4(...) !void {
pub fn drawE0D4(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), FillError> {
    // try drawE0D2(cp, canvas, width, height, metrics);
    drawE0D2(cp, canvas, width, height, metrics)?;
    // try canvas.flipHorizontal();
    canvas.flipHorizontal();
    Ok(())
}
