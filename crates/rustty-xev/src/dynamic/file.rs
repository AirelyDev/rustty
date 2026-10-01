//! The Dynamic `File` watcher.
//!
//! PORTS: libxev src/watcher/file.zig `FileDynamic` — the dynamic-dispatch arm over
//! the io_uring and epoll backends.

use super::*;

// ---------------------------------------------------------------------------
// File (watcher/file.zig `FileDynamic`, file.zig:319-499).
//
// PORT NOTE: `FileDynamic(xev)` is `struct { backend: xev.Union(&.{"File"}) }`
// over the two candidates' `api.File`. Spelled as a Rust `enum` (self-tagged)
// exactly like `TCP`/`UDP`/`Async` above — both arms are `Copy` fd handles passed
// BY VALUE and never pointer-projected, so a self-describing enum is safe and is
// what the read/write/pread/pwrite callbacks re-wrap into. The dynamic
// `read`/`write`/`pread`/`pwrite`/`close`/`poll`/`queueWrite` forward to each
// `api.File`'s methods (its `S = stream.Stream(xev, File, .{ .type = "File", ... })`
// decls plus the pread/pwrite pair).
// ---------------------------------------------------------------------------

/// File operations.
///
/// These operations typically run on the event loop thread pool, rather
/// than the core async OS APIs, because most core async OS APIs don't support
/// async operations on regular files (with many caveats attached to that
/// statement). This high-level abstraction will attempt to make the right
/// decision about what to do but this should generally be used by
/// operations that need to run on a thread pool.
//
// PORT NOTE: `Copy` — `File` is held/copied by value (the read/write callbacks
// yield a fresh `File`).
#[derive(Clone, Copy)]
pub enum File {
    IoUring(io_uring::File),
    Epoll(epoll::File),
}

impl File {
    /// Initialize a File from a file descriptor.
    //
    // PORT NOTE: `pub fn init(file: std.fs.File) !Self` (file.zig:341) — the
    // `switch (xev.backend)` constructor. Rust has no `std.fs.File`; the caller
    // passes the raw fd (its `.handle`). Infallible (the reference's `!Self` is
    // vestigial for both arms).
    pub fn init(fd: libc::c_int) -> core::result::Result<File, Error> {
        Ok(match backend() {
            Backend::IoUring => File::IoUring(io_uring::File::init(fd)),
            Backend::Epoll => File::Epoll(epoll::File::init(fd)),
            be => unreachable!("non-candidate backend {be:?}"),
        })
    }

    /// Initialize a File from a file descriptor.
    //
    // PORT NOTE: `pub fn initFd(fd: std.posix.pid_t) Self` (file.zig:354) — note
    // the reference's `pid_t` parameter type is an upstream typo for `fd_t` (both
    // are `c_int`); ported as the `fd_t` it actually is.
    pub fn init_fd(fd: libc::c_int) -> File {
        match backend() {
            Backend::IoUring => File::IoUring(io_uring::File::init_fd(fd)),
            Backend::Epoll => File::Epoll(epoll::File::init_fd(fd)),
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Clean up any watcher resources. This does NOT close the file.
    //
    // DIVERGENCE PORT NOTE: the oracle's `FileDynamic` (file.zig:319-499) exposes
    // NO `deinit` — it forwards only `close`/`poll`/`read`/`write`/`queueWrite`
    // from the shared `Stream`, plus `pread`/`pwrite`/`init`/`initFd`. `deinit`
    // lives on `FileStream` (file.zig:72) only. This method is a deliberate
    // surface EXTENSION: it mirrors `FileStream.deinit` through the Dynamic seam
    // (dispatch to the live backend arm's `deinit`) so the Dynamic `File` has the
    // same teardown surface as the per-backend one.
    pub fn deinit(&self) {
        match self {
            File::IoUring(f) => f.deinit(),
            File::Epoll(f) => f.deinit(),
        }
    }

    /// The `File::from_io_uring`/`from_epoll` re-wrap the read/write/pread/pwrite
    /// trampolines do (`initFd(s_inner.fd)`, file.zig:408/472).
    fn from_io_uring(f: io_uring::File) -> File {
        File::IoUring(f)
    }

    /// See `from_io_uring`.
    fn from_epoll(f: epoll::File) -> File {
        File::Epoll(f)
    }

    // -------------------------------------------------------------------
    // Readable (stream.zig:444 dynamic arm). `options.read == .read`.
    // -------------------------------------------------------------------

    /// Read from the file. This performs a single read.
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
                File,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::read cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (File::IoUring(f), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                f.read(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            File::from_io_uring(s_inner),
                            ReadBuffer::from_io_uring(b_inner),
                            r_inner.map_err(ReadError::from_io_uring),
                        )
                    },
                );
            },
            (File::Epoll(f), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                f.read(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            File::from_epoll(s_inner),
                            ReadBuffer::from_epoll(b_inner),
                            r_inner.map_err(ReadError::from_epoll),
                        )
                    },
                );
            },
            (f, be) => file_backend_mismatch(f, be),
        }
    }

    /// Read from the file at the given offset.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice)
    /// outlives the read.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn pread<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: ReadBuffer,
        offset: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::pread cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (File::IoUring(f), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                f.pread(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    offset,
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            File::from_io_uring(s_inner),
                            ReadBuffer::from_io_uring(b_inner),
                            r_inner.map_err(ReadError::from_io_uring),
                        )
                    },
                );
            },
            (File::Epoll(f), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                f.pread(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    offset,
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            File::from_epoll(s_inner),
                            ReadBuffer::from_epoll(b_inner),
                            r_inner.map_err(ReadError::from_epoll),
                        )
                    },
                );
            },
            (f, be) => file_backend_mismatch(f, be),
        }
    }

    // -------------------------------------------------------------------
    // Writeable (stream.zig:621 dynamic arm). `options.write == .write`.
    // -------------------------------------------------------------------

    /// Write to the file. This performs a single write, unordered with respect to
    /// other writes; use `queue_write` if ordering matters.
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
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (File::IoUring(f), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                f.write(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            File::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r_inner.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            (File::Epoll(f), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                f.write(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            File::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r_inner.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            (f, be) => file_backend_mismatch(f, be),
        }
    }

    /// Write to the file at the given offset. This performs a single write.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable; `buf` (if a slice) outlives the write.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn pwrite<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: WriteBuffer,
        offset: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::pwrite cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (File::IoUring(f), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                f.pwrite(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    offset,
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            File::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r_inner.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            (File::Epoll(f), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                f.pwrite(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    offset,
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            File::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r_inner.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            (f, be) => file_backend_mismatch(f, be),
        }
    }

    /// Write to the file. This queues the writes to ensure they remain in order.
    //
    // PORT NOTE: same `q.ensureTag(tag)` + wholesale-`req`-init contract as
    // `Stream::queue_write` (stream.zig:686); the `c` handed to `cb` MUST NOT be
    // dereferenced (see `Stream::queue_write`'s PORT NOTE).
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks. The `c` passed to `cb` MUST NOT be
    /// dereferenced.
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
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::queue_write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (File::IoUring(f), Backend::IoUring) => unsafe {
                WriteQueue::ensure_tag_raw(q, Backend::IoUring);
                f.queue_write(
                    Loop::as_io_uring(loop_),
                    WriteQueue::as_io_uring(q),
                    WriteRequest::as_io_uring(req),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            File::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r_inner.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            (File::Epoll(f), Backend::Epoll) => unsafe {
                WriteQueue::ensure_tag_raw(q, Backend::Epoll);
                f.queue_write(
                    Loop::as_epoll(loop_),
                    WriteQueue::as_epoll(q),
                    WriteRequest::as_epoll(req),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            File::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r_inner.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            (f, be) => file_backend_mismatch(f, be),
        }
    }

    /// Write to the file at the given offset. This queues the writes to ensure
    /// they remain in order.
    //
    // DIVERGENCE PORT NOTE: the oracle's `FileDynamic` (file.zig:319-499) exposes
    // NO `queuePWrite` — it forwards only `close`/`poll`/`read`/`write`/
    // `queueWrite` from the shared `Stream`, plus `pread`/`pwrite`/`init`/`initFd`.
    // `queuePWrite` lives on `FileStream` (file.zig:151) only. This method is a
    // deliberate surface EXTENSION: it mirrors `FileStream.queuePWrite` through the
    // Dynamic seam (backend-dispatch + union-tag maintenance) so the Dynamic `File`
    // has the same offset-write ergonomics as the per-backend one.
    //
    // PORT NOTE: models `FileStream.queuePWrite` (file.zig:151). Same
    // `WriteRequest`-wholesale-init contract as `queue_write`. Unlike the Zig
    // `FileStream` (whose `WriteQueue` is a single-backend concrete type), the
    // Dynamic `WriteQueue` is a self-tagged union, so both arms MUST call
    // `WriteQueue::ensure_tag_raw(q, tag)` before the union casts — required when
    // the queue was default-tagged under the OTHER backend (same discipline as
    // `queue_write` above). The `c` handed to `cb` MUST NOT be dereferenced.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks. The `c` passed to `cb` MUST NOT be
    /// dereferenced.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn queue_pwrite<UD, CB>(
        self,
        loop_: *mut Loop,
        q: *mut WriteQueue,
        req: *mut WriteRequest,
        buf: WriteBuffer,
        offset: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::queue_pwrite cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (File::IoUring(f), Backend::IoUring) => unsafe {
                WriteQueue::ensure_tag_raw(q, Backend::IoUring);
                f.queue_pwrite(
                    Loop::as_io_uring(loop_),
                    WriteQueue::as_io_uring(q),
                    WriteRequest::as_io_uring(req),
                    buf.to_io_uring(),
                    offset,
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            File::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r_inner.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            (File::Epoll(f), Backend::Epoll) => unsafe {
                WriteQueue::ensure_tag_raw(q, Backend::Epoll);
                f.queue_pwrite(
                    Loop::as_epoll(loop_),
                    WriteQueue::as_epoll(q),
                    WriteRequest::as_epoll(req),
                    buf.to_epoll(),
                    offset,
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            File::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r_inner.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            (f, be) => file_backend_mismatch(f, be),
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
                File,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::close cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (File::IoUring(f), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                f.close(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            File::from_io_uring(s_inner),
                            r_inner.map_err(CloseError::from_io_uring),
                        )
                    },
                );
            },
            (File::Epoll(f), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                f.close(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            File::from_epoll(s_inner),
                            r_inner.map_err(CloseError::from_epoll),
                        )
                    },
                );
            },
            (f, be) => file_backend_mismatch(f, be),
        }
    }

    // -------------------------------------------------------------------
    // Pollable (stream.zig:140 dynamic arm). `options.poll == true`.
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
                File,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::poll cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (File::IoUring(f), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                f.poll(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    event.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            File::from_io_uring(s_inner),
                            r_inner
                                .map(PollEvent::from_io_uring)
                                .map_err(PollError::from_io_uring),
                        )
                    },
                );
            },
            (File::Epoll(f), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                f.poll(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    event.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            File::from_epoll(s_inner),
                            r_inner
                                .map(PollEvent::from_epoll)
                                .map_err(PollError::from_epoll),
                        )
                    },
                );
            },
            (f, be) => file_backend_mismatch(f, be),
        }
    }
}

/// The `File`-method mismatch-panic, shared like `tcp_backend_mismatch`.
fn file_backend_mismatch(f: File, be: Backend) -> ! {
    let self_be = match f {
        File::IoUring(_) => Backend::IoUring,
        File::Epoll(_) => Backend::Epoll,
    };
    panic!(
        "File handle backend {self_be:?} does not match the global backend {be:?} — \
         the backend must be set before initializing any other types"
    )
}
