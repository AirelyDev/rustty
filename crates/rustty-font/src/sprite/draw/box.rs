//! Box Drawing | U+2500...U+257F
//! https://en.wikipedia.org/wiki/Box_Drawing
//!
//! ─━│┃┄┅┆┇┈┉┊┋┌┍┎┏
//! ┐┑┒┓└┕┖┗┘┙┚┛├┝┞┟
//! ┠┡┢┣┤┥┦┧┨┩┪┫┬┭┮┯
//! ┰┱┲┳┴┵┶┷┸┹┺┻┼┽┾┿
//! ╀╁╂╃╄╅╆╇╈╉╊╋╌╍╎╏
//! ═║╒╓╔╕╖╗╘╙╚╛╜╝╞╟
//! ╠╡╢╣╤╥╦╧╨╩╪╫╬╭╮╯
//! ╰╱╲╳╴╵╶╷╸╹╺╻╼╽╾╿
//!
//
// PORT STATUS: box.zig is the largest sprite-draw module (929 lines) and is
// ported in its own dedicated batch. THIS file is an incremental, INFRASTRUCTURE
// subset: it carries only the three public helpers that the already-ported
// sibling draw modules import cross-module —
//   - `lightDiagonalUpperRightToLowerLeft` / `lightDiagonalUpperLeftToLowerRight`
//     (imported by `powerline.zig` as `box.lightDiagonal…`)
// plus the shared module header. The remaining box drawing functions
// (`arc`, the intersection-line machinery, dashes, etc.) are DEFERRED to the
// box.zig batch, at which point this file is completed to a full-fidelity port.
// Every line/comment that IS ported here is verbatim per R1.

// The reference `pub fn`s keep their camelCase names; preserved verbatim.
#![allow(non_snake_case)]

// const std = @import("std");
// const assert = @import("../../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
//
// const common = @import("common.zig");
// const Thickness = common.Thickness;
// ...
//
// const font = @import("../../main.zig");
//
// PORT NOTE: only the imports used by the ported subset are wired here; the full
// import set (`Shade`, `Quads`, `Corner`, `Edge`, `Alignment`, `hline`/`vline`/
// `hlineMiddle`/`vlineMiddle`, `assert`) lands with the box.zig batch.
use super::common::Thickness;
use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color, Line, Point};

// pub fn lightDiagonalUpperRightToLowerLeft(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
// ) void {
pub fn lightDiagonalUpperRightToLowerLeft(metrics: Metrics, canvas: &mut Canvas) {
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;

    // We overshoot the corners by a tiny bit, but we need to
    // maintain the correct slope, so we calculate that here.
    // const slope_x: f64 = @min(1.0, float_width / float_height);
    let slope_x: f64 = 1.0_f64.min(float_width / float_height);
    // const slope_y: f64 = @min(1.0, float_height / float_width);
    let slope_y: f64 = 1.0_f64.min(float_height / float_width);

    // canvas.line(.{
    //     .p0 = .{
    //         .x = float_width + 0.5 * slope_x,
    //         .y = -0.5 * slope_y,
    //     },
    //     .p1 = .{
    //         .x = -0.5 * slope_x,
    //         .y = float_height + 0.5 * slope_y,
    //     },
    // }, @floatFromInt(Thickness.light.height(metrics.box_thickness)), .on) catch {};
    let _ = canvas.line(
        Line {
            p0: Point {
                x: float_width + 0.5 * slope_x,
                y: -0.5 * slope_y,
            },
            p1: Point {
                x: -0.5 * slope_x,
                y: float_height + 0.5 * slope_y,
            },
        },
        Thickness::Light.height(metrics.box_thickness) as f64,
        Color::ON,
    );
}

// pub fn lightDiagonalUpperLeftToLowerRight(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
// ) void {
pub fn lightDiagonalUpperLeftToLowerRight(metrics: Metrics, canvas: &mut Canvas) {
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;

    // We overshoot the corners by a tiny bit, but we need to
    // maintain the correct slope, so we calculate that here.
    // const slope_x: f64 = @min(1.0, float_width / float_height);
    let slope_x: f64 = 1.0_f64.min(float_width / float_height);
    // const slope_y: f64 = @min(1.0, float_height / float_width);
    let slope_y: f64 = 1.0_f64.min(float_height / float_width);

    // canvas.line(.{
    //     .p0 = .{
    //         .x = -0.5 * slope_x,
    //         .y = -0.5 * slope_y,
    //     },
    //     .p1 = .{
    //         .x = float_width + 0.5 * slope_x,
    //         .y = float_height + 0.5 * slope_y,
    //     },
    // }, @floatFromInt(Thickness.light.height(metrics.box_thickness)), .on) catch {};
    let _ = canvas.line(
        Line {
            p0: Point {
                x: -0.5 * slope_x,
                y: -0.5 * slope_y,
            },
            p1: Point {
                x: float_width + 0.5 * slope_x,
                y: float_height + 0.5 * slope_y,
            },
        },
        Thickness::Light.height(metrics.box_thickness) as f64,
        Color::ON,
    );
}
