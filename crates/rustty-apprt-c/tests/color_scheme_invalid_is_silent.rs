//! color_scheme_invalid_is_silent.rs — `rustty_app_set_color_scheme` and
//! `rustty_surface_set_color_scheme` must return SILENTLY on an out-of-range scheme
//! (parity finding S46-F3).
//!
//! The pinned oracle (`ghostty/src/apprt/embedded.zig:1788` and `:1992`) is:
//!
//! ```zig
//! const scheme = std.enums.fromInt(apprt.ColorScheme, scheme_raw) orelse return;
//! ```
//!
//! The port printed `invalid color scheme to rustty_*_set_color_scheme value=..`,
//! reproducing a pre-pin `intToEnum ... catch { log.warn(...) }` revision. Task 14d-3
//! (`d130c504`) removed both prints; this file is the test that pins it.
//!
//! Same re-exec idiom as `mouse_pressure_stage_guard.rs`: the child is this test
//! binary calling the exports as a C host would, and the parent asserts the child's
//! stderr is EMPTY and that it returned (a stdout marker printed after the calls).
//! One addition that precedent lacks: the child installs a `log` backend that writes
//! every record to stderr. Without it a `log::warn!` re-added at either site would
//! be swallowed by the absent logger and this test would stay green, so the file
//! would pin only the `eprintln!` shape of the defect.

use std::ffi::c_int;

use rustty_apprt::embedded::{App, Surface};

/// The first raw scheme with no `ColorScheme` tag (`enum(c_int) { light, dark }`).
const INVALID_SCHEME: c_int = 2;

const RETURNED_MARKER: &str = "COLOR_SCHEME_RETURNED";

struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        eprintln!("{}({}): {}", record.level(), record.target(), record.args());
    }
    fn flush(&self) {}
}

static LOGGER: StderrLogger = StderrLogger;

#[test]
fn invalid_color_scheme_is_silent() {
    const MARKER: &str = "RUSTTY_COLOR_SCHEME_GUARD_CHILD";
    if std::env::var_os(MARKER).is_some() {
        log::set_logger(&LOGGER).expect("the child installs the only logger");
        log::set_max_level(log::LevelFilter::Trace);
        // Positive control for the logger itself: a record DOES reach stderr. The
        // parent looks for this line, so a logger that printed nothing could not
        // make an empty-diagnostics assertion pass.
        log::warn!(target: "probe", "logger installed");

        // SAFETY: the out-of-range arm is the tag guard itself; it returns BEFORE
        // the pointer is read (the property under test), so a well-aligned
        // dangling pointer is never dereferenced.
        unsafe {
            rustty_apprt_c::rustty_app_set_color_scheme(
                std::ptr::dangling_mut::<App>(),
                INVALID_SCHEME,
            );
            rustty_apprt_c::rustty_surface_set_color_scheme(
                std::ptr::dangling_mut::<Surface>(),
                INVALID_SCHEME,
            );
        }
        println!("{RETURNED_MARKER}");
        return;
    }

    let exe = std::env::current_exe().expect("current_exe");
    let out = std::process::Command::new(exe)
        .args(["--exact", "invalid_color_scheme_is_silent", "--nocapture"])
        .env(MARKER, "1")
        .env("RUST_BACKTRACE", "0")
        .output()
        .expect("spawn child test process");

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "child test must exit 0; status={:?} stdout={stdout} stderr={stderr}",
        out.status,
    );
    assert!(
        stdout.contains(RETURNED_MARKER),
        "both exports must return normally on an invalid scheme; stdout={stdout}",
    );
    let diagnostics: Vec<&str> = stderr
        .lines()
        .filter(|l| !l.contains("logger installed"))
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert!(
        stderr.contains("logger installed"),
        "the child's stderr logger must be live, or silence proves nothing; stderr={stderr}",
    );
    assert!(
        diagnostics.is_empty(),
        "an invalid raw color scheme must produce NO diagnostic (oracle: \
         `std.enums.fromInt(...) orelse return`, embedded.zig:1788,1992); \
         child stderr was:\n{stderr}",
    );
}
