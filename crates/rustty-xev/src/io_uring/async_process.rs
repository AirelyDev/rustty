//! The io_uring backend's `Async` and `Process` watchers.
//!
//! PORTS: libxev src/watcher/async.zig `AsyncEventFd` and src/watcher/process.zig
//! `ProcessPidFd`, monomorphized for the io_uring backend.

use super::*;

// ---------------------------------------------------------------------------
// Async (watcher/async.zig `AsyncEventFd`) — the cross-thread loop wakeup,
// INSIDE this backend. `Async(xev)` selects `AsyncEventFd` for the io_uring and
// epoll backends (async.zig:14-16), and its `wait` is the `waitPoll` arm
// (async.zig:86).
// ---------------------------------------------------------------------------

/// "Wake up" an event loop from any thread using an async completion.
//
// PORT NOTE: `AsyncEventFd(xev)` monomorphized for io_uring. `#[derive(Clone,
// Copy)]`: the reference `wait`/`notify` take `self: Self` by value (a trivially
// copyable fd handle), exactly like `linux::eventfd::Eventfd`.
//
// PORT NOTE: the reference's `fd: posix.fd_t` + inline `posix.eventfd`/`read`/
// `write` calls are expressed through the crate's already-ported
// `linux::eventfd::Eventfd` primitive (Task 2), which IS that fd plus the very
// same `eventfd(0, EFD_CLOEXEC|EFD_NONBLOCK)` / `write(1u64)` /
// `EAGAIN => success` surface. The FreeBSD `extern "c" fn eventfd` arm is
// comptime-dead here (this module is Linux-only).
#[derive(Clone, Copy)]
pub struct Async {
    /// eventfd file descriptor
    ev: Eventfd,
}

impl Async {
    /// Create a new async. An async can be assigned to exactly one loop
    /// to be woken up. The completion must be allocated in advance.
    pub fn init() -> core::result::Result<Async, Error> {
        match Eventfd::init() {
            Ok(ev) => Ok(Async { ev }),
            Err(_) => Err(Error),
        }
    }

    /// Clean up the async. This will forcibly deinitialize any resources
    /// and may result in erroneous wait callbacks to be fired.
    //
    // PORT NOTE: the reference receiver is `self: *Async`; ported as `&mut self`
    // to match — the same choice the darwin `Async.deinit` records. Callers bind
    // their handle `mut` for it (the `Thread.init` errdefer chains), and a `&self`
    // receiver here would make those bindings warn `unused_mut` on Linux only.
    pub fn deinit(&mut self) {
        self.ev.deinit();
    }

    /// The underlying eventfd. Exposed so a caller can poll/register it directly.
    pub fn fd(&self) -> fd_t {
        self.ev.fd
    }

    /// Wait for a message on this async. Note that async messages may be
    /// coalesced (or they may not be) so you should not expect a 1:1 mapping
    /// between send and wait.
    ///
    /// Just like the rest of libxev, the wait must be re-queued if you want
    /// to continue to be notified of async events.
    ///
    /// You should NOT register an async with multiple loops (the same loop
    /// is fine -- but unnecessary). The behavior when waiting on multiple
    /// loops is undefined.
    //
    // PORT NOTE: `pub const wait = switch (xev.backend) { .io_uring, .epoll =>
    // waitPoll, ... }` — this IS `waitPoll`; the `waitRead` (kqueue) arm is
    // comptime-dead for io_uring.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `wait::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE. `WaitError = xev.ReadError`.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
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
                core::result::Result<(), ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Async::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // PORT NOTE (verbatim from the reference): We use a poll operation
            // instead of a read operation because in Kernel 6.15.4, read was
            // regressed for io_uring on eventfd/timerfd and would block forever.
            // However, poll works fine.
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.ev.fd,
                    events: libc::POLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::wait_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn wait_trampoline<UD, CB>(
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
                core::result::Result<(), ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let mapped: core::result::Result<(), ReadError> = match r {
            Result::Poll(res) => res.map_err(|e| match e {
                // PORT NOTE: `WaitError = xev.ReadError`, but the io_uring `poll` op
                // yields a `PollError`. The reference's `if (r.poll) |_| {} else
                // |err| err` relies on Zig's error-set COERCION: `PollError`'s
                // `{Canceled, Unexpected}` are both members of `ReadError`
                // (`{EOF, Canceled, Unexpected, ConnectionResetByPeer}`), so the
                // coercion is total. Rust has no error-set subtyping, so the
                // same total mapping is spelled out.
                PollError::Canceled => ReadError::Canceled,
                PollError::Unexpected => ReadError::Unexpected,
            }),
            _ => unreachable!(),
        };

        if mapped.is_ok() {
            // PORT NOTE (verbatim): We need to read so that we can consume the
            // eventfd value. We only read 8 bytes because we only write up to 8
            // bytes and we own the fd. We ignore errors here because we expect the
            // read to succeed given we just polled it.
            let fd = match unsafe { &(*c_inner).op } {
                Operation::Poll { fd, .. } => *fd,
                _ => unreachable!(),
            };
            let mut buf = [0u8; 8];
            // SAFETY: an 8-byte read into a live local buffer off our own eventfd.
            // The result is discarded exactly as the reference's `_ = posix.read(
            // c_inner.op.poll.fd, &buf) catch {}` discards it.
            let _ = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<c_void>(), buf.len()) };
        }

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
    }

    /// Notify a loop to wake up synchronously. This should never block forever
    /// (it will always EVENTUALLY succeed regardless of if the loop is currently
    /// ticking or not).
    ///
    /// The "c" value is the completion associated with the "wait".
    ///
    /// Internal details subject to change but if you're relying on these
    /// details then you may want to consider using a lower level interface
    /// using the loop directly:
    ///
    ///   - linux+io_uring: eventfd is used. If the eventfd write would block
    ///     (EAGAIN) then we assume success because the eventfd is full.
    pub fn notify(&self) -> core::result::Result<(), Error> {
        // `Eventfd::notify` IS the reference's `write(fd, &@bitCast(@as(u64, 1)))`
        // plus its `error.WouldBlock => return` arm.
        self.ev.notify().map_err(|_| Error)
    }
}

// ---------------------------------------------------------------------------
// Process (watcher/process.zig `ProcessPidFd`) — the Linux subprocess-exit
// watcher, INSIDE this backend (mirroring epoll.rs). `Process(xev)` selects
// `ProcessPidFd` for io_uring and epoll alike (process.zig:13-16); the ONLY
// backend-dependent bit is the poll event mask (`.io_uring => posix.POLL.IN`
// vs `.epoll => linux.EPOLL.IN`, process.zig:80-84).
// ---------------------------------------------------------------------------

/// PORT NOTE: `pub const WaitError = xev.Sys.PollError || error{InvalidChild}`.
/// The error that can come in the wait callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessWaitError {
    InvalidChild,
    Unexpected,
}

/// Process implementation using pidfd (Linux).
//
// PORT NOTE: `ProcessPidFd` monomorphized. `#[derive(Clone, Copy)]`: the reference
// `wait` takes `self: Self` by value (a trivially-copyable `fd` handle).
#[derive(Clone, Copy)]
pub struct Process {
    /// pidfd file descriptor
    fd: fd_t,
}

impl Process {
    /// Create a new process watcher for the given pid.
    //
    // PORT NOTE: `linux.pidfd_open(pid, posix.SOCK.NONBLOCK)` (== PIDFD_NONBLOCK).
    // `pidfd_open` has no direct `libc` binding, so it goes through the raw
    // `SYS_pidfd_open` syscall. The reference maps each errno to a specific error;
    // the D6 seam collapses those to the unified `crate::Error` (identical to
    // epoll.rs's `Process::init`).
    pub fn init(pid: pid_t) -> core::result::Result<Process, Error> {
        // SAFETY: raw pidfd_open(2) syscall; returns a new fd or -1/errno.
        let res = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, libc::PIDFD_NONBLOCK) };
        if res < 0 {
            return Err(Error);
        }
        Ok(Process { fd: res as fd_t })
    }

    /// Clean up the process watcher.
    pub fn deinit(&mut self) {
        // SAFETY: closing our own pidfd.
        unsafe { libc::close(self.fd) };
    }

    /// Wait for the process to exit. This will automatically call
    /// `waitpid` or equivalent and report the exit status.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `wait::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE. One untyped loop trampoline per
    // call-site; `loop`/`c` stay RAW.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
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
                core::result::Result<u32, ProcessWaitError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Process::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        // c.* = .{ .op = .{ .poll = { .fd, .events } }, ... };
        unsafe {
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    // io_uring arm: `const events: u32 = posix.POLL.IN;`
                    // (epoll's arm is `linux.EPOLL.IN`).
                    events: libc::POLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    /// The monomorphized untyped loop trampoline: on the pidfd signalling
    /// readable it reaps the child via `waitid(P_PIDFD)` then calls the typed `cb`.
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
                core::result::Result<u32, ProcessWaitError>,
            ) -> CallbackAction
            + 'static,
    {
        // const arg: WaitError!u32 = arg: { ... };
        let arg: core::result::Result<u32, ProcessWaitError> = 'arg: {
            // If our poll failed, report that error.
            // _ = r.poll catch |err| break :arg err;
            match r {
                Result::Poll(Ok(())) => {}
                Result::Poll(Err(_)) => break 'arg Err(ProcessWaitError::Unexpected),
                _ => unreachable!(),
            }

            // We need to wait on the pidfd because it is noted as ready.
            // const fd = c_inner.op.poll.fd;
            let fd = match unsafe { &(*c_inner).op } {
                Operation::Poll { fd, .. } => *fd,
                _ => unreachable!(),
            };
            // var info: siginfo_t; res = waitid(.PIDFD, fd, &info, W.EXITED);
            let mut info: libc::siginfo_t = unsafe { core::mem::zeroed() };
            let res =
                unsafe { libc::waitid(libc::P_PIDFD, fd as libc::id_t, &mut info, libc::WEXITED) };
            if res == 0 {
                // .SUCCESS => info.fields.common.second.sigchld.status
                break 'arg Ok(unsafe { info.si_status() } as u32);
            }
            match errno() {
                libc::ECHILD => Err(ProcessWaitError::InvalidChild),
                // The fd isn't ready to read, I guess?
                libc::EAGAIN => return CallbackAction::Rearm,
                _ => Err(ProcessWaitError::Unexpected),
            }
        };

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, arg)
    }
}
