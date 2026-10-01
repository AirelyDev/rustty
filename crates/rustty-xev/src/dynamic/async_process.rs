//! The Dynamic `Async` and `Process` watchers.
//!
//! PORTS: libxev src/watcher/async.zig `AsyncDynamic` and src/watcher/process.zig
//! `ProcessDynamic` — the dynamic-dispatch arms over the io_uring and epoll
//! backends.

use super::*;

// ---------------------------------------------------------------------------
// Async (watcher/async.zig `AsyncDynamic`, async.zig:614-712).
// ---------------------------------------------------------------------------

/// PORT NOTE: `pub const WaitError = xev.ErrorSet(&.{"Async", "WaitError"})`
/// (async.zig:621). Each backend's `Async.WaitError` IS its `ReadError`
/// (async.zig:35), so the merge is exactly this module's `ReadError`.
pub type WaitError = ReadError;

/// "Wake up" an event loop from any thread using an async completion.
//
// PORT NOTE: `AsyncDynamic(xev)` (async.zig:614) is `struct { backend: Union }`
// over `xev.Union(&.{"Async"})`. Spelled as a Rust `enum` rather than a
// `union`+global because both arms are `Copy` fd handles that are passed BY VALUE
// (`wait(self: Self)`, `notify(self: Self)`) and never pointer-projected — so
// nothing here needs the `@fieldParentPtr` layout guarantee, and a Rust enum is
// both safe and self-describing. `Copy` is REQUIRED by termio/core (`xev::Async`
// is held and copied by value).
#[derive(Clone, Copy)]
pub enum Async {
    IoUring(io_uring::Async),
    Epoll(epoll::Async),
}

impl Async {
    /// PORT NOTE: `pub fn init() !Self` (async.zig:623) — the `switch
    /// (xev.backend)` constructor.
    pub fn init() -> core::result::Result<Async, Error> {
        Ok(match backend() {
            Backend::IoUring => Async::IoUring(io_uring::Async::init()?),
            Backend::Epoll => Async::Epoll(epoll::Async::init()?),
            be => unreachable!("non-candidate backend {be:?}"),
        })
    }

    /// PORT NOTE: `pub fn deinit(self: *Self) void` (async.zig:636).
    //
    // PORT NOTE: switches on the SELF tag, not the global. The reference switches
    // on `xev.backend` (the global) because its union is untagged; a Rust enum
    // carries its own tag, which is strictly safer and observationally identical
    // whenever the global has not been changed under a live handle (which the
    // reference forbids anyway, dynamic.zig:32-33).
    pub fn deinit(&mut self) {
        match self {
            Async::IoUring(a) => a.deinit(),
            Async::Epoll(a) => a.deinit(),
        }
    }

    /// Notify a loop to wake up synchronously. This should never block forever
    /// (it will always EVENTUALLY succeed regardless of if the loop is currently
    /// ticking or not).
    //
    // PORT NOTE: `pub fn notify(self: *Self) !void` (async.zig:645). The
    // backends' `notify` returns the unified seam `Error`; the crate's public
    // spelling for this call's error is `NotifyError` (lib.rs), so the total
    // `Error → NotifyError` mapping is spelled out.
    pub fn notify(&self) -> core::result::Result<(), NotifyError> {
        match self {
            Async::IoUring(a) => a.notify().map_err(|_| crate::NotifyError),
            Async::Epoll(a) => a.notify().map_err(|_| crate::NotifyError),
        }
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
    // PORT NOTE: `pub fn wait(...)` (async.zig:654) — `c.ensureTag(tag)`, then
    // the backend's `wait` on `&@field(loop.backend, tag)` /
    // `&@field(c.value, tag)` with an `api_cb` that `@fieldParentPtr`s the inner
    // `*api.Loop`/`*api.Completion` back to the OUTER `*xev.Loop`/`*xev.Completion`.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `wait::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE. The per-arm `api_cb` is a non-capturing
    // closure that captures only the ZST `cb`, so it is itself a ZST and
    // satisfies the backend's own comptime-cb contract.
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
                core::result::Result<(), WaitError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Async::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (Async::IoUring(a), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                a.wait(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            r.map_err(ReadError::from_io_uring),
                        )
                    },
                );
            },
            (Async::Epoll(a), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                a.wait(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            r.map_err(ReadError::from_epoll),
                        )
                    },
                );
            },
            // PORT NOTE: unreachable in the reference — Zig's `switch
            // (xev.backend)` on an untagged union simply reads the arm the global
            // names. Here the `Async` handle carries its own tag, so a mismatch
            // means the caller changed the global under a live handle, which
            // dynamic.zig:32-33 forbids. Panic rather than silently misread.
            (a, be) => panic!(
                "Async handle backend {:?} does not match the global backend {be:?} — \
                 the backend must be set before initializing any other types",
                match a {
                    Async::IoUring(_) => Backend::IoUring,
                    Async::Epoll(_) => Backend::Epoll,
                }
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Process (watcher/process.zig `ProcessDynamic`, process.zig:361-430).
// ---------------------------------------------------------------------------

/// The error that can come in the wait callback.
//
// PORT NOTE: `pub const WaitError = dynamic.ErrorSet(&.{"Process", "WaitError"})`
// (process.zig:368) = io_uring `{InvalidChild, Unexpected}` || epoll
// `{InvalidChild, Unexpected}`. Named `ProcessWaitError` here (one module),
// matching epoll.rs/io_uring.rs's spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessWaitError {
    InvalidChild,
    Unexpected,
}

impl ProcessWaitError {
    fn from_io_uring(e: io_uring::ProcessWaitError) -> ProcessWaitError {
        match e {
            io_uring::ProcessWaitError::InvalidChild => ProcessWaitError::InvalidChild,
            io_uring::ProcessWaitError::Unexpected => ProcessWaitError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::ProcessWaitError) -> ProcessWaitError {
        match e {
            epoll::ProcessWaitError::InvalidChild => ProcessWaitError::InvalidChild,
            epoll::ProcessWaitError::Unexpected => ProcessWaitError::Unexpected,
        }
    }
}

/// Process watcher — waits for a subprocess to exit.
//
// PORT NOTE: `ProcessDynamic(dynamic)` (process.zig:361) is `struct { backend:
// Union }`. Spelled as a Rust `enum` for the same reason as `Async`: both arms
// are `Copy` fd handles passed BY VALUE and never pointer-projected.
#[derive(Clone, Copy)]
pub enum Process {
    IoUring(io_uring::Process),
    Epoll(epoll::Process),
}

impl Process {
    /// Create a new process watcher for the given pid.
    //
    // PORT NOTE: `pub fn init(pid: posix.pid_t) !Self` (process.zig:370).
    pub fn init(pid: libc::pid_t) -> core::result::Result<Process, Error> {
        Ok(match backend() {
            Backend::IoUring => Process::IoUring(io_uring::Process::init(pid)?),
            Backend::Epoll => Process::Epoll(epoll::Process::init(pid)?),
            be => unreachable!("non-candidate backend {be:?}"),
        })
    }

    /// Clean up the process watcher.
    //
    // PORT NOTE: `pub fn deinit(self: *Self) void` (process.zig:383). Switches on
    // the SELF tag — see the note on `Async::deinit`.
    pub fn deinit(&mut self) {
        match self {
            Process::IoUring(p) => p.deinit(),
            Process::Epoll(p) => p.deinit(),
        }
    }

    /// Wait for the process to exit. This will automatically call
    /// `waitpid` or equivalent and report the exit status.
    //
    // PORT NOTE: `pub fn wait(...)` (process.zig:392) — the same
    // `ensureTag` + down-cast + `@fieldParentPtr`-in-the-callback shape as
    // `Async::wait`; see the notes there.
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

        match (self, backend()) {
            (Process::IoUring(p), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                p.wait(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            r.map_err(ProcessWaitError::from_io_uring),
                        )
                    },
                );
            },
            (Process::Epoll(p), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                p.wait(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            r.map_err(ProcessWaitError::from_epoll),
                        )
                    },
                );
            },
            // See the same arm on `Async::wait`.
            (p, be) => panic!(
                "Process handle backend {:?} does not match the global backend {be:?} — \
                 the backend must be set before initializing any other types",
                match p {
                    Process::IoUring(_) => Backend::IoUring,
                    Process::Epoll(_) => Backend::Epoll,
                }
            ),
        }
    }
}
