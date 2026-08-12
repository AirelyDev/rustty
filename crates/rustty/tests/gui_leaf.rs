//! The GUI launch-leaf KAT (L5.1 T12).
//!
//! Proves the `rustty` bin's `main_rustty` GUI leaf (`main_ghostty.zig:96-112`)
//! is wired to the NATIVE gtk4-rs apprt backend for the `Runtime::Gtk` build:
//! `App::create` (`rustty_core::App::create`) + `gtk::App::init` + the
//! hand-rolled `gtk::App::run` loop (`class/application.zig:474-592`), replacing
//! the deferred `unimplemented!()` placeholder that stood at `main_rustty.rs:292`.
//!
//! R5: the whole file is gated `all(any(linux, freebsd), feature = "gtk")` — off
//! the gate the GTK backend (and its gtk4-rs bindings) is compiled out entirely
//! and the leaf stays the deferred placeholder, so there is nothing to assert.
//! `gtk_runtime_leaf_is_reachable` is a non-display compile-gate (normal gate);
//! `rustty_gtk_launch_and_quit` boots real GTK and is `#[ignore]`d out of the
//! headless default gate.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Path to the built `rustty` binary (Cargo exports this for integration tests).
const BIN: &str = env!("CARGO_BIN_EXE_rustty");

/// Compile-gate: on the Linux `--features gtk` build the runtime `main_rustty`
/// dispatches on is `Runtime::Gtk`, and the leaf names the NATIVE backend's
/// [`gtk::App`](rustty_apprt::gtk::App) lifecycle — the same `init`/`run`/
/// `terminate` sequence `main_rustty::run_gtk` drives (`main_ghostty.zig:97-111`).
/// A renamed method or a dropped `rustty-apprt/gtk` dependency edge breaks this,
/// re-stranding the `Runtime::Gtk` arm on the `unimplemented!()` placeholder.
/// Non-display: pure type/const witnesses, so it runs in the normal gate.
#[test]
fn gtk_runtime_leaf_is_reachable() {
    // The build-config runtime this build selects — the arm `main_rustty`
    // dispatches on (`if app_runtime() == .gtk { run_gtk(...) }`).
    assert_eq!(
        rustty_build_config::app_runtime(),
        rustty_build_config::Runtime::Gtk,
        "the Linux `--features gtk` build must select the GTK runtime leaf"
    );

    // The leaf's three reference calls, as fn-item witnesses (`App.create` →
    // `rustty_core::App::create`; `app_runtime.init`/`run`/`terminate` →
    // `gtk::App::init`/`run`/`terminate`). Referencing them fails to compile if the
    // NATIVE backend's App shim is renamed or its `gtk` edge is dropped.
    let _init = rustty_apprt::gtk::App::init;
    let _run = rustty_apprt::gtk::App::run;
    let _terminate = rustty_apprt::gtk::App::terminate;
    let _create = rustty_core::app::App::<rustty_apprt::gtk::App>::create;
    let _ = (_init, _run, _terminate, _create);
}

/// The GUI leaf actually launches: `rustty` (no CLI action, `Runtime::Gtk`) must
/// enter the hand-rolled event loop (`class/application.zig:474-592`) rather than
/// panic on the deferred `unimplemented!()` leaf. Spawns the built binary with
/// `--initial-window=false` (so `activate`/window-creation stays dormant — the
/// Surface widget + L4 Renderer land with later tasks) and asserts it boots GTK/
/// libadwaita and reaches the loop within the deadline.
///
/// PORT NOTE (how far it gets): a literal clean exit-0 is not reachable yet — the
/// GTK app has no external quit path wired (surface-close and the D-Bus `quit`
/// action land with T13+/L6, and this non-single-instance build owns no bus
/// name), so the loop runs until stopped. Reaching `entering runloop` proves the
/// whole leaf succeeded (`App::create` + `gtk::App::init` + `run`'s
/// acquire/register/loop) with no `unimplemented!()` panic; the test then stops
/// the child. `#[ignore]`: boots real GTK (needs a display).
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty --features gtk --test gui_leaf -- --ignored"]
fn rustty_gtk_launch_and_quit() {
    let mut child = Command::new(BIN)
        .arg("--initial-window=false")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn the rustty binary");

    let stderr = child.stderr.take().expect("child stderr was piped");
    let (tx, rx) = mpsc::channel();
    // Drain the child's (very chatty) stderr on a thread: signal once the loop is
    // entered, flag the deferred-leaf panic, and keep reading so the child never
    // blocks on a full stderr pipe.
    let reader = std::thread::spawn(move || {
        let mut saw_runloop = false;
        let mut saw_unimplemented = false;
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            if line.contains("entering runloop") && !saw_runloop {
                saw_runloop = true;
                let _ = tx.send(());
            }
            // The pre-T12 placeholder: `unimplemented!()` panics as
            // "not implemented: GUI launch ...".
            if line.contains("not implemented") || line.contains("no compiled backend") {
                saw_unimplemented = true;
            }
        }
        (saw_runloop, saw_unimplemented)
    });

    // Deadline for GTK/libadwaita init + reaching the loop (generous for a debug
    // build under xvfb).
    let entered = rx.recv_timeout(Duration::from_secs(30)).is_ok();

    // Stop the child (no clean external quit path yet — see the PORT NOTE).
    let _ = child.kill();
    let _ = child.wait();
    let (saw_runloop, saw_unimplemented) = reader.join().unwrap_or((false, false));

    assert!(
        !saw_unimplemented,
        "the Runtime::Gtk arm must NOT reach the deferred unimplemented!() leaf"
    );
    assert!(
        entered || saw_runloop,
        "`rustty` (Runtime::Gtk) must launch the GTK app and enter the hand-rolled \
         run loop (class/application.zig:474-592)"
    );
}
