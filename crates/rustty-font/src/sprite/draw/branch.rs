//! Branch Drawing Characters | U+F5D0...U+F60D
//!
//! Branch drawing character set, used for drawing git-like
//! graphs in the terminal. Originally implemented in Kitty.
//! Ref:
//! - https://github.com/kovidgoyal/kitty/pull/7681
//! - https://github.com/kovidgoyal/kitty/pull/7805
//! NOTE: Kitty is GPL licensed, and its code was not referenced
//!       for these characters, only the loose specification of
//!       the character set in the pull request descriptions.
//!
//!
//!
//!
//!
//!

// The reference `pub fn` keeps its camelCase name (`drawF5D0_F60D`); preserved
// verbatim.
#![allow(non_snake_case)]
// The module doc block reproduces the reference `//!` header verbatim (including
// the Kitty `NOTE:` prose and PR link list); the exact indentation trips these
// rustdoc-list lints, so they are allowed here to keep the header byte-faithful.
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]

// const std = @import("std");
// const Allocator = std.mem.Allocator;
//
// const common = @import("common.zig");
// const Thickness = common.Thickness;
// const Shade = common.Shade;
// const Edge = common.Edge;
// const hlineMiddle = common.hlineMiddle;
// const vlineMiddle = common.vlineMiddle;
//
// const arc = @import("box.zig").arc;
//
// const font = @import("../../main.zig");
//
// PORT NOTE: `common` is the sibling `super::common` module; `arc` is the
// sibling `super::r#box::arc`. `font = ../../main.zig` becomes the local crate
// (`crate::metrics::Metrics`, `crate::sprite::Canvas` / `Color`). `Allocator`
// is unused in this module. The managed-Context drawing needs the z2d
// opaque-pattern source types plus `PI`; `arc` propagates `StrokeError`.
use super::common::{hlineMiddle, vlineMiddle, Corner, Edge, Shade, Thickness};
use super::r#box::arc;
use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color};
use rustty_z2d::painter::StrokeError;
use rustty_z2d::pattern::{OpaquePattern, Pattern};
use rustty_z2d::pixel::{Alpha8, Pixel};
use std::f64::consts::PI;

/// Specification of a branch drawing node, which consists of a
/// circle which is either empty or filled, and lines connecting
/// optionally between the circle and each of the 4 edges.
// const BranchNode = packed struct(u5) {
//     up: bool = false,
//     right: bool = false,
//     down: bool = false,
//     left: bool = false,
//     filled: bool = false,
// };
//
// PORT NOTE: the reference packs five flags into a `u5`; the Rust port carries
// them as five `bool` fields (default `false`), preserving the call-site
// semantics.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct BranchNode {
    up: bool,
    right: bool,
    down: bool,
    left: bool,
    filled: bool,
}

// pub fn drawF5D0_F60D(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn drawF5D0_F60D(
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
        // ''
        // 0x0f5d0 => hlineMiddle(metrics, canvas, .light),
        0x0f5d0 => hlineMiddle(metrics, canvas, Thickness::Light),
        // ''
        // 0x0f5d1 => vlineMiddle(metrics, canvas, .light),
        0x0f5d1 => vlineMiddle(metrics, canvas, Thickness::Light),
        // ''
        // 0x0f5d2 => fadingLine(metrics, canvas, .right, .light),
        0x0f5d2 => fadingLine(metrics, canvas, Edge::Right, Thickness::Light),
        // ''
        // 0x0f5d3 => fadingLine(metrics, canvas, .left, .light),
        0x0f5d3 => fadingLine(metrics, canvas, Edge::Left, Thickness::Light),
        // ''
        // 0x0f5d4 => fadingLine(metrics, canvas, .bottom, .light),
        0x0f5d4 => fadingLine(metrics, canvas, Edge::Bottom, Thickness::Light),
        // ''
        // 0x0f5d5 => fadingLine(metrics, canvas, .top, .light),
        0x0f5d5 => fadingLine(metrics, canvas, Edge::Top, Thickness::Light),
        // ''
        // 0x0f5d6 => try arc(metrics, canvas, .br, .light),
        0x0f5d6 => arc(metrics, canvas, Corner::Br, Thickness::Light)?,
        // ''
        // 0x0f5d7 => try arc(metrics, canvas, .bl, .light),
        0x0f5d7 => arc(metrics, canvas, Corner::Bl, Thickness::Light)?,
        // ''
        // 0x0f5d8 => try arc(metrics, canvas, .tr, .light),
        0x0f5d8 => arc(metrics, canvas, Corner::Tr, Thickness::Light)?,
        // ''
        // 0x0f5d9 => try arc(metrics, canvas, .tl, .light),
        0x0f5d9 => arc(metrics, canvas, Corner::Tl, Thickness::Light)?,
        // ''
        // 0x0f5da => {
        //     vlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .tr, .light);
        // },
        0x0f5da => {
            vlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Tr, Thickness::Light)?;
        }
        // ''
        // 0x0f5db => {
        //     vlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .br, .light);
        // },
        0x0f5db => {
            vlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Br, Thickness::Light)?;
        }
        // ''
        // 0x0f5dc => {
        //     try arc(metrics, canvas, .tr, .light);
        //     try arc(metrics, canvas, .br, .light);
        // },
        0x0f5dc => {
            arc(metrics, canvas, Corner::Tr, Thickness::Light)?;
            arc(metrics, canvas, Corner::Br, Thickness::Light)?;
        }
        // ''
        // 0x0f5dd => {
        //     vlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .tl, .light);
        // },
        0x0f5dd => {
            vlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Tl, Thickness::Light)?;
        }
        // ''
        // 0x0f5de => {
        //     vlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .bl, .light);
        // },
        0x0f5de => {
            vlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Bl, Thickness::Light)?;
        }
        // ''
        // 0x0f5df => {
        //     try arc(metrics, canvas, .tl, .light);
        //     try arc(metrics, canvas, .bl, .light);
        // },
        0x0f5df => {
            arc(metrics, canvas, Corner::Tl, Thickness::Light)?;
            arc(metrics, canvas, Corner::Bl, Thickness::Light)?;
        }

        // ''
        // 0x0f5e0 => {
        //     try arc(metrics, canvas, .bl, .light);
        //     hlineMiddle(metrics, canvas, .light);
        // },
        0x0f5e0 => {
            arc(metrics, canvas, Corner::Bl, Thickness::Light)?;
            hlineMiddle(metrics, canvas, Thickness::Light);
        }
        // ''
        // 0x0f5e1 => {
        //     try arc(metrics, canvas, .br, .light);
        //     hlineMiddle(metrics, canvas, .light);
        // },
        0x0f5e1 => {
            arc(metrics, canvas, Corner::Br, Thickness::Light)?;
            hlineMiddle(metrics, canvas, Thickness::Light);
        }
        // ''
        // 0x0f5e2 => {
        //     try arc(metrics, canvas, .br, .light);
        //     try arc(metrics, canvas, .bl, .light);
        // },
        0x0f5e2 => {
            arc(metrics, canvas, Corner::Br, Thickness::Light)?;
            arc(metrics, canvas, Corner::Bl, Thickness::Light)?;
        }
        // ''
        // 0x0f5e3 => {
        //     try arc(metrics, canvas, .tl, .light);
        //     hlineMiddle(metrics, canvas, .light);
        // },
        0x0f5e3 => {
            arc(metrics, canvas, Corner::Tl, Thickness::Light)?;
            hlineMiddle(metrics, canvas, Thickness::Light);
        }
        // ''
        // 0x0f5e4 => {
        //     try arc(metrics, canvas, .tr, .light);
        //     hlineMiddle(metrics, canvas, .light);
        // },
        0x0f5e4 => {
            arc(metrics, canvas, Corner::Tr, Thickness::Light)?;
            hlineMiddle(metrics, canvas, Thickness::Light);
        }
        // ''
        // 0x0f5e5 => {
        //     try arc(metrics, canvas, .tr, .light);
        //     try arc(metrics, canvas, .tl, .light);
        // },
        0x0f5e5 => {
            arc(metrics, canvas, Corner::Tr, Thickness::Light)?;
            arc(metrics, canvas, Corner::Tl, Thickness::Light)?;
        }
        // ''
        // 0x0f5e6 => {
        //     vlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .tl, .light);
        //     try arc(metrics, canvas, .tr, .light);
        // },
        0x0f5e6 => {
            vlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Tl, Thickness::Light)?;
            arc(metrics, canvas, Corner::Tr, Thickness::Light)?;
        }
        // ''
        // 0x0f5e7 => {
        //     vlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .bl, .light);
        //     try arc(metrics, canvas, .br, .light);
        // },
        0x0f5e7 => {
            vlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Bl, Thickness::Light)?;
            arc(metrics, canvas, Corner::Br, Thickness::Light)?;
        }
        // ''
        // 0x0f5e8 => {
        //     hlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .bl, .light);
        //     try arc(metrics, canvas, .tl, .light);
        // },
        0x0f5e8 => {
            hlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Bl, Thickness::Light)?;
            arc(metrics, canvas, Corner::Tl, Thickness::Light)?;
        }
        // ''
        // 0x0f5e9 => {
        //     hlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .tr, .light);
        //     try arc(metrics, canvas, .br, .light);
        // },
        0x0f5e9 => {
            hlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Tr, Thickness::Light)?;
            arc(metrics, canvas, Corner::Br, Thickness::Light)?;
        }
        // ''
        // 0x0f5ea => {
        //     vlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .tl, .light);
        //     try arc(metrics, canvas, .br, .light);
        // },
        0x0f5ea => {
            vlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Tl, Thickness::Light)?;
            arc(metrics, canvas, Corner::Br, Thickness::Light)?;
        }
        // ''
        // 0x0f5eb => {
        //     vlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .tr, .light);
        //     try arc(metrics, canvas, .bl, .light);
        // },
        0x0f5eb => {
            vlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Tr, Thickness::Light)?;
            arc(metrics, canvas, Corner::Bl, Thickness::Light)?;
        }
        // ''
        // 0x0f5ec => {
        //     hlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .tl, .light);
        //     try arc(metrics, canvas, .br, .light);
        // },
        0x0f5ec => {
            hlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Tl, Thickness::Light)?;
            arc(metrics, canvas, Corner::Br, Thickness::Light)?;
        }
        // ''
        // 0x0f5ed => {
        //     hlineMiddle(metrics, canvas, .light);
        //     try arc(metrics, canvas, .tr, .light);
        //     try arc(metrics, canvas, .bl, .light);
        // },
        0x0f5ed => {
            hlineMiddle(metrics, canvas, Thickness::Light);
            arc(metrics, canvas, Corner::Tr, Thickness::Light)?;
            arc(metrics, canvas, Corner::Bl, Thickness::Light)?;
        }
        // ''
        // 0x0f5ee => branchNode(metrics, canvas, .{ .filled = true }, .light),
        0x0f5ee => branchNode(
            metrics,
            canvas,
            BranchNode {
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5ef => branchNode(metrics, canvas, .{}, .light),
        0x0f5ef => branchNode(metrics, canvas, BranchNode::default(), Thickness::Light),

        // ''
        // 0x0f5f0 => branchNode(metrics, canvas, .{
        //     .right = true,
        //     .filled = true,
        // }, .light),
        0x0f5f0 => branchNode(
            metrics,
            canvas,
            BranchNode {
                right: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5f1 => branchNode(metrics, canvas, .{
        //     .right = true,
        // }, .light),
        0x0f5f1 => branchNode(
            metrics,
            canvas,
            BranchNode {
                right: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5f2 => branchNode(metrics, canvas, .{
        //     .left = true,
        //     .filled = true,
        // }, .light),
        0x0f5f2 => branchNode(
            metrics,
            canvas,
            BranchNode {
                left: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5f3 => branchNode(metrics, canvas, .{
        //     .left = true,
        // }, .light),
        0x0f5f3 => branchNode(
            metrics,
            canvas,
            BranchNode {
                left: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5f4 => branchNode(metrics, canvas, .{
        //     .left = true,
        //     .right = true,
        //     .filled = true,
        // }, .light),
        0x0f5f4 => branchNode(
            metrics,
            canvas,
            BranchNode {
                left: true,
                right: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5f5 => branchNode(metrics, canvas, .{
        //     .left = true,
        //     .right = true,
        // }, .light),
        0x0f5f5 => branchNode(
            metrics,
            canvas,
            BranchNode {
                left: true,
                right: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5f6 => branchNode(metrics, canvas, .{
        //     .down = true,
        //     .filled = true,
        // }, .light),
        0x0f5f6 => branchNode(
            metrics,
            canvas,
            BranchNode {
                down: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5f7 => branchNode(metrics, canvas, .{
        //     .down = true,
        // }, .light),
        0x0f5f7 => branchNode(
            metrics,
            canvas,
            BranchNode {
                down: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5f8 => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .filled = true,
        // }, .light),
        0x0f5f8 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5f9 => branchNode(metrics, canvas, .{
        //     .up = true,
        // }, .light),
        0x0f5f9 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5fa => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .down = true,
        //     .filled = true,
        // }, .light),
        0x0f5fa => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                down: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5fb => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .down = true,
        // }, .light),
        0x0f5fb => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                down: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5fc => branchNode(metrics, canvas, .{
        //     .right = true,
        //     .down = true,
        //     .filled = true,
        // }, .light),
        0x0f5fc => branchNode(
            metrics,
            canvas,
            BranchNode {
                right: true,
                down: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5fd => branchNode(metrics, canvas, .{
        //     .right = true,
        //     .down = true,
        // }, .light),
        0x0f5fd => branchNode(
            metrics,
            canvas,
            BranchNode {
                right: true,
                down: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5fe => branchNode(metrics, canvas, .{
        //     .left = true,
        //     .down = true,
        //     .filled = true,
        // }, .light),
        0x0f5fe => branchNode(
            metrics,
            canvas,
            BranchNode {
                left: true,
                down: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f5ff => branchNode(metrics, canvas, .{
        //     .left = true,
        //     .down = true,
        // }, .light),
        0x0f5ff => branchNode(
            metrics,
            canvas,
            BranchNode {
                left: true,
                down: true,
                ..Default::default()
            },
            Thickness::Light,
        ),

        // ''
        // 0x0f600 => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .right = true,
        //     .filled = true,
        // }, .light),
        0x0f600 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                right: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f601 => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .right = true,
        // }, .light),
        0x0f601 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                right: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f602 => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .left = true,
        //     .filled = true,
        // }, .light),
        0x0f602 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                left: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f603 => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .left = true,
        // }, .light),
        0x0f603 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                left: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f604 => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .down = true,
        //     .right = true,
        //     .filled = true,
        // }, .light),
        0x0f604 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                down: true,
                right: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f605 => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .down = true,
        //     .right = true,
        // }, .light),
        0x0f605 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                down: true,
                right: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f606 => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .down = true,
        //     .left = true,
        //     .filled = true,
        // }, .light),
        0x0f606 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                down: true,
                left: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f607 => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .down = true,
        //     .left = true,
        // }, .light),
        0x0f607 => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                down: true,
                left: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f608 => branchNode(metrics, canvas, .{
        //     .down = true,
        //     .left = true,
        //     .right = true,
        //     .filled = true,
        // }, .light),
        0x0f608 => branchNode(
            metrics,
            canvas,
            BranchNode {
                down: true,
                left: true,
                right: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f609 => branchNode(metrics, canvas, .{
        //     .down = true,
        //     .left = true,
        //     .right = true,
        // }, .light),
        0x0f609 => branchNode(
            metrics,
            canvas,
            BranchNode {
                down: true,
                left: true,
                right: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f60a => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .left = true,
        //     .right = true,
        //     .filled = true,
        // }, .light),
        0x0f60a => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                left: true,
                right: true,
                filled: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f60b => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .left = true,
        //     .right = true,
        // }, .light),
        0x0f60b => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                left: true,
                right: true,
                ..Default::default()
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f60c => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .down = true,
        //     .left = true,
        //     .right = true,
        //     .filled = true,
        // }, .light),
        0x0f60c => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                down: true,
                left: true,
                right: true,
                filled: true,
            },
            Thickness::Light,
        ),
        // ''
        // 0x0f60d => branchNode(metrics, canvas, .{
        //     .up = true,
        //     .down = true,
        //     .left = true,
        //     .right = true,
        // }, .light),
        0x0f60d => branchNode(
            metrics,
            canvas,
            BranchNode {
                up: true,
                down: true,
                left: true,
                right: true,
                ..Default::default()
            },
            Thickness::Light,
        ),

        // else => unreachable,
        _ => unreachable!(),
    }

    Ok(())
}

// fn branchNode(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     node: BranchNode,
//     comptime thickness: Thickness,
// ) void {
//
// PORT NOTE: the `catch return` bail-outs below map to `if ... .is_err() {
// return; }`; the trailing one lands in tail position, so `needless_return` is
// allowed to keep the faithful early-return shape.
#[allow(clippy::needless_return)]
fn branchNode(metrics: Metrics, canvas: &mut Canvas, node: BranchNode, thickness: Thickness) {
    // const thick_px = thickness.height(metrics.box_thickness);
    let thick_px = thickness.height(metrics.box_thickness);
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;
    // const float_thick: f64 = @floatFromInt(thick_px);
    let float_thick: f64 = thick_px as f64;

    // Top of horizontal strokes
    // const h_top = (metrics.cell_height -| thick_px) / 2;
    let h_top = metrics.cell_height.saturating_sub(thick_px) / 2;
    // Bottom of horizontal strokes
    // const h_bottom = h_top +| thick_px;
    let h_bottom = h_top.saturating_add(thick_px);
    // Left of vertical strokes
    // const v_left = (metrics.cell_width -| thick_px) / 2;
    let v_left = metrics.cell_width.saturating_sub(thick_px) / 2;
    // Right of vertical strokes
    // const v_right = v_left +| thick_px;
    let v_right = v_left.saturating_add(thick_px);

    // We calculate the center of the circle this way
    // to ensure it aligns with box drawing characters
    // since the lines are sometimes off center to
    // make sure they aren't split between pixels.
    // const cx: f64 = @as(f64, @floatFromInt(v_left)) + float_thick / 2;
    let cx: f64 = v_left as f64 + float_thick / 2.0;
    // const cy: f64 = @as(f64, @floatFromInt(h_top)) + float_thick / 2;
    let cy: f64 = h_top as f64 + float_thick / 2.0;
    // The radius needs to be the smallest distance from the center to an edge.
    // const r: f64 = @min(
    //     @min(cx, cy),
    //     @min(float_width - cx, float_height - cy),
    // );
    let r: f64 = cx.min(cy).min((float_width - cx).min(float_height - cy));

    // PORT NOTE: rustty-z2d's managed `Context` borrows the canvas' surface for
    // its whole lifetime, so it cannot coexist with the direct `canvas.box`
    // pixel writes the reference interleaves between `setLineWidth` and `arc`.
    // Since `setSource`/`setLineWidth` never touch the surface and the four
    // `canvas.box` writes always precede the arc fill/stroke in surface-write
    // order, the box calls are hoisted above `getContext`, preserving the exact
    // sequence of surface mutations. The reference's original order is kept in
    // the comments below.
    //
    // var ctx = canvas.getContext();
    // defer ctx.deinit();
    // ctx.setSource(.{ .opaque_pattern = .{
    //     .pixel = .{ .alpha8 = .{ .a = @intFromEnum(Shade.on) } },
    // } });
    // ctx.setLineWidth(float_thick);

    // These @intFromFloat casts shouldn't ever fail since r can never
    // be greater than cx or cy, so when subtracting it from them the
    // result can never be negative.
    // if (node.up) canvas.box(
    //     @intCast(v_left),
    //     0,
    //     @intCast(v_right),
    //     @intFromFloat(@ceil(cy - r + float_thick / 2)),
    //     .on,
    // );
    if node.up {
        canvas.r#box(
            v_left as i32,
            0,
            v_right as i32,
            (cy - r + float_thick / 2.0).ceil() as i32,
            Color::ON,
        );
    }
    // if (node.right) canvas.box(
    //     @intFromFloat(@floor(cx + r - float_thick / 2)),
    //     @intCast(h_top),
    //     @intCast(metrics.cell_width),
    //     @intCast(h_bottom),
    //     .on,
    // );
    if node.right {
        canvas.r#box(
            (cx + r - float_thick / 2.0).floor() as i32,
            h_top as i32,
            metrics.cell_width as i32,
            h_bottom as i32,
            Color::ON,
        );
    }
    // if (node.down) canvas.box(
    //     @intCast(v_left),
    //     @intFromFloat(@floor(cy + r - float_thick / 2)),
    //     @intCast(v_right),
    //     @intCast(metrics.cell_height),
    //     .on,
    // );
    if node.down {
        canvas.r#box(
            v_left as i32,
            (cy + r - float_thick / 2.0).floor() as i32,
            v_right as i32,
            metrics.cell_height as i32,
            Color::ON,
        );
    }
    // if (node.left) canvas.box(
    //     0,
    //     @intCast(h_top),
    //     @intFromFloat(@ceil(cx - r + float_thick / 2)),
    //     @intCast(h_bottom),
    //     .on,
    // );
    if node.left {
        canvas.r#box(
            0,
            h_top as i32,
            (cx - r + float_thick / 2.0).ceil() as i32,
            h_bottom as i32,
            Color::ON,
        );
    }

    // (see the PORT NOTE above): acquire the managed context now, after the
    // direct box writes, and configure its source/line width.
    let mut ctx = canvas.getContext();
    ctx.set_source(Pattern::OpaquePattern(OpaquePattern {
        pixel: Pixel::Alpha8(Alpha8 { a: Shade::ON.0 }),
    }));
    ctx.set_line_width(float_thick);

    // if (node.filled) {
    if node.filled {
        // ctx.arc(cx, cy, r, 0, std.math.pi * 2) catch return;
        if ctx.arc(cx, cy, r, 0.0, PI * 2.0).is_err() {
            return;
        }
        // ctx.closePath() catch return;
        //
        // PORT NOTE: rustty-z2d's `close_path` is infallible (no allocation), so
        // the reference `catch return` collapses to a plain call.
        ctx.close_path();
        // ctx.fill() catch return;
        if ctx.fill().is_err() {
            return;
        }
    // } else {
    } else {
        // ctx.arc(cx, cy, r - float_thick / 2, 0, std.math.pi * 2) catch return;
        if ctx
            .arc(cx, cy, r - float_thick / 2.0, 0.0, PI * 2.0)
            .is_err()
        {
            return;
        }
        // ctx.closePath() catch return;
        ctx.close_path();
        // ctx.stroke() catch return;
        if ctx.stroke().is_err() {
            // PORT NOTE: `catch return` — swallow the stroke error and bail.
        }
    }
}

// fn fadingLine(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     comptime to: Edge,
//     comptime thickness: Thickness,
// ) void {
fn fadingLine(metrics: Metrics, canvas: &mut Canvas, to: Edge, thickness: Thickness) {
    // const thick_px = thickness.height(metrics.box_thickness);
    let thick_px = thickness.height(metrics.box_thickness);
    // const float_width: f64 = @floatFromInt(metrics.cell_width);
    let float_width: f64 = metrics.cell_width as f64;
    // const float_height: f64 = @floatFromInt(metrics.cell_height);
    let float_height: f64 = metrics.cell_height as f64;

    // Top of horizontal strokes
    // const h_top = (metrics.cell_height -| thick_px) / 2;
    let h_top = metrics.cell_height.saturating_sub(thick_px) / 2;
    // Bottom of horizontal strokes
    // const h_bottom = h_top +| thick_px;
    let h_bottom = h_top.saturating_add(thick_px);
    // Left of vertical strokes
    // const v_left = (metrics.cell_width -| thick_px) / 2;
    let v_left = metrics.cell_width.saturating_sub(thick_px) / 2;
    // Right of vertical strokes
    // const v_right = v_left +| thick_px;
    let v_right = v_left.saturating_add(thick_px);

    // If we're fading to the top or left, we start with 0.0
    // and increment up as we progress, otherwise we start
    // at 255.0 and increment down (negative).
    // var color: f64 = switch (to) {
    //     .top, .left => 0.0,
    //     .bottom, .right => 255.0,
    // };
    let mut color: f64 = match to {
        Edge::Top | Edge::Left => 0.0,
        Edge::Bottom | Edge::Right => 255.0,
    };
    // const inc: f64 = 255.0 / switch (to) {
    //     .top => float_height,
    //     .bottom => -float_height,
    //     .left => float_width,
    //     .right => -float_width,
    // };
    let inc: f64 = 255.0
        / match to {
            Edge::Top => float_height,
            Edge::Bottom => -float_height,
            Edge::Left => float_width,
            Edge::Right => -float_width,
        };

    // switch (to) {
    match to {
        // .top, .bottom => {
        Edge::Top | Edge::Bottom => {
            // for (0..metrics.cell_height) |y| {
            for y in 0..metrics.cell_height {
                // for (v_left..v_right) |x| {
                for x in v_left..v_right {
                    // canvas.pixel(
                    //     @intCast(x),
                    //     @intCast(y),
                    //     @enumFromInt(@as(u8, @intFromFloat(@round(color)))),
                    // );
                    canvas.pixel(x as i32, y as i32, Color(color.round() as u8));
                }
                // color += inc;
                color += inc;
            }
        }
        // .left, .right => {
        Edge::Left | Edge::Right => {
            // for (0..metrics.cell_width) |x| {
            for x in 0..metrics.cell_width {
                // for (h_top..h_bottom) |y| {
                for y in h_top..h_bottom {
                    // canvas.pixel(
                    //     @intCast(x),
                    //     @intCast(y),
                    //     @enumFromInt(@as(u8, @intFromFloat(@round(color)))),
                    // );
                    canvas.pixel(x as i32, y as i32, Color(color.round() as u8));
                }
                // color += inc;
                color += inc;
            }
        }
    }
}
