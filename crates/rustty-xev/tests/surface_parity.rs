//! surface_parity.rs — the PUBLIC package-surface contract of `rustty-xev`
//! (XEV_BLUEPRINT §1 Linux port, task L1 LT8).
//!
//! This is an INTEGRATION test: it sees only what `lib.rs` actually `pub use`s,
//! which is exactly what a downstream consumer (`rustty-termio`'s `xev_seam`,
//! `pub use rustty_xev::*`) sees. That makes it the right place to pin the
//! per-target re-export surface — a `#[cfg(test)]` unit test inside the crate
//! could name a private module and pass while the public seam is broken.
//!
//! PORT NOTE (what this file is the analogue of): `main.zig`'s `comptime {}`
//! block (main.zig:76-92) — "This ensures that all the public decls from the API
//! are forwarded from the main struct." — which walks
//! `Backend.default().Api()`'s decls and `@compileError("missing decl: " ++
//! decl.name)`s on any the package root fails to forward. Rust has no decl
//! reflection, so the forwarded names are spelled out by hand here; a dropped
//! `pub use` becomes a compile error in THIS file rather than in a consumer.
//!
//! PORT NOTE (the two targets): ghostty imports the xev package as
//! `@import("xev").Dynamic` (global.zig), i.e. `DynamicXev(Backend.candidates())`.
//!  * macOS: `candidates() == &.{.kqueue}`, and `Xev(bes)` short-circuits at
//!    `if (bes.len == 1) return bes[0].Api();` (dynamic.zig:38) — so the surface
//!    IS the flat kqueue `Api()` (XEV_BLUEPRINT §0).
//!  * Linux: `candidates() == &.{.io_uring, .epoll}` — two candidates, so the
//!    real `Dynamic` struct is generated and IS the surface.
//!
//! Both must therefore answer to the SAME names with the SAME bounds, which is
//! what this file asserts.

// ---------------------------------------------------------------------------
// The forwarded-name + trait-bound contract.
//
// Written as `const _: fn() = || { ... }` (the tripwire idiom `dynamic.rs`
// already uses) rather than a `#[test]`: every assertion here is discharged by
// the COMPILER, so it must fire at build time, not at run time. A `#[test]`
// would also work but would imply these are runtime checks; they are not.
// ---------------------------------------------------------------------------

/// The names + bounds forwarded on BOTH live targets (macOS/kqueue and
/// Linux/Dynamic). Naming a type is what forces its `pub use` to exist.
#[cfg(any(target_os = "linux", target_os = "macos"))]
const _: fn() = || {
    fn needs_copy<T: Copy>() {}
    fn needs_default<T: Default>() {}
    fn needs_send<T: Send>() {}
    fn needs_sized<T: Sized>() {}
    fn needs_error_bounds<T: core::fmt::Debug + Clone + Copy + PartialEq + Eq>() {}

    // --- The core loop types (main.zig:13-15,17-21). ---
    needs_sized::<rustty_xev::Loop>();
    needs_sized::<rustty_xev::Completion>();
    needs_sized::<rustty_xev::Options>();
    needs_sized::<rustty_xev::ReadBuffer>();
    needs_sized::<rustty_xev::WriteBuffer>();
    needs_sized::<rustty_xev::RunMode>();
    needs_sized::<rustty_xev::CallbackAction>();
    needs_sized::<rustty_xev::CompletionState>();
    needs_sized::<rustty_xev::Backend>();

    // --- The watchers (main.zig:34-39). `File`/`TCP`/`UDP` are deferred on
    // BOTH targets (DEFERRALS.md), so they are deliberately absent here. ---
    needs_sized::<rustty_xev::Async>();
    needs_sized::<rustty_xev::Process>();
    needs_sized::<rustty_xev::Stream>();
    needs_sized::<rustty_xev::Timer>();
    needs_sized::<rustty_xev::WriteQueue>();
    needs_sized::<rustty_xev::WriteRequest>();

    // --- The error sets (main.zig:23-30 + the crate's unified D6 seam
    // `Error`/`NotifyError`). ---
    needs_sized::<rustty_xev::AcceptError>();
    needs_sized::<rustty_xev::CancelError>();
    needs_sized::<rustty_xev::CloseError>();
    needs_sized::<rustty_xev::ConnectError>();
    needs_sized::<rustty_xev::PollError>();
    needs_sized::<rustty_xev::PollEvent>();
    needs_sized::<rustty_xev::ShutdownError>();
    needs_sized::<rustty_xev::WriteError>();
    needs_sized::<rustty_xev::ReadError>();
    needs_sized::<rustty_xev::Error>();
    needs_sized::<rustty_xev::NotifyError>();
    // The two callback-error aliases consumers must be able to SPELL: a named
    // `fn` item callback cannot infer its `r: Result<_, E>` parameter type.
    needs_sized::<rustty_xev::TimerRunError>();
    needs_sized::<rustty_xev::WaitError>();

    // --- The bounds downstream actually depends on ([ADV-FIX F3]). ---
    // `Async: Copy` — ghostty/termio stores an `xev.Async` by value and copies it
    // onto other threads to `notify()`; `Timer`/`Stream`/`Process` are `Copy` for
    // the same by-value-handle reason (their reference methods take `self: Self`).
    needs_copy::<rustty_xev::Async>();
    needs_copy::<rustty_xev::Timer>();
    needs_copy::<rustty_xev::Stream>();
    needs_copy::<rustty_xev::Process>();
    // `Default` — termio's Exec default-constructs `write_queue: xev.WriteQueue =
    // .{}` and pools `xev.WriteRequest` in a `SegmentedPool` (which
    // default-constructs its elements); `Completion` is api.zig's "completion is
    // zero-able" contract.
    needs_default::<rustty_xev::Completion>();
    needs_default::<rustty_xev::WriteQueue>();
    needs_default::<rustty_xev::WriteRequest>();
    needs_default::<rustty_xev::Options>();
    // `Send` — ghostty MOVES a `Loop` onto the spawned termio thread, and the
    // cross-thread wakeup copies an `Async` onto the notifying thread.
    needs_send::<rustty_xev::Loop>();
    needs_send::<rustty_xev::Completion>();
    needs_send::<rustty_xev::Async>();
    // `NotifyError: Debug + Clone + Copy + PartialEq + Eq` — rustty-core's
    // `surface::Error` union embeds it and itself derives `Copy`/`PartialEq`.
    needs_error_bounds::<rustty_xev::NotifyError>();
};

// ---------------------------------------------------------------------------
// `Loop::init`'s signature — the ONE forwarded decl whose shape differs per
// target, so it is asserted per target rather than once.
//
// REFERENCE: both `kqueue.zig`'s `Loop.init(options: Options)` and
// `dynamic.zig:127`'s `Loop.init(opts: Options)` take `Options`.
//
// RUSTTY DIVERGENCE (macOS side, PRE-EXISTING — kqueue.rs:345-351 + DEFERRALS.md):
// this port specialized the macOS `Loop::init` to NO-ARG, because ghostty's macOS
// build only ever calls `Loop.init(.{})` and the `Options.thread_pool` path is
// deferred with the `ThreadPool` body. The Linux `Dynamic` side is NOT specialized
// — it takes `Options` exactly as dynamic.zig:127 does, and `entries` is
// load-bearing there (it is io_uring-only, loop.zig:15, and is what
// `dynamic.rs`'s `dynamic_loop_init_dispatches_on_the_global` KAT discriminates
// the two arms with). Un-specializing macOS would edit macOS-gated code (R5), so
// the divergence is pinned HERE instead of papered over: each target asserts the
// signature it actually has, and neither can drift silently.
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
const _: fn(rustty_xev::Options) -> Result<rustty_xev::Loop, rustty_xev::Error> =
    rustty_xev::Loop::init;

#[cfg(target_os = "macos")]
const _: fn() -> Result<rustty_xev::Loop, rustty_xev::Error> = rustty_xev::Loop::init;

// ---------------------------------------------------------------------------
// Linux-only: the `Dynamic`-specific decls `main.zig` forwards only when the
// candidate list has more than one entry (dynamic.zig:44-48, 91-119). On macOS
// these are comptime-elided (§0) and are correctly absent.
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
const _: fn() = || {
    fn needs_sized<T: Sized>() {}

    // `pub const dynamic = true;` (dynamic.zig:46) — the `hasDecl` marker.
    const { assert!(rustty_xev::DYNAMIC) };
    // `pub const candidates = bes;` (dynamic.zig:48).
    const { assert!(rustty_xev::CANDIDATES.len() == 2) };
    // `detect()` (dynamic.zig:96) / `prefer()` (dynamic.zig:109) / the `backend`
    // global's reader (dynamic.zig:91). A consumer MUST be able to call
    // `detect()` at startup ("The API user should call `detect()` to set the
    // backend to the first available backend", dynamic.zig:18-19).
    let _: fn() -> Result<(), rustty_xev::Error> = rustty_xev::detect;
    let _: fn(rustty_xev::Backend) -> bool = rustty_xev::prefer;
    let _: fn() -> rustty_xev::Backend = rustty_xev::backend;

    // The two merged `ErrorSet`s whose Linux spelling differs from the macOS
    // backend-specific one (`ProcError`/`TimerError` are kqueue-only).
    needs_sized::<rustty_xev::ProcessWaitError>();
    needs_sized::<rustty_xev::TimerCancelError>();
};

// ---------------------------------------------------------------------------
// Runtime proof that the re-export reaches a LIVE backend.
//
// The compile-time block above pins the NAMES; this pins that they are wired to
// real code. It is not redundant: the pre-LT8 non-macOS seam exported all of
// `Loop`/`Async`/`Completion`/... as placeholder types whose every method was
// `unimplemented!()`. That seam satisfies every `needs_*` bound above and would
// pass the whole compile-time contract while being incapable of running a loop.
// Only actually driving the public surface distinguishes the two.
// ---------------------------------------------------------------------------
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn public_loop_surface_is_live() {
    #[cfg(target_os = "linux")]
    let mut l = {
        // dynamic.zig:18-19 — the documented startup call.
        rustty_xev::detect().expect("detect() must find an available backend");
        assert!(
            rustty_xev::CANDIDATES.contains(&rustty_xev::backend()),
            "detect() must land on a candidate backend"
        );
        rustty_xev::Loop::init(rustty_xev::Options::default()).expect("Loop::init")
    };
    #[cfg(target_os = "macos")]
    let mut l = rustty_xev::Loop::init().expect("Loop::init");

    // A real loop: it ticks, it stops, it reports stopped.
    assert!(!l.stopped());
    l.run(rustty_xev::RunMode::NoWait).expect("Loop::run");
    l.stop();
    assert!(l.stopped(), "stop() must be observable through stopped()");
    l.deinit();
}

/// The zero-value contract of the public `Completion`/`WriteQueue` — api.zig's
/// `test "completion is zero-able"` seen from OUTSIDE the crate (the shape
/// termio's `SegmentedPool`/`write_queue` rely on).
//
// PORT NOTE (`state_of`, not `state`): the reference names this decl `state`
// (kqueue.zig:1035 / epoll.zig:941 / io_uring.zig:649 / dynamic.zig:270), but
// `kqueue.rs` had to rename the public `&self` convenience form to `state_of`
// because its raw-pointer discipline already owns the name `state` for the
// `*mut Completion` associated fn (kqueue.rs:1648-1659). `state_of` is therefore
// the spelling the macOS seam exports and the one termio's completions already
// call (exec.rs:799, thread.rs:1022/1173) — so the Linux side must answer to it
// too, or `pub use rustty_xev::*` is only nominally the same surface. Asserted
// here rather than left to Task 9 to trip over.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn public_zero_values_construct() {
    let c: rustty_xev::Completion = rustty_xev::Completion::default();
    assert_eq!(c.state_of(), rustty_xev::CompletionState::Dead);

    let q: rustty_xev::WriteQueue = rustty_xev::WriteQueue::default();
    assert!(
        q.empty(),
        "a zero-initialized WriteQueue has no pending writes"
    );

    let _r: rustty_xev::WriteRequest = rustty_xev::WriteRequest::default();
}
