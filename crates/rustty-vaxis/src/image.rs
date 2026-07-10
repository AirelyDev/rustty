//! Port of `libvaxis` `Image.zig` — the Kitty-graphics `Image` value + its nested
//! shapes. CHUNK 1a ports the STRUCT SHAPES faithfully (all plain value fields — no
//! `zigimg` needed for the shape). The two methods that touch `Window`/geometry
//! (`draw`, `cell_size`) are R5 `unimplemented!()` seams (placeholder-leaf-doctrine)
//! with the verbatim Zig body preserved as a leading block comment.
//!
//! SEAM STATUS (realigned 2026-07-10, VALIDATION_REPORT M5 — the original
//! "later chunk" rationale went stale when the chunk sequence finished at 4b):
//! `Window`/`Screen` are LIVE (Chunk 2), but `draw`/`cell_size` remain deferred
//! because they are UNREACHABLE until the zigimg tier lands — a vaxis `Image` is
//! only constructed through `Vaxis::transmit_image`/`load_image`, themselves the
//! zigimg R5 seams. See DEFERRALS.md "rustty-vaxis Chunks 4a/4b".
//!
//! PORT-NOTE: `Image::Source` byte-slices → `&'a [u8]` (never lossy `String`).
//! `transmit`/`transmitImage` (the consumers of `TRANSMIT_OPENER`/`TransmitFormat`/
//! `TransmitMedium`) live in `Vaxis.zig` (`vaxis.rs`, Chunk 4a — live above the
//! zigimg seams).

// const transmit_opener = "\x1b_Gf=32,i={d},s={d},v={d},m={d};";
pub const TRANSMIT_OPENER: &[u8] = b"\x1b_Gf=32,i={d},s={d},v={d},m={d};";

// pub const Source = union(enum) { path: []const u8, mem: []const u8 };
#[derive(Clone, Copy, Debug)]
pub enum Source<'a> {
    Path(&'a [u8]),
    Mem(&'a [u8]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransmitFormat {
    Rgb,
    Rgba,
    Png,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransmitMedium {
    File,
    TempFile,
    SharedMem,
}

// pub const Placement = struct { img_id: u32, options: Image.DrawOptions };
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub img_id: u32,
    pub options: DrawOptions,
}

// pub const CellSize = struct { rows: u16, cols: u16 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSize {
    pub rows: u16,
    pub cols: u16,
}

/// `pixel_offset` — an offset into the top left cell (Zig anonymous `struct { x, y }`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelOffset {
    pub x: u16,
    pub y: u16,
}

/// `clip_region` — a clip region of the source image (Zig anonymous struct; all
/// fields default `null`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipRegion {
    pub x: Option<u16>,
    pub y: Option<u16>,
    pub width: Option<u16>,
    pub height: Option<u16>,
}

/// Scaling to apply to the Image (Zig anonymous `enum`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scale {
    /// no scaling applied. the image may extend beyond the window
    #[default]
    None,
    /// Stretch / shrink the image to fill the window
    Fill,
    /// Scale the image to fit the window, maintaining aspect ratio
    Fit,
    /// Scale the image to fit the window, only if needed.
    Contain,
}

/// `size` — the size to render the image (Zig anonymous struct; fields default `null`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageSize {
    pub rows: Option<u16>,
    pub cols: Option<u16>,
}

// pub const DrawOptions = struct { ... };
#[derive(Clone, Copy, Debug)]
pub struct DrawOptions {
    /// an offset into the top left cell, in pixels, with where to place the
    /// origin of the image. These must be less than the pixel size of a single
    /// cell
    pub pixel_offset: Option<PixelOffset>,
    /// the vertical stacking order
    /// < 0: Drawn beneath text
    /// < -1_073_741_824: Drawn beneath "default" background cells
    pub z_index: Option<i32>,
    /// A clip region of the source image to draw.
    pub clip_region: Option<ClipRegion>,
    /// Scaling to apply to the Image
    pub scale: Scale,
    /// the size to render the image. Generally you will not need to use this
    /// field, and should prefer to use scale. `draw` will fill in this field with
    /// the correct values if a scale method is applied.
    pub size: Option<ImageSize>,
}

impl Default for DrawOptions {
    fn default() -> Self {
        DrawOptions {
            pixel_offset: None,
            z_index: None,
            clip_region: None,
            scale: Scale::None,
            size: None,
        }
    }
}

// pub const Image = @This();
/// unique identifier for this image. This will be managed by the screen.
/// (plus `width`/`height` in pixels)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Image {
    /// unique identifier for this image. This will be managed by the screen.
    pub id: u32,

    /// width in pixels
    pub width: u16,
    /// height in pixels
    pub height: u16,
}

impl Image {
    // R5 SEAM (placeholder-leaf-doctrine): unreachable until the zigimg tier lands
    // (an `Image` is only constructed via the `transmit_image`/`load_image` zigimg
    // seams — DEFERRALS.md "rustty-vaxis Chunks 4a/4b"). Verbatim Zig body:
    //
    // pub fn draw(self: Image, win: Window, opts: DrawOptions) !void {
    //     var p_opts = opts;
    //     switch (opts.scale) {
    //         .none => {},
    //         .fill => {
    //             p_opts.size = .{
    //                 .rows = win.height,
    //                 .cols = win.width,
    //             };
    //         },
    //         .fit,
    //         .contain,
    //         => contain: {
    //             // cell geometry
    //             const x_pix = win.screen.width_pix;
    //             const y_pix = win.screen.height_pix;
    //             const w = win.screen.width;
    //             const h = win.screen.height;
    //
    //             const pix_per_col = try std.math.divCeil(usize, x_pix, w);
    //             const pix_per_row = try std.math.divCeil(usize, y_pix, h);
    //
    //             const win_width_pix = pix_per_col * win.width;
    //             const win_height_pix = pix_per_row * win.height;
    //
    //             const fit_x: bool = if (win_width_pix >= self.width) true else false;
    //             const fit_y: bool = if (win_height_pix >= self.height) true else false;
    //
    //             // Does the image fit with no scaling?
    //             if (opts.scale == .contain and fit_x and fit_y) break :contain;
    //
    //             // Does the image require vertical scaling?
    //             if (fit_x and !fit_y)
    //                 p_opts.size = .{
    //                     .rows = win.height,
    //                 }
    //
    //                     // Does the image require horizontal scaling?
    //             else if (!fit_x and fit_y)
    //                 p_opts.size = .{
    //                     .cols = win.width,
    //                 }
    //             else if (!fit_x and !fit_y) {
    //                 const diff_x = self.width - win_width_pix;
    //                 const diff_y = self.height - win_height_pix;
    //                 // The width difference is larger than the height difference.
    //                 // Scale by width
    //                 if (diff_x > diff_y)
    //                     p_opts.size = .{
    //                         .cols = win.width,
    //                     }
    //                 else
    //                     // The height difference is larger than the width difference.
    //                     // Scale by height
    //                     p_opts.size = .{
    //                         .rows = win.height,
    //                     };
    //             } else {
    //                 std.debug.assert(opts.scale == .fit);
    //                 std.debug.assert(win_width_pix >= self.width);
    //                 std.debug.assert(win_height_pix >= self.height);
    //
    //                 // Fits in both directions. Find the closer direction
    //                 const diff_x = win_width_pix - self.width;
    //                 const diff_y = win_height_pix - self.height;
    //                 // The width is closer in dimension. Scale by that
    //                 if (diff_x < diff_y)
    //                     p_opts.size = .{
    //                         .cols = win.width,
    //                     }
    //                 else
    //                     p_opts.size = .{
    //                         .rows = win.height,
    //                     };
    //             }
    //         },
    //     }
    //     const p = Placement{
    //         .img_id = self.id,
    //         .options = p_opts,
    //     };
    //     win.writeCell(0, 0, .{ .image = p });
    // }
    pub fn draw(&self, _opts: DrawOptions) {
        unimplemented!("Image::draw — unreachable until the zigimg tier (DEFERRALS.md vaxis 4a/4b)")
    }

    // R5 SEAM (placeholder-leaf-doctrine): unreachable until the zigimg tier lands
    // (see `draw` above — DEFERRALS.md "rustty-vaxis Chunks 4a/4b"). Verbatim Zig body:
    //
    // /// the size of the image, in cells
    // pub fn cellSize(self: Image, win: Window) !CellSize {
    //     // cell geometry
    //     const x_pix = win.screen.width_pix;
    //     const y_pix = win.screen.height_pix;
    //     const w = win.screen.width;
    //     const h = win.screen.height;
    //
    //     const pix_per_col = try std.math.divCeil(u16, x_pix, w);
    //     const pix_per_row = try std.math.divCeil(u16, y_pix, h);
    //
    //     const cell_width = std.math.divCeil(u16, self.width, pix_per_col) catch 0;
    //     const cell_height = std.math.divCeil(u16, self.height, pix_per_row) catch 0;
    //     return .{
    //         .rows = cell_height,
    //         .cols = cell_width,
    //     };
    // }
    /// the size of the image, in cells
    pub fn cell_size(&self) -> CellSize {
        unimplemented!(
            "Image::cellSize — unreachable until the zigimg tier (DEFERRALS.md vaxis 4a/4b)"
        )
    }
}
