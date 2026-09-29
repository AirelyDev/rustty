//! Types and functions related to Kitty protocols.
//!
//! PORTS: ghostty src/terminal/kitty.zig
//!
//! PORT STATUS: done

// const build_options = @import("terminal_options");
// PORT NOTE: the reference reads the build-generated `terminal_options` module;
// in Rustty that is `crate::options` (see options.rs). The `kitty_graphics` gate
// is `crate::options::KITTY_GRAPHICS`.

// const key = @import("kitty/key.zig");
// PORT NOTE: the reference imports `key` privately (`const key = ...`); we keep
// the module `pub` so its types are reachable, matching the existing skeleton.
pub mod key;
// pub const clipboard = @import("kitty/clipboard.zig");
// PORT NOTE (B5): `clipboard.zig` privately imports its four sibling files
// (`clipboard_command.zig`, `clipboard_grants.zig`, `clipboard_response.zig`,
// `clipboard_write.zig`); like the `graphics` aggregate below, those
// submodules are declared here so they are reachable, with `clipboard`
// re-exporting the reference's public surface.
pub mod clipboard;
pub mod clipboard_command;
pub mod clipboard_grants;
pub mod clipboard_response;
pub mod clipboard_write;
// pub const color = @import("kitty/color.zig");
pub mod color;
// pub const dnd = @import("kitty/dnd.zig");
// PORT NOTE: `dnd.zig` privately imports its submodules; Rust's module tree
// requires them declared here (matching the flat `graphics_*` layout below).
// `dnd_test.zig` is included via dnd.zig's `test` block upstream; here it is
// the test-only sibling module `dnd_test`.
pub mod dnd;
pub mod dnd_command;
pub mod dnd_drop;
pub mod dnd_response;
#[cfg(test)]
mod dnd_test;
// pub const graphics = if (build_options.kitty_graphics) @import("kitty/graphics.zig") else struct {};
// PORT NOTE: Zig conditionally aliases `graphics` to the module or an empty
// `struct {}` based on the comptime `build_options.kitty_graphics` flag. Rust
// cannot conditionally make a module empty via a `const` alias, so the module is
// always declared and compiled; the `kitty_graphics` gate (`crate::options::
// KITTY_GRAPHICS`) is enforced at the call sites that dispatch into it (the same
// places the reference threads `if (comptime build_options.kitty_graphics)`),
// preserving the "empty when disabled" behavior at usage. The submodules that
// `graphics.zig` aggregates are declared here so they are reachable.
pub mod graphics;
pub mod graphics_animation;
pub mod graphics_command;
pub mod graphics_exec;
pub mod graphics_image;
pub mod graphics_pixel;
pub mod graphics_render;
pub mod graphics_storage;
pub mod graphics_unicode;
// PORT NOTE (`0b5446364`): `graphics_image.zig` imports `windows.zig`
// privately (`const kitty_windows = @import("windows.zig");`) and pulls its
// tests in with `test { _ = kitty_windows; }`. Rust's module tree needs it
// declared here, next to the other `graphics_*` siblings; it stays
// crate-private as the reference's import is.
pub(crate) mod windows;

// pub const KeyFlags = key.Flags;
pub use key::Flags as KeyFlags;
// pub const KeyFlagStack = key.FlagStack;
pub use key::FlagStack as KeyFlagStack;
// pub const KeySetMode = key.SetMode;
pub use key::SetMode as KeySetMode;

// test {
//     @import("std").testing.refAllDecls(@This());
// }
// PORT NOTE: the reference root `test { std.testing.refAllDecls(@This()); }`
// forces every declaration in this aggregator (`key`, `color`, `graphics`, and
// the `KeyFlags`/`KeyFlagStack`/`KeySetMode` re-exports) to be analyzed. Rust
// type-checks every `pub mod`/`pub use` item unconditionally, so the structural
// guarantee is automatic; we mirror it concretely by naming each member so a
// removed/renamed declaration surfaces as a test failure rather than only a
// compile error.
#[cfg(test)]
mod tests {
    #[test]
    fn ref_all_decls() {
        // Reference each aggregated member so its symbol must exist with the
        // expected shape, mirroring `refAllDecls` over this module's decls.
        let _ = core::any::type_name::<super::KeyFlags>();
        let _ = core::any::type_name::<super::KeyFlagStack>();
        let _ = core::any::type_name::<super::KeySetMode>();
        let _ = core::any::type_name::<super::clipboard::WriteState>();
        let _ = core::any::type_name::<super::color::Kind>();
        let _ = core::any::type_name::<super::dnd::State>();
        let _ = core::any::type_name::<super::dnd::Event>();
        // `graphics` aggregator is reachable (gated at call sites by
        // `crate::options::KITTY_GRAPHICS`, matching `build_options.kitty_graphics`).
        let _ = core::mem::size_of::<super::graphics::Command>();
    }
}
