//! rustty-xev
//!
//! PARITY (D6): the `xev` event-loop backend — a faithful port of the libxev
//! LIBRARY (mitchellh/libxev, the commit ghostty pins), macOS/kqueue-only for now
//! (XEV_BLUEPRINT §0: on macOS `xev.Dynamic` IS the static kqueue `Api()`, so
//! `dynamic.zig` is comptime-elided/deferred, not weakened).
//!
//! Every termio reference file (and, with the core tier, renderer/Thread.zig +
//! core Surface.zig) imports `const xev = @import("../global.zig").xev` — the
//! libxev async event-loop backend (`@import("xev").Dynamic`). Per the D6
//! decision, libxev is Zig source (not C) and is ported faithfully INTO THIS
//! CRATE (vendored Zig-source precedent: rustty-z2d). `rustty-global` carries the
//! package as the `seam::Xev` (= `()`) anchor exactly as `global.zig`'s `pub
//! const xev` is; the concrete handle types live here.
//!
//! PORT STATUS: done — the whole in-scope macOS/kqueue surface (chunks C1–C7
//! below) is filled; the `ThreadPool` body is the one ledgered deferral
//! (DEFERRALS.md "kqueue: file IO on thread pool", honest-skip stub).
//! (Marker normalized from 'CHUNK STATUS' per VALIDATION_REPORT.md §3b.)
//!
//! CHUNK LOG: C1 (queue/queue_mpsc/heap) + C2 (darwin) + C3 (loop core:
//! `Loop`/`Completion`/`Operation`/`Result`, the tick, `Async`(mach) wakeup
//! subset, the `Timer` watcher run) + C4 (`Async` mach-port watcher) + C5
//! (`Process` EVFILT_PROC watcher) + C6 (`Stream`/`WriteQueue`/`WriteRequest` +
//! the `Writeable.queueWrite` ordered-write engine) + C7 (THE FINALE: this
//! `lib.rs` package surface = the kqueue `Api()` — `main.zig` re-exports +
//! `api.zig`'s 2 `test{}` + `backend.zig`'s `Backend`; `Timer::reset`/`cancel`
//! un-deferred with `watcher/timer.zig`'s 5 `test{}`) have LANDED — the whole
//! macOS/kqueue in-scope surface is now filled in place; downstream `use` paths
//! never change. Milestone A (isolated real `rustty-xev`) is DONE. See
//! .port/phase-2/XEV_BLUEPRINT.md + DEFERRALS.md + NEXT_PHASE_PLAN.md.

// PORT NOTE: `rustty_global::seam::Xev` (= `()`), the deferred xev backend package.
#[allow(unused_imports)]
use rustty_global::xev as _DeferredXevBackend;

// XEV_BLUEPRINT §1/§7 chunk C7: `backend.zig` — the `Backend` enum + `default()`/
// `candidates()`. Cross-platform (the enum names every backend; the `#[cfg]` arms
// pick the live one), so NOT macOS-gated. `Backend::default() == Kqueue` +
// single-element `candidates()` are the §0 fact that comptime-elides `dynamic.zig`.
mod backend;
pub use backend::Backend;

// XEV_BLUEPRINT §1 Linux port, task L1 LT1: gated module skeleton for the
// io_uring/epoll/Dynamic backends (backend.zig's `.linux => &.{ .io_uring,
// .epoll }` candidates, wired above in `backend.rs`). Empty stubs ONLY here —
// later L-tier tasks fill each module in place; downstream `use` paths won't
// change. `linux` groups the Linux-only helper bindings (timerfd.zig, ...) the
// way `darwin`/`kqueue`'s siblings group the macOS-only ones above.
#[cfg(target_os = "linux")]
mod dynamic;
#[cfg(target_os = "linux")]
mod epoll;
#[cfg(target_os = "linux")]
mod io_uring;
#[cfg(target_os = "linux")]
mod linux;

// XEV_BLUEPRINT §8 chunk C1: the pure intrusive data structures that establish the
// raw-pointer discipline for the whole libxev port (queue.zig / queue_mpsc.zig /
// heap.zig, byte-faithful). The kqueue loop (C3) uses them (`submissions`/
// `thread_pool_completions`/`TimerHeap`); C7 wires the package re-export surface.
#[allow(dead_code)]
mod heap;
#[allow(dead_code)]
mod queue;
#[allow(dead_code)]
mod queue_mpsc;

// XEV_BLUEPRINT §3 chunk C2: the darwin/syscall layer (`darwin.zig` ported
// byte-faithfully) — the mach-port / `mach_msg` bindings + `KernE`/`MachMsgE`
// the kqueue `Wakeup` (C3) and `Async`(mach) ride. macOS/kqueue-only. The kqueue
// syscall consts (`EVFILT_*`/`EV_*`/`NOTE_*`) + `kevent64`/`kevent64_s` come from
// `libc` directly ([ADV-FIX F4]); this module binds only the mach APIs libc lacks.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod darwin;

// XEV_BLUEPRINT §2/§8 chunk C3: the loop core.
//  - `looppkg` (loop.zig): Options/RunMode/Callback/NoopCallback/CallbackAction/
//    CompletionState (the callback ABI). `loop` is a Rust keyword → `#[path]`.
//  - `common` (watcher/common.zig): the typed→untyped `userdata_value` bridge.
//  - `thread_pool` (ThreadPool.zig): the TYPE SURFACE only (body deferred, §1/§6).
//  - `kqueue` (backend/kqueue.zig): the whole loop — `Loop`/`Completion`/
//    `Operation`/`Result`/`Wakeup`/`kevent_syscall`.
//  - `async_watcher` (watcher/async.zig): the `AsyncMachPort` C3 subset (the
//    wakeup handle the loop `Wakeup` embeds). `async` is a Rust keyword → `#[path]`.
//  - `net` (std.net.Address subset): the connect/sendto/recvfrom addr type.
//  - `timer` (watcher/timer.zig): the `xev.Timer` watcher C3 subset (init/run).
#[cfg(target_os = "macos")]
#[path = "async.rs"]
mod async_watcher;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod common;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod kqueue;
#[cfg(target_os = "macos")]
#[path = "loop.rs"]
mod looppkg;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod net;
// XEV_BLUEPRINT §4/§8 chunk C5: the `Process` watcher (`watcher/process.zig`
// `ProcessKqueue`) — the subprocess exit-wait (EVFILT_PROC + `waitpid` reap) that
// termio's Exec rides. The loop `proc` op/perform (Operation::Proc + exit decode)
// landed in C3; this module adds the high-level `xev.Process` watcher surface.
#[cfg(target_os = "macos")]
mod process;
// XEV_BLUEPRINT §4/§8 chunk C6: the `Stream` watcher (`watcher/stream.zig`) — the
// `Shared` piece (`WriteQueue`/`WriteRequest`/`PollError`/`PollEvent`) +
// `GenericStream` (init_fd/deinit/read/write/close/poll) + the `Writeable.queueWrite`
// ordered-write engine termio's pty write path rides. The loop read/write/close ops
// landed in C3; this module adds the high-level `xev.Stream` surface.
#[cfg(target_os = "macos")]
mod stream;
#[cfg(target_os = "macos")]
mod thread_pool;
#[cfg(target_os = "macos")]
mod timer;

// ---------------------------------------------------------------------------
// The macOS/kqueue package re-export surface (= the kqueue `Api()` decls the
// consumers name as `xev::*`). This is the collapse-on-macOS of `main.zig` +
// `api.zig` (§0); their reference doc comments are preserved verbatim here (R3).
//
// [main.zig] The low-level IO interfaces using the recommended compile-time
// interface for the target system. We forward these as the default
// API of this package.
//
// [api.zig] Creates the Xev API based on a backend type.
//
// For the default backend type for your system (i.e. io_uring on Linux),
// this is the main API you interact with. It is forwarded into the main
// the "xev" package so you'd use types such as `xev.Loop`, `xev.Completion`,
// etc.
//
// Unless you're using a custom or specific backend type, you do NOT ever
// need to call the Xev function itself.
//
// Per-decl (the Api() members forwarded here):
//  - `dynamic`: This is used to detect a static vs dynamic API at comptime.
//  - `backend`: The backend that this is. This is supplied at comptime so
//    it is up to the caller to say the right thing. This lets custom
//    implementations also "quack" like an implementation.
//  - `available`: A function to test if this API is available on the
//    current system.
//  - `Callback`: The callback of the main Loop operations. Higher level interfaces
//    may use a different callback mechanism.
//  - `noopCallback` (noop_callback): A callback that does nothing and immediately disarms. This
//    implements xev.Callback and is the default value for completions.
//  - `Sys`: A way to access the raw type.
//
// The high-level helper interfaces that make it easier to perform
// common tasks. These may not work with all possible Loop implementations.
// (`Async`/`Process`/`Stream`/`Timer` live; `File`/`TCP`/`UDP` deferred, §1.)
//
// [main.zig] The dynamic interface that allows for runtime selection of the
// backend to use. This is useful if you want to support multiple
// backends and have a fallback mechanism.
//
// There is a very small overhead to using this API compared to the
// static API, but it is generally negligible.
//
// The API for this isn't _exactly_ the same as the static API
// since it requires initialization of the main struct to detect
// a backend and then this needs to be passed to every high-level
// type such as Async, File, etc. so that they can use the correct
// backend that is coherent with the loop.
// (`Dynamic` is comptime-elided at a single candidate on macOS — §0 — DEFERRED.)
//
// [main.zig] System-specific interfaces. Note that they are always pub for
// all systems but if you reference them and force them to be analyzed
// the proper system APIs must exist. Due to Zig's lazy analysis, if you
// don't use any interface it will NOT be compiled (yay!).
// (`IO_Uring`/`Epoll`/`WasiPoll`/`IOCP` are whole follow-up backend ports.)
//
// [main.zig] This stream (lowercase s) can be used as a namespace to access
// Closeable, Writeable, Readable, etc. so that custom streams
// can be constructed.
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
pub use async_watcher::Async;
#[cfg(target_os = "macos")]
pub use kqueue::{
    AcceptError, CancelError, CloseError, Completion, ConnectError, KEventError, Loop,
    MachPortError, Operation, ProcError, ReadBuffer, ReadError, Result, ShutdownError, ShutdownHow,
    TimerError, TimerTrigger, WriteBuffer, WriteError,
};
#[cfg(target_os = "macos")]
pub use looppkg::{noop_callback, Callback, CallbackAction, CompletionState, Options, RunMode};
#[cfg(target_os = "macos")]
pub use process::Process;
#[cfg(target_os = "macos")]
pub use stream::{PollError, PollEvent, Stream, WriteQueue, WriteRequest};
#[cfg(target_os = "macos")]
pub use thread_pool::ThreadPool;
#[cfg(target_os = "macos")]
pub use timer::Timer;
// PORT NOTE: `Timer.run`/`reset`'s callback error type (`pub const RunError =
// error{Canceled, Unexpected}`) must be nameable by consumers — a termio
// `termiosTimer`/`syncResetCallback`/... callback spells its `r:
// Timer.RunError!void` parameter, and a named fn-item callback cannot infer it.
// The `timer` module is private, so re-export the type here (Milestone-A export
// gap surfaced by Milestone B's first typed Timer callback).
#[cfg(target_os = "macos")]
pub use timer::RunError as TimerRunError;
// PORT NOTE: `Async.wait`'s callback error type (`pub const WaitError =
// xev.Sys.MachPortError;`) must be nameable by consumers — the W2 CVDisplayLink
// KAT (and any typed `Async.wait` callback) spells its `r: Result<(), WaitError>`
// parameter, and the `async_watcher` module is private. Re-export the alias (same
// export gap the `TimerRunError` re-export closed; the underlying `MachPortError`
// is already re-exported via the `kqueue` block).
#[cfg(target_os = "macos")]
pub use async_watcher::WaitError;

// PORT NOTE: `main.zig` also forwards `File`/`TCP`/`UDP` (watchers) + `Dynamic`/
// `DynamicXev` (the runtime backend switch) + `IO_Uring`/`Epoll`/`Kqueue`/
// `WasiPoll`/`IOCP` (the per-backend Api()s) + `Sys`/`dynamic`/`backend`/
// `available`. On macOS these are DEFERRED (XEV_BLUEPRINT §0/§1): `File`/`TCP`/
// `UDP` are unused-by-ghostty watchers; the `Dynamic` layer is comptime-elided at
// a single candidate (§0); the non-kqueue backend Api()s are whole follow-up
// ports; `Sys`/`dynamic`(=false)/`backend`(=kqueue)/`available` are the internal
// Api()-selection decls. See DEFERRALS.md.

/// PORT NOTE: the error set of `Loop.init`/`Async.init`/`Timer.init` — the unified D6
/// seam error (every libxev alloc/syscall failure collapses here; termio
/// propagates `Loop::init()?`/`Async::init()?`/`Timer::init()?` through one type).
#[derive(Debug)]
pub struct Error;

/// PORT NOTE: `xev.Async.NotifyError` (the error set of `Async.notify`).
//
// PORT NOTE: the derives are required by rustty-core's `surface::Error` union
// (core C7), which embeds this error and itself derives `Copy`/`PartialEq` (Zig
// error sets are trivially copyable/comparable values, so the derives are
// faithful). [ADV-FIX F3]: load-bearing for the workspace build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyError;

// [ADV-FIX F6] NO `rustty_global::xev` re-export of these types. `os/flatpak.rs`
// and vt-search reference the xev package; resolving them by making
// `rustty_global::xev` re-export `rustty-xev` would create a `global → xev →
// global` cycle (rustty-xev deps rustty-global). Those refs instead resolve LATER
// via DIRECT edges — `os → xev` (flatpak) and `vt → xev` (search) — both acyclic
// (rustty-xev deps neither os nor vt). `rustty_global::xev` stays the `()` package
// anchor exactly as `global.zig`'s `pub const xev` is (§0 consequence 4).

// ---------------------------------------------------------------------------
// PORT NOTE: `api.zig`'s 2 `test{}` — the `Xev(be, T)` decl-forwarding struct's tests.
// On macOS the Api() collapses to THIS crate's public surface (§0/§1: `main.zig`
// re-exports the kqueue Api() decls), so api.zig's tests live here at the package
// root. The anonymous `test { refAllDecls(@This()); }` → `ref_all_decls` (a helper
// reify; anonymous `test {` is skipped by the count-gate's zig_count, so it is a
// +margin, not the R1-counted one). `test "completion is zero-able"` → the R1
// parity test mapped to api.zig (zig_count=1).
// ---------------------------------------------------------------------------
#[cfg(all(test, target_os = "macos"))]
mod api_tests {
    use super::*;

    // test { @import("std").testing.refAllDecls(@This()); }
    // Reference every in-scope public decl the kqueue Api() forwards (the Rust
    // analogue of refAllDecls — force each re-export to be named/analyzed). This is
    // also the analogue of `main.zig`'s comptime forwarding check:
    // This ensures that all the public decls from the API are forwarded
    // from the main struct. (In Rust a missing `pub use` is a consumer compile
    // error, and this test names each re-export so a drop is caught here too.)
    #[test]
    fn ref_all_decls() {
        // Core loop types.
        let _ = core::mem::size_of::<Loop>();
        let _ = core::mem::size_of::<Completion>();
        let _ = core::mem::size_of::<Result>();
        let _ = core::mem::size_of::<Operation>();
        let _ = core::mem::size_of::<ReadBuffer>();
        let _ = core::mem::size_of::<WriteBuffer>();
        let _ = core::mem::size_of::<Options>();
        let _ = core::mem::size_of::<RunMode>();
        let _ = core::mem::size_of::<CallbackAction>();
        let _ = core::mem::size_of::<CompletionState>();
        // The callback ABI + its noop default.
        let _: Callback = noop_callback;
        // Watchers.
        let _ = core::mem::size_of::<Async>();
        let _ = core::mem::size_of::<Process>();
        let _ = core::mem::size_of::<Stream>();
        let _ = core::mem::size_of::<Timer>();
        let _ = core::mem::size_of::<WriteQueue>();
        let _ = core::mem::size_of::<WriteRequest>();
        // Error sets.
        let _ = core::mem::size_of::<AcceptError>();
        let _ = core::mem::size_of::<CancelError>();
        let _ = core::mem::size_of::<CloseError>();
        let _ = core::mem::size_of::<ConnectError>();
        let _ = core::mem::size_of::<ShutdownError>();
        let _ = core::mem::size_of::<WriteError>();
        let _ = core::mem::size_of::<ReadError>();
        let _ = core::mem::size_of::<PollError>();
        let _ = core::mem::size_of::<PollEvent>();
        let _ = core::mem::size_of::<Error>();
        let _ = core::mem::size_of::<NotifyError>();
        // Backend enum + thread-pool type surface.
        let _ = Backend::default();
        let _ = core::mem::size_of::<ThreadPool>();
    }

    // test "completion is zero-able" { const c: Self.Completion = .{}; _ = c; }
    // The intrusive `Completion` is default-constructible (hand-written `Default`,
    // [ADV-FIX F3]) — the load-bearing contract termio/core rely on.
    #[test]
    fn completion_is_zero_able() {
        let c: Completion = Completion::default();
        let _ = c;
    }
}

// ---------------------------------------------------------------------------
// Non-macOS: the crate is kqueue-only (XEV_BLUEPRINT §0/§1); other backends are
// whole follow-up ports. Keep the D6 seam placeholder handle types so the
// workspace still type-checks off-macOS (real types are macOS-gated above).
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "macos"))]
pub use non_macos_seam::*;

#[cfg(not(target_os = "macos"))]
mod non_macos_seam {
    use super::Error;

    /// PORT NOTE: `xev.Loop` placeholder (non-macOS backend deferred).
    pub struct Loop {}
    /// PORT NOTE: `xev.Async` placeholder (non-macOS backend deferred).
    #[derive(Clone, Copy)]
    pub struct Async {}
    /// PORT NOTE: `xev.Completion` placeholder (non-macOS backend deferred).
    #[derive(Default)]
    pub struct Completion {}
    /// PORT NOTE: `xev.Timer` placeholder (non-macOS backend deferred).
    pub struct Timer {}
    /// PORT NOTE: `xev.Process` placeholder (non-macOS backend deferred).
    #[derive(Clone, Copy)]
    pub struct Process {}
    /// PORT NOTE: `xev.Stream` placeholder (non-macOS backend deferred).
    #[derive(Clone, Copy)]
    pub struct Stream {}
    /// PORT NOTE: `xev.WriteRequest` placeholder (non-macOS backend deferred) — Exec's
    /// `SegmentedPool<xev::WriteRequest>` requires `Default`.
    #[derive(Default)]
    pub struct WriteRequest {}
    /// PORT NOTE: `xev.WriteQueue` placeholder (non-macOS backend deferred) — Exec's
    /// `write_queue` field default-constructs it.
    #[derive(Default)]
    pub struct WriteQueue {}

    impl Stream {
        pub fn deinit(&self) {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
    }

    impl Loop {
        pub fn init() -> Result<Loop, Error> {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
        pub fn deinit(&mut self) {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
    }

    impl Async {
        pub fn init() -> Result<Async, Error> {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
        pub fn deinit(&mut self) {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
        pub fn notify(&self) -> Result<(), super::NotifyError> {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
    }

    impl Timer {
        pub fn init() -> Result<Timer, Error> {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
        pub fn deinit(&self) {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
    }

    impl Process {
        pub fn init(_pid: i32) -> Result<Process, Error> {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
        pub fn deinit(&mut self) {
            unimplemented!(
                "xev backend is kqueue/macOS-only (XEV_BLUEPRINT §0): non-macOS deferred"
            )
        }
    }
}
