//! Port of `libvaxis` `Image.zig` — the Kitty-graphics `Image` value + its nested
//! shapes. Chunk 1a ported the STRUCT SHAPES (all plain value fields — no `zigimg`
//! needed for the shape).
//!
//! SEAM STATUS: **NONE. This file is fully LIVE as of WS12 (T10/T11).** `draw` and
//! `cell_size` are implemented; the verbatim Zig bodies are retained above each as
//! reference comments, interleaved with the Rust, per house style.
//!
//! History: both methods were `unimplemented!()` R5 seams on the rationale that they
//! were UNREACHABLE — a vaxis `Image` is only constructed through
//! `Vaxis::transmit_image`/`load_image`, which were themselves zigimg seams. WS12 filled
//! those two (over `rustty-zigimg`, Decision 2 / Option Z-C), and in doing so exposed
//! that `draw`/`cell_size` never needed `zigimg` AT ALL: they are pure `Window`/`Screen`
//! geometry and could have been filled at any point. Both had also silently DROPPED
//! their `win: Window` parameter, which is what made them un-callable; the parameter is
//! restored. See `.port/phase-2/DEFERRALS.md` "rustty-vaxis Chunks 4a/4b" (now RESOLVED).
//!
//! PORT NOTE: `Image::Source` byte-slices → `&'a [u8]` (never lossy `String`).
//! `transmit`/`transmitImage` (the consumers of `TRANSMIT_OPENER`/`TransmitFormat`/
//! `TransmitMedium`) live in `Vaxis.zig` (`vaxis.rs`, Chunk 4a — live above the
//! zigimg seams).

use crate::cell::Cell;
use crate::window::Window;

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
    // Reference body (`libvaxis` 0.5.1 `src/Image.zig:87-170`), retained verbatim above
    // the port below, which follows it line by line:
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
    /// Place this image into `win` at (0,0), filling in `DrawOptions::size` per the
    /// requested [`Scale`].
    ///
    /// LIVE since WS12 T10. **No `zigimg` is involved** — this is pure `Window`/`Screen`
    /// geometry and could have been filled at any point; the seam existed only because an
    /// `Image` was unreachable while the two transmit paths were seamed.
    ///
    /// PORT NOTE — the `win` parameter is RESTORED. The seam's signature was
    /// `(&self, opts)`, having silently dropped `win: Window`, which made it impossible to
    /// call meaningfully. `Window<'_>` is `Copy` (it holds `screen: *mut Screen<'a>`), so
    /// taking it by value matches the reference's `win: Window`.
    ///
    /// ALIASING: reading `win.screen.width_pix` etc. is an `unsafe` deref of that raw
    /// pointer. The reads are kept NARROW — four scalar loads through a shared reborrow,
    /// with no whole-`&mut Screen` formed and nothing else live across them — per the
    /// crate's aliasing discipline (`vxfw/app.rs:8-17`). `win.write_cell` then does its own
    /// narrow transient reborrow (`window.rs:258-266`).
    ///
    /// ERRORS: the reference's two `std.math.divCeil` calls are `try`, so a zero divisor
    /// propagates. `win.screen.width`/`height` are the divisors, so a 0-column or 0-row
    /// screen yields [`DrawError::DivisionByZero`] rather than a silent 0 — see
    /// `cell_size`, which has the same `try` on the same two divisions.
    pub fn draw(&self, win: Window<'_>, opts: DrawOptions) -> Result<(), DrawError> {
        // var p_opts = opts;
        let mut p_opts = opts;

        // switch (opts.scale) {
        match opts.scale {
            // .none => {},
            //
            // Deliberately leaves `p_opts.size` exactly as the caller passed it (normally
            // `None`): "no scaling applied, the image may extend beyond the window".
            Scale::None => {}

            // .fill => { p_opts.size = .{ .rows = win.height, .cols = win.width }; }
            Scale::Fill => {
                p_opts.size = Some(ImageSize {
                    rows: Some(win.height),
                    cols: Some(win.width),
                });
            }

            // .fit, .contain => contain: { ... }
            Scale::Fit | Scale::Contain => {
                // cell geometry
                // const x_pix = win.screen.width_pix;  const y_pix = win.screen.height_pix;
                // const w = win.screen.width;          const h = win.screen.height;
                //
                // SAFETY: narrow shared reborrow of the `*mut Screen` for four scalar
                // reads; no `&mut Screen` is formed and no other alias is live.
                let (x_pix, y_pix, w, h) = unsafe {
                    let screen = &*win.screen;
                    (
                        screen.width_pix,
                        screen.height_pix,
                        screen.width,
                        screen.height,
                    )
                };

                // const pix_per_col = try std.math.divCeil(usize, x_pix, w);
                // const pix_per_row = try std.math.divCeil(usize, y_pix, h);
                //
                // `usize` in the reference (unlike cellSize, which uses u16 — the asymmetry
                // is upstream's and is preserved).
                let pix_per_col = div_ceil_usize(x_pix as usize, w as usize)?;
                let pix_per_row = div_ceil_usize(y_pix as usize, h as usize)?;

                // const win_width_pix = pix_per_col * win.width;
                // const win_height_pix = pix_per_row * win.height;
                let win_width_pix = pix_per_col * win.width as usize;
                let win_height_pix = pix_per_row * win.height as usize;

                // const fit_x: bool = if (win_width_pix >= self.width) true else false;
                // const fit_y: bool = if (win_height_pix >= self.height) true else false;
                let fit_x = win_width_pix >= self.width as usize;
                let fit_y = win_height_pix >= self.height as usize;

                // Does the image fit with no scaling?
                // if (opts.scale == .contain and fit_x and fit_y) break :contain;
                if opts.scale == Scale::Contain && fit_x && fit_y {
                    // `break :contain` leaves p_opts.size untouched — the early exit, and
                    // the whole point of `.contain` vs `.fit`.
                } else if fit_x && !fit_y {
                    // Does the image require vertical scaling?
                    // p_opts.size = .{ .rows = win.height }
                    //
                    // NOTE only `rows` is set; `cols` stays null so the terminal preserves
                    // the aspect ratio.
                    p_opts.size = Some(ImageSize {
                        rows: Some(win.height),
                        cols: None,
                    });
                } else if !fit_x && fit_y {
                    // Does the image require horizontal scaling?
                    // p_opts.size = .{ .cols = win.width }
                    p_opts.size = Some(ImageSize {
                        rows: None,
                        cols: Some(win.width),
                    });
                } else if !fit_x && !fit_y {
                    // const diff_x = self.width - win_width_pix;
                    // const diff_y = self.height - win_height_pix;
                    //
                    // Both are non-negative here: !fit_x means win_width_pix < self.width
                    // (and likewise for y), so the reference's unsigned subtraction cannot
                    // underflow. Kept as plain subtraction to match, with the reasoning
                    // recorded rather than papered over with a saturating op.
                    let diff_x = self.width as usize - win_width_pix;
                    let diff_y = self.height as usize - win_height_pix;
                    // The width difference is larger than the height difference.
                    // Scale by width
                    if diff_x > diff_y {
                        p_opts.size = Some(ImageSize {
                            rows: None,
                            cols: Some(win.width),
                        });
                    } else {
                        // The height difference is larger than the width difference.
                        // Scale by height
                        p_opts.size = Some(ImageSize {
                            rows: Some(win.height),
                            cols: None,
                        });
                    }
                } else {
                    // std.debug.assert(opts.scale == .fit);
                    // std.debug.assert(win_width_pix >= self.width);
                    // std.debug.assert(win_height_pix >= self.height);
                    //
                    // Reached only when fit_x && fit_y && scale == .fit (the .contain case
                    // took the early break above).
                    debug_assert!(opts.scale == Scale::Fit);
                    debug_assert!(win_width_pix >= self.width as usize);
                    debug_assert!(win_height_pix >= self.height as usize);

                    // Fits in both directions. Find the closer direction
                    // const diff_x = win_width_pix - self.width;
                    // const diff_y = win_height_pix - self.height;
                    let diff_x = win_width_pix - self.width as usize;
                    let diff_y = win_height_pix - self.height as usize;
                    // The width is closer in dimension. Scale by that
                    if diff_x < diff_y {
                        p_opts.size = Some(ImageSize {
                            rows: None,
                            cols: Some(win.width),
                        });
                    } else {
                        p_opts.size = Some(ImageSize {
                            rows: Some(win.height),
                            cols: None,
                        });
                    }
                }
            }
        }

        // const p = Placement{ .img_id = self.id, .options = p_opts };
        let p = Placement {
            img_id: self.id,
            options: p_opts,
        };

        // win.writeCell(0, 0, .{ .image = p });
        //
        // A Zig anonymous struct literal with only `.image` set means every other Cell
        // field takes its declaration default — i.e. `Cell::default()` with `image` set.
        win.write_cell(
            0,
            0,
            Cell {
                image: Some(p),
                ..Default::default()
            },
        );

        Ok(())
    }

    // Reference body (`libvaxis` 0.5.1 `src/Image.zig:171-188`), retained verbatim:
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
    ///
    /// LIVE since WS12 T11. Pure geometry; no `zigimg`. `win` restored, as in `draw`.
    ///
    /// PORT NOTE — **the error asymmetry is upstream's and is preserved exactly.** The
    /// first two divisions are `try` (a zero divisor PROPAGATES as an error); the last two
    /// are `catch 0` (a zero divisor yields 0). So a 0-column screen is an error, while a
    /// `pix_per_col` that came out 0 gives `cols == 0`. Collapsing all four to one
    /// behaviour would be a silent behavioural change, which is why
    /// `image_cell_size_zero_pix_per_col_yields_zero` and
    /// `image_cell_size_zero_screen_cells_errors` are separate tests.
    ///
    /// PORT NOTE — **rows come from height and cols from width** (`.rows = cell_height`,
    /// `.cols = cell_width`). Easy to transpose; pinned by
    /// `image_cell_size_rows_from_height` over a deliberately non-square image.
    pub fn cell_size(&self, win: Window<'_>) -> Result<CellSize, DrawError> {
        // cell geometry
        // const x_pix = win.screen.width_pix;  const y_pix = win.screen.height_pix;
        // const w = win.screen.width;          const h = win.screen.height;
        //
        // SAFETY: as in `draw` — a narrow shared reborrow for four scalar reads, no
        // `&mut Screen` formed, no other alias live.
        let (x_pix, y_pix, w, h) = unsafe {
            let screen = &*win.screen;
            (
                screen.width_pix,
                screen.height_pix,
                screen.width,
                screen.height,
            )
        };

        // const pix_per_col = try std.math.divCeil(u16, x_pix, w);
        // const pix_per_row = try std.math.divCeil(u16, y_pix, h);
        //
        // `u16` here, where `draw` uses `usize`. Upstream's asymmetry, preserved.
        let pix_per_col = div_ceil_u16(x_pix, w)?;
        let pix_per_row = div_ceil_u16(y_pix, h)?;

        // const cell_width  = std.math.divCeil(u16, self.width,  pix_per_col) catch 0;
        // const cell_height = std.math.divCeil(u16, self.height, pix_per_row) catch 0;
        let cell_width = div_ceil_u16(self.width, pix_per_col).unwrap_or(0);
        let cell_height = div_ceil_u16(self.height, pix_per_row).unwrap_or(0);

        // return .{ .rows = cell_height, .cols = cell_width };
        Ok(CellSize {
            rows: cell_height,
            cols: cell_width,
        })
    }
}

/// Error surface for the geometry methods — the `!` in `Image.draw`/`Image.cellSize`.
///
/// The reference's error union comes from `std.math.divCeil`, whose only failure mode for
/// unsigned operands is `error.DivisionByZero`. Named rather than folded into a silent 0,
/// because the reference's `try` genuinely propagates it while its `catch 0` sites
/// genuinely swallow it — two behaviours that must stay distinguishable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawError {
    /// `std.math.divCeil` with a zero divisor — `win.screen.width` or
    /// `win.screen.height` was 0.
    DivisionByZero,
}

/// `std.math.divCeil(u16, a, b)` — ceiling division, erroring on a zero divisor.
///
/// Not `u16::div_ceil`, because that PANICS on a zero divisor where the reference returns
/// an error the callers deliberately handle two different ways.
fn div_ceil_u16(a: u16, b: u16) -> Result<u16, DrawError> {
    if b == 0 {
        return Err(DrawError::DivisionByZero);
    }
    Ok(a.div_ceil(b))
}

/// `std.math.divCeil(usize, a, b)` — the `draw` variant (see the type asymmetry note).
fn div_ceil_usize(a: usize, b: usize) -> Result<usize, DrawError> {
    if b == 0 {
        return Err(DrawError::DivisionByZero);
    }
    Ok(a.div_ceil(b))
}
