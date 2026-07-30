//! wasi.rs — WASI-only shared primitives for the `wasi_poll` backend.
//!
//! Groups the WASI-only ABI bindings (`wasi_snapshot_preview1`: `poll_oneoff`,
//! `clock_time_get`, `sock_*`) the way `linux.rs` groups the Linux-only
//! primitives, PLUS byte-faithful TWINS of `loop.rs`'s pure, backend-agnostic ABI
//! enums (`RunMode`/`CallbackAction`/`CompletionState`/`Options`).
//!
//! PORT NOTE (R5 / no cross-import from macOS-gated files): `loop.rs` defines the
//! same four decls but is `#[cfg(target_os = "macos")]`-gated and its
//! `Callback`/`noop_callback` are coupled to the concrete kqueue types. The pure
//! slice is therefore ported a second time here, exactly as `linux.rs` does for
//! the two Linux backends. This is the established per-target-twin convention of
//! this crate, not a new one.
//!
//! PORT NOTE (the reference's own layering): `backend/wasi_poll.zig` reaches for
//! `std.os.wasi` (the `wasi_snapshot_preview1` import surface) and `std.posix`.
//! Rust's `std` exposes no equivalent public WASI ABI module, so — exactly as
//! `darwin.rs` does for the mach APIs `libc` lacks — the needed imports are
//! declared here against the SAME wasm import module the Zig std binds
//! (`wasi_snapshot_preview1`). This is a binding to the same system ABI, not a
//! reimplementation (R3).

#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

/// TWIN of `loop.rs`'s `Options` (byte-faithful: same fields/derives/hand-written
/// `Default`).
//
// PORT NOTE: `loop.zig`'s field is `thread_pool: ?*xev.ThreadPool`. The
// `wasi_poll` backend never reads it — `Loop.init` is literally `_ = options;`
// (wasi_poll.zig:56-59) — and the crate's `ThreadPool` body (kprotty's Zap pool)
// is built only where its futex primitive has an arm (macOS/Linux today; the
// Windows arm is tracked in DEFERRALS.md as remaining work, NOT as a scope
// exclusion). So the field is carried here as the opaque caller-owned handle the
// backend would pass through untouched. Callers on WASI have no pool to pass.
pub struct Options {
    /// The number of queued completions that can be in flight before
    /// requiring interaction with the kernel.
    ///
    /// Backends: io_uring
    pub entries: u32,

    /// A thread pool to use for blocking operations. If the backend doesn't
    /// need to perform any blocking operations then no threads will ever be
    /// spawned. If the backend does need to perform blocking operations on a
    /// thread and no thread pool is provided, the operations will simply
    /// fail. Unless you're trying to really optimize for space, it is
    /// recommended you provide a thread pool.
    ///
    /// Backends: epoll, kqueue
    pub thread_pool: Option<*mut core::ffi::c_void>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            entries: 256,
            thread_pool: None,
        }
    }
}

/// TWIN of `loop.rs`'s `RunMode` (byte-faithful).
///
/// The loop run mode -- all backends are required to support this in some way.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Run the event loop once. If there are no blocking operations ready,
    /// return immediately.
    NoWait = 0,

    /// Run the event loop once, waiting for at least one blocking operation
    /// to complete.
    Once = 1,

    /// Run the event loop until it is "done".
    UntilDone = 2,
}

/// TWIN of `loop.rs`'s `CallbackAction` (byte-faithful).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackAction {
    /// The request is complete and is not repeated.
    Disarm = 0,

    /// Requeue the same operation request with the same parameters
    /// with the event loop.
    Rearm = 1,
}

/// TWIN of `loop.rs`'s `CompletionState` (byte-faithful).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionState {
    /// The completion is not being used and is ready to be configured
    /// for new work.
    Dead = 0,

    /// The completion is part of an event loop.
    Active = 1,
}

// ---------------------------------------------------------------------------
// The `std.os.wasi` ABI subset `backend/wasi_poll.zig` names.
// ---------------------------------------------------------------------------

/// PORT NOTE: `wasi.timestamp_t` — nanoseconds since an unspecified epoch.
pub type timestamp_t = u64;
/// PORT NOTE: `wasi.userdata_t`.
pub type userdata_t = u64;
/// PORT NOTE: `wasi.fd_t` / `posix.fd_t` on WASI.
pub type fd_t = i32;
/// PORT NOTE: `posix.socket_t` on WASI (a `fd_t`).
pub type socket_t = i32;
/// PORT NOTE: `wasi.clockid_t`.
pub type clockid_t = u32;
/// PORT NOTE: `wasi.errno_t` (the `SUCCESS`/`INVAL`/... enum, ABI `u16`).
pub type errno_t = u16;
/// PORT NOTE: `wasi.eventtype_t`.
pub type eventtype_t = u8;
/// PORT NOTE: `wasi.subclockflags_t`.
pub type subclockflags_t = u16;
/// PORT NOTE: `wasi.eventrwflags_t`.
pub type eventrwflags_t = u16;
/// PORT NOTE: `wasi.filesize_t`.
pub type filesize_t = u64;
/// PORT NOTE: `wasi.riflags_t`.
pub type riflags_t = u16;
/// PORT NOTE: `wasi.roflags_t`.
pub type roflags_t = u16;
/// PORT NOTE: `wasi.siflags_t`.
pub type siflags_t = u16;
/// PORT NOTE: `wasi.sdflags_t`.
pub type sdflags_t = u8;

/// PORT NOTE: `wasi.errno_t.SUCCESS`.
pub const ERRNO_SUCCESS: errno_t = 0;
/// PORT NOTE: `wasi.errno_t.INVAL`.
pub const ERRNO_INVAL: errno_t = 28;

/// PORT NOTE: `wasi.EVENTTYPE_CLOCK`.
pub const EVENTTYPE_CLOCK: eventtype_t = 0;
/// PORT NOTE: `wasi.EVENTTYPE_FD_READ`.
pub const EVENTTYPE_FD_READ: eventtype_t = 1;
/// PORT NOTE: `wasi.EVENTTYPE_FD_WRITE`.
pub const EVENTTYPE_FD_WRITE: eventtype_t = 2;

/// PORT NOTE: `wasi.SUBSCRIPTION_CLOCK_ABSTIME`.
pub const SUBSCRIPTION_CLOCK_ABSTIME: subclockflags_t = 0x0001;

/// PORT NOTE: `posix.CLOCK.MONOTONIC` on WASI.
pub const CLOCK_MONOTONIC: clockid_t = 1;
/// PORT NOTE: `posix.CLOCK.REALTIME` on WASI.
pub const CLOCK_REALTIME: clockid_t = 0;

/// PORT NOTE: `wasi.SHUT.RD`.
pub const SHUT_RD: sdflags_t = 0x01;
/// PORT NOTE: `wasi.SHUT.WR`.
pub const SHUT_WR: sdflags_t = 0x02;

/// PORT NOTE: `wasi.subscription_clock_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct subscription_clock_t {
    pub id: clockid_t,
    pub timeout: timestamp_t,
    pub precision: timestamp_t,
    pub flags: subclockflags_t,
}

/// PORT NOTE: `wasi.subscription_fd_readwrite_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct subscription_fd_readwrite_t {
    pub fd: fd_t,
}

/// PORT NOTE: `wasi.subscription_u_u_t` (the untagged union).
#[repr(C)]
#[derive(Clone, Copy)]
pub union subscription_u_u_t {
    pub clock: subscription_clock_t,
    pub fd_read: subscription_fd_readwrite_t,
    pub fd_write: subscription_fd_readwrite_t,
}

/// PORT NOTE: `wasi.subscription_u_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct subscription_u_t {
    pub tag: eventtype_t,
    pub u: subscription_u_u_t,
}

/// PORT NOTE: `wasi.subscription_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct subscription_t {
    pub userdata: userdata_t,
    pub u: subscription_u_t,
}

/// PORT NOTE: `wasi.event_fd_readwrite_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct event_fd_readwrite_t {
    pub nbytes: filesize_t,
    pub flags: eventrwflags_t,
}

/// PORT NOTE: `wasi.event_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct event_t {
    pub userdata: userdata_t,
    /// PORT NOTE: Zig spells this `@"error"`; `error` is a Rust keyword too, so
    /// the field keeps the ABI slot under the closest legal spelling.
    pub error_: errno_t,
    pub type_: eventtype_t,
    pub fd_readwrite: event_fd_readwrite_t,
}

/// PORT NOTE: `posix.iovec` on WASI (`sock_recv`'s scatter vector).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct iovec {
    pub base: *mut u8,
    pub len: usize,
}

/// PORT NOTE: `posix.iovec_const` on WASI (`sock_send`'s gather vector).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct iovec_const {
    pub base: *const u8,
    pub len: usize,
}

// PORT NOTE: the `wasi_snapshot_preview1` imports themselves. Zig's
// `std.os.wasi` declares these with `callconv(.C)` against the same import
// module; these are the identical ABI signatures.
#[cfg(target_os = "wasi")]
#[link(wasm_import_module = "wasi_snapshot_preview1")]
extern "C" {
    /// `wasi.poll_oneoff(in, out, nsubscriptions, nevents) errno_t`
    pub fn poll_oneoff(
        in_: *const subscription_t,
        out: *mut event_t,
        nsubscriptions: usize,
        nevents: *mut usize,
    ) -> errno_t;

    /// `wasi.clock_time_get(clock_id, precision, timestamp) errno_t`
    pub fn clock_time_get(
        clock_id: clockid_t,
        precision: timestamp_t,
        timestamp: *mut timestamp_t,
    ) -> errno_t;

    /// `wasi.sock_accept(fd, flags, result_fd) errno_t`
    pub fn sock_accept(fd: fd_t, flags: u16, result_fd: *mut fd_t) -> errno_t;

    /// `wasi.sock_recv(sock, ri_data, ri_data_len, ri_flags, ro_datalen, ro_flags) errno_t`
    pub fn sock_recv(
        sock: fd_t,
        ri_data: *const iovec,
        ri_data_len: usize,
        ri_flags: riflags_t,
        ro_datalen: *mut usize,
        ro_flags: *mut roflags_t,
    ) -> errno_t;

    /// `wasi.sock_send(sock, si_data, si_data_len, si_flags, so_datalen) errno_t`
    pub fn sock_send(
        sock: fd_t,
        si_data: *const iovec_const,
        si_data_len: usize,
        si_flags: siflags_t,
        so_datalen: *mut usize,
    ) -> errno_t;

    /// `wasi.sock_shutdown(sock, how) errno_t`
    pub fn sock_shutdown(sock: fd_t, how: sdflags_t) -> errno_t;
}
