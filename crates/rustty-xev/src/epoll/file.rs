//! The epoll backend's `File` watcher.
//!
//! PORTS: libxev src/watcher/file.zig `FileStream`, monomorphized for the epoll
//! backend.

use super::*;

// ---------------------------------------------------------------------------
// File (watcher/file.zig `FileStream`, file.zig:33-317) — the per-backend
// (`epoll`) instantiation of the regular-file watcher.
//
// PORT NOTE: `xev.File` = a regular-fd handle over `stream.Stream(xev, Self, .{
// .close = true, .poll = true, .read = .read, .write = .write, .threadpool = true
// })` PLUS the pread/pwrite ops. This is the epoll arm of the comptime `File(xev)`
// split (`if (xev.dynamic) return FileDynamic(xev) else FileStream(xev)`,
// file.zig:27-30); the Dynamic arm lives in `dynamic.rs`.
//
// PORT NOTE (the Stream monomorphization): File is the SAME stream generic as the
// crate's `Stream`/`TCP`, instantiated with `.read = .read, .write = .write` (so
// the ops are `Operation::Read`/`Operation::Write`, like `Stream`) but with
// `.threadpool = true`. On epoll that ONE option flips the Readable/Writeable
// flag switch (`if (options.threadpool) c.flags.threadpool = true else c.flags.dup
// = true`): File's read/write set `threadpool` (routing them through the pool as
// BLOCKING ops), where `Stream`/`TCP` set `dup`. `close` is byte-identical to
// `Stream`'s (already `threadpool`); `poll` is identical (fd-only, no flag) except
// the callback hands back a `File`. The pread/pwrite ops are File-only additions —
// `.read`/`.write` with an `offset`, always threadpool-flagged (file.zig:131/302).
// The buffer helpers (`Stream::write_buffer_length`/`write_buffer_remainder`) are
// backend-shared and reused directly.
// ---------------------------------------------------------------------------

/// File operations.
///
/// These operations typically run on the event loop thread pool, rather
/// than the core async OS APIs, because most core async OS APIs don't support
/// async operations on regular files (with many caveats attached to that
/// statement). This high-level abstraction will attempt to make the right
/// decision about what to do but this should generally be used by
/// operations that need to run on a thread pool. For operations that you're
/// sure are better supported by core async OS APIs (such as sockets, pipes,
/// TTYs, etc.), use a specific high-level abstraction like xev.TCP or
/// the generic xev.Stream.
//
// PORT NOTE: `#[derive(Clone, Copy)]` — the reference `File` is a bare `fd` struct
// passed BY VALUE everywhere (`read(self: Self)`, `?Self` in the tests).
#[derive(Clone, Copy)]
pub struct File {
    /// The underlying file
    fd: fd_t,
}

impl File {
    /// Initialize a File from a file descriptor.
    //
    // PORT NOTE: `pub fn init(file: std.fs.File) !Self { return .{ .fd =
    // file.handle }; }`. Rust has no `std.fs.File` handle-wrapper worth threading
    // through this crate; the caller passes the raw fd (its `std.fs.File.handle`).
    // The reference's `!Self` is vestigial (the body cannot error), so this is
    // infallible.
    pub fn init(fd: fd_t) -> File {
        File { fd }
    }

    /// Initialize a File from a file descriptor.
    pub fn init_fd(fd: fd_t) -> File {
        File { fd }
    }

    /// Clean up any watcher resources. This does NOT close the file.
    /// If you want to close the file you must call close or do so
    /// synchronously.
    pub fn deinit(&self) {
        let _ = self;
    }

    // -------------------------------------------------------------------
    // Readable (stream.zig:443). `options.read == .read` for File.
    // -------------------------------------------------------------------

    /// Read from the file. This performs a single read.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the read.
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
            // epoll arm: `if (options.threadpool) threadpool=true else dup=true`.
            // File `options.threadpool == true` ⇒ threadpool.
            Completion::set_threadpool(c, true);
            (*loop_).add(c);
        }
    }

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
                File,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
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
            File::init_fd(fd),
            buffer,
            mapped,
        )
    }

    /// Read from the file at the given offset.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the read.
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
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::PRead {
                    fd: self.fd,
                    buffer: buf,
                    offset,
                },
                userdata.cast::<c_void>(),
                Self::pread_trampoline::<UD, CB>,
            );
            // epoll arm: `c.flags.threadpool = true` (file.zig:131).
            Completion::set_threadpool(c, true);
            (*loop_).add(c);
        }
    }

    fn pread_trampoline<UD, CB>(
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
                File,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (fd, buffer) = match unsafe { &(*c_inner).op } {
            Operation::PRead { fd, buffer, .. } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, ReadError> = match r {
            Result::PRead(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            File::init_fd(fd),
            buffer,
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Writeable (stream.zig:620). `options.write == .write` for File.
    // -------------------------------------------------------------------

    /// Write to the file. This queues the writes to ensure they remain in
    /// order.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks.
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
        let _ = cb;

        unsafe {
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
            // our actual completion userdata to be the WriteQueue.
            (*req).userdata = userdata.cast::<c_void>();

            // If the queue is empty, then we add our completion.
            if (*q).empty() {
                (*loop_).add(ptr::addr_of_mut!((*req).completion));
            }

            // We always add this item to our queue no matter what
            (*q).push(req);
        }
    }

    /// The ordered-write engine trampoline — chains completions through the
    /// `WriteQueue` and handles partial writes by re-`write_init`-ing the remainder.
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
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let q_inner = ud as *mut WriteQueue;
        debug_assert!(!q_inner.is_null());

        // We only peek the request here (not pop) because we may need to rearm
        // this write if the write was partial.
        let req_inner: *mut WriteRequest = unsafe { (*q_inner).head };
        debug_assert!(!req_inner.is_null());

        let (writer, cb_buf, cb_result) = File::write_result(c_inner, r);
        let mut result: core::result::Result<usize, WriteError> = cb_result;

        // Checks whether the entire buffer was written; if the write was partial,
        // it re-submits the remainder of the buffer. (`write_buffer_length`/
        // `write_buffer_remainder` are backend-shared with `Stream`.)
        let queued_len = Stream::write_buffer_length(cb_buf);
        if let Ok(written_len) = cb_result {
            if written_len < queued_len {
                // Write remainder of the buffer, reusing the same completion.
                let rem_buf = Stream::write_buffer_remainder(cb_buf, written_len);
                unsafe {
                    writer.write_init(ptr::addr_of_mut!((*req_inner).completion), rem_buf);
                    (*req_inner).completion.userdata = q_inner as *mut c_void;
                    (*req_inner).completion.callback = Self::queue_write_trampoline::<UD, CB>;
                    (*l_inner).add(ptr::addr_of_mut!((*req_inner).completion));
                }
                return CallbackAction::Disarm;
            }

            // We wrote the entire buffer; report the full length to the caller.
            result = Ok(Stream::write_buffer_length(unsafe {
                (*req_inner).full_write_buffer
            }));
        }

        // We can pop previously peeked request.
        let popped = unsafe { (*q_inner).pop() };
        debug_assert!(!popped.is_null());

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let action = cb(
            userdata_value::<UD>(unsafe { (*req_inner).userdata }),
            l_inner,
            c_inner,
            writer,
            unsafe { (*req_inner).full_write_buffer },
            result,
        );

        // Rearm requeues this request at the end of the queue.
        if action == CallbackAction::Rearm {
            unsafe { (*q_inner).push(req_inner) };
        }

        // If we have another request, add that completion next.
        let req_next = unsafe { (*q_inner).head };
        if !req_next.is_null() {
            unsafe { (*l_inner).add(ptr::addr_of_mut!((*req_next).completion)) };
        }

        // We always disarm because the completion in the next request will be used.
        CallbackAction::Disarm
    }

    /// Write to the file. This performs a single write (NOT queued).
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the write.
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
        let _ = cb;

        unsafe {
            self.write_init(c, buf);
            (*c).userdata = userdata.cast::<c_void>();
            (*c).callback = Self::write_trampoline::<UD, CB>;
            (*loop_).add(c);
        }
    }

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
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (writer, buf, result) = File::write_result(c_inner, r);
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            writer,
            buf,
            result,
        )
    }

    /// Extracts the result from a completion for a write callback (`.write` arm).
    fn write_result(
        c: *mut Completion,
        r: Result,
    ) -> (File, WriteBuffer, core::result::Result<usize, WriteError>) {
        let (fd, buffer) = match unsafe { &(*c).op } {
            Operation::Write { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let result: core::result::Result<usize, WriteError> = match r {
            Result::Write(res) => res,
            _ => unreachable!(),
        };
        (File::init_fd(fd), buffer, result)
    }

    /// Initialize the completion c for a write. This does NOT set userdata or a
    /// callback.
    //
    // PORT NOTE: epoll arm sets `c.flags.threadpool = true`
    // (`options.threadpool == true`).
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
            Completion::set_threadpool(c, true);
        }
    }

    /// Write to the file at the given offset. This queues the writes to ensure
    /// they remain in order.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks.
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
        let _ = cb;

        unsafe {
            // req.* = .{ .full_write_buffer = buf };
            //
            // Matches CURRENT upstream (libxev 9ce8e8e6, the ghostty 6ad1fe7d pin;
            // file.zig:171): `queuePWrite` now seeds `full_write_buffer` like
            // `queueWrite` does. The tracked oracle (34fa5087, file.zig:170) wrote
            // `req.* = .{}`. Behavior-neutral either way: `queue_pwrite_trampoline`
            // reads only the op's buffer via `pwrite_result`, never
            // `full_write_buffer` — absorbed here so the source body and the ported
            // `queuePWrite` KAT come from the same revision (codex W13A-02).
            *req = WriteRequest {
                full_write_buffer: buf,
                ..WriteRequest::default()
            };
            self.pwrite_init(ptr::addr_of_mut!((*req).completion), buf, offset);
            (*req).completion.userdata = q as *mut c_void;
            (*req).completion.callback = Self::queue_pwrite_trampoline::<UD, CB>;

            // The userdata as to go on the WriteRequest because we need
            // our actual completion userdata to be the WriteQueue.
            (*req).userdata = userdata.cast::<c_void>();

            // If the queue is empty, then we add our completion.
            if (*q).empty() {
                (*loop_).add(ptr::addr_of_mut!((*req).completion));
            }

            // We always add this item to our queue no matter what
            (*q).push(req);
        }
    }

    /// The ordered-pwrite engine trampoline. Mirrors `queuePWrite`'s callback
    /// (file.zig:172-208): pops the request, chains the next, but has NO
    /// partial-write re-arm (pwrite requests are one-shot at their offset).
    fn queue_pwrite_trampoline<UD, CB>(
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
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let q_inner = ud as *mut WriteQueue;
        debug_assert!(!q_inner.is_null());

        // The queue MUST have a request because a completion can only be added if
        // the queue is not empty, and nothing else should be popping!
        let req_inner: *mut WriteRequest = unsafe { (*q_inner).pop() };
        debug_assert!(!req_inner.is_null());

        let (writer, buf, result) = File::pwrite_result(c_inner, r);

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let action = cb(
            userdata_value::<UD>(unsafe { (*req_inner).userdata }),
            l_inner,
            c_inner,
            writer,
            buf,
            result,
        );

        // Rearm requeues this request, it doesn't return rearm on the actual
        // callback here...
        if action == CallbackAction::Rearm {
            unsafe { (*q_inner).push(req_inner) };
        }

        // If we have another request, add that completion next.
        let req_next = unsafe { (*q_inner).head };
        if !req_next.is_null() {
            unsafe { (*l_inner).add(ptr::addr_of_mut!((*req_next).completion)) };
        }

        // We always disarm because the completion in the next request will be used.
        CallbackAction::Disarm
    }

    /// Write to the file at the given offset. This performs a single write.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the write.
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
        let _ = cb;

        unsafe {
            self.pwrite_init(c, buf, offset);
            (*c).userdata = userdata.cast::<c_void>();
            (*c).callback = Self::pwrite_trampoline::<UD, CB>;
            (*loop_).add(c);
        }
    }

    fn pwrite_trampoline<UD, CB>(
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
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (writer, buf, result) = File::pwrite_result(c_inner, r);
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            writer,
            buf,
            result,
        )
    }

    /// Extracts the result from a completion for a pwrite callback (`.pwrite` arm).
    //
    // PORT NOTE: `pwrite_result` (file.zig:264) — the `.pwrite` twin of
    // `write_result`.
    fn pwrite_result(
        c: *mut Completion,
        r: Result,
    ) -> (File, WriteBuffer, core::result::Result<usize, WriteError>) {
        let (fd, buffer) = match unsafe { &(*c).op } {
            Operation::PWrite { fd, buffer, .. } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let result: core::result::Result<usize, WriteError> = match r {
            Result::PWrite(res) => res,
            _ => unreachable!(),
        };
        (File::init_fd(fd), buffer, result)
    }

    /// Initialize the completion c for a pwrite. This does NOT set userdata or a
    /// callback.
    //
    // PORT NOTE: `pwriteInit` (file.zig:276). epoll arm sets `c.flags.threadpool =
    // true` (file.zig:302).
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn pwrite_init(self, c: *mut Completion, buf: WriteBuffer, offset: u64) {
        unsafe {
            *c = Completion::new_op(
                Operation::PWrite {
                    fd: self.fd,
                    buffer: buf,
                    offset,
                },
                ptr::null_mut(),
                noop_callback,
            );
            Completion::set_threadpool(c, true);
        }
    }

    // -------------------------------------------------------------------
    // Closeable (stream.zig:319). `options.close == true` for File.
    // -------------------------------------------------------------------

    /// Close the file.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
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
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Close { fd: self.fd },
                userdata.cast::<c_void>(),
                Self::close_trampoline::<UD, CB>,
            );
            // epoll arm: `c.flags.threadpool = true`.
            Completion::set_threadpool(c, true);
            (*loop_).add(c);
        }
    }

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
                File,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
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
            File::init_fd(fd),
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Pollable (stream.zig:139). `options.poll == true` for File.
    // -------------------------------------------------------------------

    /// Poll the file descriptor for the given event.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
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
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    events: libc::EPOLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::poll_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

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
                File,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Poll { fd, .. } => *fd,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<PollEvent, PollError> = match r {
            Result::Poll(Ok(())) => Ok(PollEvent::Read),
            Result::Poll(Err(e)) => Err(e),
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            File::init_fd(fd),
            mapped,
        )
    }
}
