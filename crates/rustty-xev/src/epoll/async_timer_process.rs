//! The epoll backend's `Async`, `Timer` and `Process`.
//!
//! PORTS: libxev src/watcher/async.zig `AsyncEventFd`, the epoll.zig heap `Timer`,
//! and src/watcher/process.zig `ProcessPidFd`, monomorphized for the epoll
//! backend.

use super::*;

// ---------------------------------------------------------------------------
// Async (watcher/async.zig `AsyncEventFd`) — the cross-thread loop wakeup,
// INSIDE this backend (mirroring io_uring.rs). `Async(xev)` selects
// `AsyncEventFd` for the io_uring AND epoll backends alike (async.zig:14-16),
// and its `wait` is the `waitPoll` arm for both (async.zig:86) — this watcher is
// byte-identical between the two Linux backends; only the module-local
// `Loop`/`Completion`/`Operation`/`Result` types it names differ.
//
// PORT NOTE (SCOPE, task L1 LT7): Tasks 3/4 did not port this watcher (the epoll
// `Loop` embeds a bare `linux::eventfd::Eventfd` for its own internal wakeup
// instead), but `dynamic.rs` must union `epoll::Async` with `io_uring::Async` —
// `Async(Dynamic)` is `AsyncDynamic` over BOTH candidates (async.zig:9-10 +
// dynamic.zig:82). It lands here, additively, so both Linux backends expose the
// same watcher surface.
// ---------------------------------------------------------------------------

/// "Wake up" an event loop from any thread using an async completion.
//
// PORT NOTE: `AsyncEventFd(xev)` monomorphized for epoll. `#[derive(Clone,
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
    // comptime-dead for epoll.
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
            //
            // PORT NOTE: `waitPoll` spells the mask `posix.POLL.IN` for BOTH the
            // io_uring and epoll arms (async.zig:145) even though this backend's
            // `Operation::Poll.events` is documented as EPOLL events. That is
            // benign upstream and here: `POLLIN == EPOLLIN == 0x1`. Kept as the
            // reference spells it rather than "fixed" to `EPOLLIN`.
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
                // PORT NOTE: `WaitError = xev.ReadError`, but the epoll `poll` op
                // yields a `PollError`. The reference's `if (r.poll) |_| {} else
                // |err| err` relies on Zig's error-set COERCION: this backend's
                // `PollError` (`{Unexpected}`) is a subset of its `ReadError`
                // (`{EOF, Unexpected}`), so the coercion is total. Rust has no
                // error-set subtyping, so the same total mapping is spelled out.
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
// Timer (heap node) (epoll.zig:1235).
// ---------------------------------------------------------------------------

/// Timer that is inserted into the heap.
pub struct Timer {
    /// The absolute time to fire this timer next.
    pub(super) next: Timespec,

    /// Only used internally. If this is non-null and timer is CANCELLED, then
    /// the timer is rearmed automatically with this as the next time. The
    /// callback will not be called on the cancellation.
    pub(super) reset: Option<Timespec>,

    /// Internal heap fields.
    pub(super) heap: heap::IntrusiveField<Timer>,

    /// We point back to completion for now.
    /// [1]: https://github.com/ziglang/zig/issues/6611
    pub(super) c: *mut Completion,
}

// PORT NOTE: the intrusive `heap` field seam for `heap::Intrusive<Timer, ()>`.
impl heap::Node for Timer {
    unsafe fn heap_field(this: *mut Self) -> *mut heap::IntrusiveField<Self> {
        unsafe { ptr::addr_of_mut!((*this).heap) }
    }
}

/// The address of `c`'s `.timer` payload, carrying `c`'s OWN provenance.
///
/// PORT NOTE (N434): Zig reaches the timer as the place `c.op.timer` (a plain
/// `*Timer` into the completion), and the heap links it. The Rust port must not
/// derive that pointer from a `&mut (*c).op` reborrow: the heap keeps it while the
/// timer is linked, and the next `&mut (*c).op` (`timer_reset`, `stop_completion`)
/// is a fresh Unique retag from `c` that pops it under Stacked Borrows (and, once
/// the heap has written the node's links through it, is a Tree Borrows conflict).
/// This helper forms only a shared borrow of the op to find the payload and
/// returns that address with `c`'s tag, so the heap's links and every later
/// access on the heap path share one tag. Every heap-path Timer access goes
/// through it (proof: `.port/phase-2/mirihar-n434`).
///
/// # Safety
/// `c` must be a valid `Completion` pointer whose op is `.timer`.
#[inline]
pub(super) unsafe fn timer_ptr(c: *mut Completion) -> *mut Timer {
    unsafe {
        let addr = match &(*c).op {
            Operation::Timer(t) => ptr::from_ref(t).addr(),
            _ => unreachable!(),
        };
        c.cast::<u8>().with_addr(addr).cast::<Timer>()
    }
}

impl Timer {
    /// PORT NOTE: `fn less(_: void, a: *const Timer, b: *const Timer) bool`.
    pub(super) fn less(_: (), a: *mut Timer, b: *mut Timer) -> bool {
        unsafe { Timer::ns(a) < Timer::ns(b) }
    }

    /// Returns the nanoseconds of this timer. maxInt(u64) ns is 584 years, so
    /// any overflow just uses maxInt(u64).
    ///
    /// # Safety
    /// `this` must be a valid `Timer` pointer.
    unsafe fn ns(this: *mut Timer) -> u64 {
        let next = unsafe { (*this).next };
        debug_assert!(next.tv_sec >= 0);
        debug_assert!(next.tv_nsec >= 0);

        let max = u64::MAX;
        let s_ns = match (next.tv_sec as u64).checked_mul(ns_per_s) {
            Some(v) => v,
            None => return max,
        };
        s_ns.checked_add(next.tv_nsec as u64).unwrap_or(max)
    }
}

// ---------------------------------------------------------------------------
// Process (watcher/process.zig `ProcessPidFd`) — the Linux subprocess-exit
// watcher. Lives INSIDE this backend (per the brief): a `pidfd_open`'d fd,
// registered readable via the loop's `poll` op, reaped with `waitid(P_PIDFD)`.
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
    // the D6 seam collapses those to the unified `crate::Error`.
    //
    // PORT NOTE (SIGCHLD fallback): the brief notes an old-kernel `pidfd_open`
    // ENOSYS fallback to a SIGCHLD/`waitpid` path. libxev's `ProcessPidFd` has NO
    // such fallback (it returns `unexpectedErrno`), and `pidfd_open` has existed
    // since Linux 5.3 — the target here is 6.x — so ENOSYS cannot occur on any
    // supported kernel and no KAT exercises it. Rather than invent an untested,
    // reference-less signalfd/SIGCHLD watcher, ENOSYS (like any other error)
    // surfaces as `Err` from `init`; a real fallback is deferred if an old-kernel
    // target ever materializes.
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
    // fn-item/non-capturing-closure TYPE (Zig comptime `cb` ⇒ a Rust zero-sized
    // type). One untyped loop trampoline per call-site; `loop`/`c` stay RAW.
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

        // c.* = .{ .op = .{ .poll = { .fd, .events = EPOLL.IN } }, ... };
        unsafe {
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    events: libc::EPOLLIN as u32,
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
