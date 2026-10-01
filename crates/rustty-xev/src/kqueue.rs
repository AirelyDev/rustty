//! kqueue.rs — faithful port of libxev `backend/kqueue.zig` (D6, XEV_BLUEPRINT
//! §2/§3, chunk C3, the HEART).
//!
//! Backend to use kqueue. This is currently only tested on macOS but
//! support for BSDs is planned (if it doesn't already work).
//
// PORT NOTE: SCOPE (XEV_BLUEPRINT §0): on macOS AND on FreeBSD `xev.Dynamic` IS the
// static kqueue `Api()` (the single-candidate `Backend.candidates()` collapses
// `dynamic.zig` at comptime — `backend.zig:20,38`), so this is the concrete flat
// loop on both. The module is `#[cfg(any(target_os = "macos", target_os = "ios",
// target_os = "visionos", target_os = "freebsd"))]` (W2D/F-004; the Darwin set
// widened 2026-08-26, `318af0f6`): the reference's own `switch (builtin.os.tag)` arms —
// the FreeBSD `posix.Kevent` fast paths (`kqueue.zig:1863/1935/1876`), the
// `NOTE_EXIT_FLAGS` split (`:45-49`), the `isDarwin()`-only machport op
// (`:1674/1081`) and the non-Darwin `Wakeup` stub (`:1488-1510`) — are ported as
// real `#[cfg]` arms rather than elided. `darwin.rs`/mach stays Darwin-only
// (`macos + ios + visionos`), exactly as `builtin.os.tag.isDarwin()` gates it in
// the reference.
//
// PORT NOTE (the three load-bearing disciplines, §2):
//  - RAW POINTERS end-to-end: the `Loop` holds `*mut Completion` into caller
//    memory; callbacks get `*mut Loop` + `*mut Completion`; loop/completion links
//    are NEVER `&mut`/lifetimes (Stacked-Borrows UB — memory
//    `rust-mut-args-strongly-protected`). The public `Loop` methods take `&mut
//    self` but every completion is touched through a raw `*mut Completion` popped
//    from a raw-ptr queue, and callbacks receive `self as *mut Loop` (raw), so a
//    callback may legally re-add / rearm a completion — exactly libxev's contract.
//  - `unsafe impl Send` for `Loop`/`Completion` (§2 F2): ghostty MOVES a `Loop`
//    onto the termio thread; single-thread affinity after the move makes this
//    sound (documented at each impl).
//  - hand-written `Default` for `Completion` (§2 F3): fn-ptr + raw-ptr fields.

// (module is `#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos", target_os = "freebsd"))]`-gated in lib.rs)
#![allow(non_upper_case_globals)]
// PORT NOTE: `posix.fd_t`/`socket_t`/`pid_t`/`socklen_t` keep their Zig/C snake_case
// type names (they mirror `std.posix` identifiers, R2 identifier-fidelity).
#![allow(non_camel_case_types)]

use core::mem::size_of;
use core::ptr;

// PORT NOTE (W2D): the loop `Wakeup` embeds an `Async` only on Darwin
// (`kqueue.zig:1442` — the `isDarwin()` arm's `mach_port: Async` field); the
// FreeBSD arm is fieldless, so neither import is reachable there.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
use crate::async_watcher::Async;
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
use crate::darwin;
use crate::heap;
use crate::looppkg::{noop_callback, Callback, CallbackAction, CompletionState, Options, RunMode};
use crate::net;
use crate::queue;
use crate::queue_mpsc;
use crate::thread_pool::{self, ThreadPool};
use crate::Error;

// PORT NOTE: `const log = std.log.scoped(.libxev_kqueue);` — Zig scoped logging has no
// Rust equivalent that we wire here; the two `log.err`/`log.warn` call sites are
// preserved as comments at their locations (unreachable/invalid-state diagnostics).

// PORT NOTE: the `std.posix` type aliases the backend threads through. On macOS these
// are the platform C ints.
/// PORT NOTE: `posix.fd_t`.
pub type fd_t = libc::c_int;
/// PORT NOTE: `posix.socket_t`.
pub type socket_t = libc::c_int;
/// PORT NOTE: `posix.pid_t`.
pub type pid_t = libc::pid_t;
/// PORT NOTE: `posix.socklen_t`.
pub type socklen_t = libc::socklen_t;
/// PORT NOTE: `posix.timespec` — the monotonic-clock instant the loop caches + the
/// timer-heap key. macOS `timespec` (`{ tv_sec: isize, tv_nsec: isize }`), from
/// `libc`; Zig `.sec`/`.nsec` map to `.tv_sec`/`.tv_nsec`.
pub type Timespec = libc::timespec;

// PORT NOTE: `std.time.*` constants used by `now`/`update_now`/timer math.
const ms_per_s: u64 = 1_000;
const ns_per_ms: u64 = 1_000_000;
const ns_per_s: u64 = 1_000_000_000;

/// True if this backend is available on this platform.
//
// PORT NOTE: the reference `switch (builtin.os.tag)` selects at comptime; this
// module compiles for the `.ios`/`.macos`/`.visionos` AND `.freebsd` arms, for
// all of which the answer is the constant `true` (`kqueue.zig:29-36`). The
// remaining other-OS arm + its comments are preserved verbatim below (it
// documents the mach-port requirement that gates the OTHER BSDs). On every live
// target this function is only referenced by the comptime-elided
// `Backend`/`dynamic` machinery (§0), so it is `#[allow(dead_code)]`.
//
// PORT NOTE (CORRECTED — unimplemented-leaf gap-close, 2026-08-26): this note
// used to say the module gated `macos+freebsd` while `backend.rs` gated
// `macos+ios+freebsd`, and that widening to `.ios`/`.visionos` was declined
// because it "would compile the mach paths on untested targets". That split was
// a live BUG, not a deferral: on iOS `Backend::default()` returned `Kqueue`
// while `lib.rs` re-exported the `no_backend_seam` placeholders, so every
// `Loop`/`Async`/`Timer`/`Process`/`Stream` entry point was an `unimplemented!()`
// panic on a target the reference runs for real (ghostty ships a libghostty iOS
// build; libxev's `backend.zig:20` maps `.ios`/`.visionos` onto kqueue and its
// mach paths are gated `isDarwin()`, which covers both).
//
// Every Darwin gate in this crate is now the reference's own set — `macos + ios
// + visionos` for the `isDarwin()`/`Kevent` arms, plus `freebsd` for the kqueue
// family. This is no longer codegen-only debt: `aarch64-apple-ios` is a tier-2
// rustup target and `cargo check -p rustty-xev --target aarch64-apple-ios` is
// green with the REAL backend compiled (proven by temporarily planting a
// `compile_error!` in `no_backend_seam` and watching iOS stop selecting it).
// `visionos` remains tier-3 (no rustup std), so that arm is still codegen debt.
#[allow(dead_code)]
pub fn available() -> bool {
    // macOS uses kqueue
    // .ios, .macos => true,

    // BSDs use kqueue, but we only test on FreeBSD for now.
    // kqueue isn't exactly the same here as it is on Apple platforms.
    // .freebsd => true,

    // Technically other BSDs support kqueue but our implementation
    // below hard requires mach ports currently. That's not a fundamental
    // requirement but until someone makes this implementation work
    // on other BSDs we'll just say it isn't available.
    // else => false,
    true
}

/// PORT NOTE: `pub const NOTE_EXIT_FLAGS` (macOS arm: `NOTE.EXIT | NOTE.EXITSTATUS`).
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
pub const NOTE_EXIT_FLAGS: u32 = libc::NOTE_EXIT | libc::NOTE_EXITSTATUS;
/// PORT NOTE: `pub const NOTE_EXIT_FLAGS` (`kqueue.zig:45-49`) — the FreeBSD arm is
/// `.freebsd => std.c.NOTE.EXIT` alone; FreeBSD has no `NOTE_EXITSTATUS` (it puts
/// the raw wait status in `ev.data` on a plain `NOTE_EXIT` delivery).
#[cfg(target_os = "freebsd")]
pub const NOTE_EXIT_FLAGS: u32 = libc::NOTE_EXIT;

/// PORT NOTE: `posix.SOCK.CLOEXEC` (the Zig-emulated close-on-exec flag `posix.accept`
/// applies via `fcntl` on macOS — `std.c.SOCK.CLOEXEC = 1 << 15`, "Not actually
/// supported by Darwin, but Zig supplies a shim"). The `accept` op's `flags` default.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
pub const SOCK_CLOEXEC: u32 = 0x8000;
/// PORT NOTE (W2D): `posix.SOCK.CLOEXEC` on FreeBSD is the REAL header constant
/// (`std/c.zig`'s `.freebsd => .{ CLOEXEC = 0x10000000 }` == `libc::SOCK_CLOEXEC`).
/// The `accept` perform path keeps the `fcntl(F_SETFD, FD_CLOEXEC)` emulation on
/// BOTH targets because libxev's own `posix.accept` does — it calls the bare
/// `accept(2)` and then `setCloexec(fd)` whenever the bit is set (`posix.zig:337-342`),
/// never `accept4`. So this constant is only ever tested against itself.
#[cfg(target_os = "freebsd")]
pub const SOCK_CLOEXEC: u32 = libc::SOCK_CLOEXEC as u32;

/// The current thread's `errno`. PORT NOTE: `posix.errno(rc)` reads the C `errno`
/// global when `rc == -1` (macOS `std.c._errno()`, FreeBSD `std.c.__error()` — the
/// same `__error()` symbol on both), else `.SUCCESS`.
#[inline]
fn errno() -> i32 {
    // SAFETY: `__error()` returns the thread-local errno location.
    unsafe { *libc::__error() }
}

// ---------------------------------------------------------------------------
// Kevent syscall layer (§3). On macOS `Kevent == kevent64_s` and the syscall is
// `kevent64` ([ADV-FIX F4]: both in the `libc` crate); on FreeBSD `Kevent` IS
// `std.c.Kevent` (`libc::kevent`) and the syscall is the plain `kevent(2)`.
// ---------------------------------------------------------------------------

/// Kevent is either kevent_s or kevent64_s depending on the target platform.
/// This lets us support both Mac and non-Mac platforms.
//
// PORT NOTE: `const Kevent = switch (...) { .macos => kevent64_s, ... }` — the macOS arm.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
pub type Kevent = libc::kevent64_s;
/// PORT NOTE: `const Kevent = switch (...) { .freebsd => std.c.Kevent, ... }`
/// (`kqueue.zig:1863`). `libc::kevent` on freebsd is
/// `{ident: uintptr_t, filter: c_short, flags: c_ushort, fflags: c_uint,
/// data: i64, udata: *mut c_void, ext: [u64; 4]}` under libc's default
/// `freebsd12` ABI — byte-matching the Zig `std.c.Kevent` freebsd arm. This port
/// never NAMES `ext`, so a `freebsd11`-ABI build (which has no `ext`) also checks.
#[cfg(target_os = "freebsd")]
pub type Kevent = libc::kevent;

/// The fields of a `posix.Kevent` (the base, non-64 kevent) that `kevent_init`
/// bundles; a Rust stand-in for the Zig `posix.Kevent{...}` struct literal.
struct KeventInit {
    ident: u64,
    filter: i16,
    flags: u16,
    fflags: u32,
    data: i64,
    udata: u64,
}

/// kevent_init initializes a Kevent from an posix.Kevent. This is used when
/// the "ext" fields are zero.
//
// PORT NOTE: `inline fn kevent_init(ev: posix.Kevent) Kevent`. macOS is the
// widening arm and fills `.ext = {0, 0}`.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
#[inline]
fn kevent_init(ev: KeventInit) -> Kevent {
    Kevent {
        ident: ev.ident,
        filter: ev.filter,
        flags: ev.flags,
        fflags: ev.fflags,
        data: ev.data,
        udata: ev.udata,
        ext: [0, 0],
    }
}

// PORT NOTE (W2D): `if (Kevent == std.c.Kevent) return ev;` — the FreeBSD identity
// arm of `kevent_init` (`kqueue.zig:1935`). Zig's `posix.Kevent` IS the target's
// `std.c.Kevent` there, so the reference hands the literal straight back. Rust's
// `KeventInit` is a portable stand-in for that literal, so this arm is the same
// identity modulo the per-target field widths (`ident: uintptr_t`, `udata:
// *mut c_void`). Zeroed-init so the body never NAMES `ext` — which keeps it
// compiling against libc's `freebsd11` ABI too.
#[cfg(target_os = "freebsd")]
#[inline]
fn kevent_init(ev: KeventInit) -> Kevent {
    // SAFETY: `libc::kevent` is a POD C struct of integers/pointers; all-zero is a
    // valid inhabitant (the same `= .{}`/`std.mem.zeroes` shape the C API expects).
    let mut k: Kevent = unsafe { core::mem::zeroed() };
    k.ident = ev.ident as libc::uintptr_t;
    k.filter = ev.filter;
    k.flags = ev.flags;
    k.fflags = ev.fflags;
    k.data = ev.data;
    k.udata = ev.udata as *mut libc::c_void;
    k
}

// ---------------------------------------------------------------------------
// PORT NOTE (W2D): per-target `udata` accessors. Zig's `posix.Kevent.udata` is a
// `usize` on BOTH platforms, so the reference reads/writes it directly
// (`kqueue.zig:594` `@as(*Completion, @ptrFromInt(ev.udata))`, `:829`
// `events[changes].udata = 0`). `libc` types it `u64` on macOS (`kevent64_s`) and
// `*mut c_void` on FreeBSD (`kevent`), so every read/write in the tick loops goes
// through these two inlines instead. `ev.data` (i64 on both ABIs) and `ev.flags`
// (u16 on both) need NO accessor and must NOT be routed through these.
// ---------------------------------------------------------------------------

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
#[inline]
fn kev_udata(ev: &Kevent) -> u64 {
    ev.udata
}
#[cfg(target_os = "freebsd")]
#[inline]
fn kev_udata(ev: &Kevent) -> u64 {
    ev.udata as u64
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
#[inline]
fn kev_set_udata(ev: &mut Kevent, v: u64) {
    ev.udata = v;
}
#[cfg(target_os = "freebsd")]
#[inline]
fn kev_set_udata(ev: &mut Kevent, v: u64) {
    ev.udata = v as *mut libc::c_void;
}

/// kevent calls either kevent or kevent64 depending on the
/// target platform.
//
// PORT NOTE: `fn kevent_syscall(...) posix.KEventError!usize` (macOS: the `kevent64`
// variant).
//
// PORT NOTE: the reference takes `changelist: []const Kevent, eventlist:
// []Kevent` and the loop passes OVERLAPPING slices of the same `events` array
// (the kqueue idiom: the kernel drains the changelist, then writes the
// eventlist). Zig permits this aliasing; Rust's borrow checker forbids a `&[_]`
// + `&mut [_]` of the same array, so this port takes raw pointer + length pairs
// (the true FFI shape), and each call site derives both from one `events`
// raw-pointer — faithful to the reference's single-buffer aliasing.
///
/// # Safety
/// `changelist[0..changelist_len]` must be readable and `eventlist[0..
/// eventlist_len]` writable (they may overlap); `timeout` (if Some) valid.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
unsafe fn kevent_syscall(
    kq: i32,
    changelist: *const Kevent,
    changelist_len: usize,
    eventlist: *mut Kevent,
    eventlist_len: usize,
    timeout: Option<&Timespec>,
) -> core::result::Result<usize, KEventError> {
    // Normaly Kevent? Just use the normal posix.kevent call.
    // (macOS: Kevent != posix.Kevent, so we always take the kevent64 variant.)

    // Otherwise, we have to call the kevent64 variant.
    loop {
        let changelist_n: libc::c_int = match libc::c_int::try_from(changelist_len) {
            Ok(v) => v,
            Err(_) => return Err(KEventError::Overflow),
        };
        let eventlist_n: libc::c_int = match libc::c_int::try_from(eventlist_len) {
            Ok(v) => v,
            Err(_) => return Err(KEventError::Overflow),
        };
        // SAFETY: caller guarantees the change/event ranges are valid; `timeout`
        // (if Some) is a valid `timespec`; kevent64 reads/writes within bounds.
        let rc = unsafe {
            libc::kevent64(
                kq,
                changelist,
                changelist_n,
                eventlist,
                eventlist_n,
                0,
                timeout.map_or(ptr::null(), |t| t as *const Timespec),
            )
        };
        let e = if rc == -1 { errno() } else { 0 };
        match e {
            0 => return Ok(rc as usize), // .SUCCESS => @intCast(rc)
            libc::EACCES => return Err(KEventError::AccessDenied),
            libc::EFAULT => unreachable!(),
            libc::EBADF => unreachable!(), // Always a race condition.
            libc::EINTR => continue,
            libc::EINVAL => unreachable!(),
            libc::ENOENT => return Err(KEventError::EventNotFound),
            libc::ENOMEM => return Err(KEventError::SystemResources),
            libc::ESRCH => return Err(KEventError::ProcessNotFound),
            _ => unreachable!(),
        }
    }
}

/// kevent calls either kevent or kevent64 depending on the
/// target platform.
//
// PORT NOTE (W2D): `fn kevent_syscall(...)` — the `if (Kevent == std.c.Kevent)`
// FreeBSD arm (`kqueue.zig:1876-1899`): the plain `kevent(2)`, no flags/ext
// arguments, with the SAME errno map as the `kevent64` arm below it
// (`ACCES`→AccessDenied, `INTR`→continue, `NOENT`→EventNotFound,
// `NOMEM`→SystemResources, `SRCH`→ProcessNotFound; `FAULT`/`BADF`/`INVAL`
// unreachable) and the same `std.math.cast(c_int, len) orelse error.Overflow`
// count guard.
//
// PORT NOTE: the raw pointer + length pairs (rather than Zig's overlapping
// `[]const Kevent`/`[]Kevent` slices) are the same aliasing accommodation the
// macOS arm above documents.
///
/// # Safety
/// `changelist[0..changelist_len]` must be readable and `eventlist[0..
/// eventlist_len]` writable (they may overlap); `timeout` (if Some) valid.
#[cfg(target_os = "freebsd")]
unsafe fn kevent_syscall(
    kq: i32,
    changelist: *const Kevent,
    changelist_len: usize,
    eventlist: *mut Kevent,
    eventlist_len: usize,
    timeout: Option<&Timespec>,
) -> core::result::Result<usize, KEventError> {
    // Normal Kevent? Just use the normal posix.kevent call.
    loop {
        let changelist_n: libc::c_int = match libc::c_int::try_from(changelist_len) {
            Ok(v) => v,
            Err(_) => return Err(KEventError::Overflow),
        };
        let eventlist_n: libc::c_int = match libc::c_int::try_from(eventlist_len) {
            Ok(v) => v,
            Err(_) => return Err(KEventError::Overflow),
        };
        // SAFETY: caller guarantees the change/event ranges are valid; `timeout`
        // (if Some) is a valid `timespec`; kevent reads/writes within bounds.
        let rc = unsafe {
            libc::kevent(
                kq,
                changelist,
                changelist_n,
                eventlist,
                eventlist_n,
                timeout.map_or(ptr::null(), |t| t as *const Timespec),
            )
        };
        let e = if rc == -1 { errno() } else { 0 };
        match e {
            0 => return Ok(rc as usize), // .SUCCESS => @intCast(rc)
            libc::EACCES => return Err(KEventError::AccessDenied),
            libc::EFAULT => unreachable!(),
            libc::EBADF => unreachable!(), // Always a race condition.
            libc::EINTR => continue,
            libc::EINVAL => unreachable!(),
            libc::ENOENT => return Err(KEventError::EventNotFound),
            libc::ENOMEM => return Err(KEventError::SystemResources),
            libc::ESRCH => return Err(KEventError::ProcessNotFound),
            _ => unreachable!(),
        }
    }
}

/// PORT NOTE: `posix.KEventError` — the error set of `kevent_syscall`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KEventError {
    AccessDenied,
    EventNotFound,
    SystemResources,
    ProcessNotFound,
    Overflow,
}

impl From<KEventError> for Error {
    // PORT NOTE: the D6 seam collapses every loop error to the unified
    // `crate::Error` (termio propagates `Loop::init()?`/`run()` through one type).
    fn from(_: KEventError) -> Error {
        Error
    }
}

// ---------------------------------------------------------------------------
// The Loop (kqueue.zig:48).
// ---------------------------------------------------------------------------

// PORT NOTE: `const TimerHeap = heap.Intrusive(Timer, void, Timer.less);`
type TimerHeap = heap::Intrusive<Timer, ()>;
// PORT NOTE: `const TaskCompletionQueue = queue_mpsc.Intrusive(Completion);`
type TaskCompletionQueue = queue_mpsc::Intrusive<Completion>;

/// PORT NOTE: `pub const Loop = struct { ... }`.
pub struct Loop {
    /// The fd of the kqueue.
    kqueue_fd: fd_t,

    /// The wakeup mechanism (mach ports on Apple, eventfd on BSD).
    wakeup_state: Wakeup,

    /// The number of active completions. This DOES NOT include completions that
    /// are queued in the submissions queue.
    active: usize,

    /// Our queue of submissions that we want to enqueue on the next tick.
    /// These are NOT started, they are NOT submitted to kqueue. They are
    /// pending.
    submissions: queue::Intrusive<Completion>,

    /// The queue of cancellation requests. These will point to the
    /// completion that we need to cancel. We don't queue the exact completion
    /// to cancel because it may be in another queue.
    cancellations: queue::Intrusive<Completion>,

    /// Our queue of completed completions where the callback hasn't been
    /// called yet, but the "result" field should be set on every completion.
    /// This is used to delay completion callbacks until the next tick.
    /// Values in the completion queue must not be in the kqueue.
    completions: queue::Intrusive<Completion>,

    /// Heap of timers. We use heaps instead of the EVFILT_TIMER because
    /// it avoids a lot of syscalls in the case where there are a LOT of
    /// timers.
    timers: TimerHeap,

    /// The thread pool to use for blocking operations that kqueue can't do.
    thread_pool: Option<*mut ThreadPool>,

    /// The MPSC queue for completed completions from the thread pool.
    //
    // PORT NOTE [LIFETIMES-AUDIT M1+M2 fix, proven on
    // `.port/phase-2/mirihar-xev-mpsc`]: the Zig field is inline
    // (`thread_pool_completions: TaskCompletionQueue`), but that queue is
    // SELF-REFERENTIAL — its `head`/`tail` point at the embedded `stub`, an
    // address INSIDE this queue. `init` writes those stub self-pointers ONCE and
    // `pop`/`push` re-deref them on every later tick. If the queue lived inline in
    // `Loop`, each `tick(&mut self)` (and each `run(&mut self)` — the live driver
    // reborrows up to 2000×) whole-`Loop` `&mut` reborrow would INVALIDATE the
    // provenance the stub self-pointers were minted with, making the later deref
    // Stacked/Tree-Borrows UB (mirihar candidates `plain_naive`/`plain_selfptr`/
    // `cell_selfptr`/`inline_cached` are ALL UB under BOTH SB and TB — the
    // one-line `addr_of_mut!` prescription and UnsafeCell fields are BOTH
    // insufficient, because the popped lineage roots at the whole-`Loop` reborrow,
    // not at the queue field). Giving the queue its OWN heap allocation (a `Box`)
    // launders it: `tick`'s `&mut self` reborrows only the `Loop` allocation, never
    // the queue's separate heap allocation, so the stub self-pointers survive
    // across ticks — the same reason the OTHER (non-self-referential) intrusive
    // queues stay clean (their links point at EXTERNAL completions). The Zig
    // `*Self` method contract on `TaskCompletionQueue` is preserved verbatim; only
    // the storage is heap-rooted. `thread_pool_completions_ptr` caches the stable
    // raw handle established at `init` (from the `Box`), and every post-init access
    // (`pop` in tick_inner, `push` in thread_perform) routes through THAT cache —
    // the `Box` itself is never reborrowed again — so no `&mut`/`Box`-deref retag
    // ever pops the queue's internal-pointer lineage (mirihar `boxq` +
    // `xthread_boxq`: CLEAN under BOTH SB and TB, single- and cross-thread).
    thread_pool_completions: Box<TaskCompletionQueue>,

    /// Stable raw handle to `*thread_pool_completions`, cached at first-tick
    /// `init` from the `Box` (null until then). The ONLY handle used post-init by
    /// `pop`/`push` so the self-referential queue's stub pointers are never
    /// invalidated by a `&mut self` reborrow. See the field PORT NOTE above.
    thread_pool_completions_ptr: *mut TaskCompletionQueue,

    /// Cached time
    cached_now: Timespec,

    /// Some internal fields we can pack for better space.
    flags: LoopFlags,
}

/// PORT NOTE: the `flags: packed struct { init, in_run, stopped }` sub-struct.
#[derive(Clone, Copy, Default)]
struct LoopFlags {
    /// True once it is initialized.
    init: bool,
    /// Whether we're in a run or not (to prevent nested runs).
    in_run: bool,
    /// Whether our loop is in a stopped state or not.
    stopped: bool,
}

// PORT NOTE [ADV-FIX F2]: `Loop` holds raw-ptr intrusive queues/heap + `*mut
// ThreadPool` → auto-`!Send`; ghostty MOVES a `Loop` onto the spawned termio
// thread. SAFETY / single-thread affinity: after the move a `Loop` (and every
// `Completion` it links) is touched by exactly one thread at a time — the owning
// event-loop thread — so transferring ownership is sound. The cross-thread wakeup
// rides `Async{port:u32}` (auto-`Send`), never a shared `&mut Loop` — and, since
// WS19 T10, the pool worker reads that handle (and the completion queue's) out of
// the caller-owned `Completion` (`task_wakeup`/`task_completions`), so it never
// ACCESSES the `Loop` allocation at all.
unsafe impl Send for Loop {}

impl Loop {
    /// Initialize a new kqueue-backed event loop. See the Options docs
    /// for what options matter for kqueue.
    //
    // PORT NOTE: `pub fn init(options: Options) !Loop` (backend/kqueue.zig:106) —
    // restored to the reference signature (L1 LT9). This decl previously
    // specialized to a no-arg `init()` that opened with `let options =
    // Options::default();`, justified as "termio's D6 seam fixes the signature to
    // no-arg". That premise was itself the divergence: ghostty spells
    // `xev.Loop.init(.{})` (Thread.zig:95), and BOTH reference backends take
    // Options (kqueue.zig:106; dynamic.zig:127) — the 0-arg shape appears nowhere
    // upstream. Widening it here lets ONE termio spelling
    // (`Loop::init(Options::default())` == `.{}`) serve kqueue AND Dynamic.
    //
    // BEHAVIOR IS IDENTICAL: every caller passes `Options::default()`, which is
    // exactly the value the deleted first statement produced.
    //
    // The `thread_pool` path stays deferred with the ThreadPool body (DEFERRALS.md,
    // the "file IO on thread pool" skip-stub); the `thread_pool: options.thread_pool`
    // assignment is preserved, keeping the dead thread-pool branch faithful — it now
    // forwards the caller's field instead of the hardcoded default `null`, which is
    // the same `null` for every present caller.
    pub fn init(options: Options) -> core::result::Result<Loop, Error> {
        // This creates a new kqueue fd
        // SAFETY: kqueue() is a plain syscall returning a new fd or -1.
        let fd = unsafe { libc::kqueue() };
        if fd < 0 {
            return Err(Error);
        }
        // errdefer posix.close(fd);
        let wakeup_state: Wakeup = match Wakeup::init() {
            Ok(w) => w,
            Err(e) => {
                // SAFETY: closing the fd we just opened on the error path.
                unsafe { libc::close(fd) };
                return Err(e);
            }
        };
        // errdefer wakeup_state.deinit();  (no fallible step follows)

        let mut res: Loop = Loop {
            kqueue_fd: fd,
            wakeup_state,
            thread_pool: options.thread_pool,
            // .thread_pool_completions = undefined — Rust has no undefined; place a
            // valid stub queue (never `init`'d/used while thread_pool is None).
            // Heap-rooted (Box) so its self-referential stub pointers survive the
            // per-tick `&mut self` reborrows (see the field PORT NOTE).
            thread_pool_completions: Box::new(TaskCompletionQueue::new(Completion::default())),
            // Cached at first-tick `init`; null until the (dead) thread-pool branch
            // wires it. Never dereferenced while `thread_pool` is None.
            thread_pool_completions_ptr: ptr::null_mut(),
            // .cached_now = undefined — populated by update_now() below.
            cached_now: unsafe { core::mem::zeroed() },
            active: 0,
            submissions: queue::Intrusive::default(),
            cancellations: queue::Intrusive::default(),
            completions: queue::Intrusive::default(),
            timers: TimerHeap::new((), Timer::less),
            flags: LoopFlags::default(),
        };

        res.update_now();
        Ok(res)
    }

    /// Deinitialize the loop, this closes the kqueue. Any events that
    /// were unprocessed are lost -- their callbacks will never be called.
    pub fn deinit(&mut self) {
        // SAFETY: closing our own kqueue fd; wakeup_state owns the mach port.
        unsafe { libc::close(self.kqueue_fd) };
        self.wakeup_state.deinit();
    }

    /// Stop the loop. This can only be called from the main thread.
    /// This will stop the loop forever. Future ticks will do nothing.
    ///
    /// This does NOT stop any completions that are queued to be executed
    /// in the thread pool. If you are using a thread pool, completions
    /// are not safe to recover until the thread pool is shut down. If
    /// you're not using a thread pool, all completions are safe to
    /// read/write once any outstanding `run` or `tick` calls are returned.
    pub fn stop(&mut self) {
        self.flags.stopped = true;
    }

    /// Returns true if the loop is stopped. This may mean there
    /// are still pending completions to be processed.
    pub fn stopped(&self) -> bool {
        self.flags.stopped
    }

    /// Add a completion to the loop. The completion is not started until
    /// the loop is run (`run`, `tick`) or an explicit submission request
    /// is made (`submit`).
    ///
    /// # Safety
    /// `completion` must be a valid, stable pointer to a `Completion` that lives
    /// until its callback fires (libxev's intrusive-ownership contract).
    pub unsafe fn add(&mut self, completion: *mut Completion) {
        // If this is a cancellation, we special case it and add it to
        // a separate queue so we can handle them first.
        if matches!(unsafe { &(*completion).op }, Operation::Cancel { .. }) {
            let mut dummy_ev: Kevent = unsafe { core::mem::zeroed() };
            assert!(!unsafe { self.start(completion, &mut dummy_ev) });
            return;
        }

        // We just add the completion to the queue. Failures can happen
        // at submission or tick time.
        unsafe { (*completion).flags.state = State::Adding };
        unsafe { self.submissions.push(completion) };
    }

    /// Submit any enqueue completions. This does not fire any callbacks
    /// for completed events (success or error). Callbacks are only fired
    /// on the next tick.
    ///
    /// If an error is returned, some events might be lost. Errors are
    /// exceptional and should generally not happen. If we could recover
    /// which completions were not submitted and restore them we would,
    /// but the kqueue API doesn't provide that level of clarity.
    pub fn submit(&mut self) -> core::result::Result<(), Error> {
        // We try to submit as many events at once as we can.
        let mut events: [Kevent; 256] = unsafe { core::mem::zeroed() };
        let mut events_len: usize = 0;

        // Submit all the submissions. We copy the submission queue so that
        // any resubmits don't cause an infinite loop.
        let mut queued = core::mem::take(&mut self.submissions);
        // self.submissions = .{};  (done by mem::take)

        // On error, we have to restore the queue because we may be batching.
        // (errdefer self.submissions = queued — handled at the kevent error path.)

        loop {
            // queue_pop:
            'queue_pop: loop {
                let c = unsafe { queued.pop() };
                if c.is_null() {
                    break 'queue_pop;
                }
                match unsafe { (*c).flags.state } {
                    // If we're adding then we start the event.
                    State::Adding => {
                        if unsafe { self.start(c, &mut events[events_len]) } {
                            events_len += 1;
                            if events_len >= events.len() {
                                break 'queue_pop;
                            }
                        }
                    }

                    // If we're deleting then we create a deletion event and
                    // queue the completion to notify cancellation.
                    State::Deleting => {
                        if let Some(ev) = unsafe { Completion::kevent(c) } {
                            let ecanceled = -(libc::ECANCELED);
                            unsafe {
                                (*c).result = Some(Completion::syscall_result(c, ecanceled));
                                (*c).flags.state = State::Dead;
                                self.completions.push(c);
                            }

                            events[events_len] = ev;
                            events[events_len].flags = libc::EV_DELETE;
                            events_len += 1;
                            if events_len >= events.len() {
                                break 'queue_pop;
                            }
                        }
                    }

                    // This is set if the completion was canceled while in the
                    // submission queue. This is a special case where we still
                    // want to call the callback to tell it it was canceled.
                    State::Dead => unsafe { self.stop_completion(c) },

                    // Shouldn't happen if our logic is all correct.
                    State::Active => {
                        // log.err("invalid state in submission queue state={}", .{c.flags.state});
                    }
                }
            }

            // If we have no events then we have to have gone through the entire
            // submission queue and we're done.
            if events_len == 0 {
                break;
            }

            // Zero timeout so that kevent returns immediately.
            let timeout: Timespec = unsafe { core::mem::zeroed() };
            // The changelist + eventlist alias the same `events` array (§note on
            // kevent_syscall); derive both from one raw pointer.
            let events_ptr = events.as_mut_ptr();
            let events_cap = events.len();
            let completed = match unsafe {
                kevent_syscall(
                    self.kqueue_fd,
                    events_ptr,
                    events_len,
                    events_ptr,
                    events_cap,
                    Some(&timeout),
                )
            } {
                Ok(n) => n,
                Err(e) => {
                    // errdefer self.submissions = queued;
                    self.submissions = queued;
                    return Err(e.into());
                }
            };
            events_len = 0;

            // Go through the completed events and queue them.
            // NOTE: we currently never process completions (we set
            // event list to zero length) because it was leading to
            // memory corruption we need to investigate.
            for ev in &events[0..completed] {
                // Zero udata values are internal events that we do nothing
                // on such as the mach port wakeup.
                if kev_udata(ev) == 0 {
                    continue;
                }

                // We handle deletions separately.
                if ev.flags & libc::EV_DELETE != 0 {
                    continue;
                }

                let c: *mut Completion = kev_udata(ev) as *mut Completion;

                // If EV_ERROR is set, then submission failed for this
                // completion. We get the syscall errorcode from data and
                // store it.
                if ev.flags & libc::EV_ERROR != 0 {
                    unsafe {
                        (*c).result = Some(Completion::syscall_result(c, -(ev.data as i32)));
                    }
                } else {
                    // No error, means that this completion is ready to work.
                    unsafe {
                        (*c).result = Some(Completion::perform(c, Some(ev)));
                    }
                }

                assert!(unsafe { (*c).result.is_some() });
                unsafe { self.completions.push(c) };
            }
        }

        Ok(())
    }

    /// Process the cancellations queue. This doesn't call any callbacks
    /// or perform any syscalls. This just shuffles state around and sets
    /// things up for cancellation to occur.
    fn process_cancellations(&mut self) {
        loop {
            let c = unsafe { self.cancellations.pop() };
            if c.is_null() {
                break;
            }
            let target: *mut Completion = match unsafe { &(*c).op } {
                Operation::Cancel { c } => *c,
                _ => unreachable!(),
            };
            match unsafe { (*target).flags.state } {
                // If the target is dead already we do nothing.
                State::Dead => {}

                // If the targeting is in the process of being removed
                // from the kqueue we do nothing because its already done.
                State::Deleting => {}

                // If they are in the submission queue, mark them as dead
                // so they will never be submitted.
                State::Adding => unsafe { (*target).flags.state = State::Dead },

                // If it is active we need to schedule the deletion.
                State::Active => unsafe { self.stop_completion(target) },
            }

            // We completed the cancellation.
            unsafe {
                (*c).result = Some(Result::Cancel(Ok(())));
                self.completions.push(c);
            }
        }
    }

    /// Run the event loop. See RunMode documentation for details on modes.
    /// Once the loop is run, the pointer MUST remain stable.
    // PORT NOTE (N431 in NEW-FINDINGS): the reference's `run(self: *Loop, ..)`
    // body lives in `run_raw`, which takes the loop as a raw pointer, as Zig's
    // unprotected `*Loop` does. Callbacks re-enter the loop while it runs,
    // through their `l` argument or through a pointer the caller stored (termio's
    // `td.loop_`), so no `&mut Loop` may be live across a callback: a
    // function-entry `&mut self` is protected for the whole call, and any other
    // access to the loop pops it (UB under Stacked and Tree Borrows,
    // .port/phase-2/mirihar-n431). This entry converts to raw at once and never
    // uses `self` again, so it is sound when callbacks reach the loop only
    // through `l`. A caller whose callbacks re-enter through a stored pointer
    // must enter through `run_raw` with that same pointer instead: this
    // `&mut self` is itself protected for the call (mirihar-n431
    // `wrapper_*_root`).
    pub fn run(&mut self, mode: RunMode) -> core::result::Result<(), Error> {
        // SAFETY: `self` is a live, exclusive `Loop` for the whole call.
        unsafe { Loop::run_raw(self, mode) }
    }

    /// `run(self: *Loop, mode)`: the raw-pointer entry (see `run`'s PORT NOTE).
    /// Callers whose callbacks re-enter the loop through a stored pointer
    /// enter here with that pointer.
    ///
    /// # Safety
    /// `this` must point at a live, initialized `Loop` that stays at the same
    /// address and is not otherwise borrowed for the whole call, except by
    /// the loop's own callbacks through raw pointers.
    pub unsafe fn run_raw(this: *mut Loop, mode: RunMode) -> core::result::Result<(), Error> {
        unsafe {
            match mode {
                RunMode::NoWait => Loop::tick_raw(this, 0),
                RunMode::Once => Loop::tick_raw(this, 1),
                RunMode::UntilDone => {
                    while !(*this).done() {
                        Loop::tick_raw(this, 1)?;
                    }
                    Ok(())
                }
            }
        }
    }

    /// Tick through the event loop once, waiting for at least "wait" completions
    /// to be processed by the loop itself.
    pub fn tick(&mut self, wait: u32) -> core::result::Result<(), Error> {
        // SAFETY: as `run`.
        unsafe { Loop::tick_raw(self, wait) }
    }

    /// `tick(self: *Loop, wait)`: the raw-pointer twin of `tick` (see `run`).
    ///
    /// # Safety
    /// As `run_raw`.
    pub unsafe fn tick_raw(this: *mut Loop, wait: u32) -> core::result::Result<(), Error> {
        unsafe {
            // If we're stopped then the loop is fully over.
            if (*this).flags.stopped {
                return Ok(());
            }

            // We can't nest runs.
            if (*this).flags.in_run {
                return Err(Error); // error.NestedRunsNotAllowed
            }
            (*this).flags.in_run = true;
            // defer self.flags.in_run = false; — via the guard below.
            let result = Loop::tick_inner(this, wait);
            (*this).flags.in_run = false;
            result
        }
    }

    unsafe fn tick_inner(this: *mut Loop, wait: u32) -> core::result::Result<(), Error> {
        unsafe {
            // Initialize
            if !(*this).flags.init {
                (*this).flags.init = true;

                if (*this).thread_pool.is_some() {
                    // Establish the stable heap-rooted raw handle ONCE, then init the
                    // self-referential queue THROUGH it, so the stub self-pointers it
                    // writes carry the queue's own (Box) allocation provenance — not a
                    // per-tick `&mut self` tag that the next tick's reborrow would pop.
                    // (LIFETIMES-AUDIT M2 fix; mirihar `boxq`.)
                    {
                        let qp: *mut TaskCompletionQueue =
                            &raw mut *(*this).thread_pool_completions;
                        ptr::addr_of_mut!((*this).thread_pool_completions_ptr).write(qp);
                        TaskCompletionQueue::init(qp);
                    }
                }

                if let Err(err) = (*this).wakeup_state.setup((*this).kqueue_fd) {
                    // We reset initialization because we can't do anything
                    // safely unless we get this mach port registered!
                    (*this).flags.init = false;
                    return Err(err);
                }
            }

            // The list of events, used as both a changelist and eventlist.
            let mut events: [Kevent; 256] = core::mem::zeroed();

            // The number of events in the events array to submit as changes
            // on repeat ticks. Used mostly for efficient disarm.
            let mut changes: usize = 0;

            let mut wait_rem: usize = wait as usize;

            // Handle all of our cancellations first because we may be able
            // to stop submissions from even happening if its still queued.
            // Plus, cancellations sometimes add more to the submission queue
            // (to remove from kqueue)
            (*this).process_cancellations();

            // TODO(mitchellh): an optimization in the future is for the last
            // batch of submissions to return the changelist, because we can
            // reuse that for the kevent call later...
            (*this).submit()?;

            // Explaining the loop condition: we want to loop only if we have
            // active handles (because it means we have something to do)
            // and we have stuff we want to wait for still (wait_rem > 0) or
            // we requested just a nowait tick (because we have to loop at least
            // once).
            //
            // We also loop if there are any requested changes. Requested
            // changes are only ever deletions currently, so we just process
            // those until we have no more.
            loop {
                // If we're stopped then the loop is fully over.
                if (*this).flags.stopped {
                    return Ok(());
                }

                // We must update our time no matter what
                (*this).update_now();

                // NOTE(mitchellh): This is a hideous boolean statement we should
                // clean it up.
                if !(((*this).active > 0 && (wait == 0 || wait_rem > 0))
                    || changes > 0
                    || !(*this).completions.empty())
                {
                    break;
                }

                // Run our expired timers
                let mut now_timer: Timer = Timer {
                    next: (*this).cached_now,
                    reset: None,
                    heap: heap::IntrusiveField::default(),
                    c: ptr::null_mut(),
                };
                loop {
                    let t = (*this).timers.peek();
                    if t.is_null() {
                        break;
                    }
                    if !Timer::less((), t, &mut now_timer) {
                        break;
                    }

                    // Remove the timer
                    assert!((*this).timers.delete_min() == t);

                    // Mark completion as done
                    let c = (*t).c;
                    (*c).flags.state = State::Dead;

                    // We mark it as inactive here because if we rearm below
                    // the start() function will reincrement this.
                    (*this).active -= 1;

                    // Lower our remaining count since we have processed something.
                    wait_rem = wait_rem.saturating_sub(1);

                    // Invoke
                    let cb = (*c).callback;
                    let ud = (*c).userdata;
                    let action = cb(ud, this, c, Result::Timer(Ok(TimerTrigger::Expiration)));
                    match action {
                        CallbackAction::Disarm => {}

                        // We use undefined as the second param because timers
                        // never set a kevent, and we assert false for the same
                        // reason.
                        CallbackAction::Rearm => {
                            let mut dummy_ev: Kevent = core::mem::zeroed();
                            assert!(!(*this).start(c, &mut dummy_ev));
                        }
                    }
                }

                // Migrate our completions from the thread pool MPSC queue to our
                // completion queue.
                // TODO: unify the queues
                if (*this).thread_pool.is_some() {
                    loop {
                        // Drain through the cached stable raw handle (never a fresh
                        // `&mut self.thread_pool_completions`, which would pop the
                        // queue's stub-pointer lineage). LIFETIMES-AUDIT M2; mirihar
                        // `boxq`.
                        let c = TaskCompletionQueue::pop((*this).thread_pool_completions_ptr);
                        if c.is_null() {
                            break;
                        }
                        (*this).completions.push(c);
                    }
                }

                // Process the completions we already have completed.
                loop {
                    let c = (*this).completions.pop();
                    if c.is_null() {
                        break;
                    }

                    // disarm_ev is the Kevent to use for disarming if the
                    // completion wants to disarm. We have to calculate this up
                    // front because c can be reused in callback.
                    let disarm_ev: Option<Kevent> = {
                        // If we're not active then we were never part of the kqueue.
                        // If we are part of a threadpool we also never were part
                        // of the kqueue.
                        if (*c).flags.state != State::Active || (*c).flags.threadpool {
                            None
                        } else {
                            Completion::kevent(c)
                        }
                    };

                    // We store whether this completion was active so we can decrement
                    // the active count later
                    let c_active = (*c).flags.state == State::Active;
                    (*c).flags.state = State::Dead;

                    // Decrease our waiters because we are definitely processing one.
                    wait_rem = wait_rem.saturating_sub(1);

                    // Completion queue items MUST have a result set.
                    let cb = (*c).callback;
                    let ud = (*c).userdata;
                    let res = (*c).result.unwrap();
                    let action = cb(ud, this, c, res);
                    match action {
                        // If we're active we have to schedule a delete. Otherwise
                        // we do nothing because we were never part of the kqueue.
                        CallbackAction::Disarm => {
                            if let Some(ev) = disarm_ev {
                                events[changes] = ev;
                                events[changes].flags = libc::EV_DELETE;
                                kev_set_udata(&mut events[changes], 0);
                                changes += 1;
                                assert!(changes <= events.len());
                            }

                            if c_active {
                                (*this).active -= 1;
                            }
                        }

                        // Only resubmit if we aren't already active (in the queue)
                        CallbackAction::Rearm => {
                            if !c_active {
                                (*this).submissions.push(c);
                            }
                        }
                    }

                    // If we filled the events slice, we break to avoid overflow.
                    if changes == events.len() {
                        break;
                    }
                }

                // Determine our next timeout based on the timers
                let timeout: Option<Timespec> = 'timeout: {
                    if wait_rem == 0 {
                        break 'timeout Some(core::mem::zeroed());
                    }

                    // If we have a timer, we want to set the timeout to our next
                    // timer value. If we have no timer, we wait forever.
                    let t = (*this).timers.peek();
                    if t.is_null() {
                        break 'timeout None;
                    }

                    // Determine the time in milliseconds.
                    let ms_now = ((*this).cached_now.tv_sec as u64) * ms_per_s
                        + ((*this).cached_now.tv_nsec as u64) / ns_per_ms;
                    let ms_next = ((*t).next.tv_sec as u64) * ms_per_s
                        + ((*t).next.tv_nsec as u64) / ns_per_ms;
                    let ms = ms_next.saturating_sub(ms_now);

                    // Convert to s/ns for the timespec
                    let sec = ms / ms_per_s;
                    let nsec = (ms % ms_per_s) * ns_per_ms;
                    let mut ts: Timespec = core::mem::zeroed();
                    ts.tv_sec = sec as libc::time_t;
                    ts.tv_nsec = nsec as _;
                    break 'timeout Some(ts);
                };

                // Wait for changes. Note that we ALWAYS attempt to get completions
                // back even if are done waiting (wait_rem == 0) because if we have
                // to make a syscall to submit changes, we might as well also check
                // for done events too.
                // PORT NOTE: the reference wraps this in a `completed: while (true) { break
                // :completed kevent_syscall(...) catch |err| switch ... }` labeled-break
                // (EINTR retry lives inside `kevent_syscall` itself), so it never
                // actually loops — ported as a direct match.
                // The changelist + eventlist alias the same `events` array; derive
                // both from one raw pointer (§note on kevent_syscall).
                let events_ptr = events.as_mut_ptr();
                let events_cap = events.len();
                let completed: usize = match kevent_syscall(
                    (*this).kqueue_fd,
                    events_ptr,
                    changes,
                    events_ptr,
                    events_cap,
                    timeout.as_ref(),
                ) {
                    Ok(n) => n,
                    // This should never happen because we always have
                    // space in our event list. If I'm reading the BSD source
                    // right (and Apple does something similar...) then ENOENT
                    // is always put into the eventlist if there is space:
                    // https://github.com/freebsd/freebsd-src/blob/5a4a83fd0e67a0d7787d2f3e09ef0e5552a1ffb6/sys/kern/kern_event.c#L1668
                    Err(KEventError::EventNotFound) => unreachable!(),

                    // Any other error is fatal
                    Err(e) => return Err(e.into()),
                };

                // Reset changes since they're not submitted
                changes = 0;

                // Go through the completed events and queue them.
                for i in 0..completed {
                    let ev = events[i];
                    // Zero udata values are internal events that we do nothing
                    // on such as the mach port wakeup.
                    if kev_udata(&ev) == 0 {
                        continue;
                    }

                    // Ignore any successful deletions. This can only happen
                    // from disarms below and in that case we already processed
                    // their callback.
                    if ev.flags & libc::EV_DELETE != 0 {
                        continue;
                    }

                    // This can only be set during changelist processing so
                    // that means that this event was never actually active.
                    // Therefore, we only decrement the waiters by 1 if we
                    // processed an active change.
                    if ev.flags & libc::EV_ERROR != 0 {
                        // We cannot use c here because c is already dead
                        // at this point for this event.
                        continue;
                    }
                    wait_rem = wait_rem.saturating_sub(1);

                    let c: *mut Completion = kev_udata(&ev) as *mut Completion;

                    // c is ready to be reused rigt away if we're dearming
                    // so we mark it as dead.
                    (*c).flags.state = State::Dead;

                    let result = Completion::perform(c, Some(&ev));
                    let cb = (*c).callback;
                    let ud = (*c).userdata;
                    let action = cb(ud, this, c, result);
                    match action {
                        CallbackAction::Disarm => {
                            // Mark this event for deletion, it'll happen
                            // on the next tick.
                            events[changes] = ev;
                            events[changes].flags = libc::EV_DELETE;
                            kev_set_udata(&mut events[changes], 0);
                            changes += 1;
                            assert!(changes <= events.len());

                            (*this).active -= 1;
                        }

                        // We rearm by default with kqueue so we just have to make
                        // sure that the state is correct.
                        CallbackAction::Rearm => {
                            (*c).flags.state = State::Active;
                        }
                    }
                }

                // If we ran through the loop once we break if we don't care.
                if wait == 0 {
                    break;
                }
            }

            Ok(())
        }
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
    pub fn now(&self) -> i64 {
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
        } else {
            // Errors are ignored.
        }
    }

    /// Add a timer to the loop. The timer will execute in "next_ms". This
    /// is oneshot: the timer will not repeat. To repeat a timer, either
    /// schedule another in your callback or return rearm from the callback.
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
                    heap: heap::IntrusiveField::default(),
                    c: ptr::null_mut(),
                }),
                userdata,
                callback: cb,
                ..Completion::default()
            };

            self.add(c);
        }
    }

    /// See io_uring.timer_reset for docs.
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
        match unsafe { (*c).flags.state } {
            State::Dead | State::Deleting => {
                unsafe { self.timer(c, next_ms, userdata, cb) };
            }

            // Adding state we can just modify the metadata and return
            // since the timer isn't in the heap yet.
            State::Adding => unsafe {
                // N434: through `timer_ptr`, never `&mut (*c).op`.
                let next = self.timer_next(next_ms);
                (*timer_ptr(c)).next = next;
                (*c).userdata = userdata;
                (*c).callback = cb;
            },

            State::Active => unsafe {
                // Update the reset time for the timer to the desired time
                // along with all the callbacks.
                let reset = self.timer_next(next_ms);
                // N434: the timer is linked in the heap; `timer_ptr`, never
                // `&mut (*c).op`.
                (*timer_ptr(c)).reset = Some(reset);
                (*c).userdata = userdata;
                (*c).callback = cb;

                // If the cancellation is active, we assume its for this timer
                // and do nothing.
                if Completion::state(c_cancel) == CompletionState::Active {
                    return;
                }
                assert!(
                    Completion::state(c_cancel) == CompletionState::Dead
                        && Completion::state(c) == CompletionState::Active
                );
                *c_cancel = Completion {
                    op: Operation::Cancel { c },
                    ..Completion::default()
                };
                self.add(c_cancel);
            },
        }
    }

    fn timer_next(&mut self, next_ms: u64) -> Timespec {
        // Get the timestamp of the absolute time that we'll execute this timer.
        // There are lots of failure scenarios here in math. If we see any
        // of them we just use the maximum value.
        let max: Timespec = {
            let mut ts: Timespec = unsafe { core::mem::zeroed() };
            ts.tv_sec = isize::MAX as libc::time_t;
            ts.tv_nsec = isize::MAX as _;
            ts
        };

        let next_s: isize = match isize::try_from(next_ms / ms_per_s) {
            Ok(v) => v,
            Err(_) => return max,
        };
        let next_ns: isize = match isize::try_from((next_ms % ms_per_s) * ns_per_ms) {
            Ok(v) => v,
            Err(_) => return max,
        };

        // `self.update_now()` (kqueue.zig:680): the deadline must be derived
        // from a FRESH clock read, not the cached time — a timer armed from a
        // callback reached after a long blocking `kevent` wait would otherwise
        // get an already-past deadline and fire immediately.
        self.update_now();

        let mut ts: Timespec = unsafe { core::mem::zeroed() };
        ts.tv_sec = match (self.cached_now.tv_sec as isize).checked_add(next_s) {
            Some(v) => v as libc::time_t,
            None => return max,
        };
        ts.tv_nsec = match (self.cached_now.tv_nsec as isize).checked_add(next_ns) {
            Some(v) => v as _,
            None => return max,
        };
        ts
    }

    fn done(&self) -> bool {
        self.flags.stopped
            || (self.active == 0 && self.submissions.empty() && self.completions.empty())
    }

    /// Start the completion. This returns true if the Kevent was set
    /// and should be queued.
    ///
    /// # Safety
    /// `c` valid+stable; `ev` a valid writable `Kevent` slot.
    unsafe fn start(&mut self, c: *mut Completion, ev: *mut Kevent) -> bool {
        let action: StartAction = if unsafe { (*c).flags.threadpool } {
            StartAction::Threadpool
        } else {
            match unsafe { &mut (*c).op } {
                Operation::Noop => {
                    unsafe { (*c).flags.state = State::Dead };
                    return false;
                }

                Operation::Cancel { .. } => {
                    // Queue the cancel
                    StartAction::Cancel
                }

                Operation::Accept { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                Operation::Connect { socket, addr } => 'connect: loop {
                    // SAFETY: connect on the socket with the stored address.
                    let result = unsafe {
                        libc::connect(
                            *socket,
                            ptr::addr_of!(addr.any) as *const libc::sockaddr,
                            addr.get_os_sock_len(),
                        )
                    };
                    let e = if result == -1 { errno() } else { 0 };
                    match e {
                        // Interrupt, try again
                        libc::EINTR => continue 'connect,

                        // This means the connect is blocked and in progress.
                        // We register for the write event which will let us know
                        // when it is complete.
                        libc::EAGAIN | libc::EINPROGRESS => {
                            unsafe { *ev = Completion::kevent(c).unwrap() };
                            break 'connect StartAction::Kevent;
                        }

                        // Any other error we report
                        _ => break 'connect StartAction::Result(result),
                    }
                },

                Operation::Write { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                Operation::Pwrite { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                Operation::Read { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                Operation::Pread { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                Operation::Send { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                Operation::Recv { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                Operation::Sendto { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                Operation::Recvfrom { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                #[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
                Operation::Machport { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                // PORT NOTE (W2D): the `.machport` payload is `void` off Darwin
                // (`kqueue.zig:1674`) and `Completion.kevent()` returns `null`
                // for it (`:1081`), so this `start` arm's `.?` would be the
                // reference's own unreachable — nothing off Darwin constructs
                // the op.
                #[cfg(target_os = "freebsd")]
                Operation::Machport => unreachable!(),

                Operation::Proc { .. } => {
                    unsafe { *ev = Completion::kevent(c).unwrap() };
                    StartAction::Kevent
                }

                Operation::Shutdown { socket, how } => {
                    let how_c = match how {
                        ShutdownHow::Recv => libc::SHUT_RD,
                        ShutdownHow::Send => libc::SHUT_WR,
                        ShutdownHow::Both => libc::SHUT_RDWR,
                    };
                    // SAFETY: shutdown on the stored socket.
                    let result = unsafe { libc::shutdown(*socket, how_c) };
                    StartAction::Result(result)
                }

                Operation::Close { fd } => {
                    // SAFETY: close the stored fd.
                    unsafe { libc::close(*fd) };
                    StartAction::Result(0)
                }

                Operation::Timer(_) => {
                    // N434: the heap keeps this pointer; it carries `c`'s own tag
                    // (`timer_ptr`), not the `&mut` op borrow of this match.
                    let v_ptr: *mut Timer = unsafe { timer_ptr(c) };

                    // Point back to completion since we need this. In the future
                    // we want to use @fieldParentPtr but https://github.com/ziglang/zig/issues/6611
                    unsafe { (*v_ptr).c = c };

                    // Insert the timer into our heap.
                    unsafe { self.timers.insert(v_ptr) };

                    // We always run timers
                    StartAction::Timer
                }
            }
        };

        match action {
            StartAction::Kevent | StartAction::Timer => {
                // Increase our active count so we now wait for this. We
                // assume it'll successfully queue. If it doesn't we handle
                // that later (see submit)
                self.active += 1;
                unsafe { (*c).flags.state = State::Active };

                // We only return true if this is a kevent, since other
                // actions can come in here.
                matches!(action, StartAction::Kevent)
            }

            StartAction::Cancel => {
                // We are considered an active completion.
                self.active += 1;
                unsafe { (*c).flags.state = State::Active };

                unsafe { self.cancellations.push(c) };
                false
            }

            StartAction::Threadpool => {
                // We need to mark this completion as active no matter
                // what happens below so that we mark is inactive with
                // completion handling.
                self.active += 1;
                unsafe { (*c).flags.state = State::Active };

                // We need a thread pool otherwise we set an error on
                // our result and queue the completion.
                let pool = match self.thread_pool {
                    Some(p) => p,
                    None => {
                        // We use EPERM as a way to note there is no thread
                        // pool. We can change this in the future if there is
                        // a better choice.
                        let eperm = -(libc::EPERM);
                        unsafe {
                            (*c).result = Some(Completion::syscall_result(c, eperm));
                            self.completions.push(c);
                        }
                        return false;
                    }
                };

                // Setup our completion state so that the thread can
                // communicate back to our main thread.
                unsafe {
                    (*c).task_loop = self;
                    // PORT-ADDED [R1-DIVERGENCE, WS19 T10]: copy the two handles
                    // `thread_perform` needs INTO the completion, so the pool
                    // worker never touches the `Loop` allocation (which the loop
                    // thread holds a protected `&mut` to for the whole tick). See
                    // the `task_completions`/`task_wakeup` field docs and mirihar
                    // `wakeup_completion_cached`. Both reads here happen on the
                    // loop thread; `thread_pool_completions_ptr` was established
                    // at the top of this very tick (`tick_inner` init arm).
                    (*c).task_completions = self.thread_pool_completions_ptr;
                    // PORT NOTE (W2D): Darwin-only, mirroring the comptime
                    // `if (isDarwin())` around `thread_perform`'s wakeup call
                    // (`kqueue.zig:965-968`) — see the `task_wakeup` field doc.
                    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
                    {
                        (*c).task_wakeup = self.wakeup_state.mach_port;
                    }
                    (*c).task = thread_pool::Task {
                        callback: Loop::thread_perform,
                        ..thread_pool::Task::default()
                    };

                    // Schedule it, from this point forward its not safe to touch c.
                    (*pool).schedule(thread_pool::Batch::from(ptr::addr_of_mut!((*c).task)));
                }

                false
            }

            // A result is immediately available. Queue the completion to
            // be invoked.
            StartAction::Result(result) => {
                unsafe {
                    (*c).result = Some(Completion::syscall_result(c, result));
                    self.completions.push(c);
                }

                false
            }
        }
    }

    /// # Safety
    /// `c` must be a valid completion pointer owned by this loop.
    unsafe fn stop_completion(&mut self, c: *mut Completion) {
        if unsafe { (*c).flags.state } == State::Active {
            // If there is a result already, then we're already in the
            // completion queue and we can be done. Items in the completion
            // queue can NOT be in the kqueue too.
            if unsafe { (*c).result.is_some() } {
                return;
            }

            // If this completion has a kevent associated with it, then
            // we must remove the kevent. We remove the kevent by adding it
            // to the submission queue (because its the same syscall) but
            // setting the state to deleting.
            if unsafe { Completion::kevent(c) }.is_some() {
                self.active -= 1;
                unsafe {
                    (*c).flags.state = State::Deleting;
                    self.submissions.push(c);
                }
                return;
            }
        }

        // Inspect other operations. WARNING: the state can be ANYTHING
        // here so per op be sure to check the state flag.
        // PORT NOTE: `switch (c.op) { .timer => |*v| {...}, else => {} }` — the sole
        // meaningful arm is `.timer`; ported as `if let` (the `else => {}` no-op).
        // N434: a shared test; the Timer is reached through `timer_ptr`, never
        // `&mut (*c).op`.
        if matches!(unsafe { &(*c).op }, Operation::Timer(_)) {
            let v: *mut Timer = unsafe { timer_ptr(c) };
            if unsafe { (*c).flags.state } == State::Active {
                // Remove from the heap so it never fires...
                unsafe { self.timers.remove(v) };

                // If we have reset set AND we got a cancellation result,
                // that means that we were canceled so that we can update
                // our expiration time.
                if let Some(r) = unsafe { (*v).reset } {
                    unsafe {
                        (*v).next = r;
                        (*v).reset = None;
                    }
                    self.active -= 1;
                    unsafe { self.add(c) };
                    return;
                }
            }

            // Add to our completions so we trigger the callback.
            unsafe {
                (*c).result = Some(Result::Timer(Ok(TimerTrigger::Cancel)));
                self.completions.push(c);
            }

            // Note the timers state purposely remains ACTIVE so that
            // when we process the completion we decrement the
            // active count.
        }
    }

    /// This is the main callback for the threadpool to perform work
    /// on completions for the loop.
    //
    // PORT NOTE: the dead thread-pool branch (never taken while thread_pool is None,
    // §1/§6); ported for faithfulness. `@fieldParentPtr("task", t)` recovers the
    // owning Completion via `container_of` (offset-of).
    fn thread_perform(t: *mut thread_pool::Task) {
        // const c: *Completion = @fieldParentPtr("task", t);
        let c: *mut Completion = unsafe {
            (t as *mut u8).sub(core::mem::offset_of!(Completion, task)) as *mut Completion
        };

        // Do our task
        unsafe { (*c).result = Some(Completion::perform(c, None)) };

        // Add to our completion queue. Push THROUGH the loop's cached stable raw
        // handle, which `Loop::start` copied into THIS completion — never a fresh
        // `&mut …thread_pool_completions` (a unique reborrow of the queue formed on
        // a pool thread is UB regardless of the atomics inside, and would pop the
        // queue's stub-pointer lineage), and never `(*(*c).task_loop)` either: the
        // loop thread holds a protected `&mut Loop` for the whole tick, so even a
        // by-value READ out of the `Loop` allocation from here is a foreign access
        // that invalidates it (mirihar `wakeup_copied`: UB under SB). The handle
        // reached this way is rooted in the queue's own (Box) allocation, and the
        // completion is caller-owned. (LIFETIMES-AUDIT M1 + WS19 T10; mirihar
        // `xthread_boxq` + `wakeup_completion_cached`: CLEAN under SB + TB.)
        unsafe { TaskCompletionQueue::push((*c).task_completions, c) };

        // Wake up our main loop.
        //
        // PORT NOTE (W2D): the reference's wakeup call is itself comptime-gated:
        //   if (comptime builtin.target.os.tag.isDarwin()) {
        //       // Wake up our main loop
        //       c.task_loop.wakeup() catch {};
        //   }
        // (`kqueue.zig:965-968`). On FreeBSD `thread_perform` is PUSH-ONLY — the
        // push above is the whole body, and the completion is collected on the
        // loop's next tick. That is deliberate upstream: the non-Darwin `Wakeup`
        // is a `@panic("wakeup not implemented on this platform")` stub
        // (`kqueue.zig:1488-1510`), so a FreeBSD `thread_perform` that notified
        // would abort on every pool task. See DEFERRALS.md's FreeBSD row.
        //
        // The reference calls `self.wakeup()`; autorefing `Loop::wakeup(&mut self)`
        // here would form a `&mut Loop` on this pool worker while the loop thread
        // holds one (mirihar `wakeup_naive`: UB under SB AND TB). We notify the
        // `Copy` mach-port handle that `Loop::start` copied into this completion,
        // forming no reference to — and performing no access on — `Loop` at all.
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
        unsafe {
            let _ = (*c).task_wakeup.notify();
        };
    }

    /// Sends an empty message to this loop's mach port so that it wakes
    /// up if it is blocking on kevent().
    fn wakeup(&mut self) -> core::result::Result<(), Error> {
        self.wakeup_state.wakeup()
    }
}

/// PORT NOTE: the local `StartAction = union(enum) { kevent, timer, cancel,
/// threadpool, result: i32 }` in `Loop.start`.
enum StartAction {
    /// We have set the kevent out parameter
    Kevent,
    // We are a timer,
    Timer,
    // We are a cancellation
    Cancel,
    // We want to run on the threadpool
    Threadpool,
    /// We have a result code from making a system call now.
    Result(i32),
}

mod completion;
pub use self::completion::*;

// ---------------------------------------------------------------------------
// Tests (kqueue.zig `test{}` blocks). The C3 subset per XEV_BLUEPRINT §8:
// loop time / stop / timer x-family / cancellation x2 / sockets x2 (raw
// completions). "mach port" is C4 (needs the Async watcher wait path); "file IO
// on thread pool" is the deferred skip-stub (ThreadPool body, §1/§6). Callbacks
// are plain fn items matching the loop `Callback` ABI; completions/userdata are
// stack locals reached through raw pointers (the intrusive-ownership contract).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests;
