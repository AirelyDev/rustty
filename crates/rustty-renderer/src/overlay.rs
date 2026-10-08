//! PORTS: ghostty `src/renderer/Overlay.zig` — the CPU/z2d debug overlay.
//!
//! The debug overlay that can be drawn on top of the terminal
//! during the rendering process.
//!
//! This is implemented by doing all the drawing on the CPU via z2d,
//! since the debug overlay isn't that common, z2d is pretty fast, and
//! it simplifies our implementation quite a bit by not relying on us
//! having a bunch of shaders that we have to write per-platform.
//!
//! Initialize the overlay, apply features with `apply_features`, then
//! get the resulting image with `pending_image` to upload to the GPU.
//! This works in concert with `renderer.image.State` to simplify. Draw
//! it on the GPU as an image composited on top of the terminal output.
//!
//! PORT STATUS (GPU R-d P1, macOS-only): the lifecycle + dispatch surface is
//! FULL, and the four z2d render leaves are now FILLED: the ported z2d `Context`
//! exposes `anti_aliasing_mode`/`hairline` (public fields), the z2d hairline
//! stroking fast-path is ported, and the `RenderState` row/cell metadata is
//! public — so `highlight_hyperlinks`/`highlight_semantic_prompts` and the
//! `highlight_grid_rect`/`highlight_pixel_rect` rect helpers draw for real. The
//! overlay renders on the CPU to a z2d `Surface` (no Metal). `Overlay.zig`
//! carries **0** `test{}` blocks (golden-image-gated); the fill is covered by
//! host-only pixel tests below.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const z2d = @import("z2d");
// const terminal = @import("../terminal/main.zig");
// const size = @import("size.zig");
// const Size = size.Size;
// const CellSize = size.CellSize;
// const Image = @import("image.zig").Image;
use rustty_z2d::context::Context;
use rustty_z2d::options::AntiAliasMode;
use rustty_z2d::painter;
use rustty_z2d::pixel::{Pixel, Rgb, Rgba};
use rustty_z2d::surface::{self, Surface};

use rustty_vt::page::{SemanticContent, SemanticPrompt};
use rustty_vt::render::RenderState;

// PORT NOTE (linux-port): `image` is a monomorphic twin pair gated to the targets
// that have a `generic` renderer arm (macOS/Metal, Linux/OpenGL — its `Image::Ready`
// variant embeds the BACKEND's `Texture`), but NOTHING else in this file touches
// `image` — the overlay is pure CPU/z2d. So the import, and the single
// `pending_image` leaf that needs it, are gated here rather than gating the whole
// module: that keeps the backend-free surface (`Feature`, `InitError`,
// `Overlay::init`/`apply_features`/the four z2d render leaves) compiling and TESTED
// on every target. `Pending` and `PixelFormat` are themselves backend-free plain
// data (identical POD in both twins); the L4 OpenGL twin is what made them reachable
// on Linux, so this gate now tracks the dispatcher's `any(macos, linux, freebsd)`
// (W2D/F-003 widened the OpenGL arm to FreeBSD — `backend.zig:20-21`).
// PORT NOTE (task W-2, N342): Windows is the third `.opengl` target (`backend.zig:20-21`);
// its runtime is `.none`, so it builds the `apprt.embedded` arms of `OpenGL.zig`.
#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "windows"
))]
use crate::image::{Pending, PixelFormat};
use crate::size::{CellSize, Size};

// const log = std.log.scoped(.renderer_overlay);
// PORT NOTE (task-14c-2): the `log` facade is wired for this crate
// (`Cargo.toml`'s `log = { workspace = true }`; DECISION-logging.md §6r call
// shape 3); all THREE of `Overlay.zig`'s `log.warn` sites are RESTORED beneath
// their R1 comments.
/// `Overlay.zig:25`'s `const log = std.log.scoped(.renderer_overlay);`.
const LOG_SCOPE: &str = "renderer_overlay";

/// The colors we use for overlays.
// pub const Color = enum { hyperlink, semantic_prompt, semantic_input };
#[derive(Clone, Copy)]
pub enum Color {
    Hyperlink,      // light blue
    SemanticPrompt, // orange/gold
    SemanticInput,  // cyan
}

impl Color {
    pub fn rgb(self) -> Rgb {
        match self {
            Color::Hyperlink => Rgb {
                r: 180,
                g: 180,
                b: 255,
            },
            Color::SemanticPrompt => Rgb {
                r: 255,
                g: 200,
                b: 64,
            },
            Color::SemanticInput => Rgb {
                r: 64,
                g: 200,
                b: 255,
            },
        }
    }

    /// The fill color for rectangles.
    pub fn rect_fill(self) -> Pixel {
        self.alpha_pixel(96)
    }

    /// The border color for rectangles.
    pub fn rect_border(self) -> Pixel {
        self.alpha_pixel(200)
    }

    /// The raw RGB as a pixel.
    pub fn pixel(self) -> Pixel {
        self.rgb().as_pixel()
    }

    fn alpha_pixel(self, alpha: u8) -> Pixel {
        // var rgba: z2d.pixel.RGBA = .fromPixel(self.pixel());
        // rgba.a = alpha;
        // return rgba.multiply().asPixel();
        let mut rgba: Rgba = Rgba::from_pixel(self.pixel());
        rgba.a = alpha;
        rgba.multiply().as_pixel()
    }
}

/// The debug overlay (Overlay.zig `@This()` — the file-as-struct).
pub struct Overlay {
    /// The surface we're drawing our overlay to.
    pub surface: Surface,

    /// Cell size information so we can map grid coordinates to pixels.
    pub cell_size: CellSize,
}

/// The set of available features and their configuration.
// pub const Feature = union(enum) { highlight_hyperlinks, semantic_prompts };
//
// PORT NOTE: `PartialEq, Eq` added so the inspector's `renderer::Info.features`
// (the reference `AutoArrayHashMapUnmanaged(Tag(Feature), Feature)` insertion-
// ordered set — INV-7) can be modelled as an ordered unique set of `Feature`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    HighlightHyperlinks,
    SemanticPrompts,
}

/// `InitError = Allocator.Error || error{InvalidDimensions}`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InitError {
    // Allocator.Error
    OutOfMemory,
    // The terminal dimensions are invalid to support an overlay.
    // Either too small or too big.
    InvalidDimensions,
}

impl Overlay {
    /// Initialize a new, blank overlay.
    //
    // PORT NOTE (§5.1): the rustty-z2d subset dropped the Zig `Allocator` from
    // `Surface::init_pixel`/`deinit` (the surface owns its `Vec` buffer), so
    // `init` takes only `sz` and `deinit` takes only `&mut self`; the Chunk-3
    // `rebuildOverlay` caller drops the `alloc` argument correspondingly.
    pub fn init(sz: Size) -> Result<Overlay, InitError> {
        // Our surface does NOT need to take into account padding because
        // we render the overlay using the image subsystem and shaders which
        // already take that into account.
        let term_size = sz.terminal();
        // std.math.cast(i32, ...) orelse return error.InvalidDimensions
        let width = i32::try_from(term_size.width).map_err(|_| InitError::InvalidDimensions)?;
        let height = i32::try_from(term_size.height).map_err(|_| InitError::InvalidDimensions)?;
        let sfc = Surface::init_pixel(
            Pixel::Rgba(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            }),
            width,
            height,
        )
        .map_err(|err| match err {
            // error.OutOfMemory => return error.OutOfMemory,
            //   → the ported z2d `Surface::init_pixel` uses the global
            //     allocator and cannot surface `OutOfMemory`, so that arm
            //     is structurally unreachable and preserved as this comment.
            // error.InvalidWidth, error.InvalidHeight => InvalidDimensions,
            surface::Error::InvalidWidth | surface::Error::InvalidHeight => {
                InitError::InvalidDimensions
            }
        })?;
        // errdefer sfc.deinit(alloc);  → the infallible tail below cannot fail.

        Ok(Overlay {
            surface: sfc,
            cell_size: sz.cell,
        })
    }

    // pub fn deinit(self: *Overlay, alloc: Allocator) void
    pub fn deinit(&mut self) {
        self.surface.deinit();
    }

    /// Returns a pending image that can be used to copy, convert, upload, etc.
    //
    // PORT NOTE (linux-port): the ONLY `crate::image`-typed leaf on `Overlay`, hence
    // the only thing in this file that binds it to the twin-gated `image` module
    // (see the gated import above). The body is FULL and target-independent — the
    // gate is purely the `image::Pending` return type, and it lifted to
    // `any(macos, linux, freebsd)` with the L4 OpenGL image twin (W2D/F-003).
    #[cfg(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "windows"
    ))]
    pub fn pending_image(&self) -> Pending {
        Pending {
            // @intCast(self.surface.getWidth()) / getHeight()
            width: self.surface.get_width() as u32,
            height: self.surface.get_height() as u32,
            pixel_format: PixelFormat::Rgba,
            // @ptrCast(self.surface.image_surface_rgba.buf.ptr)
            //
            // PORT NOTE (§5.1): the overlay surface is always `ImageSurfaceRgba`
            // (built by `init_pixel` from an RGBA initial pixel), so we match that
            // variant and alias its `buf` (a `Vec<Rgba>`, `pub`) as `*mut u8`.
            // This is a BORROW into the live z2d buffer — `State::prep_image`
            // dupes it before it is ever owned by an `Image`.
            data: match &self.surface {
                Surface::ImageSurfaceRgba(s) => s.buf.as_ptr() as *mut u8,
                _ => unreachable!("overlay surface is always ImageSurfaceRgba"),
            },
        }
    }

    /// Clear the overlay.
    pub fn reset(&mut self) {
        self.surface.paint_pixel(Pixel::Rgba(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }));
    }

    /// Apply the given features to this overlay. This will draw on top of
    /// any pre-existing content in the overlay.
    //
    // PORT NOTE (§5.1): the `alloc` param is dropped (the z2d `Context::init`
    // subset dropped it). FULL dispatch — the `for f in features` switch mirrors
    // `State::draw`'s dispatch; the render bodies it calls are R5 seams.
    pub fn apply_features(&mut self, state: &RenderState, features: &[Feature]) {
        for f in features {
            match f {
                Feature::HighlightHyperlinks => self.highlight_hyperlinks(state),
                Feature::SemanticPrompts => self.highlight_semantic_prompts(state),
            }
        }
    }

    /// Add rectangles around contiguous hyperlinks in the render state.
    ///
    /// Note: this currently doesn't take into account unique hyperlink IDs
    /// because the render state doesn't contain this. This will be added
    /// later.
    //
    // FILLED (was R5 SEAM): draws via the `RenderState` row/cell `.hyperlink`
    // metadata (now public) + the AA/hairline `Context`. Verbatim Zig body
    // preserved:
    //
    // fn highlightHyperlinks(
    //     self: *Overlay,
    //     alloc: Allocator,
    //     state: *const terminal.RenderState,
    // ) void {
    //     const border_color = Color.hyperlink.rectBorder();
    //     const fill_color = Color.hyperlink.rectFill();
    //     const row_slice = state.row_data.slice();
    //     const row_raw = row_slice.items(.raw);
    //     const row_cells = row_slice.items(.cells);
    //     for (row_raw, row_cells, 0..) |row, cells, y| {
    //         if (!row.hyperlink) continue;
    //         const cells_slice = cells.slice();
    //         const raw_cells = cells_slice.items(.raw);
    //         var x: usize = 0;
    //         while (x < raw_cells.len) {
    //             // Skip cells without hyperlinks
    //             if (!raw_cells[x].hyperlink) {
    //                 x += 1;
    //                 continue;
    //             }
    //             // Found start of a hyperlink run
    //             const start_x = x;
    //             // Find end of contiguous hyperlink cells
    //             while (x < raw_cells.len and raw_cells[x].hyperlink) x += 1;
    //             const end_x = x;
    //             self.highlightGridRect(
    //                 alloc,
    //                 start_x,
    //                 y,
    //                 end_x - start_x,
    //                 1,
    //                 border_color,
    //                 fill_color,
    //             ) catch |err| {
    //                 std.log.warn("Error drawing hyperlink border: {}", .{err});
    //             };
    //         }
    //     }
    // }
    fn highlight_hyperlinks(&mut self, state: &RenderState) {
        let border_color = Color::Hyperlink.rect_border();
        let fill_color = Color::Hyperlink.rect_fill();

        // for (row_raw, row_cells, 0..) |row, cells, y|
        for (y, row) in state.row_data.iter().enumerate() {
            if !row.raw.hyperlink() {
                continue;
            }

            // PORT NOTE (reshape): `cells.slice().items(.raw)` -> the SoA raw column.
            let raw_cells: &[rustty_vt::page::Cell] = row.cells.raw();

            let mut x: usize = 0;
            while x < raw_cells.len() {
                // Skip cells without hyperlinks
                if !raw_cells[x].hyperlink() {
                    x += 1;
                    continue;
                }

                // Found start of a hyperlink run
                let start_x = x;

                // Find end of contiguous hyperlink cells
                while x < raw_cells.len() && raw_cells[x].hyperlink() {
                    x += 1;
                }
                let end_x = x;

                if let Err(err) = self.highlight_grid_rect(
                    start_x,
                    y,
                    end_x - start_x,
                    1,
                    border_color,
                    fill_color,
                ) {
                    // std.log.warn("Error drawing hyperlink border: {}", .{err});
                    // PORT NOTE (task-14c-2, scope): this is the reference's ONLY
                    // `std.log.warn` in this file — i.e. the `.default` scope, NOT the
                    // file's `.renderer_overlay` one (`main_ghostty.zig:133`: a
                    // `.default`-scoped record gets an EMPTY prefix). Per
                    // DECISION-logging.md §6r the `.default` scope is carried as
                    // `target: ""`; do not "fix" it to `LOG_SCOPE`, the difference is
                    // the reference's.
                    log::warn!(target: "", "Error drawing hyperlink border: {err:?}");
                }
            }
        }
    }

    // FILLED (was R5 SEAM). Verbatim Zig body preserved:
    //
    // fn highlightSemanticPrompts(
    //     self: *Overlay,
    //     alloc: Allocator,
    //     state: *const terminal.RenderState,
    // ) void {
    //     const row_slice = state.row_data.slice();
    //     const row_raw = row_slice.items(.raw);
    //     const row_cells = row_slice.items(.cells);
    //     // Highlight the row-level semantic prompt bars. The prompts are easy
    //     // because they're part of the row metadata.
    //     {
    //         const prompt_border = Color.semantic_prompt.rectBorder();
    //         const prompt_fill = Color.semantic_prompt.rectFill();
    //         var y: usize = 0;
    //         while (y < row_raw.len) {
    //             // If its not a semantic prompt row, skip it.
    //             if (row_raw[y].semantic_prompt == .none) {
    //                 y += 1;
    //                 continue;
    //             }
    //             // Find the full length of the semantic prompt row by connecting
    //             // all continuations.
    //             const start_y = y;
    //             y += 1;
    //             while (y < row_raw.len and
    //                 row_raw[y].semantic_prompt == .prompt_continuation)
    //             {
    //                 y += 1;
    //             }
    //             const end_y = y; // Exclusive
    //             const bar_width = @min(@as(usize, 5), self.cell_size.width);
    //             self.highlightPixelRect(
    //                 alloc,
    //                 0,
    //                 start_y,
    //                 bar_width,
    //                 end_y - start_y,
    //                 prompt_border,
    //                 prompt_fill,
    //             ) catch |err| {
    //                 log.warn("Error drawing semantic prompt bar: {}", .{err});
    //             };
    //         }
    //     }
    //     // Highlight contiguous semantic cells within rows.
    //     for (row_cells, 0..) |cells, y| {
    //         const cells_slice = cells.slice();
    //         const raw_cells = cells_slice.items(.raw);
    //         var x: usize = 0;
    //         while (x < raw_cells.len) {
    //             const cell = raw_cells[x];
    //             const content = cell.semantic_content;
    //             const start_x = x;
    //             // We skip output because its just the rest of the non-prompt
    //             // parts and it makes the overlay too noisy.
    //             if (cell.semantic_content == .output) {
    //                 x += 1;
    //                 continue;
    //             }
    //             // Find the end of this content.
    //             x += 1;
    //             while (x < raw_cells.len) {
    //                 const next = raw_cells[x];
    //                 if (next.semantic_content != content) break;
    //                 x += 1;
    //             }
    //             const color: Color = switch (content) {
    //                 .prompt => .semantic_prompt,
    //                 .input => .semantic_input,
    //                 .output => unreachable,
    //             };
    //             self.highlightGridRect(
    //                 alloc,
    //                 start_x,
    //                 y,
    //                 x - start_x,
    //                 1,
    //                 color.rectBorder(),
    //                 color.rectFill(),
    //             ) catch |err| {
    //                 log.warn("Error drawing semantic content highlight: {}", .{err});
    //             };
    //         }
    //     }
    // }
    fn highlight_semantic_prompts(&mut self, state: &RenderState) {
        // Highlight the row-level semantic prompt bars. The prompts are easy
        // because they're part of the row metadata.
        {
            let prompt_border = Color::SemanticPrompt.rect_border();
            let prompt_fill = Color::SemanticPrompt.rect_fill();

            let mut y: usize = 0;
            while y < state.row_data.len() {
                // If its not a semantic prompt row, skip it.
                if state.row_data[y].raw.semantic_prompt() == SemanticPrompt::None {
                    y += 1;
                    continue;
                }

                // Find the full length of the semantic prompt row by connecting
                // all continuations.
                let start_y = y;
                y += 1;
                while y < state.row_data.len()
                    && state.row_data[y].raw.semantic_prompt() == SemanticPrompt::PromptContinuation
                {
                    y += 1;
                }
                let end_y = y; // Exclusive

                // const bar_width = @min(@as(usize, 5), self.cell_size.width);
                let bar_width = std::cmp::min(5usize, self.cell_size.width as usize);
                if let Err(err) = self.highlight_pixel_rect(
                    0,
                    start_y,
                    bar_width,
                    end_y - start_y,
                    prompt_border,
                    prompt_fill,
                ) {
                    // log.warn("Error drawing semantic prompt bar: {}", .{err});
                    log::warn!(target: LOG_SCOPE, "Error drawing semantic prompt bar: {err:?}");
                }
            }
        }

        // Highlight contiguous semantic cells within rows.
        // for (row_cells, 0..) |cells, y|
        for (y, row) in state.row_data.iter().enumerate() {
            // PORT NOTE (reshape): `cells.slice().items(.raw)` -> the SoA raw column.
            let raw_cells: &[rustty_vt::page::Cell] = row.cells.raw();

            let mut x: usize = 0;
            while x < raw_cells.len() {
                let content = raw_cells[x].semantic_content();
                let start_x = x;

                // We skip output because its just the rest of the non-prompt
                // parts and it makes the overlay too noisy.
                if content == SemanticContent::Output {
                    x += 1;
                    continue;
                }

                // Find the end of this content.
                x += 1;
                while x < raw_cells.len() {
                    let next = raw_cells[x].semantic_content();
                    if next != content {
                        break;
                    }
                    x += 1;
                }

                // switch (content) { .prompt=>…, .input=>…, .output=>unreachable }
                let color: Color = match content {
                    SemanticContent::Prompt => Color::SemanticPrompt,
                    SemanticContent::Input => Color::SemanticInput,
                    SemanticContent::Output => unreachable!(),
                };

                if let Err(err) = self.highlight_grid_rect(
                    start_x,
                    y,
                    x - start_x,
                    1,
                    color.rect_border(),
                    color.rect_fill(),
                ) {
                    // log.warn("Error drawing semantic content highlight: {}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "Error drawing semantic content highlight: {err:?}"
                    );
                }
            }
        }
    }

    /// Creates a rectangle for highlighting a grid region. x/y/width/height
    /// are all in grid cells.
    //
    // FILLED (was R5 SEAM): `ctx.anti_aliasing_mode`/`ctx.hairline` are public
    // fields on the ported z2d `Context`, and z2d hairline stroking is ported.
    // Verbatim Zig body preserved:
    //
    // fn highlightGridRect(
    //     self: *Overlay,
    //     alloc: Allocator,
    //     x: usize,
    //     y: usize,
    //     width: usize,
    //     height: usize,
    //     border_color: z2d.Pixel,
    //     fill_color: z2d.Pixel,
    // ) !void {
    //     // All math below uses checked arithmetic to avoid overflows. The
    //     // inputs aren't trusted and the path this is in isn't hot enough
    //     // to wrarrant unsafe optimizations.
    //     // Calculate our width/height in pixels.
    //     const px_width = std.math.cast(i32, try std.math.mul(
    //         usize,
    //         width,
    //         self.cell_size.width,
    //     )) orelse return error.Overflow;
    //     const px_height = std.math.cast(i32, try std.math.mul(
    //         usize,
    //         height,
    //         self.cell_size.height,
    //     )) orelse return error.Overflow;
    //     // Calculate pixel coordinates
    //     const start_x: f64 = @floatFromInt(std.math.cast(i32, try std.math.mul(
    //         usize,
    //         x,
    //         self.cell_size.width,
    //     )) orelse return error.Overflow);
    //     const start_y: f64 = @floatFromInt(std.math.cast(i32, try std.math.mul(
    //         usize,
    //         y,
    //         self.cell_size.height,
    //     )) orelse return error.Overflow);
    //     const end_x: f64 = start_x + @as(f64, @floatFromInt(px_width));
    //     const end_y: f64 = start_y + @as(f64, @floatFromInt(px_height));
    //     // Grab our context to draw
    //     var ctx: z2d.Context = .init(alloc, &self.surface);
    //     defer ctx.deinit();
    //     // Don't need AA because we use sharp edges
    //     ctx.setAntiAliasingMode(.none);
    //     // Can use hairline since we have 1px borders
    //     ctx.setHairline(true);
    //     // Draw rectangle path
    //     try ctx.moveTo(start_x, start_y);
    //     try ctx.lineTo(end_x, start_y);
    //     try ctx.lineTo(end_x, end_y);
    //     try ctx.lineTo(start_x, end_y);
    //     try ctx.closePath();
    //     // Fill
    //     ctx.setSourceToPixel(fill_color);
    //     try ctx.fill();
    //     // Border
    //     ctx.setLineWidth(1);
    //     ctx.setSourceToPixel(border_color);
    //     try ctx.stroke();
    // }
    #[allow(clippy::too_many_arguments)]
    fn highlight_grid_rect(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        border_color: Pixel,
        fill_color: Pixel,
    ) -> Result<(), OverlayError> {
        // All math below uses checked arithmetic to avoid overflows. The
        // inputs aren't trusted and the path this is in isn't hot enough
        // to wrarrant unsafe optimizations.
        let cw = self.cell_size.width as usize;
        let ch = self.cell_size.height as usize;

        // Calculate our width/height in pixels.
        // std.math.cast(i32, try std.math.mul(usize, width, cell.width)) — the
        // `checked_mul` (usize product overflow) and `try_from` (exceeds i32::MAX)
        // failures both collapse onto `error.Overflow`, matching the Zig.
        let px_width: i32 = width
            .checked_mul(cw)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or(OverlayError::Overflow)?;
        let px_height: i32 = height
            .checked_mul(ch)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or(OverlayError::Overflow)?;

        // Calculate pixel coordinates
        let start_x: f64 = x
            .checked_mul(cw)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or(OverlayError::Overflow)? as f64;
        let start_y: f64 = y
            .checked_mul(ch)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or(OverlayError::Overflow)? as f64;
        let end_x: f64 = start_x + px_width as f64;
        let end_y: f64 = start_y + px_height as f64;

        // Grab our context to draw
        let mut ctx: Context = Context::init(&mut self.surface);

        // Don't need AA because we use sharp edges
        ctx.anti_aliasing_mode = AntiAliasMode::None;
        // Can use hairline since we have 1px borders
        ctx.hairline = true;

        // Draw rectangle path. (The ported z2d subset made the managed
        // move_to/line_to/close_path infallible — the reference `try` on those
        // is structurally absent; only fill/stroke remain fallible.)
        ctx.move_to(start_x, start_y);
        ctx.line_to(end_x, start_y);
        ctx.line_to(end_x, end_y);
        ctx.line_to(start_x, end_y);
        ctx.close_path();

        // Fill
        ctx.set_source_to_pixel(fill_color);
        ctx.fill().map_err(OverlayError::Fill)?;

        // Border
        ctx.set_line_width(1.0);
        ctx.set_source_to_pixel(border_color);
        ctx.stroke().map_err(OverlayError::Stroke)?;

        ctx.deinit(); // defer ctx.deinit()
        Ok(())
    }

    /// Creates a rectangle for highlighting a region. x/y are grid cells and
    /// width/height are pixels.
    //
    // FILLED (was R5 SEAM). Verbatim Zig body preserved:
    //
    // fn highlightPixelRect(
    //     self: *Overlay,
    //     alloc: Allocator,
    //     x: usize,
    //     y: usize,
    //     width_px: usize,
    //     height: usize,
    //     border_color: z2d.Pixel,
    //     fill_color: z2d.Pixel,
    // ) !void {
    //     const px_width = std.math.cast(i32, width_px) orelse return error.Overflow;
    //     const px_height = std.math.cast(i32, try std.math.mul(
    //         usize,
    //         height,
    //         self.cell_size.height,
    //     )) orelse return error.Overflow;
    //     const start_x: f64 = @floatFromInt(std.math.cast(i32, try std.math.mul(
    //         usize,
    //         x,
    //         self.cell_size.width,
    //     )) orelse return error.Overflow);
    //     const start_y: f64 = @floatFromInt(std.math.cast(i32, try std.math.mul(
    //         usize,
    //         y,
    //         self.cell_size.height,
    //     )) orelse return error.Overflow);
    //     const end_x: f64 = start_x + @as(f64, @floatFromInt(px_width));
    //     const end_y: f64 = start_y + @as(f64, @floatFromInt(px_height));
    //     var ctx: z2d.Context = .init(alloc, &self.surface);
    //     defer ctx.deinit();
    //     ctx.setAntiAliasingMode(.none);
    //     ctx.setHairline(true);
    //     try ctx.moveTo(start_x, start_y);
    //     try ctx.lineTo(end_x, start_y);
    //     try ctx.lineTo(end_x, end_y);
    //     try ctx.lineTo(start_x, end_y);
    //     try ctx.closePath();
    //     ctx.setSourceToPixel(fill_color);
    //     try ctx.fill();
    //     ctx.setLineWidth(1);
    //     ctx.setSourceToPixel(border_color);
    //     try ctx.stroke();
    // }
    #[allow(clippy::too_many_arguments)]
    fn highlight_pixel_rect(
        &mut self,
        x: usize,
        y: usize,
        width_px: usize,
        height: usize,
        border_color: Pixel,
        fill_color: Pixel,
    ) -> Result<(), OverlayError> {
        let cw = self.cell_size.width as usize;
        let ch = self.cell_size.height as usize;

        // const px_width = std.math.cast(i32, width_px) orelse return error.Overflow;
        let px_width: i32 = i32::try_from(width_px).map_err(|_| OverlayError::Overflow)?;
        let px_height: i32 = height
            .checked_mul(ch)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or(OverlayError::Overflow)?;

        let start_x: f64 = x
            .checked_mul(cw)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or(OverlayError::Overflow)? as f64;
        let start_y: f64 = y
            .checked_mul(ch)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or(OverlayError::Overflow)? as f64;
        let end_x: f64 = start_x + px_width as f64;
        let end_y: f64 = start_y + px_height as f64;

        let mut ctx: Context = Context::init(&mut self.surface);
        ctx.anti_aliasing_mode = AntiAliasMode::None;
        ctx.hairline = true;

        ctx.move_to(start_x, start_y);
        ctx.line_to(end_x, start_y);
        ctx.line_to(end_x, end_y);
        ctx.line_to(start_x, end_y);
        ctx.close_path();

        ctx.set_source_to_pixel(fill_color);
        ctx.fill().map_err(OverlayError::Fill)?;

        ctx.set_line_width(1.0);
        ctx.set_source_to_pixel(border_color);
        ctx.stroke().map_err(OverlayError::Stroke)?;

        ctx.deinit();
        Ok(())
    }
}

/// The inferred error set of the overlay rect-drawing seams: `error.Overflow`
/// (from `std.math.mul`/`cast`) plus the z2d fill/stroke errors. In the ported
/// z2d subset `move_to`/`line_to`/`close_path` are infallible, so only
/// `fill`/`stroke` contribute — hence the two wrapping variants.
//
// PORT NOTE (updated task-14c-2): the `Fill`/`Stroke` payloads carry the faithful
// z2d error set, and the reference consumes them via `log.warn("... {}", .{err})`
// at each rect-helper call site — all three of which are now RESTORED, so the
// payloads DO reach a `{err:?}` render. `#[allow(dead_code)]` nevertheless stays:
// rustc does not count a `#[derive(Debug)]` field read as a use, so removing it
// re-breaks `-D warnings` (the same finding 14c-1 recorded for
// `generic.rs`'s `RebuildRowError`).
#[derive(Debug)]
#[allow(dead_code)]
enum OverlayError {
    Overflow,
    Fill(painter::FillError),
    Stroke(painter::StrokeError),
}

// PORT NOTE (rustty-only smoke test, NOT a ghostty parity test): `Overlay.zig`
// carries 0 `test{}` blocks (golden-image-gated). This proves the pure-z2d
// lifecycle (init → pending_image → reset → deinit) compiles and runs with no
// GPU. No Metal device needed — and, since the linux-port ungated this module,
// these tests now run on every target (the `pending_image` step alone is
// macOS-gated; see `overlay_surface_roundtrip`).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::size::{CellSize, Padding, ScreenSize, Size};

    fn size(w: u32, h: u32) -> Size {
        Size {
            screen: ScreenSize {
                width: w,
                height: h,
            },
            cell: CellSize {
                width: 8,
                height: 16,
            },
            padding: Padding {
                top: 0,
                bottom: 0,
                left: 0,
                right: 0,
            },
        }
    }

    #[test]
    fn overlay_surface_roundtrip() {
        let sz = size(80, 40);
        let mut o = Overlay::init(sz).unwrap();

        // pending_image reports the terminal (padding-less) surface dims, aliasing
        // the RGBA z2d buffer.
        //
        // PORT NOTE (linux-port): `pending_image` is twin-gated (its `image::Pending`
        // return type is — see the module head), so its assertions are too. The
        // init/reset/deinit lifecycle around them is backend-free and stays covered on
        // every target.
        #[cfg(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "freebsd",
            target_os = "windows"
        ))]
        {
            let term = sz.terminal();
            let p = o.pending_image();
            assert_eq!(p.width, term.width);
            assert_eq!(p.height, term.height);
            assert_eq!(p.pixel_format, PixelFormat::Rgba);
            assert!(!p.data.is_null());
        }

        o.reset();
        o.deinit();
    }

    #[test]
    fn overlay_invalid_dimensions() {
        // A zero-width terminal is rejected by z2d (width < 1) → InvalidDimensions.
        let sz = size(0, 40);
        assert!(matches!(
            Overlay::init(sz),
            Err(InitError::InvalidDimensions)
        ));
    }

    // The overlay renders on the CPU to a z2d `Surface` — no Metal, no GPU. So
    // these are host-only: build a real `Terminal` → `RenderState`, poke the
    // row/cell metadata bits through the already-`pub` accessors, `Overlay::init`
    // + `apply_features`, then assert the drawn overlay-rect pixels (which now no
    // longer panic on z2d hairline).
    #[test]
    fn overlay_draws_hyperlink_rect() {
        use rustty_vt::alloc::Global;
        use rustty_vt::kitty;
        use rustty_vt::modes::{self as modespkg};
        use rustty_vt::render::RenderState;
        use rustty_vt::size as vt_size;
        use rustty_vt::terminal::{Colors, Options, Terminal};

        // Local opts helper — verbatim from cell.rs / render.rs test harness.
        fn opts(cols: vt_size::CellCountInt, rows: vt_size::CellCountInt) -> Options {
            Options {
                cols,
                rows,
                max_scrollback_bytes: Some(10_000),
                max_scrollback_lines: None,
                colors: Colors::default(),
                default_modes: modespkg::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                default_xt_checksum: Default::default(),
                kitty_image_storage_limit: 320 * 1000 * 1000,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            }
        }

        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"AB");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Mark row 0 cols 0..1 as a hyperlink run.
        state.row_data[0].raw.set_hyperlink(true);
        state.row_data[0].cells.raw_mut()[0].set_hyperlink(true);
        state.row_data[0].cells.raw_mut()[1].set_hyperlink(true);

        let sz = size(80, 48); // cell 8x16 (existing helper) → 10x3 terminal
        let mut o = Overlay::init(sz).unwrap();
        o.apply_features(&state, &[Feature::HighlightHyperlinks]);

        // Drawn inside cell (0,0):
        match o.surface.get_pixel(4, 8) {
            Some(Pixel::Rgba(px)) => assert!(px.a != 0, "hyperlink fill not drawn"),
            other => panic!("expected rgba, got {other:?}"),
        }
        // Untouched cell (0,2):
        match o.surface.get_pixel(4, 40) {
            Some(Pixel::Rgba(px)) => assert_eq!(px.a, 0, "non-link cell drawn"),
            other => panic!("expected rgba, got {other:?}"),
        }

        o.deinit();
        state.deinit(alloc);
        t.deinit(alloc);
    }

    #[test]
    fn overlay_draws_semantic_prompt_bar() {
        use rustty_vt::alloc::Global;
        use rustty_vt::kitty;
        use rustty_vt::modes::{self as modespkg};
        use rustty_vt::page::SemanticPrompt;
        use rustty_vt::render::RenderState;
        use rustty_vt::size as vt_size;
        use rustty_vt::terminal::{Colors, Options, Terminal};

        fn opts(cols: vt_size::CellCountInt, rows: vt_size::CellCountInt) -> Options {
            Options {
                cols,
                rows,
                max_scrollback_bytes: Some(10_000),
                max_scrollback_lines: None,
                colors: Colors::default(),
                default_modes: modespkg::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                default_xt_checksum: Default::default(),
                kitty_image_storage_limit: 320 * 1000 * 1000,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            }
        }

        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"AB");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Mark row 0 as a semantic prompt row → the left-edge bar.
        state.row_data[0]
            .raw
            .set_semantic_prompt(SemanticPrompt::Prompt);

        let sz = size(80, 48);
        let mut o = Overlay::init(sz).unwrap();
        o.apply_features(&state, &[Feature::SemanticPrompts]);

        // Bar width is min(5, cell_width=8) = 5, so x ∈ [0,5): sample (2,8).
        match o.surface.get_pixel(2, 8) {
            Some(Pixel::Rgba(px)) => assert!(px.a != 0, "prompt bar not drawn"),
            other => panic!("expected rgba, got {other:?}"),
        }
        // Untouched row 2:
        match o.surface.get_pixel(2, 40) {
            Some(Pixel::Rgba(px)) => assert_eq!(px.a, 0, "non-prompt row drawn"),
            other => panic!("expected rgba, got {other:?}"),
        }

        o.deinit();
        state.deinit(alloc);
        t.deinit(alloc);
    }

    #[test]
    fn color_alpha_pixels() {
        // rectFill uses alpha 96, rectBorder uses alpha 200; both premultiplied.
        for c in [
            Color::Hyperlink,
            Color::SemanticPrompt,
            Color::SemanticInput,
        ] {
            let fill = c.rect_fill();
            let border = c.rect_border();
            // The pixels are RGBA (premultiplied). Just assert the alpha survives.
            if let Pixel::Rgba(px) = fill {
                assert_eq!(px.a, 96);
            } else {
                panic!("expected rgba fill pixel");
            }
            if let Pixel::Rgba(px) = border {
                assert_eq!(px.a, 200);
            } else {
                panic!("expected rgba border pixel");
            }
        }
    }
}
