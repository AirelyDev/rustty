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

/// The message segments `freestanding::log` emits, in order.
///
/// PORT NOTE (parity S113-F5): factored out of `freestanding::log` — which is
/// `cfg(target_arch = "wasm32")` and therefore untestable on the host — so the
/// assembled bytes and the 2048-byte stack-buffer boundary can be pinned by a
/// unit test. The reference builds the same three pieces at comptime:
/// `const prefix = if (scope == .default) ": " else "(" ++ @tagName(scope) ++ "): ";`
/// `const txt = level_txt ++ prefix ++ format;`
pub(crate) fn message_parts<'a>(
    level_txt: &'a str,
    scope: &'a str,
    formatted: &'a str,
) -> [&'a str; 5] {
    if scope == "default" {
        [level_txt, ": ", "", "", formatted]
    } else {
        [level_txt, "(", scope, "): ", formatted]
    }
}

/// Copy `parts` into `buf` if they all fit, returning the written prefix.
/// `None` is the reference's `bufPrint ... catch` — the point where it falls
/// back to the heap.
pub(crate) fn write_parts<'a>(buf: &'a mut [u8], parts: &[&str]) -> Option<&'a [u8]> {
    let total: usize = parts.iter().map(|p| p.len()).sum();
    if total > buf.len() {
        return None;
    }
    let mut at = 0usize;
    for part in parts {
        buf[at..at + part.len()].copy_from_slice(part.as_bytes());
        at += part.len();
    }
    Some(&buf[..at])
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
        //
        // PORT NOTE (parity S113-F5): the reference NEVER allocates unless the
        // stack buffer overflows, and when the heap fallback fails it `return`s —
        // the log line is dropped, the process is not killed. That ordering is
        // the whole point of the comment above: this is the path that logs an
        // OOM. The port used to `format!` the prefix and the whole line on the
        // heap FIRST and copy into `buf` afterwards, so under wasm memory
        // exhaustion it aborted through `handle_alloc_error` instead of dropping
        // the message, and the stack buffer bought nothing. The segments are now
        // written directly into `buf`, and the fallback is a FALLIBLE heap
        // reservation.
        let parts = super::message_parts(level_txt, scope, formatted);

        // Format. We attempt to use a stack-allocated string first and if that
        // fails we'll try to allocate.
        if let Some(str) = super::write_parts(&mut buf, &parts) {
            // Send it over to the JS side
            unsafe { js::log(str.as_ptr(), str.len()) };
            return;
        }

        // ... catch str: { allocated = true; break :str std.fmt.allocPrint(...) catch return; }
        let total: usize = parts.iter().map(|p| p.len()).sum();
        let mut heap: Vec<u8> = Vec::new();
        if heap.try_reserve_exact(total).is_err() {
            // catch return — the message is dropped, nothing aborts.
            return;
        }
        for part in parts {
            heap.extend_from_slice(part.as_bytes());
        }

        // Send it over to the JS side
        // defer if (allocated) wasm.alloc.free(str); — the Vec frees on drop.
        unsafe { js::log(heap.as_ptr(), heap.len()) };
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

#[cfg(test)]
mod tests {
    use super::{message_parts, write_parts};

    // PORT NOTE: no reference test. `freestanding::log` is
    // `cfg(target_arch = "wasm32")`, so these pin the two pieces that CAN run
    // on the host: the assembled bytes are byte-identical to the reference's
    // `level_txt ++ prefix ++ format`, and the 2048-byte stack buffer is a real
    // boundary rather than decoration — the port used to build the whole line on
    // the HEAP first and copy into the buffer afterwards, so the buffer bought
    // nothing and a wasm OOM aborted instead of dropping the line (S113-F5).
    #[test]
    fn wasm_log_message_is_assembled_without_the_heap_until_it_overflows() {
        let parts = message_parts("error", "default", "boom");
        let mut buf = [0u8; 2048];
        assert_eq!(
            Some(b"error: boom".as_slice()),
            write_parts(&mut buf, &parts),
        );

        let parts = message_parts("warning", "gtk", "boom");
        let mut buf = [0u8; 2048];
        assert_eq!(
            Some(b"warning(gtk): boom".as_slice()),
            write_parts(&mut buf, &parts),
        );

        // Exactly full: "error: " is 7 bytes, so 2041 more fit.
        let body = "x".repeat(2041);
        let parts = message_parts("error", "default", &body);
        let mut buf = [0u8; 2048];
        let got = write_parts(&mut buf, &parts).expect("2048 bytes must fit");
        assert_eq!(2048, got.len());

        // One more byte does not: this is where the reference allocates.
        let body = "x".repeat(2042);
        let parts = message_parts("error", "default", &body);
        let mut buf = [0u8; 2048];
        assert_eq!(None, write_parts(&mut buf, &parts));
    }
}
