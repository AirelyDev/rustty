//! Kitty graphics protocol support.
//!
//! Documentation:
//! https://sw.kovidgoyal.net/kitty/graphics-protocol
//!
//! Performance:
//! The performance of this particular subsystem of Ghostty is not great.
//! We can avoid a lot more allocations, we can replace some C code (which
//! implicitly allocates) with native Zig, we can improve the data structures
//! to avoid repeated lookups, etc. I tried to avoid pessimization but my
//! aim to ship a v1 of this implementation came at some cost. I learned a lot
//! though and I think we can go back through and fix this up.
//!
//! PORTS: ghostty src/terminal/kitty/graphics.zig
//!
//! PORT STATUS: done

// const render = @import("graphics_render.zig");
use super::graphics_render as render;
// const command = @import("graphics_command.zig");
use super::graphics_command as command;
// const exec = @import("graphics_exec.zig");
use super::graphics_exec as exec;
// const image = @import("graphics_image.zig");
use super::graphics_image as image;
// const storage = @import("graphics_storage.zig");
// CORRECTION (parity S185-F1): a PORT NOTE here said "DEFERRED to vt-12
// (needs Screen/Terminal). `graphics_storage.zig` (->Screen) is a compiling
// stub; once ported it will re-export `ImageStorage` below alongside the
// others." The module is ported and `ImageStorage` is re-exported below.
use super::graphics_storage as storage;
// pub const animation = @import("graphics_animation.zig");
pub use super::graphics_animation as animation;
// pub const pixel = @import("graphics_pixel.zig");
pub use super::graphics_pixel as pixel;
// pub const unicode = @import("graphics_unicode.zig");
pub use super::graphics_unicode as unicode;
// pub const Animation = animation.Animation;
pub use animation::Animation;
// pub const Command = command.Command;
pub use command::Command;
// pub const CommandParser = command.Parser;
pub use command::Parser as CommandParser;
// pub const Image = image.Image;
pub use image::Image;
// pub const LoadingImage = image.LoadingImage;
pub use image::LoadingImage;
// pub const ImageStorage = storage.ImageStorage;
pub use storage::ImageStorage;

// pub const nextGeneration = storage.nextGeneration;
pub use storage::next_generation;
// pub const RenderPlacement = render.Placement;
pub use render::Placement as RenderPlacement;
// pub const Response = command.Response;
pub use command::Response;

// pub const execute = exec.execute;
pub use exec::execute;

// test {
//     @import("std").testing.refAllDecls(@This());
// }
// PORT NOTE: the reference root `test { std.testing.refAllDecls(@This()); }` is
// a reflection smoke test that forces every declaration in this aggregator to be
// analyzed. Rust type-checks every `pub use`/`pub mod` re-export unconditionally,
// so the structural guarantee is automatic; we mirror it concretely by naming
// each re-exported item below so a removed/renamed type surfaces as a test
// failure rather than only a compile error.
#[cfg(test)]
mod tests {
    #[test]
    fn ref_all_decls() {
        // Reference each available re-exported declaration so its symbol must
        // exist with the expected shape, mirroring `refAllDecls` over the
        // aggregator's members.
        let _ = super::CommandParser::init as fn(usize) -> super::CommandParser;
        let _ = super::Image::withoutData as fn(&super::Image) -> super::Image;
        let _ = super::unicode::placement_iterator;
        let _ = super::RenderPlacement::from_top_left;
        // Touch the `Command`/`Response`/`LoadingImage` re-exports so their
        // paths are exercised.
        let _ = core::mem::size_of::<super::Command>();
        let _ = core::mem::size_of::<super::Response>();
        let _ = core::any::type_name::<super::LoadingImage>();
        // Reference the now-ported `ImageStorage` re-export so its symbol must
        // exist, mirroring `refAllDecls` over the `ImageStorage` member.
        let _ = core::any::type_name::<super::ImageStorage>();
        // Reference the now-ported `execute` re-export (from `graphics_exec`) so
        // its symbol must exist with the expected shape, mirroring `refAllDecls`
        // over the aggregator's `execute` member.
        let _ = super::execute
            as fn(
                crate::alloc::Global,
                &mut crate::terminal::Terminal,
                &super::Command,
            ) -> Option<super::Response>;
    }
}
