//! rustty-vt search
//!
//! PORTS: ghostty src/terminal/search.zig
//!
//! PORT STATUS: complete (vt-15; phase-6 W2). `sliding_window`, `active`,
//! `pagelist`, `screen`, `viewport`, `terminal`, and `thread` are FILLED (with
//! their tests). `terminal` (`TerminalSearch`, oracle `149c9f562`) is the
//! whole-terminal orchestration that the reference's `search.zig` re-exports
//! as `Terminal` — the `lib` artifact's C search API builds on it. The search
//! `thread` mirrors the reference's xev event loop via an in-crate seam backed
//! by `std::thread` (xev itself is an unported, deferred upward dependency — see
//! the PORT NOTE in `thread.rs`).

// pub const options = @import("terminal_options");
//
// pub const Active = @import("search/active.zig").ActiveSearch;
// pub const PageList = @import("search/pagelist.zig").PageListSearch;
// pub const Screen = @import("search/screen.zig").ScreenSearch;
// pub const Terminal = @import("search/terminal.zig").TerminalSearch;
// pub const Viewport = @import("search/viewport.zig").ViewportSearch;
//
// // The search thread is not available in libghostty due to the xev dep
// // for now.
// pub const Thread = switch (options.artifact) {
//     .ghostty => @import("search/Thread.zig"),
//     .lib => void,
// };

pub mod active;
pub mod pagelist;
pub mod screen;
pub mod sliding_window;
pub mod terminal;
pub mod thread;
pub mod viewport;
