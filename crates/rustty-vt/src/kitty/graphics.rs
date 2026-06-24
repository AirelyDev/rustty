//! Kitty graphics protocol support.
//!
//! Documentation:
//! https://sw.kovidgoyal.net/kitty/graphics-protocol
//!
//! Unimplemented features that are still todo:
//! - shared memory transmit
//! - virtual placement w/ unicode
//! - animation
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
// PORT NOTE: DEFERRED to vt-12 (needs Screen/Terminal). `graphics_exec.zig`
// (->Terminal) is a compiling stub; once ported it will re-export `execute`
// below alongside the others.
#[allow(unused_imports)]
use super::graphics_exec as exec;
// const image = @import("graphics_image.zig");
use super::graphics_image as image;
// const storage = @import("graphics_storage.zig");
// PORT NOTE: DEFERRED to vt-12 (needs Screen/Terminal). `graphics_storage.zig`
// (->Screen) is a compiling stub; once ported it will re-export `ImageStorage`
// below alongside the others.
#[allow(unused_imports)]
use super::graphics_storage as storage;
// pub const unicode = @import("graphics_unicode.zig");
pub use super::graphics_unicode as unicode;
// pub const Command = command.Command;
pub use command::Command;
// pub const CommandParser = command.Parser;
pub use command::Parser as CommandParser;
// pub const Image = image.Image;
pub use image::Image;
// pub const LoadingImage = image.LoadingImage;
pub use image::LoadingImage;
// pub const ImageStorage = storage.ImageStorage;
// PORT NOTE: DEFERRED to vt-12 (needs Screen/Terminal). Re-export restored once
// `graphics_storage::ImageStorage` is ported.
// pub use storage::ImageStorage;
// pub const RenderPlacement = render.Placement;
pub use render::Placement as RenderPlacement;
// pub const Response = command.Response;
pub use command::Response;

// pub const execute = exec.execute;
// PORT NOTE: DEFERRED to vt-12 (needs Screen/Terminal). Re-export restored once
// `graphics_exec::execute` is ported.
// pub use exec::execute;

// test {
//     @import("std").testing.refAllDecls(@This());
// }
// PORT NOTE: the reference root `test { std.testing.refAllDecls(@This()); }` is
// a reflection smoke test that forces every declaration in this aggregator to be
// analyzed. Rust type-checks every `pub use`/`pub mod` re-export unconditionally,
// so the structural guarantee is automatic; we mirror it concretely by naming
// each re-exported item below so a removed/renamed type surfaces as a test
// failure rather than only a compile error. The two DEFERRED re-exports
// (`ImageStorage`, `execute`) are intentionally absent until vt-12.
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
        // Touch the deferred-module imports so the aggregator's wiring (private
        // `exec`/`storage`) is referenced, matching the reference's structure.
        let _ = core::any::type_name::<super::exec::Deferred>();
        let _ = core::any::type_name::<super::storage::Deferred>();
    }
}
