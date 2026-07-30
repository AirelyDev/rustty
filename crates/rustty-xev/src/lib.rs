//! rustty-xev
//!
//! PARITY (D6): the `xev` event-loop backend — a faithful port of the libxev
//! LIBRARY (mitchellh/libxev, the commit ghostty pins). Live on TWO targets:
//! macOS/kqueue (XEV_BLUEPRINT §0: there `xev.Dynamic` IS the static kqueue
//! `Api()`, since `Backend::candidates()` has one element, so `dynamic.zig` is
//! comptime-elided/deferred, not weakened) and Linux/`{io_uring, epoll}` behind
//! the real `Dynamic` selector (§1 Linux port, tasks L1 LT1-LT8 — two candidates,
//! so nothing elides). WS20 adds the remaining two reference backends —
//! WASI/`wasi_poll` (`wasi_poll.rs` + `wasi.rs`) and Windows/`iocp` (`iocp.rs` +
//! `windows.rs`, itself the port of libxev's own `src/windows.zig`) — each a
//! single-candidate target, so each is the flat static `Api()` exactly as
//! macOS/kqueue is. Those two are CROSS-COMPILE-VERIFIED ONLY on this host
//! (`cargo check --target {x86_64-pc-windows-msvc, wasm32-wasip1}`); their
//! reference tests are ported and compile but cannot be EXECUTED here — that is
//! verification debt with a runnable recipe in
//! `.port/phase-2/VERIFICATION_DEBT.md`, NOT a scope exclusion.
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
//! below) AND the whole in-scope Linux surface (L1 LT1–LT8) are filled.
//! (`Timer::cancel` WAS additionally deferred on Linux; L1 LT9 un-defers it —
//! `dynamic.rs`'s `Timer::cancel` now carries the real io_uring `timer_remove` /
//! epoll `cancel` arms of timer.zig:137-210.)
//! (Marker normalized from 'CHUNK STATUS' per VALIDATION_REPORT.md §3b.)
//!
//! POST-MILESTONE (Tasks 1-6, this branch): the `ThreadPool` body (kprotty's Zap
//! work-stealing pool) is now the FULL faithful port on BOTH targets (Task 1,
//! incl. a ledgered deliberate deviation from the Zig source — see
//! DEFERRALS.md), and Linux's `File`/`TCP`/`UDP` watchers (Tasks 3-5) route their
//! blocking ops through it (Task 2's `epoll::Loop` pool wiring). The ONE
//! remaining ledgered deferral is macOS-only: the macOS `File`/`TCP`/`UDP`
//! watchers themselves remain UNPORTED (zero ghostty uses there — DEFERRALS.md's
//! C7 finale entry), and the kqueue loop's pool-consumption wiring is part of
//! that SAME deferred chunk — the "kqueue: file IO on thread pool" honest-skip
//! stub (kqueue.rs:3660-3661: "the Zap pool + `xev.File` watcher are their own
//! chunk"). There is no macOS `File` watcher whose pool use could be stubbed;
//! watchers and pool-consumption defer together. The Linux `epoll`/`io_uring`
//! `File`/`TCP`/`UDP` watchers have no such gap.
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
//! never change. Milestone A (isolated real `rustty-xev`) is DONE.
//!
//! LINUX LOG (§1 Linux port): LT1 (module skeleton + `backend.zig`'s `.linux`
//! arms), LT2 (`linux/`: timerfd/eventfd/net plus the pure `loop.zig` ABI twins),
//! LT3–LT4 (`backend/epoll.zig`: the whole loop + its watcher surfaces), LT5–LT6
//! (`backend/io_uring.zig`: ditto, over the hand-declared io_uring ABI libc
//! lacks), LT7 (`dynamic.zig`: the hand-authored `Xev(&.{.io_uring, .epoll})`
//! instantiation — Rust has no comptime `@Type`/`@unionInit`), LT8 (THIS
//! `lib.rs` per-target surface: Linux re-exports `dynamic::*` under the SAME
//! public names the macOS block exports, so `rustty_xev::Loop` etc. resolve to
//! `Dynamic` on Linux and stay flat kqueue on macOS — pinned by
//! `tests/surface_parity.rs`; plus the [ADV-FIX F7] count-gate rows) have LANDED.
//! Milestone-A-equivalent for Linux is DONE. See
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

/// The backend in use (the reference `xev.backend` read).
///
/// PORT NOTE: on macOS the crate surface IS the static kqueue `Api()`
/// (XEV_BLUEPRINT §0 — `Backend::candidates()` is the single-element
/// `&[kqueue]`, comptime-eliding the Dynamic layer), and libxev's `Xev(be, T)`
/// declares `pub const backend = be;` — so this is the comptime `.kqueue`.
#[cfg(target_os = "macos")]
pub fn backend() -> Backend {
    Backend::Kqueue
}

// XEV_BLUEPRINT §1 Linux port: the io_uring/epoll/Dynamic backends (backend.zig's
// `.linux => &.{ .io_uring, .epoll }` candidates, wired above in `backend.rs`).
// LT1 landed these as empty stubs; LT3–LT7 filled each in place, and LT8 wires the
// package surface below — downstream `use` paths never changed, exactly as planned.
//  - `epoll` (backend/epoll.zig) + `io_uring` (backend/io_uring.zig): each is a
//    SELF-CONTAINED whole backend (its own Loop/Completion/Operation/Result/
//    Callback + the Async/Timer/Process/Stream watcher surfaces), mirroring how
//    `kqueue` + its macOS siblings form the macOS backend.
//  - `dynamic` (dynamic.zig): the 2-candidate `Xev(bes)` selector over them — the
//    Linux package surface (§0's single-candidate elision does NOT apply here).
//  - `linux` groups the Linux-only helper bindings (timerfd.zig, eventfd, the
//    std.net subset) the way `darwin`/`kqueue`'s siblings group the macOS-only
//    ones above, plus byte-faithful twins of `loop.zig`'s pure ABI enums (R5
//    forbids importing the macOS-gated `looppkg`).
#[cfg(target_os = "linux")]
mod dynamic;
#[cfg(target_os = "linux")]
mod epoll;
#[cfg(target_os = "linux")]
mod io_uring;
#[cfg(target_os = "linux")]
mod linux;

// WS20: the two remaining reference backends, ported for real (they were briefly
// and WRONGLY recorded as "out of scope by platform"; the binding rule is that
// everything the reference supports gets PORTED, and an inability to BUILD or RUN
// on the porting host is VERIFICATION DEBT, never a scope exclusion).
//
//  - `wasi_poll` (backend/wasi_poll.zig) + `wasi` (its `std.os.wasi`/`loop.zig`
//    ABI companions, the twin of `linux.rs`) — the WASI `poll_oneoff` backend.
//  - `iocp` (backend/iocp.zig) + `windows` (the port of libxev's own
//    `src/windows.zig` Win32 shim, plus the `loop.zig` ABI twins) — the Win32
//    I/O-completion-port backend.
//
// Each is a SELF-CONTAINED whole backend exactly as `kqueue`/`epoll`/`io_uring`
// are, and each is a SINGLE-candidate target (backend.rs: `.wasi =>
// &.{.wasi_poll}`, `.windows => &.{.iocp}`), so — like macOS/kqueue (§0) — the
// Dynamic layer comptime-elides and the crate surface IS the backend's `Api()`.
#[cfg(target_os = "windows")]
mod iocp;
#[cfg(target_os = "wasi")]
mod wasi;
#[cfg(target_os = "wasi")]
mod wasi_poll;
#[cfg(target_os = "windows")]
mod windows;

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
//  - `thread_pool` (ThreadPool.zig): the full body (Task 1 — kprotty's Zap
//    work-stealing pool), shared with the Linux epoll/io_uring backends below.
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
// XEV_BLUEPRINT §1/§6 chunk C3 -> Task 1 (`.superpowers/sdd/task-1-brief.md`): the
// full faithful port of `ThreadPool.zig` (kprotty's Zap work-stealing pool) now
// compiles on BOTH macOS (its original home) and Linux — the Linux epoll File/TCP/
// UDP watchers route their blocking ops (pread/pwrite/close) through this pool
// (Task 2's `epoll::Loop` pool wiring). The lock-free `NodeQueue`/`NodeBuffer`
// primitives were proven Stacked- AND Tree-Borrows clean on
// `.port/phase-2/mirihar-threadpool/` before shipping. Re-exported on BOTH targets
// below (Task 6 un-gates the Linux `pub use`, matching the macOS one that was
// already there) — library-completeness parity: `dynamic::Options.thread_pool`'s
// `*mut crate::thread_pool::ThreadPool` field needs the type nameable by any
// caller that wants to construct one, exactly as the macOS `Options` field does.
// WS20: un-gated to ALL targets. `main.zig` exports `pub const ThreadPool =
// @import("ThreadPool.zig")` unconditionally, and the module's only platform
// surface is `std.Thread.Futex`, whose Windows and single-threaded (WASI /
// freestanding wasm) arms are now ported alongside the Linux/Darwin ones.
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
// (On macOS: `Async`/`Process`/`Stream`/`Timer` live; `File`/`TCP`/`UDP` are
// still deferred there — zero ghostty uses on kqueue. On Linux, below, ALL of
// `Async`/`Process`/`Stream`/`Timer`/`File`/`TCP`/`UDP` are done, §1 Tasks 2-5.)
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
// PORT NOTE: `Process.wait`'s callback error type (`pub const WaitError =
// xev.Sys.ProcError;`, watcher/process.zig:150) must be nameable by consumers —
// ghostty's termio spells this parameter `r: xev.Process.WaitError!u32`
// (Exec.zig:303), NOT `xev.Sys.ProcError`, and a named fn-item callback cannot
// infer it. The `process` module is private, so re-export the alias (the same
// export gap the `TimerRunError`/`WaitError` re-exports closed; the underlying
// `ProcError` is already re-exported via the `kqueue` block).
//
// WHY THE `ProcessWaitError` SPELLING: Rust has no stable inherent associated
// type, so a watcher's `pub const <X>Error` flattens to a crate-root
// `<Watcher><X>Error` — the convention `TimerRunError` (timer.zig's `Timer.RunError`)
// already set. `Async.WaitError` took the unprefixed `WaitError` first, so
// `Process.WaitError` takes the prefixed spelling. This is ALSO the name the Linux
// block below already exports for the merged `ErrorSet(&.{"Process","WaitError"})`
// (watcher/process.zig:368), so ONE neutral name — the reference's own
// `Process.WaitError` — now resolves on both targets and termio's flat
// `pub use rustty_xev::*` seam needs no per-target spelling.
#[cfg(target_os = "macos")]
pub use process::WaitError as ProcessWaitError;

// ---------------------------------------------------------------------------
// The Linux/Dynamic package re-export surface (XEV_BLUEPRINT §1 Linux port, task
// L1 LT8) — the exact twin of the macOS block above, under the SAME public names.
//
// WHY THE SAME NAMES ARE THE WHOLE POINT: ghostty imports the package as
// `@import("xev").Dynamic` (global.zig), i.e. `DynamicXev(Backend.candidates())`
// — ONE import for every target. The per-target split happens INSIDE
// `dynamic.zig`:
//
//   * macOS — `candidates() == &.{.kqueue}`, and `Xev(bes)` short-circuits at
//     "Ensure that if we only have one candidate that we have no overhead to use
//     the dynamic API: if (bes.len == 1) return bes[0].Api();" (dynamic.zig:36-38).
//     So `Dynamic` IS the flat kqueue `Api()` — the §0 fact that comptime-elides
//     the whole Dynamic layer there, which is why the block above re-exports
//     `kqueue`/`timer`/`stream`/... directly.
//
//   * Linux — `candidates() == &.{.io_uring, .epoll}` (backend.zig's `.linux`
//     arm). Two candidates, so `Xev(bes)` really does generate the `Dynamic`
//     struct, and THAT is the package surface. Hence: `dynamic::*` here.
//
// Downstream never sees the difference (`rustty-termio`'s `xev_seam` is a flat
// `pub use rustty_xev::*`), which is exactly the property `tests/surface_parity.rs`
// pins on both targets.
//
// PORT NOTE (what is deliberately NOT forwarded on Linux, and why — each is the
// reference's own decision, not a weakening):
//  - `Callback` / `noopCallback` / `Result` / `Operation` / `Sys`: `dynamic.zig`'s
//    forwarded-decl list (dynamic.zig:56-88) contains none of them. They are
//    backend-typed, and a Dynamic `Completion`'s op/callback are private, so a
//    consumer could not build one to hand to `Loop.add` anyway. The macOS block
//    exports them because there the surface IS the static kqueue `Api()`.
//  - `KEventError`/`MachPortError`/`ProcError`/`TimerError`/`TimerTrigger`/
//    `ShutdownHow`: kqueue/Darwin-specific spellings with no Linux analogue; the
//    Linux twins are the merged `ErrorSet`s below (`ProcessWaitError`,
//    `TimerRunError`, ...).
//  - `File`/`TCP`/`UDP`: Linux-only exports (Tasks 3/4/5); still deferred on macOS
//    (zero ghostty uses there — the kqueue block above omits them).
//
// `Backend` (above), `Error` + `NotifyError` (below) are declared ONCE at the
// crate root for every target and so are absent from this list: `dynamic::Error`/
// `dynamic::NotifyError` are already aliases OF those crate-root types (the
// unified D6 seam error), and re-exporting an alias of a local item under its own
// name would collide with the declaration.
//
// PORT NOTE (`ThreadPool`, Task 6 — RESOLVED, was listed here as NOT forwarded):
// `dynamic.zig:56` forwards `main.ThreadPool`; that decl is NOT a `dynamic::*`
// member here (it lives in the crate-root `thread_pool` module both backends
// share, same as macOS), so it is `pub use`d separately below rather than folded
// into the `dynamic::{...}` list. With the Linux `ThreadPool` body ported (Task 1)
// and `epoll::Loop`/`io_uring::Loop` wired to accept a real
// `Option<*mut thread_pool::ThreadPool>` (Task 2's `dynamic::Options.thread_pool`
// field — no more opaque `*mut c_void`), the type is nameable under the SAME
// public name the macOS block above exports it under.
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
pub use dynamic::{
    AcceptError, Async, CallbackAction, CancelError, CloseError, Completion, CompletionState,
    ConnectError, File, Loop, Options, PollError, PollEvent, Process, ProcessWaitError, ReadBuffer,
    ReadError, RunMode, ShutdownError, Stream, Timer, TimerCancelError, TimerRunError, UdpState,
    WaitError, WriteBuffer, WriteError, WriteQueue, WriteRequest, TCP, UDP,
};
#[cfg(target_os = "linux")]
pub use thread_pool::ThreadPool;
// The `Dynamic`-only decls (`main.zig` forwards `dynamic`/`backend`/`available`
// from the selected Api(); on macOS they are the elided single-candidate ones —
// `dynamic == false`, `backend == .kqueue` — and are not exported there).
//
// PORT NOTE: `available` is NOT forwarded. `main.zig:11`'s `pub const available =
// xev.available;` is the *static* Api()'s "is this backend usable on this system"
// probe; `Dynamic` has no such decl (it has `detect`/`prefer`, which consult each
// candidate's `available()` internally — dynamic.zig:98/113). `epoll::available`/
// `io_uring::available` stay module-private, reachable only through those two.
//
// PORT NOTE (name collision, benign): the private `mod backend` (backend.zig)
// occupies `backend` in the TYPE namespace; this `pub use` puts the `fn backend()`
// reader of `dynamic.zig:91`'s `pub var backend` in the VALUE namespace. Rust
// keeps the two apart, so `rustty_xev::backend()` is the function and the module
// stays private — the same `backend`/`Backend` pairing `main.zig:9`+`main.zig:91`
// has, where Zig's single namespace distinguishes them only by case.
#[cfg(target_os = "linux")]
pub use dynamic::{backend, detect, prefer, CANDIDATES, DYNAMIC};

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
// The WASI/`wasi_poll` package re-export surface (WS20).
//
// `backend.zig`'s `.wasi` arm is the SINGLE-candidate `&.{.wasi_poll}`, so — like
// macOS/kqueue (§0) — `Xev(bes)` short-circuits to `bes[0].Api()` and the crate
// surface IS the flat static `wasi_poll` Api(). Hence the direct `wasi_poll::*` /
// `wasi::*` re-exports below, mirroring the macOS block above rather than the
// Linux `dynamic::*` one.
// ---------------------------------------------------------------------------
#[cfg(target_os = "wasi")]
pub use wasi::{CallbackAction, CompletionState, Options, RunMode};
#[cfg(target_os = "wasi")]
pub use wasi_poll::{
    noop_callback, AcceptError, AsyncError, Callback, CancelError, CloseError, Completion,
    ConnectError, Loop, Operation, ReadBuffer, ReadError, Result, ShutdownError, ShutdownHow,
    TimerError, TimerTrigger, WriteBuffer, WriteError,
};

// ---------------------------------------------------------------------------
// The Windows/`iocp` package re-export surface (WS20).
//
// `backend.zig`'s `.windows` arm is the SINGLE-candidate `&.{.iocp}` — the same
// §0 elision as macOS and WASI, so the crate surface IS the flat static `iocp`
// Api().
// ---------------------------------------------------------------------------
#[cfg(target_os = "windows")]
pub use iocp::{
    noop_callback, AcceptError, AsyncError, Callback, CancelError, CloseError, Completion,
    ConnectError, JobObjectError, JobObjectResult, Loop, Operation, ReadBuffer, ReadError, Result,
    ShutdownError, ShutdownHow, TimerError, TimerTrigger, WriteBuffer, WriteError,
};
#[cfg(target_os = "windows")]
pub use windows::{CallbackAction, CompletionState, Options, RunMode};

// `main.zig`'s `pub const ThreadPool = @import("ThreadPool.zig")` — exported on
// every target (WS20), matching the macOS/Linux re-exports above.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub use thread_pool::ThreadPool;

// ---------------------------------------------------------------------------
// REMAINING PORT WORK (explicitly NOT a scope exclusion). The `iocp` and
// `wasi_poll` LOOPS are ported in full (iocp.rs / wasi_poll.rs). The HIGH-LEVEL
// WATCHER layer (`watcher/async.zig`, `watcher/timer.zig`, `watcher/process.zig`,
// `watcher/stream.zig`) is generic over the backend in the reference and has so
// far been instantiated only against `kqueue` (macOS: `async.rs`/`timer.rs`/
// `process.rs`/`stream.rs`) and the Linux backends (`dynamic.rs`). Instantiating
// it against `iocp`/`wasi_poll` is the next chunk of this same lane — tracked in
// `.port/phase-2/DEFERRALS.md` (WS20) with a concrete recipe, NOT recorded as out
// of scope by platform.
//
// Until it lands these handle types keep placeholder bodies so the crate's public
// surface stays nameable on those targets. The panic messages say exactly what
// they are: unported work with a ledger row.
// ---------------------------------------------------------------------------
#[cfg(any(target_os = "windows", target_os = "wasi"))]
pub use watcher_seam::*;

#[cfg(any(target_os = "windows", target_os = "wasi"))]
mod watcher_seam {
    use super::Error;

    const MSG: &str = "rustty-xev: the libxev WATCHER layer (watcher/{async,timer,process,\
                       stream}.zig) is not yet instantiated against this backend. The iocp/\
                       wasi_poll LOOPS are ported (iocp.rs / wasi_poll.rs); this is REMAINING \
                       PORT WORK tracked in .port/phase-2/DEFERRALS.md (WS20), NOT a platform \
                       scope exclusion.";

    /// PORT NOTE: `xev.Async` — watcher layer, see the module note.
    #[derive(Clone, Copy)]
    pub struct Async {}
    /// PORT NOTE: `xev.Timer` — watcher layer, see the module note.
    pub struct Timer {}
    /// PORT NOTE: `xev.Process` — watcher layer, see the module note.
    #[derive(Clone, Copy)]
    pub struct Process {}
    /// PORT NOTE: `xev.Stream` — watcher layer, see the module note.
    #[derive(Clone, Copy)]
    pub struct Stream {}
    /// PORT NOTE: `xev.WriteRequest` — Exec's `SegmentedPool<xev::WriteRequest>`
    /// requires `Default`.
    #[derive(Default)]
    pub struct WriteRequest {}
    /// PORT NOTE: `xev.WriteQueue` — Exec's `write_queue` field default-constructs it.
    #[derive(Default)]
    pub struct WriteQueue {}

    impl Stream {
        pub fn deinit(&self) {
            unimplemented!("{}", MSG)
        }
    }

    impl Async {
        pub fn init() -> Result<Async, Error> {
            unimplemented!("{}", MSG)
        }
        pub fn deinit(&mut self) {
            unimplemented!("{}", MSG)
        }
        pub fn notify(&self) -> Result<(), super::NotifyError> {
            unimplemented!("{}", MSG)
        }
    }

    impl Timer {
        pub fn init() -> Result<Timer, Error> {
            unimplemented!("{}", MSG)
        }
        pub fn deinit(&self) {
            unimplemented!("{}", MSG)
        }
    }

    impl Process {
        pub fn init(_pid: i32) -> Result<Process, Error> {
            unimplemented!("{}", MSG)
        }
        pub fn deinit(&mut self) {
            unimplemented!("{}", MSG)
        }
    }
}

// ---------------------------------------------------------------------------
// The NO-BACKEND seam: a target the REFERENCE ITSELF has no backend for.
//
// `backend.zig` enumerates exactly five backends and maps every target it
// supports onto one of them. Freestanding wasm (`wasm32-unknown-unknown`) is NOT
// one of them: the reference's `.wasi` arm is the WASI ABI, and a freestanding
// wasm module has no `wasi_snapshot_preview1` import to poll. rustty widens
// `backend.rs`'s `default()`/`candidates()` to answer `WasiPoll` there only so the
// enum stays total (WS14 T6); there is no reference body to port for it.
//
// This is therefore NOT "a backend we chose not to port" — it is a target the
// upstream library does not support. Every rustty artifact for this target
// (`rustty-wasm`, `rustty-web-canvas`, `rustty-vt-c`) runs the SYNCHRONOUS C-ABI
// with no event loop, which is why the crate is only required to TYPE-CHECK here.
// ---------------------------------------------------------------------------

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "windows",
    target_os = "wasi"
)))]
pub use no_backend_seam::*;

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "windows",
    target_os = "wasi"
)))]
mod no_backend_seam {
    use super::Error;

    const MSG: &str = "rustty-xev: libxev declares no backend for this target (backend.zig maps \
                       every supported target onto one of io_uring/epoll/kqueue/wasi_poll/iocp; \
                       freestanding wasm has no WASI ABI to poll). Nothing to port — no rustty \
                       artifact for this target runs an event loop.";

    /// PORT NOTE: `xev.Loop` placeholder — no reference backend for this target.
    pub struct Loop {}
    /// PORT NOTE: `xev.Async` placeholder — no reference backend for this target.
    #[derive(Clone, Copy)]
    pub struct Async {}
    /// PORT NOTE: `xev.Completion` placeholder — no reference backend for this target.
    #[derive(Default)]
    pub struct Completion {}
    /// PORT NOTE: `xev.Timer` placeholder — no reference backend for this target.
    pub struct Timer {}
    /// PORT NOTE: `xev.Process` placeholder — no reference backend for this target.
    #[derive(Clone, Copy)]
    pub struct Process {}
    /// PORT NOTE: `xev.Stream` placeholder — no reference backend for this target.
    #[derive(Clone, Copy)]
    pub struct Stream {}
    /// PORT NOTE: `xev.WriteRequest` placeholder — Exec's
    /// `SegmentedPool<xev::WriteRequest>` requires `Default`.
    #[derive(Default)]
    pub struct WriteRequest {}
    /// PORT NOTE: `xev.WriteQueue` placeholder — Exec's `write_queue` field
    /// default-constructs it.
    #[derive(Default)]
    pub struct WriteQueue {}

    impl Stream {
        pub fn deinit(&self) {
            unimplemented!("{}", MSG)
        }
    }

    impl Loop {
        pub fn init() -> Result<Loop, Error> {
            unimplemented!("{}", MSG)
        }
        pub fn deinit(&mut self) {
            unimplemented!("{}", MSG)
        }
    }

    impl Async {
        pub fn init() -> Result<Async, Error> {
            unimplemented!("{}", MSG)
        }
        pub fn deinit(&mut self) {
            unimplemented!("{}", MSG)
        }
        pub fn notify(&self) -> Result<(), super::NotifyError> {
            unimplemented!("{}", MSG)
        }
    }

    impl Timer {
        pub fn init() -> Result<Timer, Error> {
            unimplemented!("{}", MSG)
        }
        pub fn deinit(&self) {
            unimplemented!("{}", MSG)
        }
    }

    impl Process {
        pub fn init(_pid: i32) -> Result<Process, Error> {
            unimplemented!("{}", MSG)
        }
        pub fn deinit(&mut self) {
            unimplemented!("{}", MSG)
        }
    }
}

#[cfg(test)]
mod backend_accessor_tests {
    /// WS3-B: `xev.backend` — macOS is the flat static kqueue Api
    /// (`pub const backend = be;` in libxev main.zig's Xev(be, T)), so the
    /// accessor is the comptime `.kqueue`.
    #[cfg(target_os = "macos")]
    #[test]
    fn backend_is_kqueue_on_macos() {
        assert_eq!(crate::Backend::Kqueue, crate::backend());
    }

    /// WS3-B: on Linux the accessor is the Dynamic layer's runtime global
    /// (dynamic.zig:91) — always one of the Linux candidates.
    #[cfg(target_os = "linux")]
    #[test]
    fn backend_is_a_linux_candidate() {
        let be = crate::backend();
        assert!(matches!(
            be,
            crate::Backend::IoUring | crate::Backend::Epoll
        ));
    }
}
