//! This implements the built-in "sprite face". This font renders
//! the built-in glyphs for the terminal, such as box drawing fonts, as well
//! as specific sprites that are part of our rendering model such as
//! text decorations (underlines).
//!
//! This isn't really a "font face" so much as it is quacks like a font
//! face with regards to how it works with font.Group. We don't use any
//! dynamic dispatch so it isn't truly an interface but the functions
//! and behaviors are close enough to a system face that it makes it easy
//! to integrate with font.Group. This is desirable so that higher level
//! processes such as GroupCache, Shaper, etc. don't need to be aware of
//! special sprite handling and just treat it like a normal font face.

// const Face = @This();
//
// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const wuffs = @import("wuffs");
// const z2d = @import("z2d");
// const font = @import("../main.zig");
// const Sprite = font.sprite.Sprite;
//
// const special = @import("draw/special.zig");
//
// const log = std.log.scoped(.font_sprite);
//
// PORT NOTE: `font = ../main.zig` becomes the local crate — `font.Metrics` is
// `crate::metrics::Metrics`, `font.sprite.{Canvas,Sprite}` are
// `crate::sprite::{Canvas,Sprite}`, `font.Atlas`/`font.Glyph` are
// `crate::atlas`/`crate::glyph`, and the `draw/*` structs are the sibling
// `crate::sprite::draw::*` modules. `wuffs` (runtime PNG decode) is used ONLY by
// the known-answer test; per the established `glyf_rasterize_png_test` precedent
// it is replaced there by the pure-Rust `png` dev-dependency (z2d's PNG exporter
// is not part of the ported subset). `log.scoped(.font_sprite)` maps to the
// crate's `eprintln!("error(font_sprite): ...")` diagnostic convention.
use crate::atlas::{self, Atlas};
use crate::glyph::Glyph;
use crate::metrics::Metrics;
use crate::sprite::draw::{
    block, braille, branch, geometric_shapes, powerline, r#box, special,
    symbols_for_legacy_computing as slc, symbols_for_legacy_computing_supplement as slcs,
};
use crate::sprite::{Canvas, Sprite};
use allocator_api2::alloc::Allocator;
use rustty_z2d as z2d;
use rustty_z2d::painter::{FillError, StrokeError};
use rustty_z2d::path;

/// The sprite face itself. (Zig file-struct `Face = @This()`.)
// PORT NOTE: the reference passes `Face` by value to `hasCodepoint`/`renderGlyph`
// (Zig copies the one-field struct); `Copy` here mirrors that value semantics.
#[derive(Clone, Copy)]
pub struct Face {
    /// Grid metrics for rendering sprites.
    // metrics: font.Metrics,
    pub metrics: Metrics,
}

// pub const DrawFnError =
//     Allocator.Error ||
//     z2d.Path.Error ||
//     z2d.painter.FillError ||
//     z2d.painter.StrokeError ||
//     error{
//         /// Something went wrong while doing math.
//         MathError,
//     };
//
// PORT NOTE: the reference's error set is the union of `Allocator.Error`, the
// z2d `Path`/`Fill`/`Stroke` error sets, and a bespoke `MathError`. It is
// reproduced here as a concrete enum so every `draw*` function (each of which
// returns its own concrete subset — `Infallible`, `FillError`, `StrokeError`,
// `InnerStrokeError`, `DottedError`, `CurlyError`) can be adapted to a single
// uniform `DrawFn` pointer via `From`. `OutOfMemory` (`Allocator.Error`) and
// `MathError` are part of the faithful set but are never constructed in the
// allocator-free z2d subset ported here (no `draw*` produces them), hence the
// `#[allow(dead_code)]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawFnError {
    #[allow(dead_code)]
    OutOfMemory,
    Path(path::Error),
    Fill(FillError),
    Stroke(StrokeError),
    /// PORT NOTE (P3-W8 T3, oracle `d0e72a3ab`): `innerStrokePath` now returns
    /// `z2d.Path.OffsetError`, whose Zig spelling
    /// (`InputSet.FromNodesError || OutputSet.ToNodesError ||
    /// Allocator.Error`) carries z2d's `InternalError` in addition to the sets
    /// `DrawFnError` already names. Zig grows the reference's inferred error
    /// set implicitly at the `try` site; rustty's concrete enum has to name it.
    Internal(z2d::internal::error::InternalError),
    /// Something went wrong while doing math.
    #[allow(dead_code)]
    MathError,
}

impl From<std::convert::Infallible> for DrawFnError {
    fn from(e: std::convert::Infallible) -> DrawFnError {
        match e {}
    }
}

impl From<path::Error> for DrawFnError {
    fn from(e: path::Error) -> DrawFnError {
        DrawFnError::Path(e)
    }
}

impl From<FillError> for DrawFnError {
    fn from(e: FillError) -> DrawFnError {
        DrawFnError::Fill(e)
    }
}

impl From<StrokeError> for DrawFnError {
    fn from(e: StrokeError) -> DrawFnError {
        DrawFnError::Stroke(e)
    }
}

impl From<crate::sprite::canvas::InnerStrokeError> for DrawFnError {
    fn from(e: crate::sprite::canvas::InnerStrokeError) -> DrawFnError {
        match e {
            crate::sprite::canvas::InnerStrokeError::Stroke(s) => DrawFnError::Stroke(s),
            crate::sprite::canvas::InnerStrokeError::Offset(i) => DrawFnError::Internal(i),
        }
    }
}

impl From<special::DottedError> for DrawFnError {
    fn from(e: special::DottedError) -> DrawFnError {
        match e {
            special::DottedError::Path(p) => DrawFnError::Path(p),
            special::DottedError::Fill(f) => DrawFnError::Fill(f),
        }
    }
}

impl From<special::CurlyError> for DrawFnError {
    fn from(e: special::CurlyError) -> DrawFnError {
        match e {
            special::CurlyError::Path(p) => DrawFnError::Path(p),
            special::CurlyError::Stroke(s) => DrawFnError::Stroke(s),
        }
    }
}

// /// A function that draws a glyph on the provided canvas.
// pub const DrawFn = fn (
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) DrawFnError!void;
/// A function that draws a glyph on the provided canvas.
pub type DrawFn = fn(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), DrawFnError>;

// const Range = struct {
//     min: u32,
//     max: u32,
//     draw: DrawFn,
// };
struct Range {
    min: u32,
    max: u32,
    draw: DrawFn,
}

/// Automatically collect ranges for functions with names
/// in the format `draw<CP>` or `draw<MIN>_<MAX>`.
// const ranges: []const Range = ranges: {
//     @setEvalBranchQuota(1_000_000);
//
//     // Structs containing drawing functions for codepoint ranges.
//     const structs = [_]type{
//         @import("draw/block.zig"),
//         @import("draw/box.zig"),
//         @import("draw/braille.zig"),
//         @import("draw/branch.zig"),
//         @import("draw/geometric_shapes.zig"),
//         @import("draw/powerline.zig"),
//         @import("draw/symbols_for_legacy_computing.zig"),
//         @import("draw/symbols_for_legacy_computing_supplement.zig"),
//     };
//
//     // Count how many draw fns we have
//     var range_count = 0;
//     for (structs) |s| {
//         for (@typeInfo(s).@"struct".decls) |decl| {
//             if (!@hasDecl(s, decl.name)) continue;
//             if (!std.mem.startsWith(u8, decl.name, "draw")) continue;
//             range_count += 1;
//         }
//     }
//
//     // Make an array and collect ranges for each function.
//     var r: [range_count]Range = undefined;
//     var names: [range_count][:0]const u8 = undefined;
//     var i = 0;
//     for (structs) |s| {
//         for (@typeInfo(s).@"struct".decls) |decl| {
//             if (!@hasDecl(s, decl.name)) continue;
//             if (!std.mem.startsWith(u8, decl.name, "draw")) continue;
//
//             const sep = std.mem.indexOfScalar(u8, decl.name, '_') orelse decl.name.len;
//
//             const min = std.fmt.parseInt(u21, decl.name[4..sep], 16) catch unreachable;
//
//             const max = if (sep == decl.name.len)
//                 min
//             else
//                 std.fmt.parseInt(u21, decl.name[sep + 1 ..], 16) catch unreachable;
//
//             r[i] = .{
//                 .min = min,
//                 .max = max,
//                 .draw = @field(s, decl.name),
//             };
//             names[i] = decl.name;
//             i += 1;
//         }
//     }
//
//     // Sort ranges in ascending order
//     std.mem.sortUnstableContext(0, r.len, struct {
//         r: []Range,
//         names: [][:0]const u8,
//         pub fn lessThan(self: @This(), a: usize, b: usize) bool {
//             return self.r[a].min < self.r[b].min;
//         }
//         pub fn swap(self: @This(), a: usize, b: usize) void {
//             std.mem.swap(Range, &self.r[a], &self.r[b]);
//             std.mem.swap([:0]const u8, &self.names[a], &self.names[b]);
//         }
//     }{
//         .r = &r,
//         .names = &names,
//     });
//
//     // Ensure there's no overlapping ranges
//     i = 0;
//     for (r, 0..) |n, k| {
//         if (n.min <= i) {
//             @compileError(
//                 std.fmt.comptimePrint(
//                     "Codepoint range for {s}(...) overlaps range for {s}(...), {X} <= {X} <= {X}",
//                     .{ names[k], names[k - 1], r[k - 1].min, n.min, r[k - 1].max },
//                 ),
//             );
//         }
//         i = n.max;
//     }
//
//     // We need to copy in to a const rather than a var in order to take
//     // the reference at comptime so that we can break with a slice here.
//     const fixed = r;
//
//     break :ranges &fixed;
// };
//
// PORT NOTE: the reference builds this table at comptime by reflecting over the
// `draw*` decls of the eight draw structs, parsing the hex codepoint range from
// each name, sorting by `min`, and `@compileError`-ing on any overlap. Rust has
// no comptime decl reflection, so the table is hand-maintained here in ascending
// `min` order — the unique sort of the same set — with each entry adapting the
// module's concrete-error `draw*` function to the uniform `DrawFn` via
// `map_err(DrawFnError::from)`. The reference's overlap assertion is ported as a
// runtime test (`ranges_have_no_overlap`) below.
const RANGES: &[Range] = &[
    Range {
        min: 0x2500,
        max: 0x257F,
        draw: |cp, c, w, h, m| r#box::draw2500_257F(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x2580,
        max: 0x259F,
        draw: |cp, c, w, h, m| block::draw2580_259F(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x25E2,
        max: 0x25E5,
        draw: |cp, c, w, h, m| {
            geometric_shapes::draw25E2_25E5(cp, c, w, h, m).map_err(DrawFnError::from)
        },
    },
    Range {
        min: 0x25F8,
        max: 0x25FA,
        draw: |cp, c, w, h, m| {
            geometric_shapes::draw25F8_25FA(cp, c, w, h, m).map_err(DrawFnError::from)
        },
    },
    Range {
        min: 0x25FF,
        max: 0x25FF,
        draw: |cp, c, w, h, m| {
            geometric_shapes::draw25FF(cp, c, w, h, m).map_err(DrawFnError::from)
        },
    },
    Range {
        min: 0x2800,
        max: 0x28FF,
        draw: |cp, c, w, h, m| braille::draw2800_28FF(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B0,
        max: 0xE0B0,
        draw: |cp, c, w, h, m| powerline::drawE0B0(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B1,
        max: 0xE0B1,
        draw: |cp, c, w, h, m| powerline::drawE0B1(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B2,
        max: 0xE0B2,
        draw: |cp, c, w, h, m| powerline::drawE0B2(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B3,
        max: 0xE0B3,
        draw: |cp, c, w, h, m| powerline::drawE0B3(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B4,
        max: 0xE0B4,
        draw: |cp, c, w, h, m| powerline::drawE0B4(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B5,
        max: 0xE0B5,
        draw: |cp, c, w, h, m| powerline::drawE0B5(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B6,
        max: 0xE0B6,
        draw: |cp, c, w, h, m| powerline::drawE0B6(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B7,
        max: 0xE0B7,
        draw: |cp, c, w, h, m| powerline::drawE0B7(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B8,
        max: 0xE0B8,
        draw: |cp, c, w, h, m| powerline::drawE0B8(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0B9,
        max: 0xE0B9,
        draw: |cp, c, w, h, m| powerline::drawE0B9(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0BA,
        max: 0xE0BA,
        draw: |cp, c, w, h, m| powerline::drawE0BA(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0BB,
        max: 0xE0BB,
        draw: |cp, c, w, h, m| powerline::drawE0BB(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0BC,
        max: 0xE0BC,
        draw: |cp, c, w, h, m| powerline::drawE0BC(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0BD,
        max: 0xE0BD,
        draw: |cp, c, w, h, m| powerline::drawE0BD(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0BE,
        max: 0xE0BE,
        draw: |cp, c, w, h, m| powerline::drawE0BE(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0BF,
        max: 0xE0BF,
        draw: |cp, c, w, h, m| powerline::drawE0BF(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0D2,
        max: 0xE0D2,
        draw: |cp, c, w, h, m| powerline::drawE0D2(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xE0D4,
        max: 0xE0D4,
        draw: |cp, c, w, h, m| powerline::drawE0D4(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0xF5D0,
        max: 0xF60D,
        draw: |cp, c, w, h, m| branch::drawF5D0_F60D(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CC1B,
        max: 0x1CC1E,
        draw: |cp, c, w, h, m| slcs::draw1CC1B_1CC1E(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CC21,
        max: 0x1CC2F,
        draw: |cp, c, w, h, m| slcs::draw1CC21_1CC2F(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CC30,
        max: 0x1CC3F,
        draw: |cp, c, w, h, m| slcs::draw1CC30_1CC3F(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CD00,
        max: 0x1CDE5,
        draw: |cp, c, w, h, m| slcs::draw1CD00_1CDE5(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CE00,
        max: 0x1CE00,
        draw: |cp, c, w, h, m| slcs::draw1CE00(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CE01,
        max: 0x1CE01,
        draw: |cp, c, w, h, m| slcs::draw1CE01(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CE0B,
        max: 0x1CE0B,
        draw: |cp, c, w, h, m| slcs::draw1CE0B(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CE0C,
        max: 0x1CE0C,
        draw: |cp, c, w, h, m| slcs::draw1CE0C(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CE16,
        max: 0x1CE19,
        draw: |cp, c, w, h, m| slcs::draw1CE16_1CE19(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CE51,
        max: 0x1CE8F,
        draw: |cp, c, w, h, m| slcs::draw1CE51_1CE8F(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1CE90,
        max: 0x1CEAF,
        draw: |cp, c, w, h, m| slcs::draw1CE90_1CEAF(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FB00,
        max: 0x1FB3B,
        draw: |cp, c, w, h, m| slc::draw1FB00_1FB3B(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FB3C,
        max: 0x1FB67,
        draw: |cp, c, w, h, m| slc::draw1FB3C_1FB67(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FB68,
        max: 0x1FB6F,
        draw: |cp, c, w, h, m| slc::draw1FB68_1FB6F(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FB70,
        max: 0x1FB75,
        draw: |cp, c, w, h, m| slc::draw1FB70_1FB75(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FB76,
        max: 0x1FB7B,
        draw: |cp, c, w, h, m| slc::draw1FB76_1FB7B(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FB7C,
        max: 0x1FB97,
        draw: |cp, c, w, h, m| slc::draw1FB7C_1FB97(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FB98,
        max: 0x1FB98,
        draw: |cp, c, w, h, m| slc::draw1FB98(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FB99,
        max: 0x1FB99,
        draw: |cp, c, w, h, m| slc::draw1FB99(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FB9A,
        max: 0x1FB9F,
        draw: |cp, c, w, h, m| slc::draw1FB9A_1FB9F(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FBA0,
        max: 0x1FBAE,
        draw: |cp, c, w, h, m| slc::draw1FBA0_1FBAE(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FBAF,
        max: 0x1FBAF,
        draw: |cp, c, w, h, m| slc::draw1FBAF(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FBBD,
        max: 0x1FBBD,
        draw: |cp, c, w, h, m| slc::draw1FBBD(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FBBE,
        max: 0x1FBBE,
        draw: |cp, c, w, h, m| slc::draw1FBBE(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FBBF,
        max: 0x1FBBF,
        draw: |cp, c, w, h, m| slc::draw1FBBF(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FBCE,
        max: 0x1FBCE,
        draw: |cp, c, w, h, m| slc::draw1FBCE(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FBCF,
        max: 0x1FBCF,
        draw: |cp, c, w, h, m| slc::draw1FBCF(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FBD0,
        max: 0x1FBDF,
        draw: |cp, c, w, h, m| slc::draw1FBD0_1FBDF(cp, c, w, h, m).map_err(DrawFnError::from),
    },
    Range {
        min: 0x1FBE0,
        max: 0x1FBEF,
        draw: |cp, c, w, h, m| slc::draw1FBE0_1FBEF(cp, c, w, h, m).map_err(DrawFnError::from),
    },
];

// fn getDrawFn(cp: u32) ?*const DrawFn {
fn get_draw_fn(cp: u32) -> Option<DrawFn> {
    // For special sprites (cursors, underlines, etc.) all sprites are drawn
    // by functions from `Special` that share the name of the enum field.
    // if (cp >= Sprite.start) switch (@as(Sprite, @enumFromInt(cp))) {
    //     inline else => |sprite| {
    //         return @field(special, @tagName(sprite));
    //     },
    // };
    //
    // PORT NOTE: the reference resolves the special draw fn by matching the enum
    // field name to the same-named `special.zig` decl (`@tagName` + `@field`).
    // Rust has no comptime field-name reflection, so each `Sprite` variant is
    // mapped to its same-named `special` function explicitly. As in the
    // reference, this branch is only reached for `cp >= Sprite::start`.
    if cp >= Sprite::START {
        return Some(match cp {
            x if x == Sprite::Underline as u32 => {
                |cp, c, w, h, m| special::underline(cp, c, w, h, m).map_err(DrawFnError::from)
            }
            x if x == Sprite::UnderlineDouble as u32 => |cp, c, w, h, m| {
                special::underline_double(cp, c, w, h, m).map_err(DrawFnError::from)
            },
            x if x == Sprite::UnderlineDotted as u32 => |cp, c, w, h, m| {
                special::underline_dotted(cp, c, w, h, m).map_err(DrawFnError::from)
            },
            x if x == Sprite::UnderlineDashed as u32 => |cp, c, w, h, m| {
                special::underline_dashed(cp, c, w, h, m).map_err(DrawFnError::from)
            },
            x if x == Sprite::UnderlineCurly as u32 => {
                |cp, c, w, h, m| special::underline_curly(cp, c, w, h, m).map_err(DrawFnError::from)
            }
            x if x == Sprite::Strikethrough as u32 => {
                |cp, c, w, h, m| special::strikethrough(cp, c, w, h, m).map_err(DrawFnError::from)
            }
            x if x == Sprite::Overline as u32 => {
                |cp, c, w, h, m| special::overline(cp, c, w, h, m).map_err(DrawFnError::from)
            }
            x if x == Sprite::CursorRect as u32 => {
                |cp, c, w, h, m| special::cursor_rect(cp, c, w, h, m).map_err(DrawFnError::from)
            }
            x if x == Sprite::CursorHollowRect as u32 => |cp, c, w, h, m| {
                special::cursor_hollow_rect(cp, c, w, h, m).map_err(DrawFnError::from)
            },
            x if x == Sprite::CursorBar as u32 => {
                |cp, c, w, h, m| special::cursor_bar(cp, c, w, h, m).map_err(DrawFnError::from)
            }
            x if x == Sprite::CursorUnderline as u32 => |cp, c, w, h, m| {
                special::cursor_underline(cp, c, w, h, m).map_err(DrawFnError::from)
            },
            // The reference's `@enumFromInt` is invalid for any `cp >= start`
            // that is not a `Sprite` field; this is unreachable in practice as
            // callers only pass valid sprite codepoints.
            _ => unreachable!("invalid sprite codepoint cp={cp:x}"),
        });
    }

    // Pray that the compiler is smart enough to
    // turn this in to a jump table or something...
    // inline for (ranges) |range| {
    //     if (cp >= range.min and cp <= range.max) return range.draw;
    // }
    for range in RANGES {
        if cp >= range.min && cp <= range.max {
            return Some(range.draw);
        }
    }
    // return null;
    None
}

impl Face {
    /// Returns true if the codepoint exists in our sprite font.
    // pub fn hasCodepoint(self: Face, cp: u32, p: ?font.Presentation) bool {
    //
    // PORT NOTE: `font.Presentation` lives in the not-yet-carved `font/main.zig`;
    // it is reproduced locally as `Presentation` (see below) so this signature is
    // faithful. As in the reference, both `self` and `p` are ignored.
    pub fn has_codepoint(self, cp: u32, p: Option<Presentation>) -> bool {
        // We ignore presentation. No matter what presentation is
        // requested we always provide glyphs for our codepoints.
        // _ = p;
        let _ = p;
        // _ = self;
        let _ = self;
        // return getDrawFn(cp) != null;
        get_draw_fn(cp).is_some()
    }

    /// Render the glyph.
    // pub fn renderGlyph(
    //     self: Face,
    //     alloc: Allocator,
    //     atlas: *font.Atlas,
    //     cp: u32,
    //     opts: font.Glyph.RenderOptions,
    // ) !font.Glyph {
    pub fn render_glyph<A: Allocator + Copy>(
        self,
        alloc: A,
        atlas: &mut Atlas<A>,
        cp: u32,
        opts: crate::glyph::RenderOptions,
    ) -> Result<Glyph, RenderGlyphError> {
        // if (std.debug.runtime_safety) {
        //     if (!self.hasCodepoint(cp, null)) {
        //         log.err("invalid codepoint cp={x}", .{cp});
        //         unreachable; // crash
        //     }
        // }
        if cfg!(debug_assertions) && !self.has_codepoint(cp, None) {
            eprintln!("error(font_sprite): invalid codepoint cp={cp:x}");
            unreachable!(); // crash
        }

        // It should be impossible for this to be null and we assert that
        // in runtime safety modes but in case it is its not worth memory
        // corruption so we return a valid, blank glyph.
        // const draw = getDrawFn(cp) orelse return .{ ... };
        let draw = match get_draw_fn(cp) {
            Some(draw) => draw,
            None => {
                return Ok(Glyph {
                    width: 0,
                    height: 0,
                    offset_x: 0,
                    offset_y: 0,
                    atlas_x: 0,
                    atlas_y: 0,
                });
            }
        };

        // const metrics = self.metrics;
        let metrics = self.metrics;

        // We adjust our sprite width based on the cell width.
        // const width = switch (opts.cell_width orelse 1) {
        //     0, 1 => metrics.cell_width,
        //     else => |width| metrics.cell_width * width,
        // };
        let width = match opts.cell_width.unwrap_or(1) {
            0 | 1 => metrics.cell_width,
            width => metrics.cell_width * width as u32,
        };

        // Sprite glyphs generally get the full cell height, but cursor glyphs need
        // to be affected by `adjust-cursor-height`, so we use `cursor_height` for
        // the height if it's one of the full-height cursors.
        // const height = switch (cp) {
        //     @intFromEnum(Sprite.cursor_rect),
        //     @intFromEnum(Sprite.cursor_hollow_rect),
        //     @intFromEnum(Sprite.cursor_bar),
        //     => metrics.cursor_height,
        //     else => metrics.cell_height,
        // };
        // PORT NOTE (parity dac341cad): upstream regression fix from the sprite
        // face rework (#7732); cursor sprites must respect adjust-cursor-height.
        let height = match cp {
            x if x == Sprite::CursorRect as u32
                || x == Sprite::CursorHollowRect as u32
                || x == Sprite::CursorBar as u32 =>
            {
                metrics.cursor_height
            }
            _ => metrics.cell_height,
        };

        // const padding_x = width / 4;
        let padding_x = width / 4;
        // const padding_y = height / 4;
        let padding_y = height / 4;

        // Make a canvas of the desired size
        // var canvas = try font.sprite.Canvas.init(alloc, width, height, padding_x, padding_y);
        // defer canvas.deinit();
        let mut canvas = Canvas::init(width, height, padding_x, padding_y)?;

        // try draw(cp, &canvas, width, height, metrics);
        draw(cp, &mut canvas, width, height, metrics)?;

        // Write the drawing to the atlas
        // const region = try canvas.writeAtlas(alloc, atlas);
        let region = canvas.writeAtlas(alloc, atlas)?;

        // The X offset is the displacement from the left edge of the cell that our
        // drawn sprite will be drawn in the grid. That's the same as the distance
        // we have calculated from the left of the canvas, minus the padding, since
        // the padding represents extra pixels to the left of the cell.
        // const offset_x: i32 =
        //     @as(i32, @intCast(canvas.clip_left)) -
        //     @as(i32, @intCast(padding_x));
        let offset_x: i32 = canvas.clip_left as i32 - padding_x as i32;
        // Similar logic applies for the Y offset, but with the additional factor
        // that we want to re-center cursor glyphs in the cell if they were drawn
        // taller or shorter than a normal cell is; this only applies to cursors
        // currently, but could conceivably apply to other things in the future.
        // const offset_y: i32 =
        //     @as(i32, @intCast(region.height +| canvas.clip_bottom)) -
        //     @as(i32, @intCast(padding_y)) +
        //     @divTrunc(
        //         @as(i32, @intCast(metrics.cell_height)) - @as(i32, @intCast(height)),
        //         2,
        //     );
        let offset_y: i32 = region.height.saturating_add(canvas.clip_bottom) as i32
            - padding_y as i32
            // By adding half the difference between the cell height and the
            // height we passed to the draw function, we center it in the cell.
            + (metrics.cell_height as i32 - height as i32) / 2;

        Ok(Glyph {
            width: region.width,
            height: region.height,
            offset_x,
            offset_y,
            atlas_x: region.x,
            atlas_y: region.y,
        })
    }
}

/// The presentation for an emoji.
// PORT NOTE: mirror of `font.Presentation` (`src/font/main.zig`, `enum(u1)`),
// reproduced locally so `Face::has_codepoint`'s signature is faithful until the
// font module root is carved. It is only ever an ignored parameter here.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Presentation {
    /// U+FE0E
    Text = 0,
    /// U+FE0F
    Emoji = 1,
}

/// The combined error set for `render_glyph`, mirroring the reference's inferred
/// `!font.Glyph` union of `Canvas.init` (surface alloc), the dispatched
/// `DrawFn`, and `Canvas.writeAtlas` (atlas reserve) errors.
#[derive(Debug)]
pub enum RenderGlyphError {
    Surface(rustty_z2d::surface::Error),
    Draw(DrawFnError),
    Reserve(atlas::ReserveError),
}

impl From<rustty_z2d::surface::Error> for RenderGlyphError {
    fn from(e: rustty_z2d::surface::Error) -> RenderGlyphError {
        RenderGlyphError::Surface(e)
    }
}

impl From<DrawFnError> for RenderGlyphError {
    fn from(e: DrawFnError) -> RenderGlyphError {
        RenderGlyphError::Draw(e)
    }
}

impl From<atlas::ReserveError> for RenderGlyphError {
    fn from(e: atlas::ReserveError) -> RenderGlyphError {
        RenderGlyphError::Reserve(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_util::quirks::inlineAssert as assert;
    use rustty_z2d::compositor::Operator;
    use rustty_z2d::pixel::Alpha8;
    use rustty_z2d::surface::{Surface, SurfaceType};

    /// Ports the reference's comptime overlap assertion: after sorting the
    /// ranges by `min`, each range's `min` must exceed the previous range's
    /// `max` (`if (n.min <= i) @compileError(...)`, with `i` seeded at 0 and
    /// advanced to each `n.max`). Also asserts the table is sorted ascending, as
    /// the reference guarantees via `sortUnstableContext` before the check.
    #[test]
    fn ranges_have_no_overlap() {
        // i = 0;
        let mut i: u32 = 0;
        // for (r, 0..) |n, k| { if (n.min <= i) @compileError(...); i = n.max; }
        for (k, n) in RANGES.iter().enumerate() {
            if k > 0 {
                assert!(
                    RANGES[k - 1].min < n.min,
                    "ranges must be sorted ascending by min"
                );
            }
            assert!(
                n.min > i || k == 0,
                "Codepoint range at index {k} overlaps the previous range: {:X} <= {:X}",
                n.min,
                i
            );
            i = n.max;
        }
    }

    /// Used in `test_draw_ranges`, checks for diff between the provided atlas
    /// and the reference file for the range, returns true if there is a diff.
    // fn testDiffAtlas(alloc, atlas, path, i, width, height, thickness) !bool {
    //
    // PORT NOTE: z2d's PNG *exporter* is not part of the ported subset; the
    // pure-Rust `png` dev-dependency stands in for it on the encode side (see
    // `encode_alpha8_png`) and for `wuffs.png.decode` on the decode side. This
    // reproduces the reference's integrity contract in full: encode the
    // rendered atlas to PNG bytes, byte-compare them against the checked-in
    // reference FIRST, and only decode + pixel-diff on mismatch — including
    // the explicit bytes-differ-but-pixels-match failure arm ("smuggle binary
    // data"). The reference files are 8-bit grayscale PNGs whose per-pixel
    // gray value is exactly the alpha8 coverage, re-encoded with the pinned
    // encoder settings of `encode_alpha8_png` so byte equality holds.
    //
    // The failure artifacts are written as in the reference (S97-F3): the
    // test PNG copied into the cwd on any failure, and the red/green diff PNG
    // on a pixel difference. CORRECTION (task T-F): this note said both were
    // "debugging aids with no analogue here".
    fn test_diff_atlas(atlas: &Surface, i: u32, width: u32, height: u32, thickness: u32) -> bool {
        // const ref_path = "./src/font/sprite/testdata/U+{X}...U+{X}-{d}x{d}+{d}.png"
        let ref_path = format!(
            "{}/res/U+{:X}...U+{:X}-{}x{}+{}.png",
            env!("CARGO_MANIFEST_DIR"),
            i,
            i + 0xFF,
            width,
            height,
            thickness,
        );
        // const cwd_absolute = try std.Io.Dir.cwd().realPathFileAlloc(std.testing.io, ".", alloc);
        let cwd_absolute = std::env::current_dir().unwrap();
        test_diff_atlas_at(atlas, &ref_path, &cwd_absolute, i, width, height, thickness)
    }

    /// `test_diff_atlas` with the reference path and the directory the
    /// artifacts go to (the reference's cwd) as arguments, so the failure arms
    /// can be tested.
    fn test_diff_atlas_at(
        atlas: &Surface,
        ref_path: &str,
        cwd_absolute: &std::path::Path,
        i: u32,
        width: u32,
        height: u32,
        thickness: u32,
    ) -> bool {
        // Get the file contents, we compare the PNG data first in
        // order to ensure that no one smuggles arbitrary binary
        // data in to the reference PNGs.
        //
        // (The reference exports the atlas to a tmp-dir PNG file and reads it
        // back; here the atlas is encoded to PNG bytes in memory.)
        let test_bytes = encode_alpha8_png(atlas);

        // "{s}/sprite_face_test-U+{X}...U+{X}-{d}x{d}+{d}.png"
        let test_path = cwd_absolute.join(format!(
            "sprite_face_test-U+{i:X}...U+{:X}-{width}x{height}+{thickness}.png",
            i + 0xFF,
        ));

        // const ref_file = std.fs.cwd().openFile(ref_path, ...) catch |err| { ... };
        let ref_bytes = match std::fs::read(ref_path) {
            Ok(b) => b,
            Err(err) => {
                // log.err("Can't open reference file {s}: {}\n", .{ ref_path, err });
                eprintln!("error(font_sprite): Can't open reference file {ref_path}: {err}\n");

                // Copy the test PNG in to the CWD so it isn't
                // cleaned up with the rest of the tmp dir files.
                std::fs::write(&test_path, &test_bytes).unwrap();

                return true;
            }
        };

        let outcome = diff_encoded_atlas(atlas, &test_bytes, &ref_bytes);

        // Regeneration arm (no reference analogue): with
        // `RUSTTY_REGEN_FONT_KAT_PNGS` set, rewrite the checked-in reference
        // from the freshly rendered atlas — but only when the decoded pixels
        // are exact, so a regeneration can never smuggle a visual change past
        // the KAT. This is needed when the pinned `png` encoder settings (or
        // the crate's encoded output) change: the byte-level contract treats
        // an encoder upgrade as a reference-regeneration event, exactly as
        // the reference treats z2d exporter changes.
        if std::env::var_os("RUSTTY_REGEN_FONT_KAT_PNGS").is_some()
            && outcome == AtlasDiff::BytesDifferPixelsMatch
        {
            std::fs::write(ref_path, &test_bytes).unwrap();
            eprintln!("info(font_sprite): regenerated reference file {ref_path}");
            return false;
        }
        // BytesMatch needs no rewrite; PixelsDiffer falls through to the
        // failure arm below — regeneration never overwrites a visual
        // change.

        // if (std.mem.eql(u8, test_bytes, ref_bytes)) return false;
        if outcome == AtlasDiff::BytesMatch {
            return false;
        }

        // Copy the test PNG in to the CWD so it isn't
        // cleaned up with the rest of the tmp dir files.
        std::fs::write(&test_path, &test_bytes).unwrap();
        let test_path = test_path.display();

        match outcome {
            AtlasDiff::BytesMatch => unreachable!("returned above"),
            AtlasDiff::BytesDifferPixelsMatch => {
                // If the PNG data differs but not the raw pixels, that's
                // a big red flag, since it could mean someone is trying to
                // smuggle binary data in to the test files.
                eprintln!(
                    "error(font_sprite): !!! Test PNG data does not match reference, but pixels \
                     do match! Either the `png` crate's encoder changed or someone is trying to \
                     smuggle binary data in the test files!\ntest={test_path}, reference={ref_path}",
                );
                true
            }
            AtlasDiff::PixelsDiffer => {
                // Drop the diff image as a PNG in the cwd.
                // "./sprite_face_diff-U+{X}...U+{X}-{d}x{d}+{d}.png"
                let diff_path = cwd_absolute.join(format!(
                    "sprite_face_diff-U+{i:X}...U+{:X}-{width}x{height}+{thickness}.png",
                    i + 0xFF,
                ));
                std::fs::write(&diff_path, diff_png(atlas, &ref_bytes)).unwrap();
                eprintln!(
                    "error(font_sprite): One or more glyphs differ from reference file in range \
                     U+{i:X}...U+{:X}! test={test_path}, reference={ref_path}, diff={}",
                    i + 0xFF,
                    diff_path.display(),
                );
                true
            }
        }
    }

    /// The reference's visual diff (`Face.zig:343-375`): an RGB image the size
    /// of the atlas where a pixel that matches the reference is a faded gray
    /// (`t / 3`) and one that differs carries the reference value in red and
    /// the new value in green. Encoded as an 8-bit RGB PNG (the reference
    /// exports its `image_surface_rgb` z2d surface).
    fn diff_png(atlas: &Surface, ref_bytes: &[u8]) -> Vec<u8> {
        let decoder = png::Decoder::new(std::io::Cursor::new(ref_bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut ref_gray = vec![0u8; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut ref_gray).unwrap();
        ref_gray.truncate(info.buffer_size());

        // var diff = try z2d.Surface.init(.image_surface_rgb, alloc, w, h);
        let test_gray = atlas.alpha8_buf();
        let mut diff_pix = vec![0u8; test_gray.len() * 3];
        for j in 0..test_gray.len() {
            let t = test_gray[j].a;
            let r = ref_gray[j];
            let px = &mut diff_pix[j * 3..j * 3 + 3];
            if t == r {
                // If the pixels match, write it as a faded gray.
                px.copy_from_slice(&[t / 3, t / 3, t / 3]);
            } else {
                // Otherwise put the reference value in the red
                // channel and the new value in the green channel.
                px[0] = r;
                px[1] = t;
            }
        }

        let mut out: Vec<u8> = Vec::new();
        let mut encoder = png::Encoder::new(
            &mut out,
            atlas.get_width() as u32,
            atlas.get_height() as u32,
        );
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&diff_pix).unwrap();
        writer.finish().unwrap();
        out
    }

    /// Outcome of comparing the encoded test atlas against a reference PNG.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum AtlasDiff {
        /// Encoded PNG bytes are identical; nothing to decode.
        BytesMatch,
        /// Encoded bytes differ but every decoded pixel matches: possible
        /// binary smuggling in the reference file (or encoder drift). An
        /// explicit failure in the reference, and here.
        BytesDifferPixelsMatch,
        /// One or more decoded pixels differ from the rendered atlas.
        PixelsDiffer,
    }

    /// Comparison core of `test_diff_atlas`, split out so the byte-integrity
    /// contract itself has direct unit coverage (see the `diff_atlas_*` tests).
    fn diff_encoded_atlas(atlas: &Surface, test_bytes: &[u8], ref_bytes: &[u8]) -> AtlasDiff {
        // Do our PNG bytes comparison, if it's the same then we can
        // move on, otherwise we'll decode the reference file and do
        // a pixel-for-pixel diff.
        // if (std.mem.eql(u8, test_bytes, ref_bytes)) return false;
        if test_bytes == ref_bytes {
            return AtlasDiff::BytesMatch;
        }

        // Use the pure-Rust `png` decoder to decode the reference PNG to raw
        // pixels (replacing `wuffs.png.decode`). The references are 8-bit
        // grayscale, so one decoded byte per pixel is exactly the alpha8
        // coverage we compare against.
        let decoder = png::Decoder::new(std::io::Cursor::new(ref_bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut ref_gray = vec![0u8; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut ref_gray).unwrap();
        assert(info.color_type == png::ColorType::Grayscale);
        assert(info.bit_depth == png::BitDepth::Eight);
        ref_gray.truncate(info.buffer_size());

        // assert(ref_rgba.width == atlas.getWidth());
        assert(info.width == atlas.get_width() as u32);
        // assert(ref_rgba.height == atlas.getHeight());
        assert(info.height == atlas.get_height() as u32);

        // const test_gray = std.mem.sliceAsBytes(atlas.image_surface_alpha8.buf);
        let test_gray = atlas.alpha8_buf();
        assert(test_gray.len() == ref_gray.len());

        // var differs: bool = false;
        let mut differs: bool = false;
        // for (0..test_gray.len) |j| {
        for j in 0..test_gray.len() {
            // const t = test_gray[j];
            let t = test_gray[j].a;
            // const r = ref_rgba.data[j * 4];
            let r = ref_gray[j];
            // if (t == r) { ... } else { differs = true; ... }
            if t != r {
                differs = true;
            }
        }

        if differs {
            AtlasDiff::PixelsDiffer
        } else {
            AtlasDiff::BytesDifferPixelsMatch
        }
    }

    /// Encode an alpha8 surface to PNG bytes: 8-bit grayscale, one gray byte
    /// per pixel equal to the alpha8 coverage — the same shape the checked-in
    /// reference files use.
    //
    // PORT NOTE: stands in for the reference's `z2d.png_exporter`. The
    // compression and filter settings are pinned explicitly because the
    // checked-in reference bytes are coupled to them: changing them (or
    // picking up a `png` crate release that changes encoder output) is a
    // reference-regeneration event (`RUSTTY_REGEN_FONT_KAT_PNGS`), exactly as
    // the reference treats z2d exporter changes.
    fn encode_alpha8_png(atlas: &Surface) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut encoder = png::Encoder::new(
            &mut out,
            atlas.get_width() as u32,
            atlas.get_height() as u32,
        );
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Balanced);
        encoder.set_filter(png::Filter::Adaptive);
        let mut writer = encoder.write_header().unwrap();
        let gray: Vec<u8> = atlas.alpha8_buf().iter().map(|px| px.a).collect();
        writer.write_image_data(&gray).unwrap();
        writer.finish().unwrap();
        out
    }

    /// Build a small deterministic alpha8 surface for the `diff_atlas_*` unit
    /// tests below.
    fn diff_test_surface() -> Surface {
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 16, 16).unwrap();
        for (j, px) in sfc.alpha8_buf_mut().iter_mut().enumerate() {
            px.a = (j * 7 % 251) as u8;
        }
        sfc
    }

    /// W9-02 proving test: a byte-identical reference short-circuits to a
    /// pass without decoding.
    #[test]
    fn diff_atlas_accepts_byte_identical_reference() {
        let sfc = diff_test_surface();
        let bytes = encode_alpha8_png(&sfc);
        assert_eq!(
            diff_encoded_atlas(&sfc, &bytes, &bytes),
            AtlasDiff::BytesMatch
        );
    }

    /// W9-02 proving test: a reference re-encoded with an extra (valid) tEXt
    /// chunk decodes to identical pixels but has different encoded bytes. The
    /// reference explicitly fails this arm (`Face.zig` `testDiffAtlas`:
    /// "smuggle binary data"); before the byte-compare gate was ported, the
    /// sprite KAT silently passed such a reference.
    #[test]
    fn diff_atlas_flags_byte_smuggle_when_pixels_match() {
        let sfc = diff_test_surface();
        let test_bytes = encode_alpha8_png(&sfc);

        // Same pixels, plus a smuggled tEXt chunk => different encoded bytes.
        let mut smuggled: Vec<u8> = Vec::new();
        let mut encoder = png::Encoder::new(&mut smuggled, 16, 16);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Balanced);
        encoder.set_filter(png::Filter::Adaptive);
        encoder
            .add_text_chunk("Comment".to_string(), "smuggled".to_string())
            .unwrap();
        let mut writer = encoder.write_header().unwrap();
        let gray: Vec<u8> = sfc.alpha8_buf().iter().map(|px| px.a).collect();
        writer.write_image_data(&gray).unwrap();
        writer.finish().unwrap();

        assert!(test_bytes != smuggled);
        assert_eq!(
            diff_encoded_atlas(&sfc, &test_bytes, &smuggled),
            AtlasDiff::BytesDifferPixelsMatch
        );
    }

    /// W9-02 control: a pixel edit is still reported as a pixel diff, not as
    /// byte smuggling.
    #[test]
    fn diff_atlas_flags_pixel_diff() {
        let sfc = diff_test_surface();
        let test_bytes = encode_alpha8_png(&sfc);
        let mut edited = diff_test_surface();
        edited.alpha8_buf_mut()[42].a ^= 0xFF;
        let ref_bytes = encode_alpha8_png(&edited);
        assert_eq!(
            diff_encoded_atlas(&sfc, &test_bytes, &ref_bytes),
            AtlasDiff::PixelsDiffer
        );
    }

    /// A fresh, empty directory standing in for the reference's cwd.
    fn artifact_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rustty-s97-f3-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file_names(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    const TEST_PNG: &str = "sprite_face_test-U+2500...U+25FF-9x17+1.png";
    const DIFF_PNG: &str = "sprite_face_diff-U+2500...U+25FF-9x17+1.png";

    /// S97-F3: on every failure `testDiffAtlas` copies the test PNG into the
    /// cwd (`Face.zig:287-296`, `:307-321`), and on a pixel difference writes
    /// the red/green diff PNG there too (`:343-395`); a pass writes nothing.
    #[test]
    fn diff_atlas_failures_leave_the_reference_artifacts() {
        let sfc = diff_test_surface();
        let test_bytes = encode_alpha8_png(&sfc);

        // A pass: nothing is written.
        let dir = artifact_dir("pass");
        let ref_path = dir.join("ref.png");
        std::fs::write(&ref_path, &test_bytes).unwrap();
        assert!(!test_diff_atlas_at(
            &sfc,
            ref_path.to_str().unwrap(),
            &dir,
            0x2500,
            9,
            17,
            1
        ));
        assert_eq!(file_names(&dir), ["ref.png"]);

        // No reference file: the test PNG, and nothing else.
        let dir = artifact_dir("missing");
        let missing = dir.join("absent.png");
        assert!(test_diff_atlas_at(
            &sfc,
            missing.to_str().unwrap(),
            &dir,
            0x2500,
            9,
            17,
            1
        ));
        assert_eq!(file_names(&dir), [TEST_PNG]);
        assert_eq!(std::fs::read(dir.join(TEST_PNG)).unwrap(), test_bytes);

        // A pixel difference: the test PNG and the diff PNG.
        let dir = artifact_dir("pixels");
        let mut edited = diff_test_surface();
        edited.alpha8_buf_mut()[42].a ^= 0xFF;
        let ref_path = dir.join("ref.png");
        std::fs::write(&ref_path, encode_alpha8_png(&edited)).unwrap();
        assert!(test_diff_atlas_at(
            &sfc,
            ref_path.to_str().unwrap(),
            &dir,
            0x2500,
            9,
            17,
            1
        ));
        assert_eq!(file_names(&dir), ["ref.png", DIFF_PNG, TEST_PNG]);
        assert_eq!(std::fs::read(dir.join(TEST_PNG)).unwrap(), test_bytes);
        // The diff: faded gray where equal, reference in red and new value
        // in green where not.
        let diff = std::fs::read(dir.join(DIFF_PNG)).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(&diff))
            .read_info()
            .unwrap();
        let mut rgb = vec![0u8; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgb).unwrap();
        assert_eq!(
            (info.color_type, info.width, info.height),
            (png::ColorType::Rgb, 16, 16)
        );
        let t = |j: usize| sfc.alpha8_buf()[j].a;
        assert_eq!(&rgb[0..3], &[t(0) / 3, t(0) / 3, t(0) / 3]);
        assert_eq!(&rgb[42 * 3..42 * 3 + 3], &[t(42) ^ 0xFF, t(42), 0]);

        // Bytes differ, pixels match: the test PNG, no diff.
        let dir = artifact_dir("smuggle");
        let mut smuggled: Vec<u8> = Vec::new();
        let mut encoder = png::Encoder::new(&mut smuggled, 16, 16);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .add_text_chunk("Comment".to_string(), "smuggled".to_string())
            .unwrap();
        let mut writer = encoder.write_header().unwrap();
        let gray: Vec<u8> = sfc.alpha8_buf().iter().map(|px| px.a).collect();
        writer.write_image_data(&gray).unwrap();
        writer.finish().unwrap();
        let ref_path = dir.join("ref.png");
        std::fs::write(&ref_path, &smuggled).unwrap();
        assert!(test_diff_atlas_at(
            &sfc,
            ref_path.to_str().unwrap(),
            &dir,
            0x2500,
            9,
            17,
            1
        ));
        assert_eq!(file_names(&dir), ["ref.png", TEST_PNG]);

        for tag in ["pass", "missing", "pixels", "smuggle"] {
            let _ = std::fs::remove_dir_all(artifact_dir(tag));
        }
    }

    /// Draws all ranges in to a set of 16x16 glyph atlases, checks for regressions
    /// against reference files, logs errors and exposes a diff for any difference
    /// between the reference and test atlas.
    ///
    /// Returns true if there was a diff.
    // fn testDrawRanges(width, ascent, descent, thickness) !bool {
    fn test_draw_ranges(width: u32, ascent: u32, descent: u32, thickness: u32) -> bool {
        // const metrics: font.Metrics = .calc(.{
        //     .px_per_em = 16,
        //     .cell_width = @floatFromInt(width),
        //     .ascent = @floatFromInt(ascent),
        //     .descent = -@as(f64, @floatFromInt(descent)),
        //     .line_gap = 0.0,
        //     .underline_thickness = @floatFromInt(thickness),
        //     .strikethrough_thickness = @floatFromInt(thickness),
        // });
        //
        // PORT NOTE: the reference sets only these seven `FaceMetrics` fields;
        // the remaining optionals default to `null` (`None` here).
        let metrics: Metrics = Metrics::calc(crate::metrics::FaceMetrics {
            // Fudged number, not used in anything we care about here.
            px_per_em: 16.0,

            cell_width: width as f64,
            ascent: ascent as f64,
            descent: -(descent as f64),
            line_gap: 0.0,
            underline_thickness: Some(thickness as f64),
            strikethrough_thickness: Some(thickness as f64),
            underline_position: None,
            strikethrough_position: None,
            cap_height: None,
            ex_height: None,
            ascii_height: None,
            ic_width: None,
        });

        // const height = ascent + descent;
        let height = ascent + descent;

        // const padding_x = width / 4;
        let padding_x = width / 4;
        // const padding_y = height / 4;
        let padding_y = height / 4;

        // Canvas to draw glyphs on, we'll reuse this for all glyphs.
        // var canvas = try font.sprite.Canvas.init(alloc, width, height, padding_x, padding_y);
        // defer canvas.deinit();
        let mut canvas = Canvas::init(width, height, padding_x, padding_y).unwrap();

        // We render glyphs in batches of 256, which we copy (including padding) to
        // a 16 by 16 surface to be compared with the reference file for that range.
        // const stride_x = width + 2 * padding_x;
        let stride_x = width + 2 * padding_x;
        // const stride_y = height + 2 * padding_y;
        let stride_y = height + 2 * padding_y;
        // var atlas = try z2d.Surface.init(.image_surface_alpha8, alloc, stride_x*16, stride_y*16);
        // defer atlas.deinit(alloc);
        let mut atlas = Surface::init(
            SurfaceType::ImageSurfaceAlpha8,
            (stride_x * 16) as i32,
            (stride_y * 16) as i32,
        )
        .unwrap();

        // var i: u32 = std.mem.alignBackward(u32, ranges[0].min, 0x100);
        let mut i: u32 = align_backward(RANGES[0].min, 0x100);

        // Try to make the sprite_face_test folder if it doesn't already exist.
        // var dir = testing.tmpDir(.{});
        // defer dir.cleanup();
        // const tmp_dir = try dir.dir.realpathAlloc(alloc, ".");
        // defer alloc.free(tmp_dir);
        //
        // PORT NOTE: the reference makes a tmp dir for the exported PNGs; not
        // needed here since there is no PNG export in the ported z2d subset.

        // We set this to true if we have any fails so we can
        // return an error after we're done comparing all glyphs.
        // var fail: bool = false;
        let mut fail: bool = false;

        // inline for (ranges) |range| {
        for range in RANGES {
            // for (range.min..range.max + 1) |cp| {
            for cp in range.min..range.max + 1 {
                // If we've moved to a new batch of 256, check the
                // current one and clear the surface for the next one.
                // if (cp - i >= 0x100) {
                if cp - i >= 0x100 {
                    // Export to our tmp dir. (No export in this port — diff the
                    // in-memory atlas directly against the reference PNG.)
                    // if (try testDiffAtlas(alloc, &atlas, path, i, width, height, thickness)) fail = true;
                    if test_diff_atlas(&atlas, i, width, height, thickness) {
                        fail = true;
                    }

                    // i = std.mem.alignBackward(u32, @intCast(cp), 0x100);
                    i = align_backward(cp, 0x100);
                    // @memset(std.mem.sliceAsBytes(atlas.image_surface_alpha8.buf), 0);
                    for px in atlas.alpha8_buf_mut() {
                        px.a = 0;
                    }
                }

                // try getDrawFn(@intCast(cp)).?(@intCast(cp), &canvas, width, height, metrics);
                (get_draw_fn(cp).unwrap())(cp, &mut canvas, width, height, metrics).unwrap();
                // canvas.clearClippingRegions();
                canvas.clearClippingRegions();
                // atlas.composite(&canvas.sfc, .src, stride_x*((cp-i)%16), stride_y*((cp-i)/16), .{});
                atlas.composite(
                    &canvas.sfc,
                    Operator::Src,
                    (stride_x * ((cp - i) % 16)) as i32,
                    (stride_y * ((cp - i) / 16)) as i32,
                );
                // @memset(std.mem.sliceAsBytes(canvas.sfc.image_surface_alpha8.buf), 0);
                for px in canvas.sfc.alpha8_buf_mut() {
                    px.a = 0;
                }
                // canvas.clip_top = 0;
                canvas.clip_top = 0;
                // canvas.clip_left = 0;
                canvas.clip_left = 0;
                // canvas.clip_right = 0;
                canvas.clip_right = 0;
                // canvas.clip_bottom = 0;
                canvas.clip_bottom = 0;
            }
        }

        // Final batch.
        // if (try testDiffAtlas(alloc, &atlas, path, i, width, height, thickness)) fail = true;
        if test_diff_atlas(&atlas, i, width, height, thickness) {
            fail = true;
        }

        // return fail;
        fail
    }

    // std.mem.alignBackward(u32, value, 0x100): round `value` down to a multiple
    // of `align` (a power of two).
    fn align_backward(value: u32, align: u32) -> u32 {
        value & !(align - 1)
    }

    // test "sprite face render all sprites" {
    #[test]
    fn sprite_face_render_all_sprites() {
        // This test is way too slow to run under Valgrind, unfortunately.
        // if (std.valgrind.runningOnValgrind() > 0) return error.SkipZigTest;
        //
        // PORT NOTE: no Valgrind guard here; the Miri exception list governs
        // whether this test runs under Miri (it is far too slow for it).

        // Renders all sprites to an atlas and compares
        // it to a ground truth for regression testing.

        // var diff: bool = false;
        let mut diff: bool = false;

        // testDrawRanges(width, ascent, descent, thickness):
        //
        // We compare 4 different sets of metrics;
        // - even cell size / even thickness
        // - even cell size / odd thickness
        // - odd cell size / even thickness
        // - odd cell size / odd thickness
        // (Also a decreasing range of sizes.)
        // if (try testDrawRanges(18, 30, 6, 4)) diff = true;
        if test_draw_ranges(18, 30, 6, 4) {
            diff = true;
        }
        // if (try testDrawRanges(12, 20, 4, 3)) diff = true;
        if test_draw_ranges(12, 20, 4, 3) {
            diff = true;
        }
        // if (try testDrawRanges(11, 19, 2, 2)) diff = true;
        if test_draw_ranges(11, 19, 2, 2) {
            diff = true;
        }
        // if (try testDrawRanges(9, 15, 2, 1)) diff = true;
        if test_draw_ranges(9, 15, 2, 1) {
            diff = true;
        }

        // try std.testing.expect(!diff); // There should be no diffs from reference.
        assert!(!diff); // There should be no diffs from reference.
    }

    // test "full height cursor sprites respect cursor height metric"
    // PORT NOTE (parity e8f3f6c43): regression-test hunk for the upstream
    // adjust-cursor-height fix (dac341cad). The reference builds metrics via
    // `.calc(.{ px_per_em, cell_width, ascent, descent, line_gap })`; the
    // remaining FaceMetrics fields default to null and are spelled `None` here.
    #[test]
    fn full_height_cursor_sprites_respect_cursor_height_metric() {
        let alloc = allocator_api2::alloc::Global;

        // var atlas: font.Atlas = try .init(alloc, 128, .grayscale);
        // defer atlas.deinit(alloc);
        let mut atlas = Atlas::init(alloc, 128, crate::atlas::Format::Grayscale).unwrap();

        // face with a 8x16 cell dimension.
        let mut face = Face {
            metrics: Metrics::calc(crate::metrics::FaceMetrics {
                // Fudged number, not used in anything we care about here.
                px_per_em: 16.0,

                cell_width: 8.0,
                ascent: 12.0,
                descent: -4.0,
                line_gap: 0.0,
                underline_thickness: None,
                strikethrough_thickness: None,
                underline_position: None,
                strikethrough_position: None,
                cap_height: None,
                ex_height: None,
                ascii_height: None,
                ic_width: None,
            }),
        };

        // try testing.expectEqual(16, face.metrics.cell_height);
        assert_eq!(16, face.metrics.cell_height);

        // PORT NOTE: the reference repeats a render+assert block per cursor
        // sprite and cursor_height; this helper is that block, kept
        // assertion-for-assertion.
        fn check(face: &Face, atlas: &mut Atlas, cp: u32, want_height: u32, want_offset_y: i32) {
            let glyph = face
                .render_glyph(
                    allocator_api2::alloc::Global,
                    atlas,
                    cp,
                    crate::glyph::RenderOptions {
                        grid_metrics: face.metrics,
                        ..crate::glyph::RenderOptions::default()
                    },
                )
                .unwrap();
            assert_eq!(want_height, glyph.height);
            assert_eq!(want_offset_y, glyph.offset_y);
        }

        // --- smaller than cell height ---
        face.metrics.cursor_height = 12;
        // bar
        check(&face, &mut atlas, Sprite::CursorBar as u32, 12, 14);
        // rect
        check(&face, &mut atlas, Sprite::CursorRect as u32, 12, 14);
        // hollow rect
        check(&face, &mut atlas, Sprite::CursorHollowRect as u32, 12, 14);

        // --- equal to the cell height ---
        face.metrics.cursor_height = 16;
        // bar
        check(&face, &mut atlas, Sprite::CursorBar as u32, 16, 16);
        // rect
        check(&face, &mut atlas, Sprite::CursorRect as u32, 16, 16);
        // hollow rect
        check(&face, &mut atlas, Sprite::CursorHollowRect as u32, 16, 16);

        // --- greater than the cell height ---
        face.metrics.cursor_height = 20;
        // bar
        check(&face, &mut atlas, Sprite::CursorBar as u32, 20, 18);
        // rect
        check(&face, &mut atlas, Sprite::CursorRect as u32, 20, 18);
        // hollow rect
        check(&face, &mut atlas, Sprite::CursorHollowRect as u32, 20, 18);
    }

    // A tiny compile/behaviour smoke over the public surface, standing in for the
    // reference's `test { std.testing.refAllDecls(@This()); }`.
    #[test]
    fn has_codepoint_and_blank_glyph() {
        let face = Face {
            metrics: Metrics::calc(crate::metrics::FaceMetrics {
                px_per_em: 16.0,
                cell_width: 9.0,
                ascent: 15.0,
                descent: -2.0,
                line_gap: 0.0,
                underline_thickness: Some(1.0),
                strikethrough_thickness: Some(1.0),
                underline_position: None,
                strikethrough_position: None,
                cap_height: None,
                ex_height: None,
                ascii_height: None,
                ic_width: None,
            }),
        };

        // A box-drawing codepoint is present; an ASCII letter is not.
        assert!(face.has_codepoint(0x2500, None));
        assert!(!face.has_codepoint(u32::from('A'), Some(Presentation::Text)));

        // A special (sprite) codepoint resolves to a special draw fn.
        assert!(face.has_codepoint(Sprite::CursorRect as u32, None));

        // Rendering a codepoint we don't have yields a blank, valid glyph.
        let mut atlas = Atlas::init(
            allocator_api2::alloc::Global,
            32,
            crate::atlas::Format::Grayscale,
        )
        .unwrap();
        let glyph = face
            .render_glyph(
                allocator_api2::alloc::Global,
                &mut atlas,
                Sprite::CursorRect as u32,
                crate::glyph::RenderOptions {
                    grid_metrics: face.metrics,
                    ..crate::glyph::RenderOptions::default()
                },
            )
            .unwrap();
        let _ = glyph;
        let _ = Alpha8 { a: 0 };
    }

    // test "sprite face print all sprites" {
    //     std.debug.print("\n\n", .{});
    //     inline for (ranges) |range| {
    //         for (range.min..range.max + 1) |cp| {
    //             std.debug.print("{u}", .{ @as(u21, @intCast(cp)) });
    //         }
    //     }
    //     std.debug.print("\n\n", .{});
    // }
    //
    // PORT NOTE: the reference's commented-out debugging test (prints every
    // sprite codepoint) is preserved verbatim above as a reference comment.

    // test {
    //     std.testing.refAllDecls(@This());
    // }
    //
    // PORT NOTE: `refAllDecls` is a Zig comptime smoke test that every public
    // declaration type-checks; Rust's per-item compilation gives this coverage
    // inherently. `has_codepoint_and_blank_glyph` above additionally exercises
    // the public `Face` surface at runtime.
}
