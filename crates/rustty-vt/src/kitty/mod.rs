//! rustty-vt kitty
//!
//! PORTS: ghostty src/terminal/kitty.zig
//!
//! PORT STATUS: stub

// const build_options = @import("terminal_options");

// const key = @import("kitty/key.zig");
pub mod color;
pub mod graphics;
pub mod graphics_command;
pub mod graphics_exec;
pub mod graphics_image;
pub mod graphics_render;
pub mod graphics_storage;
pub mod graphics_unicode;
// PORT NOTE: the reference imports `key` privately (`const key = ...`); we keep
// the module `pub` so its types are reachable, matching the existing skeleton.
pub mod key;

// pub const KeyFlags = key.Flags;
pub use key::Flags as KeyFlags;
// pub const KeyFlagStack = key.FlagStack;
pub use key::FlagStack as KeyFlagStack;
// pub const KeySetMode = key.SetMode;
pub use key::SetMode as KeySetMode;
