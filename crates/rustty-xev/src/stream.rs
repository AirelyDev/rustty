//! stream.rs — faithful port of libxev `watcher/stream.zig` (D6, XEV_BLUEPRINT
//! §4/§7/§8, chunk C6).
//!
//! The stream layer is what termio's pty write path (`Exec`) rides:
//! `xev.Stream.initFd(pty_fds.write)` in `threadEnter`, then the ordered-write
//! engine `write_stream.queueWrite(...)`. C6 lands the whole macOS/kqueue subset:
//!   - `Shared` — `WriteQueue` (= `queue.Intrusive(WriteRequest)`), `WriteRequest`
//!     (`completion`/`userdata`/`full_write_buffer`/`next` + `from` container_of),
//!     `PollError`/`PollEvent`.
//!   - `GenericStream` — `initFd`/`deinit`/`read`/`write`/`writeInit`/`close`/
//!     `poll`/`queueWrite`, monomorphized for `options = { .close = true, .poll =
//!     true, .read = .read, .write = .write }` (the GenericStream config).
//!   - the `Writeable.queueWrite` ordered-write engine: chains completions through
//!     the `WriteQueue`, and its trampoline handles PARTIAL writes by re-`writeInit`-
//!     ing the remainder (byte-exact `writeBufferLength`/`writeBufferRemainder`/
//!     `@memcpy` of the array tail).
//!
//! Tests: the 4 reference `GenericStreamTests` (`Stream decls`, `pty: child to
//! parent`, `pty: parent to child`, `pty: queued writes`) + rustty KAT 4 (openpty
//! -> `Stream.initFd(write)` -> `queueWrite` a payload -> read it back on the other
//! end -> assert bytes + WriteQueue drains).
//
// PORT NOTE: `watcher/stream.zig` is a family of comptime-generic factories
// (`Stream(xev, T, options)` -> `Readable`/`Writeable`/`Closeable`/`Pollable`,
// `Shared(xev)`, `GenericStream(xev)`), each dispatching on `xev.backend` +
// `xev.dynamic`. On macOS the single backend candidate is kqueue and `xev.dynamic
// == false` (§0), so ONLY the non-dynamic kqueue instantiation compiles. This
// module ports exactly that monomorphization: the `GenericStream(kqueue)` `Stream`
// with `options.read == .read`, `options.write == .write`, `options.close`,
// `options.poll`, `options.threadpool == false`, `options.type == null`. The
// `xev.dynamic` `Union`/`superset`/`ensureTag` branches + the `.recv`/`.send`
// read/write methods are comptime-elided (deferred as whole backends, §1); their
// distinctive reference doc comments are preserved below (R3). Everything is
// `#[cfg(target_os = "macos")]` via the module gate in lib.rs.

// (module is `#[cfg(target_os = "macos")]`-gated in lib.rs)

use crate::common::userdata_value;
use crate::kqueue::{
    fd_t, CloseError, Completion, Loop, Operation, ReadBuffer, ReadError, Result, WriteBuffer,
    WriteError,
};
use crate::looppkg::{noop_callback, CallbackAction};
use crate::queue;
use core::ffi::c_void;
use core::mem::size_of;
use core::ptr::{self, NonNull};

// ---------------------------------------------------------------------------
// Options (stream.zig:7). Ported as a config record (dead_code): GenericStream
// fixes these constants, so the `Stream(...)`/`Readable`/`Writeable`/... comptime
// dispatch on them is monomorphized away — but the type + its docs are preserved
// for fidelity (R3).
// ---------------------------------------------------------------------------

/// Options for creating a stream type. Each of the options makes the
/// functionality available for the stream.
#[allow(dead_code)]
pub struct Options {
    pub read: ReadMethod,
    pub write: WriteMethod,
    pub close: bool,
    pub poll: bool,

    /// True to schedule the read/write on the threadpool.
    pub threadpool: bool,

    /// Set to non-null for dynamic APIs so we can know what our
    /// parent type name is.
    pub type_: Option<&'static str>,
}

/// PORT NOTE: `pub const ReadMethod = enum { none, read, recv };`.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReadMethod {
    None,
    Read,
    Recv,
}

/// PORT NOTE: `pub const WriteMethod = enum { none, write, send };`.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WriteMethod {
    None,
    Write,
    Send,
}

// ---------------------------------------------------------------------------
// Shared (stream.zig:28): `Shared(comptime xev: type)` — the shared decls for
// all streams (`WriteQueue`/`WriteRequest`/`PollError`/`PollEvent`). Returns the
// shared decls for all streams that should be set for the xev type.
// ---------------------------------------------------------------------------

/// WriteQueue is the queue of write requests for ordered writes.
/// This can be copied around.
//
// PORT NOTE: `pub const WriteQueue = queue.Intrusive(WriteRequest);` — the intrusive
// singly-linked FIFO of `WriteRequest` (C1 `queue::Intrusive`). [ADV-FIX F3]:
// termio default-constructs it (`write_queue: xev.WriteQueue = .{}`); the
// hand-written `Default` (head/tail = null) comes from `queue::Intrusive`.
pub type WriteQueue = queue::Intrusive<WriteRequest>;

/// WriteRequest is a single request for a write. It wraps a
/// completion so that it can be inserted into the WriteQueue.
//
// PORT NOTE: the intrusive `next` link + the raw-ptr `userdata` are reached/held
// as RAW pointers (never `&mut`/lifetimes): the loop holds the embedded
// `completion` into caller memory and re-enters it across ticks
// (`rust-mut-args-strongly-protected`).
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

// PORT NOTE [ADV-FIX F3]: hand-written `Default` (NOT `#[derive]`) — the Zig
// field defaults are `completion = .{}`, `userdata = null`, `next = null`, but
// `full_write_buffer: xev.WriteBuffer` has NO Zig default (it is `undefined` in
// the pre-allocated `SegmentedPool(WriteRequest)` element and is fully overwritten
// by `queueWrite`'s `req.* = .{ .full_write_buffer = buf }` before use). Rust has
// no `undefined`, so the placeholder is an empty in-line `Array` buffer — never
// observed, always overwritten. This is load-bearing: termio's `SegmentedPool<
// xev::WriteRequest>` requires `WriteRequest: Default`.
impl Default for WriteRequest {
    fn default() -> Self {
        WriteRequest {
            completion: Completion::default(),
            userdata: ptr::null_mut(),
            // Placeholder for the Zig `undefined` pool element (overwritten by
            // queueWrite before any read).
            full_write_buffer: WriteBuffer::Array {
                array: [0u8; 32],
                len: 0,
            },
            next: ptr::null_mut(),
        }
    }
}

// PORT NOTE: intrusive `next` link seam for `queue::Intrusive<WriteRequest>` (the
// `WriteQueue`). Mirrors the `queue::Node` impl for `Completion` (kqueue.rs).
impl queue::Node for WriteRequest {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

// PORT NOTE [ADV-FIX F2]: `WriteRequest` holds a raw-ptr `userdata`/`next` +
// embeds `Completion` (an fn-ptr) → auto-`!Send`; termio pools it in a
// `SegmentedPool<WriteRequest>` inside the `ThreadData` that is MOVED onto the
// termio thread. SAFETY / single-thread affinity: a request is touched by exactly
// the owning event-loop thread at a time after the move, so transferring ownership
// is sound.
unsafe impl Send for WriteRequest {}

impl WriteRequest {
    /// This can be used to convert a completion pointer back to
    /// a WriteRequest. This is only safe of course if the completion
    /// originally is from a write request. This is useful for getting
    /// the WriteRequest back in a callback from queuedWrite.
    //
    // PORT NOTE: `return @fieldParentPtr("completion", c);` — recovers the enclosing
    // `WriteRequest` from its embedded `completion` field via `container_of`
    // (offset-of), the `vtc-effects-trampoline-seam` pattern.
    pub fn from(c: *mut Completion) -> *mut WriteRequest {
        unsafe {
            (c as *mut u8).sub(core::mem::offset_of!(WriteRequest, completion)) as *mut WriteRequest
        }
    }
}

/// Errors that can be returned from polling.
//
// PORT NOTE: `PollError = switch (xev.backend) { .kqueue => xev.ReadError, ... }` —
// the kqueue arm. (io_uring/epoll = `xev.Sys.PollError`; iocp/wasi = `error{}` —
// comptime-elided.)
pub type PollError = ReadError;

/// Events that can be polled for using the high level streams.
//
// PORT NOTE: `PollEvent = enum(u32) { read = switch (xev.backend) { .kqueue => 0, ...
// } }` — the kqueue arm (`read = 0`, "doesn't matter").
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollEvent {
    Read = 0,
}

impl PollEvent {
    // PORT NOTE: `fn fromResult(c, result) PollError!PollEvent` — the kqueue arm:
    //   .kqueue => switch (c.op) { .read, .recv => .read, else => unreachable },
    // (io_uring/epoll consult `result.poll`; iocp/wasi `@compileError` — elided.)
    // On kqueue the result is never inspected: read/recv ops always map to `.read`.
    fn from_result(
        c: *const Completion,
        _result: Result,
    ) -> core::result::Result<PollEvent, PollError> {
        match unsafe { &(*c).op } {
            Operation::Read { .. } | Operation::Recv { .. } => Ok(PollEvent::Read),
            _ => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------------------
// GenericStream (stream.zig:1043): `GenericStream(comptime xev: type)`.
//
// Creates a generic stream type that supports read, write, close, poll. This
// can be used for any file descriptor that would exhibit normal blocking
// behavior on read/write. This should NOT be used for local files because
// local files have some special properties; you should use xev.File for that.
//
// PORT NOTE: the non-dynamic `GenericStream` instantiates `S = Stream(xev, Self,
// { .close = true, .poll = true, .read = .read, .write = .write })` and re-exports
// `close`/`poll`/`read`/`write`/`writeInit`/`queueWrite`. In Rust `Stream` IS that
// monomorphized instantiation directly (no separate `S` namespace). The `Stream`
// factory doc (stream.zig:107): "Creates a stream type that is meant to be embedded
// within other types. A stream is something that supports read, write, close, etc.
// The exact operations supported are defined by the \"options\" struct."
//
// T requirements:
//   - field named "fd" of type fd_t or socket_t
//   - decl named "initFd" to initialize a new T from a fd
//
// (satisfied below: the `fd` field + the `init_fd` decl.)
// ---------------------------------------------------------------------------

/// PORT NOTE: `GenericStream(kqueue)`. `#[derive(Clone, Copy)]`: the reference
/// `read`/`write`/`close`/`poll`/`queueWrite` take `self: Self` by value (a
/// trivially-copyable `fd` handle), and termio holds `xev::Stream` by value; the
/// `fd: fd_t` field makes it auto-`Send`.
#[derive(Clone, Copy)]
pub struct Stream {
    /// The underlying file
    fd: fd_t,
}

impl Stream {
    /// Initialize a generic stream from a file descriptor.
    pub fn init_fd(fd: fd_t) -> Stream {
        Stream { fd }
    }

    /// Clean up any watcher resources. This does NOT close the file.
    /// If you want to close the file you must call close or do so
    /// synchronously.
    //
    // PORT NOTE: the reference receiver is `self: *const Self`; ported as `&self`
    // (termio binds `self.write_stream` and calls `.deinit()`).
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
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata: type` + `userdata:
    // ?*Userdata` + `comptime cb: *const fn(...)` monomorphize to `read::<UD, CB>`
    // where `CB` is the callback's ZST fn-item/non-capturing-closure TYPE (Zig's
    // comptime-known `cb` ⇒ a Rust zero-sized type, NOT a runtime fn pointer).
    // Each call-site emits one untyped loop trampoline `read_trampoline::<UD, CB>`
    // with zero allocation and one indirection. `self` is by value (Copy);
    // `loop`/`c` stay RAW pointers.
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer that outlives the read,
    /// and `loop_` a valid `Loop` pointer; both are held raw by the loop. `buf`
    /// (if a slice) must outlive the read.
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

        // switch (buf) { inline .slice, .array => { c.* = .{ .op = .{ .read = ... },
        // .userdata, .callback }; ... loop.add(c); } }
        unsafe {
            *c = Completion::new_op(
                Operation::Read {
                    fd: self.fd,
                    buffer: buf,
                },
                userdata.cast::<c_void>(),
                Self::read_trampoline::<UD, CB>,
            );

            // If we're dup-ing, then we ask the backend to manage the fd. (kqueue:)
            // If we're not reading any actual data, we don't need a threadpool
            // since only read() is blocking.
            //   switch (buf) { .array => {}, .slice => |v| if (v.len == 0) break; }
            //   if (options.threadpool) c.flags.threadpool = true;
            // For GenericStream `options.threadpool == false`, so no threadpool bit
            // is set regardless of the buffer — the block is a no-op here.

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
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };

        // .read => cb(ud, l, c, T.initFd(c.op.read.fd), c.op.read.buffer,
        //             if (r.read) |v| v else |err| err)
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

        // PORT NOTE: the elided `xev.dynamic` `queueWrite` additionally does
        // `q.ensureTag(tag)` — "Ensure our WriteQueue has the correct tag, since it
        // is regularly zero-initialized and our zero-init picks an arbitrary
        // backend." — and, to "Initialize our request since it is usually
        // undefined", `req.* = @unionInit(WriteRequest, tag, undefined)`. On the
        // non-dynamic kqueue instantiation there is a single backend and no
        // `Union`/tag, so both are comptime-elided (§0); the `WriteQueue`/
        // `WriteRequest` are plain (untagged) here.
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
        //
        // We only peek the request here (not pop) because we may
        // need to rearm this write if the write was partial.
        // const req_inner: *xev.WriteRequest = q_inner.head.?;
        let req_inner: *mut WriteRequest = unsafe { (*q_inner).head };
        debug_assert!(!req_inner.is_null());

        // const cb_res = write_result(c_inner, r);
        let cb_res = Stream::write_result(c_inner, r);
        // var result: xev.WriteError!usize = cb_res.result;
        let mut result: core::result::Result<usize, WriteError> = cb_res.result;

        // Checks whether the entire buffer was written, this is
        // necessary to guarantee correct ordering of writes.
        // If the write was partial, it re-submits the remainder of
        // the buffer.
        // const queued_len = writeBufferLength(cb_res.buf);
        let queued_len = Stream::write_buffer_length(cb_res.buf);
        // if (cb_res.result) |written_len| {
        if let Ok(written_len) = cb_res.result {
            // if (written_len < queued_len) {
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
        // } else |_| {}

        // We can pop previously peeked request.
        // _ = q_inner.pop().?;
        let popped = unsafe { (*q_inner).pop() };
        debug_assert!(!popped.is_null());

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };

        // const action = @call(.always_inline, cb, .{ userdataValue(Userdata,
        //     req_inner.userdata), l_inner, c_inner, cb_res.writer,
        //     req_inner.full_write_buffer, result });
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
        // if (action == .rearm) q_inner.push(req_inner);
        if action == CallbackAction::Rearm {
            unsafe { (*q_inner).push(req_inner) };
        }

        // If we have another request, add that completion next.
        // if (q_inner.head) |req_next| l_inner.add(&req_next.completion);
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
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };

        // const cb_res = write_result(c_inner, r);
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
    //
    // PORT NOTE: the `inline fn write_result(c, r) struct { writer, buf, result }` —
    // the `.write` arm (`options.write == .write` for GenericStream; the `.send`
    // arm is comptime-elided).
    fn write_result(c: *mut Completion, r: Result) -> WriteResult {
        // .write => .{ .writer = T.initFd(c.op.write.fd), .buf = c.op.write.buffer,
        //             .result = if (r.write) |v| v else |err| err }
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
    // PORT NOTE: the `.write` arm (`options.write == .write`). `c.* = .{ .op = .{ .write
    // = { .fd, .buffer } } }` is a full reset (userdata = null, callback =
    // noopCallback, internals default) — `Completion::new_op` with the noop
    // callback + null userdata reproduces it. `flags` is module-private; the
    // kqueue arm only sets `c.flags.threadpool = true` when `options.threadpool`,
    // which is false for GenericStream, so no `set_threadpool` call is needed.
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn write_init(self, c: *mut Completion, buf: WriteBuffer) {
        // switch (buf) { inline .slice, .array => { c.* = .{ .op = ... }; ... } }
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
        // If we're dup-ing, then we ask the backend to manage the fd. (kqueue:)
        //   if (options.threadpool) c.flags.threadpool = true;
        // `options.threadpool == false` for GenericStream → no threadpool bit.
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
                // return .{ .slice = slice[offset..] };
                let new_ptr = unsafe { slice.cast::<u8>().add(offset) };
                WriteBuffer::Slice(ptr::slice_from_raw_parts(new_ptr, slice.len() - offset))
            }
            WriteBuffer::Array { array, len } => {
                assert!(offset <= len);
                let rem_len = len - offset;
                // var wb = .{ .array = .{ .array = undefined, .len = rem_len } };
                let mut wb_array = [0u8; 32];
                // @memcpy(wb.array.array[0..rem_len], array.array[offset..][0..rem_len]);
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
            // c.* = .{ .op = .{ .close = .{ .fd = self.fd } }, .userdata, .callback };
            *c = Completion::new_op(
                Operation::Close { fd: self.fd },
                userdata.cast::<c_void>(),
                Self::close_trampoline::<UD, CB>,
            );

            // If we're dup-ing, then we ask the backend to manage the fd. (kqueue:)
            //   c.flags.threadpool = true;
            Completion::set_threadpool(c, true);

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
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };

        // const fd = T.initFd(c_inner.op.close.fd);
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Close { fd } => *fd,
            _ => unreachable!(),
        };
        // if (r.close) |_| {} else |err| err
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
    // Pollable (stream.zig:139). `options.poll == true` for GenericStream.
    //
    // PORT NOTE: the elided `xev.dynamic` `Pollable` arm additionally comptime-
    // checks: "If all candidate backends do not support poll, our dynamic type
    // cannot support poll." (returns an empty struct if so). And the non-dynamic
    // `Pollable` has, before the `return struct{}` for iocp/wasi: "Do not add the
    // methods for poll if the backend doesn't support it." — on kqueue poll IS
    // supported (`options.poll == true`), so the method below is emitted.
    // -----------------------------------------------------------------------

    /// Poll the file descriptor for the given event. The high-level
    /// abstraction only allows a single event to be polled at a time.
    /// If you want to be more efficient for multiple events then you
    /// should use the lower level interfaces for the xev backend.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn poll<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        _event: PollEvent,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::poll cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // c.* = .{ .op = switch (options.read) { .read => .{ .read = { .fd,
            //     .buffer = .{ .slice = &.{} } } }, ... }, .userdata, .callback };
            // (`options.read == .read` for GenericStream; the `.recv` arm is
            // identical — an empty-slice read that notifies of read readiness.)
            let empty: *mut [u8] =
                ptr::slice_from_raw_parts_mut(NonNull::<u8>::dangling().as_ptr(), 0);
            *c = Completion::new_op(
                Operation::Read {
                    fd: self.fd,
                    buffer: ReadBuffer::Slice(empty),
                },
                userdata.cast::<c_void>(),
                Self::poll_trampoline::<UD, CB>,
            );

            (*loop_).add(c);
        }
    }

    /// The monomorphized untyped loop trampoline for a given (`UD`, `CB`).
    fn poll_trampoline<UD, CB>(
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
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };

        // const fd = T.initFd(c_inner.op.read.fd);  (kqueue arm)
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Read { fd, .. } => *fd,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            Stream::init_fd(fd),
            PollEvent::from_result(c_inner, r),
        )
    }
}

/// PORT NOTE: the anonymous return struct of `write_result` (`struct { writer: Self,
/// buf: WriteBuffer, result: WriteError!usize }`).
struct WriteResult {
    writer: Stream,
    buf: WriteBuffer,
    result: core::result::Result<usize, WriteError>,
}

// ---------------------------------------------------------------------------
// GenericStreamTests(xev, Impl) (stream.zig:1131). On macOS the single `Stream`
// instantiation is `GenericStream(kqueue)`, so `Impl == Stream`. The four
// reference `test{}` land as live Rust tests, plus rustty KAT 4 (§7) which proves
// the pty write round-trip on a real kqueue (Miri can't — real openpty/kevent/
// read/write syscalls). `#[cfg(target_os = "macos")]` via the module gate.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::kqueue::Loop;
    use crate::looppkg::RunMode;

    fn read_slice(buf: &mut [u8]) -> ReadBuffer {
        ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(buf.as_mut_ptr(), buf.len()))
    }
    fn write_slice(buf: &[u8]) -> WriteBuffer {
        WriteBuffer::Slice(ptr::slice_from_raw_parts(buf.as_ptr(), buf.len()))
    }

    // test "Stream decls"
    //
    // PORT NOTE: the reference is a compile-time reflection test — for every decl
    // in the `S = Stream(...)` namespace it asserts `@hasDecl(Impl, name)`. In the
    // monomorphized Rust port `Stream` IS that instantiation (no separate `S`
    // namespace to iterate), so the `@hasDecl` equivalent is: reference every
    // ported decl (`init_fd`/`deinit`/`read`/`write`/`write_init`/`close`/`poll`/
    // `queue_write`) as a monomorphized fn item — if any were missing this would
    // not compile. The `if false` guard means nothing actually runs; the const
    // ZST-`cb` assert of each generic method is satisfied because each `*_cb` is a
    // fn ITEM (zero-sized), never a fn pointer.
    #[test]
    fn stream_decls() {
        fn read_cb(
            _: *mut (),
            _: *mut Loop,
            _: *mut Completion,
            _: Stream,
            _: ReadBuffer,
            _: core::result::Result<usize, ReadError>,
        ) -> CallbackAction {
            CallbackAction::Disarm
        }
        fn write_cb(
            _: *mut (),
            _: *mut Loop,
            _: *mut Completion,
            _: Stream,
            _: WriteBuffer,
            _: core::result::Result<usize, WriteError>,
        ) -> CallbackAction {
            CallbackAction::Disarm
        }
        fn close_cb(
            _: *mut (),
            _: *mut Loop,
            _: *mut Completion,
            _: Stream,
            _: core::result::Result<(), CloseError>,
        ) -> CallbackAction {
            CallbackAction::Disarm
        }
        fn poll_cb(
            _: *mut (),
            _: *mut Loop,
            _: *mut Completion,
            _: Stream,
            _: core::result::Result<PollEvent, PollError>,
        ) -> CallbackAction {
            CallbackAction::Disarm
        }

        let s = Stream::init_fd(-1);
        s.deinit();

        if false {
            let l: *mut Loop = ptr::null_mut();
            let c: *mut Completion = ptr::null_mut();
            let q: *mut WriteQueue = ptr::null_mut();
            let req: *mut WriteRequest = ptr::null_mut();
            unsafe {
                s.read::<(), _>(l, c, ReadBuffer::Array([0; 32]), ptr::null_mut(), read_cb);
                s.write::<(), _>(
                    l,
                    c,
                    WriteBuffer::Array {
                        array: [0; 32],
                        len: 0,
                    },
                    ptr::null_mut(),
                    write_cb,
                );
                s.queue_write::<(), _>(
                    l,
                    q,
                    req,
                    WriteBuffer::Array {
                        array: [0; 32],
                        len: 0,
                    },
                    ptr::null_mut(),
                    write_cb,
                );
                s.close::<(), _>(l, c, ptr::null_mut(), close_cb);
                s.poll::<(), _>(l, c, PollEvent::Read, ptr::null_mut(), poll_cb);
            }
        }
    }

    // test "pty: child to parent"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C6, .port/phase-2/MIRI_EXCEPTIONS.md): real openpty + EVFILT_READ/WRITE via the live kqueue loop -- Miri cannot execute foreign functions; runs natively + ASan (sanitizers.sh)"
    )]
    fn pty_child_to_parent() {
        // switch (builtin.os.tag) { .linux, .macos => {}, else => return
        // error.SkipZigTest }  — this module is macOS-gated, so the arm holds.

        // Create the pty parent/child side.
        let mut pty = Pty::init().unwrap();
        // defer pty.deinit();

        let mut loop_ = Loop::init(crate::Options::default()).unwrap();
        // defer loop.deinit();

        let parent = Stream::init_fd(pty.parent);
        let child = Stream::init_fd(pty.child);

        // Read
        let mut read_buf = [0u8; 128];
        let mut read_len: Option<usize> = None;
        let mut c_read = Completion::default();
        let read_cb = |ud: *mut Option<usize>,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: Stream,
                       _b: ReadBuffer,
                       r: core::result::Result<usize, ReadError>| {
            unsafe { *ud = Some(r.unwrap()) }; // ud.?.* = r catch unreachable
            CallbackAction::Disarm
        };
        unsafe {
            parent.read(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_read),
                read_slice(&mut read_buf),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }

        // This should not block!
        loop_.run(RunMode::NoWait).unwrap();
        assert!(read_len.is_none());

        // Send
        let send_buf = b"hello, world!";
        let mut c_write = Completion::default();
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            r.unwrap(); // _ = r catch unreachable
            CallbackAction::Disarm
        };
        unsafe {
            child.write::<(), _>(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_write),
                write_slice(send_buf),
                ptr::null_mut(),
                write_cb,
            );
        }

        // The write and read should trigger
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(read_len.is_some());
        assert_eq!(send_buf, &read_buf[0..read_len.unwrap()]);

        pty.deinit();
        loop_.deinit();
    }

    // test "pty: parent to child"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C6, .port/phase-2/MIRI_EXCEPTIONS.md): real openpty + EVFILT_READ/WRITE via the live kqueue loop -- Miri cannot execute foreign functions; runs natively + ASan (sanitizers.sh)"
    )]
    fn pty_parent_to_child() {
        // Create the pty parent/child side.
        let mut pty = Pty::init().unwrap();

        let mut loop_ = Loop::init(crate::Options::default()).unwrap();

        let parent = Stream::init_fd(pty.parent);
        let child = Stream::init_fd(pty.child);

        // Read
        let mut read_buf = [0u8; 128];
        let mut read_len: Option<usize> = None;
        let mut c_read = Completion::default();
        let read_cb = |ud: *mut Option<usize>,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: Stream,
                       _b: ReadBuffer,
                       r: core::result::Result<usize, ReadError>| {
            unsafe { *ud = Some(r.unwrap()) };
            CallbackAction::Disarm
        };
        unsafe {
            child.read(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_read),
                read_slice(&mut read_buf),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }

        // This should not block!
        loop_.run(RunMode::NoWait).unwrap();
        assert!(read_len.is_none());

        // Send (note the newline at the end of the buf is important
        // since we're in cooked mode)
        let send_buf = b"hello, world!\n";
        let mut c_write = Completion::default();
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            r.unwrap();
            CallbackAction::Disarm
        };
        unsafe {
            parent.write::<(), _>(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_write),
                write_slice(send_buf),
                ptr::null_mut(),
                write_cb,
            );
        }

        // The write and read should trigger
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(read_len.is_some());
        assert_eq!(send_buf, &read_buf[0..read_len.unwrap()]);

        pty.deinit();
        loop_.deinit();
    }

    // test "pty: queued writes"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C6, .port/phase-2/MIRI_EXCEPTIONS.md): real openpty + EVFILT_READ/WRITE via the live kqueue loop -- Miri cannot execute foreign functions; runs natively + ASan (sanitizers.sh)"
    )]
    fn pty_queued_writes() {
        // this test fails on x86_64 with a strange error but passes
        // on aarch64. for now, just let it go until we investigate.
        // (Non-dynamic `xev.dynamic == false`, so the reference's
        // `xev.dynamic and .x86_64` skip does not apply here.)

        // Create the pty parent/child side.
        let mut pty = Pty::init().unwrap();

        let mut loop_ = Loop::init(crate::Options::default()).unwrap();

        let parent = Stream::init_fd(pty.parent);
        let child = Stream::init_fd(pty.child);

        // Read
        let mut read_buf = [0u8; 128];
        let mut read_len: Option<usize> = None;
        let mut c_read = Completion::default();
        let read_cb = |ud: *mut Option<usize>,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: Stream,
                       _b: ReadBuffer,
                       r: core::result::Result<usize, ReadError>| {
            unsafe { *ud = Some(r.unwrap()) };
            CallbackAction::Disarm
        };
        unsafe {
            child.read(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_read),
                read_slice(&mut read_buf),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }

        // This should not block!
        loop_.run(RunMode::NoWait).unwrap();
        assert!(read_len.is_none());

        let mut write_queue: WriteQueue = WriteQueue::default();
        let mut write_req: [WriteRequest; 2] = [WriteRequest::default(), WriteRequest::default()];

        // Send (note the newline at the end of the buf is important
        // since we're in cooked mode)
        let hello_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            r.unwrap();
            CallbackAction::Disarm
        };
        unsafe {
            parent.queue_write::<(), _>(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req[0]),
                write_slice(b"hello, "),
                ptr::null_mut(),
                hello_cb,
            );
        }

        let mut c_result: Option<*mut Completion> = None;
        let world_cb = |ud: *mut Option<*mut Completion>,
                        _l: *mut Loop,
                        c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            r.unwrap();
            unsafe { *ud = Some(c) }; // ud.?.* = c
            CallbackAction::Disarm
        };
        unsafe {
            parent.queue_write(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req[1]),
                write_slice(b"world!\n"),
                ptr::addr_of_mut!(c_result),
                world_cb,
            );
        }

        // The write and read should trigger
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(read_len.is_some());
        assert_eq!(b"hello, world!\n", &read_buf[0..read_len.unwrap()]);

        // Verify our completion is equal to our request
        // (`!xev.dynamic` holds here.)
        assert_eq!(
            WriteRequest::from(c_result.unwrap()),
            ptr::addr_of_mut!(write_req[1])
        );

        pty.deinit();
        loop_.deinit();
    }

    // KAT 4 (XEV_BLUEPRINT §7) — Pty write round-trip: openpty; Stream.init_fd
    // (write side = parent); queue_write a payload; read it back on the other end
    // (child); assert bytes + WriteQueue drains. Proves the ordered-write engine +
    // the loop run on a live kqueue (Miri can't — real openpty/kevent/read/write
    // syscalls).
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C6, .port/phase-2/MIRI_EXCEPTIONS.md): real openpty + EVFILT_READ/WRITE via the live kqueue loop -- Miri cannot execute foreign functions; runs natively + ASan (sanitizers.sh)"
    )]
    fn kat4_pty_write_roundtrip() {
        let mut pty = Pty::init().unwrap();
        let mut loop_ = Loop::init(crate::Options::default()).unwrap();

        let parent = Stream::init_fd(pty.parent); // write side
        let child = Stream::init_fd(pty.child); // read side

        // Read back on the child end.
        let mut read_buf = [0u8; 128];
        let mut read_len: Option<usize> = None;
        let mut c_read = Completion::default();
        let read_cb = |ud: *mut Option<usize>,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: Stream,
                       _b: ReadBuffer,
                       r: core::result::Result<usize, ReadError>| {
            unsafe { *ud = Some(r.unwrap()) };
            CallbackAction::Disarm
        };
        unsafe {
            child.read(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_read),
                read_slice(&mut read_buf),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }
        loop_.run(RunMode::NoWait).unwrap();
        assert!(read_len.is_none());

        // queue_write the payload on the parent (write) end.
        let mut write_queue: WriteQueue = WriteQueue::default();
        let mut write_req = WriteRequest::default();
        let payload = b"KAT4 payload\n";
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            // The whole buffer was written (queueWrite reports the full length).
            assert_eq!(r.unwrap(), b"KAT4 payload\n".len());
            CallbackAction::Disarm
        };
        unsafe {
            parent.queue_write::<(), _>(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req),
                write_slice(payload),
                ptr::null_mut(),
                write_cb,
            );
        }

        // The queue is non-empty until the write drains it.
        assert!(!write_queue.empty());

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(read_len.is_some());
        assert_eq!(payload, &read_buf[0..read_len.unwrap()]);

        // The WriteQueue drains (the single request was popped by the trampoline).
        assert!(write_queue.empty());

        pty.deinit();
        loop_.deinit();
    }

    // -----------------------------------------------------------------------
    /// Helper to open a pty. This isn't exposed as a public API this is only
    /// used for tests.
    // -----------------------------------------------------------------------
    struct Pty {
        /// The file descriptors for the parent/child side of the pty. This refers
        /// to the master/slave side respectively, and while that terminology is
        /// the officially used terminology of the syscall, I will use parent/child
        /// here.
        parent: fd_t,
        child: fd_t,
    }

    /// Redeclare this winsize struct so we can just use a Zig struct. This
    /// layout should be correct on all tested platforms.
    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }

    // libc pty.h
    extern "C" {
        fn openpty(
            parent: *mut fd_t,
            child: *mut fd_t,
            name: *mut u8,
            termios: *const c_void, // termios but we don't use it
            winsize: *const Winsize,
        ) -> libc::c_int;
    }

    impl Pty {
        fn init() -> core::result::Result<Pty, ()> {
            // Reasonable size
            let size = Winsize {
                ws_row: 80,
                ws_col: 80,
                ws_xpixel: 800,
                ws_ypixel: 600,
            };

            let mut parent_fd: fd_t = 0;
            let mut child_fd: fd_t = 0;
            if unsafe {
                openpty(
                    ptr::addr_of_mut!(parent_fd),
                    ptr::addr_of_mut!(child_fd),
                    ptr::null_mut(),
                    ptr::null(),
                    &size,
                )
            } < 0
            {
                return Err(()); // error.OpenptyFailed
            }
            // errdefer { close(parent_fd); close(child_fd); }  — no fallible step
            // follows the syscall, so the errdefer never fires.

            Ok(Pty {
                parent: parent_fd,
                child: child_fd,
            })
        }

        fn deinit(&mut self) {
            unsafe {
                libc::close(self.parent);
                libc::close(self.child);
            }
        }
    }
}
