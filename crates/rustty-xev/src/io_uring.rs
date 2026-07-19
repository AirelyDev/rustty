//! io_uring.rs — faithful port of libxev `backend/io_uring.zig` (XEV_BLUEPRINT §1
//! Linux port, tasks L1 LT5 (ring + `Loop` core) and L1 LT6 (the remaining ops +
//! the watcher surfaces)).
//!
//! Backend to use io_uring. This is a SELF-CONTAINED whole backend, parallel to
//! `epoll.rs` (and the macOS `kqueue.rs`): it defines its own `Loop`/`Completion`/
//! `Operation`/`Result`/`Callback` locally and builds ONLY on the shared
//! cross-platform data structure (`queue`) and the Linux primitives/ABI enums
//! (`linux::{RunMode, CallbackAction, CompletionState, Options}`,
//! `linux::eventfd::Eventfd`, `linux::net`). It never imports from any
//! macOS-gated file (R5).
//!
//! Like `epoll.rs`, this backend also carries its OWN watcher surfaces —
//! `Async` (watcher/async.zig `AsyncEventFd`), `Process` (watcher/process.zig
//! `ProcessPidFd`) and `Stream`/`WriteQueue`/`WriteRequest` (watcher/stream.zig
//! `GenericStream`) — rather than importing the crate's macOS-gated `async.rs`/
//! `process.rs`/`stream.rs`. In the reference those files are `comptime`-generic
//! over `xev`; each backend gets its own monomorphization, and these are
//! io_uring's.
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

use core::any::TypeId;
use core::ffi::c_void;
use core::mem::size_of;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::linux::eventfd::Eventfd;
use crate::linux::net;
use crate::linux::{CallbackAction, CompletionState, Options, RunMode};
use crate::queue;
use crate::Error;

// PORT NOTE: `posix.fd_t` — the C file descriptor int.
pub type fd_t = libc::c_int;
/// PORT NOTE: `posix.socket_t` — the socket handle (a C int on Linux).
pub type socket_t = libc::c_int;
/// PORT NOTE: `posix.pid_t` — the process id.
pub type pid_t = libc::pid_t;

// PORT NOTE: `watcher/common.zig`'s `userdataValue` — the typed→untyped userdata
// bridge the io_uring `Async`/`Process`/`Stream` trampolines use. Kept local so
// this backend stays self-contained (the crate's `common.rs` is macOS-gated, R5),
// exactly as epoll.rs keeps its own copy. Zig `void` userdata ⇒ a null pointer;
// Rust's stand-in is the zero-sized unit `()`.
#[inline]
fn userdata_value<UD: 'static>(v: *mut c_void) -> *mut UD {
    if TypeId::of::<UD>() == TypeId::of::<()>() {
        return ptr::null_mut();
    }
    v as *mut UD
}

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
// stable; the still-deferred pread/pwrite ops reuse READ/WRITE when a consumer
// forces them — see the `Operation` PORT NOTE).
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

/// PORT NOTE: `io_uring_sqe.prep_sendmsg(fd, msghdr, flags)` — the msghdr POINTER
/// rides `addr` and `len` is 1 (the kernel's `io_sendmsg_prep` reads one
/// `user_msghdr`); `flags` rides `msg_flags` (the `rw_flags` union member).
///
/// # Safety
/// `sqe` live; `msghdr` (and everything it points at — `msg_name`, the `msg_iov`
/// array and each iovec's buffer) must stay valid until the op completes: the
/// kernel reads them asynchronously.
unsafe fn prep_sendmsg(sqe: *mut io_uring_sqe, fd: fd_t, msghdr: *const libc::msghdr, flags: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_SENDMSG, fd, msghdr as u64, 1, 0);
        (*sqe).rw_flags = flags;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_recvmsg(fd, msghdr, flags)`.
///
/// # Safety
/// `sqe` live; `msghdr` (and everything it points at) must stay valid+writable
/// until the op completes: the kernel reads AND writes them asynchronously.
unsafe fn prep_recvmsg(sqe: *mut io_uring_sqe, fd: fd_t, msghdr: *mut libc::msghdr, flags: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_RECVMSG, fd, msghdr as u64, 1, 0);
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

            Operation::SendMsg {
                fd, msghdr, buffer, ..
            } => unsafe {
                // if (v.buffer) |_| { @panic("TODO: sendmsg with buffer"); }
                //
                // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): the
                // buffer-populating path is UNIMPLEMENTED upstream and panics. Kept
                // as a panic rather than "fixed" (that would diverge, R1); no ported
                // watcher constructs a `SendMsg` with a buffer, so it is unreachable
                // in practice.
                if buffer.is_some() {
                    panic!("TODO: sendmsg with buffer");
                }

                prep_sendmsg(sqe, *fd, *msghdr, 0)
            },

            Operation::RecvMsg { fd, msghdr } => unsafe { prep_recvmsg(sqe, *fd, *msghdr, 0) },

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

    /// Submits a completion cancelation request.
    /// Results in error.NotFound if the completion couldn't be located because
    /// it was already completed or an invalid Completion was provided.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `cancel::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE (Zig comptime `cb` ⇒ a Rust zero-sized
    // type). One untyped loop trampoline per call-site; `loop`/`c` stay RAW. Same
    // shape as epoll.rs's `Process::wait` / `Stream::*`.
    //
    // PORT NOTE: `Loop.cancel` (io_uring.zig:565) is io_uring-ONLY — `epoll.zig`
    // has no such method (its cancellation test builds the `.cancel` op by hand,
    // which is why epoll.rs has none either). It is ported because the reference's
    // "socket read cancellation" test (io_uring.zig:1725) calls it. NOTE it ADDS
    // `c_cancel` itself (`self.add(c_cancel)`) — see that test's PORT NOTE about
    // the reference then adding it a second time.
    ///
    /// # Safety
    /// `c`/`c_cancel` must be valid, stable `Completion` pointers held raw by the
    /// loop until their callbacks fire.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn cancel<UD, CB>(
        &mut self,
        c: *mut Completion,
        c_cancel: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), CancelError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Loop::cancel cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c_cancel = Completion::new_op(
                Operation::Cancel { c },
                userdata.cast::<c_void>(),
                Self::cancel_trampoline::<UD, CB>,
            );
            self.add(c_cancel);
        }
    }

    /// The monomorphized untyped loop trampoline for `cancel`.
    fn cancel_trampoline<UD, CB>(
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
                core::result::Result<(), CancelError>,
            ) -> CallbackAction
            + 'static,
    {
        // if (r.cancel) |_| {} else |err| err
        let mapped: core::result::Result<(), CancelError> = match r {
            Result::Cancel(res) => res,
            _ => unreachable!(),
        };
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
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

    /// PORT NOTE: the `c.* = .{ .op, .userdata, .callback }` initialization the
    /// watcher `wait`/`read`/`write`/`close`/`poll` methods perform — a FULL reset
    /// with the internal fields (`next`/`flags`) taking their `Default` values.
    /// (Mirrors epoll.rs's `Completion::new_op`.)
    //
    // PORT NOTE (`pub(crate)`): `watcher/timer.zig`'s `TimerLoop.cancel`
    // (timer.zig:137-210) builds its `c_cancel` completion as `.{ .op = .{
    // .timer_remove = .{ .timer = c_timer } }, ... }` from OUTSIDE this backend
    // module (the io_uring arm of that `switch (xev.backend)`), so the watcher
    // needs this full-reset seam — exactly the visibility `kqueue.rs`'s twin
    // (kqueue.rs:1589) already carries for the macOS `timer.rs` watcher.
    pub(crate) fn new_op(op: Operation, userdata: *mut c_void, callback: Callback) -> Completion {
        Completion {
            op,
            userdata,
            callback,
            ..Completion::default()
        }
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

            Operation::SendMsg { .. } => Result::SendMsg(if res >= 0 {
                Ok(res as usize)
            } else {
                Err(match -res {
                    libc::ECANCELED => WriteError::Canceled,
                    libc::EPIPE => WriteError::BrokenPipe,
                    libc::ECONNRESET => WriteError::ConnectionResetByPeer,
                    _ => WriteError::Unexpected,
                })
            }),

            // PORT NOTE: `self.readResult(.recvmsg, res)` — the `recvmsg` op has NO
            // `buffer` field, so the Zig `@hasField(@TypeOf(active), "buffer")`
            // reflection is FALSE and a zero-byte result is an unconditional
            // `error.EOF`. That is exactly the `None` arm of `read_result`.
            Operation::RecvMsg { .. } => Result::RecvMsg(read_result(None, res)),

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
// The ported tag set mirrors epoll.rs's PLUS io_uring's own `timer_remove` (which
// `timer_reset` needs — epoll cancels via `Cancel`) and `sendmsg`/`recvmsg` (which
// io_uring's own reference tests exercise, unlike epoll's; `libc::msghdr`/`iovec`
// supply the surface epoll.rs lacked a reason to reach for).
//
// PORT NOTE: the reference's `pread`/`pwrite` tags stay DEFERRED exactly as they
// are in epoll.rs — no in-scope test or shell path constructs them, and both are
// pure `offset`-carrying variants of the already-ported `Read`/`Write` (the
// `prep_read`/`prep_write` helpers already take the `offset` argument, so landing
// them later is additive and reshapes nothing).
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

    /// Send a message on a socket using sendmsg (i.e. UDP).
    //
    // PORT NOTE: the reference field is `msghdr: *posix.msghdr_const`. `libc` has no
    // `msghdr_const` twin (Zig splits the type by constness; C/Rust do not), so the
    // constness rides the POINTER: `*const libc::msghdr`. `libc::msghdr` is the
    // glibc/kernel `struct user_msghdr` layout — the exact ABI the kernel's
    // `io_sendmsg_prep` reads through the SQE's `addr`.
    SendMsg {
        fd: fd_t,
        msghdr: *const libc::msghdr,

        /// Optionally, a write buffer can be specified and the given
        /// msghdr will be populated with information about this buffer.
        //
        // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): this field is
        // DEAD in the reference — `add_`'s `sendmsg` arm does
        // `if (v.buffer) |_| @panic("TODO: sendmsg with buffer");`, so the only
        // reachable value is `null`. The field (and `iov` below) are kept so the
        // op's shape matches the reference and so the panic arm is faithful; see
        // `add_`'s `SendMsg` arm.
        buffer: Option<WriteBuffer>,

        /// Do not use this, it is only used internally.
        //
        // PORT NOTE: `iov: [1]posix.iovec_const = undefined` — never written nor
        // read by the reference (it is the scratch the unimplemented `buffer` path
        // would populate). Kept for shape fidelity; `libc::iovec` is the same
        // layout as Zig's `iovec_const` (constness rides the pointer in C/Rust).
        iov: [libc::iovec; 1],
    },

    /// Recieve a message on a socket using recvmsg (i.e. UDP).
    RecvMsg { fd: fd_t, msghdr: *mut libc::msghdr },

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
    SendMsg(core::result::Result<usize, WriteError>),
    RecvMsg(core::result::Result<usize, ReadError>),
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

// ---------------------------------------------------------------------------
// Async (watcher/async.zig `AsyncEventFd`) — the cross-thread loop wakeup,
// INSIDE this backend. `Async(xev)` selects `AsyncEventFd` for the io_uring and
// epoll backends (async.zig:14-16), and its `wait` is the `waitPoll` arm
// (async.zig:86).
// ---------------------------------------------------------------------------

/// "Wake up" an event loop from any thread using an async completion.
//
// PORT NOTE: `AsyncEventFd(xev)` monomorphized for io_uring. `#[derive(Clone,
// Copy)]`: the reference `wait`/`notify` take `self: Self` by value (a trivially
// copyable fd handle), exactly like `linux::eventfd::Eventfd`.
//
// PORT NOTE: the reference's `fd: posix.fd_t` + inline `posix.eventfd`/`read`/
// `write` calls are expressed through the crate's already-ported
// `linux::eventfd::Eventfd` primitive (Task 2), which IS that fd plus the very
// same `eventfd(0, EFD_CLOEXEC|EFD_NONBLOCK)` / `write(1u64)` /
// `EAGAIN => success` surface. The FreeBSD `extern "c" fn eventfd` arm is
// comptime-dead here (this module is Linux-only).
#[derive(Clone, Copy)]
pub struct Async {
    /// eventfd file descriptor
    ev: Eventfd,
}

impl Async {
    /// Create a new async. An async can be assigned to exactly one loop
    /// to be woken up. The completion must be allocated in advance.
    pub fn init() -> core::result::Result<Async, Error> {
        match Eventfd::init() {
            Ok(ev) => Ok(Async { ev }),
            Err(_) => Err(Error),
        }
    }

    /// Clean up the async. This will forcibly deinitialize any resources
    /// and may result in erroneous wait callbacks to be fired.
    pub fn deinit(&self) {
        self.ev.deinit();
    }

    /// The underlying eventfd. Exposed so a caller can poll/register it directly.
    pub fn fd(&self) -> fd_t {
        self.ev.fd
    }

    /// Wait for a message on this async. Note that async messages may be
    /// coalesced (or they may not be) so you should not expect a 1:1 mapping
    /// between send and wait.
    ///
    /// Just like the rest of libxev, the wait must be re-queued if you want
    /// to continue to be notified of async events.
    ///
    /// You should NOT register an async with multiple loops (the same loop
    /// is fine -- but unnecessary). The behavior when waiting on multiple
    /// loops is undefined.
    //
    // PORT NOTE: `pub const wait = switch (xev.backend) { .io_uring, .epoll =>
    // waitPoll, ... }` — this IS `waitPoll`; the `waitRead` (kqueue) arm is
    // comptime-dead for io_uring.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `wait::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE. `WaitError = xev.ReadError`.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn wait<UD, CB>(
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
                core::result::Result<(), ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Async::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // PORT NOTE (verbatim from the reference): We use a poll operation
            // instead of a read operation because in Kernel 6.15.4, read was
            // regressed for io_uring on eventfd/timerfd and would block forever.
            // However, poll works fine.
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.ev.fd,
                    events: libc::POLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::wait_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn wait_trampoline<UD, CB>(
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
                core::result::Result<(), ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let mapped: core::result::Result<(), ReadError> = match r {
            Result::Poll(res) => res.map_err(|e| match e {
                // PORT NOTE: `WaitError = xev.ReadError`, but the io_uring `poll` op
                // yields a `PollError`. The reference's `if (r.poll) |_| {} else
                // |err| err` relies on Zig's error-set COERCION: `PollError`'s
                // `{Canceled, Unexpected}` are both members of `ReadError`
                // (`{EOF, Canceled, Unexpected, ConnectionResetByPeer}`), so the
                // coercion is total. Rust has no error-set subtyping, so the
                // same total mapping is spelled out.
                PollError::Canceled => ReadError::Canceled,
                PollError::Unexpected => ReadError::Unexpected,
            }),
            _ => unreachable!(),
        };

        if mapped.is_ok() {
            // PORT NOTE (verbatim): We need to read so that we can consume the
            // eventfd value. We only read 8 bytes because we only write up to 8
            // bytes and we own the fd. We ignore errors here because we expect the
            // read to succeed given we just polled it.
            let fd = match unsafe { &(*c_inner).op } {
                Operation::Poll { fd, .. } => *fd,
                _ => unreachable!(),
            };
            let mut buf = [0u8; 8];
            // SAFETY: an 8-byte read into a live local buffer off our own eventfd.
            // The result is discarded exactly as the reference's `_ = posix.read(
            // c_inner.op.poll.fd, &buf) catch {}` discards it.
            let _ = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<c_void>(), buf.len()) };
        }

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
    }

    /// Notify a loop to wake up synchronously. This should never block forever
    /// (it will always EVENTUALLY succeed regardless of if the loop is currently
    /// ticking or not).
    ///
    /// The "c" value is the completion associated with the "wait".
    ///
    /// Internal details subject to change but if you're relying on these
    /// details then you may want to consider using a lower level interface
    /// using the loop directly:
    ///
    ///   - linux+io_uring: eventfd is used. If the eventfd write would block
    ///     (EAGAIN) then we assume success because the eventfd is full.
    pub fn notify(&self) -> core::result::Result<(), Error> {
        // `Eventfd::notify` IS the reference's `write(fd, &@bitCast(@as(u64, 1)))`
        // plus its `error.WouldBlock => return` arm.
        self.ev.notify().map_err(|_| Error)
    }
}

// ---------------------------------------------------------------------------
// Process (watcher/process.zig `ProcessPidFd`) — the Linux subprocess-exit
// watcher, INSIDE this backend (mirroring epoll.rs). `Process(xev)` selects
// `ProcessPidFd` for io_uring and epoll alike (process.zig:13-16); the ONLY
// backend-dependent bit is the poll event mask (`.io_uring => posix.POLL.IN`
// vs `.epoll => linux.EPOLL.IN`, process.zig:80-84).
// ---------------------------------------------------------------------------

/// PORT NOTE: `pub const WaitError = xev.Sys.PollError || error{InvalidChild}`.
/// The error that can come in the wait callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessWaitError {
    InvalidChild,
    Unexpected,
}

/// Process implementation using pidfd (Linux).
//
// PORT NOTE: `ProcessPidFd` monomorphized. `#[derive(Clone, Copy)]`: the reference
// `wait` takes `self: Self` by value (a trivially-copyable `fd` handle).
#[derive(Clone, Copy)]
pub struct Process {
    /// pidfd file descriptor
    fd: fd_t,
}

impl Process {
    /// Create a new process watcher for the given pid.
    //
    // PORT NOTE: `linux.pidfd_open(pid, posix.SOCK.NONBLOCK)` (== PIDFD_NONBLOCK).
    // `pidfd_open` has no direct `libc` binding, so it goes through the raw
    // `SYS_pidfd_open` syscall. The reference maps each errno to a specific error;
    // the D6 seam collapses those to the unified `crate::Error` (identical to
    // epoll.rs's `Process::init`).
    pub fn init(pid: pid_t) -> core::result::Result<Process, Error> {
        // SAFETY: raw pidfd_open(2) syscall; returns a new fd or -1/errno.
        let res = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, libc::PIDFD_NONBLOCK) };
        if res < 0 {
            return Err(Error);
        }
        Ok(Process { fd: res as fd_t })
    }

    /// Clean up the process watcher.
    pub fn deinit(&mut self) {
        // SAFETY: closing our own pidfd.
        unsafe { libc::close(self.fd) };
    }

    /// Wait for the process to exit. This will automatically call
    /// `waitpid` or equivalent and report the exit status.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `wait::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE. One untyped loop trampoline per
    // call-site; `loop`/`c` stay RAW.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn wait<UD, CB>(
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
                core::result::Result<u32, ProcessWaitError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Process::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        // c.* = .{ .op = .{ .poll = { .fd, .events } }, ... };
        unsafe {
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    // io_uring arm: `const events: u32 = posix.POLL.IN;`
                    // (epoll's arm is `linux.EPOLL.IN`).
                    events: libc::POLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    /// The monomorphized untyped loop trampoline: on the pidfd signalling
    /// readable it reaps the child via `waitid(P_PIDFD)` then calls the typed `cb`.
    fn trampoline<UD, CB>(
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
                core::result::Result<u32, ProcessWaitError>,
            ) -> CallbackAction
            + 'static,
    {
        // const arg: WaitError!u32 = arg: { ... };
        let arg: core::result::Result<u32, ProcessWaitError> = 'arg: {
            // If our poll failed, report that error.
            // _ = r.poll catch |err| break :arg err;
            match r {
                Result::Poll(Ok(())) => {}
                Result::Poll(Err(_)) => break 'arg Err(ProcessWaitError::Unexpected),
                _ => unreachable!(),
            }

            // We need to wait on the pidfd because it is noted as ready.
            // const fd = c_inner.op.poll.fd;
            let fd = match unsafe { &(*c_inner).op } {
                Operation::Poll { fd, .. } => *fd,
                _ => unreachable!(),
            };
            // var info: siginfo_t; res = waitid(.PIDFD, fd, &info, W.EXITED);
            let mut info: libc::siginfo_t = unsafe { core::mem::zeroed() };
            let res =
                unsafe { libc::waitid(libc::P_PIDFD, fd as libc::id_t, &mut info, libc::WEXITED) };
            if res == 0 {
                // .SUCCESS => info.fields.common.second.sigchld.status
                break 'arg Ok(unsafe { info.si_status() } as u32);
            }
            match errno() {
                libc::ECHILD => Err(ProcessWaitError::InvalidChild),
                // The fd isn't ready to read, I guess?
                libc::EAGAIN => return CallbackAction::Rearm,
                _ => Err(ProcessWaitError::Unexpected),
            }
        };

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, arg)
    }
}

// ---------------------------------------------------------------------------
// Stream / WriteQueue / WriteRequest (watcher/stream.zig `GenericStream`) — the
// pty read/write engine, INSIDE this backend. Monomorphized for the io_uring
// `GenericStream` config: `{ .close = true, .poll = true, .read = .read,
// .write = .write, .threadpool = false }`.
//
// PORT NOTE (vs. epoll.rs's Stream): every one of stream.zig's per-backend
// `switch (xev.backend)` completion-flag blocks has an EMPTY `.io_uring` arm
// (stream.zig:566-570 read, 421-425 close, 980-984 writeInit) — io_uring needs
// neither epoll's `dup` nor its `threadpool` flag, because the ring performs the
// syscall itself rather than waking us to run it. Hence this `Completion` carries
// no such flags and these methods set none.
// ---------------------------------------------------------------------------

/// WriteQueue is the queue of write requests for ordered writes.
//
// PORT NOTE: `pub const WriteQueue = queue.Intrusive(WriteRequest);` — the
// hand-written `Default` (head/tail = null) comes from `queue::Intrusive`.
pub type WriteQueue = queue::Intrusive<WriteRequest>;

/// WriteRequest is a single request for a write. It wraps a
/// completion so that it can be inserted into the WriteQueue.
pub struct WriteRequest {
    pub completion: Completion,
    pub userdata: *mut c_void,

    /// This is the original buffer passed to queueWrite. We have
    /// to keep track of this because we may be forced to split
    /// the write or rearm the write due to partial writes, but when
    /// we call the final callback we want to pass the original
    /// complete buffer.
    pub full_write_buffer: WriteBuffer,

    next: *mut WriteRequest,
}

// PORT NOTE: hand-written `Default` (NOT `#[derive]`) — `full_write_buffer` has no
// Zig default (it is `undefined` in the pool element, fully overwritten by
// `queueWrite`). The placeholder empty `Array` is never observed.
impl Default for WriteRequest {
    fn default() -> Self {
        WriteRequest {
            completion: Completion::default(),
            userdata: ptr::null_mut(),
            full_write_buffer: WriteBuffer::Array {
                array: [0u8; 32],
                len: 0,
            },
            next: ptr::null_mut(),
        }
    }
}

// PORT NOTE: intrusive `next` link seam for `queue::Intrusive<WriteRequest>`.
impl queue::Node for WriteRequest {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

// PORT NOTE: `WriteRequest` holds raw-ptr fields + embeds `Completion` (fn-ptr) →
// auto-`!Send`; it rides the `Loop` moved onto the termio thread. SAFETY /
// single-thread affinity: after the move a request is touched by exactly the
// owning event-loop thread, so transferring ownership is sound.
unsafe impl Send for WriteRequest {}

impl WriteRequest {
    /// This can be used to convert a completion pointer back to
    /// a WriteRequest.
    //
    // PORT NOTE: `return @fieldParentPtr("completion", c);` — `container_of`
    // (offset-of) recovery of the enclosing `WriteRequest`.
    pub fn from(c: *mut Completion) -> *mut WriteRequest {
        unsafe {
            (c as *mut u8).sub(core::mem::offset_of!(WriteRequest, completion)) as *mut WriteRequest
        }
    }
}

/// Events that can be polled for using the high level streams (io_uring arm:
/// `read = std.posix.POLL.IN`).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollEvent {
    Read,
}

/// PORT NOTE: `GenericStream(io_uring)`. `#[derive(Clone, Copy)]`: the reference
/// `read`/`write`/`close`/`poll`/`queueWrite` take `self: Self` by value.
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
    pub fn deinit(&self) {
        let _ = self;
    }

    // -------------------------------------------------------------------
    // Readable (stream.zig:443). `options.read == .read` for GenericStream.
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

        unsafe {
            *c = Completion::new_op(
                Operation::Read {
                    fd: self.fd,
                    buffer: buf,
                },
                userdata.cast::<c_void>(),
                Self::read_trampoline::<UD, CB>,
            );
            // io_uring arm of the flags switch: `{}` (nothing to set).
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
                Stream,
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
            Stream::init_fd(fd),
            buffer,
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Writeable (stream.zig:620). `options.write == .write` for GenericStream.
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
                Stream,
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

        let cb_res = Stream::write_result(c_inner, r);
        let mut result: core::result::Result<usize, WriteError> = cb_res.result;

        // Checks whether the entire buffer was written; if the write was partial,
        // it re-submits the remainder of the buffer.
        let queued_len = Stream::write_buffer_length(cb_res.buf);
        if let Ok(written_len) = cb_res.result {
            if written_len < queued_len {
                // Write remainder of the buffer, reusing the same completion.
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
            cb_res.writer,
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

    /// Extracts the result from a completion for a write callback (`.write` arm).
    fn write_result(c: *mut Completion, r: Result) -> WriteResult {
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

    /// Initialize the completion c for a write. This does NOT set userdata or a
    /// callback.
    //
    // PORT NOTE: io_uring arm of the flags switch is `{}` (epoll's sets `dup`).
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
        }
    }

    /// Returns the length of the write buffer.
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
                let new_ptr = unsafe { slice.cast::<u8>().add(offset) };
                WriteBuffer::Slice(ptr::slice_from_raw_parts(new_ptr, slice.len() - offset))
            }
            WriteBuffer::Array { array, len } => {
                assert!(offset <= len);
                let rem_len = len - offset;
                let mut wb_array = [0u8; 32];
                wb_array[0..rem_len].copy_from_slice(&array[offset..offset + rem_len]);
                WriteBuffer::Array {
                    array: wb_array,
                    len: rem_len,
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // Closeable (stream.zig:319). `options.close == true` for GenericStream.
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
                Stream,
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
            Stream::init_fd(fd),
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Pollable (stream.zig:139). `options.poll == true` for GenericStream.
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
                Stream,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Poll { fd, .. } => *fd,
            _ => unreachable!(),
        };
        // io_uring arm: `if (result.poll) |_| @enumFromInt(c.op.poll.events) else
        // |err| err`. GenericStream always polls POLL.IN, which maps to
        // `PollEvent::Read`.
        let mapped: core::result::Result<PollEvent, PollError> = match r {
            Result::Poll(Ok(())) => Ok(PollEvent::Read),
            Result::Poll(Err(e)) => Err(e),
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
}

/// PORT NOTE: the anonymous return struct of `write_result`.
struct WriteResult {
    writer: Stream,
    buf: WriteBuffer,
    result: core::result::Result<usize, WriteError>,
}

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
    pub unsafe fn accept<UD, CB>(self, loop_: *mut Loop, c: *mut Completion, userdata: *mut UD, cb: CB)
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
        cb(userdata_value::<UD>(ud), l_inner, c_inner, writer, buf, result)
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

// ---------------------------------------------------------------------------
// UDP (watcher/udp.zig `UDPSendMsg`) — the per-backend (`io_uring`) instantiation
// of the datagram socket. On both Linux backends `UDP(xev)` selects `UDPSendMsg`
// (udp.zig:20-24), which uses `sendmsg`/`recvmsg` exclusively; this is the
// io_uring arm (byte-identical to epoll's apart from `init` omitting NONBLOCK and
// the dup/threadpool flag switches collapsing to `{}`). The Dynamic arm lives in
// `dynamic.rs`.
//
// PORT NOTE (ownership shape): the msghdr + its iovec + the address storage are
// embedded in the caller-owned `UdpState` (udp.zig:405-422) and the msghdr's
// `msg_name`/`msg_iov` point back INTO that same `UdpState` — a self-referential
// struct. `read`/`write` therefore write `*s` FIRST (establishing stable field
// addresses) and only then fix up those interior pointers; the caller must keep
// `s` pinned for the whole in-flight op (exactly the Zig `s: *State` contract).
// The KERNEL reads/writes the msghdr asynchronously (io_uring), so `s` must
// outlive the op even more literally here than on epoll.
// ---------------------------------------------------------------------------

/// UDP requires some extra state to perform operations. The state is
/// opaque. This isn't part of `Completion` because it is relatively
/// large and would force ALL operations (not just UDP) to have a relatively
/// large structure size and we didn't want to pay that cost.
//
// PORT NOTE: the reference `State.op` is an UNTAGGED `union { recv, send }` whose
// active arm is tracked by the in-flight operation; Rust models it as a safe
// tagged `enum` (`UdpStateOp`). `userdata` is the reference's `?*anyopaque = null`.
pub struct UdpState {
    userdata: *mut c_void,
    op: UdpStateOp,
}

// PORT NOTE: `libc` has a single `msghdr`/`iovec` (constness rides the pointer in
// C/Rust), so both arms name `libc::msghdr`/`libc::iovec`; the send path only
// ever READS through them.
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
    /// `read`/`write` overwrite it wholesale. Mirrors `var s: State = undefined;`.
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
// passed BY VALUE everywhere.
#[derive(Clone, Copy)]
pub struct UDP {
    fd: socket_t,
}

impl UDP {
    /// Initialize a new UDP with the family from the given address. Only
    /// the family is used, the actual address has no impact on the created
    /// resource.
    //
    // PORT NOTE: "On io_uring we don't use non-blocking sockets because we may just
    // get EAGAIN over and over from completions" — the io_uring arm OMITS NONBLOCK.
    pub fn init(addr: net::Address) -> core::result::Result<UDP, Error> {
        // SAFETY: reading `any.family` is valid for any active `Address` member
        // (all overlay `family` as their first field).
        let family = unsafe { addr.any }.family as libc::c_int;
        let flags = libc::SOCK_DGRAM | libc::SOCK_CLOEXEC;
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
            // io_uring arm of the dup switch: `{}` (nothing to set).
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
            // io_uring arm: `.poll = { .fd, .events = posix.POLL.IN }`.
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
//
// RE-MEASURED (Task 6, after adding the `SendMsg`/`RecvMsg` ops): UNCHANGED at
// 152. `Connect` still dominates the `Operation` union and nothing was added to
// `Completion` itself. Measured on aarch64-unknown-linux-gnu:
//   Completion = 152, Operation = 120, net::Address = 112,
//   Option<WriteBuffer> = 48, libc::iovec = 16
// so `Connect` (4 + 112, +tag → 120) still outweighs the widest new arm,
// `SendMsg` (msghdr 8 + buffer 48 + iov 16 + fd 4, +tag/padding → 88).
const COMPLETION_SIZE: usize = 152;
const _: () = assert!(size_of::<Completion>() == COMPLETION_SIZE);

// ---------------------------------------------------------------------------
// Tests (io_uring.zig `test{}` blocks). The FULL reference set: available /
// overflow entries count / Completion size / default completion / loop time /
// stop (L1 LT5) + timerfd / timer / timer reset / timer remove / socket
// accept-connect-send-recv-close / sendmsg-recvmsg / socket read cancellation
// (L1 LT6), plus the two Linux live KATs (process spawn+exit, pty write
// round-trip) that mirror epoll.rs's. Callbacks are plain fn items matching the
// loop `Callback` ABI; completions/userdata are stack locals reached through raw
// pointers (the intrusive-ownership contract).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::timerfd::Timerfd;
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

    // -----------------------------------------------------------------------
    // L1 LT6: the timer family (io_uring.zig:1173/1226/1272/1357).
    // -----------------------------------------------------------------------

    // test "io_uring: timerfd" (io_uring.zig:1173)
    #[test]
    fn io_uring_timerfd() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // We'll try with a simple timerfd
        let t = Timerfd::init().unwrap();
        // The reference arms a 1ns timer (`.{ .value = .{ .nanoseconds = 1 } }`);
        // our Task-2 `set` is ms-granularity, so we arm the shortest one-shot it
        // exposes (1ms). Observably identical: the timerfd becomes readable
        // near-instantly and `run(until_done)` waits. (Same divergence note as
        // epoll.rs's `epoll_timerfd`.)
        t.set(1).unwrap();

        // Add the timer.
        //
        // PORT NOTE (verbatim from the reference): we should be able to use `read`
        // here but on Kernel 6.15.4 there is a bug that prevents the read from ever
        // firing with io_uring. I don't know why. I changed this to a poll so tests
        // pass, which should also be fine!
        let mut called = false;
        let mut c = Completion {
            op: Operation::Poll {
                fd: t.fd,
                // The reference relies on the Zig field default `events =
                // posix.POLL.IN`; Rust enum variants carry no field defaults, so
                // the default value is spelled at the call site.
                events: libc::POLLIN as u32,
            },
            userdata: ptr::addr_of_mut!(called).cast::<c_void>(),
            callback: set_true_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c)) };

        // Verify states
        assert!(c.state_of() == CompletionState::Active);

        // Tick
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(called);
        assert!(c.state_of() == CompletionState::Dead);

        t.deinit();
        loop_.deinit();
    }

    // test "io_uring: timer" (io_uring.zig:1226)
    #[test]
    fn io_uring_timer() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut called = false;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                1,
                ptr::addr_of_mut!(called).cast::<c_void>(),
                set_true_cb,
            );
        }

        // Add another timer
        let mut called2 = false;
        let mut c2 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c2),
                100_000,
                ptr::addr_of_mut!(called2).cast::<c_void>(),
                set_true_cb,
            );
        }

        // State checking
        assert!(c1.state_of() == CompletionState::Active);
        assert!(c2.state_of() == CompletionState::Active);

        // Tick
        // `called`/`called2` are mutated only through the raw userdata pointer
        // inside the timer callback (the intrusive-completion contract), which
        // clippy's dataflow can't see — the reference is `while (!called) try
        // loop.run(.no_wait);`.
        #[allow(clippy::while_immutable_condition)]
        while !called {
            loop_.run(RunMode::NoWait).unwrap();
        }
        assert!(called);
        assert!(!called2);

        // State checking
        assert!(c1.state_of() == CompletionState::Dead);
        assert!(c2.state_of() == CompletionState::Active);

        loop_.deinit();
    }

    // PORT NOTE: the shared `v.* = r.timer catch unreachable; return .disarm;`
    // callback the "timer reset"/"timer remove" tests use.
    fn trigger_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        let v = ud as *mut Option<TimerTrigger>;
        match r {
            Result::Timer(Ok(t)) => unsafe { *v = Some(t) },
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }

    // test "io_uring: timer reset" (io_uring.zig:1272)
    #[test]
    fn io_uring_timer_reset() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        let cb: Callback = trigger_cb;

        // Add the timer
        let mut trigger: Option<TimerTrigger> = None;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                100_000,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                cb,
            );
        }

        // We know timer won't be called from the timer test previously.
        loop_.run(RunMode::NoWait).unwrap();
        assert!(trigger.is_none());

        // Reset the timer
        let mut c_cancel = Completion::default();
        unsafe {
            loop_.timer_reset(
                ptr::addr_of_mut!(c1),
                ptr::addr_of_mut!(c_cancel),
                1,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                cb,
            );
        }
        assert!(c1.state_of() == CompletionState::Active);
        assert!(c_cancel.state_of() == CompletionState::Active);

        // Run
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(trigger.unwrap() == TimerTrigger::Expiration);
        assert!(c1.state_of() == CompletionState::Dead);
        assert!(c_cancel.state_of() == CompletionState::Dead);

        loop_.deinit();
    }

    // test "io_uring: timer remove" (io_uring.zig:1357)
    #[test]
    fn io_uring_timer_remove() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut trigger: Option<TimerTrigger> = None;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                100_000,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                trigger_cb,
            );
        }

        // Remove it
        fn timer_remove_cb(
            _ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            // _ = r.timer_remove catch unreachable;
            match r {
                Result::TimerRemove(Ok(())) => {}
                _ => unreachable!(),
            }
            CallbackAction::Disarm
        }
        let mut c_remove = Completion {
            op: Operation::TimerRemove {
                timer: ptr::addr_of_mut!(c1),
            },
            userdata: ptr::null_mut(),
            callback: timer_remove_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_remove)) };

        // Tick
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(trigger.unwrap() == TimerTrigger::Cancel);

        loop_.deinit();
    }

    // -----------------------------------------------------------------------
    // L1 LT6: the socket family (io_uring.zig:1401/1626/1725) + the Linux live
    // KATs. These run REAL syscalls on the live io_uring ring (Miri can't).
    // -----------------------------------------------------------------------

    use crate::linux::net;

    // PORT NOTE (test-harness divergence, ports): every reference socket test binds
    // the FIXED port 3131. Zig's test runner executes a file's tests SERIALLY, so
    // the reference's three socket tests never collide. Rust's harness runs the
    // WHOLE CRATE's tests in PARALLEL threads of one process — so io_uring's socket
    // tests would race BOTH each other AND `epoll.rs`'s `epoll_socket_roundtrip`
    // (which also binds TCP 3131 with SO_REUSEADDR|SO_REUSEPORT: a second listener
    // would bind successfully and steal connections). Each io_uring socket test
    // therefore gets its own dedicated port. Nothing else about the tests changes.
    const PORT_SOCKET_ROUNDTRIP: u16 = 3132;
    const PORT_SENDMSG_RECVMSG: u16 = 3133;
    const PORT_READ_CANCELLATION: u16 = 3134;
    const PORT_UDP_READ_WRITE: u16 = 3136;

    fn write_slice(buf: &[u8]) -> WriteBuffer {
        WriteBuffer::Slice(ptr::slice_from_raw_parts(buf.as_ptr(), buf.len()))
    }
    fn read_slice(buf: &mut [u8]) -> ReadBuffer {
        ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(buf.as_mut_ptr(), buf.len()))
    }

    // PORT NOTE: `posix.socket(family, ty, flags)`; the socket test creates a
    // blocking listener + a NONBLOCK client (SOCK.CLOEXEC on both).
    unsafe fn new_socket(family: libc::c_int, ty: libc::c_int) -> socket_t {
        let fd = unsafe { libc::socket(family, ty, 0) };
        assert!(fd >= 0);
        fd
    }

    unsafe fn set_reuse(fd: socket_t, opt: libc::c_int) {
        let one: libc::c_int = 1;
        let rc = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                opt,
                ptr::addr_of!(one).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            )
        };
        assert_eq!(rc, 0);
    }

    // test "io_uring: socket accept/connect/send/recv/close" (io_uring.zig:1401)
    #[test]
    fn io_uring_socket_roundtrip() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Create a TCP server socket
        let address = net::Address::parse_ip4(b"127.0.0.1", PORT_SOCKET_ROUNDTRIP).unwrap();
        let kernel_backlog = 1;
        let family = unsafe { address.any.family } as libc::c_int;
        let ln = unsafe { new_socket(family, libc::SOCK_STREAM | libc::SOCK_CLOEXEC) };
        unsafe {
            set_reuse(ln, libc::SO_REUSEADDR);
            assert_eq!(
                libc::bind(
                    ln,
                    ptr::addr_of!(address.any) as *const libc::sockaddr,
                    address.get_os_sock_len()
                ),
                0
            );
            assert_eq!(libc::listen(ln, kernel_backlog), 0);
        }

        // Create a TCP client socket
        let mut client_conn = unsafe { new_socket(family, libc::SOCK_STREAM | libc::SOCK_CLOEXEC) };

        // Accept
        let mut server_conn: socket_t = 0;
        fn accept_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let conn = ud as *mut socket_t;
            // conn.* = r.accept catch unreachable;
            match r {
                Result::Accept(Ok(v)) => unsafe { *conn = v },
                _ => unreachable!(),
            }
            CallbackAction::Disarm
        }
        let mut c_accept = Completion {
            op: Operation::Accept {
                socket: ln,
                // The reference relies on the Zig field defaults
                // (`addr = undefined`, `addr_size = @sizeOf(posix.sockaddr)`,
                // `flags = posix.SOCK.CLOEXEC`); spelled at the call site here.
                addr: unsafe { core::mem::zeroed() },
                addr_size: size_of::<net::sockaddr::Any>() as net::socklen_t,
                flags: SOCK_CLOEXEC,
            },
            userdata: ptr::addr_of_mut!(server_conn).cast::<c_void>(),
            callback: accept_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_accept)) };

        // Connect
        let mut connected = false;
        fn connect_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            // _ = r.connect catch unreachable;
            match r {
                Result::Connect(Ok(())) => {}
                _ => unreachable!(),
            }
            unsafe { *(ud as *mut bool) = true };
            CallbackAction::Disarm
        }
        let mut c_connect = Completion {
            op: Operation::Connect {
                socket: client_conn,
                addr: address,
            },
            userdata: ptr::addr_of_mut!(connected).cast::<c_void>(),
            callback: connect_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_connect)) };

        // Wait for the connection to be established
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn > 0);
        assert!(connected);

        // Send
        let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
        fn send_cb(
            _ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            // _ = r.send catch unreachable;
            match r {
                Result::Send(Ok(_)) => {}
                _ => unreachable!(),
            }
            CallbackAction::Disarm
        }
        let mut c_send = Completion {
            op: Operation::Send {
                fd: client_conn,
                buffer: write_slice(&send_buf),
            },
            callback: send_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_send)) };

        // Receive
        let mut recv_buf = [0u8; 128];
        let mut recv_len: usize = 0;
        fn recv_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let p = ud as *mut usize;
            // ptr.* = r.recv catch unreachable;
            match r {
                Result::Recv(Ok(n)) => unsafe { *p = n },
                _ => unreachable!(),
            }
            CallbackAction::Disarm
        }
        let mut c_recv = Completion {
            op: Operation::Recv {
                fd: server_conn,
                buffer: read_slice(&mut recv_buf),
            },
            userdata: ptr::addr_of_mut!(recv_len).cast::<c_void>(),
            callback: recv_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_recv)) };

        // Wait for the send/receive
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

        // Shutdown
        let mut shutdown = false;
        fn shutdown_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            // _ = r.shutdown catch unreachable;
            match r {
                Result::Shutdown(Ok(())) => {}
                _ => unreachable!(),
            }
            unsafe { *(ud as *mut bool) = true };
            CallbackAction::Disarm
        }
        let mut c_client_shutdown = Completion {
            op: Operation::Shutdown {
                socket: client_conn,
                // Zig field default: `how = .both`.
                how: ShutdownHow::Both,
            },
            userdata: ptr::addr_of_mut!(shutdown).cast::<c_void>(),
            callback: shutdown_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_client_shutdown)) };
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(shutdown);

        // Read should be EOF
        let mut eof: Option<bool> = None;
        fn eof_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let p = ud as *mut Option<bool>;
            let v = match r {
                Result::Recv(Ok(_)) => false,
                Result::Recv(Err(ReadError::EOF)) => true,
                Result::Recv(Err(_)) => false,
                _ => unreachable!(),
            };
            unsafe { *p = Some(v) };
            CallbackAction::Disarm
        }
        c_recv = Completion {
            op: Operation::Recv {
                fd: server_conn,
                buffer: read_slice(&mut recv_buf),
            },
            userdata: ptr::addr_of_mut!(eof).cast::<c_void>(),
            callback: eof_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_recv)) };

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(eof.unwrap());

        // Close
        fn close_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            // _ = r.close catch unreachable;
            match r {
                Result::Close(Ok(())) => {}
                _ => unreachable!(),
            }
            unsafe { *(ud as *mut socket_t) = 0 };
            CallbackAction::Disarm
        }
        let mut c_client_close = Completion {
            op: Operation::Close { fd: client_conn },
            userdata: ptr::addr_of_mut!(client_conn).cast::<c_void>(),
            callback: close_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_client_close)) };

        let mut ln_var = ln;
        let mut c_server_close = Completion {
            op: Operation::Close { fd: ln },
            userdata: ptr::addr_of_mut!(ln_var).cast::<c_void>(),
            callback: close_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_server_close)) };

        // Wait for the sockets to close
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(ln_var == 0);
        assert!(client_conn == 0);

        // The accepted server connection is not closed by the reference test
        // either; close it so the crate's (long-lived, many-test) process doesn't
        // leak the fd.
        unsafe { libc::close(server_conn) };

        loop_.deinit();
    }

    // -----------------------------------------------------------------------
    // TCP watcher tests (watcher/tcp.zig `TCPTests`). Ported for the io_uring
    // backend instantiation (mirrors epoll.rs's). Unlike epoll, io_uring's TCP
    // `close` is a real SQE (no `.threadpool` flag), so the loop needs NO thread
    // pool. The `test "TCP: Stream decls"` reflection test (tcp.zig:506) is a Zig
    // comptime `@hasDecl` sweep with no Rust runtime analog (enforced at COMPILE
    // time by `TCP` carrying every method), so it is not a separate runtime test.
    // -----------------------------------------------------------------------

    // test "TCP: accept/connect/send/recv/close" (tcp.zig:518).
    #[test]
    fn io_uring_tcp_accept_connect_send_recv_close() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Choose random available port (Zig #14907)
        let mut address = net::Address::parse_ip4(b"127.0.0.1", 0).unwrap();
        let server = TCP::init(address).unwrap();

        // Bind and listen
        server.bind(address).unwrap();
        server.listen(1).unwrap();

        // Retrieve bound port and initialize client
        let mut sock_len = address.get_os_sock_len();
        unsafe {
            libc::getsockname(
                server.fd(),
                ptr::addr_of_mut!(address.any) as *mut libc::sockaddr,
                ptr::addr_of_mut!(sock_len),
            );
        }
        let client = TCP::init(address).unwrap();

        let mut c_accept = Completion::default();
        let mut c_connect = Completion::default();

        // Accept
        let mut server_conn: Option<TCP> = None;
        unsafe {
            server.accept(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_conn),
                |ud: *mut Option<TCP>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 r: core::result::Result<TCP, AcceptError>| {
                    *ud = Some(r.unwrap());
                    CallbackAction::Disarm
                },
            );
        }

        // Connect
        let mut connected = false;
        unsafe {
            client.connect(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                address,
                ptr::addr_of_mut!(connected),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), ConnectError>| {
                    r.unwrap();
                    *ud = true;
                    CallbackAction::Disarm
                },
            );
        }

        // Wait for the connection to be established
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn.is_some());
        assert!(connected);

        // Close the server
        let mut server_closed = false;
        unsafe {
            server.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_closed),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = true;
                    CallbackAction::Disarm
                },
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_closed);

        // Send
        let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
        unsafe {
            client.write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                write_slice(&send_buf),
                ptr::null_mut::<()>(),
                |_ud: *mut (),
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 _b: WriteBuffer,
                 r: core::result::Result<usize, WriteError>| {
                    r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        // Receive
        let mut recv_buf = [0u8; 128];
        let mut recv_len: usize = 0;
        let server_conn_tcp = server_conn.unwrap();
        unsafe {
            server_conn_tcp.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                read_slice(&mut recv_buf),
                ptr::addr_of_mut!(recv_len),
                |ud: *mut usize,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 _b: ReadBuffer,
                 r: core::result::Result<usize, ReadError>| {
                    *ud = r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        // Wait for the send/receive
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

        // Close
        unsafe {
            server_conn_tcp.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_conn),
                |ud: *mut Option<TCP>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = None;
                    CallbackAction::Disarm
                },
            );
            client.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                ptr::addr_of_mut!(connected),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = false;
                    CallbackAction::Disarm
                },
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn.is_none());
        assert!(!connected);
        assert!(server_closed);

        loop_.deinit();
    }

    // test "TCP: Queued writes" (tcp.zig:697).
    #[test]
    fn io_uring_tcp_queued_writes() {
        const PATTERN: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0];
        const SEND_LEN: usize = PATTERN.len() * 100_000;
        let mut send_buf: Vec<u8> = Vec::with_capacity(SEND_LEN);
        for _ in 0..100_000 {
            send_buf.extend_from_slice(&PATTERN);
        }

        let mut loop_ = Loop::init(Options::default()).unwrap();

        let mut address = net::Address::parse_ip4(b"127.0.0.1", 0).unwrap();
        let server = TCP::init(address).unwrap();
        server.bind(address).unwrap();
        server.listen(1).unwrap();

        let mut sock_len = address.get_os_sock_len();
        unsafe {
            libc::getsockname(
                server.fd(),
                ptr::addr_of_mut!(address.any) as *mut libc::sockaddr,
                ptr::addr_of_mut!(sock_len),
            );
        }
        let client = TCP::init(address).unwrap();

        let mut c_accept = Completion::default();
        let mut c_connect = Completion::default();

        // Accept
        let mut server_conn: Option<TCP> = None;
        unsafe {
            server.accept(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_conn),
                |ud: *mut Option<TCP>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 r: core::result::Result<TCP, AcceptError>| {
                    *ud = Some(r.unwrap());
                    CallbackAction::Disarm
                },
            );
        }

        // Connect
        let mut connected = false;
        unsafe {
            client.connect(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                address,
                ptr::addr_of_mut!(connected),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), ConnectError>| {
                    r.unwrap();
                    *ud = true;
                    CallbackAction::Disarm
                },
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn.is_some());
        assert!(connected);

        // Close the server (listener)
        let mut server_closed = false;
        unsafe {
            server.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_closed),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = true;
                    CallbackAction::Disarm
                },
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_closed);

        // Unqueued send - Limit send buffer to 8kB, this should force partial writes.
        let sndbuf: libc::c_int = 8192;
        unsafe {
            libc::setsockopt(
                client.fd(),
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                ptr::addr_of!(sndbuf).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            );
        }

        // First we try to send the whole 1MB buffer in one write operation, this
        // _should_ result in a partial write.
        let mut sent_unqueued: usize = 0;
        unsafe {
            client.write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                write_slice(&send_buf),
                ptr::addr_of_mut!(sent_unqueued),
                |ud: *mut usize,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 _b: WriteBuffer,
                 r: core::result::Result<usize, WriteError>| {
                    *ud = r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(sent_unqueued < (send_buf.len() / 10));

        // Set up queued write for the remainder; shutdown() the socket afterwards.
        let mut w_queue = WriteQueue::default();
        let mut wr_send = WriteRequest::default();
        let mut sent_queued: usize = 0;
        let queued_slice = write_slice(&send_buf[sent_unqueued..]);
        unsafe {
            client.queue_write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(w_queue),
                ptr::addr_of_mut!(wr_send),
                queued_slice,
                ptr::addr_of_mut!(sent_queued),
                |ud: *mut usize,
                 l: *mut Loop,
                 c: *mut Completion,
                 tcp: TCP,
                 _b: WriteBuffer,
                 r: core::result::Result<usize, WriteError>| {
                    *ud = r.unwrap();
                    tcp.shutdown(
                        l,
                        c,
                        ptr::null_mut::<()>(),
                        |_ud: *mut (),
                         _l: *mut Loop,
                         _c: *mut Completion,
                         _s: TCP,
                         _r: core::result::Result<(), ShutdownError>| {
                            CallbackAction::Disarm
                        },
                    );
                    CallbackAction::Disarm
                },
            );
        }

        // Set up receiver which keeps reading until it reads the full send buffer.
        struct Receiver {
            loop_: *mut Loop,
            conn: TCP,
            completion: Completion,
            buf: Vec<u8>,
            bytes_read: usize,
        }
        impl Receiver {
            fn read(&mut self) {
                if self.bytes_read == self.buf.len() {
                    return;
                }
                let start = self.bytes_read;
                let len = self.buf.len() - start;
                let read_buf = ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(
                    unsafe { self.buf.as_mut_ptr().add(start) },
                    len,
                ));
                let loop_ = self.loop_;
                let comp = ptr::addr_of_mut!(self.completion);
                let self_ptr = self as *mut Receiver;
                unsafe {
                    self.conn.read(loop_, comp, read_buf, self_ptr, Self::read_cb);
                }
            }

            fn read_cb(
                ud: *mut Receiver,
                _l: *mut Loop,
                _c: *mut Completion,
                _s: TCP,
                _b: ReadBuffer,
                r: core::result::Result<usize, ReadError>,
            ) -> CallbackAction {
                let receiver = unsafe { &mut *ud };
                let n_bytes = r.unwrap();
                receiver.bytes_read += n_bytes;
                if receiver.bytes_read < receiver.buf.len() {
                    receiver.read();
                }
                CallbackAction::Disarm
            }
        }
        let mut receiver = Receiver {
            loop_: ptr::addr_of_mut!(loop_),
            conn: server_conn.unwrap(),
            completion: Completion::default(),
            buf: vec![0u8; SEND_LEN],
            bytes_read: 0,
        };
        receiver.read();

        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(&send_buf[..], &receiver.buf[0..receiver.bytes_read]);
        assert_eq!(send_buf.len(), sent_unqueued + sent_queued);

        // Close
        let server_conn_tcp = server_conn.unwrap();
        unsafe {
            server_conn_tcp.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_conn),
                |ud: *mut Option<TCP>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = None;
                    CallbackAction::Disarm
                },
            );
            client.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                ptr::addr_of_mut!(connected),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = false;
                    CallbackAction::Disarm
                },
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn.is_none());
        assert!(!connected);
        assert!(server_closed);

        loop_.deinit();
    }

    // -----------------------------------------------------------------------
    // UDP watcher tests (watcher/udp.zig `UDPTests`). Ported for the io_uring
    // backend instantiation. The `test "UDP: Stream decls"` reflection test
    // (udp.zig:902) is a Zig comptime `@hasDecl` sweep with no Rust runtime
    // analog (enforced at COMPILE time by `UDP` carrying every method). Unlike
    // epoll, io_uring's `UDP::close` is a real SQE (no `.threadpool` flag), so the
    // loop needs NO thread pool.
    // -----------------------------------------------------------------------

    // test "UDP: read/write" (udp.zig:914). Dedicated port so parallel cargo runs
    // cannot cross-route datagrams (SO_REUSEPORT).
    #[test]
    fn io_uring_udp_read_write() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        let address = net::Address::parse_ip4(b"127.0.0.1", PORT_UDP_READ_WRITE).unwrap();
        let server = UDP::init(address).unwrap();
        let client = UDP::init(address).unwrap();

        // Bind / Recv
        server.bind(address).unwrap();
        let mut c_read = Completion::default();
        let mut s_read = UdpState::undefined();
        let mut recv_buf = [0u8; 128];
        let mut recv_len: usize = 0;
        unsafe {
            server.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_read),
                ptr::addr_of_mut!(s_read),
                read_slice(&mut recv_buf),
                ptr::addr_of_mut!(recv_len),
                |ud: *mut usize,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: *mut UdpState,
                 _addr: net::Address,
                 _sock: UDP,
                 _b: ReadBuffer,
                 r: core::result::Result<usize, ReadError>| {
                    *ud = r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        // Send
        let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
        let mut c_write = Completion::default();
        let mut s_write = UdpState::undefined();
        unsafe {
            client.write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_write),
                ptr::addr_of_mut!(s_write),
                address,
                write_slice(&send_buf),
                ptr::null_mut::<()>(),
                |_ud: *mut (),
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: *mut UdpState,
                 _sock: UDP,
                 _b: WriteBuffer,
                 r: core::result::Result<usize, WriteError>| {
                    r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        // Wait for the send/receive
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(recv_len > 0);
        assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

        // Close
        unsafe {
            server.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_read),
                ptr::null_mut::<()>(),
                |_ud: *mut (),
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: UDP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    CallbackAction::Disarm
                },
            );
            client.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_write),
                ptr::null_mut::<()>(),
                |_ud: *mut (),
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: UDP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();

        loop_.deinit();
    }

    // test "io_uring: sendmsg/recvmsg" (io_uring.zig:1626)
    #[test]
    fn io_uring_sendmsg_recvmsg() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Create a UDP server socket
        let address = net::Address::parse_ip4(b"127.0.0.1", PORT_SENDMSG_RECVMSG).unwrap();
        let family = unsafe { address.any.family } as libc::c_int;
        let server = unsafe { new_socket(family, libc::SOCK_DGRAM) };
        unsafe {
            set_reuse(server, libc::SO_REUSEPORT);
            set_reuse(server, libc::SO_REUSEADDR);
            assert_eq!(
                libc::bind(
                    server,
                    ptr::addr_of!(address.any) as *const libc::sockaddr,
                    address.get_os_sock_len()
                ),
                0
            );
        }

        let client = unsafe { new_socket(family, libc::SOCK_DGRAM) };

        // Send
        //
        // PORT NOTE: the reference's send-side buffers are `const` and ride the
        // `posix.msghdr_const` type. `libc` has no const twin of `msghdr` (its
        // fields are all `*mut`), so the `as *mut` casts below are pure type
        // plumbing: `IORING_OP_SENDMSG` only ever READS `msg_name`/`msg_iov`/the
        // iovec buffers, so nothing is ever written through these pointers and the
        // shared-ref provenance is never violated. (Contrast `msg_recv` below,
        // whose `msg_name` the kernel really does write.)
        let buffer_send = [42u8; 128];
        let iovecs_send = [libc::iovec {
            iov_base: buffer_send.as_ptr() as *mut libc::c_void,
            iov_len: buffer_send.len(),
        }];
        let mut msg_send = libc::msghdr {
            msg_name: ptr::addr_of!(address.any) as *mut libc::c_void,
            msg_namelen: address.get_os_sock_len(),
            msg_iov: iovecs_send.as_ptr() as *mut libc::iovec,
            msg_iovlen: 1,
            msg_control: ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        };
        fn sendmsg_cb(
            _ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            // _ = r.sendmsg catch unreachable;
            match r {
                Result::SendMsg(Ok(_)) => {}
                _ => unreachable!(),
            }
            CallbackAction::Disarm
        }
        let mut c_sendmsg = Completion {
            op: Operation::SendMsg {
                fd: client,
                msghdr: ptr::addr_of_mut!(msg_send),
                // Zig field defaults: `buffer = null`, `iov = undefined`.
                buffer: None,
                iov: [libc::iovec {
                    iov_base: ptr::null_mut(),
                    iov_len: 0,
                }],
            },
            callback: sendmsg_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_sendmsg)) };

        // Recv
        let mut buffer_recv = [0u8; 128];
        let mut iovecs_recv = [libc::iovec {
            iov_base: buffer_recv.as_mut_ptr() as *mut libc::c_void,
            iov_len: buffer_recv.len(),
        }];
        // `var address_recv` — MUTABLE on purpose: `recvmsg` WRITES the datagram's
        // source address back through `msg_name`, so the pointer handed to the
        // kernel must be derived from a mutable place (a `*mut` laundered out of
        // `addr_of!` would be writing through a shared-ref provenance).
        let mut address_recv = net::Address::init_ip4([0, 0, 0, 0], 0);
        let mut msg_recv = libc::msghdr {
            msg_name: ptr::addr_of_mut!(address_recv.any).cast::<libc::c_void>(),
            msg_namelen: address_recv.get_os_sock_len(),
            msg_iov: iovecs_recv.as_mut_ptr(),
            msg_iovlen: 1,
            msg_control: ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        };
        let mut recv_size: usize = 0;
        fn recvmsg_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let p = ud as *mut usize;
            // ptr.* = r.recvmsg catch unreachable;
            match r {
                Result::RecvMsg(Ok(n)) => unsafe { *p = n },
                _ => unreachable!(),
            }
            CallbackAction::Disarm
        }
        let mut c_recvmsg = Completion {
            op: Operation::RecvMsg {
                fd: server,
                msghdr: ptr::addr_of_mut!(msg_recv),
            },
            userdata: ptr::addr_of_mut!(recv_size).cast::<c_void>(),
            callback: recvmsg_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_recvmsg)) };

        // Wait for the sockets to close
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(recv_size, buffer_recv.len());
        assert_eq!(&buffer_send[0..buffer_recv.len()], &buffer_recv[..]);

        unsafe {
            libc::close(client);
            libc::close(server);
        }
        loop_.deinit();
    }

    // test "io_uring: socket read cancellation" (io_uring.zig:1725)
    #[test]
    fn io_uring_socket_read_cancellation() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Create a UDP server socket
        let address = net::Address::parse_ip4(b"127.0.0.1", PORT_READ_CANCELLATION).unwrap();
        let family = unsafe { address.any.family } as libc::c_int;
        let socket = unsafe { new_socket(family, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC) };
        unsafe {
            set_reuse(socket, libc::SO_REUSEADDR);
            assert_eq!(
                libc::bind(
                    socket,
                    ptr::addr_of!(address.any) as *const libc::sockaddr,
                    address.get_os_sock_len()
                ),
                0
            );
        }

        // Read
        let mut read_result: Option<Result> = None;
        fn read_result_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let p = ud as *mut Option<Result>;
            unsafe { *p = Some(r) };
            CallbackAction::Disarm
        }
        let mut c_read = Completion {
            op: Operation::Read {
                fd: socket,
                buffer: ReadBuffer::Array([0; 32]),
            },
            userdata: ptr::addr_of_mut!(read_result).cast::<c_void>(),
            callback: read_result_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_read)) };

        // Cancellation
        let mut c_read_cancel = Completion::default();
        let cancel_cb = |_ud: *mut (),
                         _l: *mut Loop,
                         _c: *mut Completion,
                         r: core::result::Result<(), CancelError>| {
            // r catch unreachable;
            r.unwrap();
            CallbackAction::Disarm
        };
        unsafe {
            loop_.cancel::<(), _>(
                ptr::addr_of_mut!(c_read),
                ptr::addr_of_mut!(c_read_cancel),
                ptr::null_mut(),
                cancel_cb,
            );
        }

        // loop.add(&c_read_cancel);
        //
        // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): `Loop::cancel`
        // ALREADY added `c_read_cancel` (io_uring.zig:592, `self.add(c_cancel)`),
        // so this second `add` submits a SECOND, redundant `IORING_OP_ASYNC_CANCEL`
        // SQE for the same target — and the cancel callback therefore fires TWICE,
        // both times asserting `r catch unreachable`. It is preserved because it is
        // what the reference test does (R1); it is benign because the kernel
        // reports success for BOTH: verified empirically on this host (Linux
        // 6.19.10, aarch64) by instrumenting `cqe.res`, which is `0` for every
        // repeated cancel of the same in-flight read, not the `-ENOENT` one might
        // expect for the second. See the report's concern about kernels that may
        // instead return `-ENOENT` here.
        unsafe { loop_.add(ptr::addr_of_mut!(c_read_cancel)) };

        // Wait for the read to be cancelled
        loop_.run(RunMode::UntilDone).unwrap();
        // try testing.expectEqual(OperationType.read, @as(OperationType, read_result));
        // try testing.expectError(error.Canceled, read_result.read);
        assert_eq!(read_result, Some(Result::Read(Err(ReadError::Canceled))));

        // The reference only `errdefer`s this close (so a passing run leaks it);
        // the crate's test process is long-lived and runs many tests, so close it.
        unsafe { libc::close(socket) };
        loop_.deinit();
    }

    // -----------------------------------------------------------------------
    // Live KATs (process / stream), mirroring epoll.rs's Task-4 KATs.
    // -----------------------------------------------------------------------

    // KAT — Process spawn+exit: posix_spawn `/bin/true`; Process.wait;
    // run(until_done); assert exit status 0. Proves the pidfd poll + waitid reap
    // on the live io_uring ring.
    #[test]
    #[allow(clippy::zombie_processes)]
    fn io_uring_process_spawn_exit() {
        // posix_spawn /bin/true (a real process that exits 0).
        let mut pid: libc::pid_t = 0;
        let path = b"/bin/true\0";
        let mut argv: [*mut libc::c_char; 2] =
            [path.as_ptr() as *mut libc::c_char, ptr::null_mut()];
        let mut envp: [*mut libc::c_char; 1] = [ptr::null_mut()];
        let rc = unsafe {
            libc::posix_spawn(
                &mut pid,
                path.as_ptr() as *const libc::c_char,
                ptr::null(),
                ptr::null(),
                argv.as_mut_ptr() as *const *mut libc::c_char,
                envp.as_mut_ptr() as *const *mut libc::c_char,
            )
        };
        assert_eq!(rc, 0, "posix_spawn /bin/true");

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let mut p = Process::init(pid).unwrap();

        // Wait
        let mut code: Option<u32> = None;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut Option<u32>,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<u32, ProcessWaitError>| {
            unsafe { *ud = Some(r.unwrap()) };
            CallbackAction::Disarm
        };
        unsafe {
            p.wait(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_wait),
                ptr::addr_of_mut!(code),
                cb,
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(code, Some(0), "exit status 0");

        p.deinit();
        loop_.deinit();
    }

    // KAT — Pty write round-trip: openpty; Stream.init_fd(write side = parent);
    // queue_write a payload; read it back on the other end (child); assert bytes +
    // WriteQueue drains. Proves the ordered-write engine on the live io_uring ring.
    #[test]
    fn io_uring_pty_write_roundtrip() {
        let mut pty = Pty::init().unwrap();
        let mut loop_ = Loop::init(Options::default()).unwrap();

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
        let payload = b"KAT payload\n";
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            assert_eq!(r.unwrap(), b"KAT payload\n".len());
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

    // KAT — Async (eventfd) wakeup: wait on an `Async`, notify it from a spawned
    // thread, and confirm the loop's `run(until_done)` returns with the wait
    // callback fired. Proves the eventfd poll + drain on the live io_uring ring.
    //
    // PORT NOTE: the reference's `AsyncTests` (`watcher/async.zig`) are generic over
    // the backend; this is the "async: notify" shape monomorphized for io_uring.
    #[test]
    fn io_uring_async_notify() {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        let notifier = Async::init().unwrap();

        let mut wake = false;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), ReadError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_wait),
                ptr::addr_of_mut!(wake),
                cb,
            );
        }

        // Not woken up before the notify.
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!wake);

        // Notify from ANOTHER thread WHILE this one is BLOCKED inside the loop —
        // the whole point of `Async`. `run(until_done)` parks in `io_uring_enter`
        // until the eventfd goes readable, so this asserts the real wakeup path
        // (joining the thread BEFORE running would only prove that an
        // already-readable eventfd completes, which the poll would do anyway).
        // Not racy: if the notify lands first the poll simply completes at once.
        let n = notifier; // `Async` is Copy (an fd handle) and Send (a plain i32).
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            n.notify().unwrap();
        });

        // Blocks here until the spawned thread notifies. A hang is a real bug.
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(wake);
        t.join().unwrap();

        // The eventfd counter was drained by the wait callback, so a second wait
        // does NOT fire spuriously.
        let mut wake2 = false;
        let mut c_wait2 = Completion::default();
        let cb2 = |ud: *mut bool,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   r: core::result::Result<(), ReadError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_wait2),
                ptr::addr_of_mut!(wake2),
                cb2,
            );
        }
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!wake2);

        notifier.deinit();
        loop_.deinit();
    }

    // Helper to open a pty (test-only), mirroring watcher/stream.zig's `Pty`.
    struct Pty {
        parent: fd_t,
        child: fd_t,
    }

    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }

    extern "C" {
        fn openpty(
            parent: *mut fd_t,
            child: *mut fd_t,
            name: *mut u8,
            termios: *const c_void,
            winsize: *const Winsize,
        ) -> libc::c_int;
    }

    impl Pty {
        fn init() -> core::result::Result<Pty, ()> {
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
                return Err(());
            }
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
