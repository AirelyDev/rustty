//! Byte-size unit helpers.
//!
//! PORTS: ghostty src/inspector/units.zig
//!
//! A leaf helper module (a sibling of `widgets.zig`/`main.zig` in the reference,
//! reached by the widgets via `@import("../units.zig")`). Consumed by the
//! `page`/`pagelist`/`screen` widgets to render memory figures in KiB.

// pub fn toKibiBytes(bytes: usize) usize {
//     return bytes / 1024;
// }
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn toKibiBytes(bytes: usize) -> usize {
    bytes / 1024
}
