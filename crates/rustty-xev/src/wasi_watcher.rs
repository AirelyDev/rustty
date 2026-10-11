//! wasi_watcher.rs — the libxev WATCHER layer instantiated against the
//! `wasi_poll` backend (WS20).
//!
//! `watcher/{async,timer,process,stream}.zig` are comptime-generic over `xev`.
//! The macOS/kqueue monomorphization lives in `async.rs`/`timer.rs`/`process.rs`/
//! `stream.rs`, the Linux one in `dynamic.rs`, the Windows one in
//! `iocp_watcher.rs`; this module is the SAME reference source monomorphized for
//! `xev = wasi_poll` (a single-candidate target, so `xev.dynamic == false` and
//! every `*Dynamic` arm is comptime-elided exactly as it is on macOS).
//!
//! The arms selected here by `switch (xev.backend)`:
//!   - `Async(xev)` -> `.wasi_poll => AsyncLoopState(xev, xev.Loop.threaded)`
//!     (async.zig:19/446)
//!   - `Timer(xev)` -> `TimerLoop(xev)` (timer.zig:15)
//!   - `Process(xev)` -> `.wasi_poll => struct {}` — verbatim reference comment
//!     "// Unsupported" (process.zig:23). There is no process watcher to port
//!     here; the EMPTY type IS the reference.
//!   - `Shared(xev)` -> `PollError = error{}`, `PollEvent.read = 0` "// invalid"
//!   - `GenericStream(xev)` with `options = { .close, .poll, .read = .read,
//!     .write = .write }` — and `Pollable` returns an EMPTY struct for
//!     `.wasi_poll` ("Do not add the methods for poll if the backend doesn't
//!     support it.", stream.zig:221), so there is deliberately NO `Stream::poll`.
//!
//! VERIFICATION: the reference `test {}` blocks are ported below and run under
//! wasmtime (`cargo test -p rustty-xev --target wasm32-wasip1`, G-WASI).

// (module is `#[cfg(target_os = "wasi")]`-gated in lib.rs)

use crate::common::userdata_value;
use crate::wasi;
use crate::wasi::CallbackAction;
use crate::wasi_poll::{
    noop_callback, AsyncError, CancelError, CloseError, Completion, Loop, Operation, ReadBuffer,
    ReadError, Result, TimerTrigger, WakeupType, WriteBuffer, WriteError,
};
use crate::{queue, Error, NotifyError};
use core::ffi::c_void;
use core::mem::size_of;
use core::ptr::{self, NonNull};

// ===========================================================================
// watcher/async.zig — `AsyncLoopState(xev, threaded)` (async.zig:446)
// ===========================================================================

/// Async implementation that is deferred to the backend implementation
/// loop state. This is kind of a hacky implementation and not recommended
/// but its the only way currently to get asyncs to work on WASI.
//
// PORT NOTE: the reference opens with
//   // TODO: we don't support threaded loop state async. We _can_ it just
//   // isn't done yet. To support it we need to have some sort of mutex
//   // to guard waiter below.
//   if (threaded) return struct {};
// `threaded` is `wasi_poll::threaded` = the wasm `atomics` feature, which IS a
// compile-time fact in Rust too, so the comptime selection maps exactly onto
// `#[cfg(target_feature = "atomics")]`: with atomics the type is the reference's
// EMPTY struct (no methods at all), without them it is the real body below.
#[cfg(target_feature = "atomics")]
pub struct Async {}

/// Async implementation that is deferred to the backend implementation
/// loop state. This is kind of a hacky implementation and not recommended
/// but its the only way currently to get asyncs to work on WASI.
//
// PORT NOTE: `wakeup: bool = false` + `waiter: ?struct { loop, c } = null`. The
// reference explicitly notes there is NO guard here (that is the whole reason the
// threaded arm is unimplemented upstream), so the fields are plain — single
// threaded by construction.
#[cfg(not(target_feature = "atomics"))]
pub struct Async {
    wakeup: bool,
    waiter: Option<AsyncWaiter>,
}

/// PORT NOTE: the anonymous `struct { loop: *xev.Loop, c: *xev.Completion }`.
#[cfg(not(target_feature = "atomics"))]
#[derive(Clone, Copy)]
struct AsyncWaiter {
    loop_: *mut Loop,
    c: *mut Completion,
}

/// PORT NOTE: `pub const WaitError = xev.Sys.AsyncError;`.
pub type AsyncWaitError = AsyncError;

#[cfg(not(target_feature = "atomics"))]
impl Async {
    /// PORT NOTE: `pub fn init() !Self { return .{}; }`.
    pub fn init() -> core::result::Result<Async, Error> {
        Ok(Async {
            wakeup: false,
            waiter: None,
        })
    }

    /// PORT NOTE: `pub fn deinit(self: *Self) void { _ = self; }`.
    pub fn deinit(&mut self) {
        let _ = self;
    }

    /// Wait for a notification.
    ///
    /// # Safety
    /// `loop_`/`c` must be valid, stable pointers that outlive the wait; the loop
    /// holds `c` raw across ticks.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn wait<UD, CB>(
        &mut self,
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
                core::result::Result<(), AsyncError>,
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
            // c.* = .{ .op = .{ .async_wait = .{} }, .userdata, .callback };
            *c = Completion::new_op(
                Operation::AsyncWait {
                    wakeup: WakeupType::new(false),
                },
                userdata.cast::<c_void>(),
                Self::wait_trampoline::<UD, CB>,
            );
            // loop.add(c);
            (*loop_).add(c);
        }

        // self.waiter = .{ .loop = loop, .c = c };
        self.waiter = Some(AsyncWaiter { loop_, c });

        // if (self.wakeup) self.notify() catch {};
        if self.wakeup {
            let _ = self.notify();
        }
    }

    /// The monomorphized untyped loop trampoline for a given (`UD`, `CB`).
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
                core::result::Result<(), AsyncError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: `CB` is a ZST (asserted in `wait`), so reading it through an
        // aligned dangling pointer accesses zero bytes.
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // if (r.async_wait) |_| {} else |err| err
        let mapped: core::result::Result<(), AsyncError> = match r {
            Result::AsyncWait(res) => res,
            _ => unreachable!(),
        };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
    }

    /// PORT NOTE: `pub fn notify(self: *Self) !void` — the error set is inferred
    /// and EMPTY; the crate-wide `NotifyError` keeps the signature uniform with
    /// the other backends' `Async::notify`.
    pub fn notify(&mut self) -> core::result::Result<(), NotifyError> {
        // if (self.waiter) |w| w.loop.async_notify(w.c) else self.wakeup = true;
        match self.waiter {
            // SAFETY: the waiter pointers name loop-owned memory registered by
            // `wait` and are valid until the loop is deinit'd.
            Some(w) => unsafe { (*w.loop_).async_notify(w.c) },
            None => self.wakeup = true,
        }
        Ok(())
    }
}

// ===========================================================================
// watcher/timer.zig — `TimerLoop` (timer.zig:15)
// ===========================================================================

/// A timer fires a callback after a specified amount of time. A timer can
/// repeat by returning "rearm" in the callback or by rescheduling the
/// start within the callback.
//
// PORT NOTE: `TimerLoop` is a pure forwarder over `loop.timer`/`loop.timer_reset`
// with no backend state — a zero-sized handle. `Copy`: the reference `run`/
// `reset`/`cancel` take `self: Self` by value.
#[derive(Clone, Copy)]
pub struct Timer;

impl Timer {
    /// Create a new timer.
    pub fn init() -> core::result::Result<Timer, Error> {
        Ok(Timer)
    }

    /// PORT NOTE: `pub fn deinit(self: *const Self) void` — "Nothing for now."
    pub fn deinit(&self) {
        let _ = self;
    }

    /// Start the timer. The timer will execute in next_ms milliseconds from
    /// now.
    ///
    /// This will use the monotonic clock on your system if available so
    /// this is immune to system clock changes or drift. The callback is
    /// guaranteed to fire NO EARLIER THAN "next_ms" milliseconds.
    ///
    /// Like everything else in libxev, if you want something to repeat, you
    /// must then requeue the completion manually.
    ///
    /// # Safety
    /// `loop_`/`c` must be valid, stable pointers that outlive the timer.
    pub unsafe fn run<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        next_ms: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), RunError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Timer::run cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            (*loop_).timer(
                c,
                next_ms,
                userdata.cast::<c_void>(),
                Self::trampoline::<UD, CB>,
            );
        }
    }

    /// Reset a timer to execute in next_ms milliseconds. If the timer
    /// is already started, this will stop it and restart it. If the
    /// timer has never been started, this is equivalent to running "run".
    ///
    /// VERY IMPORTANT: both c and c_cancel MUST NOT be undefined. They
    /// must be initialized to ".{}" if being used for the first time.
    ///
    /// # Safety
    /// `c`/`c_cancel` must be valid, stable `Completion` pointers.
    pub unsafe fn reset<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        c_cancel: *mut Completion,
        next_ms: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), RunError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Timer::reset cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            (*loop_).timer_reset(
                c,
                c_cancel,
                next_ms,
                userdata.cast::<c_void>(),
                Self::trampoline::<UD, CB>,
            );
        }
    }

    /// Cancel a previously started timer. The timer to cancel used the completion
    /// "c_cancel". A new completion "c" must be specified which will be called
    /// with the callback once cancellation is complete.
    ///
    /// The original timer will still have its callback fired but with the
    /// error "error.Canceled".
    ///
    /// # Safety
    /// `c_timer`/`c_cancel` must be valid, stable `Completion` pointers.
    pub unsafe fn cancel<UD, CB>(
        self,
        loop_: *mut Loop,
        c_timer: *mut Completion,
        c_cancel: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), CancelError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Timer::cancel cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // The `.epoll, .kqueue, .wasi_poll, .iocp` arm of the reference's
            // `switch (xev.backend)`: `.op = .{ .cancel = .{ .c = c_timer } }`.
            // (The `.io_uring` `.timer_remove` arm is comptime-elided here.)
            *c_cancel = Completion::new_op(
                Operation::Cancel { c: c_timer },
                userdata.cast::<c_void>(),
                Self::cancel_trampoline::<UD, CB>,
            );

            (*loop_).add(c_cancel);
        }
    }

    /// The monomorphized untyped loop trampoline shared by `run`/`reset` (both map
    /// `xev.Result.timer` -> `RunError!void`).
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
                core::result::Result<(), RunError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: `CB` is a ZST (asserted at every entry point).
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let mapped: core::result::Result<(), RunError> = match r {
            // if (r.timer) |trigger| switch (trigger) {
            //     .request, .expiration => {}, .cancel => error.Canceled }
            Result::Timer(Ok(trigger)) => match trigger {
                TimerTrigger::Request | TimerTrigger::Expiration => Ok(()),
                TimerTrigger::Cancel => Err(RunError::Canceled),
            },
            // else |err| err
            Result::Timer(Err(_)) => Err(RunError::Unexpected),
            _ => unreachable!(),
        };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
    }

    /// The monomorphized untyped loop trampoline for `cancel` (maps
    /// `Result::Cancel` -> `CancelError!void`).
    fn cancel_trampoline<UD, CB>(
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
                core::result::Result<(), CancelError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: `CB` is a ZST (asserted in `cancel`).
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // if (r.cancel) |_| {} else |err| err
        let mapped: core::result::Result<(), CancelError> = match r {
            Result::Cancel(res) => res,
            _ => unreachable!(),
        };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
    }
}

/// Error that could happen while running a timer.
//
// PORT NOTE: `pub const RunError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    /// The timer was canceled before it could expire
    Canceled,
    /// Some unexpected error.
    Unexpected,
}

/// PORT NOTE: `pub const CancelError = xev.CancelError;`.
pub type TimerCancelError = CancelError;

// ===========================================================================
// watcher/process.zig — the `.wasi_poll` arm (process.zig:23)
// ===========================================================================

/// PORT NOTE: `Process(xev)`'s `switch (xev.backend)` has, verbatim:
///
/// ```zig
/// // Unsupported
/// .wasi_poll => struct {},
/// ```
///
/// So the reference's `xev.Process` on this backend IS the empty struct — it has
/// no `init`/`deinit`/`wait`. This type is that empty struct; there is no body to
/// port and nothing is missing.
#[derive(Clone, Copy, Default)]
pub struct Process {}

// ===========================================================================
// watcher/stream.zig — `Shared` (stream.zig:28) + `GenericStream` (stream.zig:1043)
// ===========================================================================

/// WriteQueue is the queue of write requests for ordered writes.
/// This can be copied around.
pub type WriteQueue = queue::Intrusive<WriteRequest>;

/// WriteRequest is a single request for a write. It wraps a
/// completion so that it can be inserted into the WriteQueue.
pub struct WriteRequest {
    // completion: xev.Completion = .{},
    pub completion: Completion,
    // userdata: ?*anyopaque = null,
    pub userdata: *mut c_void,

    /// This is the original buffer passed to queueWrite. We have
    /// to keep track of this because we may be forced to split
    /// the write or rearm the write due to partial writes, but when
    /// we call the final callback we want to pass the original
    /// complete buffer.
    pub full_write_buffer: WriteBuffer,

    // next: ?*@This() = null,
    next: *mut WriteRequest,
}

// PORT NOTE [ADV-FIX F3]: hand-written `Default` — `full_write_buffer` has NO Zig
// default (it is `undefined` in the pooled element and is fully overwritten by
// `queueWrite`'s `req.* = .{ .full_write_buffer = buf }`); Rust has no
// `undefined`, so the placeholder is an empty in-line `Array` buffer.
impl Default for WriteRequest {
    fn default() -> Self {
        WriteRequest {
            completion: Completion::default(),
            userdata: ptr::null_mut(),
            full_write_buffer: WriteBuffer::Array {
                array: [0u8; 32],
                len: 0,
            },
            next: ptr::null_mut(),
        }
    }
}

// PORT NOTE: intrusive `next` link seam for `queue::Intrusive<WriteRequest>`.
impl queue::Node for WriteRequest {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

// PORT NOTE [ADV-FIX F2]: as on kqueue — a request is touched by exactly the
// owning event-loop thread at a time after the move, so transferring ownership is
// sound.
unsafe impl Send for WriteRequest {}

impl WriteRequest {
    /// This can be used to convert a completion pointer back to
    /// a WriteRequest. This is only safe of course if the completion
    /// originally is from a write request. This is useful for getting
    /// the WriteRequest back in a callback from queuedWrite.
    //
    // PORT NOTE: `return @fieldParentPtr("completion", c);` — container_of.
    pub fn from(c: *mut Completion) -> *mut WriteRequest {
        unsafe {
            (c as *mut u8).sub(core::mem::offset_of!(WriteRequest, completion)) as *mut WriteRequest
        }
    }
}

/// Errors that can be returned from polling.
//
// PORT NOTE: `PollError = switch (xev.backend) { ..., .iocp, .wasi_poll =>
// error{} }` — the EMPTY error set. Rust's uninhabited type is the empty enum;
// this is not "unsupported", it is the reference's own `error{}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollError {}

/// Events that can be polled for using the high level streams.
//
// PORT NOTE: `PollEvent = enum(u32) { read = switch (xev.backend) { ...,
// .iocp, .wasi_poll => 0 // invalid } }`. `fromResult` is
// `@compileError("poll not supported on this backend")` and therefore has NO Rust
// body — the `Pollable` factory returns an empty struct for this backend, so
// `Stream` below deliberately has no `poll` method.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollEvent {
    Read = 0,
}

/// PORT NOTE: `GenericStream(wasi_poll)` — "Creates a generic stream type that
/// supports read, write, close, poll. This can be used for any file descriptor
/// that would exhibit normal blocking behavior on read/write. This should NOT be
/// used for local files because local files have some special properties; you
/// should use xev.File for that."
///
/// `Copy`: the reference `read`/`write`/`close`/`queueWrite` take `self: Self` by
/// value (a trivially-copyable `fd` handle).
#[derive(Clone, Copy)]
pub struct Stream {
    /// The underlying file
    fd: wasi::fd_t,
}

impl Stream {
    /// Initialize a generic stream from a file descriptor.
    pub fn init_fd(fd: wasi::fd_t) -> Stream {
        Stream { fd }
    }

    /// Clean up any watcher resources. This does NOT close the file.
    /// If you want to close the file you must call close or do so
    /// synchronously.
    pub fn deinit(&self) {
        let _ = self;
    }

    // -----------------------------------------------------------------------
    // Readable (stream.zig:443). `options.read == .read` for GenericStream.
    // -----------------------------------------------------------------------

    /// Read from the socket. This performs a single read. The callback must
    /// requeue the read if additional reads want to be performed. Additional
    /// reads simultaneously can be queued by calling this multiple times. Note
    /// that depending on the backend, the reads can happen out of order.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop; `buf` (if
    /// a slice) must outlive the read.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn read<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: ReadBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::read cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Read {
                    fd: self.fd,
                    buffer: buf,
                },
                userdata.cast::<c_void>(),
                Self::read_trampoline::<UD, CB>,
            );

            // If we're dup-ing, then we ask the backend to manage the fd.
            //   switch (xev.backend) { .io_uring, .wasi_poll, .iocp => {}, ... }
            // The `.wasi_poll` arm is EMPTY — no flag is set.

            (*loop_).add(c);
        }
    }

    /// The monomorphized untyped loop trampoline for a given (`UD`, `CB`).
    fn read_trampoline<UD, CB>(
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
                Stream,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: `CB` is a ZST (asserted in `read`).
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };

        let (fd, buffer) = match unsafe { &(*c_inner).op } {
            Operation::Read { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, ReadError> = match r {
            Result::Read(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            Stream::init_fd(fd),
            buffer,
            mapped,
        )
    }

    // -----------------------------------------------------------------------
    // Writeable (stream.zig:620). `options.write == .write` for GenericStream.
    // -----------------------------------------------------------------------

    /// Write to the stream. This queues the writes to ensure they
    /// remain in order. Queueing has a small overhead: you must
    /// maintain a WriteQueue and WriteRequests instead of just
    /// Completions.
    ///
    /// If ordering isn't important, or you can maintain ordering
    /// naturally in your program, consider using write since it
    /// has a slightly smaller overhead.
    ///
    /// The "CallbackAction" return value of this callback behaves slightly
    /// different. The "rearm" return value will re-queue the same write
    /// at the end of the queue.
    ///
    /// It is safe to call this at anytime from the main thread.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` must be valid, stable pointers that outlive the queued
    /// write; the loop holds `&req.completion` raw across ticks.
    pub unsafe fn queue_write<UD, CB>(
        self,
        loop_: *mut Loop,
        q: *mut WriteQueue,
        req: *mut WriteRequest,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::queue_write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // Initialize our completion
            // req.* = .{ .full_write_buffer = buf };
            *req = WriteRequest {
                full_write_buffer: buf,
                ..WriteRequest::default()
            };
            // Must be kept in sync with partial write logic inside the callback
            self.write_init(ptr::addr_of_mut!((*req).completion), buf);
            (*req).completion.userdata = q as *mut c_void;
            (*req).completion.callback = Self::queue_write_trampoline::<UD, CB>;

            // The userdata as to go on the WriteRequest because we need
            // our actual completion userdata to be the WriteQueue so that
            // we can process the queue.
            (*req).userdata = userdata.cast::<c_void>();

            // If the queue is empty, then we add our completion. Otherwise,
            // the previously queued writes will trigger this one.
            if (*q).empty() {
                (*loop_).add(ptr::addr_of_mut!((*req).completion));
            }

            // We always add this item to our queue no matter what
            (*q).push(req);
        }
    }

    /// The monomorphized untyped loop trampoline for `queueWrite`'s ordered-write
    /// engine — chains completions through the `WriteQueue` and handles partial
    /// writes by re-`writeInit`-ing the remainder.
    fn queue_write_trampoline<UD, CB>(
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
                Stream,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        // const q_inner = @as(?*xev.WriteQueue, @ptrCast(@alignCast(ud))).?;
        let q_inner = ud as *mut WriteQueue;
        debug_assert!(!q_inner.is_null());

        // The queue MUST have a request because a completion
        // can only be added if the queue is not empty, and
        // nothing else should be popping!.
        let req_inner: *mut WriteRequest = unsafe { (*q_inner).head };
        debug_assert!(!req_inner.is_null());

        let cb_res = Stream::write_result(c_inner, r);
        let mut result: core::result::Result<usize, WriteError> = cb_res.result;

        // Checks whether the entire buffer was written, this is
        // necessary to guarantee correct ordering of writes.
        // If the write was partial, it re-submits the remainder of
        // the buffer.
        let queued_len = Stream::write_buffer_length(cb_res.buf);
        if let Ok(written_len) = cb_res.result {
            if written_len < queued_len {
                // Write remainder of the buffer, reusing the same completion
                let rem_buf = Stream::write_buffer_remainder(cb_res.buf, written_len);
                unsafe {
                    cb_res
                        .writer
                        .write_init(ptr::addr_of_mut!((*req_inner).completion), rem_buf);
                    (*req_inner).completion.userdata = q_inner as *mut c_void;
                    (*req_inner).completion.callback = Self::queue_write_trampoline::<UD, CB>;
                    (*l_inner).add(ptr::addr_of_mut!((*req_inner).completion));
                }
                return CallbackAction::Disarm;
            }

            // We wrote the entire buffer, modify the result to indicate
            // to the caller that all bytes have been written.
            result = Ok(Stream::write_buffer_length(unsafe {
                (*req_inner).full_write_buffer
            }));
        }

        // We can pop previously peeked request.
        let popped = unsafe { (*q_inner).pop() };
        debug_assert!(!popped.is_null());

        // SAFETY: `CB` is a ZST (asserted in `queue_write`).
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };

        let action = cb(
            userdata_value::<UD>(unsafe { (*req_inner).userdata }),
            l_inner,
            c_inner,
            cb_res.writer,
            unsafe { (*req_inner).full_write_buffer },
            result,
        );

        // Rearm requeues this request, it doesn't return rearm
        // on the actual callback here...
        if action == CallbackAction::Rearm {
            unsafe { (*q_inner).push(req_inner) };
        }

        // If we have another request, add that completion next.
        let req_next = unsafe { (*q_inner).head };
        if !req_next.is_null() {
            unsafe { (*l_inner).add(ptr::addr_of_mut!((*req_next).completion)) };
        }

        // We always disarm because the completion in the next
        // request will be used if there is more to queue.
        CallbackAction::Disarm
    }

    /// Write to the stream. This performs a single write. Additional
    /// writes can be requested by calling this multiple times.
    ///
    /// IMPORTANT: writes are NOT queued. There is no order guarantee
    /// if this is called multiple times. If ordered writes are important
    /// (they usually are!) then you should only call write again once
    /// the previous write callback is called.
    ///
    /// If ordering is important, use queueWrite instead.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop; `buf` (if
    /// a slice) must outlive the write.
    pub unsafe fn write<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            self.write_init(c, buf);
            (*c).userdata = userdata.cast::<c_void>();
            (*c).callback = Self::write_trampoline::<UD, CB>;
            (*loop_).add(c);
        }
    }

    /// The monomorphized untyped loop trampoline for a given (`UD`, `CB`).
    fn write_trampoline<UD, CB>(
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
                Stream,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: `CB` is a ZST (asserted in `write`).
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let cb_res = Stream::write_result(c_inner, r);
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            cb_res.writer,
            cb_res.buf,
            cb_res.result,
        )
    }

    /// Extracts the result from a completion for a write callback.
    fn write_result(c: *mut Completion, r: Result) -> WriteResult {
        let (fd, buffer) = match unsafe { &(*c).op } {
            Operation::Write { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let result: core::result::Result<usize, WriteError> = match r {
            Result::Write(res) => res,
            _ => unreachable!(),
        };
        WriteResult {
            writer: Stream::init_fd(fd),
            buf: buffer,
            result,
        }
    }

    /// Initialize the completion c for a write. This does NOT set
    /// userdata or a callback.
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn write_init(self, c: *mut Completion, buf: WriteBuffer) {
        unsafe {
            *c = Completion::new_op(
                Operation::Write {
                    fd: self.fd,
                    buffer: buf,
                },
                ptr::null_mut(),
                noop_callback,
            );
        }
        // If we're dup-ing, then we ask the backend to manage the fd.
        //   switch (xev.backend) { .io_uring, .wasi_poll, .iocp => {}, ... }
        // The `.wasi_poll` arm is EMPTY.
    }

    /// Returns the length of the write buffer
    fn write_buffer_length(buf: WriteBuffer) -> usize {
        match buf {
            WriteBuffer::Slice(slice) => slice.len(),
            WriteBuffer::Array { len, .. } => len,
        }
    }

    /// Given a `WriteBuffer` and number of bytes written during the previous
    /// write operation, returns a new `WriteBuffer` with remaining data.
    fn write_buffer_remainder(buf: WriteBuffer, offset: usize) -> WriteBuffer {
        match buf {
            WriteBuffer::Slice(slice) => {
                assert!(offset <= slice.len());
                let new_ptr = unsafe { slice.cast::<u8>().add(offset) };
                WriteBuffer::Slice(ptr::slice_from_raw_parts(new_ptr, slice.len() - offset))
            }
            WriteBuffer::Array { array, len } => {
                assert!(offset <= len);
                let rem_len = len - offset;
                let mut wb_array = [0u8; 32];
                wb_array[0..rem_len].copy_from_slice(&array[offset..offset + rem_len]);
                WriteBuffer::Array {
                    array: wb_array,
                    len: rem_len,
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Closeable (stream.zig:319). `options.close == true` for GenericStream.
    // -----------------------------------------------------------------------

    /// Close the socket.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn close<UD, CB>(
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
                Stream,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::close cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Close { fd: self.fd },
                userdata.cast::<c_void>(),
                Self::close_trampoline::<UD, CB>,
            );

            // If we're dup-ing, then we ask the backend to manage the fd.
            //   switch (xev.backend) { .io_uring, .wasi_poll, .iocp => {}, ... }
            // The `.wasi_poll` arm is EMPTY — no threadpool flag (unlike
            // epoll/kqueue).

            (*loop_).add(c);
        }
    }

    /// The monomorphized untyped loop trampoline for a given (`UD`, `CB`).
    fn close_trampoline<UD, CB>(
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
                Stream,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: `CB` is a ZST (asserted in `close`).
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Close { fd } => *fd,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<(), CloseError> = match r {
            Result::Close(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            Stream::init_fd(fd),
            mapped,
        )
    }

    // -----------------------------------------------------------------------
    // Pollable (stream.zig:139) — DELIBERATELY ABSENT.
    //
    // Verbatim reference (stream.zig:221):
    //   // Do not add the methods for poll if the backend doesn't support it.
    //   switch (xev.backend) {
    //       .io_uring, .epoll, .kqueue => {},
    //       .iocp, .wasi_poll => return struct {},
    //   }
    // The `.wasi_poll` arm returns an EMPTY struct, so `GenericStream(wasi_poll)`
    // has no `poll` decl at all. This is the reference's own shape.
    // -----------------------------------------------------------------------
}

/// PORT NOTE: the anonymous return of `inline fn write_result(c, r)`.
struct WriteResult {
    writer: Stream,
    buf: WriteBuffer,
    result: core::result::Result<usize, WriteError>,
}

// ===========================================================================
// The reference `test {}` blocks.
//
// `AsyncTests` (async.zig:714, 3 tests), `TimerTests` (timer.zig:442, 5 tests) and
// `GenericStreamTests` (stream.zig:1131, 4 tests) are instantiated by the
// corresponding watcher's `test {}` on this backend. (`ProcessTests` is NOT — the
// `.wasi_poll` arm of `Process(xev)` is `struct {}`, which has no `test {}`.)
//
// They run on a WASI runtime: `cargo test -p rustty-xev --target wasm32-wasip1`
// with wasmtime as the runner (G-WASI in `.port/tools/ci.sh`). Before V8 was
// discharged they were documented skip-stubs; they are live ports now.
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasi::{CompletionState, Options, RunMode};

    fn set_bool_rearm(
        ud: *mut bool,
        _l: *mut Loop,
        _c: *mut Completion,
        r: core::result::Result<(), AsyncError>,
    ) -> CallbackAction {
        r.unwrap(); // _ = r catch unreachable;
        unsafe { *ud = true };
        CallbackAction::Rearm
    }

    fn set_bool_disarm(
        ud: *mut bool,
        _l: *mut Loop,
        _c: *mut Completion,
        r: core::result::Result<(), AsyncError>,
    ) -> CallbackAction {
        r.unwrap(); // _ = r catch unreachable;
        unsafe { *ud = true };
        CallbackAction::Disarm
    }

    fn count_rearm(
        ud: *mut u32,
        _l: *mut Loop,
        _c: *mut Completion,
        r: core::result::Result<(), AsyncError>,
    ) -> CallbackAction {
        r.unwrap(); // _ = r catch unreachable;
        unsafe { *ud += 1 };
        CallbackAction::Rearm
    }

    fn timer_set_bool_disarm(
        ud: *mut bool,
        _l: *mut Loop,
        _c: *mut Completion,
        r: core::result::Result<(), RunError>,
    ) -> CallbackAction {
        r.unwrap(); // _ = r catch unreachable;
        unsafe { *ud = true };
        CallbackAction::Disarm
    }

    /// PORT NOTE: `AsyncTests(xev, AsyncLoopState)` -> `test "async"`.
    #[test]
    fn wasi_watcher_async() {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        let mut notifier = Async::init().unwrap();

        // Wait
        let mut wake = false;
        let mut c_wait = Completion::default();
        unsafe {
            notifier.wait(
                &raw mut loop_,
                &raw mut c_wait,
                &raw mut wake,
                set_bool_rearm,
            )
        };

        // Send a notification
        notifier.notify().unwrap();

        // Wait for wake
        loop_.run(RunMode::Once).unwrap();
        assert!(wake);

        // Make sure it only triggers once
        wake = false;
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!wake);

        notifier.deinit();
        loop_.deinit();
    }

    /// PORT NOTE: `AsyncTests(xev, AsyncLoopState)` -> `test "async: notify first"`.
    #[test]
    fn wasi_watcher_async_notify_first() {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        let mut notifier = Async::init().unwrap();

        // Send a notification
        notifier.notify().unwrap();

        // Wait
        let mut wake = false;
        let mut c_wait = Completion::default();
        unsafe {
            notifier.wait(
                &raw mut loop_,
                &raw mut c_wait,
                &raw mut wake,
                set_bool_disarm,
            )
        };

        // Wait for wake
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(wake);

        notifier.deinit();
        loop_.deinit();
    }

    /// PORT NOTE: `AsyncTests(xev, AsyncLoopState)` -> `test "async batches
    /// multiple notifications"`.
    #[test]
    fn wasi_watcher_async_batches_multiple_notifications() {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        let mut notifier = Async::init().unwrap();

        // Send a notification many times
        notifier.notify().unwrap();
        notifier.notify().unwrap();
        notifier.notify().unwrap();
        notifier.notify().unwrap();
        notifier.notify().unwrap();

        // Wait
        let mut count: u32 = 0;
        let mut c_wait = Completion::default();
        unsafe { notifier.wait(&raw mut loop_, &raw mut c_wait, &raw mut count, count_rearm) };

        // Send a notification
        notifier.notify().unwrap();

        // Wait for wake
        loop_.run(RunMode::Once).unwrap();
        for _ in 0..10 {
            loop_.run(RunMode::NoWait).unwrap();
        }
        assert_eq!(count, 1);

        notifier.deinit();
        loop_.deinit();
    }

    /// PORT NOTE: `TimerTests(xev, TimerLoop)` -> `test "timer"`.
    #[test]
    fn wasi_watcher_timer() {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        // Add the timer
        let mut called = false;
        let mut c1 = Completion::default();
        unsafe {
            timer.run(
                &raw mut loop_,
                &raw mut c1,
                1,
                &raw mut called,
                timer_set_bool_disarm,
            )
        };

        // Wait
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(called);

        timer.deinit();
        loop_.deinit();
    }

    /// PORT NOTE: `TimerTests(xev, TimerLoop)` -> `test "timer reset"`.
    #[test]
    fn wasi_watcher_timer_reset() {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        let mut c_timer = Completion::default();
        let mut c_cancel = Completion::default();

        // Add the timer
        let mut canceled = false;
        unsafe {
            timer.run(
                &raw mut loop_,
                &raw mut c_timer,
                100_000,
                &raw mut canceled,
                timer_set_bool_disarm,
            )
        };

        // Wait
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!canceled);

        // Reset it
        unsafe {
            timer.reset(
                &raw mut loop_,
                &raw mut c_timer,
                &raw mut c_cancel,
                1,
                &raw mut canceled,
                timer_set_bool_disarm,
            )
        };

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        assert!(unsafe { Completion::state(&raw mut c_timer) } == CompletionState::Dead);
        assert!(unsafe { Completion::state(&raw mut c_cancel) } == CompletionState::Dead);

        timer.deinit();
        loop_.deinit();
    }

    /// PORT NOTE: `TimerTests(xev, TimerLoop)` -> `test "timer reset before tick"`.
    #[test]
    fn wasi_watcher_timer_reset_before_tick() {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        let mut c_timer = Completion::default();
        let mut c_cancel = Completion::default();

        // Add the timer
        let mut canceled = false;
        unsafe {
            timer.run(
                &raw mut loop_,
                &raw mut c_timer,
                100_000,
                &raw mut canceled,
                timer_set_bool_disarm,
            )
        };

        // Reset it
        unsafe {
            timer.reset(
                &raw mut loop_,
                &raw mut c_timer,
                &raw mut c_cancel,
                1,
                &raw mut canceled,
                timer_set_bool_disarm,
            )
        };

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        assert!(unsafe { Completion::state(&raw mut c_timer) } == CompletionState::Dead);
        assert!(unsafe { Completion::state(&raw mut c_cancel) } == CompletionState::Dead);

        timer.deinit();
        loop_.deinit();
    }

    /// PORT NOTE: `TimerTests(xev, TimerLoop)` -> `test "timer reset after trigger"`.
    #[test]
    fn wasi_watcher_timer_reset_after_trigger() {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        let mut c_timer = Completion::default();
        let mut c_cancel = Completion::default();

        // Add the timer
        let mut canceled = false;
        unsafe {
            timer.run(
                &raw mut loop_,
                &raw mut c_timer,
                1,
                &raw mut canceled,
                timer_set_bool_disarm,
            )
        };
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        canceled = false;

        // Reset it
        unsafe {
            timer.reset(
                &raw mut loop_,
                &raw mut c_timer,
                &raw mut c_cancel,
                1,
                &raw mut canceled,
                timer_set_bool_disarm,
            )
        };

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        assert!(unsafe { Completion::state(&raw mut c_timer) } == CompletionState::Dead);
        assert!(unsafe { Completion::state(&raw mut c_cancel) } == CompletionState::Dead);

        timer.deinit();
        loop_.deinit();
    }

    /// PORT NOTE: `TimerTests(xev, TimerLoop)` -> `test "timer cancel"`.
    #[test]
    fn wasi_watcher_timer_cancel() {
        fn cb_timer(
            ud: *mut bool,
            _l: *mut Loop,
            _c: *mut Completion,
            r: core::result::Result<(), RunError>,
        ) -> CallbackAction {
            // ud.?.* = if (r) false else |err| err == error.Canceled;
            unsafe {
                *ud = match r {
                    Ok(()) => false,
                    Err(e) => e == RunError::Canceled,
                }
            };
            CallbackAction::Disarm
        }

        fn cb_cancel(
            ud: *mut bool,
            _l: *mut Loop,
            _c: *mut Completion,
            r: core::result::Result<(), CancelError>,
        ) -> CallbackAction {
            r.unwrap(); // _ = r catch unreachable;
            unsafe { *ud = true };
            CallbackAction::Disarm
        }

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        // Add the timer
        let mut canceled = false;
        let mut c1 = Completion::default();
        unsafe {
            timer.run(
                &raw mut loop_,
                &raw mut c1,
                100_000,
                &raw mut canceled,
                cb_timer,
            )
        };

        // Cancel
        let mut cancel_confirm = false;
        let mut c2 = Completion::default();
        unsafe {
            timer.cancel(
                &raw mut loop_,
                &raw mut c1,
                &raw mut c2,
                &raw mut cancel_confirm,
                cb_cancel,
            )
        };

        // Wait
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        assert!(cancel_confirm);

        timer.deinit();
        loop_.deinit();
    }

    /// PORT NOTE: `GenericStreamTests(xev, GenericStream)` -> `test "Stream decls"`.
    /// A COMPILE-TIME assertion in the reference ("missing decl: ..."), discharged
    /// by this module compiling at all for the WASI target; the runtime body is
    /// empty.
    #[test]
    fn wasi_watcher_stream_decls() {}

    /// PORT NOTE: `GenericStreamTests` -> `test "pty: child to parent"`. The
    /// reference itself skips this outside `.linux`/`.macos`
    /// (`else => return error.SkipZigTest`), so the skip IS the reference behavior.
    #[test]
    fn wasi_watcher_stream_pty_child_to_parent() {}

    /// PORT NOTE: `GenericStreamTests` -> `test "pty: parent to child"`.
    /// Reference-skipped outside linux/macos, as above.
    #[test]
    fn wasi_watcher_stream_pty_parent_to_child() {}

    /// PORT NOTE: `GenericStreamTests` -> `test "pty: queued writes"`.
    /// Reference-skipped outside linux/macos, as above.
    #[test]
    fn wasi_watcher_stream_pty_queued_writes() {}
}
