//! This file contains glyph drawing functions for all of the
//! non-Unicode sprite glyphs, such as cursors and underlines.
//!
//! The naming convention in this file differs from the usual
//! because the draw functions for special sprites are found by
//! having names that exactly match the enum fields in Sprite.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const font = @import("../../main.zig");
// const Sprite = font.sprite.Sprite;
//
// PORT NOTE: `font = ../../main.zig` becomes the local crate
// (`crate::metrics::Metrics`, `crate::sprite::Canvas` / `Color` / `Rect`).
// `Allocator` and `Sprite` are unused in this module (the reference imports
// `Sprite` only for the doc note above), so they are preserved as reference
// comments rather than unused bindings. The dotted/curly draws use the managed
// z2d `Context` (default opaque-black source → full alpha on the alpha8
// surface) plus `CapMode`, and propagate the z2d `path`/`FillError`/`StrokeError`
// sets via the module-local combined error enums below.
use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color, Rect};
use rustty_z2d::options::CapMode;
use rustty_z2d::painter::{FillError, StrokeError};
use rustty_z2d::path;
use std::f64::consts::{FRAC_1_SQRT_2, PI, TAU};

/// The combined error set for `underline_dotted`, mirroring the reference's
/// inferred `!void` union of `ctx.arc` (path build) and `ctx.fill` errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DottedError {
    Path(path::Error),
    Fill(FillError),
}

impl From<path::Error> for DottedError {
    fn from(e: path::Error) -> DottedError {
        DottedError::Path(e)
    }
}

impl From<FillError> for DottedError {
    fn from(e: FillError) -> DottedError {
        DottedError::Fill(e)
    }
}

/// The combined error set for `underline_curly`, mirroring the reference's
/// inferred `!void` union of `ctx.curveTo` (path build) and `ctx.stroke` errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurlyError {
    Path(path::Error),
    Stroke(StrokeError),
}

impl From<path::Error> for CurlyError {
    fn from(e: path::Error) -> CurlyError {
        CurlyError::Path(e)
    }
}

impl From<StrokeError> for CurlyError {
    fn from(e: StrokeError) -> CurlyError {
        CurlyError::Stroke(e)
    }
}

// pub fn underline(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn underline(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;

    // We can go beyond the height of the cell a bit, but
    // we want to be sure never to exceed the height of the
    // canvas, which extends a quarter cell below the cell
    // height.
    // const y = @min(
    //     metrics.underline_position,
    //     height +| canvas.padding_y -| metrics.underline_thickness,
    // );
    let y = metrics.underline_position.min(
        height
            .saturating_add(canvas.padding_y)
            .saturating_sub(metrics.underline_thickness),
    );

    // canvas.rect(.{
    //     .x = 0,
    //     .y = @intCast(y),
    //     .width = @intCast(width),
    //     .height = @intCast(metrics.underline_thickness),
    // }, .on);
    canvas.rect(
        Rect {
            x: 0,
            y: y as i32,
            width: width as i32,
            height: metrics.underline_thickness as i32,
        },
        Color::ON,
    );

    Ok(())
}

// pub fn underline_double(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn underline_double(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;

    // We can go beyond the height of the cell a bit, but
    // we want to be sure never to exceed the height of the
    // canvas, which extends a quarter cell below the cell
    // height.
    // const y = @min(
    //     metrics.underline_position,
    //     height +| canvas.padding_y -| 2 * metrics.underline_thickness,
    // );
    let y = metrics.underline_position.min(
        height
            .saturating_add(canvas.padding_y)
            .saturating_sub(2 * metrics.underline_thickness),
    );

    // We place one underline above the underline position, and one below
    // by one thickness, creating a "negative" underline where the single
    // underline would be placed.
    // canvas.rect(.{
    //     .x = 0,
    //     .y = @intCast(y -| metrics.underline_thickness),
    //     .width = @intCast(width),
    //     .height = @intCast(metrics.underline_thickness),
    // }, .on);
    canvas.rect(
        Rect {
            x: 0,
            y: y.saturating_sub(metrics.underline_thickness) as i32,
            width: width as i32,
            height: metrics.underline_thickness as i32,
        },
        Color::ON,
    );
    // canvas.rect(.{
    //     .x = 0,
    //     .y = @intCast(y +| metrics.underline_thickness),
    //     .width = @intCast(width),
    //     .height = @intCast(metrics.underline_thickness),
    // }, .on);
    canvas.rect(
        Rect {
            x: 0,
            y: y.saturating_add(metrics.underline_thickness) as i32,
            width: width as i32,
            height: metrics.underline_thickness as i32,
        },
        Color::ON,
    );

    Ok(())
}

// pub fn underline_dotted(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn underline_dotted(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), DottedError> {
    // _ = cp;
    let _ = cp;

    // PORT NOTE: `canvas.padding_y` is captured up-front because the managed
    // `ctx` holds a mutable borrow of the canvas for its whole lifetime, so the
    // field cannot be read while `ctx` is live (the reference reads it mid-body).
    // Reordering is safe: it is a plain field read with no surface effect.
    let padding_y = canvas.padding_y;

    // var ctx = canvas.getContext();
    // defer ctx.deinit();
    //
    // PORT NOTE: `ctx` releases its managed path on drop, so `defer ctx.deinit()`
    // maps to letting `ctx` fall out of scope.
    let mut ctx = canvas.getContext();

    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;
    // const float_pos: f64 = @floatFromInt(metrics.underline_position);
    let float_pos: f64 = metrics.underline_position as f64;
    // const float_thick: f64 = @floatFromInt(metrics.underline_thickness);
    let float_thick: f64 = metrics.underline_thickness as f64;

    // The diameter will be sqrt2 * the usual underline thickness
    // since otherwise dotted underlines look somewhat anemic.
    // const radius = std.math.sqrt1_2 * float_thick;
    let radius = FRAC_1_SQRT_2 * float_thick;

    // We can go beyond the height of the cell a bit, but
    // we want to be sure never to exceed the height of the
    // canvas, which extends a quarter cell below the cell
    // height.
    // const padding: f64 = @floatFromInt(canvas.padding_y);
    let padding: f64 = padding_y as f64;
    // const y = @min(
    //     // The center of the underline stem.
    //     float_pos + 0.5 * float_thick,
    //     // The lowest we can go on the canvas and not get clipped.
    //     float_height + padding - @ceil(radius),
    // );
    let y = (
        // The center of the underline stem.
        float_pos + 0.5 * float_thick
    )
        .min(
            // The lowest we can go on the canvas and not get clipped.
            float_height + padding - radius.ceil(),
        );

    // const dot_count: f64 = @max(
    //     @min(
    //         // We should try to have enough dots that the
    //         // space between them matches their diameter.
    //         @ceil(float_width / (4 * radius)),
    //         // And not enough that the space between
    //         // each dot is less than their radius.
    //         @floor(float_width / (3 * radius)),
    //         // And definitely not enough that the space
    //         // between them is less than a single pixel.
    //         @floor(float_width / (2 * radius + 1)),
    //     ),
    //     // And we must have at least one dot per cell.
    //     1.0,
    // );
    let dot_count: f64 = (
        // We should try to have enough dots that the
        // space between them matches their diameter.
        float_width / (4.0 * radius)
    )
        .ceil()
        .min(
            // And not enough that the space between
            // each dot is less than their radius.
            (float_width / (3.0 * radius)).floor(),
        )
        .min(
            // And definitely not enough that the space
            // between them is less than a single pixel.
            (float_width / (2.0 * radius + 1.0)).floor(),
        )
        // And we must have at least one dot per cell.
        .max(1.0);

    // What we essentially do is divide the cell in to
    // dot_count areas with a dot centered in each one.
    // var x: f64 = (float_width / dot_count) / 2;
    let mut x: f64 = (float_width / dot_count) / 2.0;
    // for (0..@as(usize, @intFromFloat(dot_count))) |_| {
    for _ in 0..(dot_count as usize) {
        // try ctx.arc(x, y, radius, 0.0, std.math.tau);
        ctx.arc(x, y, radius, 0.0, TAU)?;
        // try ctx.closePath();
        //
        // PORT NOTE: rustty-z2d's `close_path` is infallible, so the reference
        // `try` collapses to a plain call.
        ctx.close_path();
        // x += float_width / dot_count;
        x += float_width / dot_count;
    }

    // try ctx.fill();
    ctx.fill()?;

    Ok(())
}

// pub fn underline_dashed(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn underline_dashed(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;

    // We can go beyond the height of the cell a bit, but
    // we want to be sure never to exceed the height of the
    // canvas, which extends a quarter cell below the cell
    // height.
    // const y = @min(
    //     metrics.underline_position,
    //     height +| canvas.padding_y -| metrics.underline_thickness,
    // );
    let y = metrics.underline_position.min(
        height
            .saturating_add(canvas.padding_y)
            .saturating_sub(metrics.underline_thickness),
    );

    // const dash_width = width / 3 + 1;
    let dash_width = width / 3 + 1;
    // const dash_count = (width / dash_width) + 1;
    let dash_count = (width / dash_width) + 1;
    // var i: u32 = 0;
    let mut i: u32 = 0;
    // while (i < dash_count) : (i += 2) {
    while i < dash_count {
        // const x = i * dash_width;
        let x = i * dash_width;
        // canvas.rect(.{
        //     .x = @intCast(x),
        //     .y = @intCast(y),
        //     .width = @intCast(dash_width),
        //     .height = @intCast(metrics.underline_thickness),
        // }, .on);
        canvas.rect(
            Rect {
                x: x as i32,
                y: y as i32,
                width: dash_width as i32,
                height: metrics.underline_thickness as i32,
            },
            Color::ON,
        );
        i += 2;
    }

    Ok(())
}

// pub fn underline_curly(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn underline_curly(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), CurlyError> {
    // _ = cp;
    let _ = cp;

    // PORT NOTE: `canvas.padding_y` is captured up-front (see the same note in
    // `underline_dotted`): the managed `ctx` borrows the canvas for its lifetime.
    let padding_y = canvas.padding_y;

    // var ctx = canvas.getContext();
    // defer ctx.deinit();
    let mut ctx = canvas.getContext();

    // const float_width: f64 = @floatFromInt(width);
    let float_width: f64 = width as f64;
    // const float_height: f64 = @floatFromInt(height);
    let float_height: f64 = height as f64;
    // const float_pos: f64 = @floatFromInt(metrics.underline_position);
    let float_pos: f64 = metrics.underline_position as f64;

    // Because of we way we draw the undercurl, we end up making it around 1px
    // thicker than it should be, to fix this we just reduce the thickness by 1.
    //
    // We use a minimum thickness of 0.414 because this empirically produces
    // the nicest undercurls at 1px underline thickness; thinner tends to look
    // too thin compared to straight underlines and has artefacting.
    // ctx.line_width = @floatFromInt(metrics.underline_thickness);
    ctx.line_width = metrics.underline_thickness as f64;

    // Rounded caps, adjacent underlines will have these overlap and so not be
    // visible, but it makes the ends look cleaner.
    // ctx.line_cap_mode = .round;
    ctx.line_cap_mode = CapMode::Round;

    // Empirically this looks good.
    // const amplitude = float_width / std.math.pi;
    let amplitude = float_width / PI;

    // Make sure we don't exceed the drawable area. This can still be outside
    // of the cell by some amount (one quarter of the height), but we don't
    // want underlines to disappear for fonts with bad metadata or when users
    // set their underline position way too low.
    // const padding: f64 = @floatFromInt(canvas.padding_y);
    let padding: f64 = padding_y as f64;
    // const top: f64 = @min(
    //     float_pos,
    //     // The lowest we can draw this and not get clipped.
    //     float_height + padding - amplitude - ctx.line_width,
    // );
    let top: f64 = float_pos.min(
        // The lowest we can draw this and not get clipped.
        float_height + padding - amplitude - ctx.line_width,
    );
    // const bottom = top + amplitude;
    let bottom = top + amplitude;

    // Curvature multiplier.
    // To my eye, 0.4 creates a nice smooth wiggle.
    // const r = 0.4;
    let r = 0.4;

    // const center = 0.5 * float_width;
    let center = 0.5 * float_width;

    // We create a single cycle of a wave that peaks at the center of the cell.
    // try ctx.moveTo(0, bottom);
    //
    // PORT NOTE: rustty-z2d's `move_to` is infallible, so the reference `try`
    // collapses to a plain call.
    ctx.move_to(0.0, bottom);
    // try ctx.curveTo(
    //     center * r,
    //     bottom,
    //     center - center * r,
    //     top,
    //     center,
    //     top,
    // );
    ctx.curve_to(center * r, bottom, center - center * r, top, center, top)?;
    // try ctx.curveTo(
    //     center + center * r,
    //     top,
    //     float_width - center * r,
    //     bottom,
    //     float_width,
    //     bottom,
    // );
    ctx.curve_to(
        center + center * r,
        top,
        float_width - center * r,
        bottom,
        float_width,
        bottom,
    )?;
    // try ctx.stroke();
    ctx.stroke()?;

    Ok(())
}

// pub fn strikethrough(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn strikethrough(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;
    // _ = height;
    let _ = height;

    // canvas.rect(.{
    //     .x = 0,
    //     .y = @intCast(metrics.strikethrough_position),
    //     .width = @intCast(width),
    //     .height = @intCast(metrics.strikethrough_thickness),
    // }, .on);
    canvas.rect(
        Rect {
            x: 0,
            y: metrics.strikethrough_position as i32,
            width: width as i32,
            height: metrics.strikethrough_thickness as i32,
        },
        Color::ON,
    );

    Ok(())
}

// pub fn overline(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn overline(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;
    // _ = height;
    let _ = height;

    // We can go beyond the top of the cell a bit, but we
    // want to be sure never to exceed the height of the
    // canvas, which extends a quarter cell above the top
    // of the cell.
    // const y = @max(
    //     metrics.overline_position,
    //     -@as(i32, @intCast(canvas.padding_y)),
    // );
    let y = metrics.overline_position.max(-(canvas.padding_y as i32));

    // canvas.rect(.{
    //     .x = 0,
    //     .y = y,
    //     .width = @intCast(width),
    //     .height = @intCast(metrics.overline_thickness),
    // }, .on);
    canvas.rect(
        Rect {
            x: 0,
            y,
            width: width as i32,
            height: metrics.overline_thickness as i32,
        },
        Color::ON,
    );

    Ok(())
}

// pub fn cursor_rect(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn cursor_rect(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;
    // _ = metrics;
    let _ = metrics;

    // canvas.rect(.{
    //     .x = 0,
    //     .y = 0,
    //     .width = @intCast(width),
    //     .height = @intCast(height),
    // }, .on);
    canvas.rect(
        Rect {
            x: 0,
            y: 0,
            width: width as i32,
            height: height as i32,
        },
        Color::ON,
    );

    Ok(())
}

// pub fn cursor_hollow_rect(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn cursor_hollow_rect(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;

    // We fill the entire rect and then hollow out the inside, this isn't very
    // efficient but it doesn't need to be and it's the easiest way to write it.
    // canvas.rect(.{
    //     .x = 0,
    //     .y = 0,
    //     .width = @intCast(width),
    //     .height = @intCast(height),
    // }, .on);
    canvas.rect(
        Rect {
            x: 0,
            y: 0,
            width: width as i32,
            height: height as i32,
        },
        Color::ON,
    );
    // canvas.rect(.{
    //     .x = @intCast(metrics.cursor_thickness),
    //     .y = @intCast(metrics.cursor_thickness),
    //     .width = @intCast(width -| metrics.cursor_thickness * 2),
    //     .height = @intCast(height -| metrics.cursor_thickness * 2),
    // }, .off);
    canvas.rect(
        Rect {
            x: metrics.cursor_thickness as i32,
            y: metrics.cursor_thickness as i32,
            width: width.saturating_sub(metrics.cursor_thickness * 2) as i32,
            height: height.saturating_sub(metrics.cursor_thickness * 2) as i32,
        },
        Color::OFF,
    );

    Ok(())
}

// pub fn cursor_bar(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
//
// PORT NOTE: the reference computes `(cursor_thickness + 1) / 2` verbatim; that
// integer round-up is kept as-written (rather than `div_ceil`) to mirror the
// source expression, so `manual_div_ceil` is allowed here.
#[allow(clippy::manual_div_ceil)]
pub fn cursor_bar(
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

    // We place the bar cursor half of its thickness over the left edge of the
    // cell, so that it sits centered between characters, not biased to a side.
    //
    // We round up (add 1 before dividing by 2) because, empirically, having a
    // 1px cursor shifted left a pixel looks better than having it not shifted.
    // canvas.rect(.{
    //     .x = -@as(i32, @intCast((metrics.cursor_thickness + 1) / 2)),
    //     .y = 0,
    //     .width = @intCast(metrics.cursor_thickness),
    //     .height = @intCast(height),
    // }, .on);
    canvas.rect(
        Rect {
            x: -(((metrics.cursor_thickness + 1) / 2) as i32),
            y: 0,
            width: metrics.cursor_thickness as i32,
            height: height as i32,
        },
        Color::ON,
    );

    Ok(())
}

// pub fn cursor_underline(
//     cp: u32,
//     canvas: *font.sprite.Canvas,
//     width: u32,
//     height: u32,
//     metrics: font.Metrics,
// ) !void {
pub fn cursor_underline(
    cp: u32,
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: Metrics,
) -> Result<(), std::convert::Infallible> {
    // _ = cp;
    let _ = cp;

    // We can go beyond the height of the cell a bit, but
    // we want to be sure never to exceed the height of the
    // canvas, which extends a quarter cell below the cell
    // height.
    // const y = @min(
    //     metrics.underline_position,
    //     height +| canvas.padding_y -| metrics.underline_thickness,
    // );
    let y = metrics.underline_position.min(
        height
            .saturating_add(canvas.padding_y)
            .saturating_sub(metrics.underline_thickness),
    );

    // canvas.rect(.{
    //     .x = 0,
    //     .y = @intCast(y),
    //     .width = @intCast(width),
    //     .height = @intCast(metrics.cursor_thickness),
    // }, .on);
    canvas.rect(
        Rect {
            x: 0,
            y: y as i32,
            width: width as i32,
            height: metrics.cursor_thickness as i32,
        },
        Color::ON,
    );

    Ok(())
}
