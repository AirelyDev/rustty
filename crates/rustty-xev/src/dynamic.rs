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
            // PORT NOTE: `epoll::Loop::init()` takes no `Options`. The reference's
            // `epoll.Loop.init(options)` reads exactly ONE field —
            // `options.thread_pool` — and the Linux `ThreadPool` body is a
            // ledgered deferral (epoll.rs's Loop PORT NOTE: the pool machinery is
            // omitted, so `Options` is not consumed there yet). `entries` is
            // io_uring-only upstream too (loop.zig:15 "Backends: io_uring"), so
            // dropping `Options` on this arm loses nothing that is ported.
            Backend::Epoll => Loop {
                backend: LoopUnion {
                    epoll: ManuallyDrop::new(epoll::Loop::init()?),
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
    pub fn run(&mut self, mode: RunMode) -> core::result::Result<(), Error> {
        match backend() {
            // SAFETY: see `deinit`.
            // PORT NOTE: io_uring's `RunError` (`NestedRunsNotAllowed`/`Submit`)
            // collapses into the unified seam `Error` (the `ErrorSet` merge);
            // epoll's `run` already returns `Error`.
            Backend::IoUring => unsafe {
                (*Loop::as_io_uring(self))
                    .run(mode)
                    .map_err(|_| crate::Error)
            },
            Backend::Epoll => unsafe { (*Loop::as_epoll(self)).run(mode) },
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
//   epoll::Completion = 152, io_uring::Completion = 152  → CompletionUnion = 152
//   Backend = 4 (a `#[repr(C)]` tag-only enum)           → +4, padded to align 8
//   size_of::<Completion>() = 160, align 8
//   offset_of!(Completion, value) = 0, offset_of!(Completion, tag) = 152
//
// This is the "minimal overhead" the reference's doc comment promises for making
// completions tagged (dynamic.zig:244-247): ONE machine word over the widest
// backend completion. RE-MEASURE (and update) whenever either backend's
// `Completion` grows — the assert is what forces that.
const _: () = assert!(size_of::<Completion>() == 160);

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
/// ConnectionResetByPeer, Unexpected}` || epoll `{Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    Canceled,
    BrokenPipe,
    ConnectionResetByPeer,
    Unexpected,
}

/// PORT NOTE: `ErrorSet(&.{"ReadError"})` = io_uring `{EOF, Canceled,
/// Unexpected, ConnectionResetByPeer}` || epoll `{EOF, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum ReadError {
    /// PORT NOTE: keeps the reference's acronym spelling (`error.EOF`).
    EOF,
    Canceled,
    ConnectionResetByPeer,
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
            epoll::ReadError::EOF => ReadError::EOF,
            epoll::ReadError::Unexpected => ReadError::Unexpected,
        }
    }
}

// ---------------------------------------------------------------------------
// Async (watcher/async.zig `AsyncDynamic`, async.zig:614-712).
// ---------------------------------------------------------------------------

/// PORT NOTE: `pub const WaitError = xev.ErrorSet(&.{"Async", "WaitError"})`
/// (async.zig:621). Each backend's `Async.WaitError` IS its `ReadError`
/// (async.zig:35), so the merge is exactly this module's `ReadError`.
pub type WaitError = ReadError;

/// "Wake up" an event loop from any thread using an async completion.
//
// PORT NOTE: `AsyncDynamic(xev)` (async.zig:614) is `struct { backend: Union }`
// over `xev.Union(&.{"Async"})`. Spelled as a Rust `enum` rather than a
// `union`+global because both arms are `Copy` fd handles that are passed BY VALUE
// (`wait(self: Self)`, `notify(self: Self)`) and never pointer-projected — so
// nothing here needs the `@fieldParentPtr` layout guarantee, and a Rust enum is
// both safe and self-describing. `Copy` is REQUIRED by termio/core (`xev::Async`
// is held and copied by value).
#[derive(Clone, Copy)]
pub enum Async {
    IoUring(io_uring::Async),
    Epoll(epoll::Async),
}

impl Async {
    /// PORT NOTE: `pub fn init() !Self` (async.zig:623) — the `switch
    /// (xev.backend)` constructor.
    pub fn init() -> core::result::Result<Async, Error> {
        Ok(match backend() {
            Backend::IoUring => Async::IoUring(io_uring::Async::init()?),
            Backend::Epoll => Async::Epoll(epoll::Async::init()?),
            be => unreachable!("non-candidate backend {be:?}"),
        })
    }

    /// PORT NOTE: `pub fn deinit(self: *Self) void` (async.zig:636).
    //
    // PORT NOTE: switches on the SELF tag, not the global. The reference switches
    // on `xev.backend` (the global) because its union is untagged; a Rust enum
    // carries its own tag, which is strictly safer and observationally identical
    // whenever the global has not been changed under a live handle (which the
    // reference forbids anyway, dynamic.zig:32-33).
    pub fn deinit(&self) {
        match self {
            Async::IoUring(a) => a.deinit(),
            Async::Epoll(a) => a.deinit(),
        }
    }

    /// Notify a loop to wake up synchronously. This should never block forever
    /// (it will always EVENTUALLY succeed regardless of if the loop is currently
    /// ticking or not).
    //
    // PORT NOTE: `pub fn notify(self: *Self) !void` (async.zig:645). The
    // backends' `notify` returns the unified seam `Error`; the crate's public
    // spelling for this call's error is `NotifyError` (lib.rs), so the total
    // `Error → NotifyError` mapping is spelled out.
    pub fn notify(&self) -> core::result::Result<(), NotifyError> {
        match self {
            Async::IoUring(a) => a.notify().map_err(|_| crate::NotifyError),
            Async::Epoll(a) => a.notify().map_err(|_| crate::NotifyError),
        }
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
    // PORT NOTE: `pub fn wait(...)` (async.zig:654) — `c.ensureTag(tag)`, then
    // the backend's `wait` on `&@field(loop.backend, tag)` /
    // `&@field(c.value, tag)` with an `api_cb` that `@fieldParentPtr`s the inner
    // `*api.Loop`/`*api.Completion` back to the OUTER `*xev.Loop`/`*xev.Completion`.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `wait::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE. The per-arm `api_cb` is a non-capturing
    // closure that captures only the ZST `cb`, so it is itself a ZST and
    // satisfies the backend's own comptime-cb contract.
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
                core::result::Result<(), WaitError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Async::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };

        match (self, backend()) {
            (Async::IoUring(a), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                a.wait(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            r.map_err(ReadError::from_io_uring),
                        )
                    },
                );
            },
            (Async::Epoll(a), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                a.wait(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            r.map_err(ReadError::from_epoll),
                        )
                    },
                );
            },
            // PORT NOTE: unreachable in the reference — Zig's `switch
            // (xev.backend)` on an untagged union simply reads the arm the global
            // names. Here the `Async` handle carries its own tag, so a mismatch
            // means the caller changed the global under a live handle, which
            // dynamic.zig:32-33 forbids. Panic rather than silently misread.
            (a, be) => panic!(
                "Async handle backend {:?} does not match the global backend {be:?} — \
                 the backend must be set before initializing any other types",
                match a {
                    Async::IoUring(_) => Backend::IoUring,
                    Async::Epoll(_) => Backend::Epoll,
                }
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Timer (watcher/timer.zig `TimerDynamic`, timer.zig:229-430).
// ---------------------------------------------------------------------------

/// Error that could happen while running a timer.
//
// PORT NOTE: `pub const RunError = xev.ErrorSet(&.{"Timer", "RunError"})`
// (timer.zig:236). Each backend's `Timer.RunError` is the SAME literal set
// (timer.zig:213-219 — the watcher declares it, not the backend), so the merge is
// that set. Named `TimerRunError` here because everything shares one module (the
// macOS surface exports the same type as `xev::TimerRunError`, lib.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerRunError {
    /// The timer was canceled before it could expire
    Canceled,
    /// Some unexpected error.
    Unexpected,
}

/// PORT NOTE: `pub const CancelError = xev.ErrorSet(&.{"Timer", "CancelError"})`
/// (timer.zig:237); each backend's `Timer.CancelError` is `xev.CancelError`
/// (timer.zig:221), so the merge is this module's `CancelError`.
pub type TimerCancelError = CancelError;

/// A timer fires a callback after a specified amount of time. A timer can
/// repeat by returning "rearm" in the callback or by rescheduling the
/// start within the callback.
//
// PORT NOTE: `TimerDynamic` is `struct { backend: xev.Union(&.{"Timer"}) }`
// (timer.zig:233-235). Each candidate's `api.Timer` is `TimerLoop(api)`, an EMPTY
// struct (timer.zig:15-17) that only forwards to `loop.timer`/`loop.timer_reset`
// — which is why the reference has to bolt a `_zig_bug_padding: u8` arm onto the
// union to dodge a Zig 0.13 zero-sized-union crash (dynamic.zig:483-491). Rust
// has no such bug, and a union of two ZSTs carries no information, so this is the
// unit struct it always was. `Copy`: the reference `run`/`reset`/`cancel` take
// `self: Self` by value.
//
// PORT NOTE: neither `epoll.rs` nor `io_uring.rs` carries a `Timer` WATCHER
// (their `Timer` type is the loop-internal heap node / kernel timespec), because
// `TimerLoop` is a pure forwarder with no backend-specific state. So this type is
// `TimerLoop(Dynamic)`-shaped — it reaches the backend loop's `timer`/
// `timer_reset` directly, which is precisely what `TimerDynamic` → `TimerLoop` →
// `loop.timer` collapses to.
#[derive(Clone, Copy)]
pub struct Timer;

impl Timer {
    /// Create a new timer.
    //
    // PORT NOTE: `pub fn init() !Self` (timer.zig:239 / timer.zig:20).
    pub fn init() -> core::result::Result<Timer, Error> {
        Ok(Timer)
    }

    /// PORT NOTE: `pub fn deinit(self: *Self) void` (timer.zig:252 / timer.zig:24)
    /// — "Nothing for now."
    pub fn deinit(&self) {
        let _ = self;
    }

    /// Start the timer. The timer will execute in next_ms milliseconds from
    /// now.
    ///
    /// This will use the monotonic clock on your system if available so
    /// this is immune to system clock changes or drift. The callback is
    /// guaranteed to fire NO EARLIER THAN "next_ms" milliseconds. We can't
    /// make any guarantees about exactness or time bounds because its possible
    /// for your OS to just... pause.. the process for an indefinite period of
    /// time.
    ///
    /// Like everything else in libxev, if you want something to repeat, you
    /// must then requeue the completion manually. This punts off one of the
    /// "hard" aspects of timers: it is up to you to determine what the semantic
    /// meaning of intervals are.
    //
    // PORT NOTE: `pub fn run(...)` (timer.zig:261) — `c.ensureTag(tag)` then the
    // backend's `Timer.run` (= `loop.timer(c, next_ms, userdata, cb)`). Unlike
    // the other watchers, the backend entry point here takes a PLAIN fn-pointer
    // (`epoll::Callback` / `io_uring::Callback`), not a generic ZST `CB`, so the
    // per-arm bridge must be a monomorphized `fn` item (`*_run_trampoline`)
    // rather than a closure — exactly Zig's `struct { fn callback }` idiom.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers that outlive the timer.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn run<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        next_ms: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerRunError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Timer::run cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        match backend() {
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                (*Loop::as_io_uring(loop_)).timer(
                    Completion::as_io_uring(c),
                    next_ms,
                    userdata.cast::<c_void>(),
                    Self::io_uring_run_trampoline::<UD, CB>,
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                (*Loop::as_epoll(loop_)).timer(
                    Completion::as_epoll(c),
                    next_ms,
                    userdata.cast::<c_void>(),
                    Self::epoll_run_trampoline::<UD, CB>,
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Reset a timer to execute in next_ms milliseconds. If the timer
    /// is already started, this will stop it and restart it. If the
    /// timer has never been started, this is equivalent to running "run".
    /// In every case, the timer callback is updated to the given userdata
    /// and callback.
    ///
    /// This requires an additional completion c_cancel to represent
    /// the need to possibly cancel the previous timer. You can check
    /// if c_cancel was used by checking the state() after the call.
    ///
    /// VERY IMPORTANT: both c and c_cancel MUST NOT be undefined. They
    /// must be initialized to ".{}" if being used for the first time.
    //
    // PORT NOTE: `pub fn reset(...)` (timer.zig:306) — `c.ensureTag(tag)`,
    // `c_cancel.ensureTag(tag)`, then the backend's `Timer.reset` (=
    // `loop.timer_reset`). Reuses the SAME trampoline as `run` (both map the
    // loop's `Result::Timer` → `TimerRunError`), exactly as `TimerLoop` does.
    ///
    /// # Safety
    /// `c`/`c_cancel`/`loop_` must be valid, stable pointers that outlive the timer.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn reset<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        c_cancel: *mut Completion,
        next_ms: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerRunError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Timer::reset cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        match backend() {
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                Completion::ensure_tag_raw(c_cancel, Backend::IoUring);
                (*Loop::as_io_uring(loop_)).timer_reset(
                    Completion::as_io_uring(c),
                    Completion::as_io_uring(c_cancel),
                    next_ms,
                    userdata.cast::<c_void>(),
                    Self::io_uring_run_trampoline::<UD, CB>,
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                Completion::ensure_tag_raw(c_cancel, Backend::Epoll);
                (*Loop::as_epoll(loop_)).timer_reset(
                    Completion::as_epoll(c),
                    Completion::as_epoll(c_cancel),
                    next_ms,
                    userdata.cast::<c_void>(),
                    Self::epoll_run_trampoline::<UD, CB>,
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// The monomorphized io_uring trampoline for `run`/`reset` — maps
    /// `io_uring::Result::Timer` → `TimerRunError!void` and `@fieldParentPtr`s the
    /// inner pointers back to the outer ones.
    fn io_uring_run_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut io_uring::Loop,
        c_inner: *mut io_uring::Completion,
        r: io_uring::Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerRunError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: `CB` is a ZST (asserted in `run`/`reset`), so reading it through
        // an aligned dangling pointer accesses zero bytes and is sound.
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // if (r.timer) |trigger| switch (trigger) { .request, .expiration => {},
        //     .cancel => error.Canceled } else |err| err
        let mapped: core::result::Result<(), TimerRunError> = match r {
            io_uring::Result::Timer(Ok(trigger)) => match trigger {
                io_uring::TimerTrigger::Request | io_uring::TimerTrigger::Expiration => Ok(()),
                io_uring::TimerTrigger::Cancel => Err(TimerRunError::Canceled),
            },
            io_uring::Result::Timer(Err(io_uring::TimerError::Unexpected)) => {
                Err(TimerRunError::Unexpected)
            }
            _ => unreachable!("timer completion yielded a non-timer result"),
        };
        cb(
            userdata_value::<UD>(ud),
            Loop::from_io_uring(l_inner),
            Completion::from_io_uring(c_inner),
            mapped,
        )
    }

    /// See `io_uring_run_trampoline`.
    fn epoll_run_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut epoll::Loop,
        c_inner: *mut epoll::Completion,
        r: epoll::Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerRunError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: see `io_uring_run_trampoline`.
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let mapped: core::result::Result<(), TimerRunError> = match r {
            epoll::Result::Timer(Ok(trigger)) => match trigger {
                epoll::TimerTrigger::Request | epoll::TimerTrigger::Expiration => Ok(()),
                epoll::TimerTrigger::Cancel => Err(TimerRunError::Canceled),
            },
            epoll::Result::Timer(Err(epoll::TimerError::Unexpected)) => {
                Err(TimerRunError::Unexpected)
            }
            _ => unreachable!("timer completion yielded a non-timer result"),
        };
        cb(
            userdata_value::<UD>(ud),
            Loop::from_epoll(l_inner),
            Completion::from_epoll(c_inner),
            mapped,
        )
    }

    // -----------------------------------------------------------------------
    // PORT NOTE (DEFERRED — `Timer::cancel`, timer.zig:369):
    //
    // `TimerDynamic.cancel` forwards to `api.Timer.cancel` = `TimerLoop.cancel`
    // (timer.zig:137-210), whose body is a `switch (xev.backend)` that builds a
    // `.cancel = .{ .c = c_timer }` completion for kqueue/epoll and a
    // `.timer_remove = .{ .timer = c_timer }` one for io_uring — i.e. it must
    // WRITE a backend `Operation` into a completion. Those `Operation` variants
    // (`epoll::Operation::Cancel`, `io_uring::Operation::TimerRemove`) are
    // module-private to the backends, and neither backend exposes a
    // `Completion::new_op`-style seam to this module (macOS's `timer.rs` can do
    // it only because `kqueue::Completion::new_op` is `pub(crate)`).
    //
    // Ported honestly as a DEFERRAL rather than by widening two backends'
    // internals from a task whose brief scopes it to this file: nothing in the
    // Linux shell path calls `Timer.cancel` (`Timer.reset`, which IS ported above,
    // is what termio's scroll/coalesce/sync timers use), and the underlying loop
    // machinery is already ported + tested inside each backend
    // (`epoll_timer_cancellation`, `io_uring_timer_remove`). To un-defer: make
    // `Completion::new_op` `pub(crate)` in epoll.rs/io_uring.rs (as kqueue.rs
    // already does) and add the two arms here.
    // -----------------------------------------------------------------------
}

// ---------------------------------------------------------------------------
// Process (watcher/process.zig `ProcessDynamic`, process.zig:361-430).
// ---------------------------------------------------------------------------

/// The error that can come in the wait callback.
//
// PORT NOTE: `pub const WaitError = dynamic.ErrorSet(&.{"Process", "WaitError"})`
// (process.zig:368) = io_uring `{InvalidChild, Unexpected}` || epoll
// `{InvalidChild, Unexpected}`. Named `ProcessWaitError` here (one module),
// matching epoll.rs/io_uring.rs's spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessWaitError {
    InvalidChild,
    Unexpected,
}

impl ProcessWaitError {
    fn from_io_uring(e: io_uring::ProcessWaitError) -> ProcessWaitError {
        match e {
            io_uring::ProcessWaitError::InvalidChild => ProcessWaitError::InvalidChild,
            io_uring::ProcessWaitError::Unexpected => ProcessWaitError::Unexpected,
        }
    }
    fn from_epoll(e: epoll::ProcessWaitError) -> ProcessWaitError {
        match e {
            epoll::ProcessWaitError::InvalidChild => ProcessWaitError::InvalidChild,
            epoll::ProcessWaitError::Unexpected => ProcessWaitError::Unexpected,
        }
    }
}

/// Process watcher — waits for a subprocess to exit.
//
// PORT NOTE: `ProcessDynamic(dynamic)` (process.zig:361) is `struct { backend:
// Union }`. Spelled as a Rust `enum` for the same reason as `Async`: both arms
// are `Copy` fd handles passed BY VALUE and never pointer-projected.
#[derive(Clone, Copy)]
pub enum Process {
    IoUring(io_uring::Process),
    Epoll(epoll::Process),
}

impl Process {
    /// Create a new process watcher for the given pid.
    //
    // PORT NOTE: `pub fn init(pid: posix.pid_t) !Self` (process.zig:370).
    pub fn init(pid: libc::pid_t) -> core::result::Result<Process, Error> {
        Ok(match backend() {
            Backend::IoUring => Process::IoUring(io_uring::Process::init(pid)?),
            Backend::Epoll => Process::Epoll(epoll::Process::init(pid)?),
            be => unreachable!("non-candidate backend {be:?}"),
        })
    }

    /// Clean up the process watcher.
    //
    // PORT NOTE: `pub fn deinit(self: *Self) void` (process.zig:383). Switches on
    // the SELF tag — see the note on `Async::deinit`.
    pub fn deinit(&mut self) {
        match self {
            Process::IoUring(p) => p.deinit(),
            Process::Epoll(p) => p.deinit(),
        }
    }

    /// Wait for the process to exit. This will automatically call
    /// `waitpid` or equivalent and report the exit status.
    //
    // PORT NOTE: `pub fn wait(...)` (process.zig:392) — the same
    // `ensureTag` + down-cast + `@fieldParentPtr`-in-the-callback shape as
    // `Async::wait`; see the notes there.
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

        match (self, backend()) {
            (Process::IoUring(p), Backend::IoUring) => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                p.wait(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            r.map_err(ProcessWaitError::from_io_uring),
                        )
                    },
                );
            },
            (Process::Epoll(p), Backend::Epoll) => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                p.wait(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            r.map_err(ProcessWaitError::from_epoll),
                        )
                    },
                );
            },
            // See the same arm on `Async::wait`.
            (p, be) => panic!(
                "Process handle backend {:?} does not match the global backend {be:?} — \
                 the backend must be set before initializing any other types",
                match p {
                    Process::IoUring(_) => Backend::IoUring,
                    Process::Epoll(_) => Backend::Epoll,
                }
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// WriteQueue / WriteRequest (dynamic.zig:68-69 + `DynamicWriteQueue`,
// dynamic.zig:372-392).
// ---------------------------------------------------------------------------

/// WriteQueue is the queue of write requests for ordered writes.
//
// PORT NOTE: `DynamicWriteQueue` (dynamic.zig:372) — `struct { value:
// TaggedUnion(&.{"WriteQueue"}) }` with an `ensureTag`. TAGGED (like
// `Completion`, unlike `Loop`), and its zero value is the LAST candidate's
// variant (dynamic.zig:377-381). `Default` is REQUIRED (termio's Exec
// default-constructs its `write_queue` field) and hand-written, never derived.
#[repr(C)]
pub struct WriteQueue {
    value: WriteQueueUnion,
    tag: Backend,
}

#[repr(C)]
union WriteQueueUnion {
    io_uring: ManuallyDrop<io_uring::WriteQueue>,
    epoll: ManuallyDrop<epoll::WriteQueue>,
}

const _: () = assert!(offset_of!(WriteQueue, value) == 0);

impl Default for WriteQueue {
    /// PORT NOTE: dynamic.zig:377-381 — the LAST candidate's zero variant.
    fn default() -> Self {
        WriteQueue::zeroed_for(BACKEND_DEFAULT)
    }
}

impl WriteQueue {
    fn zeroed_for(tag: Backend) -> WriteQueue {
        match tag {
            Backend::IoUring => WriteQueue {
                value: WriteQueueUnion {
                    io_uring: ManuallyDrop::new(io_uring::WriteQueue::default()),
                },
                tag,
            },
            Backend::Epoll => WriteQueue {
                value: WriteQueueUnion {
                    epoll: ManuallyDrop::new(epoll::WriteQueue::default()),
                },
                tag,
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// The active backend tag.
    pub fn tag(&self) -> Backend {
        self.tag
    }

    /// Returns true if the queue has no pending write requests.
    //
    // PORT NOTE (ADDITION): `queue.Intrusive.empty()` is reachable on the static
    // API as `write_queue.empty()`; the reference's `DynamicWriteQueue` forwards
    // no decls at all (it is only ever handed to `queueWrite`). Forwarded here
    // because it is the only way a consumer (or a test) can observe the ordered-
    // write engine draining, and it is a pure read of the live arm.
    pub fn empty(&self) -> bool {
        match self.tag {
            // SAFETY: `tag` names the live arm (`zeroed_for`/`ensure_tag` are its
            // only writers and always set both together).
            Backend::IoUring => unsafe { (*WriteQueue::as_io_uring_const(self)).empty() },
            Backend::Epoll => unsafe { (*WriteQueue::as_epoll_const(self)).empty() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// PORT NOTE: `pub fn ensureTag(self: *Self, comptime tag: xev.Backend)`
    /// (dynamic.zig:383) — verbatim the `Completion` one.
    pub fn ensure_tag(&mut self, tag: Backend) {
        // SAFETY: `self` is a live, uniquely-borrowed `WriteQueue`.
        unsafe { WriteQueue::ensure_tag_raw(self, tag) }
    }

    /// See `Completion::ensure_tag_raw`.
    ///
    /// # Safety
    /// `q` must be a valid, stable `WriteQueue` pointer with no live requests
    /// under a different tag.
    unsafe fn ensure_tag_raw(q: *mut WriteQueue, tag: Backend) {
        if unsafe { (*q).tag } == tag {
            return;
        }
        unsafe { ptr::write(q, WriteQueue::zeroed_for(tag)) };
    }

    /// `&@field(q.value, @tagName(tag))` (stream.zig:747). Identity.
    ///
    /// # Safety
    /// `q`'s live arm must be the io_uring one.
    unsafe fn as_io_uring(q: *mut WriteQueue) -> *mut io_uring::WriteQueue {
        unsafe { ptr::addr_of_mut!((*q).value) }.cast::<io_uring::WriteQueue>()
    }

    /// See `as_io_uring`.
    ///
    /// # Safety
    /// `q`'s live arm must be the epoll one.
    unsafe fn as_epoll(q: *mut WriteQueue) -> *mut epoll::WriteQueue {
        unsafe { ptr::addr_of_mut!((*q).value) }.cast::<epoll::WriteQueue>()
    }

    /// The shared-reference twin of `as_io_uring`, for the `&self` accessors.
    ///
    /// # Safety
    /// See `as_io_uring`.
    unsafe fn as_io_uring_const(q: *const WriteQueue) -> *const io_uring::WriteQueue {
        unsafe { ptr::addr_of!((*q).value) }.cast::<io_uring::WriteQueue>()
    }

    /// See `as_io_uring_const`.
    ///
    /// # Safety
    /// See `as_epoll`.
    unsafe fn as_epoll_const(q: *const WriteQueue) -> *const epoll::WriteQueue {
        unsafe { ptr::addr_of!((*q).value) }.cast::<epoll::WriteQueue>()
    }
}

/// WriteRequest is a single request for a write. It wraps a
/// completion so that it can be inserted into the WriteQueue.
//
// PORT NOTE: `pub const WriteRequest = Dynamic.Union(&.{"WriteRequest"})`
// (dynamic.zig:69) — an UNTAGGED union, unlike `WriteQueue`/`Completion`. The
// reference relies on `queueWrite` re-initializing it every time
// (stream.zig:737-741: `req.* = @unionInit(xev.WriteRequest, @tagName(tag),
// undefined)`), so it never needs to know its own arm.
//
// PORT NOTE: `Default` is REQUIRED (termio's Exec pools `xev::WriteRequest` in a
// `SegmentedPool`, which default-constructs its elements) and hand-written, never
// derived. The reference's pool elements are `undefined`; the zero value here is
// the LAST candidate's variant, consistent with `Completion`/`WriteQueue`
// (dynamic.zig:251/377) — and `queue_write` overwrites it wholesale regardless.
#[repr(C)]
pub union WriteRequest {
    io_uring: ManuallyDrop<io_uring::WriteRequest>,
    epoll: ManuallyDrop<epoll::WriteRequest>,
}

impl Default for WriteRequest {
    fn default() -> Self {
        WriteRequest {
            epoll: ManuallyDrop::new(epoll::WriteRequest::default()),
        }
    }
}

// PORT NOTE: both backends' `WriteRequest`s carry a documented `unsafe impl
// Send` (raw-ptr `next` link + an embedded fn-ptr `Completion`; they ride the
// `Loop` moved onto the termio thread, and after the move exactly one thread
// touches them). Rust derives a union's auto traits from its arms, so this union
// is `Send` for the same reason with no explicit `unsafe impl` needed.

impl WriteRequest {
    /// `&@field(req, @tagName(tag))` (stream.zig:748). Identity (union arms sit at
    /// offset 0 of a `#[repr(C)]` union).
    ///
    /// # Safety
    /// `r` must be a valid `WriteRequest` pointer; the caller must be about to
    /// (re-)initialize it for the io_uring arm, as `queue_write` does.
    unsafe fn as_io_uring(r: *mut WriteRequest) -> *mut io_uring::WriteRequest {
        r.cast::<io_uring::WriteRequest>()
    }

    /// See `as_io_uring`.
    ///
    /// # Safety
    /// See `as_io_uring`.
    unsafe fn as_epoll(r: *mut WriteRequest) -> *mut epoll::WriteRequest {
        r.cast::<epoll::WriteRequest>()
    }
}

// ---------------------------------------------------------------------------
// Stream (watcher/stream.zig `GenericStream` dynamic arm, stream.zig:1044-1080 +
// `Closeable`/`Pollable`/`Readable`/`Writeable` dynamic arms).
// ---------------------------------------------------------------------------

/// Creates a generic stream type that supports read, write, close, poll. This
/// can be used for any file descriptor that would exhibit normal blocking
/// behavior on read/write. This should NOT be used for local files because
/// local files have some special properties; you should use xev.File for that.
//
// PORT NOTE: `GenericStream(xev)`'s dynamic arm (stream.zig:1044) — `struct {
// backend: xev.Union(&.{"Stream"}) }` with `options = { .close = true, .poll =
// true, .read = .read, .write = .write, .type = "Stream" }`. UNTAGGED union in
// the reference; both arms here are `Copy` fd handles, so the Rust `union` needs
// no `ManuallyDrop`, and `Stream` stays `Copy` (the reference's
// `read`/`write`/`close`/`poll`/`queueWrite` all take `self: Self` by value).
//
// PORT NOTE: switching on the SELF-carried arm is impossible for an untagged
// union, so — exactly like the reference — every method switches on the GLOBAL
// (`switch (xev.backend)`).
#[derive(Clone, Copy)]
pub struct Stream {
    backend: StreamUnion,
}

#[repr(C)]
#[derive(Clone, Copy)]
union StreamUnion {
    io_uring: io_uring::Stream,
    epoll: epoll::Stream,
}

impl Stream {
    /// Initialize a generic stream from a file descriptor.
    //
    // PORT NOTE: `pub fn initFd(fd: std.posix.pid_t) Self` (stream.zig:1065) —
    // note the reference's `pid_t` parameter type is an upstream typo for `fd_t`
    // (both are `c_int`); ported as the `fd_t` it actually is.
    pub fn init_fd(fd: libc::c_int) -> Stream {
        match backend() {
            Backend::IoUring => Stream {
                backend: StreamUnion {
                    io_uring: io_uring::Stream::init_fd(fd),
                },
            },
            Backend::Epoll => Stream {
                backend: StreamUnion {
                    epoll: epoll::Stream::init_fd(fd),
                },
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Clean up any watcher resources. This does NOT close the file.
    pub fn deinit(&self) {
        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe { self.backend.io_uring.deinit() },
            Backend::Epoll => unsafe { self.backend.epoll.deinit() },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// The `Self.initFd(s_inner.fd)` re-wrap every dynamic stream trampoline does
    /// (stream.zig:665).
    //
    // PORT NOTE: the reference reaches through the backend stream's `fd` field and
    // rebuilds; the backends' `fd` is module-private here, so the equivalent (and
    // cheaper) re-wrap of the very same handle is used instead — `initFd(s.fd)`
    // and "wrap `s`" are the same value, since `Stream` IS its `fd`.
    fn from_io_uring(s: io_uring::Stream) -> Stream {
        Stream {
            backend: StreamUnion { io_uring: s },
        }
    }

    /// See `from_io_uring`.
    fn from_epoll(s: epoll::Stream) -> Stream {
        Stream {
            backend: StreamUnion { epoll: s },
        }
    }

    // -------------------------------------------------------------------
    // Readable (stream.zig:444 dynamic arm). `options.read == .read`.
    // -------------------------------------------------------------------

    /// Read from the stream. This performs a single read.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice)
    /// outlives the read.
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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                self.backend.io_uring.read(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            ReadBuffer::from_io_uring(b_inner),
                            r.map_err(ReadError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                self.backend.epoll.read(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            ReadBuffer::from_epoll(b_inner),
                            r.map_err(ReadError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    // -------------------------------------------------------------------
    // Writeable (stream.zig:621 dynamic arm). `options.write == .write`.
    // -------------------------------------------------------------------

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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                self.backend.io_uring.write(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                self.backend.epoll.write(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Write to the stream. This queues the writes to ensure they remain in
    /// order.
    //
    // PORT NOTE: `pub fn queueWrite(...)` (stream.zig:686). Note the reference
    // does NOT `ensureTag` the completion here — the completion lives INSIDE the
    // `WriteRequest`, which `queueWrite` re-initializes wholesale
    // (stream.zig:737-741) — but it DOES `q.ensureTag(tag)` first
    // ("Ensure our WriteQueue has the correct tag, since it is regularly
    // zero-initialized and our zero-init picks an arbitrary backend",
    // stream.zig:733-735). Both are reproduced exactly.
    //
    // PORT NOTE (REFERENCE QUIRK, preserved — the `c` handed to `cb`): the
    // reference's `api_cb` (stream.zig:660-663) `@fieldParentPtr`s `c_inner` back
    // to a `*xev.Completion` — but `c_inner` here is `&req.<tag>.completion`, a
    // completion nested in the BACKEND `WriteRequest`, NOT one nested in a Dynamic
    // `Completion`. So the recovered pointer does not actually point at a valid
    // Dynamic `Completion` (its `tag` field would be whatever the `WriteRequest`
    // has at that offset). Upstream gets away with it because the pointer is only
    // ever passed through, never dereferenced — ghostty's `ttyWrite` callback
    // spells the parameter `_: *xev.Completion` (Exec.zig), as does this port's
    // `rustty-termio` `ttyWrite`. Reproduced faithfully (the `@fieldParentPtr`
    // here is an identity cast, so no out-of-bounds pointer is even computed), and
    // flagged loudly: DO NOT dereference the `c` passed to a `queue_write`
    // callback.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks. The `c` passed to `cb` MUST NOT be
    /// dereferenced (see the PORT NOTE above).
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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                // Ensure our WriteQueue has the correct tag, since it is
                // regularly zero-initialized and our zero-init picks
                // an arbitrary backend.
                WriteQueue::ensure_tag_raw(q, Backend::IoUring);
                // Initialize our request since it is usually undefined.
                // (`io_uring::Stream::queue_write` fully overwrites `*req`, which
                // is what the reference's `@unionInit(..., undefined)` amounts to
                // for an untagged union.)
                self.backend.io_uring.queue_write(
                    Loop::as_io_uring(loop_),
                    WriteQueue::as_io_uring(q),
                    WriteRequest::as_io_uring(req),
                    buf.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            WriteBuffer::from_io_uring(b_inner),
                            r.map_err(WriteError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                WriteQueue::ensure_tag_raw(q, Backend::Epoll);
                self.backend.epoll.queue_write(
                    Loop::as_epoll(loop_),
                    WriteQueue::as_epoll(q),
                    WriteRequest::as_epoll(req),
                    buf.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, b_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            WriteBuffer::from_epoll(b_inner),
                            r.map_err(WriteError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    // -------------------------------------------------------------------
    // Closeable (stream.zig:320 dynamic arm). `options.close == true`.
    // -------------------------------------------------------------------

    /// Close the file.
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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                self.backend.io_uring.close(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            r.map_err(CloseError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                self.backend.epoll.close(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            r.map_err(CloseError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    // -------------------------------------------------------------------
    // Pollable (stream.zig:140 dynamic arm). `options.poll == true`; both
    // candidates expose `poll`, so the reference's "if any candidate lacks
    // poll, drop the decl" comptime guard (stream.zig:141-150) resolves to
    // "keep it".
    // -------------------------------------------------------------------

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

        match backend() {
            // SAFETY: the global names the live arm (dynamic.zig:32-33).
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                self.backend.io_uring.poll(
                    Loop::as_io_uring(loop_),
                    Completion::as_io_uring(c),
                    event.to_io_uring(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r| {
                        cb(
                            ud,
                            Loop::from_io_uring(l_inner),
                            Completion::from_io_uring(c_inner),
                            Stream::from_io_uring(s_inner),
                            r.map(PollEvent::from_io_uring)
                                .map_err(PollError::from_io_uring),
                        )
                    },
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                self.backend.epoll.poll(
                    Loop::as_epoll(loop_),
                    Completion::as_epoll(c),
                    event.to_epoll(),
                    userdata,
                    move |ud, l_inner, c_inner, s_inner, r| {
                        cb(
                            ud,
                            Loop::from_epoll(l_inner),
                            Completion::from_epoll(c_inner),
                            Stream::from_epoll(s_inner),
                            r.map(PollEvent::from_epoll).map_err(PollError::from_epoll),
                        )
                    },
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }
}

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

    // `Default`: REQUIRED by termio (Exec default-constructs `write_queue`; its
    // `SegmentedPool` default-constructs pooled `WriteRequest`s). Hand-written
    // here, never derived — a derive would pick the wrong union arm.
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
mod tests {
    use super::*;
    use crate::backend::Backend;
    use core::ptr;
    use std::sync::Mutex;

    // PORT NOTE: Zig runs a file's `test{}` blocks SERIALLY; Rust runs the whole
    // crate's tests in parallel threads of ONE process. Every test here mutates
    // the process-global `BACKEND` (`detect`/`prefer`), so they must not overlap.
    // This lock restores the reference's serial execution for exactly the tests
    // that touch the global.
    static BACKEND_LOCK: Mutex<()> = Mutex::new(());

    /// Take the serialization lock, tolerating poisoning. A panicking test would
    /// otherwise poison the mutex and make EVERY other test here fail on
    /// `unwrap()`, hiding which one actually broke. The guarded state is `()` —
    /// there is nothing to be corrupted by a panic — so recovering the guard is
    /// sound, and the real failure stays legible.
    fn lock_backend() -> std::sync::MutexGuard<'static, ()> {
        BACKEND_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    // test { @import("std").testing.refAllDecls(@This()); }  (dynamic.zig:198)
    #[test]
    fn dynamic_ref_all_decls() {
        // The shared structures (dynamic.zig:60-69).
        let _ = core::mem::size_of::<Options>();
        let _ = core::mem::size_of::<RunMode>();
        let _ = core::mem::size_of::<CallbackAction>();
        let _ = core::mem::size_of::<CompletionState>();
        let _ = core::mem::size_of::<Completion>();
        let _ = core::mem::size_of::<PollEvent>();
        let _ = core::mem::size_of::<ReadBuffer>();
        let _ = core::mem::size_of::<WriteBuffer>();
        let _ = core::mem::size_of::<WriteQueue>();
        let _ = core::mem::size_of::<WriteRequest>();

        // Error types (dynamic.zig:72-79).
        let _ = core::mem::size_of::<AcceptError>();
        let _ = core::mem::size_of::<CancelError>();
        let _ = core::mem::size_of::<CloseError>();
        let _ = core::mem::size_of::<ConnectError>();
        let _ = core::mem::size_of::<PollError>();
        let _ = core::mem::size_of::<ShutdownError>();
        let _ = core::mem::size_of::<WriteError>();
        let _ = core::mem::size_of::<ReadError>();
        let _ = core::mem::size_of::<Error>();
        let _ = core::mem::size_of::<NotifyError>();

        // Core types (dynamic.zig:82-88) + the loop (dynamic.zig:122).
        let _ = core::mem::size_of::<Loop>();
        let _ = core::mem::size_of::<Async>();
        let _ = core::mem::size_of::<Process>();
        let _ = core::mem::size_of::<Stream>();
        let _ = core::mem::size_of::<Timer>();

        // The dynamic-API marker + candidate list (dynamic.zig:46-48).
        // `DYNAMIC` is a `const`, so the check is a compile-time one (this is
        // the `hasDecl`-style marker consumers branch on, dynamic.zig:44-46).
        const { assert!(DYNAMIC) };
        assert_eq!(CANDIDATES, [Backend::IoUring, Backend::Epoll]);
        // `pub const candidates = bes;` — this instantiation IS `Xev(&.{
        // .io_uring, .epoll })`, i.e. exactly `backend.zig`'s `.linux` arm. If
        // `Backend::candidates()` ever changes, this file must be re-authored.
        assert_eq!(CANDIDATES.as_slice(), Backend::candidates());
    }

    // test "completion is zero-able" (dynamic.zig:202)
    #[test]
    fn dynamic_completion_zero_able() {
        let c: Completion = Completion::default();

        // Strengthened beyond the reference's `_ = c;` so this is not vacuous:
        // dynamic.zig:251-255 zero-inits the LAST candidate's variant, and
        // dynamic.zig:244-248 says the tag exists precisely so a zero-initialized
        // completion has a valid, readable backend tag.
        assert_eq!(c.tag(), *CANDIDATES.last().unwrap());
        assert_eq!(c.tag(), Backend::Epoll);
        // And the zero-init really is the backend's own `.{}` (a dead completion).
        assert_eq!(c.state(), CompletionState::Dead);
    }

    // The `ensureTag` contract (dynamic.zig:283-290): a mismatched tag is replaced
    // with a ZERO-INITIALIZED value of the requested backend; a matching tag is a
    // no-op.
    #[test]
    fn dynamic_completion_ensure_tag() {
        let mut c = Completion::default();
        assert_eq!(c.tag(), Backend::Epoll);

        // Mismatch → re-tagged + zero-initialized.
        c.ensure_tag(Backend::IoUring);
        assert_eq!(c.tag(), Backend::IoUring);
        assert_eq!(c.state(), CompletionState::Dead);

        // Match → no-op (idempotent).
        c.ensure_tag(Backend::IoUring);
        assert_eq!(c.tag(), Backend::IoUring);

        // And back again.
        c.ensure_tag(Backend::Epoll);
        assert_eq!(c.tag(), Backend::Epoll);
        assert_eq!(c.state(), CompletionState::Dead);
    }

    // The global defaults to the LAST candidate (dynamic.zig:91) — NOT the first,
    // despite the doc comment at dynamic.zig:28-30 claiming otherwise. The other
    // tests mutate the global, so assert on the static's declared initializer.
    #[test]
    fn dynamic_backend_default_is_last_candidate() {
        assert_eq!(BACKEND_DEFAULT, *CANDIDATES.last().unwrap());
        assert_eq!(BACKEND_DEFAULT, Backend::Epoll);
    }

    // test "detect" (dynamic.zig:207)
    #[test]
    fn dynamic_detect() {
        let _guard = lock_backend();

        detect().unwrap();

        // inline for (bes) |be| { if (@intFromEnum(be) == @intFromEnum(backend)) {
        //     try testing.expect(be.Api().available()); break; }
        // } else try testing.expect(false);
        let be = backend();
        assert!(
            CANDIDATES.contains(&be),
            "detect() must land on a candidate backend, got {be:?}"
        );
        assert!(available(be));

        // Strengthened: detect picks the FIRST AVAILABLE candidate (dynamic.zig:96-105).
        let first_available = CANDIDATES.iter().copied().find(|b| available(*b)).unwrap();
        assert_eq!(be, first_available);
    }

    // test "prefer" (dynamic.zig:218)
    #[test]
    fn dynamic_prefer() {
        let _guard = lock_backend();

        // PORT NOTE (reference quirk, preserved): the reference asserts
        // `prefer(bes[0])` succeeds — i.e. it ASSUMES the first candidate
        // (io_uring) is available on the test host. On a kernel without io_uring
        // this test fails upstream too. This host has io_uring enabled.
        assert!(prefer(CANDIDATES[0]));

        let be = backend();
        assert!(CANDIDATES.contains(&be));
        assert!(available(be));

        // Strengthened: a successful `prefer` really SET the global (dynamic.zig:114).
        assert_eq!(be, CANDIDATES[0]);
        assert_eq!(be, Backend::IoUring);

        // A non-candidate is never preferred and never changes the global
        // (dynamic.zig:110-119: the `inline for` finds no match → `return false`).
        assert!(!prefer(Backend::Kqueue));
        assert_eq!(backend(), CANDIDATES[0]);

        // The other candidate is preferable too (epoll is always available on Linux).
        assert!(prefer(Backend::Epoll));
        assert_eq!(backend(), Backend::Epoll);
    }

    // test "loop basics" (dynamic.zig:229)
    #[test]
    fn dynamic_loop_basics() {
        let _guard = lock_backend();

        detect().unwrap();
        let mut l = Loop::init(Options::default()).unwrap();
        l.run(RunMode::UntilDone).unwrap();
        l.stop();
        assert!(l.stopped());
        l.deinit();
    }

    // -----------------------------------------------------------------------
    // Live KATs — each runs under BOTH candidate backends, so the union really
    // dispatches (a single-arm implementation would fail one half).
    // -----------------------------------------------------------------------

    // KAT — `Loop::init` really constructs the backend named by the GLOBAL
    // (dynamic.zig:128's `switch (backend)`), not a hard-wired arm.
    //
    // The `Loop` union is untagged (dynamic.zig:454 — the tag lives in the
    // global), so there is nothing on the handle to assert against; a
    // `backend_tag()` accessor would just re-read the global and prove nothing.
    // `Options::entries` discriminates the two arms for real instead: io_uring
    // rejects > 8191 entries (io_uring.zig's `u13` cast → `TooManyEntries`),
    // while epoll ignores `entries` entirely (it is documented io_uring-only,
    // loop.zig:15). So the SAME options must fail under one backend and succeed
    // under the other — which can only happen if `init` read the global.
    #[test]
    fn dynamic_loop_init_dispatches_on_the_global() {
        let _guard = lock_backend();

        let overflow = || Options {
            entries: 8192,
            ..Options::default()
        };

        assert!(prefer(Backend::IoUring));
        assert!(
            Loop::init(overflow()).is_err(),
            "io_uring must reject entries > 8191"
        );

        assert!(prefer(Backend::Epoll));
        let mut l = Loop::init(overflow()).expect("epoll ignores `entries`");
        l.deinit();
    }

    // KAT — `Loop::now`/`update_now` reach a live loop of each backend.
    #[test]
    fn dynamic_loop_now_advances() {
        let _guard = lock_backend();

        for be in CANDIDATES {
            assert!(
                prefer(be),
                "candidate {be:?} must be available on this host"
            );
            let mut l = Loop::init(Options::default()).unwrap();
            let t0 = l.now();
            assert!(t0 > 0, "backend {be:?}: monotonic clock must be readable");
            std::thread::sleep(std::time::Duration::from_millis(5));
            l.update_now();
            assert!(l.now() > t0, "backend {be:?}: update_now must advance");
            l.deinit();
        }
    }

    // KAT — Async (eventfd) wakeup through the Dynamic union: wait, notify from
    // another thread while the loop is BLOCKED, confirm the callback fired with
    // the OUTER (`dynamic::Loop`/`dynamic::Completion`) pointers.
    #[test]
    fn dynamic_async_notify() {
        let _guard = lock_backend();

        struct Ud {
            wake: bool,
            l: *mut Loop,
            c: *mut Completion,
        }

        for be in CANDIDATES {
            assert!(prefer(be));

            let mut loop_ = Loop::init(Options::default()).unwrap();
            let notifier = Async::init().unwrap();

            let mut c_wait = Completion::default();
            let mut ud = Ud {
                wake: false,
                l: &mut loop_ as *mut Loop,
                c: ptr::addr_of_mut!(c_wait),
            };
            let cb = |ud: *mut Ud,
                      l: *mut Loop,
                      c: *mut Completion,
                      r: core::result::Result<(), WaitError>| {
                r.unwrap();
                unsafe {
                    // The callback receives the OUTER pointers; assert they are the
                    // very ones we passed in (what the `@fieldParentPtr` up-cast owes us).
                    assert_eq!(l, (*ud).l, "callback must get the OUTER Loop pointer");
                    assert_eq!(c, (*ud).c, "callback must get the OUTER Completion pointer");
                    (*ud).wake = true;
                }
                CallbackAction::Disarm
            };
            unsafe {
                notifier.wait(
                    &mut loop_ as *mut Loop,
                    ptr::addr_of_mut!(c_wait),
                    ptr::addr_of_mut!(ud),
                    cb,
                );
            }

            // `wait` re-tagged the zero-initialized (epoll-tagged) completion.
            assert_eq!(c_wait.tag(), be);

            loop_.run(RunMode::NoWait).unwrap();
            assert!(!ud.wake);

            let n = notifier; // `Async` is Copy (an fd handle) and Send (a plain i32).
            let t = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(20));
                n.notify().unwrap();
            });

            // Blocks until the spawned thread notifies. A hang is a real bug.
            loop_.run(RunMode::UntilDone).unwrap();
            assert!(ud.wake, "backend {be:?} did not wake");
            t.join().unwrap();

            notifier.deinit();
            loop_.deinit();
        }
    }

    // KAT — Timer through the Dynamic union, driven from a ZERO-INITIALIZED
    // completion. This is the exact scenario dynamic.zig:244-248 says the
    // completion tag exists for: `Completion::default()` is epoll-tagged, so under
    // io_uring `Timer::run` must `ensure_tag` it before use.
    #[test]
    fn dynamic_timer() {
        let _guard = lock_backend();

        for be in CANDIDATES {
            assert!(prefer(be));

            let mut loop_ = Loop::init(Options::default()).unwrap();
            let timer = Timer::init().unwrap();

            let mut called = false;
            let mut c = Completion::default();
            assert_eq!(c.tag(), Backend::Epoll, "zero-init is always epoll-tagged");

            let cb = |ud: *mut bool,
                      _l: *mut Loop,
                      _c: *mut Completion,
                      r: core::result::Result<(), TimerRunError>| {
                r.unwrap();
                unsafe { *ud = true };
                CallbackAction::Disarm
            };
            unsafe {
                timer.run(
                    &mut loop_ as *mut Loop,
                    ptr::addr_of_mut!(c),
                    1,
                    ptr::addr_of_mut!(called),
                    cb,
                );
            }

            // The zero-initialized completion was re-tagged to the live backend.
            assert_eq!(c.tag(), be);
            assert_eq!(c.state(), CompletionState::Active);

            loop_.run(RunMode::UntilDone).unwrap();
            assert!(called, "backend {be:?} timer did not fire");
            assert_eq!(c.state(), CompletionState::Dead);

            timer.deinit();
            loop_.deinit();
        }
    }

    // KAT — Timer::reset through the Dynamic union (exercises `Loop::timer_reset`
    // dispatch + the second `ensure_tag` on `c_cancel`).
    #[test]
    fn dynamic_timer_reset() {
        let _guard = lock_backend();

        for be in CANDIDATES {
            assert!(prefer(be));

            let mut loop_ = Loop::init(Options::default()).unwrap();
            let timer = Timer::init().unwrap();

            let mut called = false;
            let mut c = Completion::default();
            let mut c_cancel = Completion::default();

            let cb = |ud: *mut bool,
                      _l: *mut Loop,
                      _c: *mut Completion,
                      r: core::result::Result<(), TimerRunError>| {
                r.unwrap();
                unsafe { *ud = true };
                CallbackAction::Disarm
            };

            // Never started → equivalent to `run`; c_cancel is NOT used.
            unsafe {
                timer.reset(
                    &mut loop_ as *mut Loop,
                    ptr::addr_of_mut!(c),
                    ptr::addr_of_mut!(c_cancel),
                    1,
                    ptr::addr_of_mut!(called),
                    cb,
                );
            }
            assert_eq!(c.tag(), be);
            assert_eq!(c_cancel.tag(), be);
            assert_eq!(c_cancel.state(), CompletionState::Dead);

            loop_.run(RunMode::UntilDone).unwrap();
            assert!(called, "backend {be:?} timer reset did not fire");

            timer.deinit();
            loop_.deinit();
        }
    }

    // KAT — Process spawn+exit through the Dynamic union.
    #[test]
    #[allow(clippy::zombie_processes)]
    fn dynamic_process_spawn_exit() {
        let _guard = lock_backend();

        for be in CANDIDATES {
            assert!(prefer(be));

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
            assert_eq!(code, Some(0), "backend {be:?}: exit status 0");

            p.deinit();
            loop_.deinit();
        }
    }

    // KAT — Pty write round-trip through the Dynamic union: `Stream::read` on the
    // child end + `Stream::queue_write` on the parent end. Exercises the dynamic
    // `WriteQueue`/`WriteRequest` tagging + the `ReadBuffer`/`WriteBuffer`
    // to/from-backend conversions.
    #[test]
    fn dynamic_pty_write_roundtrip() {
        let _guard = lock_backend();

        for be in CANDIDATES {
            assert!(prefer(be));

            let mut pty = Pty::init().unwrap();
            let mut loop_ = Loop::init(Options::default()).unwrap();

            let parent = Stream::init_fd(pty.parent); // write side
            let child = Stream::init_fd(pty.child); // read side

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
                    ReadBuffer::Slice(&mut read_buf[..] as *mut [u8]),
                    ptr::addr_of_mut!(read_len),
                    read_cb,
                );
            }
            assert_eq!(c_read.tag(), be);
            loop_.run(RunMode::NoWait).unwrap();
            assert!(read_len.is_none());

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
                    WriteBuffer::Slice(payload.as_slice() as *const [u8]),
                    ptr::null_mut(),
                    write_cb,
                );
            }

            // `queue_write` re-tagged the zero-initialized WriteQueue.
            assert_eq!(write_queue.tag(), be);
            assert!(!write_queue.empty());

            loop_.run(RunMode::UntilDone).unwrap();
            assert!(read_len.is_some(), "backend {be:?} read did not complete");
            assert_eq!(payload, &read_buf[0..read_len.unwrap()]);
            assert!(write_queue.empty());

            pty.deinit();
            loop_.deinit();
        }
    }

    // Helper to open a pty (test-only), mirroring watcher/stream.zig's `Pty`.
    struct Pty {
        parent: libc::c_int,
        child: libc::c_int,
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
            parent: *mut libc::c_int,
            child: *mut libc::c_int,
            name: *mut u8,
            termios: *const core::ffi::c_void,
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
            let mut parent_fd: libc::c_int = 0;
            let mut child_fd: libc::c_int = 0;
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
