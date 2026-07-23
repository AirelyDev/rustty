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
        let subsystem = std::ffi::CString::new(rustty_identity::BUNDLE_ID)
            .expect("bundle id is NUL-free");
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

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_global::Logging;

    #[test]
    fn log_with_gates_on_stderr_flag() {
        // Both sinks off: must be a silent no-op (no panic, no output side
        // effects we can observe — this pins the flag gating compiles + runs).
        let off = Logging { stderr: false, macos: false };
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
