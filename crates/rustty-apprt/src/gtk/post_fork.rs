//! rustty-apprt::gtk::post_fork
//!
//! PORTS: ghostty `src/apprt/gtk/post_fork.zig`.
//!
//! # THIS CODE RUNS IN THE *PARENT*, NOT THE CHILD
//!
//! Unlike [`super::pre_exec`], `postFork` is called from `Command.zig:197`
//! (`if (self.rt_post_fork) |f| try f(self);`) on the branch where `fork`
//! returned the CHILD'S PID — i.e. in the parent process, with all its threads
//! and locks intact. It even reads `cmd.pid`, which only the parent has. So the
//! async-signal-safety restriction that governs `pre_exec.rs` does NOT apply
//! here: this module is free to allocate, take locks, log through the normal
//! channels and make blocking D-Bus calls, exactly as the reference does
//! (`glib.VariantBuilder`, `dbus.callSync`, `log.err`/`log.warn`/`log.debug`).
//!
//! Its job: "If we are configured to do so, tell `systemd` to move the new child
//! PID into a transient `systemd` scope with the configured resource limits."
//! (`post_fork.zig:34-38`)
//!
//! PORT NOTE (integration seam — V13). The reference's signature is `pub fn
//! postFork(cmd: *Command) Command.PostForkError!void`, reading
//! `cmd.rt_post_fork_info` — a comptime-selected field type
//! (`Command.zig:148-155`) — and `cmd.pid`. Rust has no comptime type selection
//! and R4 forbids the `rustty-termio` -> `rustty-apprt` edge that would let
//! termio name this type, so the identity
//! `RtPostForkInfo == apprt.runtime.post_fork.PostForkInfo` is established from
//! the LOWER crate: [`PostForkInfo`] IS
//! [`rustty_termio::command::RtPostForkInfo`], re-exported here under the
//! reference's name (and [`PostForkError`] is likewise now the ONE
//! `rustty_termio::command::PostForkError`, no longer a mirror of it). The full
//! reasoning is in the seam PORT NOTE above `rustty_termio::command::RtPreExecInfo`.
//!
//! The hook FUNCTION half is [`rt_post_fork`] below — the `PostForkFn`-shaped
//! adapter the apprt installs with `rustty_termio::command::install_rt_hooks`,
//! this port's stand-in for `Exec.zig:1036`'s
//! `if (comptime @hasDecl(apprt.runtime, "post_fork")) apprt.runtime.post_fork.postFork else null`.
//! [`post_fork`] itself keeps taking the info and the pid EXPLICITLY, which is
//! what lets its five failure arms be unit-tested without a live `Command`.

use rustty_config::value_types::{GtkSingleInstance, LinuxCgroup};
use rustty_termio::command::Command;

use super::cgroup;

/// The scoped log name — `post_fork.zig:7`
/// `std.log.scoped(.gtk_post_fork)`.
const LOG_SCOPE: &str = "gtk_post_fork";

/// PORTS: `Command.zig:95` `pub const PostForkError = error{PostForkError};`,
/// the error set `postFork` returns — THE one in `rustty-termio`, re-exported
/// (V13 collapsed the mirror this module used to carry to dodge the crate edge).
pub use rustty_termio::command::PostForkError;

/// PORTS: `post_fork.zig:15-31` `pub const PostForkInfo` — see the module PORT
/// NOTE: the type is DEFINED in `rustty-termio` (as the reference's
/// `Command.RtPostForkInfo`, which the comptime branch makes the SAME type) and
/// re-exported here under the reference's name.
pub use rustty_termio::command::RtPostForkInfo as PostForkInfo;

/// The `Command.PostForkFn`-shaped adapter the apprt installs into
/// `rustty_termio::command::install_rt_hooks` — the reference's
/// `apprt.runtime.post_fork.postFork` AS SEEN BY `Exec.zig:1036`. Its body is
/// the `cmd.rt_post_fork_info` / `cmd.pid` reads the reference does inline
/// (`post_fork.zig:41,53`).
///
/// Runs in the PARENT (`Command.zig:197`), so it is free to allocate and log.
///
/// # Errors
/// Forwards [`post_fork`]'s [`PostForkError`]; `Command::start` turns it into
/// the reference's `error.PostForkError` surface-creation failure.
pub fn rt_post_fork(cmd: &mut Command<'_>) -> Result<(), PostForkError> {
    // `const pid: u32 = @intCast(cmd.pid orelse ...)` (`post_fork.zig:53`) — a
    // Zig `@intCast` traps on a negative value, which a live child pid cannot
    // be. The `orelse` arm (unknown pid) is `post_fork`'s own `None` case.
    let pid = cmd
        .pid
        .map(|pid| u32::try_from(pid).expect("a child pid is non-negative"));
    post_fork(&cmd.rt_post_fork_info, pid)
}

/// "If we are configured to do so, tell `systemd` to move the new child PID into
/// a transient `systemd` scope with the configured resource limits.
///
/// If we are configured to hard fail, log an error message and return an error
/// code if we don't detect the move in time."
///
/// PORTS: `post_fork.zig:34-115` `postFork`.
///
/// `pid` is the reference's `cmd.pid` (`:53-56`): `None` means the fork bookkeeping
/// never recorded a child, which is a hard error there and here.
///
/// # Errors
/// [`PostForkError`] on the reference's four failure arms: `gtk-single-instance
/// = detect` (`:47-49`), unknown child pid (`:53-56`), no D-Bus connection with
/// `hard_fail` (`:66-71`), `createScope` failure with `hard_fail` (`:78-86`), and
/// the 250 ms timeout with `hard_fail` (`:90-96`).
pub fn post_fork(info: &PostForkInfo, pid: Option<u32>) -> Result<(), PostForkError> {
    use gio::prelude::*;

    // `switch (cmd.rt_post_fork_info.linux_cgroup) { ... }` (`:41-51`).
    match info.linux_cgroup {
        // `.always => {}` (`:42`).
        LinuxCgroup::Always => {}
        // `.never => return` (`:43`).
        LinuxCgroup::Never => return Ok(()),
        // `.@"single-instance" => switch (...gtk_single_instance)` (`:44-50`).
        LinuxCgroup::SingleInstance => match info.gtk_single_instance {
            GtkSingleInstance::True => {}
            GtkSingleInstance::False => return Ok(()),
            GtkSingleInstance::Detect => {
                // `log.err("gtk-single-instance is set to detect which should be
                //  impossible!", .{}); return error.PostForkError;` (`:47-48`).
                log::error!(
                    target: LOG_SCOPE,
                    "gtk-single-instance is set to detect which should be impossible!",
                );
                return Err(PostForkError);
            }
        },
    }

    // `const pid: u32 = @intCast(cmd.pid orelse { log.err("PID of child not
    //  known!", .{}); return error.PostForkError; });` (`:53-56`).
    let Some(pid) = pid else {
        log::error!(target: LOG_SCOPE, "PID of child not known!");
        return Err(PostForkError);
    };

    // `var expected_cgroup_buf: [256]u8 = undefined;
    //  const expected_cgroup = cgroup.fmtScope(&expected_cgroup_buf, pid);`
    // (`:58-59`).
    let mut expected_cgroup_buf = [0u8; 256];
    let expected_len = cgroup::fmt_scope(&mut expected_cgroup_buf, pid).len();
    let expected_cgroup = &expected_cgroup_buf[..expected_len];
    // For the log lines only (`{s}` of the same bytes); the COMPARISONS below
    // stay on `&[u8]` (byte fidelity — never a lossy `String`).
    let expected_display = String::from_utf8_lossy(expected_cgroup).into_owned();

    // `log.debug("beginning transition to transient systemd scope {s}", ...)`
    // (`:61`).
    log::debug!(
        target: LOG_SCOPE,
        "beginning transition to transient systemd scope {expected_display}",
    );

    // `const app = Application.default();` (`:63`).
    let app = super::class::application::Application::default();

    // `const dbus = app.as(gio.Application).getDbusConnection() orelse { ... }`
    // (`:65-72`).
    let Some(dbus) = app.dbus_connection() else {
        if info.linux_cgroup_hard_fail {
            // `log.err("dbus connection required for cgroup isolation,
            //  exiting", .{}); return error.PostForkError;` (`:67-68`).
            log::error!(target: LOG_SCOPE, "dbus connection required for cgroup isolation, exiting");
            return Err(PostForkError);
        }
        // `return;` (`:71`).
        return Ok(());
    };

    // `cgroup.createScope(dbus, pid, .{ .memory_high = ..., .tasks_max = ... })
    //  catch |err| { ... }` (`:74-87`).
    if let Err(err) = cgroup::create_scope(
        &dbus,
        pid,
        cgroup::Options {
            memory_high: info.linux_cgroup_memory_limit,
            tasks_max: info.linux_cgroup_processes_limit,
        },
    ) {
        if info.linux_cgroup_hard_fail {
            // `log.err("unable to create transient systemd scope {s}: {t}", ...);
            //  return error.PostForkError;` (`:80-81`).
            log::error!(
                target: LOG_SCOPE,
                "unable to create transient systemd scope {expected_display}: {err}",
            );
            return Err(PostForkError);
        }
        // `log.warn("unable to create transient systemd scope {s}: {t}", ...);
        //  return;` (`:84-85`).
        log::warn!(
            target: LOG_SCOPE,
            "unable to create transient systemd scope {expected_display}: {err}",
        );
        return Ok(());
    }

    // `const start: std.Io.Timestamp = .now(global.io(), .awake);` (`:89`).
    let start = std::time::Instant::now();

    // `loop: while (true) { ... }` (`:91-115`).
    'outer: loop {
        // `if (start.untilNow(...).toMilliseconds() > 250)` (`:92`).
        if start.elapsed().as_millis() > 250 {
            // PORT NOTE (reference quirk, preserved): this arm reads
            // `cmd.rt_pre_exec_INFO.linux_cgroup_hard_fail` (`:93`), not
            // `rt_post_fork_info` — every other arm in this function reads the
            // post-fork one. Both are `init`'d from the SAME config field
            // (`pre_exec.zig:19` / `post_fork.zig:25` both read
            // `cfg.@"linux-cgroup-hard-fail"`), so the two are always equal and
            // the quirk is unobservable; we read our single `info` here.
            if info.linux_cgroup_hard_fail {
                // `log.err("transition to new transient systemd scope {s} took
                //  too long", ...); return error.PostForkError;` (`:94-95`).
                log::error!(
                    target: LOG_SCOPE,
                    "transition to new transient systemd scope {expected_display} took too long",
                );
                return Err(PostForkError);
            }
            // `log.warn("transition to transient systemd scope {s} took too
            //  long", ...); break :loop;` (`:97-98`).
            log::warn!(
                target: LOG_SCOPE,
                "transition to transient systemd scope {expected_display} took too long",
            );
            break 'outer;
        }

        // `not_found: { ... }` (`:101-112`).
        'not_found: {
            // `var current_cgroup_buf: [4096]u8 = undefined;` (`:102`).
            let mut current_cgroup_buf = [0u8; 4096];

            // `const current_cgroup_raw = internal_os.cgroup.current(
            //     &current_cgroup_buf, @intCast(pid)) orelse break :not_found;`
            // (`:104-107`). The PARENT may use the allocating
            // `rustty_os::cgroup::current` — the fork restriction that forces
            // `pre_exec.rs` to hand-roll this does not apply here.
            let Some(current_cgroup_raw) = rustty_os::cgroup::current(&mut current_cgroup_buf, pid)
            else {
                break 'not_found;
            };

            // `const index = std.mem.lastIndexOfScalar(u8, current_cgroup_raw,
            //     '/') orelse break :not_found;` (`:109`).
            let Some(index) = current_cgroup_raw.iter().rposition(|&b| b == b'/') else {
                break 'not_found;
            };
            // `const current_cgroup = current_cgroup_raw[index + 1 ..];` (`:110`).
            let current_cgroup = &current_cgroup_raw[index + 1..];

            // `if (std.mem.eql(u8, current_cgroup, expected_cgroup)) { ...
            //  break :loop; }` (`:112-115`).
            if current_cgroup == expected_cgroup {
                // `log.debug("transition to transient systemd scope {s}
                //  complete", ...)` (`:113`).
                log::debug!(
                    target: LOG_SCOPE,
                    "transition to transient systemd scope {expected_display} complete",
                );
                break 'outer;
            }
        }

        // `std.Io.sleep(global.io(), .fromMilliseconds(25), .awake) catch
        //  unreachable;` (`:118`).
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(linux_cgroup: LinuxCgroup, gtk_single_instance: GtkSingleInstance) -> PostForkInfo {
        PostForkInfo {
            gtk_single_instance,
            linux_cgroup,
            linux_cgroup_hard_fail: true,
            linux_cgroup_memory_limit: None,
            linux_cgroup_processes_limit: None,
        }
    }

    /// `.never` returns before touching D-Bus (`post_fork.zig:43`). Reached
    /// without a GTK app, which is what makes it testable headlessly.
    #[test]
    fn never_returns_ok() {
        assert_eq!(
            post_fork(&info(LinuxCgroup::Never, GtkSingleInstance::True), Some(1)),
            Ok(())
        );
    }

    /// `.single-instance` + `false` returns before touching D-Bus (`:46`).
    #[test]
    fn single_instance_false_returns_ok() {
        assert_eq!(
            post_fork(
                &info(LinuxCgroup::SingleInstance, GtkSingleInstance::False),
                Some(1)
            ),
            Ok(())
        );
    }

    /// `.single-instance` + `detect` is the "should be impossible" hard error
    /// (`:47-48`).
    ///
    /// N14: the `log.err` goes through the `log` facade under the reference's
    /// scope (`std.log.scoped(.gtk_post_fork)`), not a direct stderr print.
    #[test]
    fn single_instance_detect_is_error() {
        let cap = crate::log_capture::start();
        assert_eq!(
            post_fork(
                &info(LinuxCgroup::SingleInstance, GtkSingleInstance::Detect),
                Some(1)
            ),
            Err(PostForkError)
        );
        assert_eq!(
            cap.messages(),
            vec![(
                log::Level::Error,
                "gtk_post_fork".to_string(),
                "gtk-single-instance is set to detect which should be impossible!".to_string()
            )]
        );
    }

    /// An unknown child pid is a hard error (`:53-56`). Checked BEFORE the
    /// `Application::default()` lookup, so it too is headless-safe.
    #[test]
    fn unknown_pid_is_error() {
        let cap = crate::log_capture::start();
        assert_eq!(
            post_fork(&info(LinuxCgroup::Always, GtkSingleInstance::True), None),
            Err(PostForkError)
        );
        // N14: through the facade, `post_fork.zig:54` `log.err("PID of child
        // not known!", .{})`.
        assert_eq!(
            cap.messages(),
            vec![(
                log::Level::Error,
                "gtk_post_fork".to_string(),
                "PID of child not known!".to_string()
            )]
        );
    }
}
