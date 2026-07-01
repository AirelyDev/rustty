// const std = @import("std");
// const canvas = @import("sprite/canvas.zig");
// pub const Face = @import("sprite/Face.zig");
//
// PORT NOTE: `canvas` is the `canvas` submodule below. `Face` (sprite/Face.zig)
// is ported in a later sprite batch; its re-export is recorded here as a
// commented reference until then.
pub mod canvas;
pub mod draw;

// pub const Box = canvas.Box;
pub use canvas::Box;
// pub const Point = canvas.Point;
pub use canvas::Point;
// pub const Canvas = canvas.Canvas;
pub use canvas::Canvas;
// pub const Color = canvas.Color;
pub use canvas::Color;

/// Sprites are represented as special codepoints outside of the Unicode
/// codepoint range. Unicode maxes out at U+10FFFF (21 bits), and we use the
/// high 11 bits to hide our special characters.
///
/// These characters are ONLY used for rendering and NEVER used written to
/// text files or any other exported format, so we don't use the Private Use
/// Area of Unicode.
// pub const Sprite = enum(u32) {
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sprite {
    // Start 1 above the maximum Unicode codepoint.
    // underline = start,
    Underline = Sprite::START,
    // underline_double,
    UnderlineDouble,
    // underline_dotted,
    UnderlineDotted,
    // underline_dashed,
    UnderlineDashed,
    // underline_curly,
    UnderlineCurly,

    // strikethrough,
    Strikethrough,

    // overline,
    Overline,

    // cursor_rect,
    CursorRect,
    // cursor_hollow_rect,
    CursorHollowRect,
    // cursor_bar,
    CursorBar,
    // cursor_underline,
    CursorUnderline,
}

impl Sprite {
    // Start 1 above the maximum Unicode codepoint.
    // pub const start: u32 = std.math.maxInt(u21) + 1;
    //
    // PORT NOTE: Zig has a native `u21`; `maxInt(u21)` is `(1 << 21) - 1`, so
    // `start` is `1 << 21` (the first codepoint above U+10FFFF's 21-bit range).
    pub const START: u32 = ((1u32 << 21) - 1) + 1;
    // pub const end: u32 = std.math.maxInt(u32);
    pub const END: u32 = u32::MAX;
}

// test {
//     const testing = std.testing;
//     try testing.expectEqual(start, @intFromEnum(Sprite.underline));
// }
#[cfg(test)]
mod sprite_tests {
    use super::*;

    #[test]
    fn sprite_start() {
        // try testing.expectEqual(start, @intFromEnum(Sprite.underline));
        assert_eq!(Sprite::START, Sprite::Underline as u32);
    }
}

// test {
//     @import("std").testing.refAllDecls(@This());
// }
//
// PORT NOTE: `refAllDecls` is a Zig compile-time smoke test that every public
// declaration type-checks. Rust's module system and per-item compilation give
// this coverage inherently, so the reference root test block has no direct
// port; the submodules are declared above.
