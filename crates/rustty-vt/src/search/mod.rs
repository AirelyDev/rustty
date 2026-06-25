//! rustty-vt search
//!
//! PORTS: ghostty src/terminal/search.zig
//!
//! PORT STATUS: complete (vt-15). `sliding_window`, `active`, `pagelist`,
//! `screen`, `viewport`, and `thread` are FILLED (with their tests). The search
//! `thread` mirrors the reference's xev event loop via an in-crate seam backed
//! by `std::thread` (xev itself is an unported, deferred upward dependency — see
//! the PORT NOTE in `thread.rs`).

pub mod active;
pub mod pagelist;
pub mod screen;
pub mod sliding_window;
pub mod thread;
pub mod viewport;
