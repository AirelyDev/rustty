//! rustty-core::logging
//!
//! PORTS: ghostty src/main_ghostty.zig `logFn` + `std_options` — the process
//! log sink registered as the std.log hook for BOTH the exe root and the lib
//! root (main_c.zig re-exports main_ghostty's `std_options`). Rust has no
//! global std.log hook, so call sites invoke `log`/`log_with` directly (the
//! same collapse as the bin's original `log_fn`, which Task 8 re-routes here).
//! Hosted at the core wiring tier: below apprt/apprt-c/bin, above os/global.

use rustty_global::Logging;
use std::io::Write;

/// Log severity — the reference `std.log.Level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Err,
}

impl Level {
    // `level.asText()`.
    pub fn as_text(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warning",
            Level::Err => "error",
        }
    }
}

/// The function std.log will call (main_ghostty.zig `logFn`), over an explicit
/// `Logging` (callers that hold their own state — the bin, or `init` before
/// STATE is set — pass it directly).
pub fn log_with(level: Level, scope: &str, message: &str, logging: &Logging) {
    // On Mac, we use unified logging. To view this:
    //
    //   sudo log stream --level debug --predicate 'subsystem=="io.airely.rustty"'
    //     R2: the reference predicate names the bundle id `com.mitchellh.ghostty`.
    //
    // macOS logging is thread safe so no need for locks/mutexes
    // macos: {
    //     if (comptime !builtin.target.os.tag.isDarwin()) break :macos;
    //     if (!state.logging.macos) break :macos;
    //     const prefix = if (scope == .default) "" else @tagName(scope) ++ ": ";
    //     const mac_level: macos.os.LogType = switch (level) {
    //         .debug => .debug, .info => .info, .warn => .err, .err => .fault,
    //     };
    //     const logger = macos.os.Log.create(build_config.bundle_id, @tagName(scope));
    //     defer logger.release();
    //     logger.log(std.heap.c_allocator, mac_level, prefix ++ format, args);
    // }
    #[cfg(target_os = "macos")]
    if logging.macos {
        use rustty_os::macos::{Log, LogType};
        let prefix = if scope == "default" {
            String::new()
        } else {
            format!("{scope}: ")
        };
        // Convert our levels to Mac levels
        let mac_level = match level {
            Level::Debug => LogType::Debug,
            Level::Info => LogType::Info,
            Level::Warn => LogType::Err,
            Level::Err => LogType::Fault,
        };
        // Initialize a logger. This is slow to do on every operation
        // but we shouldn't be logging too much.
        let subsystem =
            std::ffi::CString::new(rustty_identity::BUNDLE_ID).expect("bundle id is NUL-free");
        let category = std::ffi::CString::new(scope).unwrap_or_default();
        let logger = Log::create(&subsystem, &category);
        logger.log(mac_level, &format!("{prefix}{message}"));
        logger.release();
    }

    // stderr: {
    //     // don't log debug messages to stderr unless we are a debug build
    //     if (comptime builtin.mode != .Debug and level == .debug) break :stderr;
    if !cfg!(debug_assertions) && matches!(level, Level::Debug) {
        return;
    }
    //     // skip if we are not logging to stderr
    //     if (!state.logging.stderr) break :stderr;
    if !logging.stderr {
        return;
    }
    //     const level_txt = comptime level.asText();
    //     const prefix = if (scope == .default) ": " else "(" ++ @tagName(scope) ++ "): ";
    //     nosuspend stderr.print(level_txt ++ prefix ++ format ++ "\n", args) ...
    let prefix = if scope == "default" {
        ": ".to_string()
    } else {
        format!("({scope}): ")
    };
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{}{prefix}{message}", level.as_text());
    let _ = stderr.flush();
}

/// The STATE-reading variant — the reference `logFn` reads the process-global
/// `state.logging`. Before `STATE` is set, falls back to `Logging::default()`
/// (the reference zero-initializes `state` — logging at defaults — before any
/// log output can happen).
pub fn log(level: Level, scope: &str, message: &str) {
    match crate::STATE.get() {
        Some(state) => log_with(level, scope, message, &state.logging),
        None => log_with(level, scope, message, &Logging::default()),
    }
}

// =====================================================================
// The `log` facade backend, app-build side (task-14a review CRITICAL 2,
// `.port/parity-closure/DECISION-logging.md` §6).
//
// PORT NOTE: no reference analog — `main_ghostty.zig`'s `std_options.logFn`
// is a comptime function-pointer swap reaching every `std.log.scoped(...)`
// call in the whole app binary. Rust has no such hook, so crates below this
// one that restore a `log.warn(...)` site as `log::warn!(target: "<scope>",
// ...)` (14b-14e; R4 forbids them depending on `rustty-core` directly) need a
// process-wide `log::Log` registration to reach ANYWHERE. This is that
// registration for the actual `rustty` application binary (which does NOT
// link `rustty-vt-c` — `cargo tree -p rustty` confirms no such edge — so
// `rustty-vt-c::sys::Logger`, task 14a's other backend, never runs there).
// `AppLogger` forwards into the SAME `log`/`log_with` sink immediately above,
// which is already a faithful port of the reference's `logFn`.
struct AppLogger;

/// Map a `log::Level` onto the reference's 4-level `std.log.Level` (`Level`).
/// The facade crate's 5th level, `Trace`, has no `std.log` equivalent;
/// `install_app_log_backend` never enables it (max level caps at `Debug`),
/// so this arm is unreachable in practice — mapped to `Debug` rather than
/// panicking, matching `rustty-vt-c::sys::std_level_from_log`'s identical
/// choice.
fn level_from_log(level: log::Level) -> Level {
    match level {
        log::Level::Error => Level::Err,
        log::Level::Warn => Level::Warn,
        log::Level::Info => Level::Info,
        log::Level::Debug | log::Level::Trace => Level::Debug,
    }
}

impl log::Log for AppLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // `log::set_max_level` (in `install_app_log_backend`) already gates
        // which records reach here; this sink has no independent filter,
        // matching `log`/`log_with`'s own unconditional dispatch once a
        // level/sink-flag check upstream of this passes.
        true
    }

    fn log(&self, record: &log::Record) {
        // `record.target()` carries the restored call site's oracle scope
        // (`target: "<scope>"`, `""` for `.default` — see the facade
        // pattern's own convention in DECISION-logging.md §6). Formatting
        // `record.args()` to an owned `String` here (rather than plumbing
        // `core::fmt::Arguments` through, as the vt-c backend does) matches
        // this sink's own `message: &str` signature — `log`/`log_with` were
        // ported before this facade existed and are not being changed to
        // accommodate it.
        log(
            level_from_log(record.level()),
            record.target(),
            &record.args().to_string(),
        );
    }

    fn flush(&self) {}
}

static APP_LOGGER: AppLogger = AppLogger;

/// Register [`AppLogger`] as the process-wide `log` backend for the actual
/// application binary. `std::sync::Once`-guarded: idempotent, and — like
/// `rustty-vt-c::sys::install_log_backend` — never touches `max_level` when
/// `log::set_logger` fails (an embedder/host already owns the registration),
/// only on success.
///
/// Level default ports `main_ghostty.zig`'s own `std_options.log_level`
/// exactly (`.Debug => .debug, else => .info` — the app is deliberately NEVER
/// lowered to `.debug` outside a Debug build, "our debug logs are very
/// expensive to calculate"): `cfg!(debug_assertions) ? Debug : Info`. This is
/// intentionally a different default than `rustty-vt-c::sys`'s `Debug`/`Error`
/// split — that crate is porting `lib_vt.zig`'s C-ABI arm, a different
/// `std_options` block in the reference with its own (lower) release default.
///
/// CORRECTED (task-14a review round 2 — the previous revision of this
/// comment asserted the opposite of what actually happens; do not reintroduce
/// it): `log()` above reads `rustty_core::STATE`, and `rustty_core::STATE`
/// **is** populated on the real path — `main_rustty.rs::main()` calls
/// `state_init()`, which calls `rustty_apprt::init_with_action`, which ends
/// with `let _ = rustty_core::STATE.set(state)` (`rustty-apprt/src/lib.rs`),
/// where `state.logging` is `compute_logging(action_detected, RUSTTY_LOG)` —
/// the fully resolved flags, not `Logging::default()`. `install_app_log_
/// backend` is called BEFORE `state_init()` (see `main_rustty.rs::main`'s own
/// PORT NOTE), so: any restored `log::warn!` site that runs during the
/// bootstrap sequence `init_with_action` itself performs (locale, crash-init,
/// glslang, oniguruma, resources dir, i18n — several `LOG` ledger rows land
/// inside this window, e.g. S111-F5 in `rustty-os/src/locale.rs`) reads
/// `Logging::default()` via `log()`'s own pre-`STATE` fallback — which is
/// oracle-faithful, not a gap: the reference's zero-initialized `state` gives
/// the exact same "default logging config" behavior before ITS OWN
/// `state.init()` resolves the real flags. Every site that runs AFTER
/// `state_init()` returns reads the fully resolved flags, same as the
/// reference after its own `state.init()` completes. `rustty_init`
/// (`rustty-apprt-c/src/lib.rs`, the libghostty/macOS process-init path)
/// installs the SAME way, at its own earliest point, before its own call
/// into `rustty_apprt::init_with_action`.
pub fn install_app_log_backend() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if log::set_logger(&APP_LOGGER).is_ok() {
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
mod tests {
    use super::*;
    use rustty_global::Logging;

    // task-14a review CRITICAL 2 (rustty-only proving tests, NOT reference
    // tests — `AppLogger` has no oracle analog, see its own PORT NOTE).
    //
    // `level_from_log` is exhaustively, deterministically tested below — a
    // real assertion that fails if the mapping is ever wrong. Proving that a
    // `log::warn!` call reaches the REAL sink end-to-end is not available
    // the way `rustty-vt-c`'s C-callback capture is: `log`/`log_with` write
    // directly to process stderr / macOS unified logging with no injectable
    // capture point, and the file's own pre-existing
    // `log_with_gates_on_stderr_flag` test already accepts "no panic, no
    // observable side effect we can assert on" as this sink's testing bar —
    // `app_log_backend_installs_and_dispatches_without_panicking` follows the
    // same convention rather than inventing a fake assertion that cannot
    // fail.
    #[test]
    fn level_from_log_matches_the_reference_mapping() {
        assert_eq!(Level::Err, level_from_log(log::Level::Error));
        assert_eq!(Level::Warn, level_from_log(log::Level::Warn));
        assert_eq!(Level::Info, level_from_log(log::Level::Info));
        assert_eq!(Level::Debug, level_from_log(log::Level::Debug));
        // `Trace` has no `std.log` equivalent; folds into `Debug` (see
        // `level_from_log`'s own doc).
        assert_eq!(Level::Debug, level_from_log(log::Level::Trace));
    }

    #[test]
    fn app_log_backend_installs_and_dispatches_without_panicking() {
        install_app_log_backend();
        // A second call must be a genuine no-op (the Once guard), not a
        // double-install panic or a re-touched max_level.
        install_app_log_backend();
        // Exercises the full `AppLogger::log` -> `level_from_log` ->
        // `logging::log` -> `logging::log_with` chain end to end; a panic
        // anywhere in that chain (e.g. a `CString::new` on a scope containing
        // an interior NUL) would fail this test.
        log::error!(target: "task14a_app_logger_test", "proving message {}", 1);
        log::warn!(target: "", "default-scope proving message");
    }

    #[test]
    fn log_with_gates_on_stderr_flag() {
        // Both sinks off: must be a silent no-op (no panic, no output side
        // effects we can observe — this pins the flag gating compiles + runs).
        let off = Logging {
            stderr: false,
            macos: false,
        };
        log_with(Level::Info, "default", "suppressed", &off);
        log_with(Level::Err, "test_scope", "suppressed", &off);
    }

    #[test]
    fn level_as_text_matches_reference() {
        // `level.asText()` — .warn -> "warning", .err -> "error".
        assert_eq!(Level::Debug.as_text(), "debug");
        assert_eq!(Level::Info.as_text(), "info");
        assert_eq!(Level::Warn.as_text(), "warning");
        assert_eq!(Level::Err.as_text(), "error");
    }

    #[test]
    fn state_reading_log_defaults_when_unset() {
        // `log` must not panic when STATE is not yet initialized (it falls back
        // to Logging::default()). NOTE: other tests in this binary may have set
        // STATE — either way this must not panic.
        log(Level::Info, "default", "boot message");
    }
}
