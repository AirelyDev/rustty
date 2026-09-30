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

/// `std.debug.print(fmt, args)`: format to stderr under the stderr lock and
/// drop any write error (`catch return`, Zig 0.16 `std/debug.zig:307-312`).
/// The format string carries its own `\n`, as the reference's does.
//
// PORT NOTE (parity S62-F3/F4/F5, N180): the report sites used `println!`
// (stdout). `eprintln!` would reach stderr but panics on a failed write, which
// `std.debug.print` never does.
macro_rules! debug_print {
    ($($arg:tt)*) => {{
        use ::std::io::Write as _;
        let _ = ::std::io::stderr().lock().write_fmt(format_args!($($arg)*));
    }};
}

// pub const cli = @import("cli.zig");
pub mod cli;

// TEST-ONLY `log::Log` capture backend for the restored `log.*` sites
// (task-14e-2). See the module header — in particular, why it does NOT use
// `log_backend`.
#[cfg(test)]
pub(crate) mod log_capture;

/// The `log::Log` backend the STANDALONE benchmark binary installs
/// (task-14e-2). See the module header for why the crate needs its own.
pub mod log_backend;
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

/// `std.mem.doNotOptimizeAway(val)` for register-width integers (Zig 0.16
/// std/mem.zig: `asm volatile("" :: "r"(val))` — a REGISTER constraint, no
/// store, no memory clobber).
//
// PERF(port) (fidelity fix): `core::hint::black_box` spills its argument to a
// stack slot and hands inline asm a POINTER with a memory clobber — one store
// per call in the per-codepoint bench loops, which the reference does not
// pay. Pointer-valued sinks (`black_box(&x)`) stay `black_box`: Zig's
// `.pointer` arm does use a memory operand there.
#[inline(always)]
pub fn do_not_optimize_away<T: DoNotOptimize>(val: T) {
    val.sink()
}

pub trait DoNotOptimize {
    fn sink(self);
}

macro_rules! reg_sink {
    ($($t:ty),*) => {$(
        impl DoNotOptimize for $t {
            #[inline(always)]
            fn sink(self) {
                // SAFETY: empty asm, one register input, no memory access.
                unsafe {
                    // The template must reference the operand; an assembler
                    // comment emits no instructions.
                    core::arch::asm!("/* {v} */", v = in(reg) self as u64,
                        options(nomem, nostack, preserves_flags));
                }
            }
        }
    )*};
}
reg_sink!(u8, i8, u16, i16, u32, i32, u64, i64, usize, isize);

impl DoNotOptimize for bool {
    #[inline(always)]
    fn sink(self) {
        (self as u8).sink()
    }
}

#[cfg(test)]
mod log_site_census {
    //! task-14e-2 — a crate-wide census of the restored benchmark diagnostics.
    //!
    //! STRUCTURAL, and reported as such. Most of these sites need a real
    //! failure of the thing being benchmarked (a corpus file that cannot be
    //! opened mid-run, an LZ4 compression failure, a snapshot decode failure);
    //! `hyperlink_map`'s two are the only pair with a cheap driver, and they
    //! have behavioural tests. This census pins the rest: exactly one live
    //! emission per reference call, at the reference's LEVEL, under the
    //! reference's SCOPE, with the reference's MESSAGE TEXT.
    //!
    //! Assembled from fragments so no live-looking facade call enters this file
    //! (task-14e-1's LIVE-TREE self-test lesson), and whitespace-collapsed on
    //! both sides so `cargo fmt` wrapping cannot break a correct assertion.

    /// `(reference file, port file, source)` for every mapped benchmark file.
    const SOURCES: &[(&str, &str, &str)] = &[
        (
            "ApcParser.zig",
            "apc_parser.rs",
            include_str!("apc_parser.rs"),
        ),
        ("CApi.zig", "capi.rs", include_str!("capi.rs")),
        (
            "CodepointWidth.zig",
            "codepoint_width.rs",
            include_str!("codepoint_width.rs"),
        ),
        (
            "GraphemeBreak.zig",
            "grapheme_break.rs",
            include_str!("grapheme_break.rs"),
        ),
        (
            "HyperlinkMap.zig",
            "hyperlink_map.rs",
            include_str!("hyperlink_map.rs"),
        ),
        ("IsSymbol.zig", "is_symbol.rs", include_str!("is_symbol.rs")),
        (
            "OscParser.zig",
            "osc_parser.rs",
            include_str!("osc_parser.rs"),
        ),
        (
            "PageCompression.zig",
            "page_compression.rs",
            include_str!("page_compression.rs"),
        ),
        (
            "ScreenClone.zig",
            "screen_clone.rs",
            include_str!("screen_clone.rs"),
        ),
        (
            "ScrollbackCompression.zig",
            "scrollback_compression.rs",
            include_str!("scrollback_compression.rs"),
        ),
        (
            "TerminalFormatter.zig",
            "terminal_formatter.rs",
            include_str!("terminal_formatter.rs"),
        ),
        (
            "TerminalParser.zig",
            "terminal_parser.rs",
            include_str!("terminal_parser.rs"),
        ),
        (
            "TerminalResize.zig",
            "terminal_resize.rs",
            include_str!("terminal_resize.rs"),
        ),
        (
            "TerminalSnapshot.zig",
            "terminal_snapshot.rs",
            include_str!("terminal_snapshot.rs"),
        ),
        (
            "TerminalStream.zig",
            "terminal_stream.rs",
            include_str!("terminal_stream.rs"),
        ),
    ];

    /// `(port file, level, message, how many)` — one row per reference call,
    /// enumerated from the ORACLE with `check_log_reconcile.py`'s own
    /// `zig_sites()` scanner, not from a grep of the port.
    #[allow(clippy::type_complexity)]
    const EXPECTED: &[(&str, &str, &str, usize)] = &[
        // ApcParser.zig:102,130
        (
            "apc_parser.rs",
            "warn",
            "error opening data file err={err}",
            1,
        ),
        (
            "apc_parser.rs",
            "warn",
            "error reading data file err={err:?}",
            1,
        ),
        // CApi.zig:17,26
        ("capi.rs", "warn", "unknown action={}", 1),
        (
            "capi.rs",
            "warn",
            // S61-F8 (task T-D): the action is printed by its tag name.
            "failed to run action={} err={err:?}",
            1,
        ),
        // CodepointWidth.zig:89,110,125,152,184
        (
            "codepoint_width.rs",
            "warn",
            "error opening data file err={err}",
            1,
        ),
        (
            "codepoint_width.rs",
            "warn",
            "wcwidth is not available on Windows",
            1,
        ),
        (
            "codepoint_width.rs",
            "warn",
            "error reading data file err={err:?}",
            3,
        ),
        // GraphemeBreak.zig:78,103,128
        (
            "grapheme_break.rs",
            "warn",
            "error opening data file err={err}",
            1,
        ),
        (
            "grapheme_break.rs",
            "warn",
            "error reading data file err={err:?}",
            2,
        ),
        // HyperlinkMap.zig:48,69 — the only `log.err` pair in the tree.
        (
            "hyperlink_map.rs",
            "error",
            "entries must be a power of two greater than or equal to 16",
            1,
        ),
        (
            "hyperlink_map.rs",
            "error",
            "insufficient map capacity expected_at_least={} actual={}",
            1,
        ),
        // IsSymbol.zig:76,101,128
        (
            "is_symbol.rs",
            "warn",
            "error opening data file err={err}",
            1,
        ),
        (
            "is_symbol.rs",
            "warn",
            "error reading data file err={err:?}",
            2,
        ),
        // OscParser.zig:65
        (
            "osc_parser.rs",
            "warn",
            "error opening data file err={err}",
            1,
        ),
        // PageCompression.zig:203,340,361,376,406,431 (:385 is UNRESTORABLE —
        // an allocation-failure arm; see that site's PORT NOTE).
        (
            "page_compression.rs",
            "warn",
            "failed to prepare page compression benchmark err={err:?}",
            1,
        ),
        (
            "page_compression.rs",
            "warn",
            "page compression failed err={err:?}",
            1,
        ),
        (
            "page_compression.rs",
            "warn",
            "failed to size stored page output err={err:?}",
            1,
        ),
        (
            "page_compression.rs",
            "warn",
            "stored page compression failed err={err:?}",
            1,
        ),
        (
            "page_compression.rs",
            "warn",
            "page decompression failed err={err:?}",
            1,
        ),
        (
            "page_compression.rs",
            "warn",
            "page compression report failed err={err:?}",
            1,
        ),
        // ScreenClone.zig:209,223 + thirteen `error cloning screen` sites
        // (nine before `ca4f719f7`; its renderScrollLoop and stepRenderOutput
        // add four, P7-W3 T3), anchors at 12752b2ac.
        (
            "screen_clone.rs",
            "warn",
            "error opening data file err={err}",
            1,
        ),
        (
            "screen_clone.rs",
            "warn",
            "error reading data file err={err:?}",
            1,
        ),
        (
            "screen_clone.rs",
            "warn",
            "error cloning screen err={err:?}",
            13,
        ),
        // ScrollbackCompression.zig:173
        (
            "scrollback_compression.rs",
            "warn",
            "failed to prepare scrollback compression benchmark err={err:?}",
            1,
        ),
        // TerminalFormatter.zig:189,280,301,361
        (
            "terminal_formatter.rs",
            "warn",
            "failed to prepare formatter benchmark err={err:?}",
            1,
        ),
        (
            "terminal_formatter.rs",
            "warn",
            "formatting failed err={err}",
            2,
        ),
        (
            "terminal_formatter.rs",
            "warn",
            "roundtrip failed err={err:?}",
            1,
        ),
        // TerminalParser.zig:57,88
        (
            "terminal_parser.rs",
            "warn",
            "error opening data file err={err}",
            1,
        ),
        (
            "terminal_parser.rs",
            "warn",
            "error reading data file err={err:?}",
            1,
        ),
        // TerminalResize.zig:232,247,267
        (
            "terminal_resize.rs",
            "warn",
            "error opening data file err={err}",
            1,
        ),
        (
            "terminal_resize.rs",
            "warn",
            "error reading data file err={err:?}",
            1,
        ),
        (
            "terminal_resize.rs",
            "warn",
            "error resizing terminal err={err:?}",
            1,
        ),
        // TerminalSnapshot.zig:145,216,234,256,289
        (
            "terminal_snapshot.rs",
            "warn",
            "failed to prepare snapshot benchmark err={err:?}",
            1,
        ),
        (
            "terminal_snapshot.rs",
            "warn",
            "snapshot encoding failed err={err:?}",
            1,
        ),
        (
            "terminal_snapshot.rs",
            "warn",
            "snapshot decoding failed err={err:?}",
            1,
        ),
        (
            "terminal_snapshot.rs",
            "warn",
            "snapshot READY decoding failed err={err:?}",
            1,
        ),
        (
            "terminal_snapshot.rs",
            "warn",
            "unknown record tag {tag_raw}",
            1,
        ),
        // TerminalStream.zig:107,142
        (
            "terminal_stream.rs",
            "warn",
            "error opening data file err={err}",
            1,
        ),
        (
            "terminal_stream.rs",
            "warn",
            "error reading data file err={err:?}",
            1,
        ),
    ];

    fn collapse_ws(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let mut in_ws = false;
        for c in src.chars() {
            if c.is_whitespace() {
                in_ws = true;
                continue;
            }
            // A space right after `(` is dropped so a `cargo fmt`-wrapped call
            // collapses to the same text as a one-line one.
            if in_ws && !out.is_empty() && !out.ends_with('(') {
                out.push(' ');
            }
            in_ws = false;
            out.push(c);
        }
        out
    }

    #[test]
    fn every_reference_diagnostic_is_live_at_its_level_and_scope() {
        // Each source is whitespace-collapsed once, not once per expected
        // emission (task C-1: re-collapsing every file per entry kept this test
        // past a 20-minute Miri budget; the assertions are unchanged).
        let collapsed: Vec<(&str, String)> = SOURCES
            .iter()
            .map(|(_z, p, s)| (*p, collapse_ws(s)))
            .collect();
        for (_zig, port, level, message, count) in EXPECTED
            .iter()
            .map(|(port, level, message, count)| ("", *port, *level, *message, *count))
        {
            let src = &collapsed
                .iter()
                .find(|(p, _s)| *p == port)
                .unwrap_or_else(|| panic!("no source registered for {port}"))
                .1;
            let macro_name = ["log", "::", level, "!"].concat();
            let wanted = collapse_ws(&format!("{macro_name}(target: LOG_SCOPE, \"{message}\""));
            assert_eq!(
                count,
                src.matches(&wanted).count(),
                "expected {count} live emission(s) of `{wanted}` in {port}"
            );
        }
    }

    /// Every mapped benchmark file declares the reference's own scope tag, and
    /// no file left a `[scope]`-prefixed `eprintln!` behind.
    #[test]
    fn no_facade_bypassing_print_remains() {
        for (zig, port, src) in SOURCES {
            assert!(
                src.contains("const LOG_SCOPE: &str ="),
                "{port} (ports {zig}) declares no LOG_SCOPE"
            );
            assert_eq!(
                0,
                src.matches("eprintln!(\"[{LOG_SCOPE}]").count(),
                "{port} still writes a facade-bypassing print"
            );
        }
    }
}
