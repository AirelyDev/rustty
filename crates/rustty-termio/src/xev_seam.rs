//! PARITY (DEFERRED SEAM, R4 / D6): the `xev` event-loop backend surface used
//! by termio.
//!
//! Every termio reference file imports `const xev =
//! @import("../global.zig").xev` — the libxev async event-loop backend
//! (`@import("xev").Dynamic`). Per the integration-tier blueprint's deferred
//! decision point D6, libxev is Zig source (not C) and will be ported
//! faithfully as its own low crate (`rustty-xev`, precedent: rustty-z2d) when a
//! real "run a shell end-to-end" milestone forces it; today `rustty-global`
//! carries the package as the `seam::Xev` (= `()`) placeholder, exactly as
//! `global.zig`'s `pub const xev` is a deferred seam.
//!
//! termio needs the individual handle TYPES (`Async`/`Loop`/`Completion`/
//! `Timer`) for faithful field and signature shapes, so they are declared here
//! as termio-local placeholders (precedent: rustty-os cf_release_thread's
//! `xev_seam` module). The method bodies are intentionally
//! `unimplemented!()` markers: they are *dependency* placeholders, to be
//! replaced by the real libxev port — no termio reference logic is stubbed or
//! weakened behind them. See .port/phase-2/DEFERRALS.md ("rustty-termio stage 2").

// PORT: `rustty_global::seam::Xev` (= `()`), the deferred xev backend package.
#[allow(unused_imports)]
use rustty_global::xev as _DeferredXevBackend;

/// PORT: `xev.Loop` (the per-thread event loop; termio Thread owns one and
/// `Termio.ThreadData.loop` points at it).
pub struct Loop {}

/// PORT: `xev.Async` (a cross-thread wakeup handle).
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
#[derive(Debug)]
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
