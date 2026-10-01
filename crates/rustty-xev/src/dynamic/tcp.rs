//! The Dynamic `TCP` watcher.
//!
//! PORTS: libxev src/watcher/tcp.zig `TCPDynamic` — the dynamic-dispatch arm over
//! the io_uring and epoll backends.

use super::*;

// ---------------------------------------------------------------------------
// TCP (watcher/tcp.zig `TCPDynamic`, tcp.zig:246-502).
//
// PORT NOTE: `TCPDynamic(xev)` is `struct { backend: xev.Union(&.{"TCP"}) }`
// over the two candidates' `api.TCP`. Spelled as a Rust `enum` (self-tagged)
// exactly like `Async` above — both arms are `Copy` fd handles passed BY VALUE
// (`accept(self)`, `?TCP` in the tests) and never pointer-projected, so nothing
// needs `@fieldParentPtr` layout and a self-describing enum is both safe and
// what the accept/connect callbacks re-wrap into. The dynamic `read`/`write`/
// `close`/`poll`/`queue_write` forward to each `api.TCP`'s recv/send/close/poll
// methods (its `S = stream.Stream(xev, TCP, .{ .type = "TCP", ... })` decls).
// ---------------------------------------------------------------------------

/// TCP client and server.
//
// PORT NOTE: `Copy` — `TCP` is held/copied by value (the tests keep `?TCP`, the
// accept callback yields a fresh `TCP`).
#[derive(Clone, Copy)]
pub enum TCP {
    IoUring(io_uring::TCP),
    Epoll(epoll::TCP),
}

impl TCP {
    /// Initialize a new TCP with the family from the given address. Only
    /// the family is used, the actual address has no impact on the created
    /// resource.
    //
    // PORT NOTE: `pub fn init(addr) !Self` (tcp.zig:272) — the `switch
    // (xev.backend)` constructor.
    pub fn init(addr: net::Address) -> core::result::Result<TCP, Error> {
        Ok(match backend() {
            Backend::IoUring => TCP::IoUring(io_uring::TCP::init(addr)?),
            Backend::Epoll => TCP::Epoll(epoll::TCP::init(addr)?),
            be => unreachable!("non-candidate backend {be:?}"),
        })
    }

    /// Initialize a TCP socket from a file descriptor.
    //
    // PORT NOTE: `pub fn initFd(fdvalue) Self` (tcp.zig:285).
    pub fn init_fd(fd: libc::c_int) -> TCP {
        match backend() {
            Backend::IoUring => TCP::IoUring(io_uring::TCP::init_fd(fd)),
            Backend::Epoll => TCP::Epoll(epoll::TCP::init_fd(fd)),
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// The `TCP::from_io_uring`/`from_epoll` re-wrap the accept/connect/shutdown
    /// trampolines do (`initFd(s_inner.fd)`, tcp.zig:361/420/478).
    fn from_io_uring(t: io_uring::TCP) -> TCP {
        TCP::IoUring(t)
    }

    /// See `from_io_uring`.
    fn from_epoll(t: epoll::TCP) -> TCP {
        TCP::Epoll(t)
    }

    /// The underlying socket file descriptor.
    //
    // PORT NOTE: `pub fn fd(self) FdType` (tcp.zig:316).
    pub fn fd(self) -> libc::c_int {
        match self {
            TCP::IoUring(t) => t.fd(),
            TCP::Epoll(t) => t.fd(),
        }
    }

    /// Bind the address to the socket.
    //
    // PORT NOTE: `pub fn bind(self, addr) !void` (tcp.zig:298).
    pub fn bind(self, addr: net::Address) -> core::result::Result<(), Error> {
        match self {
            TCP::IoUring(t) => t.bind(addr),
            TCP::Epoll(t) => t.bind(addr),
        }
    }

    /// Listen for connections on the socket.
    //
    // PORT NOTE: `pub fn listen(self, backlog: u31) !void` (tcp.zig:307).
    pub fn listen(self, backlog: libc::c_int) -> core::result::Result<(), Error> {
        match self {
            TCP::IoUring(t) => t.listen(backlog),
            TCP::Epoll(t) => t.listen(backlog),
        }
    }

    /// Accept a single connection.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn accept<UD, CB>(
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
                core::result::Result<TCP, AcceptError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::accept cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (TCP::IoUring(t), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                t.accept(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            match r_inner {
                                Ok(t_inner) => Ok(TCP::from_io_uring(t_inner)),
                                Err(e) => Err(AcceptError::from_io_uring(e)),
                            },
                        )
                    },
                );
            },
            (TCP::Epoll(t), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                t.accept(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            match r_inner {
                                Ok(t_inner) => Ok(TCP::from_epoll(t_inner)),
                                Err(e) => Err(AcceptError::from_epoll(e)),
                            },
                        )
                    },
                );
            },
            (t, be) => tcp_backend_mismatch(t, be),
        }
    }

    /// Establish a connection as a client.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn connect<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        addr: net::Address,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                core::result::Result<(), ConnectError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::connect cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (TCP::IoUring(t), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                t.connect(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    addr,
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            TCP::from_io_uring(s_inner),
                            r_inner.map_err(ConnectError::from_io_uring),
                        )
                    },
                );
            },
            (TCP::Epoll(t), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                t.connect(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    addr,
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            TCP::from_epoll(s_inner),
                            r_inner.map_err(ConnectError::from_epoll),
                        )
                    },
                );
            },
            (t, be) => tcp_backend_mismatch(t, be),
        }
    }

    /// Shutdown the socket. This always only shuts down the writer side.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn shutdown<UD, CB>(
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
                TCP,
                core::result::Result<(), ShutdownError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::shutdown cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (TCP::IoUring(t), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                t.shutdown(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            TCP::from_io_uring(s_inner),
                            r_inner.map_err(ShutdownError::from_io_uring),
                        )
                    },
                );
            },
            (TCP::Epoll(t), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                t.shutdown(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            TCP::from_epoll(s_inner),
                            r_inner.map_err(ShutdownError::from_epoll),
                        )
                    },
                );
            },
            (t, be) => tcp_backend_mismatch(t, be),
        }
    }

    /// Read from the socket. This performs a single read.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the read.
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
                TCP,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::read cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (TCP::IoUring(t), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                t.read(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            TCP::from_io_uring(s_inner),
                            ReadBuffer::from_io_uring(b_inner),
                            r_inner.map_err(ReadError::from_io_uring),
                        )
                    },
                );
            },
            (TCP::Epoll(t), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                t.read(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            TCP::from_epoll(s_inner),
                            ReadBuffer::from_epoll(b_inner),
                            r_inner.map_err(ReadError::from_epoll),
                        )
                    },
                );
            },
            (t, be) => tcp_backend_mismatch(t, be),
        }
    }

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
                TCP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (TCP::IoUring(t), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                t.write(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            TCP::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r_inner.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            (TCP::Epoll(t), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                t.write(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            TCP::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r_inner.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            (t, be) => tcp_backend_mismatch(t, be),
        }
    }

    /// Write to the stream. This queues the writes to ensure they remain in
    /// order.
    //
    // PORT NOTE: see the dynamic `Stream::queue_write` PORT NOTES — the same
    // `q.ensureTag`, the same wholesale `req` re-init, and the same
    // "DO NOT dereference the `c` passed to the callback" reference quirk apply.
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
                TCP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::queue_write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (TCP::IoUring(t), Backend::IoUring) => unsafe {
                WriteQueue::ensure_tag_raw(q, Backend::IoUring);
                t.queue_write(
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
                            TCP::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r_inner.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            (TCP::Epoll(t), Backend::Epoll) => unsafe {
                WriteQueue::ensure_tag_raw(q, Backend::Epoll);
                t.queue_write(
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
                            TCP::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r_inner.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            (t, be) => tcp_backend_mismatch(t, be),
        }
    }

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
                TCP,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::close cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (TCP::IoUring(t), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                t.close(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            TCP::from_io_uring(s_inner),
                            r_inner.map_err(CloseError::from_io_uring),
                        )
                    },
                );
            },
            (TCP::Epoll(t), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                t.close(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            TCP::from_epoll(s_inner),
                            r_inner.map_err(CloseError::from_epoll),
                        )
                    },
                );
            },
            (t, be) => tcp_backend_mismatch(t, be),
        }
    }

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
                TCP,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::poll cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (TCP::IoUring(t), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                t.poll(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    event.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            TCP::from_io_uring(s_inner),
                            r_inner
                                .map(PollEvent::from_io_uring)
                                .map_err(PollError::from_io_uring),
                        )
                    },
                );
            },
            (TCP::Epoll(t), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                t.poll(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    event.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            TCP::from_epoll(s_inner),
                            r_inner
                                .map(PollEvent::from_epoll)
                                .map_err(PollError::from_epoll),
                        )
                    },
                );
            },
            (t, be) => tcp_backend_mismatch(t, be),
        }
    }
}

/// The `Async::wait` mismatch-panic message, shared by the `TCP` methods.
//
// PORT NOTE: unreachable in the reference (its untagged union reads the arm the
// global names); here the self-tagged `TCP` guards against a caller changing the
// global under a live handle (dynamic.zig:32-33 forbids it). Panic, don't misread.
fn tcp_backend_mismatch(t: TCP, be: Backend) -> ! {
    let self_be = match t {
        TCP::IoUring(_) => Backend::IoUring,
        TCP::Epoll(_) => Backend::Epoll,
    };
    panic!(
        "TCP handle backend {self_be:?} does not match the global backend {be:?} — \
         the backend must be set before initializing any other types"
    )
}
