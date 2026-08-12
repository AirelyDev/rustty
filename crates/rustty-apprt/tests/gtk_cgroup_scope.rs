//! WS20 CGROUP-SCOPE KAT — `gtk/cgroup.zig` `createScope` against real systemd.
//!
//! Compile-time green proves nothing here: `createScope` builds a
//! `(ssa(sv)a(sa(sv)))` GVariant by hand (`cgroup.zig:37-92`) and hands it to
//! `org.freedesktop.systemd1.Manager.StartTransientUnit`. Every one of those type
//! strings, the `au` PID array, and the `a(sa(sv))` empty-aux tuple type compiles
//! fine when wrong and fails only at the call. This test spawns a throwaway child,
//! moves it into a transient scope for real, and then verifies via
//! `/proc/<pid>/cgroup` that the child ACTUALLY LANDED in the scope
//! `cgroup::fmt_scope` names — which is exactly the predicate
//! `pre_exec`/`post_fork` poll on (`pre_exec.zig:70-73`, `post_fork.zig:109-115`).
//!
//! Needs a user session bus and a running `systemd --user`. No GTK init, no
//! display (G-GTKINIT: zero `gtk4::init()` here).
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]

use std::process::{Command, Stdio};

use rustty_apprt::gtk::cgroup;

#[test]
#[ignore = "needs a systemd user session + session bus; run: cargo test -p rustty-apprt \
            --features gtk --test gtk_cgroup_scope -- --ignored --exact create_scope_moves_child_into_transient_scope"]
fn create_scope_moves_child_into_transient_scope() {
    let dbus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .expect("a session bus is required for this KAT");

    // A throwaway child to move. `sleep` is in coreutils on every target distro.
    let mut child = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();

    // `cgroup.createScope(dbus, pid, .{ ... })` (`post_fork.zig:74-80`), with the
    // reference's two optional resource limits actually populated so the
    // `MemoryHigh` / `TasksMax` `(sv)` entries are exercised too
    // (`cgroup.zig:55-61`).
    let result = cgroup::create_scope(
        &dbus,
        pid,
        cgroup::Options {
            memory_high: Some(512 * 1024 * 1024),
            tasks_max: Some(4096),
        },
    );

    // Whatever happens next, do not leak the child.
    let outcome = (|| {
        result.map_err(|e| format!("StartTransientUnit failed: {e}"))?;

        // The predicate `pre_exec`/`post_fork` poll on: the last `/`-segment of
        // `/proc/<pid>/cgroup`'s third field must equal `fmt_scope(pid)`.
        let mut expected_buf = [0u8; 256];
        let expected_len = cgroup::fmt_scope(&mut expected_buf, pid).len();
        let expected = &expected_buf[..expected_len];

        for _ in 0..40 {
            let mut buf = [0u8; 4096];
            if let Some(raw) = rustty_os::cgroup::current(&mut buf, pid) {
                if let Some(i) = raw.iter().rposition(|&b| b == b'/') {
                    if &raw[i + 1..] == expected {
                        return Ok(());
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        let mut buf = [0u8; 4096];
        let actual = rustty_os::cgroup::current(&mut buf, pid)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_else(|| "<unreadable>".into());
        Err(format!(
            "child never landed in the transient scope; expected tail {:?}, cgroup is {actual:?}",
            String::from_utf8_lossy(expected)
        ))
    })();

    let _ = child.kill();
    let _ = child.wait();

    if let Err(msg) = outcome {
        panic!("{msg}");
    }
}
