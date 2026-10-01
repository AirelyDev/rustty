//! The io_uring backend's `TCP` watcher.
//!
//! PORTS: libxev src/watcher/tcp.zig `TCPStream`, monomorphized for the io_uring
//! backend.

use super::*;

// ---------------------------------------------------------------------------
// TCP (watcher/tcp.zig `TCPStream`, tcp.zig:22-244).
//
// PORT NOTE: `xev.TCP` = a socket fd over `stream.Stream(xev, Self, .{ .close =
// true, .poll = true, .read = .recv, .write = .send })` PLUS the
// Accept/Connect/Shutdown ops. This is the per-backend (`io_uring`)
// instantiation of the comptime `TCP(xev)` — the Rust shape of libxev's
// `if (xev.dynamic) return TCPDynamic(xev)` else `TCPStream(xev)` split
// (tcp.zig:17-20); the Dynamic arm lives in `dynamic.rs`.
//
// PORT NOTE (the Stream monomorphization): the crate's `Stream` (above) is the
// GenericStream instantiation (`.read = .read, .write = .write`); TCP is the
// SAME stream generic instantiated with `.read = .recv, .write = .send`, so its
// `read`/`write`/`queue_write` reproduce the `Stream` bodies verbatim but emit
// `Operation::Recv`/`Operation::Send` (and map `Result::Recv`/`Result::Send`).
// `close`/`poll` are identical to `Stream`'s (the op is fd-only) except the
// callback hands back a `TCP` instead of a `Stream`. The io_uring flag switch is
// `{}` in every arm (nothing to set), exactly as `Stream`'s. The buffer-remainder
// helpers (`Stream::write_buffer_length`/`write_buffer_remainder`) are
// backend-shared and reused directly (same module).
// ---------------------------------------------------------------------------

/// TCP client and server.
///
/// This is a "higher-level abstraction" in libxev. The goal of higher-level
/// abstractions in libxev are to make it easier to use specific functionality
/// with the event loop, but does not promise perfect flexibility or optimal
/// performance. In almost all cases, the abstraction is good enough. But,
/// if you have specific needs or want to push for the most optimal performance,
/// use the platform-specific Loop directly.
//
// PORT NOTE: `#[derive(Clone, Copy)]` — the reference `TCP` is a bare `fd`
// struct passed BY VALUE everywhere (`accept(self: Self)`, `?Self` in the tests).
#[derive(Clone, Copy)]
pub struct TCP {
    fd: socket_t,
}

impl TCP {
    /// Initialize a new TCP with the family from the given address. Only
    /// the family is used, the actual address has no impact on the created
    /// resource.
    //
    // PORT NOTE: the io_uring arm OMITS `SOCK.NONBLOCK` ("On io_uring we don't use
    // non-blocking sockets because we may just get EAGAIN over and over from
    // completions").
    pub fn init(addr: net::Address) -> core::result::Result<TCP, Error> {
        // SAFETY: reading `any.family` is valid for any active `Address` member
        // (all overlay `family` as their first field).
        let family = unsafe { addr.any }.family as libc::c_int;
        let flags = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
        let fd = unsafe { libc::socket(family, flags, 0) };
        if fd < 0 {
            return Err(Error);
        }
        Ok(TCP { fd })
    }

    /// Initialize a TCP socket from a file descriptor.
    pub fn init_fd(fd: socket_t) -> TCP {
        TCP { fd }
    }

    /// The underlying socket file descriptor.
    //
    // PORT NOTE: the reference exposes `fd` as a public FIELD (`tcp.zig:27`); the
    // Rust field stays private (mirroring `Stream`) and this accessor stands in —
    // it is what the Dynamic wrapper's `fd()` and `getsockname` in the tests reach.
    pub fn fd(self) -> socket_t {
        self.fd
    }

    /// Bind the address to the socket.
    pub fn bind(self, addr: net::Address) -> core::result::Result<(), Error> {
        let one: libc::c_int = 1;
        // SAFETY: `self.fd` is a live socket; `addr.any` overlays a valid sockaddr
        // for the bound length.
        unsafe {
            if libc::setsockopt(
                self.fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                ptr::addr_of!(one).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            ) != 0
            {
                return Err(Error);
            }
            if libc::bind(
                self.fd,
                ptr::addr_of!(addr.any) as *const libc::sockaddr,
                addr.get_os_sock_len(),
            ) != 0
            {
                return Err(Error);
            }
        }
        Ok(())
    }

    /// Listen for connections on the socket. This puts the socket into passive
    /// listening mode. Connections must still be accepted one at a time.
    //
    // PORT NOTE: the reference `backlog` is `u31` (Zig); Rust has no `u31`, so it
    // is the `c_int` `libc::listen` wants (a `u31` always fits `c_int`).
    pub fn listen(self, backlog: libc::c_int) -> core::result::Result<(), Error> {
        // SAFETY: `self.fd` is a live socket.
        if unsafe { libc::listen(self.fd, backlog) } != 0 {
            return Err(Error);
        }
        Ok(())
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
        let _ = cb;

        unsafe {
            // The reference relies on the Zig `.accept` field defaults
            // (`addr = undefined`, `addr_size = @sizeOf(posix.sockaddr)`,
            // `flags = posix.SOCK.CLOEXEC`); spelled at the call site here.
            *c = Completion::new_op(
                Operation::Accept {
                    socket: self.fd,
                    addr: core::mem::zeroed(),
                    addr_size: size_of::<net::sockaddr::Any>() as net::socklen_t,
                    flags: SOCK_CLOEXEC,
                },
                userdata.cast::<c_void>(),
                Self::accept_trampoline::<UD, CB>,
            );
            // io_uring arm of the dup switch: `{}` (nothing to set).
            (*loop_).add(c);
        }
    }

    fn accept_trampoline<UD, CB>(
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
                core::result::Result<TCP, AcceptError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // if (r.accept) |fd| initFd(fd) else |err| err
        let mapped: core::result::Result<TCP, AcceptError> = match r {
            Result::Accept(Ok(fd)) => Ok(TCP::init_fd(fd)),
            Result::Accept(Err(e)) => Err(e),
            _ => unreachable!(),
        };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
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
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Connect {
                    socket: self.fd,
                    addr,
                },
                userdata.cast::<c_void>(),
                Self::connect_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn connect_trampoline<UD, CB>(
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
                TCP,
                core::result::Result<(), ConnectError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // initFd(c_inner.op.connect.socket), if (r.connect) |_| {} else |err| err
        let socket = match unsafe { &(*c_inner).op } {
            Operation::Connect { socket, .. } => *socket,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<(), ConnectError> = match r {
            Result::Connect(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            TCP::init_fd(socket),
            mapped,
        )
    }

    /// Shutdown the socket. This always only shuts down the writer side. You
    /// can use the lower level interface directly to control this if the
    /// platform supports it.
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
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Shutdown {
                    socket: self.fd,
                    how: ShutdownHow::Send,
                },
                userdata.cast::<c_void>(),
                Self::shutdown_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn shutdown_trampoline<UD, CB>(
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
                TCP,
                core::result::Result<(), ShutdownError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // initFd(c_inner.op.shutdown.socket), if (r.shutdown) |_| {} else |err| err
        let socket = match unsafe { &(*c_inner).op } {
            Operation::Shutdown { socket, .. } => *socket,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<(), ShutdownError> = match r {
            Result::Shutdown(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            TCP::init_fd(socket),
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Readable (stream.zig:443). `options.read == .recv` for TCP.
    // -------------------------------------------------------------------

    /// Read from the socket. This performs a single read.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the read — the KERNEL writes it asynchronously.
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
        let _ = cb;

        unsafe {
            // `options.read == .recv` ⇒ the `.recv` op.
            *c = Completion::new_op(
                Operation::Recv {
                    fd: self.fd,
                    buffer: buf,
                },
                userdata.cast::<c_void>(),
                Self::read_trampoline::<UD, CB>,
            );
            // io_uring arm of the dup switch: `{}` (nothing to set).
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
                TCP,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (fd, buffer) = match unsafe { &(*c_inner).op } {
            Operation::Recv { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, ReadError> = match r {
            Result::Recv(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            TCP::init_fd(fd),
            buffer,
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Writeable (stream.zig:620). `options.write == .send` for TCP.
    // -------------------------------------------------------------------

    /// Write to the stream. This queues the writes to ensure they remain in
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

            // If the queue is empty, then we add our completion. Otherwise,
            // the previously queued writes will trigger this one.
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
                TCP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let q_inner = ud as *mut WriteQueue;
        debug_assert!(!q_inner.is_null());

        // The queue MUST have a request because a completion can only be added if
        // the queue is not empty, and nothing else should be popping!
        //
        // We only peek the request here (not pop) because we may need to rearm
        // this write if the write was partial.
        let req_inner: *mut WriteRequest = unsafe { (*q_inner).head };
        debug_assert!(!req_inner.is_null());

        let (writer, cb_buf, cb_result) = TCP::write_result(c_inner, r);
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

    /// Write to the stream. This performs a single write (NOT queued).
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the write — the KERNEL reads it asynchronously.
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
                TCP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (writer, buf, result) = TCP::write_result(c_inner, r);
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            writer,
            buf,
            result,
        )
    }

    /// Extracts the result from a completion for a write callback (`.send` arm).
    fn write_result(
        c: *mut Completion,
        r: Result,
    ) -> (TCP, WriteBuffer, core::result::Result<usize, WriteError>) {
        let (fd, buffer) = match unsafe { &(*c).op } {
            Operation::Send { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let result: core::result::Result<usize, WriteError> = match r {
            Result::Send(res) => res,
            _ => unreachable!(),
        };
        (TCP::init_fd(fd), buffer, result)
    }

    /// Initialize the completion c for a write. This does NOT set userdata or a
    /// callback.
    //
    // PORT NOTE: `options.write == .send` ⇒ the `.send` op. io_uring arm of the
    // flags switch is `{}` (epoll's sets `dup`).
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn write_init(self, c: *mut Completion, buf: WriteBuffer) {
        unsafe {
            *c = Completion::new_op(
                Operation::Send {
                    fd: self.fd,
                    buffer: buf,
                },
                ptr::null_mut(),
                noop_callback,
            );
        }
    }

    // -------------------------------------------------------------------
    // Closeable (stream.zig:319). `options.close == true` for TCP.
    // -------------------------------------------------------------------

    /// Close the socket.
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
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Close { fd: self.fd },
                userdata.cast::<c_void>(),
                Self::close_trampoline::<UD, CB>,
            );
            // io_uring arm of the flags switch: `{}` (epoll's sets `threadpool`).
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
                TCP,
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
            TCP::init_fd(fd),
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Pollable (stream.zig:139). `options.poll == true` for TCP.
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
        let _ = cb;

        unsafe {
            // io_uring arm: `.poll = { .fd, .events = posix.POLL.IN }` (the `.read`
            // event — the ONLY `PollEvent` the reference defines).
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    events: libc::POLLIN as u32,
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
                TCP,
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
            TCP::init_fd(fd),
            mapped,
        )
    }
}
