//! The io_uring kernel ABI and the ring itself.
//!
//! PORTS: the `std.os.linux` io_uring surface that libxev src/backend/io_uring.zig
//! uses — the hand-declared kernel structs and constants, the `io_uring_setup`/
//! `io_uring_enter` syscalls, `IoUring` with its `Sq`/`Cq`, and the SQE `prep_*`
//! helpers.

use super::*;

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
pub(super) struct io_uring_sqe {
    opcode: u8,
    flags: u8,
    ioprio: u16,
    fd: i32,
    off: u64,
    addr: u64,
    len: u32,
    rw_flags: u32,
    pub(super) user_data: u64,
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
pub(super) struct io_uring_cqe {
    pub(super) user_data: u64,
    pub(super) res: i32,
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
pub(super) const IORING_TIMEOUT_ABS: u32 = 1 << 0;

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
    pub(super) fn get_sqe(
        &mut self,
    ) -> core::result::Result<*mut io_uring_sqe, SubmissionQueueFull> {
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
    pub(super) fn submit(&mut self) -> core::result::Result<u32, SubmitError> {
        self.submit_and_wait(0)
    }

    /// PORT NOTE: `IoUring.submit_and_wait` — flush + enter, waiting for `wait_nr`
    /// completions. THIS is the one-syscall fast path the loop's tick rides.
    pub(super) fn submit_and_wait(
        &mut self,
        wait_nr: u32,
    ) -> core::result::Result<u32, SubmitError> {
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
    pub(super) fn copy_cqes(
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
pub(super) unsafe fn prep_accept(
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
pub(super) unsafe fn prep_connect(
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
pub(super) unsafe fn prep_poll_add(sqe: *mut io_uring_sqe, fd: fd_t, poll_mask: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_POLL_ADD, fd, 0, 0, 0);
        (*sqe).rw_flags = poll_mask;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_read(fd, buffer, offset)`.
///
/// # Safety
/// `sqe` live; `buf[..len]` must stay valid+writable until the op completes.
pub(super) unsafe fn prep_read(
    sqe: *mut io_uring_sqe,
    fd: fd_t,
    buf: *mut u8,
    len: usize,
    offset: u64,
) {
    unsafe { prep_rw(sqe, IORING_OP_READ, fd, buf as u64, len as u32, offset) };
}

/// PORT NOTE: `io_uring_sqe.prep_write(fd, buffer, offset)`.
///
/// # Safety
/// `sqe` live; `buf[..len]` must stay valid until the op completes.
pub(super) unsafe fn prep_write(
    sqe: *mut io_uring_sqe,
    fd: fd_t,
    buf: *const u8,
    len: usize,
    offset: u64,
) {
    unsafe { prep_rw(sqe, IORING_OP_WRITE, fd, buf as u64, len as u32, offset) };
}

/// PORT NOTE: `io_uring_sqe.prep_send(fd, buffer, flags)`.
///
/// # Safety
/// `sqe` live; `buf[..len]` must stay valid until the op completes.
pub(super) unsafe fn prep_send(
    sqe: *mut io_uring_sqe,
    fd: fd_t,
    buf: *const u8,
    len: usize,
    flags: u32,
) {
    unsafe {
        prep_rw(sqe, IORING_OP_SEND, fd, buf as u64, len as u32, 0);
        (*sqe).rw_flags = flags;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_recv(fd, buffer, flags)`.
///
/// # Safety
/// `sqe` live; `buf[..len]` must stay valid+writable until the op completes.
pub(super) unsafe fn prep_recv(
    sqe: *mut io_uring_sqe,
    fd: fd_t,
    buf: *mut u8,
    len: usize,
    flags: u32,
) {
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
pub(super) unsafe fn prep_sendmsg(
    sqe: *mut io_uring_sqe,
    fd: fd_t,
    msghdr: *const libc::msghdr,
    flags: u32,
) {
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
pub(super) unsafe fn prep_recvmsg(
    sqe: *mut io_uring_sqe,
    fd: fd_t,
    msghdr: *mut libc::msghdr,
    flags: u32,
) {
    unsafe {
        prep_rw(sqe, IORING_OP_RECVMSG, fd, msghdr as u64, 1, 0);
        (*sqe).rw_flags = flags;
    }
}

/// PORT NOTE: `io_uring_sqe.prep_close(fd)`.
///
/// # Safety
/// `sqe` must point at a live SQE slot.
pub(super) unsafe fn prep_close(sqe: *mut io_uring_sqe, fd: fd_t) {
    unsafe { prep_rw(sqe, IORING_OP_CLOSE, fd, 0, 0, 0) };
}

/// PORT NOTE: `io_uring_sqe.prep_shutdown(sockfd, how)` — `how` rides `len`.
///
/// # Safety
/// `sqe` must point at a live SQE slot.
pub(super) unsafe fn prep_shutdown(sqe: *mut io_uring_sqe, sockfd: socket_t, how: u32) {
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
pub(super) unsafe fn prep_timeout(
    sqe: *mut io_uring_sqe,
    ts: *const KernelTimespec,
    count: u32,
    flags: u32,
) {
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
pub(super) unsafe fn prep_timeout_remove(sqe: *mut io_uring_sqe, timer_user_data: u64, flags: u32) {
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
pub(super) unsafe fn prep_cancel(sqe: *mut io_uring_sqe, cancel_user_data: u64, flags: u32) {
    unsafe {
        prep_rw(sqe, IORING_OP_ASYNC_CANCEL, -1, cancel_user_data, 0, 0);
        (*sqe).rw_flags = flags;
    }
}
