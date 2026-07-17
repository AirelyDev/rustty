//! io_uring.rs — faithful port of libxev `backend/io_uring.zig` (XEV_BLUEPRINT §1
//! Linux port, task L1 LT5).
//!
//! Backend to use io_uring. This is a SELF-CONTAINED whole backend, parallel to
//! `epoll.rs` (and the macOS `kqueue.rs`): it defines its own `Loop`/`Completion`/
//! `Operation`/`Result`/`Callback` locally and builds ONLY on the shared
//! cross-platform data structure (`queue`) and the Linux ABI enums
//! (`linux::{RunMode, CallbackAction, CompletionState, Options}`) + `linux::net`.
//! It never imports from any macOS-gated file (R5).
//
// PORT NOTE (THE MODEL — why this file looks nothing like epoll.rs): io_uring is
// SUBMISSION-based, not readiness-based. There is no "register an fd, get told
// when it's ready" step: you fill a Submission Queue Entry (SQE) describing the
// whole operation, hand the ring to the kernel with one `io_uring_enter`, and the
// kernel posts a Completion Queue Entry (CQE) carrying the operation's RESULT
// (`res`, a negative errno on failure) and your `user_data`. Consequences that
// shape this port vs. epoll.rs:
//  - There is NO `perform` step. epoll's `start` registers and `perform` runs the
//    syscall when the fd fires; here `add_` fills the SQE and `invoke` merely
//    DECODES `cqe.res` into a `Result`. All the syscall work happened in-kernel.
//  - There is NO timer heap and NO wakeup eventfd. epoll derives timeouts from a
//    pairing heap and needs an eventfd to break `epoll_wait`; io_uring expresses a
//    timer AS AN OPERATION (`IORING_OP_TIMEOUT` with `IORING_TIMEOUT_ABS`), so the
//    kernel owns the timer and `io_uring_enter` returns when it fires. Hence this
//    `Loop` carries neither `heap::Intrusive` nor `linux::eventfd::Eventfd`
//    (the reference carries neither either).
//  - `submissions` means something DIFFERENT here. In epoll it is the queue of
//    work to register on the next tick; in io_uring it is the OVERFLOW queue of
//    completions that could not get an SQE because the ring was full.
//
// PORT NOTE (R3 — the ring is hand-rolled over raw libc syscalls): the reference
// rides Zig's `std.os.linux.IoUring`, itself a thin wrapper over
// `io_uring_setup`/`io_uring_enter` + three `mmap`s. `libc` 0.2 exposes ONLY the
// syscall numbers (`SYS_io_uring_setup`/`SYS_io_uring_enter`) — it carries NO
// `io_uring_params`/`io_uring_sqe`/`io_uring_cqe` struct and NO `IORING_*`
// constant (verified against libc 0.2.186) — so the whole ABI is hand-declared
// below, per the brief's "hand-declare only what libc lacks". No `io-uring`/`nix`/
// `mio` crate is added; the crate's dep set stays `{rustty-global, libc}`.
//
// PORT NOTE (the load-bearing disciplines, mirrored from epoll.rs / kqueue.rs §2):
//  - RAW POINTERS end-to-end: the `Loop` holds `*mut Completion` into caller
//    memory; callbacks get `*mut Loop` + `*mut Completion`; links are NEVER
//    `&mut`/lifetimes (Stacked-Borrows UB). A callback may legally re-add / rearm
//    its completion — exactly libxev's contract.
//  - `unsafe impl Send` for `Loop`/`Completion` (documented at each impl).
//  - hand-written `Default` for `Completion`: fn-ptr + raw-ptr fields.
//  - the `const _: () = assert!(size_of::<Completion>() == N)` layout tripwire.
//
// MIRI_EXCEPTIONS: this whole module is an isolated-Miri EXCEPTION and cannot be
// run under Miri at all. The ring's SQ/CQ live in `mmap`'d, KERNEL-SHARED memory
// obtained from `io_uring_setup` + `mmap(..., MAP_SHARED, ring_fd, IORING_OFF_*)`;
// Miri implements neither `mmap` of a device fd nor the `io_uring_setup`/
// `io_uring_enter` syscalls, and it cannot model the kernel as a concurrent writer
// of the CQ tail / SQ head. RAW ALIASING NOTE: every head/tail index is touched
// ONLY through a raw `*mut u32` reborrowed per-access via `AtomicU32::from_ptr`
// (never a long-lived `&mut`), so the kernel's concurrent stores never race a Rust
// reference; the `sqes`/`cqes`/`array` regions are likewise reached by raw-pointer
// arithmetic off the mmap base and are never covered by a Rust reference. The
// pure data structures this backend rides (`queue`) remain Miri-clean on their own
// tests. See also the `Sq`/`Cq` SAFETY notes.
#![allow(dead_code)]
// PORT NOTE: `ms_per_s`/`ns_per_ms`/`ns_per_s`/`fd_t` keep their Zig/`std.posix`
// snake_case identifiers (R2 identifier-fidelity), the `io_uring_*`/`IORING_*` ABI
// decls keep their KERNEL spelling, and `ReadError::EOF` keeps the reference's
// acronym spelling — mirroring epoll.rs/kqueue.rs's module-level allows.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(clippy::upper_case_acronyms)]

use core::mem::size_of;
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::linux::net;
use crate::linux::{CallbackAction, CompletionState, Options, RunMode};
use crate::queue;

// PORT NOTE: `posix.fd_t` — the C file descriptor int.
pub type fd_t = libc::c_int;
/// PORT NOTE: `posix.socket_t` — the socket handle (a C int on Linux).
pub type socket_t = libc::c_int;

/// PORT NOTE: `posix.SOCK.CLOEXEC` — the accept-op `flags` default.
pub const SOCK_CLOEXEC: u32 = libc::SOCK_CLOEXEC as u32;

/// PORT NOTE: `posix.timespec` — the monotonic-clock instant the loop caches.
pub type Timespec = libc::timespec;

// PORT NOTE: `std.time.*` constants used by `now`/`update_now`/timer math.
const ms_per_s: u64 = 1_000;
const ns_per_ms: u64 = 1_000_000;
const ns_per_s: u64 = 1_000_000_000;

// PORT NOTE: `const errno` reader — `posix.errno(rc)` reads the C `errno` global.
#[inline]
fn errno() -> i32 {
    // SAFETY: `__errno_location()` returns the thread-local errno location.
    unsafe { *libc::__errno_location() }
}

// ===========================================================================
// The io_uring kernel ABI (hand-declared — `libc` carries none of it, R3).
//
// These mirror `std.os.linux`'s io_uring decls, which are themselves verbatim
// ports of the kernel's `include/uapi/linux/io_uring.h`. Field names/order/types
// are the kernel's; the layout is load-bearing (the kernel reads/writes these
// structs directly), hence `#[repr(C)]` + the size tripwires below.
// ===========================================================================

/// PORT NOTE: kernel `struct io_sqring_offsets` — where, inside the SQ-ring mmap,
/// each SQ index/field lives (byte offsets from the mmap base).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct io_sqring_offsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    flags: u32,
    dropped: u32,
    array: u32,
    resv1: u32,
    resv2: u64,
}

/// PORT NOTE: kernel `struct io_cqring_offsets` — where, inside the CQ-ring mmap,
/// each CQ index/field lives (byte offsets from the mmap base).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct io_cqring_offsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    overflow: u32,
    cqes: u32,
    flags: u32,
    resv1: u32,
    resv2: u64,
}

/// PORT NOTE: kernel `struct io_uring_params` — the in/out parameter block of
/// `io_uring_setup`. We pass `flags`; the kernel fills `sq_entries`/`cq_entries`
/// (rounded up to a power of two), `features`, and both offset blocks.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct io_uring_params {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off: io_sqring_offsets,
    cq_off: io_cqring_offsets,
}

/// PORT NOTE: kernel `struct io_uring_sqe` — ONE submission. The kernel's many
/// anonymous unions are flattened to the widest member exactly as
/// `std.os.linux.io_uring_sqe` does: `off` covers `off`/`addr2`, `addr` covers
/// `addr`/`splice_off_in`, and `rw_flags` covers the whole `*_flags` union
/// (`timeout_flags`/`poll32_events`/`accept_flags`/`msg_flags`/`cancel_flags`/...).
/// 64 bytes — asserted below.
#[repr(C)]
#[derive(Clone, Copy)]
struct io_uring_sqe {
    opcode: u8,
    flags: u8,
    ioprio: u16,
    fd: i32,
    off: u64,
    addr: u64,
    len: u32,
    rw_flags: u32,
    user_data: u64,
    buf_index: u16,
    personality: u16,
    splice_fd_in: i32,
    addr3: u64,
    resv: u64,
}

/// PORT NOTE: kernel `struct io_uring_cqe` — ONE completion. `user_data` is echoed
/// back verbatim from the SQE (we always store the `*mut Completion` there); `res`
/// is the operation's return value — `>= 0` on success, `-errno` on failure.
/// 16 bytes — asserted below.
#[repr(C)]
#[derive(Clone, Copy)]
struct io_uring_cqe {
    user_data: u64,
    res: i32,
    flags: u32,
}

// PORT NOTE: the kernel ABI layout is load-bearing (these structs are read/written
// by the kernel). Trip at compile time on any drift.
const _: () = assert!(size_of::<io_uring_sqe>() == 64);
const _: () = assert!(size_of::<io_uring_cqe>() == 16);
const _: () = assert!(size_of::<io_uring_params>() == 120);

// PORT NOTE: `linux.IORING_OFF_*` — the magic `mmap` offsets that select which
// ring region the ring fd maps.
const IORING_OFF_SQ_RING: i64 = 0;
const IORING_OFF_CQ_RING: i64 = 0x0800_0000;
const IORING_OFF_SQES: i64 = 0x1000_0000;

// PORT NOTE: `linux.IORING_SETUP_*` — `io_uring_setup` flags. We pass 0 (the
// reference's `linux.IoUring.init(entries, 0)`); these are named because
// `submit_and_wait`/`sq_ring_needs_enter` branch on them.
const IORING_SETUP_IOPOLL: u32 = 1 << 0;
const IORING_SETUP_SQPOLL: u32 = 1 << 1;
const IORING_SETUP_CQSIZE: u32 = 1 << 3;

// PORT NOTE: `linux.IORING_SQ_*` — kernel→user flags in the SQ `flags` word.
const IORING_SQ_NEED_WAKEUP: u32 = 1 << 0;
const IORING_SQ_CQ_OVERFLOW: u32 = 1 << 1;

// PORT NOTE: `linux.IORING_ENTER_*` — `io_uring_enter` flags.
const IORING_ENTER_GETEVENTS: u32 = 1 << 0;
const IORING_ENTER_SQ_WAKEUP: u32 = 1 << 1;

/// PORT NOTE: `linux.IORING_FEAT_SINGLE_MMAP` — set by the kernel (>= 5.4) when the
/// SQ and CQ rings share ONE mapping. `std.os.linux.IoUring` asserts this and so do
/// we (see `Sq::init`).
const IORING_FEAT_SINGLE_MMAP: u32 = 1 << 0;

/// PORT NOTE: `linux.IORING_TIMEOUT_ABS` — the `timeout_flags` bit that makes an
/// `IORING_OP_TIMEOUT`'s timespec an ABSOLUTE `CLOCK_MONOTONIC` deadline rather
/// than a relative duration. This is what lets the loop's `cached_now + next_ms`
/// deadline (`timer_next`) be handed straight to the kernel.
const IORING_TIMEOUT_ABS: u32 = 1 << 0;

// PORT NOTE: `linux.IORING_OP` — the opcode enum. Only the opcodes this backend's
// ported `Operation` surface preps are named (the kernel numbering is dense and
// stable; the deferred pread/pwrite/sendmsg/recvmsg ops reuse READ/WRITE/SENDMSG/
// RECVMSG when Task 6+ lands them).
const IORING_OP_NOP: u8 = 0;
const IORING_OP_POLL_ADD: u8 = 6;
const IORING_OP_SENDMSG: u8 = 9;
const IORING_OP_RECVMSG: u8 = 10;
const IORING_OP_TIMEOUT: u8 = 11;
const IORING_OP_TIMEOUT_REMOVE: u8 = 12;
const IORING_OP_ACCEPT: u8 = 13;
const IORING_OP_ASYNC_CANCEL: u8 = 14;
const IORING_OP_CONNECT: u8 = 16;
const IORING_OP_CLOSE: u8 = 19;
const IORING_OP_READ: u8 = 22;
const IORING_OP_WRITE: u8 = 23;
const IORING_OP_SEND: u8 = 26;
const IORING_OP_RECV: u8 = 27;
const IORING_OP_SHUTDOWN: u8 = 34;

/// PORT NOTE: `linux.kernel_timespec` — the kernel's `struct __kernel_timespec`
/// (`{ i64, i64 }` on every arch, unlike `posix.timespec`). This is the type
/// `IORING_OP_TIMEOUT` reads through the SQE's `addr`.
//
// PORT NOTE: Zig `.sec`/`.nsec` field names are preserved verbatim (R2).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KernelTimespec {
    pub sec: i64,
    pub nsec: i64,
}

// PORT NOTE: Zig's `linux.NSIG` — `io_uring_enter`'s `sigsz` argument is
// `NSIG / 8` when a sigset is passed. We always pass a NULL sigset (the kernel
// then ignores `sigsz`), but the value mirrors `std.os.linux.io_uring_enter`.
const NSIG: usize = 64;

/// PORT NOTE: `linux.io_uring_setup(entries, p)` — raw syscall. `libc::syscall`
/// returns -1/errno (rather than Zig's raw `-errno` return), so callers read
/// `errno()`.
///
/// # Safety
/// `p` must point to a valid, writable `io_uring_params`.
unsafe fn io_uring_setup(entries: u32, p: *mut io_uring_params) -> libc::c_long {
    // SAFETY: the kernel reads/writes `*p` (a valid params block) and returns a fd.
    unsafe { libc::syscall(libc::SYS_io_uring_setup, entries as libc::c_long, p) }
}

/// PORT NOTE: `linux.io_uring_enter(fd, to_submit, min_complete, flags, sig)` —
/// raw syscall. The kernel's `SYSCALL_DEFINE6(io_uring_enter, ...)` takes
/// `(fd, to_submit, min_complete, flags, argp, argsz)`; we always pass a NULL
/// `argp` (so `argsz` is ignored) exactly as `std.os.linux.io_uring_enter` does.
///
/// # Safety
/// `fd` must be a live io_uring ring fd.
unsafe fn io_uring_enter(fd: fd_t, to_submit: u32, min_complete: u32, flags: u32) -> libc::c_long {
    // SAFETY: a live ring fd + a NULL sigset; the kernel consumes SQEs and posts CQEs.
    unsafe {
        libc::syscall(
            libc::SYS_io_uring_enter,
            fd as libc::c_long,
            to_submit as libc::c_long,
            min_complete as libc::c_long,
            flags as libc::c_long,
            ptr::null::<libc::c_void>(),
            (NSIG / 8) as libc::c_long,
        )
    }
}

// ---------------------------------------------------------------------------
// Ring index atomics.
//
// PORT NOTE: the SQ/CQ head+tail live in memory SHARED WITH THE KERNEL, which
// writes `sq.head`/`cq.tail`/`sq.flags` concurrently with us. Each access
// reborrows the raw pointer for the duration of the single atomic op via
// `AtomicU32::from_ptr` — we never hold a `&mut u32` across a syscall, so the
// kernel's stores can never race a live Rust reference (see MIRI_EXCEPTIONS).
// The orderings mirror `std.os.linux.IoUring`'s `@atomicLoad`/`@atomicStore`:
//  - `sq.head` / `cq.tail`: the KERNEL is the writer → Acquire load.
//  - `sq.tail` / `cq.head`: WE are the writer → Release store (publishes the SQEs
//    / the consumed CQEs), Relaxed load (nobody else writes them).
//  - `sq.flags`: kernel-written hint → Relaxed (Zig's `.unordered`).
// ---------------------------------------------------------------------------

/// # Safety
/// `p` must be a valid, aligned, live `u32` in the ring mmap.
#[inline]
unsafe fn atomic_load(p: *mut u32, order: Ordering) -> u32 {
    // SAFETY: caller guarantees `p` is a valid aligned live u32 for this access.
    unsafe { AtomicU32::from_ptr(p) }.load(order)
}

/// # Safety
/// `p` must be a valid, aligned, live `u32` in the ring mmap.
#[inline]
unsafe fn atomic_store(p: *mut u32, v: u32, order: Ordering) {
    // SAFETY: caller guarantees `p` is a valid aligned live u32 for this access.
    unsafe { AtomicU32::from_ptr(p) }.store(v, order)
}

// ===========================================================================
// The ring: `IoUring` — the faithful subset of `std.os.linux.IoUring` the
// reference uses (`init`/`get_sqe`/`submit`/`submit_and_wait`/`copy_cqes`/
// `deinit`), hand-rolled per R3.
// ===========================================================================

/// PORT NOTE: `std.os.linux.IoUring.SubmissionQueue`.
//
// SAFETY/OWNERSHIP: every pointer here is an interior pointer into `mmap`
// (the SQ-ring mapping) or `mmap_sqes` (the SQE-array mapping), both owned by this
// struct and unmapped in `deinit`. They are stable for the life of the ring: the
// mappings are never moved or resized. `array`/`sqes` are `[entries]`-long.
struct Sq {
    /// Kernel-written consumer index (bumped as the kernel eats SQEs).
    head: *mut u32,
    /// User-written producer index (published by `flush_sq`).
    tail: *mut u32,
    /// `entries - 1` — the power-of-two wrap mask.
    mask: u32,
    /// Kernel-written hint word (`IORING_SQ_*`).
    flags: *mut u32,
    /// Kernel-written count of SQEs dropped for being invalid.
    dropped: *mut u32,
    /// The SQ "array": the indirection table mapping ring slot → SQE index.
    array: *mut u32,
    /// The SQE array proper (`mmap_sqes`).
    sqes: *mut io_uring_sqe,
    /// `p.sq_entries` — the length of both `array` and `sqes`.
    entries: u32,

    mmap: *mut libc::c_void,
    mmap_len: usize,
    mmap_sqes: *mut libc::c_void,
    mmap_sqes_len: usize,

    // PORT NOTE (verbatim from the reference): We use `sqe_head` and `sqe_tail` in
    // the same way as liburing: We increment `sqe_tail` (but not `tail`) for each
    // call to `get_sqe()`. We then set `tail` to `sqe_tail` once, only when these
    // events are actually submitted. This allows us to amortize the cost of the
    // atomic store to `tail` across multiple SQEs.
    sqe_head: u32,
    sqe_tail: u32,
}

/// PORT NOTE: `std.os.linux.IoUring.CompletionQueue`.
//
// SAFETY/OWNERSHIP: with `IORING_FEAT_SINGLE_MMAP` the CQ shares the SQ's `mmap`,
// so this struct owns NO mapping of its own (its `deinit` is a no-op, exactly as
// the reference's is) — every pointer is an interior pointer into `Sq::mmap`.
struct Cq {
    /// User-written consumer index (published by `cq_advance`).
    head: *mut u32,
    /// Kernel-written producer index.
    tail: *mut u32,
    /// `cq_entries - 1` — the power-of-two wrap mask.
    mask: u32,
    /// Kernel-written count of CQEs dropped on overflow.
    overflow: *mut u32,
    /// The CQE array.
    cqes: *mut io_uring_cqe,
    /// `p.cq_entries` — the length of `cqes`.
    entries: u32,
}

/// PORT NOTE: `std.os.linux.IoUring` — the ring handle.
pub struct IoUring {
    fd: fd_t,
    sq: Sq,
    cq: Cq,
    flags: u32,
    features: u32,
}

/// PORT NOTE: the error set of `IoUring.init` (`std.os.linux.IoUring.init`'s
/// `io_uring_setup` + `mmap` errors). Collapsed to the arms the reference's
/// `Loop.init` can surface; `available()` maps ANY of them to "unavailable".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingInitError {
    EntriesZero,
    EntriesNotPowerOfTwo,
    ArgumentsInvalid,
    ProcessFdQuotaExceeded,
    SystemFdQuotaExceeded,
    SystemResources,
    PermissionDenied,
    /// The kernel has no `io_uring_setup` (ENOSYS) — io_uring is unavailable.
    SystemOutdated,
    /// The kernel is too old to report `IORING_FEAT_SINGLE_MMAP` (< 5.4).
    SingleMmapUnsupported,
    MmapFailed,
    Unexpected,
}

/// PORT NOTE: `IoUring.get_sqe`'s error set (`error{SubmissionQueueFull}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmissionQueueFull;

/// PORT NOTE: `Loop.SubmitError` (io_uring.zig:211) — verbatim. "The errors that
/// can come from submit. This has to be explicit for now since Zig 0.11 has issues
/// with the inferred error set. We should try again someday."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    Unexpected,
    SystemResources,
    FileDescriptorInvalid,
    FileDescriptorInBadState,
    CompletionQueueOvercommitted,
    SubmissionQueueEntryInvalid,
    BufferInvalid,
    RingShuttingDown,
    OpcodeNotSupported,
    SignalInterrupt,
}

impl IoUring {
    /// PORT NOTE: `IoUring.init(entries, flags)` → `init_params` with a zeroed
    /// `io_uring_params` carrying `flags` (+ the reference's `sq_thread_idle =
    /// 1000`, which only matters under SQPOLL).
    pub fn init(entries: u16, flags: u32) -> core::result::Result<IoUring, RingInitError> {
        let mut params = io_uring_params {
            flags,
            sq_thread_idle: 1000,
            ..io_uring_params::default()
        };
        IoUring::init_params(entries, &mut params)
    }

    /// PORT NOTE: `IoUring.init_params(entries, p)`.
    fn init_params(
        entries: u16,
        p: *mut io_uring_params,
    ) -> core::result::Result<IoUring, RingInitError> {
        if entries == 0 {
            return Err(RingInitError::EntriesZero);
        }
        if !entries.is_power_of_two() {
            return Err(RingInitError::EntriesNotPowerOfTwo);
        }

        // SAFETY: `p` is a valid params block; the kernel fills it and returns a fd.
        let res = unsafe { io_uring_setup(entries as u32, p) };
        if res < 0 {
            return Err(match errno() {
                libc::EFAULT => RingInitError::Unexpected,
                libc::EINVAL => RingInitError::ArgumentsInvalid,
                libc::EMFILE => RingInitError::ProcessFdQuotaExceeded,
                libc::ENFILE => RingInitError::SystemFdQuotaExceeded,
                libc::ENOMEM => RingInitError::SystemResources,
                // PORT NOTE: EPERM is what a hardened kernel returns when io_uring
                // is administratively disabled (`kernel.io_uring_disabled=2`), and
                // ENOSYS when the syscall isn't built in — both mean "unavailable"
                // to `available()`.
                libc::EPERM => RingInitError::PermissionDenied,
                libc::ENOSYS => RingInitError::SystemOutdated,
                _ => RingInitError::Unexpected,
            });
        }
        let fd = res as fd_t;

        // PORT NOTE: `assert((p.features & linux.IORING_FEAT_SINGLE_MMAP) != 0);` —
        // the reference (and liburing) require the >= 5.4 single-mapping layout.
        // We return an error rather than assert so `available()` can degrade
        // gracefully on an ancient kernel instead of panicking.
        let features = unsafe { (*p).features };
        if features & IORING_FEAT_SINGLE_MMAP == 0 {
            // SAFETY: closing the ring fd we just opened.
            unsafe { libc::close(fd) };
            return Err(RingInitError::SingleMmapUnsupported);
        }

        let sq = match unsafe { Sq::init(fd, p) } {
            Ok(v) => v,
            Err(e) => {
                // errdefer posix.close(fd);
                unsafe { libc::close(fd) };
                return Err(e);
            }
        };
        // SAFETY: `sq` is initialized and shares its mmap with the CQ (SINGLE_MMAP).
        let cq = unsafe { Cq::init(p, &sq) };

        let flags = unsafe { (*p).flags };
        Ok(IoUring {
            fd,
            sq,
            cq,
            flags,
            features,
        })
    }

    /// PORT NOTE: `IoUring.deinit`. "The mmaps depend on the fd, so the order of
    /// these calls is important" — CQ (no-op under SINGLE_MMAP), then SQ, then the fd.
    pub fn deinit(&mut self) {
        debug_assert!(self.fd >= 0);
        // self.cq.deinit(); — a no-op: the CQ rides the SQ's mmap (SINGLE_MMAP).
        // SAFETY: unmapping mappings this ring owns, then closing our own fd.
        unsafe { self.sq.deinit() };
        unsafe { libc::close(self.fd) };
        self.fd = -1;
    }

    /// PORT NOTE: `IoUring.get_sqe` — hand out the next SQE, bumping only the
    /// USERSPACE `sqe_tail` (the kernel-visible `tail` is published later by
    /// `flush_sq`). Returns `SubmissionQueueFull` when the ring has no free slot.
    fn get_sqe(&mut self) -> core::result::Result<*mut io_uring_sqe, SubmissionQueueFull> {
        // SAFETY: `sq.head` is a live u32 in the ring mmap (kernel-written).
        let head = unsafe { atomic_load(self.sq.head, Ordering::Acquire) };
        let next = self.sq.sqe_tail.wrapping_add(1);
        if next.wrapping_sub(head) > self.sq.entries {
            return Err(SubmissionQueueFull);
        }
        // SAFETY: `sqe_tail & mask` is in `0..entries`, within the sqes mapping.
        let sqe = unsafe { self.sq.sqes.add((self.sq.sqe_tail & self.sq.mask) as usize) };
        self.sq.sqe_tail = next;
        Ok(sqe)
    }

    /// PORT NOTE: `IoUring.flush_sq` — publish every SQE handed out since the last
    /// flush by writing its index into the SQ `array` and Release-storing `tail`.
    fn flush_sq(&mut self) -> u32 {
        if self.sq.sqe_head != self.sq.sqe_tail {
            let to_submit = self.sq.sqe_tail.wrapping_sub(self.sq.sqe_head);
            // SAFETY: `sq.tail` is a live u32 in the ring mmap; we are its only writer.
            let mut tail = unsafe { atomic_load(self.sq.tail, Ordering::Relaxed) };
            let mut i: u32 = 0;
            while i < to_submit {
                // SAFETY: `tail & mask` is in `0..entries`, within the array region.
                unsafe {
                    self.sq
                        .array
                        .add((tail & self.sq.mask) as usize)
                        .write(self.sq.sqe_head & self.sq.mask)
                };
                tail = tail.wrapping_add(1);
                self.sq.sqe_head = self.sq.sqe_head.wrapping_add(1);
                i += 1;
            }
            // The Release store publishes the `array` writes (and the SQEs they
            // point at) to the kernel.
            // SAFETY: `sq.tail` is a live u32 in the ring mmap.
            unsafe { atomic_store(self.sq.tail, tail, Ordering::Release) };
        }
        self.sq_ready()
    }

    /// PORT NOTE: `IoUring.sq_ready` — SQEs submitted-but-not-yet-consumed.
    fn sq_ready(&self) -> u32 {
        // SAFETY: `sq.head` is a live u32 in the ring mmap (kernel-written).
        self.sq
            .sqe_tail
            .wrapping_sub(unsafe { atomic_load(self.sq.head, Ordering::Acquire) })
    }

    /// PORT NOTE: `IoUring.cq_ready` — CQEs posted-but-not-yet-consumed.
    fn cq_ready(&self) -> u32 {
        // SAFETY: both are live u32s in the ring mmap; `cq.tail` is kernel-written.
        unsafe {
            atomic_load(self.cq.tail, Ordering::Acquire)
                .wrapping_sub(atomic_load(self.cq.head, Ordering::Relaxed))
        }
    }

    /// PORT NOTE: `IoUring.sq_ring_needs_enter`. Without SQPOLL (our case — the
    /// reference passes `flags = 0`) this is unconditionally true.
    fn sq_ring_needs_enter(&self, flags: &mut u32) -> bool {
        debug_assert!(*flags == 0);
        if self.flags & IORING_SETUP_SQPOLL == 0 {
            return true;
        }
        // SAFETY: `sq.flags` is a live u32 in the ring mmap (kernel-written).
        if unsafe { atomic_load(self.sq.flags, Ordering::Relaxed) } & IORING_SQ_NEED_WAKEUP != 0 {
            *flags |= IORING_ENTER_SQ_WAKEUP;
            return true;
        }
        false
    }

    /// PORT NOTE: `IoUring.cq_ring_needs_flush`.
    fn cq_ring_needs_flush(&self) -> bool {
        // SAFETY: `sq.flags` is a live u32 in the ring mmap (kernel-written).
        let flags = unsafe { atomic_load(self.sq.flags, Ordering::Relaxed) };
        flags & IORING_SQ_CQ_OVERFLOW != 0
    }

    /// PORT NOTE: `IoUring.cq_advance` — hand consumed CQE slots back to the kernel.
    fn cq_advance(&mut self, count: u32) {
        if count > 0 {
            // SAFETY: `cq.head` is a live u32 in the ring mmap; we are its only writer.
            unsafe {
                let head = atomic_load(self.cq.head, Ordering::Relaxed);
                atomic_store(self.cq.head, head.wrapping_add(count), Ordering::Release)
            };
        }
    }

    /// PORT NOTE: `IoUring.enter` — THE syscall. Hands `to_submit` SQEs to the
    /// kernel and (with `IORING_ENTER_GETEVENTS`) waits for `min_complete` CQEs.
    fn enter(
        &mut self,
        to_submit: u32,
        min_complete: u32,
        flags: u32,
    ) -> core::result::Result<u32, SubmitError> {
        debug_assert!(self.fd >= 0);
        // SAFETY: a live ring fd; the kernel consumes the published SQEs.
        let res = unsafe { io_uring_enter(self.fd, to_submit, min_complete, flags) };
        if res < 0 {
            return Err(match errno() {
                libc::EAGAIN => SubmitError::SystemResources,
                libc::EBADF => SubmitError::FileDescriptorInvalid,
                libc::EBADFD => SubmitError::FileDescriptorInBadState,
                libc::EBUSY => SubmitError::CompletionQueueOvercommitted,
                libc::EINVAL => SubmitError::SubmissionQueueEntryInvalid,
                libc::EFAULT => SubmitError::BufferInvalid,
                libc::ENXIO => SubmitError::RingShuttingDown,
                libc::EOPNOTSUPP => SubmitError::OpcodeNotSupported,
                // EINTR means our blocking syscall was interrupted by some process
                // signal. Callers retry. See signal(7).
                libc::EINTR => SubmitError::SignalInterrupt,
                _ => SubmitError::Unexpected,
            });
        }
        Ok(res as u32)
    }

    /// PORT NOTE: `IoUring.submit` — flush + enter, never waiting.
    fn submit(&mut self) -> core::result::Result<u32, SubmitError> {
        self.submit_and_wait(0)
    }

    /// PORT NOTE: `IoUring.submit_and_wait` — flush + enter, waiting for `wait_nr`
    /// completions. THIS is the one-syscall fast path the loop's tick rides.
    fn submit_and_wait(&mut self, wait_nr: u32) -> core::result::Result<u32, SubmitError> {
        let submitted = self.flush_sq();
        let mut flags: u32 = 0;
        if self.sq_ring_needs_enter(&mut flags) || wait_nr > 0 {
            if wait_nr > 0 || (self.flags & IORING_SETUP_IOPOLL) != 0 {
                flags |= IORING_ENTER_GETEVENTS;
            }
            return self.enter(submitted, wait_nr, flags);
        }
        Ok(submitted)
    }

    /// PORT NOTE: `IoUring.copy_cqes` — drain up to `cqes.len()` completions,
    /// entering the kernel to wait if none are ready and `wait_nr > 0`.
    fn copy_cqes(
        &mut self,
        cqes: &mut [io_uring_cqe],
        wait_nr: u32,
    ) -> core::result::Result<u32, SubmitError> {
        let count = self.copy_cqes_ready(cqes);
        if count > 0 {
            return Ok(count);
        }
        if self.cq_ring_needs_flush() || wait_nr > 0 {
            self.enter(0, wait_nr, IORING_ENTER_GETEVENTS)?;
            return Ok(self.copy_cqes_ready(cqes));
        }
        Ok(0)
    }

    /// PORT NOTE: `IoUring.copy_cqes_ready` — memcpy the ready CQEs out of the
    /// kernel-shared ring (handling the wrap), then advance the consumer index.
    fn copy_cqes_ready(&mut self, cqes: &mut [io_uring_cqe]) -> u32 {
        let ready = self.cq_ready();
        let count = core::cmp::min(cqes.len() as u32, ready);
        // SAFETY: `cq.head` is a live u32 in the ring mmap; we are its only writer.
        let head = unsafe { atomic_load(self.cq.head, Ordering::Relaxed) } & self.cq.mask;

        // The CQEs before wrapping.
        let n = core::cmp::min(self.cq.entries - head, count);
        // SAFETY: `head..head+n` is within the cqes region; `cqes` has room for `n`.
        unsafe {
            ptr::copy_nonoverlapping(
                self.cq.cqes.add(head as usize),
                cqes.as_mut_ptr(),
                n as usize,
            )
        };

        if count > n {
            // The CQEs after wrapping.
            // SAFETY: `0..count-n` is within the cqes region; `cqes[n..count]` has room.
            unsafe {
                ptr::copy_nonoverlapping(
                    self.cq.cqes,
                    cqes.as_mut_ptr().add(n as usize),
                    (count - n) as usize,
                )
            };
        }

        self.cq_advance(count);
        count
    }
}

impl Sq {
    /// PORT NOTE: `SubmissionQueue.init(fd, p)` — the TWO mmaps. Under
    /// `IORING_FEAT_SINGLE_MMAP` the SQ-ring mapping is sized to cover the CQ ring
    /// too (hence the `max`), and the SQE array gets its own mapping.
    ///
    /// # Safety
    /// `fd` must be a live ring fd and `p` a kernel-filled params block.
    unsafe fn init(fd: fd_t, p: *mut io_uring_params) -> core::result::Result<Sq, RingInitError> {
        debug_assert!(fd >= 0);
        let p = unsafe { &*p };

        let size = core::cmp::max(
            p.sq_off.array as usize + p.sq_entries as usize * size_of::<u32>(),
            p.cq_off.cqes as usize + p.cq_entries as usize * size_of::<io_uring_cqe>(),
        );

        // SAFETY: mapping the ring fd's SQ-ring region; the kernel owns the pages.
        let mmap = unsafe {
            libc::mmap(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_POPULATE,
                fd,
                IORING_OFF_SQ_RING,
            )
        };
        if mmap == libc::MAP_FAILED {
            return Err(RingInitError::MmapFailed);
        }

        // PORT NOTE (verbatim): The motivation for the `sqes` and `array`
        // indirection is to make it possible for the application to preallocate
        // static io_uring_sqe entries and then replay them when needed.
        let size_sqes = p.sq_entries as usize * size_of::<io_uring_sqe>();
        // SAFETY: mapping the ring fd's SQE-array region.
        let mmap_sqes = unsafe {
            libc::mmap(
                ptr::null_mut(),
                size_sqes,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_POPULATE,
                fd,
                IORING_OFF_SQES,
            )
        };
        if mmap_sqes == libc::MAP_FAILED {
            // errdefer posix.munmap(mmap);
            unsafe { libc::munmap(mmap, size) };
            return Err(RingInitError::MmapFailed);
        }

        // SAFETY: every offset below is a kernel-reported byte offset into the
        // freshly-mapped `mmap`, naturally aligned by the ring ABI.
        let base = mmap.cast::<u8>();
        let sq = unsafe {
            Sq {
                head: base.add(p.sq_off.head as usize).cast::<u32>(),
                tail: base.add(p.sq_off.tail as usize).cast::<u32>(),
                mask: *base.add(p.sq_off.ring_mask as usize).cast::<u32>(),
                flags: base.add(p.sq_off.flags as usize).cast::<u32>(),
                dropped: base.add(p.sq_off.dropped as usize).cast::<u32>(),
                array: base.add(p.sq_off.array as usize).cast::<u32>(),
                sqes: mmap_sqes.cast::<io_uring_sqe>(),
                entries: p.sq_entries,
                mmap,
                mmap_len: size,
                mmap_sqes,
                mmap_sqes_len: size_sqes,
                sqe_head: 0,
                sqe_tail: 0,
            }
        };

        // assert(p.sq_entries == mmap[p.sq_off.ring_entries]);
        debug_assert_eq!(p.sq_entries, unsafe {
            *base.add(p.sq_off.ring_entries as usize).cast::<u32>()
        });

        Ok(sq)
    }

    /// PORT NOTE: `SubmissionQueue.deinit` — unmap both mappings.
    ///
    /// # Safety
    /// Must be called at most once, and only while the ring fd is still open.
    unsafe fn deinit(&mut self) {
        // SAFETY: unmapping the two mappings this struct owns.
        unsafe {
            libc::munmap(self.mmap, self.mmap_len);
            libc::munmap(self.mmap_sqes, self.mmap_sqes_len);
        }
    }
}

impl Cq {
    /// PORT NOTE: `CompletionQueue.init(fd, p, sq)` — NO mmap of its own: under
    /// `IORING_FEAT_SINGLE_MMAP` the CQ rides the SQ's mapping (the reference
    /// asserts the same).
    ///
    /// # Safety
    /// `p` must be a kernel-filled params block and `sq` an initialized `Sq`.
    unsafe fn init(p: *mut io_uring_params, sq: &Sq) -> Cq {
        let p = unsafe { &*p };
        let base = sq.mmap.cast::<u8>();

        // SAFETY: every offset is a kernel-reported byte offset into `sq.mmap`,
        // whose size already covers the CQ region (see `Sq::init`'s `max`).
        let cq = unsafe {
            Cq {
                head: base.add(p.cq_off.head as usize).cast::<u32>(),
                tail: base.add(p.cq_off.tail as usize).cast::<u32>(),
                mask: *base.add(p.cq_off.ring_mask as usize).cast::<u32>(),
                overflow: base.add(p.cq_off.overflow as usize).cast::<u32>(),
                cqes: base.add(p.cq_off.cqes as usize).cast::<io_uring_cqe>(),
                entries: p.cq_entries,
            }
        };

        // assert(p.cq_entries == mmap[p.cq_off.ring_entries]);
        debug_assert_eq!(p.cq_entries, unsafe {
            *base.add(p.cq_off.ring_entries as usize).cast::<u32>()
        });

        cq
    }
}

// ---------------------------------------------------------------------------
// SQE prep helpers — `std.os.linux.io_uring_sqe`'s `prep_*` methods.
//
// PORT NOTE: every `prep_*` FULLY overwrites the SQE (the reference's `prep_rw`
// assigns `sqe.* = .{...}`), which is why `add_` must re-set `user_data` AFTER
// prepping ("The prep functions above reset the user data so we have to do this
// here").
// ---------------------------------------------------------------------------

/// PORT NOTE: `io_uring_sqe.prep_rw(op, fd, addr, len, offset)`.
///
/// # Safety
/// `sqe` must point at a live SQE slot in the ring's SQE mapping.
unsafe fn prep_rw(sqe: *mut io_uring_sqe, op: u8, fd: i32, addr: u64, len: u32, offset: u64) {
    // SAFETY: `sqe` is a live, writable SQE slot.
    unsafe {
        sqe.write(io_uring_sqe {
            opcode: op,
            flags: 0,
            ioprio: 0,
            fd,
            off: offset,
            addr,
            len,
            rw_flags: 0,
            user_data: 0,
            buf_index: 0,
            personality: 0,
            splice_fd_in: 0,
            addr3: 0,
            resv: 0,
        })
    };
}

/// PORT NOTE: `io_uring_sqe.prep_accept(fd, addr, addrlen, flags)`. The addr goes
/// in `addr`, the addrlen POINTER in `off` (the kernel writes both back).
///
/// # Safety
/// `sqe` live; `addr`/`addrlen` must stay valid until the op completes.
unsafe fn prep_accept(
    sqe: *mut io_uring_sqe,
    fd: fd_t,
    addr: *mut net::sockaddr::Any,
    addrlen: *mut net::socklen_t,
    flags: u32,
) {
    unsafe {
        prep_rw(sqe, IORING_OP_ACCEPT, fd, addr as u64, 0, addrlen as u64);
        (*sqe).rw_flags = flags;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_connect(fd, addr, addrlen)`. Unlike accept, the
/// addrlen is passed BY VALUE in `off`.
///
/// # Safety
/// `sqe` live; `addr` must stay valid until the op completes.
unsafe fn prep_connect(
    sqe: *mut io_uring_sqe,
    fd: fd_t,
    addr: *const net::sockaddr::Any,
    addrlen: net::socklen_t,
) {
    unsafe { prep_rw(sqe, IORING_OP_CONNECT, fd, addr as u64, 0, addrlen as u64) };
}

/// PORT NOTE: `io_uring_sqe.prep_poll_add(fd, poll_mask)` — oneshot poll; the mask
/// rides `poll32_events` (the `rw_flags` union member).
///
/// # Safety
/// `sqe` must point at a live SQE slot.
unsafe fn prep_poll_add(sqe: *mut io_uring_sqe, fd: fd_t, poll_mask: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_POLL_ADD, fd, 0, 0, 0);
        (*sqe).rw_flags = poll_mask;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_read(fd, buffer, offset)`.
///
/// # Safety
/// `sqe` live; `buf[..len]` must stay valid+writable until the op completes.
unsafe fn prep_read(sqe: *mut io_uring_sqe, fd: fd_t, buf: *mut u8, len: usize, offset: u64) {
    unsafe { prep_rw(sqe, IORING_OP_READ, fd, buf as u64, len as u32, offset) };
}

/// PORT NOTE: `io_uring_sqe.prep_write(fd, buffer, offset)`.
///
/// # Safety
/// `sqe` live; `buf[..len]` must stay valid until the op completes.
unsafe fn prep_write(sqe: *mut io_uring_sqe, fd: fd_t, buf: *const u8, len: usize, offset: u64) {
    unsafe { prep_rw(sqe, IORING_OP_WRITE, fd, buf as u64, len as u32, offset) };
}

/// PORT NOTE: `io_uring_sqe.prep_send(fd, buffer, flags)`.
///
/// # Safety
/// `sqe` live; `buf[..len]` must stay valid until the op completes.
unsafe fn prep_send(sqe: *mut io_uring_sqe, fd: fd_t, buf: *const u8, len: usize, flags: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_SEND, fd, buf as u64, len as u32, 0);
        (*sqe).rw_flags = flags;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_recv(fd, buffer, flags)`.
///
/// # Safety
/// `sqe` live; `buf[..len]` must stay valid+writable until the op completes.
unsafe fn prep_recv(sqe: *mut io_uring_sqe, fd: fd_t, buf: *mut u8, len: usize, flags: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_RECV, fd, buf as u64, len as u32, 0);
        (*sqe).rw_flags = flags;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_close(fd)`.
///
/// # Safety
/// `sqe` must point at a live SQE slot.
unsafe fn prep_close(sqe: *mut io_uring_sqe, fd: fd_t) {
    unsafe { prep_rw(sqe, IORING_OP_CLOSE, fd, 0, 0, 0) };
}

/// PORT NOTE: `io_uring_sqe.prep_shutdown(sockfd, how)` — `how` rides `len`.
///
/// # Safety
/// `sqe` must point at a live SQE slot.
unsafe fn prep_shutdown(sqe: *mut io_uring_sqe, sockfd: socket_t, how: u32) {
    unsafe { prep_rw(sqe, IORING_OP_SHUTDOWN, sockfd, 0, how, 0) };
}

/// PORT NOTE: `io_uring_sqe.prep_timeout(ts, count, flags)` — `fd = -1`, the
/// timespec POINTER rides `addr`, `len = 1`, `count` rides `off`, and `flags`
/// (we always pass `IORING_TIMEOUT_ABS`) rides `timeout_flags`.
///
/// # Safety
/// `sqe` live; `ts` MUST remain valid until the timeout completes — it is read by
/// the kernel asynchronously (we always point it at the `Timer::next` field of a
/// stable `Completion`).
unsafe fn prep_timeout(sqe: *mut io_uring_sqe, ts: *const KernelTimespec, count: u32, flags: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_TIMEOUT, -1, ts as u64, 1, count as u64);
        (*sqe).rw_flags = flags;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_timeout_remove(timer_user_data, flags)` — cancels
/// a pending `IORING_OP_TIMEOUT` identified by ITS `user_data`.
///
/// # Safety
/// `sqe` must point at a live SQE slot.
unsafe fn prep_timeout_remove(sqe: *mut io_uring_sqe, timer_user_data: u64, flags: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_TIMEOUT_REMOVE, -1, timer_user_data, 0, 0);
        (*sqe).rw_flags = flags;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_cancel(cancel_user_data, flags)` — cancels any
/// pending op identified by ITS `user_data`.
///
/// # Safety
/// `sqe` must point at a live SQE slot.
unsafe fn prep_cancel(sqe: *mut io_uring_sqe, cancel_user_data: u64, flags: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_ASYNC_CANCEL, -1, cancel_user_data, 0, 0);
        (*sqe).rw_flags = flags;
    }
}

// ---------------------------------------------------------------------------
// available() (io_uring.zig:14).
// ---------------------------------------------------------------------------

/// True if this backend is available on this platform.
//
// PORT NOTE (verbatim from the reference): Perform a harmless syscall to determine
// if we have io_uring support. We should really only be checking for a subset of
// possible errors to determine if io_uring is available but there isn't really a
// strong course of action for any other error so we just mark it as unavailable.
//
// Also, this is a bit expensive -- we could get away with a single
// io_uring_register syscall to check for availability but that involves manually
// handling errors vs. the tried and true Zig stdlib here.
//
// PORT NOTE: `if (comptime builtin.os.tag != .linux) return false;` — this module
// compiles only on Linux (`#[cfg(target_os = "linux")]` in lib.rs), so that arm is
// comptime-elided. Every `IoUring::init` failure (ENOSYS on a kernel without
// io_uring, EPERM under `kernel.io_uring_disabled=2`, ...) degrades to `false`
// gracefully — no panic, exactly as the reference's `catch return false`.
pub fn available() -> bool {
    // var ring = linux.IoUring.init(256, 0) catch return false;
    let mut ring = match IoUring::init(256, 0) {
        Ok(v) => v,
        Err(_) => return false,
    };
    // ring.deinit();
    ring.deinit();
    true
}

/// The callback of the main Loop operations.
//
// PORT NOTE: `loop.zig`'s `Callback(@This())` monomorphized for the io_uring
// backend — a concrete fn-pointer referencing THIS module's `Loop`/`Completion`/
// `Result` (self-contained, per the brief). `loop_`/`completion` are RAW pointers.
pub type Callback = fn(
    userdata: *mut core::ffi::c_void,
    loop_: *mut Loop,
    completion: *mut Completion,
    result: Result,
) -> CallbackAction;

/// A callback that does nothing and immediately disarms. This implements the
/// io_uring `Callback` and is the default value for completions.
//
// PORT NOTE: `loop.zig`'s `NoopCallback(@This())` monomorphized for io_uring.
pub fn noop_callback(
    _: *mut core::ffi::c_void,
    _: *mut Loop,
    _: *mut Completion,
    _: Result,
) -> CallbackAction {
    CallbackAction::Disarm
}

// ---------------------------------------------------------------------------
// The Loop (io_uring.zig:31).
// ---------------------------------------------------------------------------

/// PORT NOTE: the error set of `Loop.init` — `error{TooManyEntries}` (the `u13`
/// entries cast) unioned with `IoUring.init`'s errors.
//
// PORT NOTE: this is a LOCAL error enum rather than the crate's unified `Error`
// because the `io_uring: overflow entries count` test (io_uring.zig:1128)
// discriminates `error.TooManyEntries` specifically — a unit-struct `Error` could
// not express that assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    /// `options.entries` did not fit the reference's `u13` cast (> 8191).
    TooManyEntries,
    /// The ring itself could not be created.
    Ring(RingInitError),
}

/// PORT NOTE: `Loop.tick_`'s error set — `error{NestedRunsNotAllowed}` unioned
/// with `SubmitError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    NestedRunsNotAllowed,
    Submit(SubmitError),
}

/// PORT NOTE: `pub const Loop = struct { ... }`.
//
// PORT NOTE (vs. epoll's Loop): NO `timers` heap and NO `eventfd` — io_uring
// expresses timers as `IORING_OP_TIMEOUT` operations owned by the kernel, and
// `io_uring_enter` needs no eventfd to be interruptible. See the module MODEL note.
pub struct Loop {
    ring: IoUring,

    /// The number of active completions. This DOES NOT include completions that
    /// are queued in the submissions queue.
    active: usize,

    /// Our queue of submissions that failed to enqueue.
    //
    // PORT NOTE: unlike epoll's `submissions` (work to register next tick), this is
    // the OVERFLOW queue: completions that could not get an SQE because the ring
    // was full, retried by `submit`.
    submissions: queue::Intrusive<Completion>,

    /// Cached time
    cached_now: Timespec,

    flags: LoopFlags,
}

/// PORT NOTE: the `flags: packed struct { now_outdated, stopped, in_run }` sub-struct.
#[derive(Clone, Copy)]
struct LoopFlags {
    /// True if the "now" field is outdated and should be updated when it is used.
    now_outdated: bool,
    /// Whether the loop is stopped or not.
    stopped: bool,
    /// Whether we're in a run or not (to prevent nested runs).
    in_run: bool,
}

// PORT NOTE: the Zig field defaults are `now_outdated = true`, `stopped = false`,
// `in_run = false` — `#[derive(Default)]` would render `now_outdated` as `false`,
// so `Default` is hand-written (the `true` default is load-bearing: a fresh
// `Loop.now()` must read the clock).
impl Default for LoopFlags {
    fn default() -> Self {
        LoopFlags {
            now_outdated: true,
            stopped: false,
            in_run: false,
        }
    }
}

// PORT NOTE: `Loop` holds the ring's raw mmap pointers + a raw-ptr intrusive queue
// + an fd → auto-`!Send`; ghostty MOVES a `Loop` onto the spawned termio thread.
// SAFETY / single-thread affinity: after the move a `Loop` (and every `Completion`
// it links) is touched by exactly one thread at a time — the owning event-loop
// thread — so transferring ownership is sound. The ring's SQ/CQ are shared with the
// KERNEL, not with another Rust thread; that sharing is mediated entirely by the
// head/tail atomics (see the ring-atomics note above), which is exactly what makes
// the single-owner-thread invariant sufficient here.
unsafe impl Send for Loop {}

impl Loop {
    /// Initialize the event loop. "entries" is the maximum number of
    /// submissions that can be queued at one time. The number of completions
    /// always matches the number of entries so the memory allocated will be
    /// 2x entries (plus the basic loop overhead).
    pub fn init(options: Options) -> core::result::Result<Loop, InitError> {
        // const entries = std.math.cast(u13, options.entries) orelse
        //     return error.TooManyEntries;
        //
        // PORT NOTE: Zig's `@as(u13, ...)` cast fails for any value that doesn't fit
        // 13 bits, i.e. > 8191. Rust has no u13, so the bound is spelled explicitly;
        // `2^13 = 8192` (the reference test's value) is the first rejected entry count.
        const MAX_ENTRIES: u32 = (1 << 13) - 1;
        if options.entries > MAX_ENTRIES {
            return Err(InitError::TooManyEntries);
        }
        let entries = options.entries as u16;

        let mut result: Loop = Loop {
            // TODO(mitchellh): add an init_advanced function or something
            // for people using the io_uring API directly to be able to set
            // the flags for this.
            ring: IoUring::init(entries, 0).map_err(InitError::Ring)?,
            active: 0,
            submissions: queue::Intrusive::default(),
            // .cached_now = undefined — populated by update_now() below.
            cached_now: unsafe { core::mem::zeroed() },
            flags: LoopFlags::default(),
        };
        result.update_now();

        Ok(result)
    }

    pub fn deinit(&mut self) {
        self.ring.deinit();
    }

    /// Run the event loop. See RunMode documentation for details on modes.
    pub fn run(&mut self, mode: RunMode) -> core::result::Result<(), RunError> {
        // PORT NOTE: the reference's `tick_` takes the mode COMPTIME ("so we can do
        // some tricks to avoid function calls and runtime branching"). Rust has no
        // comptime-parameter monomorphization over a value, so the mode is a runtime
        // parameter (exactly as epoll.rs's `tick` takes a runtime `wait`); the
        // branch arms are identical.
        self.tick_(mode)
    }

    /// Stop the loop. This can only be called from the main thread.
    /// This will stop the loop forever. Future ticks will do nothing.
    /// All completions are safe to read/write once any outstanding
    /// `run` or `tick` calls are returned.
    pub fn stop(&mut self) {
        self.flags.stopped = true;
    }

    /// Returns true if the loop is stopped. This may mean there
    /// are still pending completions to be processed.
    pub fn stopped(&self) -> bool {
        self.flags.stopped
    }

    /// Returns the "loop" time in milliseconds. The loop time is updated
    /// once per loop tick, before IO polling occurs. It remains constant
    /// throughout callback execution.
    ///
    /// You can force an update of the "now" value by calling update_now()
    /// at any time from the main thread.
    ///
    /// The clock that is used is not guaranteed. In general, a monotonic
    /// clock source is always used if available. This value should typically
    /// just be used for relative time calculations within the loop, such as
    /// answering the question "did this happen <x> ms ago?".
    //
    // PORT NOTE: takes `&mut self` (not epoll's `&self`) because the reference
    // `now(self: *Loop)` lazily refreshes the cache via the `now_outdated` flag.
    pub fn now(&mut self) -> i64 {
        if self.flags.now_outdated {
            self.update_now();
        }

        // If anything overflows we just return the max value.
        let max = i64::MAX;

        // Calculate all the values, being careful about overflows in order
        // to just return the maximum value.
        let sec = match (self.cached_now.tv_sec as isize).checked_mul(ms_per_s as isize) {
            Some(v) => v,
            None => return max,
        };
        let nsec = (self.cached_now.tv_nsec as isize).div_euclid(ns_per_ms as isize);
        // std.math.lossyCast(i64, sec +| nsec)
        (sec.saturating_add(nsec)) as i64
    }

    /// Update the cached time.
    pub fn update_now(&mut self) {
        // if (posix.clock_gettime(posix.CLOCK.MONOTONIC)) |new_time| ... else |_| {}
        let mut new_time: Timespec = unsafe { core::mem::zeroed() };
        // SAFETY: clock_gettime writes a `timespec` into `new_time`.
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut new_time) } == 0 {
            self.cached_now = new_time;
            self.flags.now_outdated = false;
        } else {
            // Errors are ignored.
        }
    }

    /// Tick the loop.
    fn tick_(&mut self, mode: RunMode) -> core::result::Result<(), RunError> {
        // We can't nest runs.
        if self.flags.in_run {
            return Err(RunError::NestedRunsNotAllowed);
        }
        self.flags.in_run = true;
        // defer self.flags.in_run = false; — via the explicit reset below.
        let result = self.tick_inner(mode);
        self.flags.in_run = false;
        result
    }

    fn tick_inner(&mut self, mode: RunMode) -> core::result::Result<(), RunError> {
        let self_ptr: *mut Loop = self;

        // The total number of events we're waiting for.
        let wait: u32 = match mode {
            // until_done is one because we need to wait for at least one
            // (if we have any).
            RunMode::UntilDone => 1,
            RunMode::Once => 1,
            RunMode::NoWait => 0,
        };

        // var cqes: [128]linux.io_uring_cqe = undefined;
        let mut cqes: [io_uring_cqe; 128] = unsafe { core::mem::zeroed() };
        loop {
            // If we're stopped then the loop is fully over.
            if self.flags.stopped {
                break;
            }

            // We always note that our "now" value is outdated even if we have
            // no completions to execute.
            self.flags.now_outdated = true;

            if self.active == 0 && self.submissions.empty() {
                break;
            }

            // If we have no queued submissions then we do the wait as part
            // of the submit call, because then we can do exactly once syscall
            // to get all our events.
            if self.submissions.empty() {
                match self.ring.submit_and_wait(wait) {
                    Ok(_) => {}
                    Err(SubmitError::SignalInterrupt) => continue,
                    Err(e) => return Err(RunError::Submit(e)),
                }
            } else {
                // We have submissions, meaning we have to do multiple submissions
                // anyways so we always just do non-waiting ones.
                match self.submit() {
                    Ok(()) => {}
                    Err(SubmitError::SignalInterrupt) => continue,
                    Err(e) => return Err(RunError::Submit(e)),
                }
            }

            // Wait for completions...
            let count = match self.ring.copy_cqes(&mut cqes, wait) {
                Ok(v) => v,
                // EINTR means our blocking syscall was interrupted by some
                // process signal. We just retry when we can. See signal(7).
                Err(SubmitError::SignalInterrupt) => continue,
                Err(e) => return Err(RunError::Submit(e)),
            };

            for cqe in &cqes[0..count as usize] {
                // const c = @as(?*Completion, @ptrFromInt(cqe.user_data)) orelse continue;
                //
                // PORT NOTE: a NULL `user_data` is the `.noop` completion's stray SQE
                // (see `add_`'s noop arm) — the kernel posts a CQE for it, and we
                // skip it WITHOUT touching `active` (the noop arm never counted it).
                let c: *mut Completion = cqe.user_data as *mut Completion;
                if c.is_null() {
                    continue;
                }

                self.active -= 1;
                unsafe { (*c).flags.state = State::Dead };
                match unsafe { Completion::invoke(c, self_ptr, cqe.res) } {
                    CallbackAction::Disarm => {}
                    CallbackAction::Rearm => unsafe { self.add(c) },
                }
            }

            // Subtract our waiters. If we reached zero then we're done.
            match mode {
                RunMode::NoWait => break,
                RunMode::Once => {
                    if count > 0 {
                        break;
                    }
                }
                RunMode::UntilDone => {}
            }
        }

        Ok(())
    }

    /// Submit all queued operations. This never does an io_uring submit
    /// and wait operation.
    pub fn submit(&mut self) -> core::result::Result<(), SubmitError> {
        self.ring.submit()?;

        // If we have any submissions that failed to submit, we try to
        // send those now. We have to make a copy so that any failures are
        // resubmitted without an infinite loop.
        let mut queued = core::mem::take(&mut self.submissions);
        // self.submissions = .{};  (done by mem::take)
        loop {
            let c = unsafe { queued.pop() };
            if c.is_null() {
                break;
            }
            unsafe { self.add_(c, true) };
        }

        Ok(())
    }

    /// Add a timer to the loop. The timer will initially execute in "next_ms"
    /// from now and will repeat every "repeat_ms" thereafter. If "next_ms" is
    /// zero then the timer will invoke on the next loop tick.
    ///
    /// If next_ms is too large of a value that it doesn't fit into the
    /// kernel time structure, the maximum sleep value will be used for your
    /// system (this is at least 45 days on a 32-bit system, and thousands of
    /// years on a 64-bit system).
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer that outlives the timer.
    pub unsafe fn timer(
        &mut self,
        c: *mut Completion,
        next_ms: u64,
        userdata: *mut core::ffi::c_void,
        cb: Callback,
    ) {
        unsafe {
            *c = Completion {
                op: Operation::Timer(Timer {
                    next: self.timer_next(next_ms),
                    reset: None,
                }),
                userdata,
                callback: cb,
                ..Completion::default()
            };

            self.add(c);
        }
    }

    /// Reset a timer to trigger in next_ms.
    ///
    /// Important: c and c_cancel must NEVER be undefined. If you are using
    /// timer_reset you should always initial c to the zero value ".{}".
    ///
    /// If the timer completion "c" is dead, this is equivalent to calling
    /// "timer" and "c_cancel" is not used. You can verify this scenario by
    /// checking "c_cancel.state()" after this call.
    ///
    /// If the timer completion "c" is active, and "c_cancel" is also active,
    /// it is assumed that c_cancel is already resetting the timer. The
    /// "next_ms" time is updated but otherwise the cancellation continues
    /// and reset works.
    ///
    /// If the timer completion "c" is active, and "c_cancel" is dead,
    /// "c" is cancelled and restarted to trigger in next_ms.
    ///
    /// There is no guarantee that c_cancel will ever be used. Check the
    /// resulting state to know if you can reclaim its memory or not.
    ///
    /// # Safety
    /// `c`/`c_cancel` must be valid, stable `Completion` pointers.
    pub unsafe fn timer_reset(
        &mut self,
        c: *mut Completion,
        c_cancel: *mut Completion,
        next_ms: u64,
        userdata: *mut core::ffi::c_void,
        cb: Callback,
    ) {
        if unsafe { Completion::state(c) } == CompletionState::Dead {
            unsafe { self.timer(c, next_ms, userdata, cb) };
            return;
        }

        // Update the reset time for the timer to the desired time
        // along with all the callbacks.
        let reset = self.timer_next(next_ms);
        unsafe {
            match &mut (*c).op {
                Operation::Timer(t) => t.reset = Some(reset),
                _ => unreachable!(),
            }
            (*c).userdata = userdata;
            (*c).callback = cb;
        }

        // If the cancellation is active, we assume its for this timer
        // and do nothing.
        if unsafe { Completion::state(c_cancel) } == CompletionState::Active {
            return;
        }

        debug_assert!(
            unsafe { Completion::state(c_cancel) } == CompletionState::Dead
                && unsafe { Completion::state(c) } == CompletionState::Active
        );

        // PORT NOTE (verbatim): we COULD use IORING_TIMEOUT_UPDATE here with the
        // IORING_TIMEOUT_REMOVE op. We don't right now because it complicates
        // our logic and ups our required kernel version to 5.11 while this
        // works good enough. If there is a compelling reason to use
        // IORING_TIMEOUT_UPDATE, I'm open to hearing it.
        unsafe {
            *c_cancel = Completion {
                op: Operation::TimerRemove { timer: c },
                ..Completion::default()
            };
            self.add(c_cancel);
        }
    }

    fn timer_next(&mut self, next_ms: u64) -> KernelTimespec {
        // Get the timestamp of the absolute time that we'll execute this timer.
        // There are lots of failure scenarios here in math. If we see any
        // of them we just use the maximum value.
        let max: KernelTimespec = KernelTimespec {
            sec: isize::MAX as i64,
            nsec: isize::MAX as i64,
        };

        let next_s: isize = match isize::try_from(next_ms / ms_per_s) {
            Ok(v) => v,
            Err(_) => return max,
        };
        let next_ns: isize = match isize::try_from((next_ms % ms_per_s) * ns_per_ms) {
            Ok(v) => v,
            Err(_) => return max,
        };

        if self.flags.now_outdated {
            self.update_now();
        }

        // PORT NOTE: the resulting `nsec` may exceed 1e9 (it is `cached_now.nsec +
        // next_ns`, un-normalized). This is FAITHFUL and correct: the kernel's
        // `io_timeout_prep` only rejects NEGATIVE fields, and `timespec64_to_ktime`
        // computes `sec * 1e9 + nsec`, so a carry-less timespec is handled exactly.
        KernelTimespec {
            sec: match (self.cached_now.tv_sec as isize).checked_add(next_s) {
                Some(v) => v as i64,
                None => return max,
            },
            nsec: match (self.cached_now.tv_nsec as isize).checked_add(next_ns) {
                Some(v) => v as i64,
                None => return max,
            },
        }
    }

    /// Add a completion to the loop. This does NOT start the operation!
    /// You must call "submit" at some point to submit all of the queued
    /// work.
    ///
    /// # Safety
    /// `completion` must be a valid, stable pointer to a `Completion` that lives
    /// until its callback fires (libxev's intrusive-ownership contract).
    pub unsafe fn add(&mut self, completion: *mut Completion) {
        unsafe { self.add_(completion, false) }
    }

    /// Internal add function. The only difference is try_submit. If try_submit
    /// is true, then this function will attempt to submit the queue to the
    /// ring if the submission queue is full rather than filling up our FIFO.
    ///
    /// # Safety
    /// `completion` must be a valid, stable `Completion` pointer.
    unsafe fn add_(&mut self, completion: *mut Completion, try_submit: bool) {
        // The completion at this point is active no matter what because
        // it is going to be queued.
        unsafe { (*completion).flags.state = State::Active };

        let sqe: *mut io_uring_sqe = match unsafe { self.get_sqe_or_queue(completion, try_submit) }
        {
            Some(v) => v,
            None => return,
        };

        // Increase active to the amount in the ring.
        self.active += 1;

        // Setup the submission depending on the operation
        match unsafe { &mut (*completion).op } {
            // Do nothing with noop completions.
            //
            // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): this arm returns
            // WITHOUT filling the SQE it just took from `get_sqe`, yet `get_sqe`
            // already advanced `sqe_tail` — so `flush_sq` WILL hand that stale slot
            // to the kernel. The reference does exactly this (io_uring.zig:394); it
            // is what the reference test means by "Due to implementation details, we
            // need to have an ongoing completion to test that the noop operation
            // works properly".
            //
            // Why it is benign for a FRESH slot: the kernel allocates the SQE mapping
            // with `__GFP_ZERO`, so a never-before-used slot reads as `opcode =
            // IORING_OP_NOP` (0) with `user_data = 0` — the kernel posts a harmless
            // NOP CQE and `tick_` skips any CQE with a null `user_data` (which is
            // also why the noop arm must NOT leave `active` incremented: it is
            // decremented here, and the null-user_data CQE never decrements it).
            //
            // CAVEAT (latent, inherited): if the slot was previously used by a REAL
            // op, it still holds that op's opcode AND `user_data`, so a `.noop` add
            // could re-submit a stale operation pointing at an old completion. This
            // is a reference bug, not a port artifact. It is unreachable in practice:
            // nothing in ghostty (nor any ported watcher) ever adds a `.noop`
            // completion — only libxev's own test does, and it does so on a fresh
            // ring. Flagged here so Task 6+ doesn't "discover" it as a port defect.
            // Fixing it would mean rewinding `sqe_tail`, which would DIVERGE from the
            // reference; left faithful per R3.
            Operation::Noop => {
                unsafe { (*completion).flags.state = State::Dead };
                self.active -= 1;
                return;
            }

            Operation::Accept {
                socket,
                addr,
                addr_size,
                flags,
            } => unsafe {
                prep_accept(
                    sqe,
                    *socket,
                    ptr::addr_of_mut!(*addr),
                    ptr::addr_of_mut!(*addr_size),
                    *flags,
                )
            },

            Operation::Close { fd } => unsafe { prep_close(sqe, *fd) },

            Operation::Connect { socket, addr } => unsafe {
                let len = addr.get_os_sock_len();
                prep_connect(sqe, *socket, ptr::addr_of!(addr.any), len)
            },

            Operation::Poll { fd, events } => unsafe { prep_poll_add(sqe, *fd, *events) },

            Operation::Read { fd, buffer } => unsafe {
                match buffer {
                    // offset is a u64 but if the value is -1 then it uses
                    // the offset in the fd.
                    ReadBuffer::Array(buf) => {
                        prep_read(sqe, *fd, buf.as_mut_ptr(), buf.len(), -1i64 as u64)
                    }
                    ReadBuffer::Slice(buf) => {
                        prep_read(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), -1i64 as u64)
                    }
                }
            },

            Operation::Recv { fd, buffer } => unsafe {
                match buffer {
                    ReadBuffer::Array(buf) => prep_recv(sqe, *fd, buf.as_mut_ptr(), buf.len(), 0),
                    ReadBuffer::Slice(buf) => {
                        prep_recv(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), 0)
                    }
                }
            },

            Operation::Send { fd, buffer } => unsafe {
                match buffer {
                    WriteBuffer::Array { array, len } => {
                        prep_send(sqe, *fd, array.as_ptr(), *len, 0)
                    }
                    WriteBuffer::Slice(buf) => {
                        prep_send(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), 0)
                    }
                }
            },

            Operation::Shutdown { socket, how } => unsafe {
                prep_shutdown(
                    sqe,
                    *socket,
                    match how {
                        ShutdownHow::Both => libc::SHUT_RDWR as u32,
                        ShutdownHow::Send => libc::SHUT_WR as u32,
                        ShutdownHow::Recv => libc::SHUT_RD as u32,
                    },
                )
            },

            Operation::Timer(v) => unsafe {
                // PORT NOTE: `&v.next` — the timespec pointer handed to the kernel
                // points INTO the completion, which the caller guarantees is stable
                // until the callback fires. That is precisely why the SQE may
                // reference it asynchronously.
                prep_timeout(sqe, ptr::addr_of!(v.next), 0, IORING_TIMEOUT_ABS)
            },

            Operation::TimerRemove { timer } => unsafe {
                prep_timeout_remove(sqe, *timer as u64, 0)
            },

            Operation::Write { fd, buffer } => unsafe {
                match buffer {
                    // offset is a u64 but if the value is -1 then it uses
                    // the offset in the fd.
                    WriteBuffer::Array { array, len } => {
                        prep_write(sqe, *fd, array.as_ptr(), *len, -1i64 as u64)
                    }
                    WriteBuffer::Slice(buf) => {
                        prep_write(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), -1i64 as u64)
                    }
                }
            },

            Operation::Cancel { c } => unsafe { prep_cancel(sqe, *c as u64, 0) },
        }

        // Our sqe user data always points back to the completion.
        // The prep functions above reset the user data so we have to do this
        // here.
        // SAFETY: `sqe` is the live slot we just prepped.
        unsafe { (*sqe).user_data = completion as u64 };
    }

    /// PORT NOTE: the `catch |err| switch (err) { error.SubmissionQueueFull =>
    /// retry: {...} }` block of `add_`, lifted to a helper so the (recursive)
    /// `self.submit()` retry doesn't nest inside the `match` on `self.ring`.
    /// Returns `None` when the completion was pushed to the overflow queue.
    ///
    /// # Safety
    /// `completion` must be a valid, stable `Completion` pointer.
    unsafe fn get_sqe_or_queue(
        &mut self,
        completion: *mut Completion,
        try_submit: bool,
    ) -> Option<*mut io_uring_sqe> {
        match self.ring.get_sqe() {
            Ok(sqe) => Some(sqe),
            Err(SubmissionQueueFull) => {
                // If the queue is full and we're in try_submit mode then we
                // attempt to submit. This is used during submission flushing.
                if try_submit && self.submit().is_ok() {
                    // Submission succeeded but we may still fail (unlikely)
                    // to get an SQE...
                    if let Ok(sqe) = self.ring.get_sqe() {
                        return Some(sqe);
                    }
                }

                // Add the completion to our submissions to try to flush later.
                unsafe { self.submissions.push(completion) };
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Completion (io_uring.zig:613).
// ---------------------------------------------------------------------------

/// A completion represents a single queued request in the ring.
/// Completions must have stable pointers.
///
/// For the lowest overhead, these can be created manually and queued
/// directly. The API over the individual fields isn't the most user-friendly
/// since it is tune for performance. For user-friendly operations,
/// use the higher-level functions on this structure or the even
/// higher-level abstractions like the Timer struct.
#[repr(C)]
pub struct Completion {
    /// Operation to execute. This is only safe to read BEFORE the completion
    /// is queued. After being queued (with "add"), the operation may change.
    pub op: Operation,

    /// Userdata and callback for when the completion is finished.
    pub userdata: *mut core::ffi::c_void,
    pub callback: Callback,

    /// Internally set
    next: *mut Completion,

    flags: CompletionFlags,
}

/// PORT NOTE: the `flags: packed struct { state: State = .dead }` sub-struct.
#[derive(Clone, Copy)]
struct CompletionFlags {
    state: State,
}

/// PORT NOTE: `const State = enum(u1) { dead, active }`. NOTE this is a TWO-state
/// enum, unlike epoll's four-state one: io_uring has no "adding"/"deleting"
/// intermediate (a completion is either in the ring or it isn't).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// completion is not part of any loop
    Dead = 0,
    /// completion is registered with the ring
    Active = 1,
}

// PORT NOTE: hand-written `Default` (NOT `#[derive]`) — `Completion` has an
// fn-ptr `callback` + raw-ptr fields with no `Default`. Mirrors the Zig field
// defaults `op = .noop`, `userdata = null`, `callback = noopCallback`,
// `next = null`, `flags = { state: dead }`.
impl Default for Completion {
    fn default() -> Self {
        Completion {
            op: Operation::default(),
            userdata: ptr::null_mut(),
            callback: noop_callback,
            next: ptr::null_mut(),
            flags: CompletionFlags { state: State::Dead },
        }
    }
}

// PORT NOTE: `Completion` holds a raw-ptr `next` link + an fn-ptr → auto-`!Send`;
// it rides the `Loop` moved onto the termio thread. SAFETY / single-thread
// affinity: after the move a completion is touched by exactly the owning
// event-loop thread, so transferring ownership is sound.
unsafe impl Send for Completion {}

// PORT NOTE: intrusive `next` link seam for `queue::Intrusive<Completion>`
// (the overflow `submissions` queue).
impl queue::Node for Completion {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

impl Completion {
    /// Returns the state of this completion. There are some things to
    /// be caution about when calling this function.
    ///
    /// First, this is only safe to call from the main thread. This cannot
    /// be called from any other thread.
    ///
    /// Second, if you are using default "undefined" completions, this will
    /// NOT return a valid value if you access it. You must zero your
    /// completion using ".{}". You only need to zero the completion once.
    /// Once the completion is in use, it will always be valid.
    ///
    /// Third, if you stop the loop (loop.stop()), the completions registered
    /// with the loop will NOT be reset to a dead state.
    //
    // PORT NOTE: reference `state(self: Completion)` (by value); ported as an
    // associated fn taking `*mut Completion` (raw self) to stay on the raw-ptr
    // discipline. Test call sites (`c1.state()`) route through `state_of`.
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn state(c: *mut Completion) -> CompletionState {
        match unsafe { (*c).flags.state } {
            State::Dead => CompletionState::Dead,
            State::Active => CompletionState::Active,
        }
    }

    /// Convenience wrapper for `state` callable on a `&Completion` (tests do
    /// `c1.state_of()`).
    pub fn state_of(&self) -> CompletionState {
        // SAFETY: `self` is a live `Completion`.
        unsafe { Completion::state(self as *const Completion as *mut Completion) }
    }

    /// Invokes the callback for this completion after properly constructing
    /// the Result based on the res code.
    ///
    /// # Safety
    /// `self_` must be a valid, stable `Completion` pointer and `loop_` the owning
    /// loop; `res` is the raw `cqe.res` for this completion.
    unsafe fn invoke(self_: *mut Completion, loop_: *mut Loop, res: i32) -> CallbackAction {
        let result: Result = match unsafe { &mut (*self_).op } {
            Operation::Noop => unreachable!(),

            Operation::Accept { .. } => Result::Accept(if res >= 0 {
                Ok(res as socket_t)
            } else {
                Err(match -res {
                    libc::ECANCELED => AcceptError::Canceled,
                    libc::EAGAIN => AcceptError::Again,
                    _ => AcceptError::Unexpected,
                })
            }),

            Operation::Close { .. } => Result::Close(if res >= 0 {
                Ok(())
            } else {
                Err(CloseError::Unexpected)
            }),

            Operation::Connect { .. } => Result::Connect(if res >= 0 {
                Ok(())
            } else {
                Err(match -res {
                    libc::ECANCELED => ConnectError::Canceled,
                    libc::ECONNREFUSED => ConnectError::ConnectionRefused,
                    libc::ETIMEDOUT => ConnectError::TimedOut,
                    libc::EHOSTUNREACH => ConnectError::HostUnreachable,
                    _ => ConnectError::Unexpected,
                })
            }),

            Operation::Poll { .. } => Result::Poll(if res >= 0 {
                Ok(())
            } else {
                Err(PollError::Unexpected)
            }),

            Operation::Read { buffer, .. } => Result::Read(read_result(Some(buffer), res)),

            Operation::Recv { buffer, .. } => Result::Recv(read_result(Some(buffer), res)),

            Operation::Send { .. } => Result::Send(if res >= 0 {
                Ok(res as usize)
            } else {
                Err(match -res {
                    libc::ECANCELED => WriteError::Canceled,
                    libc::EPIPE => WriteError::BrokenPipe,
                    libc::ECONNRESET => WriteError::ConnectionResetByPeer,
                    _ => WriteError::Unexpected,
                })
            }),

            Operation::Shutdown { .. } => Result::Shutdown(if res >= 0 {
                Ok(())
            } else {
                Err(match -res {
                    libc::ECANCELED => ShutdownError::Canceled,
                    _ => ShutdownError::Unexpected,
                })
            }),

            Operation::Timer(op) => {
                // If we have reset set, that means that we were canceled so
                // that we can update our expiration time.
                //
                // PORT NOTE: this arm returns EARLY — the callback is deliberately
                // NOT called on the cancellation that a `timer_reset` induced.
                if let Some(r) = op.reset {
                    op.next = r;
                    op.reset = None;
                    return CallbackAction::Rearm;
                }

                Result::Timer(if res >= 0 {
                    Ok(TimerTrigger::Request)
                } else {
                    match -res {
                        libc::ETIME => Ok(TimerTrigger::Expiration),
                        libc::ECANCELED => Ok(TimerTrigger::Cancel),
                        _ => Err(TimerError::Unexpected),
                    }
                })
            }

            Operation::TimerRemove { .. } => Result::TimerRemove(if res >= 0 {
                Ok(())
            } else {
                match -res {
                    libc::ENOENT => Err(TimerRemoveError::NotFound),
                    libc::EBUSY => Err(TimerRemoveError::ExpirationInProgress),

                    // PORT NOTE (verbatim): Okay, I don't know why this might
                    // happen. It appears to happen when the timer you're attempting
                    // to cancel has already expired, but I can't reproduce this in a
                    // unit test. If this isn't safe to ignore or someone can find
                    // the meaning of this, I'd be curious.
                    libc::ETIME => Ok(()),

                    _ => Err(TimerRemoveError::Unexpected),
                }
            }),

            Operation::Write { .. } => {
                if res >= 0 {
                    Result::Write(Ok(res as usize))
                } else {
                    match -res {
                        libc::ECANCELED => Result::Write(Err(WriteError::Canceled)),
                        // If a write is interrupted, we retry it automatically.
                        libc::EINTR => return CallbackAction::Rearm,
                        _ => Result::Write(Err(WriteError::Unexpected)),
                    }
                }
            }

            Operation::Cancel { .. } => Result::Cancel(if res >= 0 {
                Ok(())
            } else {
                Err(match -res {
                    libc::ENOENT => CancelError::NotFound,
                    libc::EALREADY => CancelError::ExpirationInProgress,
                    _ => CancelError::Unexpected,
                })
            }),
        };

        // return self.callback(self.userdata, loop, self, result);
        let cb = unsafe { (*self_).callback };
        let ud = unsafe { (*self_).userdata };
        cb(ud, loop_, self_, result)
    }
}

/// PORT NOTE: `Completion.readResult(op, res)` — the shared read/pread/recv/recvmsg
/// result decoder. The Zig comptime `@field(self.op, @tagName(op))` +
/// `@hasField(..., "buffer")` reflection collapses to an `Option<&ReadBuffer>`
/// parameter (`None` = the buffer-less ops, i.e. the deferred `recvmsg`).
fn read_result(buffer: Option<&ReadBuffer>, res: i32) -> core::result::Result<usize, ReadError> {
    if res > 0 {
        return Ok(res as usize);
    }

    if res == 0 {
        // if (!@hasField(@TypeOf(active), "buffer")) return ReadError.EOF;
        let Some(buffer) = buffer else {
            return Err(ReadError::EOF);
        };

        // If we receieve a zero byte read, it is an EOF _unless_
        // the requestesd buffer size was zero (weird).
        return match buffer {
            ReadBuffer::Slice(b) => {
                if (*b).len() == 0 {
                    Ok(0)
                } else {
                    Err(ReadError::EOF)
                }
            }
            ReadBuffer::Array(_) => Err(ReadError::EOF),
        };
    }

    Err(match -res {
        libc::ECANCELED => ReadError::Canceled,
        libc::ECONNRESET => ReadError::ConnectionResetByPeer,
        _ => ReadError::Unexpected,
    })
}

// ---------------------------------------------------------------------------
// Operation / Result unions (io_uring.zig:890/914) + errors + buffers.
// ---------------------------------------------------------------------------

/// All the supported operations of this event loop. These are always
/// backend-specific and therefore the structure and types change depending
/// on the underlying system in use. The high level operations are
/// done by initializing the request handles.
//
// PORT NOTE: `op: Operation = .{ .noop = {} }` — the `#[default]` `Noop` variant.
// The ported tag set mirrors epoll.rs's (Task 3/4 scope) PLUS io_uring's own
// `timer_remove` (which `timer_reset` needs — epoll cancels via `Cancel`). The
// reference's `pread`/`pwrite`/`sendmsg`/`recvmsg` tags stay DEFERRED exactly as
// they are in epoll.rs (no in-scope test/shell-path use; they need the `msghdr`/
// `iovec` surface the crate hasn't ported).
#[derive(Default)]
pub enum Operation {
    /// Do nothing. This operation will not be queued and will never
    /// have its callback fired. This is NOT equivalent to the io_uring
    /// "nop" operation.
    #[default]
    Noop,

    /// Accept a connection on a socket.
    Accept {
        socket: socket_t,
        addr: net::sockaddr::Any,
        addr_size: net::socklen_t,
        flags: u32,
    },

    /// Close a file descriptor.
    Close { fd: fd_t },

    /// Initiate a connection on a socket.
    Connect {
        socket: socket_t,
        addr: net::Address,
    },

    /// Poll a fd. Only oneshot mode is supported today.
    Poll { fd: fd_t, events: u32 },

    /// Read
    Read { fd: fd_t, buffer: ReadBuffer },

    /// Receive a message from a socket.
    Recv { fd: fd_t, buffer: ReadBuffer },

    /// Send a message on a socket.
    Send { fd: fd_t, buffer: WriteBuffer },

    /// Shutdown all or part of a full-duplex connection.
    Shutdown { socket: socket_t, how: ShutdownHow },

    /// Write
    Write { fd: fd_t, buffer: WriteBuffer },

    /// A oneshot or repeating timer. For io_uring, this is implemented
    /// using the timeout mechanism.
    Timer(Timer),

    /// Cancel an existing timer.
    TimerRemove { timer: *mut Completion },

    /// Cancel an existing operation.
    Cancel { c: *mut Completion },
}

/// PORT NOTE: the `timer: struct { next, reset }` payload of `Operation`.
//
// PORT NOTE (vs. epoll's `Timer`): NO `heap`/`c` fields — io_uring's timer lives in
// the KERNEL (an `IORING_OP_TIMEOUT` SQE), not in a userspace pairing heap, so it
// needs neither a heap link nor a back-pointer to its completion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timer {
    pub next: KernelTimespec,

    /// Only used internally. If this is non-null and timer is
    /// CANCELLED, then the timer is rearmed automatically with this
    /// as the next time. The callback will not be called on the
    /// cancellation.
    pub reset: Option<KernelTimespec>,
}

/// PORT NOTE: `posix.ShutdownHow` — the `how` selector of the `shutdown` op
/// (`.both` default).
#[derive(Clone, Copy)]
pub enum ShutdownHow {
    Recv,
    Send,
    Both,
}

/// The result type based on the operation type. For a callback, the
/// result tag will ALWAYS match the operation tag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Result {
    Noop,
    Accept(core::result::Result<socket_t, AcceptError>),
    Close(core::result::Result<(), CloseError>),
    Connect(core::result::Result<(), ConnectError>),
    Poll(core::result::Result<(), PollError>),
    Read(core::result::Result<usize, ReadError>),
    Recv(core::result::Result<usize, ReadError>),
    Send(core::result::Result<usize, WriteError>),
    Shutdown(core::result::Result<(), ShutdownError>),
    Write(core::result::Result<usize, WriteError>),
    Timer(core::result::Result<TimerTrigger, TimerError>),
    TimerRemove(core::result::Result<(), TimerRemoveError>),
    Cancel(core::result::Result<(), CancelError>),
}

/// PORT NOTE: `pub const AcceptError = error{Canceled, Again, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptError {
    Canceled,
    Again,
    Unexpected,
}

/// PORT NOTE: `pub const CancelError = error{NotFound, ExpirationInProgress, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelError {
    NotFound,
    ExpirationInProgress,
    Unexpected,
}

/// PORT NOTE: `pub const CloseError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const ConnectError = error{Canceled, Unexpected,
/// ConnectionRefused, HostUnreachable, TimedOut}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    Canceled,
    Unexpected,
    ConnectionRefused,
    HostUnreachable,
    TimedOut,
}

/// PORT NOTE: `pub const PollError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const ReadError = error{EOF, Canceled, Unexpected,
/// ConnectionResetByPeer}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    EOF,
    Canceled,
    Unexpected,
    ConnectionResetByPeer,
}

/// PORT NOTE: `pub const ShutdownError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const WriteError = error{Canceled, BrokenPipe,
/// ConnectionResetByPeer, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    Canceled,
    BrokenPipe,
    ConnectionResetByPeer,
    Unexpected,
}

/// PORT NOTE: `pub const TimerError = error{Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    Unexpected,
}

/// PORT NOTE: `pub const TimerRemoveError = error{NotFound, ExpirationInProgress,
/// Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerRemoveError {
    NotFound,
    ExpirationInProgress,
    Unexpected,
}

/// PORT NOTE: `pub const TimerTrigger = enum { request, expiration, cancel }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerTrigger {
    /// Timer completed due to linked request completing in time.
    Request,
    /// Timer expired.
    Expiration,
    /// Timer was canceled.
    Cancel,
}

/// ReadBuffer are the various options for reading.
//
// PORT NOTE (R3): the Zig `slice: []u8` ports as a raw fat pointer `*mut [u8]`
// (never a lifetime — the buffer lives in caller memory and the KERNEL writes it
// asynchronously), NEVER a `String`.
#[derive(Clone, Copy)]
pub enum ReadBuffer {
    /// Read into this slice.
    Slice(*mut [u8]),

    /// Read into this array, just set this to undefined and it will
    /// be populated up to the size of the array. This is an option because
    /// the other union members force a specific size anyways so this lets us
    /// use the other size in the union to support small reads without worrying
    /// about buffer allocation.
    ///
    /// To know the size read you have to use the return value of the
    /// read operations (i.e. recv).
    ///
    /// Note that the union at the time of this writing could accomodate a
    /// much larger fixed size array here but we want to retain flexiblity
    /// for future fields.
    Array([u8; 32]),
    // TODO: future will have vectors
}

/// WriteBuffer are the various options for writing.
//
// PORT NOTE (R3): the Zig `slice: []const u8` ports as a raw fat pointer
// `*const [u8]` (never a lifetime — the buffer lives in caller memory and the
// KERNEL reads it asynchronously).
#[derive(Clone, Copy)]
pub enum WriteBuffer {
    /// Write from this buffer.
    Slice(*const [u8]),

    /// Write from this array. See ReadBuffer.array for why we support this.
    Array { array: [u8; 32], len: usize },
    // TODO: future will have vectors
}

// PORT NOTE: `test "Completion size"` (io_uring.zig:1117) — the layout tripwire.
//
// PORT NOTE: the reference asserts the Zig `@sizeOf(Completion) == 152`. Rust's
// tagged-enum + struct layout differs FUNDAMENTALLY from Zig's tagged-union
// layout, so the concrete byte size is not 152. The tripwire's PURPOSE — trip on
// any accidental `Completion` bloat — is preserved by pinning the MEASURED Rust
// size; drift in the op union / buffers / error sets will fail this `const`
// assert exactly as the Zig `@compileLog` guard does. (Same divergence as
// epoll.rs's / the kqueue port's `COMPLETION_SIZE`.)
//
// Measured Rust size on aarch64-unknown-linux-gnu. As in epoll.rs, the
// `Operation` union is dominated by `Connect`, which embeds `net::Address` (a
// `sockaddr` union whose largest member is `sockaddr_un`, ~110 bytes).
const COMPLETION_SIZE: usize = 152;
const _: () = assert!(size_of::<Completion>() == COMPLETION_SIZE);

// ---------------------------------------------------------------------------
// Tests (io_uring.zig `test{}` blocks). The L1 LT5 subset: available /
// overflow entries count / Completion size / default completion / loop time /
// stop. (timer/timer-reset/timerfd/timer-remove/socket/cancellation tests are
// Task 6+.) Callbacks are plain fn items matching the loop `Callback` ABI;
// completions/userdata are stack locals reached through raw pointers (the
// intrusive-ownership contract).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_void;

    // PORT NOTE: the shared `.{ b.* = true; return .disarm; }` timer callback the
    // "default completion"/"stop" tests use.
    fn set_true_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        _r: Result,
    ) -> CallbackAction {
        // const b = @as(*bool, @ptrCast(ud.?)); b.* = true;
        unsafe { *ud.cast::<bool>() = true };
        CallbackAction::Disarm
    }

    // test "io_uring: available"
    #[test]
    fn io_uring_available() {
        assert!(available());
    }

    // test "io_uring: overflow entries count"
    #[test]
    fn io_uring_overflow_entries_count() {
        {
            let mut loop_ = Loop::init(Options::default()).unwrap();
            loop_.deinit();
        }

        {
            // try testing.expectError(error.TooManyEntries, Loop.init(.{
            //     .entries = std.math.pow(u14, 2, 13),
            // }));
            let r = Loop::init(Options {
                entries: 2u32.pow(13),
                ..Options::default()
            });
            assert!(matches!(r, Err(InitError::TooManyEntries)));
        }
    }

    // test "Completion size"
    #[test]
    fn io_uring_completion_size() {
        // Just so we are aware when we change the size (Rust layout differs from
        // Zig's — the pinned constant is the measured Rust size, see COMPLETION_SIZE).
        assert_eq!(core::mem::size_of::<Completion>(), COMPLETION_SIZE);
    }

    // test "io_uring: default completion"
    #[test]
    fn io_uring_default_completion() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Due to implementation details, we need to have an ongoing completion
        // to test that the noop operation works properly.
        let mut timer_called = false;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                1,
                ptr::addr_of_mut!(timer_called).cast::<c_void>(),
                set_true_cb,
            );
        }

        // var c: Completion = .{};
        let mut c = Completion::default();
        unsafe { loop_.add(ptr::addr_of_mut!(c)) };

        // Tick
        loop_.run(RunMode::UntilDone).unwrap();

        // Completion should have been called and be dead.
        assert!(c.state_of() == CompletionState::Dead);
        assert!(timer_called);

        loop_.deinit();
    }

    // test "io_uring: loop time"
    #[test]
    fn io_uring_loop_time() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // should never init zero
        let now = loop_.now();
        assert!(now > 0);

        // should update on a loop tick
        while now == loop_.now() {
            loop_.run(RunMode::NoWait).unwrap();
        }

        loop_.deinit();
    }

    // test "io_uring: stop"
    // `called` is mutated only through the raw userdata pointer inside the timer
    // callback (the intrusive-completion contract), which clippy's dataflow can't
    // see — the reference is `try testing.expect(!called)`.
    #[allow(clippy::while_immutable_condition)]
    #[test]
    fn io_uring_stop() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut called = false;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                1_000_000,
                ptr::addr_of_mut!(called).cast::<c_void>(),
                set_true_cb,
            );
        }

        // Tick
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!called);

        // Stop
        loop_.stop();
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(!called);

        loop_.deinit();
    }
}
