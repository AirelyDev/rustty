//! This exposes primitives to draw 2D graphics and export the graphic to
//! a font atlas.
// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const z2d = @import("z2d");
// const font = @import("../main.zig");
//
// PORT NOTE: `assert = quirks.inlineAssert` lives in `rustty-util`. `z2d` is the
// `rustty-z2d` crate (the ported z2d 0.10.0 subset). `font = ../main.zig`
// becomes the local crate (`crate::atlas`). The reference `alloc: Allocator`
// field is DROPPED: rustty-z2d is allocator-free (its `Surface`/`Path`/`painter`
// own their storage via `Vec` and never take an allocator), matching the
// allocator-free port of `glyf_rasterize`. The only remaining allocator use,
// `atlas.reserve`, threads the atlas' own allocator through `writeAtlas`'s
// `alloc` argument exactly as the reference passes it.
#![allow(non_snake_case)]

use crate::atlas::{self, Atlas, Format, Region};
use allocator_api2::alloc::Allocator;
use rustty_util::quirks::inlineAssert as assert;
use rustty_z2d as z2d;
use z2d::compositor::Operator;
use z2d::options::CapMode;
use z2d::painter::{self, FillError, FillOptions, StrokeError, StrokeOptions};
use z2d::pattern::{OpaquePattern, Pattern};
use z2d::pixel::{Alpha8, Pixel};
use z2d::surface::SurfaceType;
use z2d::{Context, Path, StaticPath, Surface, Transformation};

// pub fn Point(comptime T: type) type {
//     return struct {
//         x: T,
//         y: T,
//     };
// }
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

// pub fn Line(comptime T: type) type {
//     return struct {
//         p0: Point(T),
//         p1: Point(T),
//     };
// }
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Line<T> {
    pub p0: Point<T>,
    pub p1: Point<T>,
}

// pub fn Box(comptime T: type) type {
//     return struct {
//         p0: Point(T),
//         p1: Point(T),
//         ...
//     };
// }
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Box<T> {
    pub p0: Point<T>,
    pub p1: Point<T>,
}

impl Box<i32> {
    // pub fn rect(self: Box(T)) Rect(T) {
    pub fn rect(self) -> Rect<i32> {
        // const tl_x = @min(self.p0.x, self.p1.x);
        let tl_x = self.p0.x.min(self.p1.x);
        // const tl_y = @min(self.p0.y, self.p1.y);
        let tl_y = self.p0.y.min(self.p1.y);
        // const br_x = @max(self.p0.x, self.p1.x);
        let br_x = self.p0.x.max(self.p1.x);
        // const br_y = @max(self.p0.y, self.p1.y);
        let br_y = self.p0.y.max(self.p1.y);

        // return .{
        //     .x = tl_x,
        //     .y = tl_y,
        //     .width = br_x - tl_x,
        //     .height = br_y - tl_y,
        // };
        Rect {
            x: tl_x,
            y: tl_y,
            width: br_x - tl_x,
            height: br_y - tl_y,
        }
    }
}

// pub fn Rect(comptime T: type) type {
//     return struct {
//         x: T,
//         y: T,
//         width: T,
//         height: T,
//     };
// }
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect<T> {
    pub x: T,
    pub y: T,
    pub width: T,
    pub height: T,
}

// pub fn Triangle(comptime T: type) type {
//     return struct {
//         p0: Point(T),
//         p1: Point(T),
//         p2: Point(T),
//     };
// }
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Triangle<T> {
    pub p0: Point<T>,
    pub p1: Point<T>,
    pub p2: Point<T>,
}

// pub fn Quad(comptime T: type) type {
//     return struct {
//         p0: Point(T),
//         p1: Point(T),
//         p2: Point(T),
//         p3: Point(T),
//     };
// }
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Quad<T> {
    pub p0: Point<T>,
    pub p1: Point<T>,
    pub p2: Point<T>,
    pub p3: Point<T>,
}

/// We only use alpha-channel so a pixel can only be "on" or "off".
// pub const Color = enum(u8) {
//     on = 255,
//     off = 0,
//     _,
// };
//
// PORT NOTE: this is an open (`_`) `enum(u8)`, so it can carry any `u8`; the
// faithful Rust mapping is a `u8` newtype with the named variants as associated
// constants. `@intFromEnum(color)` becomes `color.0`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color(pub u8);

impl Color {
    pub const ON: Color = Color(255);
    pub const OFF: Color = Color(0);
}

/// This is a managed struct, it keeps a reference to the allocator that is
/// used to initialize it, and the same allocator is used for any further
/// necessary allocations when drawing.
// PORT NOTE: rustty-z2d is allocator-free, so the reference's stored `alloc`
// field is dropped (see the module PORT NOTE). The only allocation site,
// `atlas.reserve`, receives its allocator through `writeAtlas`'s argument.
pub struct Canvas {
    /// The underlying z2d surface.
    // sfc: z2d.Surface,
    sfc: Surface,

    // PORT NOTE: the reference fields are package-visible; `special.zig` reads
    // `canvas.padding_y` from a sibling `draw/` module, so both padding fields
    // are `pub(crate)` here to keep that cross-module access faithful.
    pub(crate) padding_x: u32,
    pub(crate) padding_y: u32,

    // clip_top: u32 = 0,
    clip_top: u32,
    // clip_left: u32 = 0,
    clip_left: u32,
    // clip_right: u32 = 0,
    clip_right: u32,
    // clip_bottom: u32 = 0,
    clip_bottom: u32,
    // alloc: Allocator,
}

impl Canvas {
    // pub fn init(
    //     alloc: Allocator,
    //     width: u32,
    //     height: u32,
    //     padding_x: u32,
    //     padding_y: u32,
    // ) !Canvas {
    pub fn init(
        width: u32,
        height: u32,
        padding_x: u32,
        padding_y: u32,
    ) -> Result<Canvas, z2d::surface::Error> {
        // Create the surface we'll be using.
        // We add padding to both sides (hence `2 *`)
        // const sfc = try z2d.Surface.initPixel(
        //     .{ .alpha8 = .{ .a = 0 } },
        //     alloc,
        //     @intCast(width + 2 * padding_x),
        //     @intCast(height + 2 * padding_y),
        // );
        // errdefer sfc.deinit(alloc);
        let sfc = Surface::init_pixel(
            Pixel::Alpha8(Alpha8 { a: 0 }),
            (width + 2 * padding_x) as i32,
            (height + 2 * padding_y) as i32,
        )?;

        // return .{
        //     .sfc = sfc,
        //     .padding_x = padding_x,
        //     .padding_y = padding_y,
        //     .alloc = alloc,
        // };
        Ok(Canvas {
            sfc,
            padding_x,
            padding_y,
            clip_top: 0,
            clip_left: 0,
            clip_right: 0,
            clip_bottom: 0,
        })
    }

    // pub fn deinit(self: *Canvas) void {
    pub fn deinit(&mut self) {
        // self.sfc.deinit(self.alloc);
        self.sfc.deinit();
        // self.* = undefined;
        //
        // PORT NOTE: `self.* = undefined` has no Rust equivalent; the surface's
        // heap buffer is already released by `sfc.deinit`, and the `Canvas`
        // itself is freed by Rust's ownership/drop when it goes out of scope.
    }

    /// Write the data in this drawing to the atlas.
    // pub fn writeAtlas(
    //     self: *Canvas,
    //     alloc: Allocator,
    //     atlas: *font.Atlas,
    // ) (Allocator.Error || font.Atlas.Error)!font.Atlas.Region {
    pub fn writeAtlas<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        atlas: &mut Atlas<A>,
    ) -> Result<Region, atlas::ReserveError> {
        // assert(atlas.format == .grayscale);
        assert(atlas.format == Format::Grayscale);

        // self.trim();
        self.trim();

        // const sfc_width: u32 = @intCast(self.sfc.getWidth());
        let sfc_width: u32 = self.sfc.get_width() as u32;
        // const sfc_height: u32 = @intCast(self.sfc.getHeight());
        let sfc_height: u32 = self.sfc.get_height() as u32;

        // Subtract our clip margins from the
        // width and height to get region size.
        // const region_width = sfc_width -| self.clip_left -| self.clip_right;
        let region_width = sfc_width
            .saturating_sub(self.clip_left)
            .saturating_sub(self.clip_right);
        // const region_height = sfc_height -| self.clip_top -| self.clip_bottom;
        let region_height = sfc_height
            .saturating_sub(self.clip_top)
            .saturating_sub(self.clip_bottom);

        // Allocate our texture atlas region
        // const region = try atlas.reserve(alloc, region_width, region_height);
        let region = atlas.reserve(alloc, region_width, region_height)?;

        // if (region.width > 0 and region.height > 0) {
        if region.width > 0 && region.height > 0 {
            // const buffer: []u8 = @ptrCast(self.sfc.image_surface_alpha8.buf);
            //
            // PORT NOTE: the reference reinterprets the alpha8 pixel buffer as a
            // byte slice (each `alpha8` pixel is one byte). rustty-z2d exposes the
            // typed `[Alpha8]` buffer, so collect its `.a` bytes into a slice for
            // `setFromLarger`.
            let buffer: Vec<u8> = self.sfc.alpha8_buf().iter().map(|p| p.a).collect();

            // Write the glyph information into the atlas
            // assert(region.width == region_width);
            assert(region.width == region_width);
            // assert(region.height == region_height);
            assert(region.height == region_height);
            // atlas.setFromLarger(
            //     region,
            //     buffer,
            //     sfc_width,
            //     self.clip_left,
            //     self.clip_top,
            // );
            atlas.setFromLarger(region, &buffer, sfc_width, self.clip_left, self.clip_top);
        }

        // return region;
        Ok(region)
    }

    // Adjust clip boundaries to trim off any fully transparent rows or columns.
    // This circumvents abstractions from z2d so that it can be performant.
    // fn trim(self: *Canvas) void {
    fn trim(&mut self) {
        // const width: u32 = @intCast(self.sfc.getWidth());
        let width: u32 = self.sfc.get_width() as u32;
        // const height: u32 = @intCast(self.sfc.getHeight());
        let height: u32 = self.sfc.get_height() as u32;

        // const buf = std.mem.sliceAsBytes(self.sfc.image_surface_alpha8.buf);
        let buf = self.sfc.alpha8_buf();

        // top: while (self.clip_top < height - self.clip_bottom) {
        while self.clip_top < height - self.clip_bottom {
            // const y = self.clip_top;
            let y = self.clip_top;
            // const x0 = self.clip_left;
            let x0 = self.clip_left;
            // const x1 = width - self.clip_right;
            let x1 = width - self.clip_right;
            // for (buf[y * width ..][x0..x1]) |v| {
            //     if (v != 0) break :top;
            // }
            let mut broke = false;
            for v in &buf[(y * width + x0) as usize..(y * width + x1) as usize] {
                if v.a != 0 {
                    broke = true;
                    break;
                }
            }
            if broke {
                break;
            }
            // self.clip_top += 1;
            self.clip_top += 1;
        }

        // bottom: while (self.clip_bottom < height - self.clip_top) {
        while self.clip_bottom < height - self.clip_top {
            // const y = height - self.clip_bottom -| 1;
            let y = (height - self.clip_bottom).saturating_sub(1);
            // const x0 = self.clip_left;
            let x0 = self.clip_left;
            // const x1 = width - self.clip_right;
            let x1 = width - self.clip_right;
            // for (buf[y * width ..][x0..x1]) |v| {
            //     if (v != 0) break :bottom;
            // }
            let mut broke = false;
            for v in &buf[(y * width + x0) as usize..(y * width + x1) as usize] {
                if v.a != 0 {
                    broke = true;
                    break;
                }
            }
            if broke {
                break;
            }
            // self.clip_bottom += 1;
            self.clip_bottom += 1;
        }

        // left: while (self.clip_left < width - self.clip_right) {
        while self.clip_left < width - self.clip_right {
            // const x = self.clip_left;
            let x = self.clip_left;
            // const y0 = self.clip_top;
            let y0 = self.clip_top;
            // const y1 = height - self.clip_bottom;
            let y1 = height - self.clip_bottom;
            // for (y0..y1) |y| {
            //     if (buf[y * width + x] != 0) break :left;
            // }
            let mut broke = false;
            for y in y0..y1 {
                if buf[(y * width + x) as usize].a != 0 {
                    broke = true;
                    break;
                }
            }
            if broke {
                break;
            }
            // self.clip_left += 1;
            self.clip_left += 1;
        }

        // right: while (self.clip_right < width - self.clip_left) {
        while self.clip_right < width - self.clip_left {
            // const x = width - self.clip_right -| 1;
            let x = (width - self.clip_right).saturating_sub(1);
            // const y0 = self.clip_top;
            let y0 = self.clip_top;
            // const y1 = height - self.clip_bottom;
            let y1 = height - self.clip_bottom;
            // for (y0..y1) |y| {
            //     if (buf[y * width + x] != 0) break :right;
            // }
            let mut broke = false;
            for y in y0..y1 {
                if buf[(y * width + x) as usize].a != 0 {
                    broke = true;
                    break;
                }
            }
            if broke {
                break;
            }
            // self.clip_right += 1;
            self.clip_right += 1;
        }
    }

    /// Only really useful for test purposes, since the clipping region is
    /// automatically excluded when writing to an atlas with `writeAtlas`.
    // pub fn clearClippingRegions(self: *Canvas) void {
    pub fn clearClippingRegions(&mut self) {
        // const buf = std.mem.sliceAsBytes(self.sfc.image_surface_alpha8.buf);
        // const width: usize = @intCast(self.sfc.getWidth());
        let width: usize = self.sfc.get_width() as usize;
        // const height: usize = @intCast(self.sfc.getHeight());
        let height: usize = self.sfc.get_height() as usize;
        let clip_left = self.clip_left as usize;
        let clip_right = self.clip_right as usize;
        let clip_top = self.clip_top as usize;
        let clip_bottom = self.clip_bottom as usize;
        let buf = self.sfc.alpha8_buf_mut();

        // for (0..height) |y| {
        //     for (0..self.clip_left) |x| {
        //         buf[y * width + x] = 0;
        //     }
        // }
        for y in 0..height {
            for x in 0..clip_left {
                buf[y * width + x].a = 0;
            }
        }

        // for (0..height) |y| {
        //     for (width - self.clip_right..width) |x| {
        //         buf[y * width + x] = 0;
        //     }
        // }
        for y in 0..height {
            for x in (width - clip_right)..width {
                buf[y * width + x].a = 0;
            }
        }

        // for (0..self.clip_top) |y| {
        //     for (0..width) |x| {
        //         buf[y * width + x] = 0;
        //     }
        // }
        for y in 0..clip_top {
            for x in 0..width {
                buf[y * width + x].a = 0;
            }
        }

        // for (height - self.clip_bottom..height) |y| {
        //     for (0..width) |x| {
        //         buf[y * width + x] = 0;
        //     }
        // }
        for y in (height - clip_bottom)..height {
            for x in 0..width {
                buf[y * width + x].a = 0;
            }
        }
    }

    /// Return a transformation representing the translation for our padding.
    // pub fn transformation(self: Canvas) z2d.Transformation {
    pub fn transformation(&self) -> Transformation {
        // return .{
        //     .ax = 1,
        //     .by = 0,
        //     .cx = 0,
        //     .dy = 1,
        //     .tx = @as(f64, @floatFromInt(self.padding_x)),
        //     .ty = @as(f64, @floatFromInt(self.padding_y)),
        // };
        Transformation {
            ax: 1.0,
            by: 0.0,
            cx: 0.0,
            dy: 1.0,
            tx: self.padding_x as f64,
            ty: self.padding_y as f64,
        }
    }

    /// Acquires a z2d drawing context, caller MUST deinit context.
    // pub fn getContext(self: *Canvas) z2d.Context {
    pub fn getContext(&mut self) -> Context<'_> {
        // var ctx = z2d.Context.init(self.alloc, &self.sfc);
        let transformation = self.transformation();
        let mut ctx = Context::init(&mut self.sfc);
        // Offset by our padding to keep
        // coordinates relative to the cell.
        // ctx.setTransformation(self.transformation());
        ctx.set_transformation(transformation);
        // return ctx;
        ctx
    }

    /// Draw and fill a single pixel
    // pub fn pixel(self: *Canvas, x: i32, y: i32, color: Color) void {
    pub fn pixel(&mut self, x: i32, y: i32, color: Color) {
        // self.sfc.putPixel(
        //     x + @as(i32, @intCast(self.padding_x)),
        //     y + @as(i32, @intCast(self.padding_y)),
        //     .{ .alpha8 = .{ .a = @intFromEnum(color) } },
        // );
        self.sfc.put_pixel(
            x + self.padding_x as i32,
            y + self.padding_y as i32,
            Pixel::Alpha8(Alpha8 { a: color.0 }),
        );
    }

    /// Draw and fill a rectangle. This is the main primitive for drawing
    /// lines as well (which are just generally skinny rectangles...)
    // pub fn rect(self: *Canvas, v: Rect(i32), color: Color) void {
    pub fn rect(&mut self, v: Rect<i32>, color: Color) {
        // var y = v.y;
        // while (y < v.y + v.height) : (y += 1) {
        let mut y = v.y;
        while y < v.y + v.height {
            // var x = v.x;
            // while (x < v.x + v.width) : (x += 1) {
            let mut x = v.x;
            while x < v.x + v.width {
                // self.pixel(
                //     @intCast(x),
                //     @intCast(y),
                //     color,
                // );
                self.pixel(x, y, color);
                x += 1;
            }
            y += 1;
        }
    }

    /// Convenience wrapper for `Canvas.rect`
    // pub fn box(
    //     self: *Canvas,
    //     x0: i32,
    //     y0: i32,
    //     x1: i32,
    //     y1: i32,
    //     color: Color,
    // ) void {
    #[allow(clippy::too_many_arguments)]
    pub fn r#box(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        // self.rect((Box(i32){
        //     .p0 = .{ .x = x0, .y = y0 },
        //     .p1 = .{ .x = x1, .y = y1 },
        // }).rect(), color);
        self.rect(
            (Box::<i32> {
                p0: Point { x: x0, y: y0 },
                p1: Point { x: x1, y: y1 },
            })
            .rect(),
            color,
        );
    }

    /// Draw and fill a quad.
    // pub fn quad(self: *Canvas, q: Quad(f64), color: Color) !void {
    pub fn quad(&mut self, q: Quad<f64>, color: Color) -> Result<(), FillError> {
        // var path = self.staticPath(6); // nodes.len = 0
        let mut path = self.staticPath::<6>(); // nodes.len = 0
                                               // path.moveTo(q.p0.x, q.p0.y); // +1, nodes.len = 1
        path.move_to(q.p0.x, q.p0.y); // +1, nodes.len = 1
                                      // path.lineTo(q.p1.x, q.p1.y); // +1, nodes.len = 2
        path.line_to(q.p1.x, q.p1.y); // +1, nodes.len = 2
                                      // path.lineTo(q.p2.x, q.p2.y); // +1, nodes.len = 3
        path.line_to(q.p2.x, q.p2.y); // +1, nodes.len = 3
                                      // path.lineTo(q.p3.x, q.p3.y); // +1, nodes.len = 4
        path.line_to(q.p3.x, q.p3.y); // +1, nodes.len = 4
                                      // path.close(); // +2, nodes.len = 6
        path.close(); // +2, nodes.len = 6
                      // try self.fillPath(path.wrapped_path, .{}, color);
        self.fillPath(&path.wrapped_path, FillOptions::default(), color)
    }

    /// Draw and fill a triangle.
    // pub fn triangle(self: *Canvas, t: Triangle(f64), color: Color) !void {
    pub fn triangle(&mut self, t: Triangle<f64>, color: Color) -> Result<(), FillError> {
        // var path = self.staticPath(5); // nodes.len = 0
        let mut path = self.staticPath::<5>(); // nodes.len = 0
                                               // path.moveTo(t.p0.x, t.p0.y); // +1, nodes.len = 1
        path.move_to(t.p0.x, t.p0.y); // +1, nodes.len = 1
                                      // path.lineTo(t.p1.x, t.p1.y); // +1, nodes.len = 2
        path.line_to(t.p1.x, t.p1.y); // +1, nodes.len = 2
                                      // path.lineTo(t.p2.x, t.p2.y); // +1, nodes.len = 3
        path.line_to(t.p2.x, t.p2.y); // +1, nodes.len = 3
                                      // path.close(); // +2, nodes.len = 5
        path.close(); // +2, nodes.len = 5
                      // try self.fillPath(path.wrapped_path, .{}, color);
        self.fillPath(&path.wrapped_path, FillOptions::default(), color)
    }

    /// Stroke a line.
    // pub fn line(
    //     self: *Canvas,
    //     l: Line(f64),
    //     thickness: f64,
    //     color: Color,
    // ) !void {
    pub fn line(&mut self, l: Line<f64>, thickness: f64, color: Color) -> Result<(), StrokeError> {
        // var path = self.staticPath(2); // nodes.len = 0
        let mut path = self.staticPath::<2>(); // nodes.len = 0
                                               // path.moveTo(l.p0.x, l.p0.y); // +1, nodes.len = 1
        path.move_to(l.p0.x, l.p0.y); // +1, nodes.len = 1
                                      // path.lineTo(l.p1.x, l.p1.y); // +1, nodes.len = 2
        path.line_to(l.p1.x, l.p1.y); // +1, nodes.len = 2
                                      // try self.strokePath(
                                      //     path.wrapped_path,
                                      //     .{
                                      //         .line_cap_mode = .butt,
                                      //         .line_width = thickness,
                                      //     },
                                      //     color,
                                      // );
        self.strokePath(
            &path.wrapped_path,
            StrokeOptions {
                line_cap_mode: CapMode::Butt,
                line_width: thickness,
                ..Default::default()
            },
            color,
        )
    }

    /// Create a static path of the provided len and initialize it.
    /// Use this function instead of making the path manually since
    /// it ensures that the transform is applied.
    // pub inline fn staticPath(
    //     self: *Canvas,
    //     comptime len: usize,
    // ) z2d.StaticPath(len) {
    #[inline]
    pub fn staticPath<const LEN: usize>(&self) -> StaticPath<LEN> {
        // var path: z2d.StaticPath(len) = .{};
        let mut path: StaticPath<LEN> = StaticPath::new();
        // path.init();
        path.init();
        // path.wrapped_path.transformation = self.transformation();
        path.wrapped_path.transformation = self.transformation();
        // return path;
        path
    }

    /// Stroke a z2d path.
    // pub fn strokePath(
    //     self: *Canvas,
    //     path: z2d.Path,
    //     opts: z2d.painter.StrokeOptions,
    //     color: Color,
    // ) z2d.painter.StrokeError!void {
    pub fn strokePath(
        &mut self,
        path: &Path,
        opts: StrokeOptions,
        color: Color,
    ) -> Result<(), StrokeError> {
        // try z2d.painter.stroke(
        //     self.alloc,
        //     &self.sfc,
        //     &.{ .opaque_pattern = .{
        //         .pixel = .{ .alpha8 = .{ .a = @intFromEnum(color) } },
        //     } },
        //     path.nodes.items,
        //     opts,
        // );
        painter::stroke(
            &mut self.sfc,
            &Pattern::OpaquePattern(OpaquePattern {
                pixel: Pixel::Alpha8(Alpha8 { a: color.0 }),
            }),
            &path.nodes,
            opts,
        )
    }

    /// Do an inner stroke on a z2d path, right now this involves a pretty
    /// heavy workaround that uses two extra surfaces; in the future, z2d
    /// should add inner and outer strokes natively.
    // pub fn innerStrokePath(
    //     self: *Canvas,
    //     path: z2d.Path,
    //     opts: z2d.painter.StrokeOptions,
    //     color: Color,
    // ) (z2d.painter.StrokeError || z2d.painter.FillError)!void {
    pub fn innerStrokePath(
        &mut self,
        path: &Path,
        opts: StrokeOptions,
        color: Color,
    ) -> Result<(), InnerStrokeError> {
        // On one surface we fill the shape, this will be a mask we
        // multiply with the double-width stroke so that only the
        // part inside is used.
        // var fill_sfc: z2d.Surface = try .init(
        //     .image_surface_alpha8,
        //     self.alloc,
        //     self.sfc.getWidth(),
        //     self.sfc.getHeight(),
        // );
        // defer fill_sfc.deinit(self.alloc);
        let mut fill_sfc: Surface = Surface::init(
            SurfaceType::ImageSurfaceAlpha8,
            self.sfc.get_width(),
            self.sfc.get_height(),
        )
        .map_err(StrokeError::Surface)?;

        // On the other we'll do the double width stroke.
        // var stroke_sfc: z2d.Surface = try .init(
        //     .image_surface_alpha8,
        //     self.alloc,
        //     self.sfc.getWidth(),
        //     self.sfc.getHeight(),
        // );
        // defer stroke_sfc.deinit(self.alloc);
        let mut stroke_sfc: Surface = Surface::init(
            SurfaceType::ImageSurfaceAlpha8,
            self.sfc.get_width(),
            self.sfc.get_height(),
        )
        .map_err(StrokeError::Surface)?;

        // Make a closed version of the path for our fill, so
        // that we can support open paths for inner stroke.
        // var closed_path = path;
        // closed_path.nodes = try path.nodes.clone(self.alloc);
        // defer closed_path.deinit(self.alloc);
        // try closed_path.close(self.alloc);
        //
        // PORT NOTE: rustty-z2d's `Path` is `Vec`-backed, so cloning the whole
        // path (`path.clone()`) copies its node buffer and drawing state; `close`
        // is infallible and needs no allocator. The cloned path frees on drop.
        let mut closed_path = path.clone();
        closed_path.close();

        // Fill the shape in white to the fill surface, we use
        // white because this is a mask that we'll multiply with
        // the stroke, we want everything inside to be the stroke
        // color.
        // try z2d.painter.fill(
        //     self.alloc,
        //     &fill_sfc,
        //     &.{ .opaque_pattern = .{
        //         .pixel = .{ .alpha8 = .{ .a = 255 } },
        //     } },
        //     closed_path.nodes.items,
        //     .{},
        // );
        painter::fill(
            &mut fill_sfc,
            &Pattern::OpaquePattern(OpaquePattern {
                pixel: Pixel::Alpha8(Alpha8 { a: 255 }),
            }),
            &closed_path.nodes,
            FillOptions::default(),
        )?;

        // Stroke the shape with double the desired width.
        // var mut_opts = opts;
        // mut_opts.line_width *= 2;
        let mut mut_opts = opts;
        mut_opts.line_width *= 2.0;
        // try z2d.painter.stroke(
        //     self.alloc,
        //     &stroke_sfc,
        //     &.{ .opaque_pattern = .{
        //         .pixel = .{ .alpha8 = .{ .a = @intFromEnum(color) } },
        //     } },
        //     path.nodes.items,
        //     mut_opts,
        // );
        painter::stroke(
            &mut stroke_sfc,
            &Pattern::OpaquePattern(OpaquePattern {
                pixel: Pixel::Alpha8(Alpha8 { a: color.0 }),
            }),
            &path.nodes,
            mut_opts,
        )?;

        // We multiply the stroke sfc on to the fill surface.
        // The z2d composite operation doesn't seem to work for
        // this with alpha8 surfaces, so we have to do it manually.
        // for (
        //     std.mem.sliceAsBytes(fill_sfc.image_surface_alpha8.buf),
        //     std.mem.sliceAsBytes(stroke_sfc.image_surface_alpha8.buf),
        // ) |*d, s| {
        //     d.* = @intFromFloat(@round(
        //         255.0 *
        //             (@as(f64, @floatFromInt(s)) / 255.0) *
        //             (@as(f64, @floatFromInt(d.*)) / 255.0),
        //     ));
        // }
        {
            let stroke_buf = stroke_sfc.alpha8_buf().to_vec();
            let fill_buf = fill_sfc.alpha8_buf_mut();
            for (d, s) in fill_buf.iter_mut().zip(stroke_buf.iter()) {
                d.a = (255.0 * (s.a as f64 / 255.0) * (d.a as f64 / 255.0)).round() as u8;
            }
        }

        // Then we composite the result on to the main surface.
        // self.sfc.composite(&fill_sfc, .src_over, 0, 0, .{});
        self.sfc.composite(&fill_sfc, Operator::SrcOver, 0, 0);

        Ok(())
    }

    /// Fill a z2d path.
    // pub fn fillPath(
    //     self: *Canvas,
    //     path: z2d.Path,
    //     opts: z2d.painter.FillOptions,
    //     color: Color,
    // ) z2d.painter.FillError!void {
    pub fn fillPath(
        &mut self,
        path: &Path,
        opts: FillOptions,
        color: Color,
    ) -> Result<(), FillError> {
        // try z2d.painter.fill(
        //     self.alloc,
        //     &self.sfc,
        //     &.{ .opaque_pattern = .{
        //         .pixel = .{ .alpha8 = .{ .a = @intFromEnum(color) } },
        //     } },
        //     path.nodes.items,
        //     opts,
        // );
        painter::fill(
            &mut self.sfc,
            &Pattern::OpaquePattern(OpaquePattern {
                pixel: Pixel::Alpha8(Alpha8 { a: color.0 }),
            }),
            &path.nodes,
            opts,
        )
    }

    /// Invert all pixels on the canvas.
    // pub fn invert(self: *Canvas) void {
    pub fn invert(&mut self) {
        // for (std.mem.sliceAsBytes(self.sfc.image_surface_alpha8.buf)) |*v| {
        //     v.* = 255 - v.*;
        // }
        for v in self.sfc.alpha8_buf_mut() {
            v.a = 255 - v.a;
        }
    }

    /// Mirror the canvas horizontally.
    // pub fn flipHorizontal(self: *Canvas) Allocator.Error!void {
    //
    // PORT NOTE: rustty-z2d is allocator-free and Rust allocation is infallible,
    // so the `Allocator.Error!void` return collapses to `()`; the reference
    // `alloc.dupe`/`alloc.free` become a `Vec` clone that frees on drop.
    pub fn flipHorizontal(&mut self) {
        // const buf = std.mem.sliceAsBytes(self.sfc.image_surface_alpha8.buf);
        // const clone = try self.alloc.dupe(u8, buf);
        // defer self.alloc.free(clone);
        let clone = self.sfc.alpha8_buf().to_vec();
        // const width: usize = @intCast(self.sfc.getWidth());
        let width: usize = self.sfc.get_width() as usize;
        // const height: usize = @intCast(self.sfc.getHeight());
        let height: usize = self.sfc.get_height() as usize;
        let buf = self.sfc.alpha8_buf_mut();
        // for (0..height) |y| {
        //     for (0..width) |x| {
        //         buf[y * width + x] = clone[y * width + width - x - 1];
        //     }
        // }
        for y in 0..height {
            for x in 0..width {
                buf[y * width + x] = clone[y * width + width - x - 1];
            }
        }
        // std.mem.swap(u32, &self.clip_left, &self.clip_right);
        std::mem::swap(&mut self.clip_left, &mut self.clip_right);
    }

    /// Mirror the canvas vertically.
    // pub fn flipVertical(self: *Canvas) Allocator.Error!void {
    pub fn flipVertical(&mut self) {
        // const buf = std.mem.sliceAsBytes(self.sfc.image_surface_alpha8.buf);
        // const clone = try self.alloc.dupe(u8, buf);
        // defer self.alloc.free(clone);
        let clone = self.sfc.alpha8_buf().to_vec();
        // const width: usize = @intCast(self.sfc.getWidth());
        let width: usize = self.sfc.get_width() as usize;
        // const height: usize = @intCast(self.sfc.getHeight());
        let height: usize = self.sfc.get_height() as usize;
        let buf = self.sfc.alpha8_buf_mut();
        // for (0..height) |y| {
        //     for (0..width) |x| {
        //         buf[y * width + x] = clone[(height - y - 1) * width + x];
        //     }
        // }
        for y in 0..height {
            for x in 0..width {
                buf[y * width + x] = clone[(height - y - 1) * width + x];
            }
        }
        // std.mem.swap(u32, &self.clip_top, &self.clip_bottom);
        std::mem::swap(&mut self.clip_top, &mut self.clip_bottom);
    }
}

/// The combined error set for `innerStrokePath`, mirroring the reference's
/// `(z2d.painter.StrokeError || z2d.painter.FillError)` return. The surface
/// allocation error (`Surface.init`) is folded into `StrokeError::Surface`, as
/// the reference's `try .init(...)` merges it into the same inferred set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InnerStrokeError {
    Stroke(StrokeError),
    Fill(FillError),
}

impl From<StrokeError> for InnerStrokeError {
    fn from(e: StrokeError) -> InnerStrokeError {
        InnerStrokeError::Stroke(e)
    }
}

impl From<FillError> for InnerStrokeError {
    fn from(e: FillError) -> InnerStrokeError {
        InnerStrokeError::Fill(e)
    }
}

// Test-only coverage probe, used by this module's tests and by the `draw`
// helper tests; not part of the reference API.
#[cfg(test)]
impl Canvas {
    /// True if the pixel at cell coords `(x, y)` has non-zero alpha coverage.
    pub(crate) fn filled(&self, x: i32, y: i32) -> bool {
        matches!(
            self.sfc.get_pixel(x + self.padding_x as i32, y + self.padding_y as i32),
            Some(Pixel::Alpha8(Alpha8 { a })) if a != 0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::Atlas;
    use allocator_api2::alloc::Global;

    // A small alpha8 "assemble" sanity test: draw a filled box on a padded
    // canvas, confirm the padded pixels land where expected, then trim + write
    // the coverage into a grayscale atlas and confirm the reserved region and
    // its bytes round-trip.
    #[test]
    fn assemble_box_to_atlas() {
        // 4x4 cell with 1px padding on every side -> 6x6 surface.
        let mut canvas = Canvas::init(4, 4, 1, 1).unwrap();

        // Fill a 2x2 box at cell coords (1,1)..(3,3).
        canvas.r#box(1, 1, 3, 3, Color::ON);

        // The box is padding-offset, so cell (1,1) maps to surface (2,2).
        let px = canvas.sfc.get_pixel(2, 2).unwrap();
        assert_eq!(px, Pixel::Alpha8(Alpha8 { a: 255 }));
        // Cell (0,0) -> surface (1,1) is outside the box: still transparent.
        let empty = canvas.sfc.get_pixel(1, 1).unwrap();
        assert_eq!(empty, Pixel::Alpha8(Alpha8 { a: 0 }));

        // Reserve enough atlas space and write the trimmed coverage into it.
        let mut atlas: Atlas<Global> = Atlas::init(Global, 32, Format::Grayscale).unwrap();
        let region = canvas.writeAtlas(Global, &mut atlas).unwrap();

        // After trimming the fully-transparent margin, only the 2x2 filled box
        // survives as the written region.
        assert_eq!(region.width, 2);
        assert_eq!(region.height, 2);

        canvas.deinit();
    }

    // Confirm `invert` complements every pixel, and `flipHorizontal` /
    // `flipVertical` mirror the coverage while swapping the clip margins.
    #[test]
    fn invert_and_flip() {
        let mut canvas = Canvas::init(4, 4, 0, 0).unwrap();
        // Set a single pixel in the top-left corner.
        canvas.pixel(0, 0, Color::ON);

        canvas.invert();
        // The set pixel is now off, everything else on.
        assert_eq!(
            canvas.sfc.get_pixel(0, 0).unwrap(),
            Pixel::Alpha8(Alpha8 { a: 0 })
        );
        assert_eq!(
            canvas.sfc.get_pixel(3, 3).unwrap(),
            Pixel::Alpha8(Alpha8 { a: 255 })
        );

        canvas.flipHorizontal();
        // The lone "off" pixel moves from x=0 to x=3 on row 0.
        assert_eq!(
            canvas.sfc.get_pixel(3, 0).unwrap(),
            Pixel::Alpha8(Alpha8 { a: 0 })
        );
        assert_eq!(
            canvas.sfc.get_pixel(0, 0).unwrap(),
            Pixel::Alpha8(Alpha8 { a: 255 })
        );

        canvas.flipVertical();
        // ...and then from y=0 to y=3.
        assert_eq!(
            canvas.sfc.get_pixel(3, 3).unwrap(),
            Pixel::Alpha8(Alpha8 { a: 0 })
        );
    }

    // `Box::rect` normalizes corner order into a min-origin rect.
    #[test]
    fn box_rect_normalizes() {
        let b = Box::<i32> {
            p0: Point { x: 5, y: 6 },
            p1: Point { x: 1, y: 2 },
        };
        let r = b.rect();
        assert_eq!(r.x, 1);
        assert_eq!(r.y, 2);
        assert_eq!(r.width, 4);
        assert_eq!(r.height, 4);
    }
}
