//! dynamic.rs — the hand-authored 2-backend union selector (XEV_BLUEPRINT §1
//! Linux port, task L1 LT7). Port of libxev `dynamic.zig`.
//!
//! Creates the Xev API based on a set of backend types that allows
//! for dynamic choice between the various candidate backends (assuming
//! they're available). For example, on Linux, this allows a consumer to
//! choose between io_uring and epoll, or for automatic fallback to epoll
//! to occur.
//!
//! The goal of this API is to match the static Xev API as closely as possible.
//! It can't be exact since this is an abstraction and therefore must be limited
//! to the least common denominator of all backends.
//!
//! Since the static Xev API exposes types that can be initialized on their
//! own (i.e. xev.Async.init()), this API does the same. Since we need to
//! know the backend to use, this uses a global variable. The backend to use
//! defaults to the first candidate backend and DOES NOT test that it is
//! available. The API user should call `detect()` to set the backend to
//! the first available backend.
//!
//! Additionally, a caller can manually set the backend to use, but this
//! must be done before initializing any other types.
//
// ===========================================================================
// PORT NOTE (THE SHAPE OF THIS PORT) — READ FIRST.
//
// `dynamic.zig` is pure comptime metaprogramming: `Xev(bes)` builds every type
// with `@Type`/`@unionInit`/`EnumSubset`/`Union`/`ErrorSet` over an arbitrary
// candidate list. Rust has NO comptime type reflection, so this file is the
// CONCRETE hand-authored instantiation of `Xev(&.{ .io_uring, .epoll })` — the
// exact one `backend.zig`'s `.linux => &.{ .io_uring, .epoll }` candidate list
// (backend.rs `Backend::candidates()`) asks for. Every generated type below is
// spelled out as a fixed 2-arm union over {IoUring, Epoll}; the SEMANTICS are
// the reference's, only the generation mechanism differs.
//
// The Zig→Rust mapping of the four generators:
//
//  * `EnumSubset(AllBackend, bes)` (dynamic.zig:429) → there is no subset enum.
//    Rust has ONE `Backend` (backend.rs) with all five variants; the "subset"
//    is the runtime `CANDIDATES` array, and `subset()`/`superset()`
//    (dynamic.zig:178-184, pure `@enumFromInt` round-trips between the two
//    enums) become the identity — they are not ported as functions.
//
//  * `Union(bes, field, tagged=false)` (dynamic.zig:456) → a `#[repr(C)] union`
//    of the two backends' types. This is the FAITHFUL analogue: the reference's
//    union really is UNTAGGED ("to save an extra bit of memory per instance
//    since we have the active backend from the outer struct", dynamic.zig:454)
//    — a Rust `enum` would add a redundant discriminant the reference
//    deliberately does not have. `ManuallyDrop` wraps each arm because the
//    backend types are not `Copy` (Rust unions require `Copy`/`ManuallyDrop`);
//    none of them has a `Drop` impl, so no drop glue is suppressed — `deinit()`
//    is explicit, exactly as in Zig. The `_zig_bug_padding` arm
//    (dynamic.zig:483-491) is a Zig-0.13 compiler workaround with no Rust
//    analogue and is not ported.
//
//  * `Union(bes, field, tagged=true)` → a `#[repr(C)] struct { value: <union>,
//    tag: Backend }`, i.e. Zig's `union(enum)` written out by hand. `value` is
//    FIRST so its offset is 0 (asserted below).
//
//  * `ErrorSet(bes, field)` (dynamic.zig:512) → Zig merges error sets with
//    `Set || NextSet`; the Rust analogue is an enum whose variants are the
//    UNION of both backends' variants, plus a total per-backend mapping at each
//    dispatch site (Rust has no error-set subtyping/coercion).
//
// WHY `#[repr(C)]` struct+union RATHER THAN A RUST `enum` (load-bearing):
// the reference's watcher trampolines recover the OUTER type from a pointer to
// the INNER backend value with `@fieldParentPtr` (timer.zig:290-295,
// async.zig:681-688, stream.zig:656-663). Rust's `core::mem::offset_of!` works
// on struct fields but NOT on enum variant payloads (the `offset_of_enum`
// feature is unstable), and projecting an enum payload via `match &mut *c`
// would mint a `&mut` whose derived raw pointer is invalidated by any later
// access through the outer raw pointer (Stacked Borrows) — precisely the UB the
// port's raw-pointer discipline forbids. A `#[repr(C)]` struct + union has a
// GUARANTEED layout: every union arm sits at offset 0 of the union, and `value`
// sits at offset 0 of the struct, so both the down-cast and the
// `@fieldParentPtr` up-cast are plain identity `cast()`s with NO pointer
// arithmetic at all (const-asserted below).
//
// PORT NOTE (raw-pointer / Stacked-Borrows discipline, inherited from
// epoll.rs/io_uring.rs): `*mut Loop` / `*mut Completion` end-to-end. No `&mut`
// is ever held across a loop tick or a callback; every projection into a
// completion goes through `ptr::addr_of_mut!` on the caller's raw pointer, and
// every callback receives raw pointers.
// ===========================================================================

#![allow(dead_code)]

use core::any::TypeId;
use core::ffi::c_void;
use core::mem::{offset_of, size_of, ManuallyDrop};
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::backend::Backend;
use crate::epoll;
use crate::io_uring;
use crate::linux::net;

// PORT NOTE: `pub const Options = looppkg.Options; pub const RunMode = ...`
// (dynamic.zig:60-63) — "Forward some global types so that users can replace
// @import("xev") with the dynamic package and everything works." On Linux those
// live in `linux.rs` (the byte-faithful twins of the macOS-gated `loop.rs`).
pub use crate::linux::{CallbackAction, CompletionState, Options, RunMode};

/// PORT NOTE: `pub const dynamic = true;` (dynamic.zig:46) — "This is a flag
/// that can be used to detect that you're using a dynamic API and not the
/// static API via `hasDecl`."
pub const DYNAMIC: bool = true;

/// PORT NOTE: `pub const candidates = bes;` (dynamic.zig:48). This IS
/// `backend.zig`'s `.linux => &.{ .io_uring, .epoll }` arm, spelled as a `const`
/// array so the last-candidate default below is a compile-time value.
/// `dynamic_ref_all_decls` asserts it agrees with `Backend::candidates()`.
pub const CANDIDATES: [Backend; 2] = [Backend::IoUring, Backend::Epoll];

/// PORT NOTE: the error set of `Loop::init`/`Async::init`/`Timer::init`/
/// `detect` — the crate's unified D6 seam error (lib.rs).
///
/// The reference spells each of these as a distinct inferred/merged error set
/// (`detect()` is `error{NoAvailableBackends}!void`; `Loop.init` is `!Loop`
/// merging both backends' init errors). This crate collapses every libxev
/// alloc/syscall failure into ONE `Error` (see lib.rs's declaration and
/// epoll.rs, which already returns it), so the Dynamic layer aliases it rather
/// than minting a second name: `lib.rs` must export exactly one `Error` per
/// target, and every backend arm's error maps into it totally.
pub type Error = crate::Error;

/// PORT NOTE: `xev.Async.NotifyError` — the error set of `Async::notify`.
/// Aliased to the crate's unified seam type for the same reason as `Error`; it
/// already carries the `Debug + Clone + Copy + PartialEq + Eq` derives
/// rustty-core's `surface::Error` union requires.
pub type NotifyError = crate::NotifyError;

// ---------------------------------------------------------------------------
// The backend selector (dynamic.zig:90-120).
// ---------------------------------------------------------------------------

/// PORT NOTE: the initializer of `pub var backend: Backend = subset(bes[bes.len
/// - 1]);` (dynamic.zig:91).
///
/// REFERENCE QUIRK (preserved, do NOT "fix"): the doc comment at
/// dynamic.zig:28-30 says the backend "defaults to the FIRST candidate backend",
/// but the code initializes it to the LAST (`bes[bes.len - 1]` → epoll). The
/// code is what everything else depends on — `DynamicCompletion`'s zero value
/// (dynamic.zig:251-255) and `DynamicWriteQueue`'s (dynamic.zig:377-381) both
/// key off the same LAST candidate, so a zero-initialized completion is
/// epoll-tagged. Ported as the code reads; the doc/code mismatch is upstream's.
pub const BACKEND_DEFAULT: Backend = CANDIDATES[CANDIDATES.len() - 1];

/// The backend that is in use.
//
// PORT NOTE: `pub var backend: Backend` (dynamic.zig:91) — a plain mutable Zig
// global. Rust has no mutable statics without `unsafe`, so this is an
// `AtomicU8` holding the `Backend` discriminant. That is not just a lint dodge:
// `Async::notify` is explicitly a CROSS-THREAD call, so the global is read from
// threads other than the one that called `detect`/`prefer`, and a non-atomic
// `static mut` would be a data race. `SeqCst` keeps it simple; the global is
// written at most a handful of times at startup.
pub static BACKEND: AtomicU8 = AtomicU8::new(BACKEND_DEFAULT as u8);

/// Read the backend that is in use (the reference's `backend` global read).
pub fn backend() -> Backend {
    let raw = BACKEND.load(Ordering::SeqCst);
    // PORT NOTE: the reverse of `subset()`/`superset()` (dynamic.zig:178-184),
    // which are pure `@enumFromInt(@intFromEnum(..))` round-trips. `BACKEND`
    // only ever holds a candidate (it starts at `BACKEND_DEFAULT` and is
    // written only by `detect`/`prefer`, both of which store a candidate).
    match raw {
        r if r == Backend::IoUring as u8 => Backend::IoUring,
        r if r == Backend::Epoll as u8 => Backend::Epoll,
        _ => unreachable!("BACKEND holds a non-candidate discriminant"),
    }
}

/// PORT NOTE: `backend = subset(be);` (dynamic.zig:99/114).
fn set_backend(be: Backend) {
    BACKEND.store(be as u8, Ordering::SeqCst);
}

/// PORT NOTE: `be.Api().available()` (dynamic.zig:98/113) — `Api()` returns a
/// comptime `type` in Zig; here each candidate's module IS its Api(), so this is
/// the concrete 2-arm dispatch to `epoll::available`/`io_uring::available`.
/// Non-candidates are never passed by `detect`/`prefer` (both check candidacy
/// first); the arm exists so the match is total.
fn available(be: Backend) -> bool {
    match be {
        Backend::IoUring => io_uring::available(),
        Backend::Epoll => epoll::available(),
        Backend::Kqueue | Backend::WasiPoll | Backend::Iocp => false,
    }
}

/// Detect the preferred backend based on the system and set it
/// for use. "Preferred" is the first available (API exists) candidate
/// backend in the list.
//
// PORT NOTE: `pub fn detect() error{NoAvailableBackends}!void` (dynamic.zig:96).
// The `inline for (bes)` becomes a plain `for` over `CANDIDATES` (the list is a
// runtime array here, not a comptime one). `error.NoAvailableBackends` collapses
// into the crate's unified `Error` (see the `Error` alias above).
pub fn detect() -> core::result::Result<(), Error> {
    for be in CANDIDATES {
        if available(be) {
            set_backend(be);
            return Ok(());
        }
    }

    Err(crate::Error)
}

/// Manually set the backend to use, but if the backend is not
/// available, this will not change the backend in use.
//
// PORT NOTE: `pub fn prefer(be: AllBackend) bool` (dynamic.zig:109) — the
// parameter is the FULL backend enum (`AllBackend`), not the candidate subset,
// and a non-candidate falls out of the `inline for` and returns `false`. That is
// exactly what this `for`/`return false` reproduces.
pub fn prefer(be: Backend) -> bool {
    for candidate in CANDIDATES {
        if candidate == be {
            if !available(candidate) {
                return false;
            }
            set_backend(candidate);
            return true;
        }
    }

    false
}

// PORT NOTE: `watcher/common.zig`'s `userdataValue` — the typed→untyped userdata
// bridge every trampoline below uses. Kept local so this module stays
// self-contained (the crate's `common.rs` is macOS-gated, R5), mirroring the
// identical private copies in epoll.rs/io_uring.rs. Zig `void` userdata ⇒ a null
// pointer; Rust's stand-in is the zero-sized unit `()`.
#[inline]
fn userdata_value<UD: 'static>(v: *mut c_void) -> *mut UD {
    if TypeId::of::<UD>() == TypeId::of::<()>() {
        return ptr::null_mut();
    }
    v as *mut UD
}

// ---------------------------------------------------------------------------
// Loop (dynamic.zig:122-175).
// ---------------------------------------------------------------------------

/// PORT NOTE: `pub const Loop = struct { backend: Loop.Union }` (dynamic.zig:122)
/// where `Union = Dynamic.Union(&.{"Loop"})` — an UNTAGGED union
/// (dynamic.zig:454: the active arm is known from the global, so the union
/// carries no tag of its own).
#[repr(C)]
pub struct Loop {
    backend: LoopUnion,
}

/// PORT NOTE: `Dynamic.Union(&.{"Loop"})` — `union { io_uring: IoUring.Loop,
/// epoll: Epoll.Loop }`, untagged.
#[repr(C)]
union LoopUnion {
    io_uring: ManuallyDrop<io_uring::Loop>,
    epoll: ManuallyDrop<epoll::Loop>,
}

// The `@fieldParentPtr` contract: `backend` at offset 0 of `Loop`, and every
// union arm at offset 0 of the union (both guaranteed by `#[repr(C)]`). This
// makes the down-cast and the up-cast identity casts with NO pointer arithmetic.
const _: () = assert!(offset_of!(Loop, backend) == 0);

// PORT NOTE: `Loop` embeds the backends' `Loop`s, each of which carries its own
// documented `unsafe impl Send` (raw-ptr intrusive queues + fds + mmap'd ring
// pointers). Rust derives the union's auto traits from its arms, so `Loop` is
// `Send` for exactly the same reason and by the same justification: ghostty MOVES
// a `Loop` onto the spawned termio thread, and after the move a `Loop` (and every
// `Completion` it links) is touched by exactly one thread at a time — the owning
// event-loop thread. No explicit `unsafe impl` is needed or wanted here.
//
// This derivation is an implicit chain, so it is PINNED by the `needs_send`
// trait-bound tripwire near the bottom of this file — if an arm ever stops being
// `Send`, that assert fires here rather than at termio's `thread::spawn`.

/// PORT NOTE (N406 in NEW-FINDINGS, parity tail L-5): the union arms are
/// `ManuallyDrop`, so without this the backend loop's Rust-owned fields were
/// never dropped: `deinit` above closes the backend's fds (the reference's
/// whole `deinit`), but epoll's heap-rooted `thread_pool_completions` box
/// (a port addition, see that field's PORT NOTE) leaked with every
/// `Loop`, i.e. with every termio thread on Linux (memcheck: 240 bytes per
/// loop). Dropping the live arm here frees exactly what a backend `Loop` used
/// directly frees when it goes out of scope; no backend loop implements `Drop`,
/// so this runs no teardown of its own after `deinit`. The live arm is named by
/// the same global `deinit` reads (dynamic.zig:32's invariant).
impl Drop for Loop {
    fn drop(&mut self) {
        match backend() {
            // SAFETY: the global names the live arm (see `deinit`), and this is
            // the only place an arm is dropped, once, as the `Loop` itself dies.
            Backend::IoUring => unsafe { ManuallyDrop::drop(&mut self.backend.io_uring) },
            Backend::Epoll => unsafe { ManuallyDrop::drop(&mut self.backend.epoll) },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }
}

impl Loop {
    /// PORT NOTE: `pub fn init(opts: Options) !Loop` (dynamic.zig:127) — the
    /// `switch (backend) { inline else => |tag| @unionInit(Loop.Union,
    /// @tagName(tag), try api.Loop.init(opts)) }` constructor. Reads the GLOBAL,
    /// exactly as the reference does.
    pub fn init(options: Options) -> core::result::Result<Loop, Error> {
        Ok(match backend() {
            Backend::IoUring => Loop {
                backend: LoopUnion {
                    io_uring: ManuallyDrop::new(
                        // PORT NOTE: io_uring's `InitError` (`TooManyEntries` /
                        // ring-setup failure) collapses into the unified seam
                        // `Error` — the `ErrorSet` merge (dynamic.zig:512) of two
                        // backends whose only shared spelling in this crate is
                        // `Error`. The discriminating `TooManyEntries` assertion
                        // still lives on `io_uring::Loop::init` itself
                        // (`io_uring_overflow_entries_count`).
                        io_uring::Loop::init(options).map_err(|_| crate::Error)?,
                    ),
                },
            },
            // PORT NOTE (Task 2): `epoll::Loop::init(options)` reads exactly ONE
            // field — `options.thread_pool` — for the blocking-op thread-pool path
            // (`entries` is io_uring-only upstream, loop.zig:15 "Backends:
            // io_uring"). With the Linux `ThreadPool` body ported (Task 1) it now
            // takes `Options` like `io_uring::Loop::init` + the oracle.
            Backend::Epoll => Loop {
                backend: LoopUnion {
                    epoll: ManuallyDrop::new(epoll::Loop::init(options)?),
                },
            },
            be => unreachable!("non-candidate backend {be:?}"),
        })
    }

    /// PORT NOTE: `pub fn deinit(self: *Loop) void` (dynamic.zig:140).
    pub fn deinit(&mut self) {
        match backend() {
            // SAFETY: the global names the live arm — the reference's own
            // invariant ("a caller can manually set the backend to use, but this
            // must be done before initializing any other types", dynamic.zig:32).
            Backend::IoUring => unsafe { (*Loop::as_io_uring(self)).deinit() },
            Backend::Epoll => unsafe { (*Loop::as_epoll(self)).deinit() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// PORT NOTE: `pub fn run(self: *Loop, mode: RunMode) !void` (dynamic.zig:167).
    //
    // PORT NOTE (N431 in NEW-FINDINGS): a thin entry over `run_raw`, as the
    // backends' own `run` (see epoll.rs `run`): no `&mut Loop` may be live across
    // a callback, and a caller whose callbacks re-enter the loop through a stored
    // pointer must enter through `run_raw` with that pointer.
    pub fn run(&mut self, mode: RunMode) -> core::result::Result<(), Error> {
        // SAFETY: `self` is a live, exclusive `Loop` for the whole call.
        unsafe { Loop::run_raw(self, mode) }
    }

    /// `run(self: *Loop, mode)`: the raw-pointer entry (see `run`).
    ///
    /// # Safety
    /// `this` must point at a live, initialized `Loop` that stays at the same
    /// address and is not otherwise borrowed for the whole call, except by the
    /// loop's own callbacks through raw pointers.
    pub unsafe fn run_raw(this: *mut Loop, mode: RunMode) -> core::result::Result<(), Error> {
        match backend() {
            // SAFETY: see `deinit`.
            // PORT NOTE: io_uring's `RunError` (`NestedRunsNotAllowed`/`Submit`)
            // collapses into the unified seam `Error` (the `ErrorSet` merge);
            // epoll's `run` already returns `Error`.
            Backend::IoUring => unsafe {
                io_uring::Loop::run_raw(Loop::as_io_uring(this), mode).map_err(|_| crate::Error)
            },
            Backend::Epoll => unsafe { epoll::Loop::run_raw(Loop::as_epoll(this), mode) },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// PORT NOTE: `pub fn stop(self: *Loop) void` (dynamic.zig:149).
    pub fn stop(&mut self) {
        match backend() {
            // SAFETY: see `deinit`.
            Backend::IoUring => unsafe { (*Loop::as_io_uring(self)).stop() },
            Backend::Epoll => unsafe { (*Loop::as_epoll(self)).stop() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// PORT NOTE: `pub fn stopped(self: *Loop) bool` (dynamic.zig:158).
    pub fn stopped(&self) -> bool {
        match backend() {
            // SAFETY: see `deinit`.
            Backend::IoUring => unsafe { (*Loop::as_io_uring_const(self)).stopped() },
            Backend::Epoll => unsafe { (*Loop::as_epoll_const(self)).stopped() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Returns the "loop" time in milliseconds. The loop time is updated
    /// once per loop tick, before IO polling occurs. It remains constant
    /// throughout callback execution.
    //
    // PORT NOTE (ADDITION beyond dynamic.zig): `dynamic.zig`'s `Loop` carries
    // ONLY init/deinit/stop/stopped/run. `now`/`update_now` are added here
    // because BOTH candidates expose them identically and they carry no callback
    // ABI (see the "omitted Loop methods" note below), and consumers name
    // `loop.now()`. `&mut self` because io_uring's `now` lazily refreshes its
    // cache (epoll's takes `&self`; widening is harmless).
    pub fn now(&mut self) -> i64 {
        match backend() {
            // SAFETY: see `deinit`.
            Backend::IoUring => unsafe { (*Loop::as_io_uring(self)).now() },
            Backend::Epoll => unsafe { (*Loop::as_epoll(self)).now() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Update the cached time.
    //
    // PORT NOTE: see `now` (ADDITION beyond dynamic.zig).
    pub fn update_now(&mut self) {
        match backend() {
            // SAFETY: see `deinit`.
            Backend::IoUring => unsafe { (*Loop::as_io_uring(self)).update_now() },
            Backend::Epoll => unsafe { (*Loop::as_epoll(self)).update_now() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // PORT NOTE (OMITTED `Loop` methods — deliberate, R1):
    //
    //  * `tick` is epoll-ONLY (epoll.zig:287); `submit` is io_uring-ONLY
    //    (io_uring.zig:226); `delete` is epoll-only (epoll.zig:152); `cancel` is
    //    io_uring-only (io_uring.zig:565). None of them can be dispatched to both
    //    arms, which is exactly the "least common denominator of all backends"
    //    limit the reference states up front (dynamic.zig:20-23) — and none of
    //    them appears on `dynamic.zig`'s `Loop` either.
    //
    //  * `add`/`timer`/`timer_reset` DO exist on both arms, but they take the
    //    backend's `Callback` fn-pointer + yield the backend's `Result`, and the
    //    Dynamic layer deliberately forwards NEITHER (`dynamic.zig:60-88` lists
    //    no `Callback`, `noopCallback` or `Result`). A Dynamic `Completion`'s
    //    op/callback are backend-typed and private, so a consumer could not build
    //    one to hand to `Loop.add` anyway. The supported entry points are the
    //    watchers below (`Timer`/`Async`/`Process`/`Stream`), which reach the
    //    backend loop directly — exactly as `TimerDynamic`/`AsyncDynamic`/the
    //    dynamic stream do via `&@field(loop.backend, @tagName(tag))`
    //    (timer.zig:302, async.zig:698, stream.zig:673).
    // -----------------------------------------------------------------------

    /// The `@field(loop.backend, "io_uring")` down-cast (timer.zig:302 etc.):
    /// a raw projection to the union arm. Identity (`offset_of!(Loop, backend) ==
    /// 0`, union arms at offset 0), so no pointer arithmetic is performed.
    ///
    /// # Safety
    /// `l` must be a valid `Loop` pointer whose live arm is the io_uring one.
    unsafe fn as_io_uring(l: *mut Loop) -> *mut io_uring::Loop {
        unsafe { ptr::addr_of_mut!((*l).backend) }.cast::<io_uring::Loop>()
    }

    /// See `as_io_uring`.
    ///
    /// # Safety
    /// `l` must be a valid `Loop` pointer whose live arm is the epoll one.
    unsafe fn as_epoll(l: *mut Loop) -> *mut epoll::Loop {
        unsafe { ptr::addr_of_mut!((*l).backend) }.cast::<epoll::Loop>()
    }

    /// The shared-reference twin of `as_io_uring`, for the `&self` accessors.
    ///
    /// # Safety
    /// See `as_io_uring`.
    unsafe fn as_io_uring_const(l: *const Loop) -> *const io_uring::Loop {
        unsafe { ptr::addr_of!((*l).backend) }.cast::<io_uring::Loop>()
    }

    /// See `as_io_uring_const`.
    ///
    /// # Safety
    /// See `as_epoll`.
    unsafe fn as_epoll_const(l: *const Loop) -> *const epoll::Loop {
        unsafe { ptr::addr_of!((*l).backend) }.cast::<epoll::Loop>()
    }

    /// The `@fieldParentPtr("backend", @fieldParentPtr(@tagName(tag), l_inner))`
    /// up-cast every dynamic watcher trampoline performs (timer.zig:290-293,
    /// async.zig:681-684, stream.zig:656-659) — recover the enclosing `Loop` from
    /// a pointer to its live union arm. An identity cast (see `as_io_uring`).
    fn from_io_uring(l: *mut io_uring::Loop) -> *mut Loop {
        l.cast::<Loop>()
    }

    /// See `from_io_uring`.
    fn from_epoll(l: *mut epoll::Loop) -> *mut Loop {
        l.cast::<Loop>()
    }
}

// ---------------------------------------------------------------------------
// Completion (dynamic.zig:240-292 `DynamicCompletion`).
// ---------------------------------------------------------------------------

/// A completion is a request to perform some work with the loop.
//
// PORT NOTE (verbatim, dynamic.zig:244-248): Completions, unlike almost any
// other dynamic type in this file, are tagged unions. This adds a minimal
// overhead to the completion but is necessary to ensure that we have
// zero-initialized the correct type for where it matters (timers).
//
// PORT NOTE: `DynamicCompletion` is `struct { value: TaggedUnion }`; Zig's
// `union(enum)` payload/tag placement is unspecified, so the tag is written out
// by hand here as a sibling field with `value` FIRST — that pins the payload at
// offset 0, which is what makes the `@fieldParentPtr` up-cast an identity cast.
//
// PORT NOTE: NOT `Copy` (and so not `Clone`) — unlike Zig, where every struct is
// trivially copyable. Both backends' `Completion`s are intrusive queue nodes
// with raw `next` links and are deliberately non-`Copy` in this port; copying one
// would duplicate a live loop registration. `Default` is hand-written (never
// derived), per the port's discipline.
#[repr(C)]
pub struct Completion {
    value: CompletionUnion,
    tag: Backend,
}

/// PORT NOTE: `Dynamic.TaggedUnion(&.{"Completion"})`'s payload (dynamic.zig:249)
/// — the tag lives in `Completion::tag`.
#[repr(C)]
union CompletionUnion {
    io_uring: ManuallyDrop<io_uring::Completion>,
    epoll: ManuallyDrop<epoll::Completion>,
}

// The `@fieldParentPtr` contract — see the same assert on `Loop`.
const _: () = assert!(offset_of!(Completion, value) == 0);

// PORT NOTE (size tripwire, mirroring epoll.rs/io_uring.rs's own
// `size_of::<Completion>()` asserts): the Dynamic completion is the LARGER of the
// two backend completions (the union) plus the tag, rounded to alignment.
// RE-MEASURED empirically on aarch64-unknown-linux-gnu via a temporary probe
// (since removed):
//
//   epoll::Completion = 208, io_uring::Completion = 152  → CompletionUnion = 208
//   Backend = 4 (a `#[repr(C)]` tag-only enum)           → +4, padded to align 8
//   size_of::<Completion>() = 216, align 8
//   offset_of!(Completion, value) = 0, offset_of!(Completion, tag) = 208
//
// This is the "minimal overhead" the reference's doc comment promises for making
// completions tagged (dynamic.zig:244-247): ONE machine word over the widest
// backend completion. (Task 2 grew epoll::Completion 152 → 208 by adding the
// thread-pool `task`/`task_loop`/`task_result` fields, so the union — and this
// pin — grew 160 → 216.) RE-MEASURE (and update) whenever either backend's
// `Completion` grows — the assert is what forces that.
//
// RE-MEASURED (WS19 T10): 216 → 232. `epoll::Completion` grew 208 → 224 (the
// pool-worker aliasing fix's `task_completions` + `task_wakeup`; see epoll.rs's
// `COMPLETION_SIZE` note), so the union is 224 and the tag word takes it to 232.
// `io_uring::Completion` is unaffected — the io_uring backend has no thread-pool
// path at all (`grep -n 'task_loop\|thread_perform' io_uring.rs` → 0 hits).
const _: () = assert!(size_of::<Completion>() == 232);

impl Default for Completion {
    /// PORT NOTE (dynamic.zig:251-255):
    ///   value: Self.Union = @unionInit(
    ///       Self.Union,
    ///       @tagName(dynamic.candidates[dynamic.candidates.len - 1]),
    ///       .{},
    ///   ),
    /// The zero value is the LAST candidate's variant (epoll) — NOT the detected
    /// backend, which isn't known at compile time. `ensure_tag` (below) is what
    /// reconciles a zero-initialized completion with the live backend.
    fn default() -> Self {
        Completion::zeroed_for(BACKEND_DEFAULT)
    }
}

impl Completion {
    /// The `@unionInit(Self.Union, @tagName(tag), .{})` builder shared by the
    /// zero value (dynamic.zig:251) and `ensureTag` (dynamic.zig:285).
    fn zeroed_for(tag: Backend) -> Completion {
        match tag {
            Backend::IoUring => Completion {
                value: CompletionUnion {
                    io_uring: ManuallyDrop::new(io_uring::Completion::default()),
                },
                tag,
            },
            Backend::Epoll => Completion {
                value: CompletionUnion {
                    epoll: ManuallyDrop::new(epoll::Completion::default()),
                },
                tag,
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// The active backend tag (`self.value`'s tag in the reference's
    /// `union(enum)`).
    pub fn tag(&self) -> Backend {
        self.tag
    }

    /// PORT NOTE: `pub fn state(self: *Self) dynamic.CompletionState`
    /// (dynamic.zig:270) — `switch (self.value) { inline else => |*v| v.state() }`.
    /// Switches on the COMPLETION'S OWN tag (not the global), exactly as the
    /// reference's switch-on-the-union does.
    pub fn state(&self) -> CompletionState {
        match self.tag {
            // SAFETY: `tag` names the live arm — the invariant `zeroed_for`/
            // `ensure_tag` are the only writers of, and they always set both
            // together.
            Backend::IoUring => unsafe { (*Completion::as_io_uring_const(self)).state_of() },
            Backend::Epoll => unsafe { (*Completion::as_epoll_const(self)).state_of() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Convenience alias of `state` under the spelling the macOS seam exports
    /// (LT8).
    //
    // PORT NOTE: the reference names this decl `state` on EVERY backend
    // (kqueue.zig:1035, epoll.zig:941, io_uring.zig:649) and on the Dynamic
    // completion (dynamic.zig:270) — which is why `state` above is the faithful
    // spelling and stays. But `kqueue.rs` could not use that name for its public
    // `&self` form: its raw-pointer discipline already owns `state` for the
    // `fn state(c: *mut Completion)` associated fn, so the public accessor there
    // is `state_of` (kqueue.rs:1648-1659) — and `state_of` is therefore the name
    // the macOS `xev::Completion` answers to, and the one termio's completions
    // already call (exec.rs:799, thread.rs:1022/1173). The same rename already
    // happened one level down: `epoll::Completion`/`io_uring::Completion` expose
    // `state_of` too, which is what `state` above dispatches to.
    //
    // lib.rs's job is to make ONE surface out of two backends; a `pub use` can
    // rename a TYPE but not a method, so the Linux side grows the macOS spelling
    // as an alias rather than the seam silently differing per target. Both names
    // are the same reference decl; `tests/surface_parity.rs` pins `state_of` on
    // both targets. DEFERRALS.md records this as a known cross-target divergence.
    #[inline]
    pub fn state_of(&self) -> CompletionState {
        self.state()
    }

    /// This ensures that the tag is currently set for this
    /// completion. If it is not, it will be set to the given
    /// tag with a zero-initialized value.
    ///
    /// This lets users zero-intialize the completion and then
    /// call any watcher API on it without having to worry about
    /// the correct detected backend tag being set.
    //
    // PORT NOTE: `pub fn ensureTag(self: *Self, comptime tag: dynamic.Backend)`
    // (dynamic.zig:283). The Zig `comptime tag` is a runtime parameter here (the
    // whole point of the concrete instantiation); the body is the reference's
    // `if (self.value == tag) return; self.value = @unionInit(..., .{});`.
    pub fn ensure_tag(&mut self, tag: Backend) {
        // SAFETY: `self` is a live, uniquely-borrowed `Completion`.
        unsafe { Completion::ensure_tag_raw(self, tag) }
    }

    /// The raw-pointer form of `ensure_tag`, used by every watcher below so that
    /// no `&mut Completion` is ever minted from the caller's raw pointer (a
    /// `&mut`-derived interior pointer would be invalidated by later accesses
    /// through the outer raw pointer under Stacked Borrows).
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer that is not currently
    /// registered with a loop under a different tag.
    unsafe fn ensure_tag_raw(c: *mut Completion, tag: Backend) {
        if unsafe { (*c).tag } == tag {
            return;
        }
        // `Completion` has no `Drop` glue (the union arms are `ManuallyDrop` and
        // neither backend's `Completion` implements `Drop`), so overwriting is a
        // plain store — the reference's `self.value = @unionInit(...)`.
        unsafe { ptr::write(c, Completion::zeroed_for(tag)) };
    }

    /// The `&@field(c.value, @tagName(tag))` down-cast (timer.zig:303 etc.).
    /// Identity — see `Loop::as_io_uring`.
    ///
    /// # Safety
    /// `c` must be a valid `Completion` pointer whose live arm is the io_uring one.
    unsafe fn as_io_uring(c: *mut Completion) -> *mut io_uring::Completion {
        unsafe { ptr::addr_of_mut!((*c).value) }.cast::<io_uring::Completion>()
    }

    /// See `as_io_uring`.
    ///
    /// # Safety
    /// `c` must be a valid `Completion` pointer whose live arm is the epoll one.
    unsafe fn as_epoll(c: *mut Completion) -> *mut epoll::Completion {
        unsafe { ptr::addr_of_mut!((*c).value) }.cast::<epoll::Completion>()
    }

    /// The shared-reference twin of `as_io_uring`, for the `&self` accessors.
    ///
    /// # Safety
    /// See `as_io_uring`.
    unsafe fn as_io_uring_const(c: *const Completion) -> *const io_uring::Completion {
        unsafe { ptr::addr_of!((*c).value) }.cast::<io_uring::Completion>()
    }

    /// See `as_io_uring_const`.
    ///
    /// # Safety
    /// See `as_epoll`.
    unsafe fn as_epoll_const(c: *const Completion) -> *const epoll::Completion {
        unsafe { ptr::addr_of!((*c).value) }.cast::<epoll::Completion>()
    }

    /// The `@fieldParentPtr("value", @fieldParentPtr(@tagName(tag), c_inner))`
    /// up-cast every dynamic watcher trampoline performs (timer.zig:294-297,
    /// async.zig:685-688, stream.zig:660-663). Identity — see `Loop::as_io_uring`.
    fn from_io_uring(c: *mut io_uring::Completion) -> *mut Completion {
        c.cast::<Completion>()
    }

    /// See `from_io_uring`.
    fn from_epoll(c: *mut epoll::Completion) -> *mut Completion {
        c.cast::<Completion>()
    }
}

// ---------------------------------------------------------------------------
// ReadBuffer / WriteBuffer / PollEvent (dynamic.zig:294-424).
// ---------------------------------------------------------------------------

/// ReadBuffer are the various options for reading.
//
// PORT NOTE (verbatim, dynamic.zig:294-298): Our read buffer supports a least
// common denominator of read targets available by all backends. The direct
// backend may support more read targets but if you need that you should use the
// backend directly and not through the dynamic API.
//
// PORT NOTE (R3): the Zig `slice: []u8` ports as a raw fat pointer `*mut [u8]`
// (never a lifetime — the buffer lives in caller memory and the loop/kernel
// re-enters it across ticks), matching epoll.rs/io_uring.rs.
#[derive(Clone, Copy)]
pub enum ReadBuffer {
    /// Read into this slice.
    Slice(*mut [u8]),

    /// Read into this array, just set this to undefined and it will
    /// be populated up to the size of the array.
    Array([u8; 32]),
}

impl ReadBuffer {
    /// PORT NOTE: `pub fn toBackend(self, comptime be) ...ReadBuffer`
    /// (dynamic.zig:321).
    fn to_io_uring(self) -> io_uring::ReadBuffer {
        match self {
            ReadBuffer::Slice(s) => io_uring::ReadBuffer::Slice(s),
            ReadBuffer::Array(a) => io_uring::ReadBuffer::Array(a),
        }
    }

    /// See `to_io_uring`.
    fn to_epoll(self) -> epoll::ReadBuffer {
        match self {
            ReadBuffer::Slice(s) => epoll::ReadBuffer::Slice(s),
            ReadBuffer::Array(a) => epoll::ReadBuffer::Array(a),
        }
    }

    /// PORT NOTE: `pub fn fromBackend(comptime be, buf) Self` (dynamic.zig:306).
    fn from_io_uring(b: io_uring::ReadBuffer) -> ReadBuffer {
        match b {
            io_uring::ReadBuffer::Slice(s) => ReadBuffer::Slice(s),
            io_uring::ReadBuffer::Array(a) => ReadBuffer::Array(a),
        }
    }

    /// See `from_io_uring`.
    fn from_epoll(b: epoll::ReadBuffer) -> ReadBuffer {
        match b {
            epoll::ReadBuffer::Slice(s) => ReadBuffer::Slice(s),
            epoll::ReadBuffer::Array(a) => ReadBuffer::Array(a),
        }
    }
}

/// WriteBuffer are the various options for writing.
//
// PORT NOTE (verbatim, dynamic.zig:336-340): Our write buffer supports a least
// common denominator of write targets available by all backends. The direct
// backend may support more write targets but if you need that you should use the
// backend directly and not through the dynamic API.
#[derive(Clone, Copy)]
pub enum WriteBuffer {
    /// Write from this buffer.
    Slice(*const [u8]),

    /// Write from this array. See ReadBuffer.array for why we support this.
    Array { array: [u8; 32], len: usize },
}

impl WriteBuffer {
    /// PORT NOTE: `pub fn toBackend(self, comptime be)` (dynamic.zig:360).
    fn to_io_uring(self) -> io_uring::WriteBuffer {
        match self {
            WriteBuffer::Slice(s) => io_uring::WriteBuffer::Slice(s),
            WriteBuffer::Array { array, len } => io_uring::WriteBuffer::Array { array, len },
        }
    }

    /// See `to_io_uring`.
    fn to_epoll(self) -> epoll::WriteBuffer {
        match self {
            WriteBuffer::Slice(s) => epoll::WriteBuffer::Slice(s),
            WriteBuffer::Array { array, len } => epoll::WriteBuffer::Array { array, len },
        }
    }

    /// PORT NOTE: `pub fn fromBackend(comptime be, buf) Self` (dynamic.zig:348).
    fn from_io_uring(b: io_uring::WriteBuffer) -> WriteBuffer {
        match b {
            io_uring::WriteBuffer::Slice(s) => WriteBuffer::Slice(s),
            io_uring::WriteBuffer::Array { array, len } => WriteBuffer::Array { array, len },
        }
    }

    /// See `from_io_uring`.
    fn from_epoll(b: epoll::WriteBuffer) -> WriteBuffer {
        match b {
            epoll::WriteBuffer::Slice(s) => WriteBuffer::Slice(s),
            epoll::WriteBuffer::Array { array, len } => WriteBuffer::Array { array, len },
        }
    }
}

/// Events that can be polled for using the high level streams.
//
// PORT NOTE: `DynamicPollEvent` (dynamic.zig:394) — `enum { read }` +
// `fromBackend`/`toBackend`. Unlike the static `PollEvent` (stream.zig:71), the
// dynamic one carries NO backend-specific integer value: each backend's own
// `PollEvent::Read` supplies `posix.POLL.IN`/`linux.EPOLL.IN` on conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollEvent {
    Read,
}

impl PollEvent {
    /// PORT NOTE: `pub fn toBackend(self, comptime tag)` (dynamic.zig:412).
    fn to_io_uring(self) -> io_uring::PollEvent {
        match self {
            PollEvent::Read => io_uring::PollEvent::Read,
        }
    }

    /// See `to_io_uring`.
    fn to_epoll(self) -> epoll::PollEvent {
        match self {
            PollEvent::Read => epoll::PollEvent::Read,
        }
    }

    /// PORT NOTE: `pub fn fromBackend(comptime tag, event) Self` (dynamic.zig:400).
    fn from_io_uring(e: io_uring::PollEvent) -> PollEvent {
        match e {
            io_uring::PollEvent::Read => PollEvent::Read,
        }
    }

    /// See `from_io_uring`.
    fn from_epoll(e: epoll::PollEvent) -> PollEvent {
        match e {
            epoll::PollEvent::Read => PollEvent::Read,
        }
    }
}

// ---------------------------------------------------------------------------
// Error sets (dynamic.zig:72-79 + the `ErrorSet` generator, dynamic.zig:512).
//
// PORT NOTE: `ErrorSet(bes, field)` merges the two backends' error sets with
// Zig's `Set || NextSet`. Rust has no error-set union type, so each merged set is
// an enum whose variants are the UNION of both backends' variants, and every
// dispatch site maps the backend's error into it TOTALLY (Zig gets that mapping
// for free from error-set coercion). Each `PORT NOTE` below records the two
// sources being merged.
// ---------------------------------------------------------------------------

/// PORT NOTE: `ErrorSet(&.{"AcceptError"})` = io_uring `{Canceled, Again,
/// Unexpected}` || epoll `{Unknown, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptError {
    Canceled,
    Again,
    Unknown,
    Unexpected,
}

/// PORT NOTE: `ErrorSet(&.{"CancelError"})` = io_uring `{NotFound,
/// ExpirationInProgress, Unexpected}` || epoll `{NotFound}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelError {
    NotFound,
    ExpirationInProgress,
    Unexpected,
}

/// PORT NOTE: `ErrorSet(&.{"CloseError"})` = io_uring `{Canceled, Unexpected}`
/// || epoll `{ThreadPoolRequired, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseError {
    Canceled,
    ThreadPoolRequired,
    Unexpected,
}

/// PORT NOTE: `ErrorSet(&.{"ConnectError"})` = io_uring `{Canceled, Unexpected,
/// ConnectionRefused, HostUnreachable, TimedOut}` || epoll `{Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    Canceled,
    ConnectionRefused,
    HostUnreachable,
    TimedOut,
    Unexpected,
}

/// PORT NOTE: `ErrorSet(&.{"PollError"})` = io_uring `{Canceled, Unexpected}` ||
/// epoll `{Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `ErrorSet(&.{"ShutdownError"})` = io_uring `{Canceled,
/// Unexpected}` || epoll `{Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `ErrorSet(&.{"WriteError"})` = io_uring `{Canceled, BrokenPipe,
/// ConnectionResetByPeer, Unexpected}` || epoll `{ThreadPoolRequired, Unexpected}`
/// (the epoll `WriteError` gained `ThreadPoolRequired` with the File watcher's
/// threadpool-flagged write/pwrite, Task 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    Canceled,
    BrokenPipe,
    ConnectionResetByPeer,
    ThreadPoolRequired,
    Unexpected,
}

/// PORT NOTE: `ErrorSet(&.{"ReadError"})` = io_uring `{EOF, Canceled,
/// Unexpected, ConnectionResetByPeer}` || epoll `{ThreadPoolRequired, EOF,
/// Unexpected}` (the epoll `ReadError` gained `ThreadPoolRequired` with the File
/// watcher's threadpool-flagged read/pread, Task 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum ReadError {
    /// PORT NOTE: keeps the reference's acronym spelling (`error.EOF`).
    EOF,
    Canceled,
    ConnectionResetByPeer,
    ThreadPoolRequired,
    Unexpected,
}

impl AcceptError {
    fn from_io_uring(e: io_uring::AcceptError) -> AcceptError {
        match e {
            io_uring::AcceptError::Canceled => AcceptError::Canceled,
            io_uring::AcceptError::Again => AcceptError::Again,
            io_uring::AcceptError::Unexpected => AcceptError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::AcceptError) -> AcceptError {
        match e {
            epoll::AcceptError::Unknown => AcceptError::Unknown,
            epoll::AcceptError::Unexpected => AcceptError::Unexpected,
        }
    }
}

impl ConnectError {
    fn from_io_uring(e: io_uring::ConnectError) -> ConnectError {
        match e {
            io_uring::ConnectError::Canceled => ConnectError::Canceled,
            io_uring::ConnectError::ConnectionRefused => ConnectError::ConnectionRefused,
            io_uring::ConnectError::HostUnreachable => ConnectError::HostUnreachable,
            io_uring::ConnectError::TimedOut => ConnectError::TimedOut,
            io_uring::ConnectError::Unexpected => ConnectError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::ConnectError) -> ConnectError {
        match e {
            epoll::ConnectError::Unexpected => ConnectError::Unexpected,
        }
    }
}

impl ShutdownError {
    fn from_io_uring(e: io_uring::ShutdownError) -> ShutdownError {
        match e {
            io_uring::ShutdownError::Canceled => ShutdownError::Canceled,
            io_uring::ShutdownError::Unexpected => ShutdownError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::ShutdownError) -> ShutdownError {
        match e {
            epoll::ShutdownError::Unexpected => ShutdownError::Unexpected,
        }
    }
}

impl CancelError {
    fn from_io_uring(e: io_uring::CancelError) -> CancelError {
        match e {
            io_uring::CancelError::NotFound => CancelError::NotFound,
            io_uring::CancelError::ExpirationInProgress => CancelError::ExpirationInProgress,
            io_uring::CancelError::Unexpected => CancelError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::CancelError) -> CancelError {
        match e {
            epoll::CancelError::NotFound => CancelError::NotFound,
        }
    }
}

impl CloseError {
    fn from_io_uring(e: io_uring::CloseError) -> CloseError {
        match e {
            io_uring::CloseError::Canceled => CloseError::Canceled,
            io_uring::CloseError::Unexpected => CloseError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::CloseError) -> CloseError {
        match e {
            epoll::CloseError::ThreadPoolRequired => CloseError::ThreadPoolRequired,
            epoll::CloseError::Unexpected => CloseError::Unexpected,
        }
    }
}

impl PollError {
    fn from_io_uring(e: io_uring::PollError) -> PollError {
        match e {
            io_uring::PollError::Canceled => PollError::Canceled,
            io_uring::PollError::Unexpected => PollError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::PollError) -> PollError {
        match e {
            epoll::PollError::Unexpected => PollError::Unexpected,
        }
    }
}

impl WriteError {
    fn from_io_uring(e: io_uring::WriteError) -> WriteError {
        match e {
            io_uring::WriteError::Canceled => WriteError::Canceled,
            io_uring::WriteError::BrokenPipe => WriteError::BrokenPipe,
            io_uring::WriteError::ConnectionResetByPeer => WriteError::ConnectionResetByPeer,
            io_uring::WriteError::Unexpected => WriteError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::WriteError) -> WriteError {
        match e {
            epoll::WriteError::ThreadPoolRequired => WriteError::ThreadPoolRequired,
            epoll::WriteError::Unexpected => WriteError::Unexpected,
        }
    }
}

impl ReadError {
    fn from_io_uring(e: io_uring::ReadError) -> ReadError {
        match e {
            io_uring::ReadError::EOF => ReadError::EOF,
            io_uring::ReadError::Canceled => ReadError::Canceled,
            io_uring::ReadError::ConnectionResetByPeer => ReadError::ConnectionResetByPeer,
            io_uring::ReadError::Unexpected => ReadError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::ReadError) -> ReadError {
        match e {
            epoll::ReadError::ThreadPoolRequired => ReadError::ThreadPoolRequired,
            epoll::ReadError::EOF => ReadError::EOF,
            epoll::ReadError::Unexpected => ReadError::Unexpected,
        }
    }
}

mod async_process;
pub use self::async_process::*;

mod timer;
pub use self::timer::*;

mod stream;
pub use self::stream::*;

mod tcp;
pub use self::tcp::*;

mod udp;
pub use self::udp::*;

mod file;
pub use self::file::*;

// ---------------------------------------------------------------------------
// Trait-bound tripwires (brief Step 3: "Assert the trait bounds with
// `const _: fn() = || { fn needs<T: Copy>(){} needs::<Async>(); };`-style
// compile checks").
// ---------------------------------------------------------------------------
//
// These are compile-time-only: a `const _: fn()` binding forces the closure to
// type-check (and so the bounds to be satisfied) without emitting any code or
// running anything. They exist because the bounds below are load-bearing for
// CONSUMERS of this module, and every one of them is satisfied IMPLICITLY here —
// by auto-trait derivation or by `#[derive]` — rather than by an explicit `impl`
// that a reader (or a future edit) would have to notice. Without these, breaking
// one of them is silent at this layer and surfaces as a confusing error at a
// distance in termio.
//
// The `Send` set is the sharp one. This module deliberately writes NO
// `unsafe impl Send` (see the PORT NOTE above `Loop`, and the matching ones on
// `Completion`/`WriteRequest`): each type is a union whose arms already carry
// their own documented `unsafe impl Send` (epoll.rs:205/1104/2064,
// io_uring.rs:1233/1978/2916), `ManuallyDrop` is `repr(transparent)` and
// transparent to auto traits, and Rust derives a union's auto traits from its
// arms — so `Send` arrives for free, for exactly the arms' reasons. That
// argument is PROSE, and these asserts are the only machine-checkable statement
// of it: they fail HERE, instead of at the termio `thread::spawn` that moves a
// `Loop` onto the event-loop thread.
//
// What actually trips them (verified by negative control, not assumed): because
// each arm carries an EXPLICIT `unsafe impl Send`, an arm merely gaining a
// non-`Send` field is NOT enough — the explicit impl keeps the arm `Send` and
// the chain holds. The tripwire fires when an arm genuinely stops being `Send`:
// its `unsafe impl` is deleted while a non-`Send` field is present, or an arm is
// retyped/added (a third backend) that isn't `Send`. Confirmed by temporarily
// removing epoll.rs's `unsafe impl Send for Loop` AND adding a
// `PhantomData<*const ()>` field: `needs_send::<Loop>()` then fails with
// "`*const ()` cannot be sent between threads safely", pointing through
// `LoopUnion` to the offending field.
//
// NOTE: the tripwire pins the PROPERTY, not the justification — if one fires,
// the fix is to re-examine the offending arm's thread-affinity story, NOT to
// paper over it with an `unsafe impl Send` on the union.
const _: fn() = || {
    // `Send`: asserted, never `unsafe impl`'d — see above.
    fn needs_send<T: Send>() {}
    needs_send::<Loop>();
    needs_send::<Completion>();
    needs_send::<WriteRequest>();

    // `Copy`: the brief marks `Async: Copy` REQUIRED (ghostty stores an `Async`
    // by value and copies it onto other threads to `notify()`); `Timer`/`Stream`
    // are `Copy` for the same by-value-handle reason. Each is a `#[derive]`,
    // which a future field addition (e.g. a non-`Copy` arm) would silently break.
    fn needs_copy<T: Copy>() {}
    needs_copy::<Async>();
    needs_copy::<Timer>();
    needs_copy::<Stream>();
    needs_copy::<File>();

    // `Default`: REQUIRED by termio (Exec default-constructs `write_queue`) and
    // by `queue_write`'s wholesale `..WriteRequest::default()` re-init.
    // Hand-written here, never derived — a derive would pick the wrong union arm.
    fn needs_default<T: Default>() {}
    needs_default::<Completion>();
    needs_default::<WriteQueue>();
    needs_default::<WriteRequest>();

    // `NotifyError: Debug + Clone + Copy + PartialEq + Eq` (brief's merged
    // `ErrorSet`, dynamic.zig:512). Satisfied by the crate's unified seam type
    // (lib.rs) that this module re-exports; pinned here so the re-export cannot
    // regress the bound out from under callers.
    fn needs_error_bounds<T: core::fmt::Debug + Clone + Copy + PartialEq + Eq>() {}
    needs_error_bounds::<NotifyError>();
};

// ---------------------------------------------------------------------------
// Tests — the 5 `dynamic.zig` `test{}` blocks + the live KATs that prove the
// dispatch really reaches both backends.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests;
