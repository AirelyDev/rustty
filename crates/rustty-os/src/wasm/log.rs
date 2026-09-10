//! rustty-os::wasm::log
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/wasm/log.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std` and `wasm = @import("../wasm.zig")`. The
// implementation is wasm-only (freestanding browser target) and calls an
// `extern "env" fn log`. In Rust the module body is `cfg(target_arch = "wasm32")`;
// `std.log`'s level/scope formatting maps to a formatted byte string handed to
// the imported `env.log`.
#![allow(dead_code)]

// Use the correct implementation
#[cfg(target_arch = "wasm32")]
pub use freestanding::log;

/// A log function that discards everything. Since the format string and
/// scope are comptime parameters, using this compiles out all logging
/// machinery: call sites, format strings, and the std.fmt code they
/// reference (~220KB in a ReleaseFast wasm build).
//
// pub fn noop(
//     comptime level: std.log.Level,
//     comptime scope: @TypeOf(.EnumLiteral),
//     comptime format: []const u8,
//     args: anytype,
// ) void {
//     _ = level;
//     _ = scope;
//     _ = format;
//     _ = args;
// }
//
// PORT NOTE (`d61920d80`): Rust has no comptime log sink, so a call to this
// runtime `noop` cannot itself compile the caller's format machinery out. The
// release-wasm arm of the installed sink (`rustty-vt-c` `sys::log_fn`)
// therefore returns BEFORE formatting — that early return is the Rust spelling
// of installing this noop; the function is carried here for file fidelity (R1)
// and takes the same already-formatted-parts signature as `log` above.
pub fn noop(level_txt: &str, scope: &str, formatted: &str) {
    let _ = level_txt;
    let _ = scope;
    let _ = formatted;
}

/// Freestanding implementation calls an extern "log" function.
#[cfg(target_arch = "wasm32")]
pub mod freestanding {
    // The function std.log will call.
    //
    // PORT NOTE: the reference is a `comptime`-generic `fn log(level, scope, format,
    // args)` matching the `std.log` signature. In Rust there is no equivalent
    // comptime log hook; the parity-preserving shape is a function that takes the
    // already-formatted level text, scope, format string, and formatted args and
    // emits the same `level(scope): message` line to the host.
    pub fn log(level_txt: &str, scope: &str, formatted: &str) {
        // The buffer for putting our log message. We try to use a stack-allocated
        // buffer first because we want to avoid allocation. If we are logging
        // an error DUE to an OOM, allocating will of course fail and we'd like
        // to see the error message so we prefer to use this.
        let mut buf = [0u8; 2048];

        // Build the string
        // PORT NOTE: `const prefix = if (scope == .default) ": " else "(" ++ @tagName(scope)
        // ++ "): ";` then `const txt = level_txt ++ prefix ++ format;`.
        let prefix = if scope == "default" {
            ": ".to_string()
        } else {
            format!("({scope}): ")
        };
        let txt = format!("{level_txt}{prefix}{formatted}");

        // Format. We attempt to use a stack-allocated string first and if that
        // fails we'll try to allocate.
        let bytes = txt.as_bytes();
        let str: &[u8] = if bytes.len() <= buf.len() {
            buf[..bytes.len()].copy_from_slice(bytes);
            &buf[..bytes.len()]
        } else {
            bytes
        };

        // Send it over to the JS side
        unsafe { js::log(str.as_ptr(), str.len()) };
    }

    // We wrap our externs in this namespace so we can reuse symbols, otherwise
    // "log" would collide.
    //
    // PORT NOTE: `#[link(wasm_import_module = "env")]` is REQUIRED, not
    // decoration. The reference declares this as `extern "env" fn log(...)`, and
    // Zig's freestanding link has nothing else named `log`. Without the explicit
    // import module, rustc emits a plain undefined symbol named `log`, which
    // `wasm-ld` resolves against the Rust sysroot's libm `log` (`f64 -> f64`):
    // the link then silently produces a `signature_mismatch:log` trap stub and
    // NO `env.log` import at all, so every logged line would trap instead of
    // reaching the host. The sibling wasm externs use the same attribute
    // (`rustty-font` `atlas.rs`, `rustty-font-shaper` `face/web_canvas.rs`).
    mod js {
        #[link(wasm_import_module = "env")]
        extern "C" {
            #[link_name = "log"]
            pub fn log(ptr: *const u8, len: usize);
        }
    }
}

// =====================================================================
// The `log` facade backend, wasm-artifact side (task-14a review round 3,
// finding 3; `.port/parity-closure/DECISION-logging.md` §6r3).
//
// PORT NOTE: no reference analog of its own — `main_wasm.zig`'s
// `std_options.logFn = @import("os/wasm/log.zig").log` is a COMPTIME
// function-pointer swap, active for the whole module from its first
// instruction; every `std.log.scoped(...)` call anywhere in the compiled
// module (`rustty-vt`/`rustty-config`/`rustty-os`/`rustty-args`/
// `rustty-font-shaper`/`rustty-simd` — the crates `cargo tree -p rustty-wasm`
// links) already reaches `freestanding::log` in the oracle. Rust has no such
// hook, so crates that restore a `log.warn(...)` site as `log::warn!(target:
// "<scope>", ...)` need a process-wide `log::Log` registration to reach
// ANYWHERE in this artifact — this is that registration, hosted here (not in
// `rustty-wasm` itself) because `rustty-os` is already a dependency of every
// crate that owns an actual JS-callable entry point in this artifact
// (`rustty-config`'s wasm-feature `config_new`, `rustty-wasm`'s own atlas
// exports), so any of them can call `install_log_backend()` without a
// layering cycle — the same "install from wherever the real entry points
// are" shape `rustty-vt-c::sys::install_log_backend` and `rustty-core::
// logging::install_app_log_backend` use for their own artifacts.
#[cfg(target_arch = "wasm32")]
struct WasmLogger;

/// `level.asText()` — the same 4 strings `freestanding::log`'s `level_txt`
/// parameter expects (see its own call sites in this file). `log::Level`'s
/// 5th level, `Trace`, has no `std.log` equivalent and folds into `"debug"`,
/// matching `rustty-vt-c::sys::std_level_from_log`'s and `rustty-core::
/// logging::level_from_log`'s identical choice.
#[cfg(target_arch = "wasm32")]
fn level_text(level: log::Level) -> &'static str {
    match level {
        log::Level::Error => "error",
        log::Level::Warn => "warning",
        log::Level::Info => "info",
        log::Level::Debug | log::Level::Trace => "debug",
    }
}

#[cfg(target_arch = "wasm32")]
impl log::Log for WasmLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // `log::set_max_level` (in `install_log_backend`) already gates which
        // records reach here; this sink has no independent filter.
        true
    }

    fn log(&self, record: &log::Record) {
        // `record.target()` carries the restored call site's oracle scope
        // (`target: "<scope>"`, `""` for `.default` — `freestanding::log`
        // itself already maps `scope == "default"` specially, so an empty
        // target is translated to the literal string `"default"` here to
        // match that convention exactly).
        let scope = if record.target().is_empty() {
            "default"
        } else {
            record.target()
        };
        freestanding::log(
            level_text(record.level()),
            scope,
            &record.args().to_string(),
        );
    }

    fn flush(&self) {}
}

#[cfg(target_arch = "wasm32")]
static WASM_LOGGER: WasmLogger = WasmLogger;

/// Register [`WasmLogger`] as the process-wide `log` backend for this wasm
/// artifact. `std::sync::Once`-guarded: idempotent, and — like the vt-c and
/// app-build backends — never touches `max_level` when `log::set_logger`
/// fails (a host/embedder already owns the registration), only on success.
///
/// Level default ports `main_wasm.zig`'s own `std_options.log_level`
/// (`.Debug => .debug, .ReleaseSmall => .warn, else => .info`; the Rust build
/// model has no `ReleaseSmall` cfg, so — per this crate's own `LOG_LEVEL`
/// const above (`rustty-wasm/src/lib.rs`) — that arm collapses into the
/// `else => .info` path, same divergence, already documented there).
#[cfg(target_arch = "wasm32")]
pub fn install_log_backend() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if log::set_logger(&WASM_LOGGER).is_ok() {
            let level = if cfg!(debug_assertions) {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            };
            log::set_max_level(level);
        }
        // else: a logger (an embedder's own, or another instance of this
        // one) is already installed — leave its max_level filter alone.
    });
}

#[cfg(test)]
#[cfg(target_arch = "wasm32")]
mod facade_tests {
    use super::*;

    // task-14a review round 3 (rustty-only proving test, NOT a reference
    // test — `WasmLogger` has no oracle analog, see its own PORT NOTE).
    // wasm32-only: exercises the full `log::warn!` -> `WasmLogger::log` ->
    // `level_text` -> `freestanding::log` -> `js::log` chain. Not run by the
    // host `cargo test` (this module is `cfg(target_arch = "wasm32")`); the
    // crate's existing `wasm-host-buildable` split means this test only
    // compiles/runs under a `--target wasm32-unknown-unknown` test harness.
    #[test]
    fn install_log_backend_registers_without_panicking() {
        install_log_backend();
        install_log_backend();
        log::warn!(target: "task14a_wasm_test", "proving message {}", 1);
    }
}
