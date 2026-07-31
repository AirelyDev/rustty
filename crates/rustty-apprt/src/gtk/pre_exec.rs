//! rustty-apprt::gtk::pre_exec
//!
//! PORTS: ghostty `src/apprt/gtk/pre_exec.zig`.
//!
//! # THIS CODE RUNS BETWEEN `fork(2)` AND `exec(2)`
//!
//! `Command.zig:224-225` calls the `rt_pre_exec` hook in the FORKED CHILD, after
//! `fork` has returned 0 and before `execvp`. In a multi-threaded parent (which
//! a GTK app always is: GLib worker threads, the renderer thread, the termio
//! thread) only the forking thread survives into the child, so **any lock held
//! by another thread at the instant of the fork is held forever in the child**.
//! POSIX therefore permits only async-signal-safe operations here.
//!
//! The reference honors that discipline exactly, and this port preserves it
//! operation-for-operation:
//!
//! | reference (`pre_exec.zig`)             | why it is legal            | this port |
//! |----------------------------------------|----------------------------|-----------|
//! | `switch (cmd.rt_pre_exec_info...)`      | pure, on pre-`fork` copies | `match` on a `Copy` struct captured before the fork |
//! | `std.os.linux.getpid()`                 | raw syscall (AS-safe)      | `libc::getpid()` |
//! | `cgroup.fmtScope(&stack_buf, pid)`      | fills a STACK buffer, no allocator | [`super::cgroup::fmt_scope`] — documented alloc-free |
//! | `std.Io.Timestamp.now(.awake)`          | `clock_gettime` (AS-safe)  | `libc::clock_gettime(CLOCK_MONOTONIC)` |
//! | `internal_os.cgroup.current(&stack_buf, pid)` | `open`/`read`/`close` into a STACK buffer | [`current_cgroup`] below — hand-rolled, NOT `rustty_os::cgroup::current` (see below) |
//! | `std.mem.lastIndexOfScalar` / `eql`     | pure slice math            | `rposition` / `==` on `&[u8]` |
//! | `std.Io.sleep(25ms, .awake)`            | `nanosleep` (AS-safe)      | `libc::nanosleep` |
//! | `log.err(...)`                          | (the reference's one liberty) | a raw `write(2)` of a `&'static [u8]` — strictly SAFER than the oracle's buffered/locking logger |
//!
//! **THE ONE PLACE THE OBVIOUS RUST PORT WOULD HAVE BEEN WRONG.**
//! `rustty_os::cgroup::current` (our port of `src/os/cgroup.zig`, which the
//! reference calls here) is written with `format!("/proc/{pid}/cgroup")` and
//! `std::fs::File::open` — i.e. it **allocates**. The Zig reference does not: it
//! builds the path into `var path_buf: [std.fs.max_path_bytes]u8` with
//! `bufPrint`. Calling the allocating version in the forked child risks a
//! deadlock on the allocator's internal lock. So this module hand-rolls an
//! allocation-free equivalent ([`current_cgroup`]) with the same bytes and the
//! same semantics. `rustty_os::cgroup::current` remains correct for its own
//! (non-forked) callers; it is simply not usable here.
//!
//! No heap allocation, no `String`, no `Vec`, no `Mutex`, no `eprintln!`, no
//! `std::fs`, no `format!` appears below this line.
//!
//! PORT NOTE (integration seam — V13, the divergence and its exact shape). The
//! reference's signature is `pub fn preExec(cmd: *Command) ?u8`, and it reads
//! `cmd.rt_pre_exec_info`, a field whose TYPE is comptime-selected as
//! `apprt.runtime.pre_exec.PreExecInfo` (`Command.zig:139-146`):
//!
//! ```zig
//! pub const RtPreExecInfo = if (@hasDecl(apprt.runtime, "pre_exec"))
//!     apprt.runtime.pre_exec.PreExecInfo
//! else struct { pub inline fn init(_: *const configpkg.Config) @This() { return .{}; } };
//! ```
//!
//! Rust has no comptime type selection, and R4 forbids the `rustty-termio` ->
//! `rustty-apprt` edge that naming this type from termio would need (it would
//! cycle: `rustty-apprt` -> `rustty-core` -> `rustty-termio`). So the identity
//! `RtPreExecInfo == apprt.runtime.pre_exec.PreExecInfo` is established from the
//! LOWER crate instead: [`PreExecInfo`] IS
//! [`rustty_termio::command::RtPreExecInfo`], re-exported here under the
//! reference's name. `PreExecInfo::init` likewise lives there
//! (`rustty-termio` already depends on `rustty-config`, so it needs no new
//! edge). Nothing about the FIELDS or `init` differs from `pre_exec.zig:11-23`.
//!
//! The hook FUNCTION half is [`rt_pre_exec`] below — the `PreExecFn`-shaped
//! adapter the apprt installs with `rustty_termio::command::install_rt_hooks`,
//! which is this port's stand-in for `Exec.zig:1034`'s
//! `if (comptime @hasDecl(apprt.runtime, "pre_exec")) apprt.runtime.pre_exec.preExec else null`.
//! [`pre_exec`] itself keeps taking the info EXPLICITLY: it is the
//! async-signal-safe body, and a `&PreExecInfo` parameter is what lets it be
//! unit-tested without a live `Command`.

use rustty_config::value_types::{GtkSingleInstance, LinuxCgroup};
use rustty_termio::command::Command;

/// PORTS: `pre_exec.zig:11-23` `pub const PreExecInfo` — see the module PORT
/// NOTE: the type is DEFINED in `rustty-termio` (as the reference's
/// `Command.RtPreExecInfo`, which the comptime branch makes the SAME type) and
/// re-exported here under the reference's name.
///
/// A `Copy` snapshot of the three config fields the child needs, taken in the
/// PARENT before the fork — the reference does the same (`Command.zig:92`
/// stores it by value in `Command`), which is what makes the child-side reads
/// allocation- and lock-free.
pub use rustty_termio::command::RtPreExecInfo as PreExecInfo;

/// The `Command.PreExecFn`-shaped adapter the apprt installs into
/// `rustty_termio::command::install_rt_hooks` — the reference's
/// `apprt.runtime.pre_exec.preExec` AS SEEN BY `Exec.zig:1034`, whose whole body
/// is the `cmd.rt_pre_exec_info` read the reference does inline.
///
/// # Safety / discipline
/// Runs in the forked child, like [`pre_exec`]: a single `Copy` field read, then
/// the async-signal-safe body.
#[must_use]
pub fn rt_pre_exec(cmd: &mut Command<'_>) -> Option<u8> {
    pre_exec(&cmd.rt_pre_exec_info)
}

/// `log.err("gtk-single-instance is set to detect", .{})` (`pre_exec.zig:39`).
const ERR_SINGLE_INSTANCE_DETECT: &[u8] =
    b"err(gtk_pre_exec): gtk-single-instance is set to detect\n";

/// `log.err("transition to new transient systemd scope took too long", .{})`
/// (`pre_exec.zig:58`).
const ERR_TOO_LONG: &[u8] =
    b"err(gtk_pre_exec): transition to new transient systemd scope took too long\n";

/// "If we are expecting to be moved to a transient systemd scope, wait to see if
/// that happens by checking for the correct name of the current cgroup. Wait at
/// most 250ms so that we don't overly delay the soft-fail scenario.
///
/// If we are configured to hard fail, log an error message and return an error
/// code if we don't detect the move in time."
///
/// PORTS: `pre_exec.zig:26-79` `preExec`. A `Some(code)` return makes
/// `Command.zig:225` `exit(code)` in the child.
///
/// # Safety / discipline
/// Runs in the forked child. See the module doc — every operation below is
/// async-signal-safe.
#[must_use]
pub fn pre_exec(info: &PreExecInfo) -> Option<u8> {
    // `switch (cmd.rt_pre_exec_info.linux_cgroup) { ... }` (`:33-42`).
    match info.linux_cgroup {
        // `.always => {}` (`:34`).
        LinuxCgroup::Always => {}
        // `.never => return null` (`:35`).
        LinuxCgroup::Never => return None,
        // `.@"single-instance" => switch (...gtk_single_instance)` (`:36-41`).
        LinuxCgroup::SingleInstance => match info.gtk_single_instance {
            GtkSingleInstance::True => {}
            GtkSingleInstance::False => return None,
            GtkSingleInstance::Detect => {
                // `log.err("gtk-single-instance is set to detect", .{});
                //  return 127;` (`:39-40`).
                write_stderr(ERR_SINGLE_INSTANCE_DETECT);
                return Some(127);
            }
        },
    }

    // `const pid: u32 = @intCast(std.os.linux.getpid());` (`:44`).
    let pid = unsafe { libc::getpid() } as u32;

    // `var expected_cgroup_buf: [256]u8 = undefined;
    //  const expected_cgroup = cgroup.fmtScope(&expected_cgroup_buf, pid);`
    // (`:46-47`).
    let mut expected_cgroup_buf = [0u8; 256];
    let expected_cgroup_len = super::cgroup::fmt_scope(&mut expected_cgroup_buf, pid).len();
    let expected_cgroup = &expected_cgroup_buf[..expected_cgroup_len];

    // `const start: std.Io.Timestamp = .now(global.io(), .awake);` (`:49`).
    let start = monotonic_millis();

    // `while (true) { ... }` (`:51-77`).
    loop {
        // `if (start.untilNow(...).toMilliseconds() > 250)` (`:52`).
        if monotonic_millis().wrapping_sub(start) > 250 {
            // `if (cmd.rt_pre_exec_info.linux_cgroup_hard_fail) { log.err(...);
            //  return 127; }` (`:53-56`).
            if info.linux_cgroup_hard_fail {
                write_stderr(ERR_TOO_LONG);
                return Some(127);
            }
            // `break;` (`:59`).
            break;
        }

        // `not_found: { ... }` (`:62-73`).
        'not_found: {
            // `var current_cgroup_buf: [4096]u8 = undefined;` (`:63`).
            let mut current_cgroup_buf = [0u8; 4096];

            // `const current_cgroup_raw = internal_os.cgroup.current(
            //     &current_cgroup_buf, @intCast(pid)) orelse break :not_found;`
            // (`:65-68`).
            let Some(len) = current_cgroup(&mut current_cgroup_buf, pid) else {
                break 'not_found;
            };
            let current_cgroup_raw = &current_cgroup_buf[..len];

            // `const index = std.mem.lastIndexOfScalar(u8, current_cgroup_raw,
            //     '/') orelse break :not_found;` (`:70`).
            let Some(index) = current_cgroup_raw.iter().rposition(|&b| b == b'/') else {
                break 'not_found;
            };
            // `const current_cgroup = current_cgroup_raw[index + 1 ..];` (`:71`).
            let current_cgroup = &current_cgroup_raw[index + 1..];

            // `if (std.mem.eql(u8, current_cgroup, expected_cgroup)) return null;`
            // (`:73`).
            if current_cgroup == expected_cgroup {
                return None;
            }
        }

        // `std.Io.sleep(global.io(), .fromMilliseconds(25), .awake) catch
        //  unreachable;` (`:76`).
        sleep_millis(25);
    }

    // `return null;` (`:78`).
    None
}

/// An ALLOCATION-FREE re-expression of `rustty_os::cgroup::current` (the port of
/// `src/os/cgroup.zig:14-36`), safe to call in the forked child.
///
/// "Returns the path to the cgroup for the given pid." Reads the first line of
/// `/proc/<pid>/cgroup` (`0::/user.slice/...`) into `buf` and returns the LENGTH
/// of the third field (everything after the last `:`), right-trimmed of
/// `" \r\n"`. The bytes live in `buf[..len]`.
///
/// PORT NOTE (`rust-mut-args` / byte fidelity): returns a length rather than a
/// borrow so the caller keeps `buf` addressable; bytes stay `[u8]` throughout —
/// never a `String`, lossy or otherwise. This is a cgroup path, and cgroup paths
/// are arbitrary bytes.
fn current_cgroup(buf: &mut [u8], pid: u32) -> Option<usize> {
    // The reference builds the path with `bufPrint` into a stack
    // `[std.fs.max_path_bytes]u8` (`os/cgroup.zig:16,21`). `/proc/` + 10 digits
    // + `/cgroup` + NUL fits comfortably in 32 bytes.
    let mut path = [0u8; 32];
    let mut n = 0usize;
    for &b in b"/proc/" {
        path[n] = b;
        n += 1;
    }
    let mut digits = [0u8; 10];
    let mut d = digits.len();
    let mut v = pid;
    loop {
        d -= 1;
        digits[d] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    while d < digits.len() {
        path[n] = digits[d];
        n += 1;
        d += 1;
    }
    for &b in b"/cgroup\0" {
        path[n] = b;
        n += 1;
    }

    // `std.fs.File.open(path)` (`os/cgroup.zig:22`) — `open(2)` is AS-safe.
    // `O_CLOEXEC` because this fd must not survive the `exec` that follows.
    let fd = unsafe {
        libc::open(
            path.as_ptr().cast::<libc::c_char>(),
            libc::O_RDONLY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        // `.ok()?` on the open (`os/cgroup.zig:22`).
        return None;
    }

    // A single bounded short read into `buf` (`os/cgroup.zig:31`).
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
    unsafe { libc::close(fd) };
    let len = usize::try_from(n).ok()?;
    let contents = &buf[..len];

    // "Find the last ':'" (`os/cgroup.zig:35`).
    let idx = contents.iter().rposition(|&b| b == b':')?;

    // `std.mem.trimRight(u8, slice, " \r\n")` (`os/cgroup.zig:36`).
    let start = idx + 1;
    let mut end = len;
    while end > start && matches!(buf[end - 1], b' ' | b'\r' | b'\n') {
        end -= 1;
    }

    // Shift the field to the front of `buf` so the caller can return a length.
    let field_len = end - start;
    buf.copy_within(start..end, 0);
    Some(field_len)
}

/// `std.Io.Timestamp.now(global.io(), .awake)` (`pre_exec.zig:49`) in
/// milliseconds. `.awake` is the MONOTONIC clock (it does not tick while
/// suspended), so `CLOCK_MONOTONIC` is the match. `clock_gettime` is
/// async-signal-safe.
fn monotonic_millis() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, live `timespec`; `CLOCK_MONOTONIC` always exists.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &raw mut ts) };
    (ts.tv_sec as u64).wrapping_mul(1000) + (ts.tv_nsec as u64) / 1_000_000
}

/// `std.Io.sleep(global.io(), .fromMilliseconds(ms), .awake)`
/// (`pre_exec.zig:76`). `nanosleep` is async-signal-safe.
fn sleep_millis(ms: i64) {
    let ts = libc::timespec {
        tv_sec: ms / 1000,
        tv_nsec: (ms % 1000) * 1_000_000,
    };
    // SAFETY: `ts` is a valid `timespec` with a normalized `tv_nsec`; passing a
    // null `rem` is permitted (an EINTR simply shortens the sleep, exactly as
    // the reference's `catch unreachable` would tolerate).
    unsafe { libc::nanosleep(&raw const ts, core::ptr::null_mut()) };
}

/// The reference's `log.err(...)` (`pre_exec.zig:39,58`) — but rendered as a raw
/// `write(2)` of a `&'static [u8]`.
///
/// Rust's `eprintln!` takes the process-wide `Stderr` lock and formats through a
/// buffered writer; neither is async-signal-safe, and a `fork` that races
/// another thread holding that lock would deadlock the child. `write(2)` is on
/// POSIX's async-signal-safe list, so this is STRICTLY SAFER than the oracle's
/// own logger while emitting the same bytes to the same fd.
fn write_stderr(msg: &'static [u8]) {
    // SAFETY: `msg` is a live `'static` byte slice; fd 2 is stderr. A short or
    // failed write is ignored, as a logger's would be.
    unsafe {
        let _ = libc::write(2, msg.as_ptr().cast::<libc::c_void>(), msg.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `.never` short-circuits before any syscall (`pre_exec.zig:35`).
    #[test]
    fn never_returns_none() {
        let info = PreExecInfo {
            gtk_single_instance: GtkSingleInstance::True,
            linux_cgroup: LinuxCgroup::Never,
            linux_cgroup_hard_fail: true,
        };
        assert_eq!(pre_exec(&info), None);
    }

    /// `.single-instance` + `gtk-single-instance=false` short-circuits
    /// (`pre_exec.zig:38`).
    #[test]
    fn single_instance_false_returns_none() {
        let info = PreExecInfo {
            gtk_single_instance: GtkSingleInstance::False,
            linux_cgroup: LinuxCgroup::SingleInstance,
            linux_cgroup_hard_fail: true,
        };
        assert_eq!(pre_exec(&info), None);
    }

    /// `.single-instance` + `detect` is the impossible-state exit code 127
    /// (`pre_exec.zig:38-40`).
    #[test]
    fn single_instance_detect_returns_127() {
        let info = PreExecInfo {
            gtk_single_instance: GtkSingleInstance::Detect,
            linux_cgroup: LinuxCgroup::SingleInstance,
            linux_cgroup_hard_fail: false,
        };
        assert_eq!(pre_exec(&info), Some(127));
    }

    /// `.always` with `hard_fail` set waits 250ms for a scope that will never
    /// appear (this test process is not in a rustty transient scope) and then
    /// returns 127 (`pre_exec.zig:52-58`).
    #[test]
    fn always_hard_fail_times_out_with_127() {
        let info = PreExecInfo {
            gtk_single_instance: GtkSingleInstance::True,
            linux_cgroup: LinuxCgroup::Always,
            linux_cgroup_hard_fail: true,
        };
        let t0 = monotonic_millis();
        assert_eq!(pre_exec(&info), Some(127));
        assert!(
            monotonic_millis().wrapping_sub(t0) >= 250,
            "must have waited the full 250ms budget"
        );
    }

    /// The soft-fail path breaks out of the loop and returns `None`
    /// (`pre_exec.zig:59,78`).
    #[test]
    fn always_soft_fail_times_out_with_none() {
        let info = PreExecInfo {
            gtk_single_instance: GtkSingleInstance::True,
            linux_cgroup: LinuxCgroup::Always,
            linux_cgroup_hard_fail: false,
        };
        assert_eq!(pre_exec(&info), None);
    }

    /// [`current_cgroup`] must agree byte-for-byte with the allocating
    /// `rustty_os::cgroup::current` it stands in for.
    #[test]
    fn current_cgroup_matches_rustty_os_cgroup() {
        let pid = unsafe { libc::getpid() } as u32;

        let mut ours = [0u8; 4096];
        let ours = current_cgroup(&mut ours, pid).map(|n| ours[..n].to_vec());

        let mut theirs_buf = [0u8; 4096];
        let theirs = rustty_os::cgroup::current(&mut theirs_buf, pid).map(<[u8]>::to_vec);

        assert_eq!(ours, theirs);
    }
}
