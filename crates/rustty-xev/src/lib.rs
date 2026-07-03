//! rustty-xev
//!
//! PARITY (DEFERRED SEAM, R4 / D6): the `xev` event-loop backend surface —
//! libxev's future home.
//!
//! Every termio reference file (and, with the core tier, renderer/Thread.zig +
//! core Surface.zig) imports `const xev = @import("../global.zig").xev` — the
//! libxev async event-loop backend (`@import("xev").Dynamic`). Per the
//! integration-tier blueprint's deferred decision point D6, libxev is Zig
//! source (not C) and will be ported faithfully INTO THIS CRATE (vendored
//! Zig-source precedent: rustty-z2d) when a real "run a shell end-to-end"
//! milestone forces it; today `rustty-global` carries the package as the
//! `seam::Xev` (= `()`) placeholder, exactly as `global.zig`'s `pub const xev`
//! is a deferred seam.
//!
//! The consumers need the individual handle TYPES (`Async`/`Loop`/
//! `Completion`/`Timer`/...) for faithful field and signature shapes, so they
//! are declared here as SHARED placeholders (hoisted from the termio-local
//! `xev_seam` module with core chunk C0 — CORE_BLUEPRINT §5 "xev handle
//! unification": core hands `render_thread.wakeup`, an `xev.Async`, to
//! `termio::Options.renderer_wakeup`, so the type must be one crate both
//! sides name; precedent: rustty-os cf_release_thread's `xev_seam` module).
//! The method bodies are intentionally `unimplemented!()` markers: they are
//! *dependency* placeholders, to be replaced by the real libxev port — no
//! reference logic is stubbed or weakened behind them. When the port lands the
//! types are filled IN PLACE, so downstream `use` paths never change. See
//! .port/phase-2/DEFERRALS.md ("rustty-termio stage 2").

// PORT: `rustty_global::seam::Xev` (= `()`), the deferred xev backend package.
#[allow(unused_imports)]
use rustty_global::xev as _DeferredXevBackend;

// XEV_BLUEPRINT §8 chunk C1: the pure intrusive data structures that establish the
// raw-pointer discipline for the whole libxev port. They are INTERNAL modules for
// now (queue.zig / queue_mpsc.zig / heap.zig ported byte-faithfully); the kqueue
// loop (C3) uses them (`submissions`/`thread_pool_completions`/`TimerHeap`), and C7
// wires the package re-export surface. `allow(dead_code)`: they are sequenced ahead
// of their first non-test consumer (C3), so the lib-only build sees them as unused.
#[allow(dead_code)]
mod heap;
#[allow(dead_code)]
mod queue;
#[allow(dead_code)]
mod queue_mpsc;

// XEV_BLUEPRINT §3 chunk C2: the darwin/syscall layer (`darwin.zig` ported
// byte-faithfully) — the mach-port / `mach_msg` bindings + `KernE`/`MachMsgE`
// the kqueue `Wakeup` (C3) and `Async`(mach) (C4) ride. macOS/kqueue-only, so
// `#[cfg(target_os = "macos")]`. `allow(dead_code)`: sequenced ahead of its
// first consumer (C3/C4), so the lib-only build sees it as unused. The kqueue
// syscall consts (`EVFILT_*`/`EV_*`/`NOTE_*`) + `kevent64`/`kevent64_s` come
// from `libc` directly ([ADV-FIX F4]); this module binds only the mach APIs
// libc lacks.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod darwin;

/// PORT: `xev.Loop` (the per-thread event loop; termio Thread owns one and
/// `Termio.ThreadData.loop` points at it).
pub struct Loop {}

/// PORT: `xev.Async` (a cross-thread wakeup handle).
//
// PORT NOTE: `Clone`/`Copy` — the reference copies the handle by value
// (Surface.zig `.renderer_wakeup = render_thread.wakeup`; Zig struct
// assignment is a copy). A real libxev Async is a small fd/kqueue/eventfd
// handle struct for which the by-value copy is exactly the reference
// semantics, so the derive is kept when the port fills this in.
#[derive(Clone, Copy)]
pub struct Async {}

/// PORT: `xev.Completion` (default-initialized `.{}` in the reference).
#[derive(Default)]
pub struct Completion {}

/// PORT: `xev.Timer` (the coalesce/sync-reset/selection-scroll timers).
pub struct Timer {}

/// PORT: the error set of `Loop.init`/`Async.init`/`Timer.init`.
#[derive(Debug)]
pub struct Error;

/// PORT: `xev.Async.NotifyError` (the error set of `Async.notify`).
//
// PORT NOTE: the extra derives are required by rustty-core's `surface::Error`
// union (core C7), which embeds this error and itself derives
// `Copy`/`PartialEq` (Zig error sets are trivially copyable/comparable
// values, so the derives are faithful and survive the real libxev fill).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyError;

impl Loop {
    // PORT: `xev.Loop.init(.{})`.
    pub fn init() -> Result<Loop, Error> {
        unimplemented!("xev backend deferred seam (R4/D6): rustty-xev tier")
    }
    pub fn deinit(&mut self) {
        unimplemented!("xev backend deferred seam (R4/D6): rustty-xev tier")
    }
}

impl Async {
    pub fn init() -> Result<Async, Error> {
        unimplemented!("xev backend deferred seam (R4/D6): rustty-xev tier")
    }
    pub fn deinit(&mut self) {
        unimplemented!("xev backend deferred seam (R4/D6): rustty-xev tier")
    }
    pub fn notify(&mut self) -> Result<(), NotifyError> {
        unimplemented!("xev backend deferred seam (R4/D6): rustty-xev tier")
    }
}

impl Timer {
    pub fn init() -> Result<Timer, Error> {
        unimplemented!("xev backend deferred seam (R4/D6): rustty-xev tier")
    }
    pub fn deinit(&mut self) {
        unimplemented!("xev backend deferred seam (R4/D6): rustty-xev tier")
    }
}

/// PORT: `xev.Stream` (the pty write stream — `xev.Stream.initFd(pty_fds.
/// write)` in Exec's threadEnter; stage 4).
pub struct Stream {}

impl Stream {
    pub fn deinit(&mut self) {
        unimplemented!("xev backend deferred seam (R4/D6): rustty-xev tier")
    }
}

/// PORT: `xev.Process` (the subprocess exit watcher — `xev.Process.init(pid)`
/// in Exec's threadEnter; stage 4).
pub struct Process {}

impl Process {
    pub fn deinit(&mut self) {
        unimplemented!("xev backend deferred seam (R4/D6): rustty-xev tier")
    }
}

/// PORT: `xev.WriteRequest` (the pooled stream-write request — Exec's
/// `write_req_pool: SegmentedPool(xev.WriteRequest, ...)`; default-initialized
/// by the pool).
#[derive(Default)]
pub struct WriteRequest {}

/// PORT: `xev.WriteQueue` (default-initialized `.{}` in the reference — Exec's
/// `write_queue` field).
#[derive(Default)]
pub struct WriteQueue {}
