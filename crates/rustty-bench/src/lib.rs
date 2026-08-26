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
//!     refAllDecls(@This()) }` ports as `tests::ref_all_decls` (workspace
//!     convention — same pattern as rustty-crash / rustty-zf and 30 siblings).
//!
//! Test contract: 8 Zig `test{}` blocks -> 8 `#[test]` fns (Benchmark + the 6
//! benchmark decltests + `ref_all_decls`; ScreenClone has none).

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
// pub const TerminalFormatter = @import("TerminalFormatter.zig");
pub mod terminal_formatter;
// pub const TerminalParser = @import("TerminalParser.zig");
pub mod terminal_parser;
// pub const TerminalResize = @import("TerminalResize.zig");
pub mod terminal_resize;
// pub const TerminalSnapshot = @import("TerminalSnapshot.zig");
pub mod terminal_snapshot;
// pub const IsSymbol = @import("IsSymbol.zig");
pub mod is_symbol;
// pub const PageCompression = @import("PageCompression.zig");
pub mod page_compression;
// pub const ScrollbackCompression = @import("ScrollbackCompression.zig");
pub mod scrollback_compression;
// pub const HyperlinkMap = @import("HyperlinkMap.zig");
pub mod hyperlink_map;
// pub const ApcParser = @import("ApcParser.zig");
pub mod apc_parser;

// PORT NOTE: `main.zig` re-exports the decls above but omits `OscParser` and
// `options` (they are reached only via `cli.Action.Struct`). They are declared
// here as modules so the tree is complete (and `osc-parser` is a real action).
pub mod options;
pub mod osc_parser;

/// Build a `terminal::Options` reproducing the Zig `Terminal.Options` field
/// defaults (§7.5/§7.7): the ScreenClone/TerminalStream benchmarks init with
/// `.{ .rows, .cols }`, relying on the reference struct's field defaults for the
/// rest. rustty's `terminal::Options` has no `Default` impl, so we reproduce the
/// `.ghostty`-artifact defaults exactly (`max_scrollback_bytes = 10_000`, `colors =
/// .default`, `default_modes = .{}`, `kitty_image_storage_limit = 320MB`,
/// `kitty_image_loading_limits = .direct`).
pub(crate) fn terminal_options(rows: u16, cols: u16) -> rustty_vt::terminal::Options {
    rustty_vt::terminal::Options {
        // .cols = opts.@"terminal-cols",
        cols,
        // .rows = opts.@"terminal-rows",
        rows,
        // max_scrollback_bytes: usize = 10_000,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        // colors: Colors = .default,
        colors: rustty_vt::terminal::Colors::default(),
        // default_modes: modespkg.ModePacked = .{},
        default_modes: rustty_vt::modes::ModePacked::default(),
        default_cursor_style: rustty_vt::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        // kitty_image_storage_limit: usize = 320 * 1000 * 1000, // (.ghostty)
        kitty_image_storage_limit: 320 * 1000 * 1000,
        // kitty_image_loading_limits: ... = .direct,
        kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
    }
}

// test {
//     @import("std").testing.refAllDecls(@This());
// }
#[cfg(test)]
mod tests {
    /// Ports `main.zig`'s anonymous `refAllDecls(@This())` block (workspace
    /// `ref_all_decls` convention): compile-touch every pub decl of this hub
    /// module so the crate surface is exercised by the test build. Nothing is
    /// run — exactly what the Zig reify test achieves.
    #[test]
    fn ref_all_decls() {
        // pub const cli = @import("cli.zig");
        let _ = core::any::type_name::<crate::cli::Action>();
        // pub const Benchmark = @import("Benchmark.zig");
        let _ = core::any::type_name::<crate::benchmark::Benchmark>();
        // pub const CApi = @import("CApi.zig"); (its sole decl is the export fn)
        let _: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_char) -> bool =
            crate::capi::rustty_benchmark_cli;
        // pub const TerminalStream = @import("TerminalStream.zig");
        let _ = core::any::type_name::<crate::terminal_stream::TerminalStream>();
        // pub const CodepointWidth = @import("CodepointWidth.zig");
        let _ = core::any::type_name::<crate::codepoint_width::CodepointWidth>();
        // pub const GraphemeBreak = @import("GraphemeBreak.zig");
        let _ = core::any::type_name::<crate::grapheme_break::GraphemeBreak>();
        // pub const ScreenClone = @import("ScreenClone.zig");
        let _ = core::any::type_name::<crate::screen_clone::ScreenClone>();
        // pub const TerminalFormatter = @import("TerminalFormatter.zig");
        let _ = core::any::type_name::<crate::terminal_formatter::TerminalFormatter>();
        // pub const TerminalParser = @import("TerminalParser.zig");
        let _ = core::any::type_name::<crate::terminal_parser::TerminalParser>();
        // pub const TerminalResize = @import("TerminalResize.zig");
        let _ = core::any::type_name::<crate::terminal_resize::TerminalResize>();
        // pub const TerminalSnapshot = @import("TerminalSnapshot.zig");
        let _ = core::any::type_name::<crate::terminal_snapshot::TerminalSnapshot>();
        // pub const IsSymbol = @import("IsSymbol.zig");
        let _ = core::any::type_name::<crate::is_symbol::IsSymbol>();
        // pub const PageCompression = @import("PageCompression.zig");
        let _ = core::any::type_name::<crate::page_compression::PageCompression>();
        // pub const ScrollbackCompression = @import("ScrollbackCompression.zig");
        let _ = core::any::type_name::<crate::scrollback_compression::ScrollbackCompression>();
        // pub const HyperlinkMap = @import("HyperlinkMap.zig");
        let _ = core::any::type_name::<crate::hyperlink_map::HyperlinkMap>();
        // pub const ApcParser = @import("ApcParser.zig");
        let _ = core::any::type_name::<crate::apc_parser::ApcParser>();
        // `options` / `osc_parser` are not re-exported by `main.zig` (see the
        // PORT NOTE above) but are pub mods of this hub, so touch them too.
        let _ = core::any::type_name::<crate::options::DataFile>();
        let _ = core::any::type_name::<crate::osc_parser::OscParser>();
    }
}
