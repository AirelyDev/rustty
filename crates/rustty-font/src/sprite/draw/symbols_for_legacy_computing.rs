//! Symbols for Legacy Computing | U+1FB00...U+1FBFF
//! https://en.wikipedia.org/wiki/Symbols_for_Legacy_Computing
//!
//! 🬀 🬁 🬂 🬃 🬄 🬅 🬆 🬇 🬈 🬉 🬊 🬋 🬌 🬍 🬎 🬏
//! 🬐 🬑 🬒 🬓 🬔 🬕 🬖 🬗 🬘 🬙 🬚 🬛 🬜 🬝 🬞 🬟
//! 🬠 🬡 🬢 🬣 🬤 🬥 🬦 🬧 🬨 🬩 🬪 🬫 🬬 🬭 🬮 🬯
//! 🬰 🬱 🬲 🬳 🬴 🬵 🬶 🬷 🬸 🬹 🬺 🬻 🬼 🬽 🬾 🬿
//! 🭀 🭁 🭂 🭃 🭄 🭅 🭆 🭇 🭈 🭉 🭊 🭋 🭌 🭍 🭎 🭏
//! 🭐 🭑 🭒 🭓 🭔 🭕 🭖 🭗 🭘 🭙 🭚 🭛 🭜 🭝 🭞 🭟
//! 🭠 🭡 🭢 🭣 🭤 🭥 🭦 🭧 🭨 🭩 🭪 🭫 🭬 🭭 🭮 🭯
//! 🭰 🭱 🭲 🭳 🭴 🭵 🭶 🭷 🭸 🭹 🭺 🭻 🭼 🭽 🭾 🭿
//! 🮀 🮁 🮂 🮃 🮄 🮅 🮆 🮇 🮈 🮉 🮊 🮋 🮌 🮍 🮎 🮏
//! 🮐 🮑 🮒   🮔 🮕 🮖 🮗 🮘 🮙 🮚 🮛 🮜 🮝 🮞 🮟
//! 🮠 🮡 🮢 🮣 🮤 🮥 🮦 🮧 🮨 🮩 🮪 🮫 🮬 🮭 🮮 🮯
//! 🮰 🮱 🮲 🮳 🮴 🮵 🮶 🮷 🮸 🮹 🮺 🮻 🮼 🮽 🮾 🮿
//! 🯀 🯁 🯂 🯃 🯄 🯅 🯆 🯇 🯈 🯉 🯊 🯋 🯌 🯍 🯎 🯏
//! 🯐 🯑 🯒 🯓 🯔 🯕 🯖 🯗 🯘 🯙 🯚 🯛 🯜 🯝 🯞 🯟
//! 🯠 🯡 🯢 🯣 🯤 🯥 🯦 🯧 🯨 🯩 🯪 🯫 🯬 🯭 🯮 🯯
//! 🯰 🯱 🯲 🯳 🯴 🯵 🯶 🯷 🯸 🯹
//!

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const assert = @import("../../../quirks.zig").inlineAssert;
//
// const common = @import("common.zig");
// const Thickness = common.Thickness;
// const Alignment = common.Alignment;
// const Fraction = common.Fraction;
// const Corner = common.Corner;
// const Quads = common.Quads;
// const Edge = common.Edge;
// const Shade = common.Shade;
// const fill = common.fill;
//
// const box = @import("box.zig");
// const block = @import("block.zig");
// const geo = @import("geometric_shapes.zig");
//
// const font = @import("../../main.zig");
//
// PORT NOTE: `common` is the sibling `super::common` module; `box`/`block`/`geo`
// are the sibling `super::r#box`/`super::block`/`super::geometric_shapes`
// modules. `font = ../../main.zig` becomes the local crate (`crate::metrics`,
// `crate::sprite`). `std`/`Allocator`/`assert` are only used by the comptime
// octant table in the *supplement* module; this module needs only `std.math.pi`
// (via `std::f64::consts::PI`) for `circle`. `circle` drives the z2d managed
// `Context` (opaque `alpha8` source), so the z2d pattern/pixel types are pulled
// in for it.
#![allow(non_snake_case)]
// PORT NOTE: the reference `pub fn`s keep their `drawXXXX_YYYY` names and the
// utility fraction constants (`one_eighth`, ...) are lowercase; preserved
// verbatim rather than renamed.
#![allow(non_upper_case_globals)]

use super::common::{
    fill, Alignment, Corner, Edge, Fraction, Horizontal, Quads, Shade, Thickness, Vertical,
};
use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color, Line, Point, Rect};
use rustty_z2d::painter::FillError;
use rustty_z2d::pattern::{OpaquePattern, Pattern};
use rustty_z2d::pixel::{Alpha8, Pixel};
use std::f64::consts::PI;

// Utility names for common fractions
// const one_eighth: f64 = 0.125;
const one_eighth: f64 = 0.125;
// const one_quarter: f64 = 0.25;
const one_quarter: f64 = 0.25;
// const one_third: f64 = (1.0 / 3.0);
const one_third: f64 = 1.0 / 3.0;
// const three_eighths: f64 = 0.375;
const three_eighths: f64 = 0.375;
// const half: f64 = 0.5;
const half: f64 = 0.5;
// const five_eighths: f64 = 0.625;
const five_eighths: f64 = 0.625;
// const two_thirds: f64 = (2.0 / 3.0);
const two_thirds: f64 = 2.0 / 3.0;
// const three_quarters: f64 = 0.75;
const three_quarters: f64 = 0.75;
// const seven_eighths: f64 = 0.875;
const seven_eighths: f64 = 0.875;

// const SmoothMosaic = packed struct(u10) {
//     tl: bool,
//     ul: bool,
//     ll: bool,
//     bl: bool,
//     bc: bool,
//     br: bool,
//     lr: bool,
//     ur: bool,
//     tr: bool,
//     tc: bool,
//     ...
// };
//
// PORT NOTE: the reference packs ten flags into a `u10`; the Rust port carries
// them as ten `bool` fields, which preserves the semantics used at every call
// site (only field reads, no bit-cast).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct SmoothMosaic {
    tl: bool,
    ul: bool,
    ll: bool,
    bl: bool,
    bc: bool,
    br: bool,
    lr: bool,
    ur: bool,
    tr: bool,
    tc: bool,
}

impl SmoothMosaic {
    // fn from(comptime pattern: *const [15:0]u8) SmoothMosaic {
    //
    // PORT NOTE: the reference takes a comptime `[15:0]u8` multiline-string
    // pattern (a 4x3 grid: rows at bytes 0..3, 4..7, 8..11, 12..15 with `\n`
    // separators at 3/7/11). The Rust port takes the same 15 bytes as a byte
    // string `&[u8; 15]`; only the `#` positions matter (any non-`#` byte —
    // `.`, `/`, `\` — reads as `false`), so it is a plain (non-comptime) fn.
    fn from(pattern: &[u8; 15]) -> SmoothMosaic {
        // return .{
        SmoothMosaic {
            // .tl = pattern[0] == '#',
            tl: pattern[0] == b'#',

            // .ul = pattern[4] == '#' and
            //     (pattern[0] != '#' or pattern[8] != '#'),
            ul: pattern[4] == b'#' && (pattern[0] != b'#' || pattern[8] != b'#'),

            // .ll = pattern[8] == '#' and
            //     (pattern[4] != '#' or pattern[12] != '#'),
            ll: pattern[8] == b'#' && (pattern[4] != b'#' || pattern[12] != b'#'),

            // .bl = pattern[12] == '#',
            bl: pattern[12] == b'#',

            // .bc = pattern[13] == '#' and
            //     (pattern[12] != '#' or pattern[14] != '#'),
            bc: pattern[13] == b'#' && (pattern[12] != b'#' || pattern[14] != b'#'),

            // .br = pattern[14] == '#',
            br: pattern[14] == b'#',

            // .lr = pattern[10] == '#' and
            //     (pattern[14] != '#' or pattern[6] != '#'),
            lr: pattern[10] == b'#' && (pattern[14] != b'#' || pattern[6] != b'#'),

            // .ur = pattern[6] == '#' and
            //     (pattern[10] != '#' or pattern[2] != '#'),
            ur: pattern[6] == b'#' && (pattern[10] != b'#' || pattern[2] != b'#'),

            // .tr = pattern[2] == '#',
            tr: pattern[2] == b'#',

            // .tc = pattern[1] == '#' and
            //     (pattern[2] != '#' or pattern[0] != '#'),
            tc: pattern[1] == b'#' && (pattern[2] != b'#' || pattern[0] != b'#'),
        }
    }
}

/// Sextants
// pub fn draw1FB00_1FB3B(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
//
// PORT NOTE: the reference `assert` is the explicit `cp >= 0x1fb00 and cp <=
// 0x1fb3b`; kept as-written (rather than a `RangeInclusive::contains`) to mirror
// the source, so `manual_range_contains` is allowed here.
#[allow(clippy::manual_range_contains)]
pub fn draw1FB00_1FB3B(
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

    // const Sextants = packed struct(u6) {
    //     tl: bool,
    //     tr: bool,
    //     ml: bool,
    //     mr: bool,
    //     bl: bool,
    //     br: bool,
    // };
    //
    // PORT NOTE: the reference's local `packed struct(u6)` cast target is
    // modeled by extracting each bit from the `@bitCast` source (bit 0 = tl,
    // bit 1 = tr, ... bit 5 = br), reproducing the layout used below.
    struct Sextants {
        tl: bool,
        tr: bool,
        ml: bool,
        mr: bool,
        bl: bool,
        br: bool,
    }

    // assert(cp >= 0x1fb00 and cp <= 0x1fb3b);
    assert!(cp >= 0x1fb00 && cp <= 0x1fb3b);
    // const idx = cp - 0x1fb00;
    let idx = cp - 0x1fb00;
    // const sex: Sextants = @bitCast(@as(u6, @intCast(
    //     idx + (idx / 0x14) + 1,
    // )));
    let sex: Sextants = {
        let bits = ((idx + (idx / 0x14) + 1) & 0x3f) as u8;
        Sextants {
            tl: bits & 0b000001 != 0,
            tr: bits & 0b000010 != 0,
            ml: bits & 0b000100 != 0,
            mr: bits & 0b001000 != 0,
            bl: bits & 0b010000 != 0,
            br: bits & 0b100000 != 0,
        }
    };
    // if (sex.tl) fill(metrics, canvas, .zero, .half, .zero, .one_third);
    if sex.tl {
        fill(
            metrics,
            canvas,
            Fraction::Zero,
            Fraction::Half,
            Fraction::Zero,
            Fraction::OneThird,
        );
    }
    // if (sex.tr) fill(metrics, canvas, .half, .full, .zero, .one_third);
    if sex.tr {
        fill(
            metrics,
            canvas,
            Fraction::Half,
            Fraction::Full,
            Fraction::Zero,
            Fraction::OneThird,
        );
    }
    // if (sex.ml) fill(metrics, canvas, .zero, .half, .one_third, .two_thirds);
    if sex.ml {
        fill(
            metrics,
            canvas,
            Fraction::Zero,
            Fraction::Half,
            Fraction::OneThird,
            Fraction::TwoThirds,
        );
    }
    // if (sex.mr) fill(metrics, canvas, .half, .full, .one_third, .two_thirds);
    if sex.mr {
        fill(
            metrics,
            canvas,
            Fraction::Half,
            Fraction::Full,
            Fraction::OneThird,
            Fraction::TwoThirds,
        );
    }
    // if (sex.bl) fill(metrics, canvas, .zero, .half, .two_thirds, .end);
    if sex.bl {
        fill(
            metrics,
            canvas,
            Fraction::Zero,
            Fraction::Half,
            Fraction::TwoThirds,
            Fraction::End,
        );
    }
    // if (sex.br) fill(metrics, canvas, .half, .full, .two_thirds, .end);
    if sex.br {
        fill(
            metrics,
            canvas,
            Fraction::Half,
            Fraction::Full,
            Fraction::TwoThirds,
            Fraction::End,
        );
    }

    Ok(())
}

/// Smooth Mosaics
// pub fn draw1FB3C_1FB67(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FB3C_1FB67(
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

    // Hand written lookup table for these shapes since I couldn't
    // determine any sort of mathematical pattern in the codepoints.
    // const mosaic: SmoothMosaic = switch (cp) {
    let mosaic: SmoothMosaic = match cp {
        // '🬼'
        // 0x1fb3c => SmoothMosaic.from(
        //     \\...
        //     \\...
        //     \\#..
        //     \\##.
        // ),
        0x1fb3c => SmoothMosaic::from(b"...\n...\n#..\n##."),
        // '🬽'
        // 0x1fb3d => SmoothMosaic.from(
        //     \\...
        //     \\...
        //     \\#\.
        //     \\###
        // ),
        0x1fb3d => SmoothMosaic::from(b"...\n...\n#\\.\n###"),
        // '🬾'
        // 0x1fb3e => SmoothMosaic.from(
        //     \\...
        //     \\#..
        //     \\#\.
        //     \\##.
        // ),
        0x1fb3e => SmoothMosaic::from(b"...\n#..\n#\\.\n##."),
        // '🬿'
        // 0x1fb3f => SmoothMosaic.from(
        //     \\...
        //     \\#..
        //     \\##.
        //     \\###
        // ),
        0x1fb3f => SmoothMosaic::from(b"...\n#..\n##.\n###"),
        // '🭀'
        // 0x1fb40 => SmoothMosaic.from(
        //     \\#..
        //     \\#..
        //     \\##.
        //     \\##.
        // ),
        0x1fb40 => SmoothMosaic::from(b"#..\n#..\n##.\n##."),

        // '🭁'
        // 0x1fb41 => SmoothMosaic.from(
        //     \\/##
        //     \\###
        //     \\###
        //     \\###
        // ),
        0x1fb41 => SmoothMosaic::from(b"/##\n###\n###\n###"),
        // '🭂'
        // 0x1fb42 => SmoothMosaic.from(
        //     \\./#
        //     \\###
        //     \\###
        //     \\###
        // ),
        0x1fb42 => SmoothMosaic::from(b"./#\n###\n###\n###"),
        // '🭃'
        // 0x1fb43 => SmoothMosaic.from(
        //     \\.##
        //     \\.##
        //     \\###
        //     \\###
        // ),
        0x1fb43 => SmoothMosaic::from(b".##\n.##\n###\n###"),
        // '🭄'
        // 0x1fb44 => SmoothMosaic.from(
        //     \\..#
        //     \\.##
        //     \\###
        //     \\###
        // ),
        0x1fb44 => SmoothMosaic::from(b"..#\n.##\n###\n###"),
        // '🭅'
        // 0x1fb45 => SmoothMosaic.from(
        //     \\.##
        //     \\.##
        //     \\.##
        //     \\###
        // ),
        0x1fb45 => SmoothMosaic::from(b".##\n.##\n.##\n###"),
        // '🭆'
        // 0x1fb46 => SmoothMosaic.from(
        //     \\...
        //     \\./#
        //     \\###
        //     \\###
        // ),
        0x1fb46 => SmoothMosaic::from(b"...\n./#\n###\n###"),

        // '🭇'
        // 0x1fb47 => SmoothMosaic.from(
        //     \\...
        //     \\...
        //     \\..#
        //     \\.##
        // ),
        0x1fb47 => SmoothMosaic::from(b"...\n...\n..#\n.##"),
        // '🭈'
        // 0x1fb48 => SmoothMosaic.from(
        //     \\...
        //     \\...
        //     \\./#
        //     \\###
        // ),
        0x1fb48 => SmoothMosaic::from(b"...\n...\n./#\n###"),
        // '🭉'
        // 0x1fb49 => SmoothMosaic.from(
        //     \\...
        //     \\..#
        //     \\./#
        //     \\.##
        // ),
        0x1fb49 => SmoothMosaic::from(b"...\n..#\n./#\n.##"),
        // '🭊'
        // 0x1fb4a => SmoothMosaic.from(
        //     \\...
        //     \\..#
        //     \\.##
        //     \\###
        // ),
        0x1fb4a => SmoothMosaic::from(b"...\n..#\n.##\n###"),
        // '🭋'
        // 0x1fb4b => SmoothMosaic.from(
        //     \\..#
        //     \\..#
        //     \\.##
        //     \\.##
        // ),
        0x1fb4b => SmoothMosaic::from(b"..#\n..#\n.##\n.##"),

        // '🭌'
        // 0x1fb4c => SmoothMosaic.from(
        //     \\##\
        //     \\###
        //     \\###
        //     \\###
        // ),
        0x1fb4c => SmoothMosaic::from(b"##\\\n###\n###\n###"),
        // '🭍'
        // 0x1fb4d => SmoothMosaic.from(
        //     \\#\.
        //     \\###
        //     \\###
        //     \\###
        // ),
        0x1fb4d => SmoothMosaic::from(b"#\\.\n###\n###\n###"),
        // '🭎'
        // 0x1fb4e => SmoothMosaic.from(
        //     \\##.
        //     \\##.
        //     \\###
        //     \\###
        // ),
        0x1fb4e => SmoothMosaic::from(b"##.\n##.\n###\n###"),
        // '🭏'
        // 0x1fb4f => SmoothMosaic.from(
        //     \\#..
        //     \\##.
        //     \\###
        //     \\###
        // ),
        0x1fb4f => SmoothMosaic::from(b"#..\n##.\n###\n###"),
        // '🭐'
        // 0x1fb50 => SmoothMosaic.from(
        //     \\##.
        //     \\##.
        //     \\##.
        //     \\###
        // ),
        0x1fb50 => SmoothMosaic::from(b"##.\n##.\n##.\n###"),
        // '🭑'
        // 0x1fb51 => SmoothMosaic.from(
        //     \\...
        //     \\#\.
        //     \\###
        //     \\###
        // ),
        0x1fb51 => SmoothMosaic::from(b"...\n#\\.\n###\n###"),

        // '🭒'
        // 0x1fb52 => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\###
        //     \\\##
        // ),
        0x1fb52 => SmoothMosaic::from(b"###\n###\n###\n\\##"),
        // '🭓'
        // 0x1fb53 => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\###
        //     \\.\#
        // ),
        0x1fb53 => SmoothMosaic::from(b"###\n###\n###\n.\\#"),
        // '🭔'
        // 0x1fb54 => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\.##
        //     \\.##
        // ),
        0x1fb54 => SmoothMosaic::from(b"###\n###\n.##\n.##"),
        // '🭕'
        // 0x1fb55 => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\.##
        //     \\..#
        // ),
        0x1fb55 => SmoothMosaic::from(b"###\n###\n.##\n..#"),
        // '🭖'
        // 0x1fb56 => SmoothMosaic.from(
        //     \\###
        //     \\.##
        //     \\.##
        //     \\.##
        // ),
        0x1fb56 => SmoothMosaic::from(b"###\n.##\n.##\n.##"),

        // '🭗'
        // 0x1fb57 => SmoothMosaic.from(
        //     \\##.
        //     \\#..
        //     \\...
        //     \\...
        // ),
        0x1fb57 => SmoothMosaic::from(b"##.\n#..\n...\n..."),
        // '🭘'
        // 0x1fb58 => SmoothMosaic.from(
        //     \\###
        //     \\#/.
        //     \\...
        //     \\...
        // ),
        0x1fb58 => SmoothMosaic::from(b"###\n#/.\n...\n..."),
        // '🭙'
        // 0x1fb59 => SmoothMosaic.from(
        //     \\##.
        //     \\#/.
        //     \\#..
        //     \\...
        // ),
        0x1fb59 => SmoothMosaic::from(b"##.\n#/.\n#..\n..."),
        // '🭚'
        // 0x1fb5a => SmoothMosaic.from(
        //     \\###
        //     \\##.
        //     \\#..
        //     \\...
        // ),
        0x1fb5a => SmoothMosaic::from(b"###\n##.\n#..\n..."),
        // '🭛'
        // 0x1fb5b => SmoothMosaic.from(
        //     \\##.
        //     \\##.
        //     \\#..
        //     \\#..
        // ),
        0x1fb5b => SmoothMosaic::from(b"##.\n##.\n#..\n#.."),

        // '🭜'
        // 0x1fb5c => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\#/.
        //     \\...
        // ),
        0x1fb5c => SmoothMosaic::from(b"###\n###\n#/.\n..."),
        // '🭝'
        // 0x1fb5d => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\###
        //     \\##/
        // ),
        0x1fb5d => SmoothMosaic::from(b"###\n###\n###\n##/"),
        // '🭞'
        // 0x1fb5e => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\###
        //     \\#/.
        // ),
        0x1fb5e => SmoothMosaic::from(b"###\n###\n###\n#/."),
        // '🭟'
        // 0x1fb5f => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\##.
        //     \\##.
        // ),
        0x1fb5f => SmoothMosaic::from(b"###\n###\n##.\n##."),
        // '🭠'
        // 0x1fb60 => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\##.
        //     \\#..
        // ),
        0x1fb60 => SmoothMosaic::from(b"###\n###\n##.\n#.."),
        // '🭡'
        // 0x1fb61 => SmoothMosaic.from(
        //     \\###
        //     \\##.
        //     \\##.
        //     \\##.
        // ),
        0x1fb61 => SmoothMosaic::from(b"###\n##.\n##.\n##."),

        // '🭢'
        // 0x1fb62 => SmoothMosaic.from(
        //     \\.##
        //     \\..#
        //     \\...
        //     \\...
        // ),
        0x1fb62 => SmoothMosaic::from(b".##\n..#\n...\n..."),
        // '🭣'
        // 0x1fb63 => SmoothMosaic.from(
        //     \\###
        //     \\.\#
        //     \\...
        //     \\...
        // ),
        0x1fb63 => SmoothMosaic::from(b"###\n.\\#\n...\n..."),
        // '🭤'
        // 0x1fb64 => SmoothMosaic.from(
        //     \\.##
        //     \\.\#
        //     \\..#
        //     \\...
        // ),
        0x1fb64 => SmoothMosaic::from(b".##\n.\\#\n..#\n..."),
        // '🭥'
        // 0x1fb65 => SmoothMosaic.from(
        //     \\###
        //     \\.##
        //     \\..#
        //     \\...
        // ),
        0x1fb65 => SmoothMosaic::from(b"###\n.##\n..#\n..."),
        // '🭦'
        // 0x1fb66 => SmoothMosaic.from(
        //     \\.##
        //     \\.##
        //     \\..#
        //     \\..#
        // ),
        0x1fb66 => SmoothMosaic::from(b".##\n.##\n..#\n..#"),
        // '🭧'
        // 0x1fb67 => SmoothMosaic.from(
        //     \\###
        //     \\###
        //     \\.\#
        //     \\...
        // ),
        0x1fb67 => SmoothMosaic::from(b"###\n###\n.\\#\n..."),
        // else => unreachable,
        _ => unreachable!(),
    };

    // const top: f64 = 0.0;
    let top: f64 = 0.0;
    // const upper: f64 = Fraction.one_third.float(metrics.cell_height);
    let upper: f64 = Fraction::OneThird.float(metrics.cell_height);
    // const lower: f64 = Fraction.two_thirds.float(metrics.cell_height);
    let lower: f64 = Fraction::TwoThirds.float(metrics.cell_height);
    // const bottom: f64 = @floatFromInt(metrics.cell_height);
    let bottom: f64 = metrics.cell_height as f64;
    // const left: f64 = 0.0;
    let left: f64 = 0.0;
    // const center: f64 = Fraction.half.float(metrics.cell_width);
    let center: f64 = Fraction::Half.float(metrics.cell_width);
    // const right: f64 = @floatFromInt(metrics.cell_width);
    let right: f64 = metrics.cell_width as f64;

    // var path = canvas.staticPath(12); // nodes.len = 0
    let mut path = canvas.staticPath::<12>(); // nodes.len = 0
                                              // if (mosaic.tl) path.lineTo(left, top); // +1, nodes.len = 1
    if mosaic.tl {
        path.line_to(left, top);
    } // +1, nodes.len = 1
      // if (mosaic.ul) path.lineTo(left, upper); // +1, nodes.len = 2
    if mosaic.ul {
        path.line_to(left, upper);
    } // +1, nodes.len = 2
      // if (mosaic.ll) path.lineTo(left, lower); // +1, nodes.len = 3
    if mosaic.ll {
        path.line_to(left, lower);
    } // +1, nodes.len = 3
      // if (mosaic.bl) path.lineTo(left, bottom); // +1, nodes.len = 4
    if mosaic.bl {
        path.line_to(left, bottom);
    } // +1, nodes.len = 4
      // if (mosaic.bc) path.lineTo(center, bottom); // +1, nodes.len = 5
    if mosaic.bc {
        path.line_to(center, bottom);
    } // +1, nodes.len = 5
      // if (mosaic.br) path.lineTo(right, bottom); // +1, nodes.len = 6
    if mosaic.br {
        path.line_to(right, bottom);
    } // +1, nodes.len = 6
      // if (mosaic.lr) path.lineTo(right, lower); // +1, nodes.len = 7
    if mosaic.lr {
        path.line_to(right, lower);
    } // +1, nodes.len = 7
      // if (mosaic.ur) path.lineTo(right, upper); // +1, nodes.len = 8
    if mosaic.ur {
        path.line_to(right, upper);
    } // +1, nodes.len = 8
      // if (mosaic.tr) path.lineTo(right, top); // +1, nodes.len = 9
    if mosaic.tr {
        path.line_to(right, top);
    } // +1, nodes.len = 9
      // if (mosaic.tc) path.lineTo(center, top); // +1, nodes.len = 10
    if mosaic.tc {
        path.line_to(center, top);
    } // +1, nodes.len = 10
      // path.close(); // +2, nodes.len = 12
    path.close(); // +2, nodes.len = 12

    // try canvas.fillPath(path.wrapped_path, .{}, .on);
    canvas.fillPath(&path.wrapped_path, Default::default(), Color::ON)
}

// pub fn draw1FB68_1FB6F(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FB68_1FB6F(
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
        // '🭨'
        // 0x1fb68 => {
        0x1fb68 => {
            // try edgeTriangle(metrics, canvas, .left);
            edgeTriangle(metrics, canvas, Edge::Left)?;
            // canvas.invert();
            canvas.invert();
            // Set the clip so we don't include anything outside of the cell.
            // canvas.clip_left = canvas.padding_x;
            canvas.clip_left = canvas.padding_x;
            // canvas.clip_right = canvas.padding_x;
            canvas.clip_right = canvas.padding_x;
            // canvas.clip_top = canvas.padding_y;
            canvas.clip_top = canvas.padding_y;
            // canvas.clip_bottom = canvas.padding_y;
            canvas.clip_bottom = canvas.padding_y;
        }
        // '🭩'
        // 0x1fb69 => {
        0x1fb69 => {
            // try edgeTriangle(metrics, canvas, .top);
            edgeTriangle(metrics, canvas, Edge::Top)?;
            // canvas.invert();
            canvas.invert();
            // Set the clip so we don't include anything outside of the cell.
            // canvas.clip_left = canvas.padding_x;
            canvas.clip_left = canvas.padding_x;
            // canvas.clip_right = canvas.padding_x;
            canvas.clip_right = canvas.padding_x;
            // canvas.clip_top = canvas.padding_y;
            canvas.clip_top = canvas.padding_y;
            // canvas.clip_bottom = canvas.padding_y;
            canvas.clip_bottom = canvas.padding_y;
        }
        // '🭪'
        // 0x1fb6a => {
        0x1fb6a => {
            // try edgeTriangle(metrics, canvas, .right);
            edgeTriangle(metrics, canvas, Edge::Right)?;
            // canvas.invert();
            canvas.invert();
            // Set the clip so we don't include anything outside of the cell.
            // canvas.clip_left = canvas.padding_x;
            canvas.clip_left = canvas.padding_x;
            // canvas.clip_right = canvas.padding_x;
            canvas.clip_right = canvas.padding_x;
            // canvas.clip_top = canvas.padding_y;
            canvas.clip_top = canvas.padding_y;
            // canvas.clip_bottom = canvas.padding_y;
            canvas.clip_bottom = canvas.padding_y;
        }
        // '🭫'
        // 0x1fb6b => {
        0x1fb6b => {
            // try edgeTriangle(metrics, canvas, .bottom);
            edgeTriangle(metrics, canvas, Edge::Bottom)?;
            // canvas.invert();
            canvas.invert();
            // Set the clip so we don't include anything outside of the cell.
            // canvas.clip_left = canvas.padding_x;
            canvas.clip_left = canvas.padding_x;
            // canvas.clip_right = canvas.padding_x;
            canvas.clip_right = canvas.padding_x;
            // canvas.clip_top = canvas.padding_y;
            canvas.clip_top = canvas.padding_y;
            // canvas.clip_bottom = canvas.padding_y;
            canvas.clip_bottom = canvas.padding_y;
        }
        // '🭬'
        // 0x1fb6c => try edgeTriangle(metrics, canvas, .left),
        0x1fb6c => edgeTriangle(metrics, canvas, Edge::Left)?,
        // '🭭'
        // 0x1fb6d => try edgeTriangle(metrics, canvas, .top),
        0x1fb6d => edgeTriangle(metrics, canvas, Edge::Top)?,
        // '🭮'
        // 0x1fb6e => try edgeTriangle(metrics, canvas, .right),
        0x1fb6e => edgeTriangle(metrics, canvas, Edge::Right)?,
        // '🭯'
        // 0x1fb6f => try edgeTriangle(metrics, canvas, .bottom),
        0x1fb6f => edgeTriangle(metrics, canvas, Edge::Bottom)?,

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

/// Vertical one eighth blocks
// pub fn draw1FB70_1FB75(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FB70_1FB75(
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

    // const n = cp + 1 - 0x1fb70;
    let n = (cp + 1 - 0x1fb70) as usize;

    // fill(
    //     metrics,
    //     canvas,
    //     Fraction.eighths[n],
    //     Fraction.eighths[n + 1],
    //     .top,
    //     .bottom,
    // );
    fill(
        metrics,
        canvas,
        Fraction::eighths[n],
        Fraction::eighths[n + 1],
        Fraction::Top,
        Fraction::Bottom,
    );

    Ok(())
}

/// Horizontal one eighth blocks
// pub fn draw1FB76_1FB7B(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FB76_1FB7B(
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

    // const n = cp + 1 - 0x1fb76;
    let n = (cp + 1 - 0x1fb76) as usize;

    // fill(
    //     metrics,
    //     canvas,
    //     .left,
    //     .right,
    //     Fraction.eighths[n],
    //     Fraction.eighths[n + 1],
    // );
    fill(
        metrics,
        canvas,
        Fraction::Left,
        Fraction::Right,
        Fraction::eighths[n],
        Fraction::eighths[n + 1],
    );

    Ok(())
}

// pub fn draw1FB7C_1FB97(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FB7C_1FB97(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // switch (cp) {
    match cp {
        // '🭼' LEFT AND LOWER ONE EIGHTH BLOCK
        // 0x1fb7c => {
        0x1fb7c => {
            // block.block(metrics, canvas, .left, one_eighth, 1);
            super::block::block(metrics, canvas, Alignment::LEFT, one_eighth, 1.0);
            // block.block(metrics, canvas, .lower, 1, one_eighth);
            super::block::block(metrics, canvas, Alignment::LOWER, 1.0, one_eighth);
        }
        // '🭽' LEFT AND UPPER ONE EIGHTH BLOCK
        // 0x1fb7d => {
        0x1fb7d => {
            // block.block(metrics, canvas, .left, one_eighth, 1);
            super::block::block(metrics, canvas, Alignment::LEFT, one_eighth, 1.0);
            // block.block(metrics, canvas, .upper, 1, one_eighth);
            super::block::block(metrics, canvas, Alignment::UPPER, 1.0, one_eighth);
        }
        // '🭾' RIGHT AND UPPER ONE EIGHTH BLOCK
        // 0x1fb7e => {
        0x1fb7e => {
            // block.block(metrics, canvas, .right, one_eighth, 1);
            super::block::block(metrics, canvas, Alignment::RIGHT, one_eighth, 1.0);
            // block.block(metrics, canvas, .upper, 1, one_eighth);
            super::block::block(metrics, canvas, Alignment::UPPER, 1.0, one_eighth);
        }
        // '🭿' RIGHT AND LOWER ONE EIGHTH BLOCK
        // 0x1fb7f => {
        0x1fb7f => {
            // block.block(metrics, canvas, .right, one_eighth, 1);
            super::block::block(metrics, canvas, Alignment::RIGHT, one_eighth, 1.0);
            // block.block(metrics, canvas, .lower, 1, one_eighth);
            super::block::block(metrics, canvas, Alignment::LOWER, 1.0, one_eighth);
        }
        // '🮀' UPPER AND LOWER ONE EIGHTH BLOCK
        // 0x1fb80 => {
        0x1fb80 => {
            // block.block(metrics, canvas, .upper, 1, one_eighth);
            super::block::block(metrics, canvas, Alignment::UPPER, 1.0, one_eighth);
            // block.block(metrics, canvas, .lower, 1, one_eighth);
            super::block::block(metrics, canvas, Alignment::LOWER, 1.0, one_eighth);
        }
        // '🮁' Horizontal One Eighth Block 1358
        // 0x1fb81 => {
        0x1fb81 => {
            // We just call the draw function for each of the relevant codepoints.
            // The first codepoint is actually a lie, it's before the range, but
            // we need it to get the first (0th) block position. This might be a
            // bit brittle, oh well, if it breaks we can fix it.
            // try draw1FB76_1FB7B(0x1fb74 + 1, canvas, width, height, metrics);
            draw1FB76_1FB7B(0x1fb74 + 1, canvas, width, height, metrics)?;
            // try draw1FB76_1FB7B(0x1fb74 + 3, canvas, width, height, metrics);
            draw1FB76_1FB7B(0x1fb74 + 3, canvas, width, height, metrics)?;
            // try draw1FB76_1FB7B(0x1fb74 + 5, canvas, width, height, metrics);
            draw1FB76_1FB7B(0x1fb74 + 5, canvas, width, height, metrics)?;
            // try draw1FB76_1FB7B(0x1fb74 + 8, canvas, width, height, metrics);
            draw1FB76_1FB7B(0x1fb74 + 8, canvas, width, height, metrics)?;
        }

        // '🮂' UPPER ONE QUARTER BLOCK
        // 0x1fb82 => block.block(metrics, canvas, .upper, 1, one_quarter),
        0x1fb82 => super::block::block(metrics, canvas, Alignment::UPPER, 1.0, one_quarter),
        // '🮃' UPPER THREE EIGHTHS BLOCK
        // 0x1fb83 => block.block(metrics, canvas, .upper, 1, three_eighths),
        0x1fb83 => super::block::block(metrics, canvas, Alignment::UPPER, 1.0, three_eighths),
        // '🮄' UPPER FIVE EIGHTHS BLOCK
        // 0x1fb84 => block.block(metrics, canvas, .upper, 1, five_eighths),
        0x1fb84 => super::block::block(metrics, canvas, Alignment::UPPER, 1.0, five_eighths),
        // '🮅' UPPER THREE QUARTERS BLOCK
        // 0x1fb85 => block.block(metrics, canvas, .upper, 1, three_quarters),
        0x1fb85 => super::block::block(metrics, canvas, Alignment::UPPER, 1.0, three_quarters),
        // '🮆' UPPER SEVEN EIGHTHS BLOCK
        // 0x1fb86 => block.block(metrics, canvas, .upper, 1, seven_eighths),
        0x1fb86 => super::block::block(metrics, canvas, Alignment::UPPER, 1.0, seven_eighths),

        // '🮇' RIGHT ONE QUARTER BLOCK
        // 0x1fb87 => block.block(metrics, canvas, .right, one_quarter, 1),
        0x1fb87 => super::block::block(metrics, canvas, Alignment::RIGHT, one_quarter, 1.0),
        // '🮈' RIGHT THREE EIGHTHS BLOCK
        // 0x1fb88 => block.block(metrics, canvas, .right, three_eighths, 1),
        0x1fb88 => super::block::block(metrics, canvas, Alignment::RIGHT, three_eighths, 1.0),
        // '🮉' RIGHT FIVE EIGHTHS BLOCK
        // 0x1fb89 => block.block(metrics, canvas, .right, five_eighths, 1),
        0x1fb89 => super::block::block(metrics, canvas, Alignment::RIGHT, five_eighths, 1.0),
        // '🮊' RIGHT THREE QUARTERS BLOCK
        // 0x1fb8a => block.block(metrics, canvas, .right, three_quarters, 1),
        0x1fb8a => super::block::block(metrics, canvas, Alignment::RIGHT, three_quarters, 1.0),
        // '🮋' RIGHT SEVEN EIGHTHS BLOCK/
        // 0x1fb8b => block.block(metrics, canvas, .right, seven_eighths, 1),
        0x1fb8b => super::block::block(metrics, canvas, Alignment::RIGHT, seven_eighths, 1.0),

        // '🮌'
        // 0x1fb8c => block.blockShade(metrics, canvas, .left, half, 1, .medium),
        0x1fb8c => {
            super::block::blockShade(metrics, canvas, Alignment::LEFT, half, 1.0, Shade::MEDIUM)
        }
        // '🮍'
        // 0x1fb8d => block.blockShade(metrics, canvas, .right, half, 1, .medium),
        0x1fb8d => {
            super::block::blockShade(metrics, canvas, Alignment::RIGHT, half, 1.0, Shade::MEDIUM)
        }
        // '🮎'
        // 0x1fb8e => block.blockShade(metrics, canvas, .upper, 1, half, .medium),
        0x1fb8e => {
            super::block::blockShade(metrics, canvas, Alignment::UPPER, 1.0, half, Shade::MEDIUM)
        }
        // '🮏'
        // 0x1fb8f => block.blockShade(metrics, canvas, .lower, 1, half, .medium),
        0x1fb8f => {
            super::block::blockShade(metrics, canvas, Alignment::LOWER, 1.0, half, Shade::MEDIUM)
        }

        // '🮐'
        // 0x1fb90 => block.fullBlockShade(metrics, canvas, .medium),
        0x1fb90 => super::block::fullBlockShade(metrics, canvas, Shade::MEDIUM),
        // '🮑'
        // 0x1fb91 => {
        0x1fb91 => {
            // block.fullBlockShade(metrics, canvas, .medium);
            super::block::fullBlockShade(metrics, canvas, Shade::MEDIUM);
            // block.block(metrics, canvas, .upper, 1, half);
            super::block::block(metrics, canvas, Alignment::UPPER, 1.0, half);
        }
        // '🮒'
        // 0x1fb92 => {
        0x1fb92 => {
            // block.fullBlockShade(metrics, canvas, .medium);
            super::block::fullBlockShade(metrics, canvas, Shade::MEDIUM);
            // block.block(metrics, canvas, .lower, 1, half);
            super::block::block(metrics, canvas, Alignment::LOWER, 1.0, half);
        }
        // 0x1fb93 => {
        0x1fb93 => {
            // NOTE: This codepoint is currently un-allocated, it's a hole
            //       in the unicode block, so it's safe to just render it
            //       as an empty glyph, probably.
        }
        // '🮔'
        // 0x1fb94 => {
        0x1fb94 => {
            // block.fullBlockShade(metrics, canvas, .medium);
            super::block::fullBlockShade(metrics, canvas, Shade::MEDIUM);
            // block.block(metrics, canvas, .right, half, 1);
            super::block::block(metrics, canvas, Alignment::RIGHT, half, 1.0);
        }
        // '🮕'
        // 0x1fb95 => checkerboardFill(metrics, canvas, 0),
        0x1fb95 => checkerboardFill(metrics, canvas, 0),
        // '🮖'
        // 0x1fb96 => checkerboardFill(metrics, canvas, 1),
        0x1fb96 => checkerboardFill(metrics, canvas, 1),
        // '🮗'
        // 0x1fb97 => {
        0x1fb97 => {
            // canvas.box(
            //     0,
            //     @intCast(height / 4),
            //     @intCast(width),
            //     @intCast(2 * height / 4),
            //     .on,
            // );
            canvas.r#box(
                0,
                (height / 4) as i32,
                width as i32,
                (2 * height / 4) as i32,
                Color::ON,
            );
            // canvas.box(
            //     0,
            //     @intCast(3 * height / 4),
            //     @intCast(width),
            //     @intCast(height),
            //     .on,
            // );
            canvas.r#box(
                0,
                (3 * height / 4) as i32,
                width as i32,
                height as i32,
                Color::ON,
            );
        }

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

/// Upper Left to Lower Right Fill
/// 🮘
// pub fn draw1FB98(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FB98(
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

    // Set the clip so we don't include anything outside of the cell.
    // canvas.clip_left = canvas.padding_x;
    canvas.clip_left = canvas.padding_x;
    // canvas.clip_right = canvas.padding_x;
    canvas.clip_right = canvas.padding_x;
    // canvas.clip_top = canvas.padding_y;
    canvas.clip_top = canvas.padding_y;
    // canvas.clip_bottom = canvas.padding_y;
    canvas.clip_bottom = canvas.padding_y;

    // TODO: This doesn't align properly for most cell sizes, fix that.

    // const thick_px = Thickness.light.height(metrics.box_thickness);
    let thick_px = Thickness::Light.height(metrics.box_thickness);
    // const line_count = metrics.cell_width / (2 * thick_px);
    let line_count = metrics.cell_width / (2 * thick_px);

    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;
    // const float_thick: f64 = @floatFromInt(thick_px);
    let float_thick: f64 = thick_px as f64;
    // const stride = @round(float_width / @as(f64, @floatFromInt(line_count)));
    let stride = (float_width / line_count as f64).round();

    // for (0..line_count * 2 + 1) |_i| {
    for _i in 0..line_count * 2 + 1 {
        // const i = @as(i32, @intCast(_i)) - @as(i32, @intCast(line_count));
        let i = _i as i32 - line_count as i32;
        // const top_x = @as(f64, @floatFromInt(i)) * stride;
        let top_x = i as f64 * stride;
        // const bottom_x = float_width + top_x;
        let bottom_x = float_width + top_x;
        // canvas.line(.{
        //     .p0 = .{ .x = top_x, .y = 0 },
        //     .p1 = .{ .x = bottom_x, .y = float_height },
        // }, float_thick, .on) catch {};
        let _ = canvas.line(
            Line {
                p0: Point { x: top_x, y: 0.0 },
                p1: Point {
                    x: bottom_x,
                    y: float_height,
                },
            },
            float_thick,
            Color::ON,
        );
    }

    Ok(())
}

/// Upper Right to Lower Left Fill
/// 🮙
// pub fn draw1FB99(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FB99(
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

    // Set the clip so we don't include anything outside of the cell.
    // canvas.clip_left = canvas.padding_x;
    canvas.clip_left = canvas.padding_x;
    // canvas.clip_right = canvas.padding_x;
    canvas.clip_right = canvas.padding_x;
    // canvas.clip_top = canvas.padding_y;
    canvas.clip_top = canvas.padding_y;
    // canvas.clip_bottom = canvas.padding_y;
    canvas.clip_bottom = canvas.padding_y;

    // TODO: This doesn't align properly for most cell sizes, fix that.

    // const thick_px = Thickness.light.height(metrics.box_thickness);
    let thick_px = Thickness::Light.height(metrics.box_thickness);
    // const line_count = metrics.cell_width / (2 * thick_px);
    let line_count = metrics.cell_width / (2 * thick_px);

    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;
    // const float_thick: f64 = @floatFromInt(thick_px);
    let float_thick: f64 = thick_px as f64;
    // const stride = @round(float_width / @as(f64, @floatFromInt(line_count)));
    let stride = (float_width / line_count as f64).round();

    // for (0..line_count * 2 + 1) |_i| {
    for _i in 0..line_count * 2 + 1 {
        // const i = @as(i32, @intCast(_i)) - @as(i32, @intCast(line_count));
        let i = _i as i32 - line_count as i32;
        // const bottom_x = @as(f64, @floatFromInt(i)) * stride;
        let bottom_x = i as f64 * stride;
        // const top_x = float_width + bottom_x;
        let top_x = float_width + bottom_x;
        // canvas.line(.{
        //     .p0 = .{ .x = top_x, .y = 0 },
        //     .p1 = .{ .x = bottom_x, .y = float_height },
        // }, float_thick, .on) catch {};
        let _ = canvas.line(
            Line {
                p0: Point { x: top_x, y: 0.0 },
                p1: Point {
                    x: bottom_x,
                    y: float_height,
                },
            },
            float_thick,
            Color::ON,
        );
    }

    Ok(())
}

// pub fn draw1FB9A_1FB9F(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FB9A_1FB9F(
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
        // '🮚'
        // 0x1fb9a => {
        0x1fb9a => {
            // try edgeTriangle(metrics, canvas, .top);
            edgeTriangle(metrics, canvas, Edge::Top)?;
            // try edgeTriangle(metrics, canvas, .bottom);
            edgeTriangle(metrics, canvas, Edge::Bottom)?;
        }
        // '🮛'
        // 0x1fb9b => {
        0x1fb9b => {
            // try edgeTriangle(metrics, canvas, .left);
            edgeTriangle(metrics, canvas, Edge::Left)?;
            // try edgeTriangle(metrics, canvas, .right);
            edgeTriangle(metrics, canvas, Edge::Right)?;
        }
        // '🮜'
        // 0x1fb9c => try geo.cornerTriangleShade(metrics, canvas, .tl, .medium),
        0x1fb9c => super::geometric_shapes::cornerTriangleShade(
            metrics,
            canvas,
            Corner::Tl,
            Shade::MEDIUM,
        )?,
        // '🮝'
        // 0x1fb9d => try geo.cornerTriangleShade(metrics, canvas, .tr, .medium),
        0x1fb9d => super::geometric_shapes::cornerTriangleShade(
            metrics,
            canvas,
            Corner::Tr,
            Shade::MEDIUM,
        )?,
        // '🮞'
        // 0x1fb9e => try geo.cornerTriangleShade(metrics, canvas, .br, .medium),
        0x1fb9e => super::geometric_shapes::cornerTriangleShade(
            metrics,
            canvas,
            Corner::Br,
            Shade::MEDIUM,
        )?,
        // '🮟'
        // 0x1fb9f => try geo.cornerTriangleShade(metrics, canvas, .bl, .medium),
        0x1fb9f => super::geometric_shapes::cornerTriangleShade(
            metrics,
            canvas,
            Corner::Bl,
            Shade::MEDIUM,
        )?,

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

// pub fn draw1FBA0_1FBAE(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FBA0_1FBAE(
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
        // '🮠'
        // 0x1fba0 => cornerDiagonalLines(metrics, canvas, .{ .tl = true }),
        0x1fba0 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tl: true,
                ..Default::default()
            },
        ),
        // '🮡'
        // 0x1fba1 => cornerDiagonalLines(metrics, canvas, .{ .tr = true }),
        0x1fba1 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tr: true,
                ..Default::default()
            },
        ),
        // '🮢'
        // 0x1fba2 => cornerDiagonalLines(metrics, canvas, .{ .bl = true }),
        0x1fba2 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                bl: true,
                ..Default::default()
            },
        ),
        // '🮣'
        // 0x1fba3 => cornerDiagonalLines(metrics, canvas, .{ .br = true }),
        0x1fba3 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                br: true,
                ..Default::default()
            },
        ),
        // '🮤'
        // 0x1fba4 => cornerDiagonalLines(metrics, canvas, .{ .tl = true, .bl = true }),
        0x1fba4 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tl: true,
                bl: true,
                ..Default::default()
            },
        ),
        // '🮥'
        // 0x1fba5 => cornerDiagonalLines(metrics, canvas, .{ .tr = true, .br = true }),
        0x1fba5 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tr: true,
                br: true,
                ..Default::default()
            },
        ),
        // '🮦'
        // 0x1fba6 => cornerDiagonalLines(metrics, canvas, .{ .bl = true, .br = true }),
        0x1fba6 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                bl: true,
                br: true,
                ..Default::default()
            },
        ),
        // '🮧'
        // 0x1fba7 => cornerDiagonalLines(metrics, canvas, .{ .tl = true, .tr = true }),
        0x1fba7 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tl: true,
                tr: true,
                ..Default::default()
            },
        ),
        // '🮨'
        // 0x1fba8 => cornerDiagonalLines(metrics, canvas, .{ .tl = true, .br = true }),
        0x1fba8 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tl: true,
                br: true,
                ..Default::default()
            },
        ),
        // '🮩'
        // 0x1fba9 => cornerDiagonalLines(metrics, canvas, .{ .tr = true, .bl = true }),
        0x1fba9 => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tr: true,
                bl: true,
                ..Default::default()
            },
        ),
        // '🮪'
        // 0x1fbaa => cornerDiagonalLines(metrics, canvas, .{ .tr = true, .bl = true, .br = true }),
        0x1fbaa => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tr: true,
                bl: true,
                br: true,
                ..Default::default()
            },
        ),
        // '🮫'
        // 0x1fbab => cornerDiagonalLines(metrics, canvas, .{ .tl = true, .bl = true, .br = true }),
        0x1fbab => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tl: true,
                bl: true,
                br: true,
                ..Default::default()
            },
        ),
        // '🮬'
        // 0x1fbac => cornerDiagonalLines(metrics, canvas, .{ .tl = true, .tr = true, .br = true }),
        0x1fbac => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tl: true,
                tr: true,
                br: true,
                ..Default::default()
            },
        ),
        // '🮭'
        // 0x1fbad => cornerDiagonalLines(metrics, canvas, .{ .tl = true, .tr = true, .bl = true }),
        0x1fbad => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tl: true,
                tr: true,
                bl: true,
                ..Default::default()
            },
        ),
        // '🮮'
        // 0x1fbae => cornerDiagonalLines(metrics, canvas, .{ .tl = true, .tr = true, .bl = true, .br = true }),
        0x1fbae => cornerDiagonalLines(
            metrics,
            canvas,
            Quads {
                tl: true,
                tr: true,
                bl: true,
                br: true,
            },
        ),

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

/// 🮯
// pub fn draw1FBAF(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FBAF(
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

    // box.linesChar(metrics, canvas, .{
    //     .up = .heavy,
    //     .down = .heavy,
    //     .left = .light,
    //     .right = .light,
    // });
    super::r#box::linesChar(
        metrics,
        canvas,
        super::r#box::Lines {
            up: super::r#box::Style::Heavy,
            down: super::r#box::Style::Heavy,
            left: super::r#box::Style::Light,
            right: super::r#box::Style::Light,
        },
    );

    Ok(())
}

/// 🮽
// pub fn draw1FBBD(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FBBD(
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

    // box.lightDiagonalCross(metrics, canvas);
    super::r#box::lightDiagonalCross(metrics, canvas);
    // canvas.invert();
    canvas.invert();
    // Set the clip so we don't include anything outside of the cell.
    // canvas.clip_left = canvas.padding_x;
    canvas.clip_left = canvas.padding_x;
    // canvas.clip_right = canvas.padding_x;
    canvas.clip_right = canvas.padding_x;
    // canvas.clip_top = canvas.padding_y;
    canvas.clip_top = canvas.padding_y;
    // canvas.clip_bottom = canvas.padding_y;
    canvas.clip_bottom = canvas.padding_y;

    Ok(())
}

/// 🮾
// pub fn draw1FBBE(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FBBE(
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

    // cornerDiagonalLines(metrics, canvas, .{ .br = true });
    cornerDiagonalLines(
        metrics,
        canvas,
        Quads {
            br: true,
            ..Default::default()
        },
    );
    // canvas.invert();
    canvas.invert();
    // Set the clip so we don't include anything outside of the cell.
    // canvas.clip_left = canvas.padding_x;
    canvas.clip_left = canvas.padding_x;
    // canvas.clip_right = canvas.padding_x;
    canvas.clip_right = canvas.padding_x;
    // canvas.clip_top = canvas.padding_y;
    canvas.clip_top = canvas.padding_y;
    // canvas.clip_bottom = canvas.padding_y;
    canvas.clip_bottom = canvas.padding_y;

    Ok(())
}

/// 🮿
// pub fn draw1FBBF(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FBBF(
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

    // cornerDiagonalLines(metrics, canvas, .{
    //     .tl = true,
    //     .tr = true,
    //     .bl = true,
    //     .br = true,
    // });
    cornerDiagonalLines(
        metrics,
        canvas,
        Quads {
            tl: true,
            tr: true,
            bl: true,
            br: true,
        },
    );
    // canvas.invert();
    canvas.invert();
    // Set the clip so we don't include anything outside of the cell.
    // canvas.clip_left = canvas.padding_x;
    canvas.clip_left = canvas.padding_x;
    // canvas.clip_right = canvas.padding_x;
    canvas.clip_right = canvas.padding_x;
    // canvas.clip_top = canvas.padding_y;
    canvas.clip_top = canvas.padding_y;
    // canvas.clip_bottom = canvas.padding_y;
    canvas.clip_bottom = canvas.padding_y;

    Ok(())
}

/// 🯎
// pub fn draw1FBCE(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FBCE(
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

    // block.block(metrics, canvas, .left, two_thirds, 1);
    super::block::block(metrics, canvas, Alignment::LEFT, two_thirds, 1.0);

    Ok(())
}

// 🯏
// pub fn draw1FBCF(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FBCF(
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

    // block.block(metrics, canvas, .left, one_third, 1);
    super::block::block(metrics, canvas, Alignment::LEFT, one_third, 1.0);

    Ok(())
}

/// Cell diagonals.
// pub fn draw1FBD0_1FBDF(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FBD0_1FBDF(
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
        // '🯐'
        // 0x1fbd0 => cellDiagonal(
        //     metrics,
        //     canvas,
        //     .middle_right,
        //     .lower_left,
        // ),
        0x1fbd0 => cellDiagonal(
            metrics,
            canvas,
            Alignment::MIDDLE_RIGHT,
            Alignment::LOWER_LEFT,
        ),
        // '🯑'
        // 0x1fbd1 => cellDiagonal(
        //     metrics,
        //     canvas,
        //     .upper_right,
        //     .middle_left,
        // ),
        0x1fbd1 => cellDiagonal(
            metrics,
            canvas,
            Alignment::UPPER_RIGHT,
            Alignment::MIDDLE_LEFT,
        ),
        // '🯒'
        // 0x1fbd2 => cellDiagonal(
        //     metrics,
        //     canvas,
        //     .upper_left,
        //     .middle_right,
        // ),
        0x1fbd2 => cellDiagonal(
            metrics,
            canvas,
            Alignment::UPPER_LEFT,
            Alignment::MIDDLE_RIGHT,
        ),
        // '🯓'
        // 0x1fbd3 => cellDiagonal(
        //     metrics,
        //     canvas,
        //     .middle_left,
        //     .lower_right,
        // ),
        0x1fbd3 => cellDiagonal(
            metrics,
            canvas,
            Alignment::MIDDLE_LEFT,
            Alignment::LOWER_RIGHT,
        ),
        // '🯔'
        // 0x1fbd4 => cellDiagonal(
        //     metrics,
        //     canvas,
        //     .upper_left,
        //     .lower_center,
        // ),
        0x1fbd4 => cellDiagonal(
            metrics,
            canvas,
            Alignment::UPPER_LEFT,
            Alignment::LOWER_CENTER,
        ),
        // '🯕'
        // 0x1fbd5 => cellDiagonal(
        //     metrics,
        //     canvas,
        //     .upper_center,
        //     .lower_right,
        // ),
        0x1fbd5 => cellDiagonal(
            metrics,
            canvas,
            Alignment::UPPER_CENTER,
            Alignment::LOWER_RIGHT,
        ),
        // '🯖'
        // 0x1fbd6 => cellDiagonal(
        //     metrics,
        //     canvas,
        //     .upper_right,
        //     .lower_center,
        // ),
        0x1fbd6 => cellDiagonal(
            metrics,
            canvas,
            Alignment::UPPER_RIGHT,
            Alignment::LOWER_CENTER,
        ),
        // '🯗'
        // 0x1fbd7 => cellDiagonal(
        //     metrics,
        //     canvas,
        //     .upper_center,
        //     .lower_left,
        // ),
        0x1fbd7 => cellDiagonal(
            metrics,
            canvas,
            Alignment::UPPER_CENTER,
            Alignment::LOWER_LEFT,
        ),
        // '🯘'
        // 0x1fbd8 => {
        0x1fbd8 => {
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .upper_left,
            //     .middle_center,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::UPPER_LEFT,
                Alignment::MIDDLE_CENTER,
            );
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .middle_center,
            //     .upper_right,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::MIDDLE_CENTER,
                Alignment::UPPER_RIGHT,
            );
        }
        // '🯙'
        // 0x1fbd9 => {
        0x1fbd9 => {
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .upper_right,
            //     .middle_center,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::UPPER_RIGHT,
                Alignment::MIDDLE_CENTER,
            );
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .middle_center,
            //     .lower_right,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::MIDDLE_CENTER,
                Alignment::LOWER_RIGHT,
            );
        }
        // '🯚'
        // 0x1fbda => {
        0x1fbda => {
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .lower_left,
            //     .middle_center,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::LOWER_LEFT,
                Alignment::MIDDLE_CENTER,
            );
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .middle_center,
            //     .lower_right,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::MIDDLE_CENTER,
                Alignment::LOWER_RIGHT,
            );
        }
        // '🯛'
        // 0x1fbdb => {
        0x1fbdb => {
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .upper_left,
            //     .middle_center,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::UPPER_LEFT,
                Alignment::MIDDLE_CENTER,
            );
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .middle_center,
            //     .lower_left,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::MIDDLE_CENTER,
                Alignment::LOWER_LEFT,
            );
        }
        // '🯜'
        // 0x1fbdc => {
        0x1fbdc => {
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .upper_left,
            //     .lower_center,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::UPPER_LEFT,
                Alignment::LOWER_CENTER,
            );
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .lower_center,
            //     .upper_right,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::LOWER_CENTER,
                Alignment::UPPER_RIGHT,
            );
        }
        // '🯝'
        // 0x1fbdd => {
        0x1fbdd => {
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .upper_right,
            //     .middle_left,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::UPPER_RIGHT,
                Alignment::MIDDLE_LEFT,
            );
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .middle_left,
            //     .lower_right,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::MIDDLE_LEFT,
                Alignment::LOWER_RIGHT,
            );
        }
        // '🯞'
        // 0x1fbde => {
        0x1fbde => {
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .lower_left,
            //     .upper_center,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::LOWER_LEFT,
                Alignment::UPPER_CENTER,
            );
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .upper_center,
            //     .lower_right,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::UPPER_CENTER,
                Alignment::LOWER_RIGHT,
            );
        }
        // '🯟'
        // 0x1fbdf => {
        0x1fbdf => {
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .upper_left,
            //     .middle_right,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::UPPER_LEFT,
                Alignment::MIDDLE_RIGHT,
            );
            // cellDiagonal(
            //     metrics,
            //     canvas,
            //     .middle_right,
            //     .lower_left,
            // );
            cellDiagonal(
                metrics,
                canvas,
                Alignment::MIDDLE_RIGHT,
                Alignment::LOWER_LEFT,
            );
        }

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

// pub fn draw1FBE0_1FBEF(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn draw1FBE0_1FBEF(
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
        // '🯠'
        // 0x1fbe0 => circle(metrics, canvas, .top, false),
        0x1fbe0 => circle(metrics, canvas, Alignment::TOP, false),
        // '🯡'
        // 0x1fbe1 => circle(metrics, canvas, .right, false),
        0x1fbe1 => circle(metrics, canvas, Alignment::RIGHT, false),
        // '🯢'
        // 0x1fbe2 => circle(metrics, canvas, .bottom, false),
        0x1fbe2 => circle(metrics, canvas, Alignment::BOTTOM, false),
        // '🯣'
        // 0x1fbe3 => circle(metrics, canvas, .left, false),
        0x1fbe3 => circle(metrics, canvas, Alignment::LEFT, false),
        // '🯤'
        // 0x1fbe4 => block.block(metrics, canvas, .upper_center, 0.5, 0.5),
        0x1fbe4 => super::block::block(metrics, canvas, Alignment::UPPER_CENTER, 0.5, 0.5),
        // '🯥'
        // 0x1fbe5 => block.block(metrics, canvas, .lower_center, 0.5, 0.5),
        0x1fbe5 => super::block::block(metrics, canvas, Alignment::LOWER_CENTER, 0.5, 0.5),
        // '🯦'
        // 0x1fbe6 => block.block(metrics, canvas, .middle_left, 0.5, 0.5),
        0x1fbe6 => super::block::block(metrics, canvas, Alignment::MIDDLE_LEFT, 0.5, 0.5),
        // '🯧'
        // 0x1fbe7 => block.block(metrics, canvas, .middle_right, 0.5, 0.5),
        0x1fbe7 => super::block::block(metrics, canvas, Alignment::MIDDLE_RIGHT, 0.5, 0.5),
        // '🯨'
        // 0x1fbe8 => circle(metrics, canvas, .top, true),
        0x1fbe8 => circle(metrics, canvas, Alignment::TOP, true),
        // '🯩'
        // 0x1fbe9 => circle(metrics, canvas, .right, true),
        0x1fbe9 => circle(metrics, canvas, Alignment::RIGHT, true),
        // '🯪'
        // 0x1fbea => circle(metrics, canvas, .bottom, true),
        0x1fbea => circle(metrics, canvas, Alignment::BOTTOM, true),
        // '🯫'
        // 0x1fbeb => circle(metrics, canvas, .left, true),
        0x1fbeb => circle(metrics, canvas, Alignment::LEFT, true),
        // '🯬'
        // 0x1fbec => circle(metrics, canvas, .top_right, true),
        0x1fbec => circle(metrics, canvas, Alignment::TOP_RIGHT, true),
        // '🯭'
        // 0x1fbed => circle(metrics, canvas, .bottom_left, true),
        0x1fbed => circle(metrics, canvas, Alignment::BOTTOM_LEFT, true),
        // '🯮'
        // 0x1fbee => circle(metrics, canvas, .bottom_right, true),
        0x1fbee => circle(metrics, canvas, Alignment::BOTTOM_RIGHT, true),
        // '🯯'
        // 0x1fbef => circle(metrics, canvas, .top_left, true),
        0x1fbef => circle(metrics, canvas, Alignment::TOP_LEFT, true),

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

// fn edgeTriangle(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime edge: Edge,
// ) !void {
fn edgeTriangle(metrics: Metrics, canvas: &mut Canvas, edge: Edge) -> Result<(), FillError> {
    // const upper: f64 = 0.0;
    let upper: f64 = 0.0;
    // const middle: f64 = @round(@as(f64, @floatFromInt(metrics.cell_height)) / 2);
    let middle: f64 = (metrics.cell_height as f64 / 2.0).round();
    // const lower: f64 = @floatFromInt(metrics.cell_height);
    let lower: f64 = metrics.cell_height as f64;
    // const left: f64 = 0.0;
    let left: f64 = 0.0;
    // const center: f64 = @round(@as(f64, @floatFromInt(metrics.cell_width)) / 2);
    let center: f64 = (metrics.cell_width as f64 / 2.0).round();
    // const right: f64 = @floatFromInt(metrics.cell_width);
    let right: f64 = metrics.cell_width as f64;

    // const x0, const y0, const x1, const y1 = switch (edge) {
    let (x0, y0, x1, y1): (f64, f64, f64, f64) = match edge {
        // .top => .{ right, upper, left, upper },
        Edge::Top => (right, upper, left, upper),
        // .left => .{ left, upper, left, lower },
        Edge::Left => (left, upper, left, lower),
        // .bottom => .{ left, lower, right, lower },
        Edge::Bottom => (left, lower, right, lower),
        // .right => .{ right, lower, right, upper },
        Edge::Right => (right, lower, right, upper),
    };

    // var path = canvas.staticPath(5); // nodes.len = 0
    let mut path = canvas.staticPath::<5>(); // nodes.len = 0
                                             // path.moveTo(center, middle); // +1, nodes.len = 1
    path.move_to(center, middle); // +1, nodes.len = 1
                                  // path.lineTo(x0, y0); // +1, nodes.len = 2
    path.line_to(x0, y0); // +1, nodes.len = 2
                          // path.lineTo(x1, y1); // +1, nodes.len = 3
    path.line_to(x1, y1); // +1, nodes.len = 3
                          // path.close(); // +2, nodes.len = 5
    path.close(); // +2, nodes.len = 5

    // try canvas.fillPath(path.wrapped_path, .{}, .on);
    canvas.fillPath(&path.wrapped_path, Default::default(), Color::ON)
}

// fn cornerDiagonalLines(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime corners: Quads,
// ) void {
fn cornerDiagonalLines(metrics: Metrics, canvas: &mut Canvas, corners: Quads) {
    // const thick_px = Thickness.light.height(metrics.box_thickness);
    let thick_px = Thickness::Light.height(metrics.box_thickness);

    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;
    // const float_thick: f64 = @floatFromInt(thick_px);
    let float_thick: f64 = thick_px as f64;
    // const center_x: f64 = @floatFromInt(metrics.cell_width / 2 + metrics.cell_width % 2);
    let center_x: f64 = (metrics.cell_width / 2 + metrics.cell_width % 2) as f64;
    // const center_y: f64 = @floatFromInt(metrics.cell_height / 2 + metrics.cell_height % 2);
    let center_y: f64 = (metrics.cell_height / 2 + metrics.cell_height % 2) as f64;

    // if (corners.tl) canvas.line(.{
    //     .p0 = .{ .x = center_x, .y = 0 },
    //     .p1 = .{ .x = 0, .y = center_y },
    // }, float_thick, .on) catch {};
    if corners.tl {
        let _ = canvas.line(
            Line {
                p0: Point {
                    x: center_x,
                    y: 0.0,
                },
                p1: Point {
                    x: 0.0,
                    y: center_y,
                },
            },
            float_thick,
            Color::ON,
        );
    }

    // if (corners.tr) canvas.line(.{
    //     .p0 = .{ .x = center_x, .y = 0 },
    //     .p1 = .{ .x = float_width, .y = center_y },
    // }, float_thick, .on) catch {};
    if corners.tr {
        let _ = canvas.line(
            Line {
                p0: Point {
                    x: center_x,
                    y: 0.0,
                },
                p1: Point {
                    x: float_width,
                    y: center_y,
                },
            },
            float_thick,
            Color::ON,
        );
    }

    // if (corners.bl) canvas.line(.{
    //     .p0 = .{ .x = center_x, .y = float_height },
    //     .p1 = .{ .x = 0, .y = center_y },
    // }, float_thick, .on) catch {};
    if corners.bl {
        let _ = canvas.line(
            Line {
                p0: Point {
                    x: center_x,
                    y: float_height,
                },
                p1: Point {
                    x: 0.0,
                    y: center_y,
                },
            },
            float_thick,
            Color::ON,
        );
    }

    // if (corners.br) canvas.line(.{
    //     .p0 = .{ .x = center_x, .y = float_height },
    //     .p1 = .{ .x = float_width, .y = center_y },
    // }, float_thick, .on) catch {};
    if corners.br {
        let _ = canvas.line(
            Line {
                p0: Point {
                    x: center_x,
                    y: float_height,
                },
                p1: Point {
                    x: float_width,
                    y: center_y,
                },
            },
            float_thick,
            Color::ON,
        );
    }
}

// fn cellDiagonal(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime from: Alignment,
//     comptime to: Alignment,
// ) void {
fn cellDiagonal(metrics: Metrics, canvas: &mut Canvas, from: Alignment, to: Alignment) {
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;

    // const x0: f64 = switch (from.horizontal) {
    //     .left => 0,
    //     .right => float_width,
    //     .center => float_width / 2,
    // };
    let x0: f64 = match from.horizontal {
        Horizontal::Left => 0.0,
        Horizontal::Right => float_width,
        Horizontal::Center => float_width / 2.0,
    };
    // const y0: f64 = switch (from.vertical) {
    //     .top => 0,
    //     .bottom => float_height,
    //     .middle => float_height / 2,
    // };
    let y0: f64 = match from.vertical {
        Vertical::Top => 0.0,
        Vertical::Bottom => float_height,
        Vertical::Middle => float_height / 2.0,
    };
    // const x1: f64 = switch (to.horizontal) {
    //     .left => 0,
    //     .right => float_width,
    //     .center => float_width / 2,
    // };
    let x1: f64 = match to.horizontal {
        Horizontal::Left => 0.0,
        Horizontal::Right => float_width,
        Horizontal::Center => float_width / 2.0,
    };
    // const y1: f64 = switch (to.vertical) {
    //     .top => 0,
    //     .bottom => float_height,
    //     .middle => float_height / 2,
    // };
    let y1: f64 = match to.vertical {
        Vertical::Top => 0.0,
        Vertical::Bottom => float_height,
        Vertical::Middle => float_height / 2.0,
    };

    // canvas.line(
    //     .{
    //         .p0 = .{ .x = x0, .y = y0 },
    //         .p1 = .{ .x = x1, .y = y1 },
    //     },
    //     @floatFromInt(Thickness.light.height(metrics.box_thickness)),
    //     .on,
    // ) catch {};
    let _ = canvas.line(
        Line {
            p0: Point { x: x0, y: y0 },
            p1: Point { x: x1, y: y1 },
        },
        Thickness::Light.height(metrics.box_thickness) as f64,
        Color::ON,
    );
}

// fn checkerboardFill(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     parity: u1,
// ) void {
//
// PORT NOTE: Rust has no `u1`; `parity` is carried as a `u8` (only 0/1 are ever
// passed) and compared against `(x + y) % 2` after a widening cast.
fn checkerboardFill(metrics: Metrics, canvas: &mut Canvas, parity: u8) {
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;
    // const x_size: usize = 4;
    let x_size: usize = 4;
    // const y_size: usize = @intFromFloat(@round(4 * (float_height / float_width)));
    let y_size: usize = (4.0 * (float_height / float_width)).round() as usize;
    // for (0..x_size) |x| {
    for x in 0..x_size {
        // const x0 = (metrics.cell_width * x) / x_size;
        let x0 = (metrics.cell_width as usize * x) / x_size;
        // const x1 = (metrics.cell_width * (x + 1)) / x_size;
        let x1 = (metrics.cell_width as usize * (x + 1)) / x_size;
        // for (0..y_size) |y| {
        for y in 0..y_size {
            // const y0 = (metrics.cell_height * y) / y_size;
            let y0 = (metrics.cell_height as usize * y) / y_size;
            // const y1 = (metrics.cell_height * (y + 1)) / y_size;
            let y1 = (metrics.cell_height as usize * (y + 1)) / y_size;
            // if ((x + y) % 2 == parity) {
            if (x + y) % 2 == parity as usize {
                // canvas.rect(.{
                //     .x = @intCast(x0),
                //     .y = @intCast(y0),
                //     .width = @intCast(x1 -| x0),
                //     .height = @intCast(y1 -| y0),
                // }, .on);
                canvas.rect(
                    Rect {
                        x: x0 as i32,
                        y: y0 as i32,
                        width: x1.saturating_sub(x0) as i32,
                        height: y1.saturating_sub(y0) as i32,
                    },
                    Color::ON,
                );
            }
        }
    }
}

// pub fn circle(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime position: Alignment,
//     comptime filled: bool,
// ) void {
//
// PORT NOTE: the reference swallows every `ctx.*` error with `catch return`; the
// port mirrors that with `if <op>.is_err() { return; }` on each step. The final
// step's early `return` is redundant (it is the last statement), but is kept to
// preserve the uniform `catch return` shape, so `needless_return` is allowed.
#[allow(clippy::needless_return)]
pub fn circle(metrics: Metrics, canvas: &mut Canvas, position: Alignment, filled: bool) {
    // Set the clip so we don't include anything outside of the cell.
    // canvas.clip_left = canvas.padding_x;
    canvas.clip_left = canvas.padding_x;
    // canvas.clip_right = canvas.padding_x;
    canvas.clip_right = canvas.padding_x;
    // canvas.clip_top = canvas.padding_y;
    canvas.clip_top = canvas.padding_y;
    // canvas.clip_bottom = canvas.padding_y;
    canvas.clip_bottom = canvas.padding_y;

    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;

    // const x: f64 = switch (position.horizontal) {
    //     .left => 0,
    //     .right => float_width,
    //     .center => float_width / 2,
    // };
    let x: f64 = match position.horizontal {
        Horizontal::Left => 0.0,
        Horizontal::Right => float_width,
        Horizontal::Center => float_width / 2.0,
    };
    // const y: f64 = switch (position.vertical) {
    //     .top => 0,
    //     .bottom => float_height,
    //     .middle => float_height / 2,
    // };
    let y: f64 = match position.vertical {
        Vertical::Top => 0.0,
        Vertical::Bottom => float_height,
        Vertical::Middle => float_height / 2.0,
    };
    // const r: f64 = 0.5 * @min(float_width, float_height);
    let r: f64 = 0.5 * float_width.min(float_height);

    // var ctx = canvas.getContext();
    // defer ctx.deinit();
    //
    // PORT NOTE: `ctx` releases its managed path on drop, so `defer ctx.deinit()`
    // maps to letting `ctx` fall out of scope.
    let mut ctx = canvas.getContext();
    // ctx.setSource(.{ .opaque_pattern = .{
    //     .pixel = .{ .alpha8 = .{ .a = @intFromEnum(Shade.on) } },
    // } });
    ctx.set_source(Pattern::OpaquePattern(OpaquePattern {
        pixel: Pixel::Alpha8(Alpha8 { a: Shade::ON.0 }),
    }));
    // ctx.setLineWidth(
    //     @floatFromInt(Thickness.light.height(metrics.box_thickness)),
    // );
    ctx.set_line_width(Thickness::Light.height(metrics.box_thickness) as f64);

    // if (filled) {
    if filled {
        // ctx.arc(x, y, r, 0, std.math.pi * 2) catch return;
        if ctx.arc(x, y, r, 0.0, PI * 2.0).is_err() {
            return;
        }
        // ctx.closePath() catch return;
        //
        // PORT NOTE: rustty-z2d's `close_path` is infallible, so the reference
        // `try`/`catch return` collapses to a plain call.
        ctx.close_path();
        // ctx.fill() catch return;
        if ctx.fill().is_err() {
            return;
        }
    } else {
        // ctx.arc(x, y, r - ctx.line_width / 2, 0, std.math.pi * 2) catch return;
        if ctx
            .arc(x, y, r - ctx.line_width / 2.0, 0.0, PI * 2.0)
            .is_err()
        {
            return;
        }
        // ctx.closePath() catch return;
        ctx.close_path();
        // ctx.stroke() catch return;
        if ctx.stroke().is_err() {
            return;
        }
    }
}
