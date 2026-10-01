//! The epoll backend's `UDP` watcher.
//!
//! PORTS: libxev src/watcher/udp.zig `UDPSendMsg`, monomorphized for the epoll
//! backend.

use super::*;

// ---------------------------------------------------------------------------
// UDP (watcher/udp.zig `UDPSendMsg`) — the per-backend (`epoll`) instantiation of
// the datagram socket. On both Linux backends `UDP(xev)` selects `UDPSendMsg`
// (udp.zig:20-24), which uses `sendmsg`/`recvmsg` exclusively; this is the epoll
// arm (the io_uring arm is byte-identical apart from `init`'s NONBLOCK and the
// dup/threadpool flag switches). The Dynamic arm lives in `dynamic.rs`.
//
// PORT NOTE (ownership shape): the msghdr + its iovec + the address storage are
// embedded in the caller-owned `UdpState` (udp.zig:405-422) and the msghdr's
// `msg_name`/`msg_iov` point back INTO that same `UdpState` — a self-referential
// struct. `read`/`write` therefore write `*s` FIRST (establishing stable field
// addresses) and only then fix up those interior pointers; the caller must keep
// `s` pinned for the whole in-flight op (exactly the Zig `s: *State` contract).
// `close`/`poll` reproduce `Stream`'s bodies (fd-only ops) but hand back a `UDP`.
// ---------------------------------------------------------------------------

/// UDP requires some extra state to perform operations. The state is
/// opaque. This isn't part of `Completion` because it is relatively
/// large and would force ALL operations (not just UDP) to have a relatively
/// large structure size and we didn't want to pay that cost.
//
// PORT NOTE: the reference `State.op` is an UNTAGGED `union { recv, send }` whose
// active arm is tracked by the in-flight operation; Rust models it as a safe
// tagged `enum` (`UdpStateOp`) — the discriminant is the same information Zig's
// `s.op = .{ .recv = undefined }` tag-set line encodes. `userdata` is the
// reference's `?*anyopaque = null` (set by `read`/`write`).
pub struct UdpState {
    userdata: *mut c_void,
    op: UdpStateOp,
}

// PORT NOTE: `msghdr`/`iov` for the recv arm are the writable `posix.msghdr`/
// `iovec`; the send arm's are `posix.msghdr_const`/`iovec_const`. `libc` has a
// single `msghdr`/`iovec` (constness rides the pointer in C/Rust), so both arms
// name `libc::msghdr`/`libc::iovec` and the send path only ever READS through them.
enum UdpStateOp {
    Recv {
        buf: ReadBuffer,
        addr_buffer: libc::sockaddr_storage,
        msghdr: libc::msghdr,
        iov: [libc::iovec; 1],
    },
    Send {
        buf: WriteBuffer,
        addr: net::Address,
        msghdr: libc::msghdr,
        iov: [libc::iovec; 1],
    },
}

impl UdpState {
    /// A throwaway `UdpState` used only to establish a valid Rust value before
    /// `read`/`write` overwrite it wholesale. Mirrors the reference's
    /// `var s: State = undefined;` (the content is never observed).
    pub fn undefined() -> UdpState {
        UdpState {
            userdata: ptr::null_mut(),
            // SAFETY: `sockaddr_storage`/`msghdr`/`iovec` are POD; all-zero is a
            // valid bit pattern. `ReadBuffer::Slice(empty)` is a valid enum value.
            op: UdpStateOp::Recv {
                buf: ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(ptr::null_mut(), 0)),
                addr_buffer: unsafe { core::mem::zeroed() },
                msghdr: unsafe { core::mem::zeroed() },
                iov: [libc::iovec {
                    iov_base: ptr::null_mut(),
                    iov_len: 0,
                }],
            },
        }
    }
}

/// UDP client and server.
///
/// This is a "higher-level abstraction" in libxev. The goal of higher-level
/// abstractions in libxev are to make it easier to use specific functionality
/// with the event loop, but does not promise perfect flexibility or optimal
/// performance. In almost all cases, the abstraction is good enough. But,
/// if you have specific needs or want to push for the most optimal performance,
/// use the platform-specific Loop directly.
//
// PORT NOTE: `#[derive(Clone, Copy)]` — the reference `UDP` is a bare `fd` struct
// passed BY VALUE everywhere (`bind(self)`, `Impl` in the tests).
#[derive(Clone, Copy)]
pub struct UDP {
    fd: socket_t,
}

impl UDP {
    /// Initialize a new UDP with the family from the given address. Only
    /// the family is used, the actual address has no impact on the created
    /// resource.
    //
    // PORT NOTE: `if (xev.backend != .io_uring) flags |= posix.SOCK.NONBLOCK` —
    // this is the epoll arm, which KEEPS NONBLOCK (io_uring omits it).
    pub fn init(addr: net::Address) -> core::result::Result<UDP, Error> {
        // SAFETY: reading `any.family` is valid for any active `Address` member
        // (all overlay `family` as their first field).
        let family = unsafe { addr.any }.family as libc::c_int;
        let flags = libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK;
        let fd = unsafe { libc::socket(family, flags, 0) };
        if fd < 0 {
            return Err(Error);
        }
        Ok(UDP { fd })
    }

    /// Initialize a UDP socket from a file descriptor.
    pub fn init_fd(fd: socket_t) -> UDP {
        UDP { fd }
    }

    /// The underlying socket file descriptor.
    //
    // PORT NOTE: the reference exposes `fd` as a public FIELD; the Rust field stays
    // private (mirroring `TCP`) and this accessor stands in — it is what the Dynamic
    // wrapper's `fd()` and `getsockname`/`setsockopt` in the tests reach.
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
                libc::SO_REUSEPORT,
                ptr::addr_of!(one).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            ) != 0
            {
                return Err(Error);
            }
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

    /// Read from the socket. This performs a single read. The callback must
    /// requeue the read if additional reads want to be performed. Additional
    /// reads simultaneously can be queued by calling this multiple times. Note
    /// that depending on the backend, the reads can happen out of order.
    ///
    /// # Safety
    /// `c`/`loop_`/`s` valid+stable (held raw by the loop); `s` must outlive the
    /// op (the msghdr embedded in it points back into it); `buf` (if a slice)
    /// outlives the read — the KERNEL writes it asynchronously.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn read<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        s: *mut UdpState,
        buf: ReadBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                *mut UdpState,
                net::Address,
                UDP,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "UDP::read cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // s.* = .{ .userdata, .op = .{ .recv = .{ ... } } }; — write the State
            // FIRST so its interior addresses are stable, then patch the self-refs.
            *s = UdpState {
                userdata: userdata.cast::<c_void>(),
                op: UdpStateOp::Recv {
                    buf,
                    addr_buffer: core::mem::zeroed(),
                    msghdr: core::mem::zeroed(),
                    iov: [libc::iovec {
                        iov_base: ptr::null_mut(),
                        iov_len: 0,
                    }],
                },
            };

            let UdpStateOp::Recv {
                buf,
                addr_buffer,
                msghdr,
                iov,
            } = &mut (*s).op
            else {
                unreachable!()
            };

            match buf {
                ReadBuffer::Slice(v) => {
                    iov[0] = libc::iovec {
                        iov_base: (*v).cast::<u8>() as *mut c_void,
                        iov_len: (*v).len(),
                    };
                }
                ReadBuffer::Array(arr) => {
                    iov[0] = libc::iovec {
                        iov_base: arr.as_mut_ptr().cast::<c_void>(),
                        iov_len: arr.len(),
                    };
                }
            }

            *msghdr = core::mem::zeroed::<libc::msghdr>();
            msghdr.msg_name = ptr::addr_of_mut!(*addr_buffer).cast::<c_void>();
            msghdr.msg_namelen = size_of::<libc::sockaddr_storage>() as net::socklen_t;
            msghdr.msg_iov = iov.as_mut_ptr();
            msghdr.msg_iovlen = 1;

            let msghdr_ptr: *mut libc::msghdr = ptr::addr_of_mut!(*msghdr);

            *c = Completion::new_op(
                Operation::RecvMsg {
                    fd: self.fd,
                    msghdr: msghdr_ptr,
                },
                s as *mut c_void,
                Self::read_trampoline::<UD, CB>,
            );
            // epoll arm of the dup switch: `c.flags.dup = true`.
            Completion::set_dup(c, true);
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
                *mut UdpState,
                net::Address,
                UDP,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // ud is the *State (the Completion's userdata).
        let s_inner = ud as *mut UdpState;
        let fd = match unsafe { &(*c_inner).op } {
            Operation::RecvMsg { fd, .. } => *fd,
            _ => unreachable!(),
        };
        let (addr, buf) = match unsafe { &(*s_inner).op } {
            UdpStateOp::Recv {
                buf, addr_buffer, ..
            } => (
                // std.net.Address.initPosix(@ptrCast(&s_inner.op.recv.addr_buffer))
                unsafe {
                    net::Address::init_posix(
                        ptr::addr_of!(*addr_buffer).cast::<net::sockaddr::Any>(),
                    )
                },
                *buf,
            ),
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, ReadError> = match r {
            Result::RecvMsg(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(unsafe { (*s_inner).userdata }),
            l_inner,
            c_inner,
            s_inner,
            addr,
            UDP::init_fd(fd),
            buf,
            mapped,
        )
    }

    /// Write to the socket. This performs a single write. Additional writes
    /// can be queued by calling this multiple times. Note that depending on the
    /// backend, writes can happen out of order.
    ///
    /// # Safety
    /// `c`/`loop_`/`s` valid+stable (held raw by the loop); `s` must outlive the
    /// op (the msghdr embedded in it points back into it); `buf` (if a slice)
    /// outlives the write — the KERNEL reads it asynchronously.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn write<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        s: *mut UdpState,
        addr: net::Address,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                *mut UdpState,
                UDP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "UDP::write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // s.* = .{ .userdata, .op = .{ .send = .{ ... } } }; — write FIRST, then
            // patch the msghdr's self-references (msg_name → s.op.send.addr, msg_iov
            // → s.op.send.iov).
            *s = UdpState {
                userdata: userdata.cast::<c_void>(),
                op: UdpStateOp::Send {
                    buf,
                    addr,
                    msghdr: core::mem::zeroed(),
                    iov: [libc::iovec {
                        iov_base: ptr::null_mut(),
                        iov_len: 0,
                    }],
                },
            };

            let UdpStateOp::Send {
                buf,
                addr,
                msghdr,
                iov,
            } = &mut (*s).op
            else {
                unreachable!()
            };

            match buf {
                WriteBuffer::Slice(v) => {
                    iov[0] = libc::iovec {
                        iov_base: (*v).cast::<u8>() as *mut c_void,
                        iov_len: (*v).len(),
                    };
                }
                WriteBuffer::Array { array, len } => {
                    iov[0] = libc::iovec {
                        iov_base: array.as_mut_ptr().cast::<c_void>(),
                        iov_len: *len,
                    };
                }
            }

            let namelen = addr.get_os_sock_len();
            *msghdr = core::mem::zeroed::<libc::msghdr>();
            msghdr.msg_name = ptr::addr_of_mut!(addr.any).cast::<c_void>();
            msghdr.msg_namelen = namelen;
            msghdr.msg_iov = iov.as_mut_ptr();
            msghdr.msg_iovlen = 1;

            let msghdr_ptr: *const libc::msghdr = ptr::addr_of!(*msghdr);

            *c = Completion::new_op(
                Operation::SendMsg {
                    fd: self.fd,
                    msghdr: msghdr_ptr,
                    // Zig field defaults: `buffer = null`, `iov = undefined`.
                    buffer: None,
                    iov: [libc::iovec {
                        iov_base: ptr::null_mut(),
                        iov_len: 0,
                    }],
                },
                s as *mut c_void,
                Self::write_trampoline::<UD, CB>,
            );
            // epoll arm of the dup switch: `c.flags.dup = true`.
            Completion::set_dup(c, true);
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
                *mut UdpState,
                UDP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let s_inner = ud as *mut UdpState;
        let fd = match unsafe { &(*c_inner).op } {
            Operation::SendMsg { fd, .. } => *fd,
            _ => unreachable!(),
        };
        let buf = match unsafe { &(*s_inner).op } {
            UdpStateOp::Send { buf, .. } => *buf,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, WriteError> = match r {
            Result::SendMsg(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(unsafe { (*s_inner).userdata }),
            l_inner,
            c_inner,
            s_inner,
            UDP::init_fd(fd),
            buf,
            mapped,
        )
    }

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
                UDP,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "UDP::close cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
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
                UDP,
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
            UDP::init_fd(fd),
            mapped,
        )
    }

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
                UDP,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "UDP::poll cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
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
                UDP,
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
            UDP::init_fd(fd),
            mapped,
        )
    }
}
