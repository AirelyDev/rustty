//! mouse_pressure_stage_guard.rs — the `rustty_surface_mouse_pressure` invalid-stage
//! guard must be SILENT (parity finding W22-B02-DIAG-01).
//!
//! The pinned oracle (`ghostty/src/apprt/embedded.zig:1880`) is:
//!
//! ```zig
//! const stage = std.enums.fromInt(input.MousePressureStage, stage_raw) orelse return;
//! ```
//!
//! i.e. an out-of-range raw stage returns with NO output at all. (Upstream did
//! `intToEnum ... catch { log.warn(...); return; }` until e8525c0fd "Update to Zig
//! 0.16.0"; the port had tracked that stale revision and emitted an `eprintln!`,
//! which is a stderr write no oracle revision ever performed — Ghostty's `log.warn`
//! goes through `std_options.logFn`.)
//!
//! The pin uses the `panic_policy.rs` re-exec idiom: the child process is this same
//! test binary, which calls the export exactly as a C host would; the parent asserts
//! the child wrote NOTHING to stderr and still returned normally (a stdout marker
//! printed after the call). Running the call in a child is what makes the assertion
//! possible at all — `eprintln!` from the test thread is swallowed by libtest's
//! output capture and cannot be observed in-process.

use rustty_apprt::embedded::Surface;

/// The first raw stage value with no `MousePressureStage` tag
/// (`enum(u8) { none = 0, normal = 1, deep = 2 }`).
const INVALID_STAGE: u32 = 3;

/// Printed by the child AFTER the export returns: proves the call was actually made
/// and came back (so an empty stderr is "silent", not "never ran"/"crashed").
const RETURNED_MARKER: &str = "MOUSE_PRESSURE_RETURNED";

#[test]
fn invalid_mouse_pressure_stage_is_silent() {
    const MARKER: &str = "RUSTTY_MOUSE_PRESSURE_GUARD_CHILD";
    if std::env::var_os(MARKER).is_some() {
        // Child: drive the export with an out-of-range stage.
        //
        // SAFETY: the invalid-stage arm is the tag guard itself — it returns BEFORE
        // the `surface` pointer is read (that is precisely the property under test),
        // so a well-aligned dangling pointer is never dereferenced. A live `Surface`
        // is not constructible off the GPU/xev seam, and constructing one would not
        // strengthen this pin.
        unsafe {
            rustty_apprt_c::rustty_surface_mouse_pressure(
                std::ptr::dangling_mut::<Surface>(),
                INVALID_STAGE,
                1.0,
            );
        }
        println!("{RETURNED_MARKER}");
        return;
    }

    let exe = std::env::current_exe().expect("current_exe");
    let out = std::process::Command::new(exe)
        .args(["--exact", "invalid_mouse_pressure_stage_is_silent"])
        .env(MARKER, "1")
        // libtest captures child-thread output by default; `--nocapture` lets the
        // export's own writes reach the real stdout/stderr this parent reads.
        .arg("--nocapture")
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
        "the export must return normally on an invalid stage; stdout={stdout}",
    );
    assert!(
        !stderr.contains("mouse pressure"),
        "rustty_surface_mouse_pressure must emit NO diagnostic for an invalid raw \
         stage (oracle: `std.enums.fromInt(...) orelse return`, embedded.zig:1880); \
         child stderr was:\n{stderr}",
    );
}
