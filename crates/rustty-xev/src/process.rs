//! process.rs — faithful port of libxev `watcher/process.zig` `ProcessKqueue`
//! (D6, XEV_BLUEPRINT §4/§7/§8, chunk C5).
//!
//! Reference (process.zig:8): "Process management, such as waiting for process
//! exit."
//!
//! `ProcessKqueue` is the macOS subprocess-exit watcher: it rides an
//! `EVFILT_PROC` (`NOTE_EXIT | NOTE_EXITSTATUS`) completion and, on fire, reaps
//! the child with `waitpid(pid, 0)` and reports the decoded exit status. termio's
//! Exec names `xev::Process` (`xev.Process.init(pid)` in `threadEnter`, the
//! subprocess exit-wait) and default/`wait()`s it. C5 completes the watcher
//! surface: `init`/`deinit`/`wait` (the `proc`-op trampoline) + the `ProcessTests`
//! (process×3) + rustty KAT 3 (posix_spawn `/bin/true` → wait → exit status 0).
//! The non-kqueue `ProcessPidFd`/`ProcessIocp`/`ProcessDynamic` variants are
//! comptime-elided on macOS (§0) and ported later as whole backends (§1); their
//! reference doc comments are preserved below (R3).
//
// PORT NOTE: the reference is `fn Process(comptime xev: type) type` — a comptime
// dispatcher on `xev.backend`. Monomorphized for the single macOS backend (xev =
// kqueue, §0), it is this concrete `Process` = `ProcessKqueue`. It is
// `#[cfg(target_os = "macos")]` (EVFILT_PROC + Darwin waitpid reap) exactly as the
// whole kqueue port is.

// (module is `#[cfg(target_os = "macos")]`-gated in lib.rs)

use crate::kqueue::{pid_t, Completion, Loop, Operation, ProcError, Result, NOTE_EXIT_FLAGS};
use crate::looppkg::CallbackAction;
use crate::Error;
use core::ffi::c_void;
use core::ptr::NonNull;

// ---------------------------------------------------------------------------
// PORT NOTE (§0/§7): the sibling `Process` variants are COMPTIME-ELIDED on macOS and
// DEFERRED (not weakened). The reference `pub fn Process(comptime xev: type) type`
// — doc comment "Process management, such as waiting for process exit." — dispatches
// on `xev.backend`:
//
//     // Supported, uses pidfd
//     .io_uring, .epoll => ProcessPidFd(xev),
//     .kqueue           => ProcessKqueue(xev),
//     .iocp             => ProcessIocp(xev),
//     // Unsupported
//     .wasi_poll        => struct {},
//
// On macOS the single backend candidate is kqueue, so ONLY `ProcessKqueue` (below)
// instantiates. The others are ported as whole backends later (blueprint §1); their
// reference doc comments are preserved here (R3) so nothing is dropped:
//
//   ProcessPidFd (Linux, pidfd) — verbatim reference comment lines (R3):
//     /// Process implementation using pidfd (Linux).
//     /// The error that can come in the wait callback.
//     /// pidfd file descriptor
//     /// Create a new process watcher for the given pid.
//     // Note: SOCK_NONBLOCK == PIDFD_NONBLOCK but we should PR that
//     // over to Zig.
//     /// Clean up the process watcher.
//     /// Wait for the process to exit. This will automatically call
//     /// `waitpid` or equivalent and report the exit status.
//     // If our poll failed, report that error.
//     // We need to wait on the pidfd because it is noted as ready
//     // The fd isn't ready to read, I guess?
//
//   ProcessIocp (Windows, job object) — verbatim reference comment lines (R3):
//     // Duplicate the process handle so we don't rely on the caller keeping it alive
//     // There was a period of time between when the job object was created
//     // and when it was associated with the completion port. We may have
//     // missed a notification, so check if it's still alive.
//
//   ProcessDynamic (multi-backend): code-only, no distinctive doc comments;
//   deferred with the other backends.
// ---------------------------------------------------------------------------

/// PORT NOTE: `pub const WaitError = xev.Sys.ProcError;`.
/// The error that can come in the wait callback.
pub type WaitError = ProcError;

/// PORT NOTE: `ProcessKqueue` monomorphized. `#[derive(Clone, Copy)]`: the reference
/// `wait` takes `self: Self` by value (a trivially-copyable `pid` handle) and
/// termio holds `xev::Process` fields by value; the `pid: pid_t` field makes it
/// auto-`Send`.
#[derive(Clone, Copy)]
pub struct Process {
    /// The pid to watch.
    pid: pid_t,
}

impl Process {
    /// Create a new process watcher for the given pid.
    //
    // PORT NOTE: the reference error is `!Self` (the kqueue arm never fails —
    // `init` only stores the pid); per the D6 seam (blueprint §2.1 / termio
    // `xev::Process::init(pid)?`), the fallible signature collapses to the unified
    // `crate::Error`, and the kqueue arm always returns `Ok`.
    pub fn init(pid: pid_t) -> core::result::Result<Process, Error> {
        Ok(Process { pid })
    }

    /// Does nothing for Kqueue.
    //
    // PORT NOTE: the reference receiver is `self: *Self`; ported as `&mut self`
    // (termio binds `mut process` and calls `process.deinit()`).
    pub fn deinit(&mut self) {
        let _ = self;
    }

    /// Wait for the process to exit. This will automatically call
    /// `waitpid` or equivalent and report the exit status.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata: type` + `userdata:
    // ?*Userdata` + `comptime cb: *const fn(...)` monomorphize to a generic
    // `wait::<UD, CB>` where `CB` is the callback's ZST fn-item/non-capturing-
    // closure TYPE (Zig's comptime-known `cb` value ⇒ a Rust zero-sized type, NOT
    // a runtime fn pointer). Each call-site emits exactly one untyped loop
    // trampoline `trampoline::<UD, CB>` with zero allocation and one indirection —
    // matching Zig's comptime `struct{fn callback}` (no `dyn`, no boxing). `self`
    // is by value (Copy); `loop`/`c` stay RAW pointers (`rust-mut-args-strongly-
    // protected`: the loop re-enters `c` across ticks, a `&mut` re-read would be UB).
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer that outlives the wait,
    /// and `loop_` a valid `Loop` pointer; both are held raw by the loop.
    pub unsafe fn wait<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<u32, WaitError>,
            ) -> CallbackAction
            + 'static,
    {
        // The typed `cb` must be a zero-sized comptime-known value (a fn-item or
        // non-capturing closure) so the trampoline can reconstruct it with no
        // state — exactly Zig's comptime capture. `cb` itself is dropped here.
        const {
            assert!(
                core::mem::size_of::<CB>() == 0,
                "Process::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        // SAFETY: `c` is a caller-owned, stable Completion; we configure it as a
        // `.proc` op (a full reset, per `Completion::new_op`) and hand it to the
        // loop by raw pointer (never a borrow).
        unsafe {
            *c = Completion::new_op(
                Operation::Proc {
                    pid: self.pid,
                    flags: NOTE_EXIT_FLAGS,
                },
                userdata.cast::<c_void>(),
                Self::trampoline::<UD, CB>,
            );

            (*loop_).add(c);
        }
    }

    /// The monomorphized untyped loop trampoline for a given (`UD`, `CB`). On fire
    /// it reaps the child (`waitpid`) then calls the typed `cb` with the exit status.
    fn trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<u32, WaitError>,
            ) -> CallbackAction
            + 'static,
    {
        // Reap the process. We do this because our xev.Process
        // docs note that this is the equivalent of calling
        // `wait` on a process. The Linux side (pidfd) does this
        // automatically since the `waitid` syscall is used.
        //
        // if (r.proc) |_| { _ = posix.waitpid(c_inner.op.proc.pid, 0); } else |_| {}
        if let Result::Proc(Ok(_)) = r {
            // SAFETY: this trampoline is only ever installed by `wait` on a `.proc`
            // op, so `c_inner`'s op carries the pid to reap.
            let pid = match unsafe { &(*c_inner).op } {
                Operation::Proc { pid, .. } => *pid,
                _ => unreachable!(),
            };
            // SAFETY: `waitpid` reads only the local `status` out-param; POD FFI.
            let mut status: libc::c_int = 0;
            unsafe { libc::waitpid(pid, &mut status, 0) };
        }

        // Reconstruct the zero-sized `CB` value — the Rust equivalent of Zig
        // baking `comptime cb` into the monomorphized trampoline.
        //
        // SAFETY: `CB` is a ZST (asserted in `wait`), so reading it through an
        // aligned dangling pointer accesses zero bytes and is sound (ZST reads
        // never dereference).
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };

        let ud_typed = crate::common::userdata_value::<UD>(ud);
        // if (r.proc) |v| v else |err| err
        let mapped: core::result::Result<u32, WaitError> = match r {
            Result::Proc(res) => res,
            _ => unreachable!(),
        };
        cb(ud_typed, l_inner, c_inner, mapped)
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE: `ProcessTests(xev, Impl)` (process.zig:455). On macOS the single `Process`
// instantiation is `ProcessKqueue` (§0/§7), so `Impl == Process`. The three
// reference `test{}` land as live Rust tests, plus rustty KAT 3 (§7) which proves
// the spawn+exit round-trip on a real kqueue (Miri can't — real EVFILT_PROC /
// waitpid syscalls). `#[cfg(target_os = "macos")]` via the module gate.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::kqueue::{Completion, Loop};
    use crate::looppkg::RunMode;
    use std::process::Command;

    // PORT NOTE: `argv_0`/`argv_42` — the `builtin.os.tag` switch's non-windows arm
    // (`&.{ "sh", "-c", "exit N" }`); the `.windows => &.{ "cmd.exe", "/C", "exit N" }`
    // arm is comptime-elided on this macOS-gated module.
    const ARGV_0: [&str; 3] = ["sh", "-c", "exit 0"];
    const ARGV_42: [&str; 3] = ["sh", "-c", "exit 42"];

    // test "process wait"
    //
    // PORT NOTE: like the reference test, we do NOT `child.wait()` here — the
    // xev `Process.wait` trampoline reaps the child via `waitpid` (that IS the
    // point of the watcher). clippy's `zombie_processes` lint can't see the FFI
    // reap, so it is allowed with this note (the process is reaped, not leaked).
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C5, .port/phase-2/MIRI_EXCEPTIONS.md): real posix_spawn/waitpid + EVFILT_PROC via the live kqueue loop -- Miri cannot execute foreign functions; runs natively + ASan (sanitizers.sh)"
    )]
    #[allow(clippy::zombie_processes)]
    fn process_wait() {
        // var child = std.process.Child.init(argv_0, alloc); try child.spawn();
        let child = Command::new(ARGV_0[0]).args(&ARGV_0[1..]).spawn().unwrap();

        let mut loop_ = Loop::init().unwrap();
        // defer loop.deinit()

        let mut p = Process::init(child.id() as pid_t).unwrap();
        // defer p.deinit()

        // Wait
        let mut code: Option<u32> = None;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut Option<u32>,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<u32, WaitError>| {
            unsafe { *ud = Some(r.unwrap()) }; // ud.?.* = r catch unreachable
            CallbackAction::Disarm
        };
        unsafe {
            p.wait(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(code),
                cb,
            );
        }

        // Wait for wake
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(code, Some(0));

        p.deinit();
        loop_.deinit();
    }

    // test "process wait with non-zero exit code"
    //
    // PORT NOTE: as in `process_wait`, the xev trampoline reaps the child via
    // `waitpid`; clippy's `zombie_processes` lint is allowed for the same reason.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C5, .port/phase-2/MIRI_EXCEPTIONS.md): real posix_spawn/waitpid + EVFILT_PROC via the live kqueue loop -- Miri cannot execute foreign functions; runs natively + ASan (sanitizers.sh)"
    )]
    #[allow(clippy::zombie_processes)]
    fn process_wait_with_non_zero_exit_code() {
        // if (builtin.os.tag == .freebsd) return error.SkipZigTest;
        // (macOS is not freebsd; the freebsd skip does not apply here.)
        let child = Command::new(ARGV_42[0])
            .args(&ARGV_42[1..])
            .spawn()
            .unwrap();

        let mut loop_ = Loop::init().unwrap();
        // defer loop.deinit()

        let mut p = Process::init(child.id() as pid_t).unwrap();
        // defer p.deinit()

        // Wait
        let mut code: Option<u32> = None;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut Option<u32>,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<u32, WaitError>| {
            unsafe { *ud = Some(r.unwrap()) }; // ud.?.* = r catch unreachable
            CallbackAction::Disarm
        };
        unsafe {
            p.wait(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(code),
                cb,
            );
        }

        // Wait for wake
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(code, Some(42));

        p.deinit();
        loop_.deinit();
    }

    // test "process wait on a process that already exited"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C5, .port/phase-2/MIRI_EXCEPTIONS.md): real posix_spawn/waitpid + EVFILT_PROC via the live kqueue loop -- Miri cannot execute foreign functions; runs natively + ASan (sanitizers.sh)"
    )]
    fn process_wait_on_a_process_that_already_exited() {
        let mut child = Command::new(ARGV_0[0]).args(&ARGV_0[1..]).spawn().unwrap();

        let mut loop_ = Loop::init().unwrap();
        // defer loop.deinit()

        let mut p = Process::init(child.id() as pid_t).unwrap();
        // defer p.deinit()

        // _ = try child.wait();
        let _ = child.wait().unwrap();

        // Wait
        let mut code: Option<u32> = None;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut Option<u32>,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<u32, WaitError>| {
            unsafe { *ud = Some(r.unwrap_or(0)) }; // ud.?.* = r catch 0
            CallbackAction::Disarm
        };
        unsafe {
            p.wait(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(code),
                cb,
            );
        }

        // Wait for wake
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(code, Some(0));

        p.deinit();
        loop_.deinit();
    }

    // KAT 3 (XEV_BLUEPRINT §7) — Process spawn+exit: `posix_spawn` `/bin/true`;
    // `Process.wait`; `run(.until_done)`; assert exit status 0. Proves the loop
    // reaps + decodes NOTE_EXIT/NOTE_EXITSTATUS on a live kqueue (Miri can't — real
    // EVFILT_PROC / waitpid syscalls).
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C5, .port/phase-2/MIRI_EXCEPTIONS.md): real posix_spawn/waitpid + EVFILT_PROC via the live kqueue loop -- Miri cannot execute foreign functions; runs natively + ASan (sanitizers.sh)"
    )]
    fn kat3_process_spawn_exit() {
        // posix_spawn /bin/true (canonicalized to /usr/bin/true — the `true`
        // coreutil's real path on modern macOS, where /bin/true no longer ships;
        // the KAT only needs a real process that exits 0).
        let mut pid: libc::pid_t = 0;
        let path = b"/usr/bin/true\0";
        let mut argv: [*mut libc::c_char; 2] = [
            path.as_ptr() as *mut libc::c_char, // argv[0] = "/usr/bin/true"
            core::ptr::null_mut(),
        ];
        let mut envp: [*mut libc::c_char; 1] = [core::ptr::null_mut()];
        // SAFETY: `posix_spawn` reads the NUL-terminated `path`/`argv`/`envp` and
        // writes the spawned pid to `pid`; all POD buffers live for the call.
        let rc = unsafe {
            libc::posix_spawn(
                &mut pid,
                path.as_ptr() as *const libc::c_char,
                core::ptr::null(), // file_actions
                core::ptr::null(), // attrp
                argv.as_mut_ptr() as *const *mut libc::c_char,
                envp.as_mut_ptr() as *const *mut libc::c_char,
            )
        };
        assert_eq!(rc, 0, "posix_spawn /usr/bin/true");

        let mut loop_ = Loop::init().unwrap();

        let mut p = Process::init(pid).unwrap();

        // Wait
        let mut code: Option<u32> = None;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut Option<u32>,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<u32, WaitError>| {
            unsafe { *ud = Some(r.unwrap()) };
            CallbackAction::Disarm
        };
        unsafe {
            p.wait(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(code),
                cb,
            );
        }

        // Wait for wake
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(code, Some(0), "exit status 0");

        p.deinit();
        loop_.deinit();
    }
}
