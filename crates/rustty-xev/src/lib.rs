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
//! CHUNK STATUS: C1 (queue/queue_mpsc/heap) + C2 (darwin) + C3 (loop core:
//! `Loop`/`Completion`/`Operation`/`Result`, the tick, `Async`(mach) wakeup
//! subset, the `Timer` watcher run) + C4 (`Async` mach-port watcher) + C5
//! (`Process` EVFILT_PROC watcher) have LANDED. `Stream`/`WriteQueue`/
//! `WriteRequest` remain sequencing placeholders (chunk C6) — filled in place;
//! downstream `use` paths never change. See .port/phase-2/XEV_BLUEPRINT.md +
//! DEFERRALS.md.

// PORT: `rustty_global::seam::Xev` (= `()`), the deferred xev backend package.
#[allow(unused_imports)]
use rustty_global::xev as _DeferredXevBackend;

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
#[cfg(target_os = "macos")]
mod thread_pool;
#[cfg(target_os = "macos")]
mod timer;

// ---------------------------------------------------------------------------
// The macOS/kqueue package re-export surface (= the kqueue `Api()` decls the
// consumers name as `xev::*`). Filled by the real port (C3).
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
pub use looppkg::{CallbackAction, CompletionState, Options, RunMode};
#[cfg(target_os = "macos")]
pub use process::Process;
#[cfg(target_os = "macos")]
pub use timer::Timer;

/// PORT: the error set of `Loop.init`/`Async.init`/`Timer.init` — the unified D6
/// seam error (every libxev alloc/syscall failure collapses here; termio
/// propagates `Loop::init()?`/`Async::init()?`/`Timer::init()?` through one type).
#[derive(Debug)]
pub struct Error;

/// PORT: `xev.Async.NotifyError` (the error set of `Async.notify`).
//
// PORT NOTE: the derives are required by rustty-core's `surface::Error` union
// (core C7), which embeds this error and itself derives `Copy`/`PartialEq` (Zig
// error sets are trivially copyable/comparable values, so the derives are
// faithful). [ADV-FIX F3]: load-bearing for the workspace build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyError;

// ---------------------------------------------------------------------------
// Sequencing placeholders for the not-yet-ported `Stream` watcher (chunk C6).
// Filled IN PLACE; downstream `use` paths never change. termio names these types +
// default-constructs `WriteRequest`/`WriteQueue` (F3), so the trait surface is
// preserved. Method bodies are `unimplemented!()` dependency markers — no
// reference logic is stubbed behind them.
// ---------------------------------------------------------------------------

/// PORT: `xev.Stream` (the pty write stream — `xev.Stream.initFd(pty_fds.write)`
/// in Exec's threadEnter; chunk C6).
pub struct Stream {}

impl Stream {
    pub fn deinit(&mut self) {
        unimplemented!("xev Stream deferred (XEV_BLUEPRINT chunk C6): rustty-xev tier")
    }
}

/// PORT: `xev.WriteRequest` (the pooled stream-write request — Exec's
/// `write_req_pool: SegmentedPool(xev.WriteRequest, ...)`; default-initialized
/// by the pool; chunk C6).
#[derive(Default)]
pub struct WriteRequest {}

/// PORT: `xev.WriteQueue` (default-initialized `.{}` in the reference — Exec's
/// `write_queue` field; chunk C6).
#[derive(Default)]
pub struct WriteQueue {}

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

    /// PORT: `xev.Loop` placeholder (non-macOS backend deferred).
    pub struct Loop {}
    /// PORT: `xev.Async` placeholder (non-macOS backend deferred).
    #[derive(Clone, Copy)]
    pub struct Async {}
    /// PORT: `xev.Completion` placeholder (non-macOS backend deferred).
    #[derive(Default)]
    pub struct Completion {}
    /// PORT: `xev.Timer` placeholder (non-macOS backend deferred).
    pub struct Timer {}
    /// PORT: `xev.Process` placeholder (non-macOS backend deferred).
    #[derive(Clone, Copy)]
    pub struct Process {}

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
