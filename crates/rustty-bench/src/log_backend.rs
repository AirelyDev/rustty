//! The `log::Log` backend for the STANDALONE `rustty-bench` binary.
//!
//! # Why this exists (task-14e-2)
//!
//! Every diagnostic in this crate used to be a bare
//! `eprintln!("[{LOG_SCOPE}] …")`. That reached a human but reproduced neither
//! `logFn`'s output nor its behaviour, and `check_log_reconcile.py` counted it
//! as a DROPPED diagnostic — correctly, since it is not the reference's
//! rendering and honours no level or scope. Routing those 52 sites through the
//! `log` facade (DECISION-logging.md §6r4 call shape 2) fixes both, but a
//! `log::warn!` emits NOTHING until some binary installs a backend — and
//! §6r2/§7 record that the standalone `[[bin]]` (`main_bench.rs`) installs
//! neither of the two the workspace ships:
//!
//!   * `rustty-vt-c::sys::Logger` is installed by the `rustty_*_new` C exports,
//!     which the bench binary never calls;
//!   * `rustty-core::logging::AppLogger` is installed by `rustty_init` and by
//!     `main_rustty.rs`, neither of which is this binary.
//!
//! `rustty-bench` can depend on neither crate without inverting R4's layering,
//! so converting the sites without this backend would have made the standalone
//! bench binary SILENT where it used to print — strictly worse than the state
//! it started from. This is the "add a bin-side install" that §6r2 names as the
//! missing piece.
//!
//! # What it is NOT
//!
//! It is **not** `global.init(.{ .tool = ... })`. `S62-F2` is about the whole
//! process init the reference's `cli.main` performs — logging, `GHOSTTY_LOG`,
//! rlimits, locale, oniguruma/glslang, the resources dir and signal handlers —
//! and this supplies only the logging part. That row stays **OPEN**.
//!
//! # The rendering
//!
//! `main_ghostty.zig`'s `logFn` writes `level_txt ++ "(" ++ @tagName(scope) ++
//! "): " ++ message` to stderr, with `level_txt` from
//! `std.log.Level.asText()` — so `.err` renders as **`error`**, not `err`
//! (`lib/std/log.zig`). The `.default` scope (the facade's EMPTY target) drops
//! the parenthesised part entirely, which is N17's fix in
//! `rustty_core::logging::scope_from_target`, reproduced here because this
//! crate cannot name that one.
//!
//! The library path is unaffected: when `rustty-bench` is reached through the
//! libghostty artifact (`rustty-apprt-c/src/lib.rs`'s
//! `extern crate rustty_bench as _;`), `rustty_init` has already installed
//! `AppLogger` and this install is a no-op — `log::set_logger`'s
//! "already installed" error is swallowed, the same shape both production
//! backends use.

use std::sync::Once;

struct BenchLogger;

/// `std.log.Level.asText()` — the `level_txt` `logFn` prefixes with.
///
/// `.err` renders as **`error`**, not `err` (`lib/std/log.zig`). Split out from
/// [`BenchLogger::log`] so it can be asserted without capturing the process's
/// stderr.
pub(crate) fn level_txt(level: log::Level) -> &'static str {
    match level {
        log::Level::Error => "error",
        log::Level::Warn => "warning",
        log::Level::Info => "info",
        // `std.log` has four levels; `Trace` folds into `debug`, the same fold
        // `rustty-vt-c::sys::std_level_from_log` performs.
        log::Level::Debug | log::Level::Trace => "debug",
    }
}

/// One rendered `logFn` line, without the trailing newline.
///
/// `level_txt ++ "(" ++ @tagName(scope) ++ "): " ++ message`, with the
/// parenthesised part elided for the `.default` scope (N17).
pub(crate) fn render(level: log::Level, target: &str, message: &str) -> String {
    let level_txt = level_txt(level);
    if target.is_empty() || target == "default" {
        format!("{level_txt}: {message}")
    } else {
        format!("{level_txt}({target}): {message}")
    }
}

impl log::Log for BenchLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        eprintln!(
            "{}",
            render(record.level(), record.target(), &record.args().to_string())
        );
    }

    fn flush(&self) {}
}

static LOGGER: BenchLogger = BenchLogger;
static INSTALL: Once = Once::new();

/// Installs this crate's backend for the standalone benchmark binary.
///
/// Idempotent, and deliberately tolerant of an already-installed logger: when
/// the bench LIBRARY is reached through libghostty, `rustty_init` has already
/// registered `AppLogger` and that one wins.
pub fn install_bench_log_backend() {
    INSTALL.call_once(|| {
        if log::set_logger(&LOGGER).is_ok() {
            // The reference's default `log_level` is `.debug` in debug builds
            // and `.info` otherwise (`main_ghostty.zig`'s `std_options`); the
            // workspace's other backends make the same split.
            log::set_max_level(if cfg!(debug_assertions) {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The level token is **`error`**, not `err`: `std.log.Level.err.asText()`
    /// is `"error"`. Two stages on this plan got this wrong, so it is asserted
    /// against the real renderer, positively AND negatively.
    #[test]
    fn level_tokens_match_zig_as_text() {
        assert_eq!("error", level_txt(log::Level::Error));
        assert_ne!("err", level_txt(log::Level::Error));
        assert_eq!("warning", level_txt(log::Level::Warn));
        assert_eq!("info", level_txt(log::Level::Info));
        assert_eq!("debug", level_txt(log::Level::Debug));
        assert_eq!("debug", level_txt(log::Level::Trace));
    }

    /// The whole line, including the `.default`-scope elision (N17).
    ///
    /// NOTE: this test does NOT call `install_bench_log_backend`, and must not.
    /// `log::set_logger` succeeds once per process, so installing here would
    /// take the slot the test binary's capture logger needs and silence every
    /// site test in this crate.
    #[test]
    fn rendered_lines_match_logfn() {
        assert_eq!(
            "warning(terminal-stream-bench): error opening data file err=nope",
            render(
                log::Level::Warn,
                "terminal-stream-bench",
                "error opening data file err=nope"
            )
        );
        assert_eq!(
            "error(hyperlink-map-bench): boom",
            render(log::Level::Error, "hyperlink-map-bench", "boom")
        );
        // `.default` — the EMPTY target — drops the parenthesised scope.
        assert_eq!("warning: boom", render(log::Level::Warn, "", "boom"));
        assert_eq!("warning: boom", render(log::Level::Warn, "default", "boom"));
    }
}
