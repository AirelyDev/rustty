//! rustty-vt search
//!
//! PORTS: ghostty src/terminal/search.zig
//!
//! PORT STATUS: in-progress (vt-15). `sliding_window` is FILLED; the remaining
//! submodules (active/pagelist/screen/viewport/thread) are still stubs.

pub mod active;
pub mod pagelist;
pub mod screen;
pub mod sliding_window;
pub mod thread;
pub mod viewport;
