//! rustty-vt search
//!
//! PORTS: ghostty src/terminal/search.zig
//!
//! PORT STATUS: in-progress (vt-15). `sliding_window`, `active`, `pagelist`,
//! `screen`, and `viewport` are FILLED (with their tests); `thread` is still a
//! stub.

pub mod active;
pub mod pagelist;
pub mod screen;
pub mod sliding_window;
pub mod thread;
pub mod viewport;
