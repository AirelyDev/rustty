//! rustty-wasm — the WASM/web target module.
//!
//! PORTS: ghostty `src/main_wasm.zig`.
//!
//! This is the main file for the WASM module. The WASM module has to
//! export a C ABI compatible API.
//!
//! PORT STATUS: ported (host-buildable rlib; the wasm32 cdylib link is gated on
//! the B2 cross-workspace wasm32 gating — see the workstream report). The
//! reference's `comptime { _ = @import(...); }` retention block is modelled by
//! (a) the crate DEPENDENCY edges (`rustty-os`/`rustty-config`/`rustty-vt`/
//! `rustty-core` — the `@import` analog that pulls each module into the link)
//! plus (b) the `#[used]` fn-pointer retention statics below that pin the
//! `config_*` C-ABI exports so `--gc-sections`/`wasm-opt` cannot drop them.

// PORT: `const std = @import("std");` / `const builtin = @import("builtin");`.
// The Rust std/builtins are used directly; the optimization-mode switch below
// reads `rustty_build_config` (the `builtin.mode` analog).
#![allow(dead_code)]

// PORT:
//   comptime {
//       _ = @import("os/wasm.zig");
//       _ = @import("font/main.zig");
//       _ = @import("terminal/main.zig");
//       _ = @import("config.zig").Wasm;
//       _ = @import("App.zig").Wasm;
//   }
//
// The `_ = @import(...)` idiom force-ANALYZES a module so its `export fn`s are
// retained in the wasm artifact. In Rust two mechanisms combine:
//
//   * The crate DEPENDENCY edges on `rustty-os` (`os/wasm.zig`), `rustty-vt`
//     (`terminal/main.zig`), and `rustty-core` (`App.zig`) pull those modules
//     into the link — the `_ = @import` analog. (`font/main.zig`'s wasm export
//     block is BLOCKED — B1, the `web_canvas` shaper+face — and recorded as a
//     documented-deferral in `rustty-font-shaper::shape::web_canvas`; it is NOT
//     depended on here, so nothing selects that unportable arm.)
//
//   * `#[no_mangle]` symbols in an rlib DEPENDENCY are dropped by the linker
//     unless referenced, and `pub use` does NOT re-export a symbol from a
//     cdylib. So the four `config.zig`.`Wasm` C-ABI exports are pinned by the
//     `#[used]` retention tuple below (the faithful `_ = ...Wasm` analog).
//
// The `config.zig`.`Wasm` exports are compiled unconditionally here because the
// `rustty-config` dependency is declared with `features = ["wasm"]` (Cargo.toml),
// so `rustty_config::wasm::wasm_exports` is always present in this crate's link.
mod retain {
    use rustty_config::wasm::wasm_exports::{
        config_finalize, config_free, config_load_string, config_new,
    };
    use rustty_config::Config;

    // A `#[used]` static of the four `config_*` C-ABI function pointers. `#[used]`
    // forbids dead-symbol elimination of this static, and its initializers name
    // each export — together that keeps the unmangled `config_new`/`config_free`/
    // `config_load_string`/`config_finalize` symbols in the emitted `.wasm`.
    #[used]
    static RETAIN_CONFIG_WASM_EXPORTS: (
        extern "C" fn() -> *mut Config,
        unsafe extern "C" fn(*mut Config),
        unsafe extern "C" fn(*mut Config, *const u8, usize),
        unsafe extern "C" fn(*mut Config),
    ) = (config_new, config_free, config_load_string, config_finalize);
}

/// The `std.log.Level` analog for the wasm module's compile-time log level.
// PORT: `std.log.Level` — the level enum `std_options.log_level` selects from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Err,
}

// PORT:
//   pub const std_options = struct {
//       // Set our log level. We try to get as much logging as possible but in
//       // ReleaseSmall mode where we're optimizing for space, we elevate the
//       // log level.
//       pub const log_level: std.log.Level = switch (builtin.mode) {
//           .Debug => .debug,
//           .ReleaseSmall => .warn,
//           else => .info,
//       };
//
//       // Set our log function
//       pub const logFn = @import("os/wasm/log.zig").log;
//   };
//
// Rust has no global `std_options`; the level is a plain const and the log sink
// is the `os/wasm/log` freestanding fn (installed/retained below). Per the
// build-config precedent (`mode_string`), the Rust build model distinguishes
// only `Debug` from optimized (`ReleaseFast`) — there is no `ReleaseSmall`
// cfg — so the `.ReleaseSmall => .warn` arm collapses into the `else => .info`
// path (documented divergence; a debug build still reports `.debug`).

/// Set our log level. We try to get as much logging as possible but in
/// ReleaseSmall mode where we're optimizing for space, we elevate the
/// log level.
pub const LOG_LEVEL: LogLevel = if cfg!(debug_assertions) {
    // .Debug => .debug
    LogLevel::Debug
} else {
    // .ReleaseSmall => .warn, else => .info — see the note above: the Rust
    // build model does not surface `ReleaseSmall`, so optimized builds take the
    // `.info` arm.
    LogLevel::Info
};

// Set our log function.
//
// PORT: `pub const logFn = @import("os/wasm/log.zig").log;`. The freestanding
// wasm log sink lives in `rustty_os::wasm::log::freestanding::log` and is only
// compiled for `wasm32` (it calls the imported `env.log`). Installing it as the
// process-wide sink is a wasm-only concern; here we RETAIN its symbol (the
// `logFn` binding analog) so it is present in the wasm artifact for the JS host
// to drive. On host builds the freestanding sink does not exist, so this is
// `wasm32`-gated.
#[cfg(target_arch = "wasm32")]
mod log_sink {
    /// The installed log function: `os/wasm/log`'s freestanding sink.
    // PORT: `pub const logFn = @import("os/wasm/log.zig").log;` — the binding
    // that routes formatted log lines to the host's imported `env.log`.
    #[used]
    static RETAIN_LOG_FN: fn(&str, &str, &str) = rustty_os::wasm::log::freestanding::log;
}
