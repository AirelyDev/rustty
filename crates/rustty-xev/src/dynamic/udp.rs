//! The Dynamic `UDP` watcher.
//!
//! PORTS: libxev src/watcher/udp.zig `UDPDynamic` — the dynamic-dispatch arm over
//! the io_uring and epoll backends.

use super::*;

// ---------------------------------------------------------------------------
// UDP (watcher/udp.zig `UDPDynamic`, udp.zig:682-898).
//
// PORT NOTE: `UDPDynamic(xev)` is `struct { backend: xev.Union(&.{"UDP"}) }` over
// the two candidates' `api.UDP`. Spelled as a Rust `enum` (self-tagged) exactly
// like the Dynamic `TCP` above — a `UDP` is a bare fd held/copied by value and
// never pointer-projected, so nothing depends on its address.
//
// Its `State` (`xev.Union(&.{ "UDP", "State" })`) is UNTAGGED — the active arm is
// the GLOBAL backend, not a per-value tag — so it is the same `#[repr(C)]`
// struct+union shape as the Dynamic `Loop`/`Completion` (arms at offset 0 ⇒
// `@fieldParentPtr` up/down-casts are identity). `read`/`write` write the correct
// arm into `*s` (the reference's `s.* = @unionInit(State, tag, undefined)`) and
// hand the backend the interior arm pointer; the callback recovers the enclosing
// dynamic `UdpState` from that pointer (`@fieldParentPtr(tag, st_inner)`).
// ---------------------------------------------------------------------------

/// UDP requires some extra state to perform operations (a msghdr + iovec +
/// address storage). This is the Dynamic union of the two backends' `UDP::State`.
//
// PORT NOTE: `pub const State = xev.Union(&.{ "UDP", "State" })` — untagged, arms
// at offset 0. `ManuallyDrop` wraps each arm (the backend states are not `Copy`);
// neither backend `UdpState` has `Drop` glue (all POD), so no drop is skipped.
#[repr(C)]
pub struct UdpState {
    backend: UdpStateUnion,
}

#[repr(C)]
union UdpStateUnion {
    io_uring: ManuallyDrop<io_uring::UdpState>,
    epoll: ManuallyDrop<epoll::UdpState>,
}

// The `@fieldParentPtr` contract: `backend` at offset 0 of `UdpState`, every union
// arm at offset 0 of the union — so the down/up-casts are identity casts.
const _: () = assert!(offset_of!(UdpState, backend) == 0);

impl UdpState {
    /// A throwaway `UdpState` used only to establish a valid Rust value before
    /// `read`/`write` overwrite it (the reference's `var s: State = undefined;`).
    /// Zero-inits the LAST candidate's arm (epoll), mirroring `Completion::default`.
    pub fn undefined() -> UdpState {
        UdpState {
            backend: UdpStateUnion {
                epoll: ManuallyDrop::new(epoll::UdpState::undefined()),
            },
        }
    }

    /// # Safety
    /// `s` valid; its live arm must be the io_uring one.
    unsafe fn as_io_uring(s: *mut UdpState) -> *mut io_uring::UdpState {
        unsafe { ptr::addr_of_mut!((*s).backend) }.cast::<io_uring::UdpState>()
    }

    /// # Safety
    /// `s` valid; its live arm must be the epoll one.
    unsafe fn as_epoll(s: *mut UdpState) -> *mut epoll::UdpState {
        unsafe { ptr::addr_of_mut!((*s).backend) }.cast::<epoll::UdpState>()
    }

    /// The `@fieldParentPtr` up-cast — recover the enclosing `UdpState` from a
    /// pointer to its live union arm. An identity cast (arms at offset 0).
    fn from_io_uring(s: *mut io_uring::UdpState) -> *mut UdpState {
        s.cast::<UdpState>()
    }

    /// See `from_io_uring`.
    fn from_epoll(s: *mut epoll::UdpState) -> *mut UdpState {
        s.cast::<UdpState>()
    }
}

/// UDP client and server.
//
// PORT NOTE: `Copy` — `UDP` is held/copied by value (a bare fd handle).
#[derive(Clone, Copy)]
pub enum UDP {
    IoUring(io_uring::UDP),
    Epoll(epoll::UDP),
}

impl UDP {
    /// Initialize a new UDP with the family from the given address. Only
    /// the family is used, the actual address has no impact on the created
    /// resource.
    pub fn init(addr: net::Address) -> core::result::Result<UDP, Error> {
        Ok(match backend() {
            Backend::IoUring => UDP::IoUring(io_uring::UDP::init(addr)?),
            Backend::Epoll => UDP::Epoll(epoll::UDP::init(addr)?),
            be => unreachable!("non-candidate backend {be:?}"),
        })
    }

    /// Initialize a UDP socket from a file descriptor.
    pub fn init_fd(fd: libc::c_int) -> UDP {
        match backend() {
            Backend::IoUring => UDP::IoUring(io_uring::UDP::init_fd(fd)),
            Backend::Epoll => UDP::Epoll(epoll::UDP::init_fd(fd)),
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    // The `UDP::from_io_uring`/`from_epoll` re-wrap the fd the callbacks yield.
    fn from_io_uring(u: io_uring::UDP) -> UDP {
        UDP::IoUring(u)
    }

    fn from_epoll(u: epoll::UDP) -> UDP {
        UDP::Epoll(u)
    }

    /// The underlying socket file descriptor.
    pub fn fd(self) -> libc::c_int {
        match self {
            UDP::IoUring(u) => u.fd(),
            UDP::Epoll(u) => u.fd(),
        }
    }

    /// Bind the address to the socket.
    pub fn bind(self, addr: net::Address) -> core::result::Result<(), Error> {
        match self {
            UDP::IoUring(u) => u.bind(addr),
            UDP::Epoll(u) => u.bind(addr),
        }
    }

    /// Read from the socket. This performs a single read.
    ///
    /// # Safety
    /// `c`/`loop_`/`s` valid+stable (held raw by the loop); `s` must outlive the
    /// op (the msghdr embedded in it points back into it); `buf` (if a slice)
    /// outlives the read.
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

        match (self, backend()) {
            (UDP::IoUring(u), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                // s.* = @unionInit(State, .io_uring, undefined)
                (*s).backend = UdpStateUnion {
                    io_uring: ManuallyDrop::new(io_uring::UdpState::undefined()),
                };
                u.read(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    UdpState::as_io_uring(s),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, st_inner, addr_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            UdpState::from_io_uring(st_inner),
                            addr_inner,
                            UDP::from_io_uring(s_inner),
                            ReadBuffer::from_io_uring(b_inner),
                            r_inner.map_err(ReadError::from_io_uring),
                        )
                    },
                );
            },
            (UDP::Epoll(u), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                // s.* = @unionInit(State, .epoll, undefined)
                (*s).backend = UdpStateUnion {
                    epoll: ManuallyDrop::new(epoll::UdpState::undefined()),
                };
                u.read(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    UdpState::as_epoll(s),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, st_inner, addr_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            UdpState::from_epoll(st_inner),
                            addr_inner,
                            UDP::from_epoll(s_inner),
                            ReadBuffer::from_epoll(b_inner),
                            r_inner.map_err(ReadError::from_epoll),
                        )
                    },
                );
            },
            (u, be) => udp_backend_mismatch(u, be),
        }
    }

    /// Write to the socket. This performs a single write.
    ///
    /// # Safety
    /// `c`/`loop_`/`s` valid+stable (held raw by the loop); `s` must outlive the
    /// op (the msghdr embedded in it points back into it); `buf` (if a slice)
    /// outlives the write.
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

        match (self, backend()) {
            (UDP::IoUring(u), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                (*s).backend = UdpStateUnion {
                    io_uring: ManuallyDrop::new(io_uring::UdpState::undefined()),
                };
                u.write(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    UdpState::as_io_uring(s),
                    addr,
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, st_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            UdpState::from_io_uring(st_inner),
                            UDP::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r_inner.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            (UDP::Epoll(u), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                (*s).backend = UdpStateUnion {
                    epoll: ManuallyDrop::new(epoll::UdpState::undefined()),
                };
                u.write(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    UdpState::as_epoll(s),
                    addr,
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, st_inner, s_inner, b_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            UdpState::from_epoll(st_inner),
                            UDP::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r_inner.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            (u, be) => udp_backend_mismatch(u, be),
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

        match (self, backend()) {
            (UDP::IoUring(u), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                u.close(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            UDP::from_io_uring(s_inner),
                            r_inner.map_err(CloseError::from_io_uring),
                        )
                    },
                );
            },
            (UDP::Epoll(u), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                u.close(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            UDP::from_epoll(s_inner),
                            r_inner.map_err(CloseError::from_epoll),
                        )
                    },
                );
            },
            (u, be) => udp_backend_mismatch(u, be),
        }
    }

    /// Poll for events but do not perform any operations on them being ready.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
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

        match (self, backend()) {
            (UDP::IoUring(u), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                u.poll(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    event.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            UDP::from_io_uring(s_inner),
                            r_inner
                                .map(PollEvent::from_io_uring)
                                .map_err(PollError::from_io_uring),
                        )
                    },
                );
            },
            (UDP::Epoll(u), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                u.poll(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    event.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r_inner| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            UDP::from_epoll(s_inner),
                            r_inner
                                .map(PollEvent::from_epoll)
                                .map_err(PollError::from_epoll),
                        )
                    },
                );
            },
            (u, be) => udp_backend_mismatch(u, be),
        }
    }
}

/// The `UDP`-method mismatch-panic, shared like `tcp_backend_mismatch`.
fn udp_backend_mismatch(u: UDP, be: Backend) -> ! {
    let self_be = match u {
        UDP::IoUring(_) => Backend::IoUring,
        UDP::Epoll(_) => Backend::Epoll,
    };
    panic!(
        "UDP handle backend {self_be:?} does not match the global backend {be:?} — \
         the backend must be set before initializing any other types"
    )
}
