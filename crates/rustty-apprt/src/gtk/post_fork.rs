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
//! PORT NOTE (integration seam): the reference's signature is `pub fn
//! postFork(cmd: *Command) Command.PostForkError!void`, reading
//! `cmd.rt_post_fork_info` (a comptime-selected field type,
//! `Command.zig:154-161`) and `cmd.pid`. Rust has no comptime type selection and
//! `rustty_termio::command::RtPostForkInfo` is still the empty growth-point
//! struct, so [`post_fork`] takes the [`PostForkInfo`] and the child pid
//! explicitly. Wiring `RtPostForkInfo = PostForkInfo` is a `rustty-termio`
//! change (see the WS20 report).

use rustty_config::value_types::{GtkSingleInstance, LinuxCgroup};

use super::cgroup;

/// The scoped log name — `post_fork.zig:7`
/// `std.log.scoped(.gtk_post_fork)`.
const LOG_SCOPE: &str = "gtk_post_fork";

/// PORTS: `Command.zig:101` `pub const PostForkError = error{PostForkError};`,
/// the error set `postFork` returns.
///
/// PORT NOTE: `rustty_termio::command::PostForkError` is the same unit marker;
/// it is mirrored here rather than imported because `rustty-apprt` deliberately
/// carries NO `[dependencies]` edge to `rustty-termio` (it reaches termio only
/// transitively through `rustty_core::surface::Surface`). The two collapse into
/// one when the `RtPostForkInfo` growth point is filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostForkError;

impl core::fmt::Display for PostForkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "error.PostForkError")
    }
}

impl std::error::Error for PostForkError {}

/// PORTS: `post_fork.zig:15-32` `pub const PostForkInfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostForkInfo {
    /// `gtk_single_instance: configpkg.Config.GtkSingleInstance` (`:16`).
    pub gtk_single_instance: GtkSingleInstance,
    /// `linux_cgroup: configpkg.Config.LinuxCgroup` (`:17`).
    pub linux_cgroup: LinuxCgroup,
    /// `linux_cgroup_hard_fail: bool` (`:18`).
    pub linux_cgroup_hard_fail: bool,
    /// `linux_cgroup_memory_limit: ?u64` (`:19`).
    pub linux_cgroup_memory_limit: Option<u64>,
    /// `linux_cgroup_processes_limit: ?u64` (`:20`).
    pub linux_cgroup_processes_limit: Option<u64>,
}

impl PostForkInfo {
    /// PORTS: `post_fork.zig:22-30` `init`.
    #[must_use]
    pub fn init(cfg: &rustty_config::Config) -> Self {
        Self {
            gtk_single_instance: cfg.gtk_single_instance,
            linux_cgroup: cfg.linux_cgroup,
            linux_cgroup_hard_fail: cfg.linux_cgroup_hard_fail,
            linux_cgroup_memory_limit: cfg.linux_cgroup_memory_limit,
            linux_cgroup_processes_limit: cfg.linux_cgroup_processes_limit,
        }
    }
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
                eprintln!(
                    "err({LOG_SCOPE}): gtk-single-instance is set to detect which should be impossible!"
                );
                return Err(PostForkError);
            }
        },
    }

    // `const pid: u32 = @intCast(cmd.pid orelse { log.err("PID of child not
    //  known!", .{}); return error.PostForkError; });` (`:53-56`).
    let Some(pid) = pid else {
        eprintln!("err({LOG_SCOPE}): PID of child not known!");
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
    eprintln!(
        "debug({LOG_SCOPE}): beginning transition to transient systemd scope {expected_display}"
    );

    // `const app = Application.default();` (`:63`).
    let app = super::class::application::Application::default();

    // `const dbus = app.as(gio.Application).getDbusConnection() orelse { ... }`
    // (`:65-72`).
    let Some(dbus) = app.dbus_connection() else {
        if info.linux_cgroup_hard_fail {
            // `log.err("dbus connection required for cgroup isolation,
            //  exiting", .{}); return error.PostForkError;` (`:67-68`).
            eprintln!("err({LOG_SCOPE}): dbus connection required for cgroup isolation, exiting");
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
            eprintln!(
                "err({LOG_SCOPE}): unable to create transient systemd scope {expected_display}: {err}"
            );
            return Err(PostForkError);
        }
        // `log.warn("unable to create transient systemd scope {s}: {t}", ...);
        //  return;` (`:84-85`).
        eprintln!(
            "warn({LOG_SCOPE}): unable to create transient systemd scope {expected_display}: {err}"
        );
        return Ok(());
    }

    // `const start: std.Io.Timestamp = .now(global.io(), .awake);` (`:89`).
    let start = std::time::Instant::now();

    // `loop: while (true) { ... }` (`:91-115`).
    loop {
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
                eprintln!(
                    "err({LOG_SCOPE}): transition to new transient systemd scope {expected_display} took too long"
                );
                return Err(PostForkError);
            }
            // `log.warn("transition to transient systemd scope {s} took too
            //  long", ...); break :loop;` (`:97-98`).
            eprintln!(
                "warn({LOG_SCOPE}): transition to transient systemd scope {expected_display} took too long"
            );
            break;
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
                eprintln!(
                    "debug({LOG_SCOPE}): transition to transient systemd scope {expected_display} complete"
                );
                break;
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
    #[test]
    fn single_instance_detect_is_error() {
        assert_eq!(
            post_fork(
                &info(LinuxCgroup::SingleInstance, GtkSingleInstance::Detect),
                Some(1)
            ),
            Err(PostForkError)
        );
    }

    /// An unknown child pid is a hard error (`:53-56`). Checked BEFORE the
    /// `Application::default()` lookup, so it too is headless-safe.
    #[test]
    fn unknown_pid_is_error() {
        assert_eq!(
            post_fork(&info(LinuxCgroup::Always, GtkSingleInstance::True), None),
            Err(PostForkError)
        );
    }
}
