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

// The reference `pub fn`s keep their camelCase names; preserved verbatim.
#![allow(non_snake_case)]

// const std = @import("std");
// const assert = @import("../../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
//
// const common = @import("common.zig");
// const Thickness = common.Thickness;
// const Shade = common.Shade;
// const Quads = common.Quads;
// const Corner = common.Corner;
// const Edge = common.Edge;
// const Alignment = common.Alignment;
// const hline = common.hline;
// const vline = common.vline;
// const hlineMiddle = common.hlineMiddle;
// const vlineMiddle = common.vlineMiddle;
//
// const font = @import("../../main.zig");
//
// PORT NOTE: `assert = quirks.inlineAssert` lives in `rustty-util`. `font =
// ../../main.zig` becomes the local crate (`crate::metrics::Metrics`,
// `crate::sprite::canvas`). `Allocator` is unused in this module. Of the
// reference `common` imports, `Shade`, `Quads`, `Edge`, and `Alignment` are
// imported but never referenced by any function in `box.zig`; they are preserved
// as commented references above rather than as unused Rust `use`s.
use super::common::{hline, hlineMiddle, vline, vlineMiddle, Corner, Thickness};
use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color, Line, Point};
use rustty_util::quirks::inlineAssert as assert;
use rustty_z2d::options::CapMode;
use rustty_z2d::painter::{StrokeError, StrokeOptions};

/// Specification of a traditional intersection-style line/box-drawing char,
/// which can have a different style of line from each edge to the center.
// pub const Lines = packed struct(u8) {
//     up: Style = .none,
//     right: Style = .none,
//     down: Style = .none,
//     left: Style = .none,
//
//     const Style = enum(u2) {
//         none,
//         light,
//         heavy,
//         double,
//     };
// };
//
// PORT NOTE: the reference packs four `Style`s into a `u8`; the Rust port carries
// them as four `Style` fields (default `.none`), which preserves the semantics
// used at every call site. `Style` is a file-private (`const Style`) enum in the
// reference, so the Rust enum is module-private, and the `Lines` fields are kept
// module-private too — every construction/read site lives in this module.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Lines {
    up: Style,
    right: Style,
    down: Style,
    left: Style,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Style {
    #[default]
    None,
    Light,
    Heavy,
    Double,
}

// pub fn draw2500_257F(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw2500_257F(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), StrokeError> {
    // _ = width;
    let _ = width;
    // _ = height;
    let _ = height;

    // switch (cp) {
    match cp {
        // '─'
        // 0x2500 => linesChar(metrics, canvas, .{ .left = .light, .right = .light }),
        0x2500 => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '━'
        // 0x2501 => linesChar(metrics, canvas, .{ .left = .heavy, .right = .heavy }),
        0x2501 => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Heavy,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '│'
        // 0x2502 => linesChar(metrics, canvas, .{ .up = .light, .down = .light }),
        0x2502 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                ..Default::default()
            },
        ),
        // '┃'
        // 0x2503 => linesChar(metrics, canvas, .{ .up = .heavy, .down = .heavy }),
        0x2503 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                down: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┄'
        // 0x2504 => dashHorizontal(
        //     metrics,
        //     canvas,
        //     3,
        //     Thickness.light.height(metrics.box_thickness),
        //     @max(4, Thickness.light.height(metrics.box_thickness)),
        // ),
        0x2504 => dashHorizontal(
            metrics,
            canvas,
            3,
            Thickness::Light.height(metrics.box_thickness),
            4u32.max(Thickness::Light.height(metrics.box_thickness)),
        ),
        // '┅'
        // 0x2505 => dashHorizontal(
        //     metrics,
        //     canvas,
        //     3,
        //     Thickness.heavy.height(metrics.box_thickness),
        //     @max(4, Thickness.light.height(metrics.box_thickness)),
        // ),
        0x2505 => dashHorizontal(
            metrics,
            canvas,
            3,
            Thickness::Heavy.height(metrics.box_thickness),
            4u32.max(Thickness::Light.height(metrics.box_thickness)),
        ),
        // '┆'
        // 0x2506 => dashVertical(
        //     metrics,
        //     canvas,
        //     3,
        //     Thickness.light.height(metrics.box_thickness),
        //     @max(4, Thickness.light.height(metrics.box_thickness)),
        // ),
        0x2506 => dashVertical(
            metrics,
            canvas,
            3,
            Thickness::Light.height(metrics.box_thickness),
            4u32.max(Thickness::Light.height(metrics.box_thickness)),
        ),
        // '┇'
        // 0x2507 => dashVertical(
        //     metrics,
        //     canvas,
        //     3,
        //     Thickness.heavy.height(metrics.box_thickness),
        //     @max(4, Thickness.light.height(metrics.box_thickness)),
        // ),
        0x2507 => dashVertical(
            metrics,
            canvas,
            3,
            Thickness::Heavy.height(metrics.box_thickness),
            4u32.max(Thickness::Light.height(metrics.box_thickness)),
        ),
        // '┈'
        // 0x2508 => dashHorizontal(
        //     metrics,
        //     canvas,
        //     4,
        //     Thickness.light.height(metrics.box_thickness),
        //     @max(4, Thickness.light.height(metrics.box_thickness)),
        // ),
        0x2508 => dashHorizontal(
            metrics,
            canvas,
            4,
            Thickness::Light.height(metrics.box_thickness),
            4u32.max(Thickness::Light.height(metrics.box_thickness)),
        ),
        // '┉'
        // 0x2509 => dashHorizontal(
        //     metrics,
        //     canvas,
        //     4,
        //     Thickness.heavy.height(metrics.box_thickness),
        //     @max(4, Thickness.light.height(metrics.box_thickness)),
        // ),
        0x2509 => dashHorizontal(
            metrics,
            canvas,
            4,
            Thickness::Heavy.height(metrics.box_thickness),
            4u32.max(Thickness::Light.height(metrics.box_thickness)),
        ),
        // '┊'
        // 0x250a => dashVertical(
        //     metrics,
        //     canvas,
        //     4,
        //     Thickness.light.height(metrics.box_thickness),
        //     @max(4, Thickness.light.height(metrics.box_thickness)),
        // ),
        0x250a => dashVertical(
            metrics,
            canvas,
            4,
            Thickness::Light.height(metrics.box_thickness),
            4u32.max(Thickness::Light.height(metrics.box_thickness)),
        ),
        // '┋'
        // 0x250b => dashVertical(
        //     metrics,
        //     canvas,
        //     4,
        //     Thickness.heavy.height(metrics.box_thickness),
        //     @max(4, Thickness.light.height(metrics.box_thickness)),
        // ),
        0x250b => dashVertical(
            metrics,
            canvas,
            4,
            Thickness::Heavy.height(metrics.box_thickness),
            4u32.max(Thickness::Light.height(metrics.box_thickness)),
        ),
        // '┌'
        // 0x250c => linesChar(metrics, canvas, .{ .down = .light, .right = .light }),
        0x250c => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┍'
        // 0x250d => linesChar(metrics, canvas, .{ .down = .light, .right = .heavy }),
        0x250d => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┎'
        // 0x250e => linesChar(metrics, canvas, .{ .down = .heavy, .right = .light }),
        0x250e => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┏'
        // 0x250f => linesChar(metrics, canvas, .{ .down = .heavy, .right = .heavy }),
        0x250f => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                right: Style::Heavy,
                ..Default::default()
            },
        ),

        // '┐'
        // 0x2510 => linesChar(metrics, canvas, .{ .down = .light, .left = .light }),
        0x2510 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '┑'
        // 0x2511 => linesChar(metrics, canvas, .{ .down = .light, .left = .heavy }),
        0x2511 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                left: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┒'
        // 0x2512 => linesChar(metrics, canvas, .{ .down = .heavy, .left = .light }),
        0x2512 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '┓'
        // 0x2513 => linesChar(metrics, canvas, .{ .down = .heavy, .left = .heavy }),
        0x2513 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                left: Style::Heavy,
                ..Default::default()
            },
        ),
        // '└'
        // 0x2514 => linesChar(metrics, canvas, .{ .up = .light, .right = .light }),
        0x2514 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┕'
        // 0x2515 => linesChar(metrics, canvas, .{ .up = .light, .right = .heavy }),
        0x2515 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┖'
        // 0x2516 => linesChar(metrics, canvas, .{ .up = .heavy, .right = .light }),
        0x2516 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┗'
        // 0x2517 => linesChar(metrics, canvas, .{ .up = .heavy, .right = .heavy }),
        0x2517 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┘'
        // 0x2518 => linesChar(metrics, canvas, .{ .up = .light, .left = .light }),
        0x2518 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '┙'
        // 0x2519 => linesChar(metrics, canvas, .{ .up = .light, .left = .heavy }),
        0x2519 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                left: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┚'
        // 0x251a => linesChar(metrics, canvas, .{ .up = .heavy, .left = .light }),
        0x251a => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '┛'
        // 0x251b => linesChar(metrics, canvas, .{ .up = .heavy, .left = .heavy }),
        0x251b => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                left: Style::Heavy,
                ..Default::default()
            },
        ),
        // '├'
        // 0x251c => linesChar(metrics, canvas, .{ .up = .light, .down = .light, .right = .light }),
        0x251c => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┝'
        // 0x251d => linesChar(metrics, canvas, .{ .up = .light, .down = .light, .right = .heavy }),
        0x251d => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┞'
        // 0x251e => linesChar(metrics, canvas, .{ .up = .heavy, .right = .light, .down = .light }),
        0x251e => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                right: Style::Light,
                down: Style::Light,
                ..Default::default()
            },
        ),
        // '┟'
        // 0x251f => linesChar(metrics, canvas, .{ .down = .heavy, .right = .light, .up = .light }),
        0x251f => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                right: Style::Light,
                up: Style::Light,
                ..Default::default()
            },
        ),

        // '┠'
        // 0x2520 => linesChar(metrics, canvas, .{ .up = .heavy, .down = .heavy, .right = .light }),
        0x2520 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                down: Style::Heavy,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┡'
        // 0x2521 => linesChar(metrics, canvas, .{ .down = .light, .right = .heavy, .up = .heavy }),
        0x2521 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                right: Style::Heavy,
                up: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┢'
        // 0x2522 => linesChar(metrics, canvas, .{ .up = .light, .right = .heavy, .down = .heavy }),
        0x2522 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                right: Style::Heavy,
                down: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┣'
        // 0x2523 => linesChar(metrics, canvas, .{ .up = .heavy, .down = .heavy, .right = .heavy }),
        0x2523 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                down: Style::Heavy,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┤'
        // 0x2524 => linesChar(metrics, canvas, .{ .up = .light, .down = .light, .left = .light }),
        0x2524 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '┥'
        // 0x2525 => linesChar(metrics, canvas, .{ .up = .light, .down = .light, .left = .heavy }),
        0x2525 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                left: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┦'
        // 0x2526 => linesChar(metrics, canvas, .{ .up = .heavy, .left = .light, .down = .light }),
        0x2526 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                left: Style::Light,
                down: Style::Light,
                ..Default::default()
            },
        ),
        // '┧'
        // 0x2527 => linesChar(metrics, canvas, .{ .down = .heavy, .left = .light, .up = .light }),
        0x2527 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                left: Style::Light,
                up: Style::Light,
                ..Default::default()
            },
        ),
        // '┨'
        // 0x2528 => linesChar(metrics, canvas, .{ .up = .heavy, .down = .heavy, .left = .light }),
        0x2528 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                down: Style::Heavy,
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '┩'
        // 0x2529 => linesChar(metrics, canvas, .{ .down = .light, .left = .heavy, .up = .heavy }),
        0x2529 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                left: Style::Heavy,
                up: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┪'
        // 0x252a => linesChar(metrics, canvas, .{ .up = .light, .left = .heavy, .down = .heavy }),
        0x252a => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                left: Style::Heavy,
                down: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┫'
        // 0x252b => linesChar(metrics, canvas, .{ .up = .heavy, .down = .heavy, .left = .heavy }),
        0x252b => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                down: Style::Heavy,
                left: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┬'
        // 0x252c => linesChar(metrics, canvas, .{ .down = .light, .left = .light, .right = .light }),
        0x252c => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                left: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┭'
        // 0x252d => linesChar(metrics, canvas, .{ .left = .heavy, .right = .light, .down = .light }),
        0x252d => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Heavy,
                right: Style::Light,
                down: Style::Light,
                ..Default::default()
            },
        ),
        // '┮'
        // 0x252e => linesChar(metrics, canvas, .{ .right = .heavy, .left = .light, .down = .light }),
        0x252e => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Heavy,
                left: Style::Light,
                down: Style::Light,
                ..Default::default()
            },
        ),
        // '┯'
        // 0x252f => linesChar(metrics, canvas, .{ .down = .light, .left = .heavy, .right = .heavy }),
        0x252f => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                left: Style::Heavy,
                right: Style::Heavy,
                ..Default::default()
            },
        ),

        // '┰'
        // 0x2530 => linesChar(metrics, canvas, .{ .down = .heavy, .left = .light, .right = .light }),
        0x2530 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                left: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┱'
        // 0x2531 => linesChar(metrics, canvas, .{ .right = .light, .left = .heavy, .down = .heavy }),
        0x2531 => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Light,
                left: Style::Heavy,
                down: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┲'
        // 0x2532 => linesChar(metrics, canvas, .{ .left = .light, .right = .heavy, .down = .heavy }),
        0x2532 => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Light,
                right: Style::Heavy,
                down: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┳'
        // 0x2533 => linesChar(metrics, canvas, .{ .down = .heavy, .left = .heavy, .right = .heavy }),
        0x2533 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                left: Style::Heavy,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┴'
        // 0x2534 => linesChar(metrics, canvas, .{ .up = .light, .left = .light, .right = .light }),
        0x2534 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                left: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┵'
        // 0x2535 => linesChar(metrics, canvas, .{ .left = .heavy, .right = .light, .up = .light }),
        0x2535 => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Heavy,
                right: Style::Light,
                up: Style::Light,
                ..Default::default()
            },
        ),
        // '┶'
        // 0x2536 => linesChar(metrics, canvas, .{ .right = .heavy, .left = .light, .up = .light }),
        0x2536 => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Heavy,
                left: Style::Light,
                up: Style::Light,
                ..Default::default()
            },
        ),
        // '┷'
        // 0x2537 => linesChar(metrics, canvas, .{ .up = .light, .left = .heavy, .right = .heavy }),
        0x2537 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                left: Style::Heavy,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┸'
        // 0x2538 => linesChar(metrics, canvas, .{ .up = .heavy, .left = .light, .right = .light }),
        0x2538 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                left: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '┹'
        // 0x2539 => linesChar(metrics, canvas, .{ .right = .light, .left = .heavy, .up = .heavy }),
        0x2539 => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Light,
                left: Style::Heavy,
                up: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┺'
        // 0x253a => linesChar(metrics, canvas, .{ .left = .light, .right = .heavy, .up = .heavy }),
        0x253a => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Light,
                right: Style::Heavy,
                up: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┻'
        // 0x253b => linesChar(metrics, canvas, .{ .up = .heavy, .left = .heavy, .right = .heavy }),
        0x253b => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                left: Style::Heavy,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '┼'
        // 0x253c => linesChar(metrics, canvas, .{ .up = .light, .down = .light, .left = .light, .right = .light }),
        0x253c => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                left: Style::Light,
                right: Style::Light,
            },
        ),
        // '┽'
        // 0x253d => linesChar(metrics, canvas, .{ .left = .heavy, .right = .light, .up = .light, .down = .light }),
        0x253d => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Heavy,
                right: Style::Light,
                up: Style::Light,
                down: Style::Light,
            },
        ),
        // '┾'
        // 0x253e => linesChar(metrics, canvas, .{ .right = .heavy, .left = .light, .up = .light, .down = .light }),
        0x253e => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Heavy,
                left: Style::Light,
                up: Style::Light,
                down: Style::Light,
            },
        ),
        // '┿'
        // 0x253f => linesChar(metrics, canvas, .{ .up = .light, .down = .light, .left = .heavy, .right = .heavy }),
        0x253f => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                left: Style::Heavy,
                right: Style::Heavy,
            },
        ),

        // '╀'
        // 0x2540 => linesChar(metrics, canvas, .{ .up = .heavy, .down = .light, .left = .light, .right = .light }),
        0x2540 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                down: Style::Light,
                left: Style::Light,
                right: Style::Light,
            },
        ),
        // '╁'
        // 0x2541 => linesChar(metrics, canvas, .{ .down = .heavy, .up = .light, .left = .light, .right = .light }),
        0x2541 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                up: Style::Light,
                left: Style::Light,
                right: Style::Light,
            },
        ),
        // '╂'
        // 0x2542 => linesChar(metrics, canvas, .{ .up = .heavy, .down = .heavy, .left = .light, .right = .light }),
        0x2542 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                down: Style::Heavy,
                left: Style::Light,
                right: Style::Light,
            },
        ),
        // '╃'
        // 0x2543 => linesChar(metrics, canvas, .{ .left = .heavy, .up = .heavy, .right = .light, .down = .light }),
        0x2543 => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Heavy,
                up: Style::Heavy,
                right: Style::Light,
                down: Style::Light,
            },
        ),
        // '╄'
        // 0x2544 => linesChar(metrics, canvas, .{ .right = .heavy, .up = .heavy, .left = .light, .down = .light }),
        0x2544 => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Heavy,
                up: Style::Heavy,
                left: Style::Light,
                down: Style::Light,
            },
        ),
        // '╅'
        // 0x2545 => linesChar(metrics, canvas, .{ .left = .heavy, .down = .heavy, .right = .light, .up = .light }),
        0x2545 => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Heavy,
                down: Style::Heavy,
                right: Style::Light,
                up: Style::Light,
            },
        ),
        // '╆'
        // 0x2546 => linesChar(metrics, canvas, .{ .right = .heavy, .down = .heavy, .left = .light, .up = .light }),
        0x2546 => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Heavy,
                down: Style::Heavy,
                left: Style::Light,
                up: Style::Light,
            },
        ),
        // '╇'
        // 0x2547 => linesChar(metrics, canvas, .{ .down = .light, .up = .heavy, .left = .heavy, .right = .heavy }),
        0x2547 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                up: Style::Heavy,
                left: Style::Heavy,
                right: Style::Heavy,
            },
        ),
        // '╈'
        // 0x2548 => linesChar(metrics, canvas, .{ .up = .light, .down = .heavy, .left = .heavy, .right = .heavy }),
        0x2548 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Heavy,
                left: Style::Heavy,
                right: Style::Heavy,
            },
        ),
        // '╉'
        // 0x2549 => linesChar(metrics, canvas, .{ .right = .light, .left = .heavy, .up = .heavy, .down = .heavy }),
        0x2549 => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Light,
                left: Style::Heavy,
                up: Style::Heavy,
                down: Style::Heavy,
            },
        ),
        // '╊'
        // 0x254a => linesChar(metrics, canvas, .{ .left = .light, .right = .heavy, .up = .heavy, .down = .heavy }),
        0x254a => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Light,
                right: Style::Heavy,
                up: Style::Heavy,
                down: Style::Heavy,
            },
        ),
        // '╋'
        // 0x254b => linesChar(metrics, canvas, .{ .up = .heavy, .down = .heavy, .left = .heavy, .right = .heavy }),
        0x254b => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                down: Style::Heavy,
                left: Style::Heavy,
                right: Style::Heavy,
            },
        ),
        // '╌'
        // 0x254c => dashHorizontal(
        //     metrics,
        //     canvas,
        //     2,
        //     Thickness.light.height(metrics.box_thickness),
        //     Thickness.light.height(metrics.box_thickness),
        // ),
        0x254c => dashHorizontal(
            metrics,
            canvas,
            2,
            Thickness::Light.height(metrics.box_thickness),
            Thickness::Light.height(metrics.box_thickness),
        ),
        // '╍'
        // 0x254d => dashHorizontal(
        //     metrics,
        //     canvas,
        //     2,
        //     Thickness.heavy.height(metrics.box_thickness),
        //     Thickness.heavy.height(metrics.box_thickness),
        // ),
        0x254d => dashHorizontal(
            metrics,
            canvas,
            2,
            Thickness::Heavy.height(metrics.box_thickness),
            Thickness::Heavy.height(metrics.box_thickness),
        ),
        // '╎'
        // 0x254e => dashVertical(
        //     metrics,
        //     canvas,
        //     2,
        //     Thickness.light.height(metrics.box_thickness),
        //     Thickness.heavy.height(metrics.box_thickness),
        // ),
        0x254e => dashVertical(
            metrics,
            canvas,
            2,
            Thickness::Light.height(metrics.box_thickness),
            Thickness::Heavy.height(metrics.box_thickness),
        ),
        // '╏'
        // 0x254f => dashVertical(
        //     metrics,
        //     canvas,
        //     2,
        //     Thickness.heavy.height(metrics.box_thickness),
        //     Thickness.heavy.height(metrics.box_thickness),
        // ),
        0x254f => dashVertical(
            metrics,
            canvas,
            2,
            Thickness::Heavy.height(metrics.box_thickness),
            Thickness::Heavy.height(metrics.box_thickness),
        ),

        // '═'
        // 0x2550 => linesChar(metrics, canvas, .{ .left = .double, .right = .double }),
        0x2550 => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Double,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '║'
        // 0x2551 => linesChar(metrics, canvas, .{ .up = .double, .down = .double }),
        0x2551 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                down: Style::Double,
                ..Default::default()
            },
        ),
        // '╒'
        // 0x2552 => linesChar(metrics, canvas, .{ .down = .light, .right = .double }),
        0x2552 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╓'
        // 0x2553 => linesChar(metrics, canvas, .{ .down = .double, .right = .light }),
        0x2553 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Double,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '╔'
        // 0x2554 => linesChar(metrics, canvas, .{ .down = .double, .right = .double }),
        0x2554 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Double,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╕'
        // 0x2555 => linesChar(metrics, canvas, .{ .down = .light, .left = .double }),
        0x2555 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                left: Style::Double,
                ..Default::default()
            },
        ),
        // '╖'
        // 0x2556 => linesChar(metrics, canvas, .{ .down = .double, .left = .light }),
        0x2556 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Double,
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '╗'
        // 0x2557 => linesChar(metrics, canvas, .{ .down = .double, .left = .double }),
        0x2557 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Double,
                left: Style::Double,
                ..Default::default()
            },
        ),
        // '╘'
        // 0x2558 => linesChar(metrics, canvas, .{ .up = .light, .right = .double }),
        0x2558 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╙'
        // 0x2559 => linesChar(metrics, canvas, .{ .up = .double, .right = .light }),
        0x2559 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '╚'
        // 0x255a => linesChar(metrics, canvas, .{ .up = .double, .right = .double }),
        0x255a => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╛'
        // 0x255b => linesChar(metrics, canvas, .{ .up = .light, .left = .double }),
        0x255b => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                left: Style::Double,
                ..Default::default()
            },
        ),
        // '╜'
        // 0x255c => linesChar(metrics, canvas, .{ .up = .double, .left = .light }),
        0x255c => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '╝'
        // 0x255d => linesChar(metrics, canvas, .{ .up = .double, .left = .double }),
        0x255d => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                left: Style::Double,
                ..Default::default()
            },
        ),
        // '╞'
        // 0x255e => linesChar(metrics, canvas, .{ .up = .light, .down = .light, .right = .double }),
        0x255e => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╟'
        // 0x255f => linesChar(metrics, canvas, .{ .up = .double, .down = .double, .right = .light }),
        0x255f => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                down: Style::Double,
                right: Style::Light,
                ..Default::default()
            },
        ),

        // '╠'
        // 0x2560 => linesChar(metrics, canvas, .{ .up = .double, .down = .double, .right = .double }),
        0x2560 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                down: Style::Double,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╡'
        // 0x2561 => linesChar(metrics, canvas, .{ .up = .light, .down = .light, .left = .double }),
        0x2561 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                left: Style::Double,
                ..Default::default()
            },
        ),
        // '╢'
        // 0x2562 => linesChar(metrics, canvas, .{ .up = .double, .down = .double, .left = .light }),
        0x2562 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                down: Style::Double,
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '╣'
        // 0x2563 => linesChar(metrics, canvas, .{ .up = .double, .down = .double, .left = .double }),
        0x2563 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                down: Style::Double,
                left: Style::Double,
                ..Default::default()
            },
        ),
        // '╤'
        // 0x2564 => linesChar(metrics, canvas, .{ .down = .light, .left = .double, .right = .double }),
        0x2564 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                left: Style::Double,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╥'
        // 0x2565 => linesChar(metrics, canvas, .{ .down = .double, .left = .light, .right = .light }),
        0x2565 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Double,
                left: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '╦'
        // 0x2566 => linesChar(metrics, canvas, .{ .down = .double, .left = .double, .right = .double }),
        0x2566 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Double,
                left: Style::Double,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╧'
        // 0x2567 => linesChar(metrics, canvas, .{ .up = .light, .left = .double, .right = .double }),
        0x2567 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                left: Style::Double,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╨'
        // 0x2568 => linesChar(metrics, canvas, .{ .up = .double, .left = .light, .right = .light }),
        0x2568 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                left: Style::Light,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '╩'
        // 0x2569 => linesChar(metrics, canvas, .{ .up = .double, .left = .double, .right = .double }),
        0x2569 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                left: Style::Double,
                right: Style::Double,
                ..Default::default()
            },
        ),
        // '╪'
        // 0x256a => linesChar(metrics, canvas, .{ .up = .light, .down = .light, .left = .double, .right = .double }),
        0x256a => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Light,
                left: Style::Double,
                right: Style::Double,
            },
        ),
        // '╫'
        // 0x256b => linesChar(metrics, canvas, .{ .up = .double, .down = .double, .left = .light, .right = .light }),
        0x256b => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                down: Style::Double,
                left: Style::Light,
                right: Style::Light,
            },
        ),
        // '╬'
        // 0x256c => linesChar(metrics, canvas, .{ .up = .double, .down = .double, .left = .double, .right = .double }),
        0x256c => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Double,
                down: Style::Double,
                left: Style::Double,
                right: Style::Double,
            },
        ),
        // '╭'
        // 0x256d => try arc(metrics, canvas, .br, .light),
        0x256d => arc(metrics, canvas, Corner::Br, Thickness::Light)?,
        // '╮'
        // 0x256e => try arc(metrics, canvas, .bl, .light),
        0x256e => arc(metrics, canvas, Corner::Bl, Thickness::Light)?,
        // '╯'
        // 0x256f => try arc(metrics, canvas, .tl, .light),
        0x256f => arc(metrics, canvas, Corner::Tl, Thickness::Light)?,

        // '╰'
        // 0x2570 => try arc(metrics, canvas, .tr, .light),
        0x2570 => arc(metrics, canvas, Corner::Tr, Thickness::Light)?,
        // '╱'
        // 0x2571 => lightDiagonalUpperRightToLowerLeft(metrics, canvas),
        0x2571 => lightDiagonalUpperRightToLowerLeft(metrics, canvas),
        // '╲'
        // 0x2572 => lightDiagonalUpperLeftToLowerRight(metrics, canvas),
        0x2572 => lightDiagonalUpperLeftToLowerRight(metrics, canvas),
        // '╳'
        // 0x2573 => lightDiagonalCross(metrics, canvas),
        0x2573 => lightDiagonalCross(metrics, canvas),
        // '╴'
        // 0x2574 => linesChar(metrics, canvas, .{ .left = .light }),
        0x2574 => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Light,
                ..Default::default()
            },
        ),
        // '╵'
        // 0x2575 => linesChar(metrics, canvas, .{ .up = .light }),
        0x2575 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                ..Default::default()
            },
        ),
        // '╶'
        // 0x2576 => linesChar(metrics, canvas, .{ .right = .light }),
        0x2576 => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '╷'
        // 0x2577 => linesChar(metrics, canvas, .{ .down = .light }),
        0x2577 => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Light,
                ..Default::default()
            },
        ),
        // '╸'
        // 0x2578 => linesChar(metrics, canvas, .{ .left = .heavy }),
        0x2578 => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Heavy,
                ..Default::default()
            },
        ),
        // '╹'
        // 0x2579 => linesChar(metrics, canvas, .{ .up = .heavy }),
        0x2579 => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                ..Default::default()
            },
        ),
        // '╺'
        // 0x257a => linesChar(metrics, canvas, .{ .right = .heavy }),
        0x257a => linesChar(
            metrics,
            canvas,
            Lines {
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '╻'
        // 0x257b => linesChar(metrics, canvas, .{ .down = .heavy }),
        0x257b => linesChar(
            metrics,
            canvas,
            Lines {
                down: Style::Heavy,
                ..Default::default()
            },
        ),
        // '╼'
        // 0x257c => linesChar(metrics, canvas, .{ .left = .light, .right = .heavy }),
        0x257c => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Light,
                right: Style::Heavy,
                ..Default::default()
            },
        ),
        // '╽'
        // 0x257d => linesChar(metrics, canvas, .{ .up = .light, .down = .heavy }),
        0x257d => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Light,
                down: Style::Heavy,
                ..Default::default()
            },
        ),
        // '╾'
        // 0x257e => linesChar(metrics, canvas, .{ .left = .heavy, .right = .light }),
        0x257e => linesChar(
            metrics,
            canvas,
            Lines {
                left: Style::Heavy,
                right: Style::Light,
                ..Default::default()
            },
        ),
        // '╿'
        // 0x257f => linesChar(metrics, canvas, .{ .up = .heavy, .down = .light }),
        0x257f => linesChar(
            metrics,
            canvas,
            Lines {
                up: Style::Heavy,
                down: Style::Light,
                ..Default::default()
            },
        ),

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

// pub fn linesChar(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     lines: Lines,
// ) void {
pub fn linesChar(metrics: Metrics, canvas: &mut Canvas, lines: Lines) {
    // const light_px = Thickness.light.height(metrics.box_thickness);
    let light_px = Thickness::Light.height(metrics.box_thickness);
    // const heavy_px = Thickness.heavy.height(metrics.box_thickness);
    let heavy_px = Thickness::Heavy.height(metrics.box_thickness);

    // Top of light horizontal strokes
    // const h_light_top = (metrics.cell_height -| light_px) / 2;
    let h_light_top = metrics.cell_height.saturating_sub(light_px) / 2;
    // Bottom of light horizontal strokes
    // const h_light_bottom = h_light_top +| light_px;
    let h_light_bottom = h_light_top.saturating_add(light_px);

    // Top of heavy horizontal strokes
    // const h_heavy_top = (metrics.cell_height -| heavy_px) / 2;
    let h_heavy_top = metrics.cell_height.saturating_sub(heavy_px) / 2;
    // Bottom of heavy horizontal strokes
    // const h_heavy_bottom = h_heavy_top +| heavy_px;
    let h_heavy_bottom = h_heavy_top.saturating_add(heavy_px);

    // Top of the top doubled horizontal stroke (bottom is `h_light_top`)
    // const h_double_top = h_light_top -| light_px;
    let h_double_top = h_light_top.saturating_sub(light_px);
    // Bottom of the bottom doubled horizontal stroke (top is `h_light_bottom`)
    // const h_double_bottom = h_light_bottom +| light_px;
    let h_double_bottom = h_light_bottom.saturating_add(light_px);

    // Left of light vertical strokes
    // const v_light_left = (metrics.cell_width -| light_px) / 2;
    let v_light_left = metrics.cell_width.saturating_sub(light_px) / 2;
    // Right of light vertical strokes
    // const v_light_right = v_light_left +| light_px;
    let v_light_right = v_light_left.saturating_add(light_px);

    // Left of heavy vertical strokes
    // const v_heavy_left = (metrics.cell_width -| heavy_px) / 2;
    let v_heavy_left = metrics.cell_width.saturating_sub(heavy_px) / 2;
    // Right of heavy vertical strokes
    // const v_heavy_right = v_heavy_left +| heavy_px;
    let v_heavy_right = v_heavy_left.saturating_add(heavy_px);

    // Left of the left doubled vertical stroke (right is `v_light_left`)
    // const v_double_left = v_light_left -| light_px;
    let v_double_left = v_light_left.saturating_sub(light_px);
    // Right of the right doubled vertical stroke (left is `v_light_right`)
    // const v_double_right = v_light_right +| light_px;
    let v_double_right = v_light_right.saturating_add(light_px);

    // The bottom of the up line
    // const up_bottom = if (lines.left == .heavy or lines.right == .heavy)
    //     h_heavy_bottom
    // else if (lines.left != lines.right or lines.down == lines.up)
    //     if (lines.left == .double or lines.right == .double)
    //         h_double_bottom
    //     else
    //         h_light_bottom
    // else if (lines.left == .none and lines.right == .none)
    //     h_light_bottom
    // else
    //     h_light_top;
    let up_bottom = if lines.left == Style::Heavy || lines.right == Style::Heavy {
        h_heavy_bottom
    } else if lines.left != lines.right || lines.down == lines.up {
        if lines.left == Style::Double || lines.right == Style::Double {
            h_double_bottom
        } else {
            h_light_bottom
        }
    } else if lines.left == Style::None && lines.right == Style::None {
        h_light_bottom
    } else {
        h_light_top
    };

    // The top of the down line
    // const down_top = if (lines.left == .heavy or lines.right == .heavy)
    //     h_heavy_top
    // else if (lines.left != lines.right or lines.up == lines.down)
    //     if (lines.left == .double or lines.right == .double)
    //         h_double_top
    //     else
    //         h_light_top
    // else if (lines.left == .none and lines.right == .none)
    //     h_light_top
    // else
    //     h_light_bottom;
    let down_top = if lines.left == Style::Heavy || lines.right == Style::Heavy {
        h_heavy_top
    } else if lines.left != lines.right || lines.up == lines.down {
        if lines.left == Style::Double || lines.right == Style::Double {
            h_double_top
        } else {
            h_light_top
        }
    } else if lines.left == Style::None && lines.right == Style::None {
        h_light_top
    } else {
        h_light_bottom
    };

    // The right of the left line
    // const left_right = if (lines.up == .heavy or lines.down == .heavy)
    //     v_heavy_right
    // else if (lines.up != lines.down or lines.left == lines.right)
    //     if (lines.up == .double or lines.down == .double)
    //         v_double_right
    //     else
    //         v_light_right
    // else if (lines.up == .none and lines.down == .none)
    //     v_light_right
    // else
    //     v_light_left;
    let left_right = if lines.up == Style::Heavy || lines.down == Style::Heavy {
        v_heavy_right
    } else if lines.up != lines.down || lines.left == lines.right {
        if lines.up == Style::Double || lines.down == Style::Double {
            v_double_right
        } else {
            v_light_right
        }
    } else if lines.up == Style::None && lines.down == Style::None {
        v_light_right
    } else {
        v_light_left
    };

    // The left of the right line
    // const right_left = if (lines.up == .heavy or lines.down == .heavy)
    //     v_heavy_left
    // else if (lines.up != lines.down or lines.right == lines.left)
    //     if (lines.up == .double or lines.down == .double)
    //         v_double_left
    //     else
    //         v_light_left
    // else if (lines.up == .none and lines.down == .none)
    //     v_light_left
    // else
    //     v_light_right;
    let right_left = if lines.up == Style::Heavy || lines.down == Style::Heavy {
        v_heavy_left
    } else if lines.up != lines.down || lines.right == lines.left {
        if lines.up == Style::Double || lines.down == Style::Double {
            v_double_left
        } else {
            v_light_left
        }
    } else if lines.up == Style::None && lines.down == Style::None {
        v_light_left
    } else {
        v_light_right
    };

    // switch (lines.up) {
    match lines.up {
        // .none => {},
        Style::None => {}
        // .light => canvas.box(
        //     @intCast(v_light_left),
        //     0,
        //     @intCast(v_light_right),
        //     @intCast(up_bottom),
        //     .on,
        // ),
        Style::Light => canvas.r#box(
            v_light_left as i32,
            0,
            v_light_right as i32,
            up_bottom as i32,
            Color::ON,
        ),
        // .heavy => canvas.box(
        //     @intCast(v_heavy_left),
        //     0,
        //     @intCast(v_heavy_right),
        //     @intCast(up_bottom),
        //     .on,
        // ),
        Style::Heavy => canvas.r#box(
            v_heavy_left as i32,
            0,
            v_heavy_right as i32,
            up_bottom as i32,
            Color::ON,
        ),
        // .double => {
        Style::Double => {
            // const left_bottom = if (lines.left == .double) h_light_top else up_bottom;
            let left_bottom = if lines.left == Style::Double {
                h_light_top
            } else {
                up_bottom
            };
            // const right_bottom = if (lines.right == .double) h_light_top else up_bottom;
            let right_bottom = if lines.right == Style::Double {
                h_light_top
            } else {
                up_bottom
            };

            // canvas.box(
            //     @intCast(v_double_left),
            //     0,
            //     @intCast(v_light_left),
            //     @intCast(left_bottom),
            //     .on,
            // );
            canvas.r#box(
                v_double_left as i32,
                0,
                v_light_left as i32,
                left_bottom as i32,
                Color::ON,
            );
            // canvas.box(
            //     @intCast(v_light_right),
            //     0,
            //     @intCast(v_double_right),
            //     @intCast(right_bottom),
            //     .on,
            // );
            canvas.r#box(
                v_light_right as i32,
                0,
                v_double_right as i32,
                right_bottom as i32,
                Color::ON,
            );
        }
    }

    // switch (lines.right) {
    match lines.right {
        // .none => {},
        Style::None => {}
        // .light => canvas.box(
        //     @intCast(right_left),
        //     @intCast(h_light_top),
        //     @intCast(metrics.cell_width),
        //     @intCast(h_light_bottom),
        //     .on,
        // ),
        Style::Light => canvas.r#box(
            right_left as i32,
            h_light_top as i32,
            metrics.cell_width as i32,
            h_light_bottom as i32,
            Color::ON,
        ),
        // .heavy => canvas.box(
        //     @intCast(right_left),
        //     @intCast(h_heavy_top),
        //     @intCast(metrics.cell_width),
        //     @intCast(h_heavy_bottom),
        //     .on,
        // ),
        Style::Heavy => canvas.r#box(
            right_left as i32,
            h_heavy_top as i32,
            metrics.cell_width as i32,
            h_heavy_bottom as i32,
            Color::ON,
        ),
        // .double => {
        Style::Double => {
            // const top_left = if (lines.up == .double) v_light_right else right_left;
            let top_left = if lines.up == Style::Double {
                v_light_right
            } else {
                right_left
            };
            // const bottom_left = if (lines.down == .double) v_light_right else right_left;
            let bottom_left = if lines.down == Style::Double {
                v_light_right
            } else {
                right_left
            };

            // canvas.box(
            //     @intCast(top_left),
            //     @intCast(h_double_top),
            //     @intCast(metrics.cell_width),
            //     @intCast(h_light_top),
            //     .on,
            // );
            canvas.r#box(
                top_left as i32,
                h_double_top as i32,
                metrics.cell_width as i32,
                h_light_top as i32,
                Color::ON,
            );
            // canvas.box(
            //     @intCast(bottom_left),
            //     @intCast(h_light_bottom),
            //     @intCast(metrics.cell_width),
            //     @intCast(h_double_bottom),
            //     .on,
            // );
            canvas.r#box(
                bottom_left as i32,
                h_light_bottom as i32,
                metrics.cell_width as i32,
                h_double_bottom as i32,
                Color::ON,
            );
        }
    }

    // switch (lines.down) {
    match lines.down {
        // .none => {},
        Style::None => {}
        // .light => canvas.box(
        //     @intCast(v_light_left),
        //     @intCast(down_top),
        //     @intCast(v_light_right),
        //     @intCast(metrics.cell_height),
        //     .on,
        // ),
        Style::Light => canvas.r#box(
            v_light_left as i32,
            down_top as i32,
            v_light_right as i32,
            metrics.cell_height as i32,
            Color::ON,
        ),
        // .heavy => canvas.box(
        //     @intCast(v_heavy_left),
        //     @intCast(down_top),
        //     @intCast(v_heavy_right),
        //     @intCast(metrics.cell_height),
        //     .on,
        // ),
        Style::Heavy => canvas.r#box(
            v_heavy_left as i32,
            down_top as i32,
            v_heavy_right as i32,
            metrics.cell_height as i32,
            Color::ON,
        ),
        // .double => {
        Style::Double => {
            // const left_top = if (lines.left == .double) h_light_bottom else down_top;
            let left_top = if lines.left == Style::Double {
                h_light_bottom
            } else {
                down_top
            };
            // const right_top = if (lines.right == .double) h_light_bottom else down_top;
            let right_top = if lines.right == Style::Double {
                h_light_bottom
            } else {
                down_top
            };

            // canvas.box(
            //     @intCast(v_double_left),
            //     @intCast(left_top),
            //     @intCast(v_light_left),
            //     @intCast(metrics.cell_height),
            //     .on,
            // );
            canvas.r#box(
                v_double_left as i32,
                left_top as i32,
                v_light_left as i32,
                metrics.cell_height as i32,
                Color::ON,
            );
            // canvas.box(
            //     @intCast(v_light_right),
            //     @intCast(right_top),
            //     @intCast(v_double_right),
            //     @intCast(metrics.cell_height),
            //     .on,
            // );
            canvas.r#box(
                v_light_right as i32,
                right_top as i32,
                v_double_right as i32,
                metrics.cell_height as i32,
                Color::ON,
            );
        }
    }

    // switch (lines.left) {
    match lines.left {
        // .none => {},
        Style::None => {}
        // .light => canvas.box(
        //     0,
        //     @intCast(h_light_top),
        //     @intCast(left_right),
        //     @intCast(h_light_bottom),
        //     .on,
        // ),
        Style::Light => canvas.r#box(
            0,
            h_light_top as i32,
            left_right as i32,
            h_light_bottom as i32,
            Color::ON,
        ),
        // .heavy => canvas.box(
        //     0,
        //     @intCast(h_heavy_top),
        //     @intCast(left_right),
        //     @intCast(h_heavy_bottom),
        //     .on,
        // ),
        Style::Heavy => canvas.r#box(
            0,
            h_heavy_top as i32,
            left_right as i32,
            h_heavy_bottom as i32,
            Color::ON,
        ),
        // .double => {
        Style::Double => {
            // const top_right = if (lines.up == .double) v_light_left else left_right;
            let top_right = if lines.up == Style::Double {
                v_light_left
            } else {
                left_right
            };
            // const bottom_right = if (lines.down == .double) v_light_left else left_right;
            let bottom_right = if lines.down == Style::Double {
                v_light_left
            } else {
                left_right
            };

            // canvas.box(
            //     0,
            //     @intCast(h_double_top),
            //     @intCast(top_right),
            //     @intCast(h_light_top),
            //     .on,
            // );
            canvas.r#box(
                0,
                h_double_top as i32,
                top_right as i32,
                h_light_top as i32,
                Color::ON,
            );
            // canvas.box(
            //     0,
            //     @intCast(h_light_bottom),
            //     @intCast(bottom_right),
            //     @intCast(h_double_bottom),
            //     .on,
            // );
            canvas.r#box(
                0,
                h_light_bottom as i32,
                bottom_right as i32,
                h_double_bottom as i32,
                Color::ON,
            );
        }
    }
}

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

// pub fn lightDiagonalCross(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
// ) void {
pub fn lightDiagonalCross(metrics: Metrics, canvas: &mut Canvas) {
    // lightDiagonalUpperRightToLowerLeft(metrics, canvas);
    lightDiagonalUpperRightToLowerLeft(metrics, canvas);
    // lightDiagonalUpperLeftToLowerRight(metrics, canvas);
    lightDiagonalUpperLeftToLowerRight(metrics, canvas);
}

// pub fn arc(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime corner: Corner,
//     comptime thickness: Thickness,
// ) !void {
pub fn arc(
    metrics: Metrics,
    canvas: &mut Canvas,
    corner: Corner,
    thickness: Thickness,
) -> Result<(), StrokeError> {
    // const thick_px = thickness.height(metrics.box_thickness);
    let thick_px = thickness.height(metrics.box_thickness);
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;
    // const float_thick: f64 = @floatFromInt(thick_px);
    let float_thick: f64 = thick_px as f64;
    // const center_x: f64 = @as(f64, @floatFromInt((metrics.cell_width -| thick_px) / 2)) + float_thick / 2;
    let center_x: f64 =
        ((metrics.cell_width.saturating_sub(thick_px) / 2) as f64) + float_thick / 2.0;
    // const center_y: f64 = @as(f64, @floatFromInt((metrics.cell_height -| thick_px) / 2)) + float_thick / 2;
    let center_y: f64 =
        ((metrics.cell_height.saturating_sub(thick_px) / 2) as f64) + float_thick / 2.0;

    // const r = @min(float_width, float_height) / 2;
    let r = float_width.min(float_height) / 2.0;

    // Fraction away from the center to place the middle control points,
    // const s: f64 = 0.25;
    let s: f64 = 0.25;

    // var path = canvas.staticPath(4);
    let mut path = canvas.staticPath::<4>();

    // switch (corner) {
    match corner {
        // .tl => {
        Corner::Tl => {
            // path.moveTo(center_x, 0);
            path.move_to(center_x, 0.0);
            // path.lineTo(center_x, center_y - r);
            path.line_to(center_x, center_y - r);
            // path.curveTo(
            //     center_x,
            //     center_y - s * r,
            //     center_x - s * r,
            //     center_y,
            //     center_x - r,
            //     center_y,
            // );
            path.curve_to(
                center_x,
                center_y - s * r,
                center_x - s * r,
                center_y,
                center_x - r,
                center_y,
            );
            // path.lineTo(0, center_y);
            path.line_to(0.0, center_y);
        }
        // .tr => {
        Corner::Tr => {
            // path.moveTo(center_x, 0);
            path.move_to(center_x, 0.0);
            // path.lineTo(center_x, center_y - r);
            path.line_to(center_x, center_y - r);
            // path.curveTo(
            //     center_x,
            //     center_y - s * r,
            //     center_x + s * r,
            //     center_y,
            //     center_x + r,
            //     center_y,
            // );
            path.curve_to(
                center_x,
                center_y - s * r,
                center_x + s * r,
                center_y,
                center_x + r,
                center_y,
            );
            // path.lineTo(float_width, center_y);
            path.line_to(float_width, center_y);
        }
        // .bl => {
        Corner::Bl => {
            // path.moveTo(center_x, float_height);
            path.move_to(center_x, float_height);
            // path.lineTo(center_x, center_y + r);
            path.line_to(center_x, center_y + r);
            // path.curveTo(
            //     center_x,
            //     center_y + s * r,
            //     center_x - s * r,
            //     center_y,
            //     center_x - r,
            //     center_y,
            // );
            path.curve_to(
                center_x,
                center_y + s * r,
                center_x - s * r,
                center_y,
                center_x - r,
                center_y,
            );
            // path.lineTo(0, center_y);
            path.line_to(0.0, center_y);
        }
        // .br => {
        Corner::Br => {
            // path.moveTo(center_x, float_height);
            path.move_to(center_x, float_height);
            // path.lineTo(center_x, center_y + r);
            path.line_to(center_x, center_y + r);
            // path.curveTo(
            //     center_x,
            //     center_y + s * r,
            //     center_x + s * r,
            //     center_y,
            //     center_x + r,
            //     center_y,
            // );
            path.curve_to(
                center_x,
                center_y + s * r,
                center_x + s * r,
                center_y,
                center_x + r,
                center_y,
            );
            // path.lineTo(float_width, center_y);
            path.line_to(float_width, center_y);
        }
    }

    // try canvas.strokePath(
    //     path.wrapped_path,
    //     .{
    //         .line_cap_mode = .butt,
    //         .line_width = float_thick,
    //     },
    //     .on,
    // );
    canvas.strokePath(
        &path.wrapped_path,
        StrokeOptions {
            line_cap_mode: CapMode::Butt,
            line_width: float_thick,
            ..Default::default()
        },
        Color::ON,
    )
}

// fn dashHorizontal(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     count: u8,
//     thick_px: u32,
//     desired_gap: u32,
// ) void {
#[allow(clippy::manual_range_contains)]
fn dashHorizontal(
    metrics: Metrics,
    canvas: &mut Canvas,
    count: u8,
    thick_px: u32,
    desired_gap: u32,
) {
    // assert(count >= 2 and count <= 4);
    assert(count >= 2 && count <= 4);

    // +------------+
    // |            |
    // |            |
    // |            |
    // |            |
    // | --  --  -- |
    // |            |
    // |            |
    // |            |
    // |            |
    // +------------+
    // Our dashed line should be made such that when tiled horizontally
    // it creates one consistent line with no uneven gap or segment sizes.
    // In order to make sure this is the case, we should have half-sized
    // gaps on the left and right so that it is centered properly.

    // For N dashes, there are N - 1 gaps between them, but we also have
    // half-sized gaps on either side, adding up to N total gaps.
    // const gap_count = count;
    let gap_count = count;

    // We need at least 1 pixel for each gap and each dash, if we don't
    // have that then we can't draw our dashed line correctly so we just
    // draw a solid line and return.
    // if (metrics.cell_width < count + gap_count) {
    //     hlineMiddle(metrics, canvas, .light);
    //     return;
    // }
    if metrics.cell_width < count as u32 + gap_count as u32 {
        hlineMiddle(metrics, canvas, Thickness::Light);
        return;
    }

    // We never want the gaps to take up more than 50% of the space,
    // because if they do the dashes are too small and look wrong.
    // const gap_width: i32 = @intCast(@min(desired_gap, metrics.cell_width / (2 * count)));
    let gap_width: i32 = desired_gap.min(metrics.cell_width / (2 * count as u32)) as i32;
    // const total_gap_width: i32 = gap_count * gap_width;
    let total_gap_width: i32 = gap_count as i32 * gap_width;
    // const total_dash_width: i32 = @as(i32, @intCast(metrics.cell_width)) - total_gap_width;
    let total_dash_width: i32 = metrics.cell_width as i32 - total_gap_width;
    // const dash_width: i32 = @divFloor(total_dash_width, count);
    let dash_width: i32 = total_dash_width.div_euclid(count as i32);
    // const remaining: i32 = @mod(total_dash_width, count);
    let remaining: i32 = total_dash_width.rem_euclid(count as i32);

    // assert(dash_width * count + gap_width * gap_count + remaining == metrics.cell_width);
    assert(
        dash_width * count as i32 + gap_width * gap_count as i32 + remaining
            == metrics.cell_width as i32,
    );

    // Our dashes should be centered vertically.
    // const y: i32 = @intCast((metrics.cell_height -| thick_px) / 2);
    let y: i32 = (metrics.cell_height.saturating_sub(thick_px) / 2) as i32;

    // We start at half a gap from the left edge, in order to center
    // our dashes properly.
    // var x: i32 = @divFloor(gap_width, 2);
    let mut x: i32 = gap_width.div_euclid(2);

    // We'll distribute the extra space in to dash widths, 1px at a
    // time. We prefer this to making gaps larger since that is much
    // more visually obvious.
    // var extra: i32 = remaining;
    let mut extra: i32 = remaining;

    // for (0..count) |_| {
    for _ in 0..count {
        // var x1 = x + dash_width;
        let mut x1 = x + dash_width;
        // We distribute left-over size in to dash widths,
        // since it's less obvious there than in the gaps.
        // if (extra > 0) {
        //     extra -= 1;
        //     x1 += 1;
        // }
        if extra > 0 {
            extra -= 1;
            x1 += 1;
        }
        // hline(canvas, x, x1, y, thick_px);
        hline(canvas, x, x1, y, thick_px);
        // Advance by the width of the dash we drew and the width
        // of a gap to get the start of the next dash.
        // x = x1 + gap_width;
        x = x1 + gap_width;
    }
}

// fn dashVertical(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime count: u8,
//     thick_px: u32,
//     desired_gap: u32,
// ) void {
#[allow(clippy::manual_range_contains)]
fn dashVertical(metrics: Metrics, canvas: &mut Canvas, count: u8, thick_px: u32, desired_gap: u32) {
    // assert(count >= 2 and count <= 4);
    assert(count >= 2 && count <= 4);

    // +-----------+
    // |     |     |
    // |     |     |
    // |           |
    // |     |     |
    // |     |     |
    // |           |
    // |     |     |
    // |     |     |
    // |           |
    // +-----------+
    // Our dashed line should be made such that when tiled vertically it
    // it creates one consistent line with no uneven gap or segment sizes.
    // In order to make sure this is the case, we should have an extra gap
    // gap at the bottom.
    //
    // A single full-sized extra gap is preferred to two half-sized ones for
    // vertical to allow better joining to solid characters without creating
    // visible half-sized gaps. Unlike horizontal, centering is a lot less
    // important, visually.

    // Because of the extra gap at the bottom, there are as many gaps as
    // there are dashes.
    // const gap_count = count;
    let gap_count = count;

    // We need at least 1 pixel for each gap and each dash, if we don't
    // have that then we can't draw our dashed line correctly so we just
    // draw a solid line and return.
    // if (metrics.cell_height < count + gap_count) {
    //     vlineMiddle(metrics, canvas, .light);
    //     return;
    // }
    if metrics.cell_height < count as u32 + gap_count as u32 {
        vlineMiddle(metrics, canvas, Thickness::Light);
        return;
    }

    // We never want the gaps to take up more than 50% of the space,
    // because if they do the dashes are too small and look wrong.
    // const gap_height: i32 = @intCast(@min(desired_gap, metrics.cell_height / (2 * count)));
    let gap_height: i32 = desired_gap.min(metrics.cell_height / (2 * count as u32)) as i32;
    // const total_gap_height: i32 = gap_count * gap_height;
    let total_gap_height: i32 = gap_count as i32 * gap_height;
    // const total_dash_height: i32 = @as(i32, @intCast(metrics.cell_height)) - total_gap_height;
    let total_dash_height: i32 = metrics.cell_height as i32 - total_gap_height;
    // const dash_height: i32 = @divFloor(total_dash_height, count);
    let dash_height: i32 = total_dash_height.div_euclid(count as i32);
    // const remaining: i32 = @mod(total_dash_height, count);
    let remaining: i32 = total_dash_height.rem_euclid(count as i32);

    // assert(dash_height * count + gap_height * gap_count + remaining == metrics.cell_height);
    assert(
        dash_height * count as i32 + gap_height * gap_count as i32 + remaining
            == metrics.cell_height as i32,
    );

    // Our dashes should be centered horizontally.
    // const x: i32 = @intCast((metrics.cell_width -| thick_px) / 2);
    let x: i32 = (metrics.cell_width.saturating_sub(thick_px) / 2) as i32;

    // We start at the top of the cell.
    // var y: i32 = 0;
    let mut y: i32 = 0;

    // We'll distribute the extra space in to dash heights, 1px at a
    // time. We prefer this to making gaps larger since that is much
    // more visually obvious.
    // var extra: i32 = remaining;
    let mut extra: i32 = remaining;

    // inline for (0..count) |_| {
    for _ in 0..count {
        // var y1 = y + dash_height;
        let mut y1 = y + dash_height;
        // We distribute left-over size in to dash widths,
        // since it's less obvious there than in the gaps.
        // if (extra > 0) {
        //     extra -= 1;
        //     y1 += 1;
        // }
        if extra > 0 {
            extra -= 1;
            y1 += 1;
        }
        // vline(canvas, y, y1, x, thick_px);
        vline(canvas, y, y1, x, thick_px);
        // Advance by the height of the dash we drew and the height
        // of a gap to get the start of the next dash.
        // y = y1 + gap_height;
        y = y1 + gap_height;
    }
}
