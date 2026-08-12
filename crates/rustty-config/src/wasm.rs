//! PORTS: ghostty `src/config/Wasm.zig` — the browser/wasm `Config` C ABI
//! (`config_new`/`config_free`/`config_load_string`/`config_finalize`).
//!
//! PORT STATUS: ported (browser-gated). The reference module is only ever
//! compiled into the wasm build (`@import("../os/wasm.zig").alloc`, the
//! `comptime` `isWasm` gate elsewhere); it has 0 `test {}` blocks (Phase-0
//! inventory contract). It is feature-gated behind an off-by-default `wasm`
//! cargo feature — the desktop config cluster never compiles it. The gate is
//! FLIPPED ON by `rustty-wasm` (WS14), so these four exports ship in
//! `target/wasm32-unknown-unknown/release/rustty_wasm.wasm` today; the artifact
//! shape (these exports + the `env.log` import the error arms below log through)
//! is asserted by `crates/rustty-wasm/tests/artifact_shape.rs`.
//!
//! The faithful port body is recorded below under `#[cfg(feature = "wasm")]`. It
//! mirrors `Wasm.zig` 1:1: `config_new`/`config_free` heap the `Config` on the
//! wasm allocator, `config_load_string` runs the file-syntax line parser over a
//! string, and `config_finalize` calls `Config.finalize`. The `rustty_` prefix
//! is NOT applied here — these wasm symbols (`config_new` etc.) are unprefixed in
//! the reference because they are bound by the JS host by their bare names.

// const std = @import("std");
// const wasm = @import("../os/wasm.zig");
// const cli = @import("../cli.zig");
// const alloc = wasm.alloc;
//
// const Config = @import("Config.zig");
//
// const log = std.log.scoped(.config);

// PORT NOTE: `pub` (not private) so the wasm module entry (`rustty-wasm`, the
// port of `main_wasm.zig`) can name these C-ABI exports in its `#[used]`
// fn-pointer retention array — the Rust analog of `comptime { _ =
// @import("config.zig").Wasm; }` which force-retains the `export fn`s. The
// symbols are still emitted unmangled (`config_new` etc.) for the JS host.
#[cfg(feature = "wasm")]
pub mod wasm_exports {
    use crate::config_struct::Config;

    /// `const log = std.log.scoped(.config);` + `log.err(...)`.
    ///
    /// PORT NOTE: `std.log`'s sink in the wasm build is `main_wasm.zig`'s
    /// `std_options.logFn = @import("os/wasm/log.zig").log`, which formats
    /// `level_txt ++ "(" ++ @tagName(scope) ++ "): " ++ msg` and hands it to the
    /// imported `env.log`. Rust has no global `std.log` hook, so the scoped-`err`
    /// call is rendered as a direct call into that same ported sink
    /// (`rustty_os::wasm::log::freestanding::log`) with the `.err` level text
    /// (`std.log.Level.asText()` == `"error"`) and the `.config` scope. Host
    /// (non-wasm) builds of this feature keep `eprintln!` — there is no `env.log`
    /// import to route to.
    fn log_err(msg: &str) {
        #[cfg(target_arch = "wasm32")]
        rustty_os::wasm::log::freestanding::log("error", "config", msg);
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!("{msg}");
    }

    /// Create a new configuration filled with the initial default values.
    // export fn config_new() ?*Config {
    #[no_mangle]
    pub extern "C" fn config_new() -> *mut Config {
        // const result = alloc.create(Config) catch ...;
        // result.* = Config.default(alloc) catch ...;
        Box::into_raw(Box::new(Config::default_config()))
    }

    // export fn config_free(ptr: ?*Config) void {
    /// # Safety
    /// `ptr` must be null or a live pointer from `config_new`.
    #[no_mangle]
    pub unsafe extern "C" fn config_free(ptr: *mut Config) {
        // if (ptr) |v| { v.deinit(); alloc.destroy(v); }
        if !ptr.is_null() {
            let mut v = unsafe { Box::from_raw(ptr) };
            v.deinit();
        }
    }

    /// Load the configuration from a string in the same format as
    /// the file-based syntax for the desktop version of the terminal.
    // export fn config_load_string(self: *Config, str: [*]const u8, len: usize) void {
    /// # Safety
    /// `self` must be a live `Config` pointer and `str`/`len` a readable range.
    #[no_mangle]
    pub unsafe extern "C" fn config_load_string(self_: *mut Config, str_: *const u8, len: usize) {
        // config_load_string_(self, str[0..len]) catch |err| { log.err(...); };
        let s = unsafe { std::slice::from_raw_parts(str_, len) };
        if let Err(err) = config_load_string_(unsafe { &mut *self_ }, s) {
            log_err(&format!("error loading config err={err:?}"));
        }
    }

    // fn config_load_string_(self: *Config, str: []const u8) !void {
    fn config_load_string_(
        self_: &mut Config,
        str_: &[u8],
    ) -> Result<(), rustty_args::args::ParseError> {
        // var fbs = std.io.fixedBufferStream(str);
        // var iter = cli.args.lineIterator(fbs.reader());
        // try cli.args.parse(Config, alloc, self, &iter);
        let mut iter = rustty_args::args::LineIterator::init(str_);
        self_.load_iter(&mut iter)
    }

    // export fn config_finalize(self: *Config) void {
    /// # Safety
    /// `self` must be a live `Config` pointer.
    #[no_mangle]
    pub unsafe extern "C" fn config_finalize(self_: *mut Config) {
        // self.finalize() catch |err| { log.err(...); };
        if let Err(err) = unsafe { &mut *self_ }.finalize() {
            log_err(&format!("error finalizing config err={err:?}"));
        }
    }
}
