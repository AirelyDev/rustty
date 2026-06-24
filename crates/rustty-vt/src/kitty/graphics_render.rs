//! rustty-vt kitty/graphics_render
//!
//! PORTS: ghostty src/terminal/kitty/graphics_render.zig
//!
//! PORT STATUS: ported

// const std = @import("std");
// const testing = std.testing;
// const terminal = @import("../main.zig");
use crate::pagelist::Pin;

/// A render placement is a way to position a Kitty graphics image onto
/// the screen. It is broken down into the fields that make it easier to
/// position the image using a renderer.
// PORT NOTE: the reference is a plain struct (no derives). `Pin` is non-`Copy`/
// non-`Clone` in the Rust port (raw `*mut Node`), so we keep this struct as-is.
pub struct Placement {
    /// The top-left corner of the image in grid coordinates.
    pub top_left: Pin,

    /// The offset in pixels from the top-left corner of the grid cell.
    pub offset_x: u32,
    pub offset_y: u32,

    /// The source rectangle of the image to render. This doesn't have to
    /// match the size the destination size and the renderer is expected
    /// to scale the image to fit the destination size.
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,

    /// The final width/height of the image in pixels.
    pub dest_width: u32,
    pub dest_height: u32,
}

// PORT NOTE: the reference declares default field values (`offset_x: u32 = 0`,
// etc.) so that `.{ .top_left = pin }` is a valid struct literal. Rust has no
// per-field defaults, so we provide a constructor mirroring that single-field
// initialization (only `top_left` set, the rest defaulting to 0). `graphics_unicode`
// uses exactly this form (`return .{ .top_left = self.pin }`).
impl Placement {
    /// Construct a placement with only `top_left` set and all pixel fields zero,
    /// mirroring the reference `.{ .top_left = pin }` struct literal.
    pub fn from_top_left(top_left: Pin) -> Placement {
        Placement {
            top_left,
            offset_x: 0,
            offset_y: 0,
            source_x: 0,
            source_y: 0,
            source_width: 0,
            source_height: 0,
            dest_width: 0,
            dest_height: 0,
        }
    }
}
