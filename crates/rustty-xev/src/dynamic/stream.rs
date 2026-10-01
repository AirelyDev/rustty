//! The Dynamic `WriteQueue`/`WriteRequest` and `Stream` watcher.
//!
//! PORTS: libxev src/dynamic.zig `DynamicWriteQueue` and src/watcher/stream.zig
//! `GenericStream` (dynamic arm) — the dynamic-dispatch arms over the io_uring
//! and epoll backends.

use super::*;

// ---------------------------------------------------------------------------
// WriteQueue / WriteRequest (dynamic.zig:68-69 + `DynamicWriteQueue`,
// dynamic.zig:372-392).
// ---------------------------------------------------------------------------

/// WriteQueue is the queue of write requests for ordered writes.
//
// PORT NOTE: `DynamicWriteQueue` (dynamic.zig:372) — `struct { value:
// TaggedUnion(&.{"WriteQueue"}) }` with an `ensureTag`. TAGGED (like
// `Completion`, unlike `Loop`), and its zero value is the LAST candidate's
// variant (dynamic.zig:377-381). `Default` is REQUIRED (termio's Exec
// default-constructs its `write_queue` field) and hand-written, never derived.
#[repr(C)]
pub struct WriteQueue {
    value: WriteQueueUnion,
    tag: Backend,
}

#[repr(C)]
union WriteQueueUnion {
    io_uring: ManuallyDrop<io_uring::WriteQueue>,
    epoll: ManuallyDrop<epoll::WriteQueue>,
}

const _: () = assert!(offset_of!(WriteQueue, value) == 0);

impl Default for WriteQueue {
    /// PORT NOTE: dynamic.zig:377-381 — the LAST candidate's zero variant.
    fn default() -> Self {
        WriteQueue::zeroed_for(BACKEND_DEFAULT)
    }
}

impl WriteQueue {
    fn zeroed_for(tag: Backend) -> WriteQueue {
        match tag {
            Backend::IoUring => WriteQueue {
                value: WriteQueueUnion {
                    io_uring: ManuallyDrop::new(io_uring::WriteQueue::default()),
                },
                tag,
            },
            Backend::Epoll => WriteQueue {
                value: WriteQueueUnion {
                    epoll: ManuallyDrop::new(epoll::WriteQueue::default()),
                },
                tag,
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// The active backend tag.
    pub fn tag(&self) -> Backend {
        self.tag
    }

    /// Returns true if the queue has no pending write requests.
    //
    // PORT NOTE (ADDITION): `queue.Intrusive.empty()` is reachable on the static
    // API as `write_queue.empty()`; the reference's `DynamicWriteQueue` forwards
    // no decls at all (it is only ever handed to `queueWrite`). Forwarded here
    // because it is the only way a consumer (or a test) can observe the ordered-
    // write engine draining, and it is a pure read of the live arm.
    pub fn empty(&self) -> bool {
        match self.tag {
            // SAFETY: `tag` names the live arm (`zeroed_for`/`ensure_tag` are its
            // only writers and always set both together).
            Backend::IoUring => unsafe { (*WriteQueue::as_io_uring_const(self)).empty() },
            Backend::Epoll => unsafe { (*WriteQueue::as_epoll_const(self)).empty() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// PORT NOTE: `pub fn ensureTag(self: *Self, comptime tag: xev.Backend)`
    /// (dynamic.zig:383) — verbatim the `Completion` one.
    pub fn ensure_tag(&mut self, tag: Backend) {
        // SAFETY: `self` is a live, uniquely-borrowed `WriteQueue`.
        unsafe { WriteQueue::ensure_tag_raw(self, tag) }
    }

    /// See `Completion::ensure_tag_raw`.
    ///
    /// # Safety
    /// `q` must be a valid, stable `WriteQueue` pointer with no live requests
    /// under a different tag.
    pub(super) unsafe fn ensure_tag_raw(q: *mut WriteQueue, tag: Backend) {
        if unsafe { (*q).tag } == tag {
            return;
        }
        unsafe { ptr::write(q, WriteQueue::zeroed_for(tag)) };
    }

    /// `&@field(q.value, @tagName(tag))` (stream.zig:747). Identity.
    ///
    /// # Safety
    /// `q`'s live arm must be the io_uring one.
    pub(super) unsafe fn as_io_uring(q: *mut WriteQueue) -> *mut io_uring::WriteQueue {
        unsafe { ptr::addr_of_mut!((*q).value) }.cast::<io_uring::WriteQueue>()
    }

    /// See `as_io_uring`.
    ///
    /// # Safety
    /// `q`'s live arm must be the epoll one.
    pub(super) unsafe fn as_epoll(q: *mut WriteQueue) -> *mut epoll::WriteQueue {
        unsafe { ptr::addr_of_mut!((*q).value) }.cast::<epoll::WriteQueue>()
    }

    /// The shared-reference twin of `as_io_uring`, for the `&self` accessors.
    ///
    /// # Safety
    /// See `as_io_uring`.
    unsafe fn as_io_uring_const(q: *const WriteQueue) -> *const io_uring::WriteQueue {
        unsafe { ptr::addr_of!((*q).value) }.cast::<io_uring::WriteQueue>()
    }

    /// See `as_io_uring_const`.
    ///
    /// # Safety
    /// See `as_epoll`.
    unsafe fn as_epoll_const(q: *const WriteQueue) -> *const epoll::WriteQueue {
        unsafe { ptr::addr_of!((*q).value) }.cast::<epoll::WriteQueue>()
    }
}

/// WriteRequest is a single request for a write. It wraps a
/// completion so that it can be inserted into the WriteQueue.
//
// PORT NOTE: `pub const WriteRequest = Dynamic.Union(&.{"WriteRequest"})`
// (dynamic.zig:69) — an UNTAGGED union, unlike `WriteQueue`/`Completion`. The
// reference relies on `queueWrite` re-initializing it every time
// (stream.zig:737-741: `req.* = @unionInit(xev.WriteRequest, @tagName(tag),
// undefined)`), so it never needs to know its own arm.
//
// PORT NOTE: `Default` is REQUIRED (`queue_write`'s wholesale re-init uses
// `..WriteRequest::default()`; termio's pooled `ThreadData.Write` node hands the
// embedded request over logically uninitialized) and hand-written, never
// derived. The reference's pool elements are `undefined`; the zero value here is
// the LAST candidate's variant, consistent with `Completion`/`WriteQueue`
// (dynamic.zig:251/377) — and `queue_write` overwrites it wholesale regardless.
#[repr(C)]
pub union WriteRequest {
    io_uring: ManuallyDrop<io_uring::WriteRequest>,
    epoll: ManuallyDrop<epoll::WriteRequest>,
}

impl Default for WriteRequest {
    fn default() -> Self {
        WriteRequest {
            epoll: ManuallyDrop::new(epoll::WriteRequest::default()),
        }
    }
}

// PORT NOTE: both backends' `WriteRequest`s carry a documented `unsafe impl
// Send` (raw-ptr `next` link + an embedded fn-ptr `Completion`; they ride the
// `Loop` moved onto the termio thread, and after the move exactly one thread
// touches them). Rust derives a union's auto traits from its arms, so this union
// is `Send` for the same reason with no explicit `unsafe impl` needed.

impl WriteRequest {
    /// `&@field(req, @tagName(tag))` (stream.zig:748). Identity (union arms sit at
    /// offset 0 of a `#[repr(C)]` union).
    ///
    /// # Safety
    /// `r` must be a valid `WriteRequest` pointer; the caller must be about to
    /// (re-)initialize it for the io_uring arm, as `queue_write` does.
    pub(super) unsafe fn as_io_uring(r: *mut WriteRequest) -> *mut io_uring::WriteRequest {
        r.cast::<io_uring::WriteRequest>()
    }

    /// See `as_io_uring`.
    ///
    /// # Safety
    /// See `as_io_uring`.
    pub(super) unsafe fn as_epoll(r: *mut WriteRequest) -> *mut epoll::WriteRequest {
        r.cast::<epoll::WriteRequest>()
    }
}

// ---------------------------------------------------------------------------
// Stream (watcher/stream.zig `GenericStream` dynamic arm, stream.zig:1044-1080 +
// `Closeable`/`Pollable`/`Readable`/`Writeable` dynamic arms).
// ---------------------------------------------------------------------------

/// Creates a generic stream type that supports read, write, close, poll. This
/// can be used for any file descriptor that would exhibit normal blocking
/// behavior on read/write. This should NOT be used for local files because
/// local files have some special properties; you should use xev.File for that.
//
// PORT NOTE: `GenericStream(xev)`'s dynamic arm (stream.zig:1044) — `struct {
// backend: xev.Union(&.{"Stream"}) }` with `options = { .close = true, .poll =
// true, .read = .read, .write = .write, .type = "Stream" }`. UNTAGGED union in
// the reference; both arms here are `Copy` fd handles, so the Rust `union` needs
// no `ManuallyDrop`, and `Stream` stays `Copy` (the reference's
// `read`/`write`/`close`/`poll`/`queueWrite` all take `self: Self` by value).
//
// PORT NOTE: switching on the SELF-carried arm is impossible for an untagged
// union, so — exactly like the reference — every method switches on the GLOBAL
// (`switch (xev.backend)`).
#[derive(Clone, Copy)]
pub struct Stream {
    backend: StreamUnion,
}

#[repr(C)]
#[derive(Clone, Copy)]
union StreamUnion {
    io_uring: io_uring::Stream,
    epoll: epoll::Stream,
}

impl Stream {
    /// Initialize a generic stream from a file descriptor.
    //
    // PORT NOTE: `pub fn initFd(fd: std.posix.pid_t) Self` (stream.zig:1065) —
    // note the reference's `pid_t` parameter type is an upstream typo for `fd_t`
    // (both are `c_int`); ported as the `fd_t` it actually is.
    pub fn init_fd(fd: libc::c_int) -> Stream {
        match backend() {
            Backend::IoUring => Stream {
                backend: StreamUnion {
                    io_uring: io_uring::Stream::init_fd(fd),
                },
            },
            Backend::Epoll => Stream {
                backend: StreamUnion {
                    epoll: epoll::Stream::init_fd(fd),
                },
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Clean up any watcher resources. This does NOT close the file.
    pub fn deinit(&self) {
        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe { self.backend.io_uring.deinit() },
            Backend::Epoll => unsafe { self.backend.epoll.deinit() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// The `Self.initFd(s_inner.fd)` re-wrap every dynamic stream trampoline does
    /// (stream.zig:665).
    //
    // PORT NOTE: the reference reaches through the backend stream's `fd` field and
    // rebuilds; the backends' `fd` is module-private here, so the equivalent (and
    // cheaper) re-wrap of the very same handle is used instead — `initFd(s.fd)`
    // and "wrap `s`" are the same value, since `Stream` IS its `fd`.
    fn from_io_uring(s: io_uring::Stream) -> Stream {
        Stream {
            backend: StreamUnion { io_uring: s },
        }
    }

    /// See `from_io_uring`.
    fn from_epoll(s: epoll::Stream) -> Stream {
        Stream {
            backend: StreamUnion { epoll: s },
        }
    }

    // -------------------------------------------------------------------
    // Readable (stream.zig:444 dynamic arm). `options.read == .read`.
    // -------------------------------------------------------------------

    /// Read from the stream. This performs a single read.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice)
    /// outlives the read.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                self.backend.io_uring.read(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            ReadBuffer::from_io_uring(b_inner),
                            r.map_err(ReadError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                self.backend.epoll.read(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            ReadBuffer::from_epoll(b_inner),
                            r.map_err(ReadError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    // -------------------------------------------------------------------
    // Writeable (stream.zig:621 dynamic arm). `options.write == .write`.
    // -------------------------------------------------------------------

    /// Write to the stream. This performs a single write, unordered with
    /// respect to other writes; use `queue_write` if ordering matters.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable; `buf` (if a slice) outlives the write.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                self.backend.io_uring.write(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                self.backend.epoll.write(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Write to the stream. This queues the writes to ensure they remain in
    /// order.
    //
    // PORT NOTE: `pub fn queueWrite(...)` (stream.zig:686). Note the reference
    // does NOT `ensureTag` the completion here — the completion lives INSIDE the
    // `WriteRequest`, which `queueWrite` re-initializes wholesale
    // (stream.zig:737-741) — but it DOES `q.ensureTag(tag)` first
    // ("Ensure our WriteQueue has the correct tag, since it is regularly
    // zero-initialized and our zero-init picks an arbitrary backend",
    // stream.zig:733-735). Both are reproduced exactly.
    //
    // PORT NOTE (REFERENCE QUIRK, preserved — the `c` handed to `cb`): the
    // reference's `api_cb` (stream.zig:660-663) `@fieldParentPtr`s `c_inner` back
    // to a `*xev.Completion` — but `c_inner` here is `&req.<tag>.completion`, a
    // completion nested in the BACKEND `WriteRequest`, NOT one nested in a Dynamic
    // `Completion`. So the recovered pointer does not actually point at a valid
    // Dynamic `Completion` (its `tag` field would be whatever the `WriteRequest`
    // has at that offset). Upstream gets away with it because the pointer is only
    // ever passed through, never dereferenced — ghostty's `ttyWrite` callback
    // spells the parameter `_: *xev.Completion` (Exec.zig), as does this port's
    // `rustty-termio` `ttyWrite`. Reproduced faithfully (the `@fieldParentPtr`
    // here is an identity cast, so no out-of-bounds pointer is even computed), and
    // flagged loudly: DO NOT dereference the `c` passed to a `queue_write`
    // callback.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks. The `c` passed to `cb` MUST NOT be
    /// dereferenced (see the PORT NOTE above).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                // Ensure our WriteQueue has the correct tag, since it is
                // regularly zero-initialized and our zero-init picks
                // an arbitrary backend.
                WriteQueue::ensure_tag_raw(q, Backend::IoUring);
                // Initialize our request since it is usually undefined.
                // (`io_uring::Stream::queue_write` fully overwrites `*req`, which
                // is what the reference's `@unionInit(..., undefined)` amounts to
                // for an untagged union.)
                self.backend.io_uring.queue_write(
                    Loop::as_io_uring(loop_),
                    WriteQueue::as_io_uring(q),
                    WriteRequest::as_io_uring(req),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                WriteQueue::ensure_tag_raw(q, Backend::Epoll);
                self.backend.epoll.queue_write(
                    Loop::as_epoll(loop_),
                    WriteQueue::as_epoll(q),
                    WriteRequest::as_epoll(req),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    // -------------------------------------------------------------------
    // Closeable (stream.zig:320 dynamic arm). `options.close == true`.
    // -------------------------------------------------------------------

    /// Close the file.
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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                self.backend.io_uring.close(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            r.map_err(CloseError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                self.backend.epoll.close(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            r.map_err(CloseError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    // -------------------------------------------------------------------
    // Pollable (stream.zig:140 dynamic arm). `options.poll == true`; both
    // candidates expose `poll`, so the reference's "if any candidate lacks
    // poll, drop the decl" comptime guard (stream.zig:141-150) resolves to
    // "keep it".
    // -------------------------------------------------------------------

    /// Poll for events but do not perform any operations on them being ready.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn poll<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        event: PollEvent,
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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                self.backend.io_uring.poll(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    event.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            r.map(PollEvent::from_io_uring)
                                .map_err(PollError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                self.backend.epoll.poll(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    event.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            r.map(PollEvent::from_epoll).map_err(PollError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }
}
