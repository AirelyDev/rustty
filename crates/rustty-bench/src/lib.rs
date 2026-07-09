//! rustty-bench
//!
//! PORTS: ghostty src/benchmark/ (Benchmark, CApi, CodepointWidth, GraphemeBreak,
//! IsSymbol, OscParser, ScreenClone, TerminalParser, TerminalStream, cli, main,
//! options). Runs against phase-0 BASELINE.json (perf = Goal-2 gradient).
//!
//! PORT STATUS: done
//!
//! R1: every line/comment/test{} ported assertion-for-assertion. The runner's
//! type-erased `ptr + VTable` fn-ptr vtable is modeled as a raw-pointer
//! trampoline (§4), NOT a `dyn Trait`, preserving the `*anyopaque` + per-callee
//! `@ptrCast(@alignCast)` structure and staying Miri-clean.
//!
//! Module map (`main.zig`'s re-export hub becomes this `lib.rs`):
//!   - `main.zig`         -> this `lib.rs` (`pub mod` each). Its `test {
//!     refAllDecls(@This()) }` is DROPPED — a Zig reflection idiom with no Rust
//!     analogue; `cargo build --all-targets` + the workspace test run is the
//!     "all decls compile" equivalent. This makes `rust_test_count =
//!     zig_test_count - 1` for the `lib.rs`/`main.zig` module (recorded for
//!     `check_test_counts.py`; DEFERRALS.md).
//!
//! Test contract: the 8 Zig `test{}` blocks -> 7 `#[test]` fns (Benchmark + the 6
//! benchmark decltests; ScreenClone has none) + the dropped `refAllDecls`.

// pub const cli = @import("cli.zig");
pub mod cli;
// pub const Benchmark = @import("Benchmark.zig");
pub mod benchmark;
// pub const CApi = @import("CApi.zig");
pub mod capi;
// pub const TerminalStream = @import("TerminalStream.zig");
pub mod terminal_stream;
// pub const CodepointWidth = @import("CodepointWidth.zig");
pub mod codepoint_width;
// pub const GraphemeBreak = @import("GraphemeBreak.zig");
pub mod grapheme_break;
// pub const ScreenClone = @import("ScreenClone.zig");
pub mod screen_clone;
// pub const TerminalParser = @import("TerminalParser.zig");
pub mod terminal_parser;
// pub const IsSymbol = @import("IsSymbol.zig");
pub mod is_symbol;

// PORT NOTE: `main.zig` re-exports the decls above but omits `OscParser` and
// `options` (they are reached only via `cli.Action.Struct`). They are declared
// here as modules so the tree is complete (and `osc-parser` is a real action).
pub mod options;
pub mod osc_parser;

// test { @import("std").testing.refAllDecls(@This()); }
//   -> DROPPED (see the module doc above / DEFERRALS.md).

/// Build a `terminal::Options` reproducing the Zig `Terminal.Options` field
/// defaults (§7.5/§7.7): the ScreenClone/TerminalStream benchmarks init with
/// `.{ .rows, .cols }`, relying on the reference struct's field defaults for the
/// rest. rustty's `terminal::Options` has no `Default` impl, so we reproduce the
/// `.ghostty`-artifact defaults exactly (`max_scrollback = 10_000`, `colors =
/// .default`, `default_modes = .{}`, `kitty_image_storage_limit = 320MB`,
/// `kitty_image_loading_limits = .direct`).
pub(crate) fn terminal_options(rows: u16, cols: u16) -> rustty_vt::terminal::Options {
    rustty_vt::terminal::Options {
        // .cols = opts.@"terminal-cols",
        cols,
        // .rows = opts.@"terminal-rows",
        rows,
        // max_scrollback: usize = 10_000,
        max_scrollback: 10_000,
        // colors: Colors = .default,
        colors: rustty_vt::terminal::Colors::default(),
        // default_modes: modespkg.ModePacked = .{},
        default_modes: rustty_vt::modes::ModePacked::default(),
        // kitty_image_storage_limit: usize = 320 * 1000 * 1000, // (.ghostty)
        kitty_image_storage_limit: 320 * 1000 * 1000,
        // kitty_image_loading_limits: ... = .direct,
        kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
    }
}
