//! The termio backend unions: the kinds of backends and their delegating
//! `Backend`/`ThreadData` wrappers.
//!
//! PORTS: ghostty src/termio/backend.zig (0 `test{}` blocks).
//!
//! PORT STATUS: struct shapes ported (integration-tier stage 2). The one
//! backend implementation, `termio.Exec` (src/termio/Exec.zig, 10 tests), is
//! stage 4 of the integration-tier blueprint; until it lands the `exec`
//! payload types are uninhabited placeholders (precedent: rustty-renderer
//! `state::Inspector`), which makes every delegation arm below statically
//! unreachable — the verbatim delegation is preserved in comments and becomes
//! live when Exec lands.

// PARITY: the reference's identifiers are camelCase (`initTerminal`,
// `threadEnter`, `queueWrite`, ...); preserved verbatim per R1/R2.
#![allow(non_snake_case)]

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const posix = std.posix;
// const renderer = @import("../renderer.zig");
// const terminal = @import("../terminal/main.zig");
// const termio = @import("../termio.zig");
// const ProcessInfo = @import("../pty.zig").ProcessInfo;
//
// PORT NOTE: `Allocator` mirrors rustty-vt's Terminal convention
// (`allocator_api2::alloc::Global`); `ProcessInfo` is the already-ported
// rustty-os `pty::ProcessInfo`.
use allocator_api2::alloc::Global as Allocator;
use rustty_apprt_api::surface::AppMailbox;
use rustty_renderer::size::{GridSize, ScreenSize};
use rustty_vt::terminal::Terminal;

use crate::termio_impl::{self, Termio};
use crate::Error;

// The preallocation size for the write request pool. This should be big
// enough to satisfy most write requests. It must be a power of 2.
//
// PORT NOTE: reference `const WRITE_REQ_PREALLOC = std.math.pow(usize, 2, 5);`
// (file-private; consumed by Exec's write pool — stage 4).
#[allow(dead_code)]
pub(crate) const WRITE_REQ_PREALLOC: usize = usize::pow(2, 5);

// PORT NOTE (DEFERRED, integration-tier stage 4): the reference `exec` union
// payloads are `termio.Exec` / `termio.Exec.Config` / `termio.Exec.ThreadData`
// (src/termio/Exec.zig — the posix exec/pty backend, ported in the "termio
// testable logic" stage). These uninhabited placeholders keep the union shapes
// faithful without fabricating Exec; they are REPLACED by the real types when
// Exec lands (DEFERRALS.md).
pub enum Exec {}
/// Placeholder for the reference `termio.Exec.Config`.
pub enum ExecConfig {}
/// Placeholder for the reference `termio.Exec.ThreadData`.
pub enum ExecThreadData {}

/// The kinds of backends.
//
// PORT NOTE: reference `pub const Kind = enum { exec };`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Exec,
}

/// Configuration for the various backend types.
//
// PORT NOTE: reference `pub const Config = union(Kind)`.
pub enum Config {
    /// Exec uses posix exec to run a command with a pty.
    Exec(ExecConfig),
}

/// Backend implementations. A backend is responsible for owning the pty
/// behavior and providing read/write capabilities.
//
// PORT NOTE: reference `pub const Backend = union(Kind)`. Each method is the
// reference's exhaustive `switch (self.*)` delegation; with the stage-4 `Exec`
// placeholder uninhabited, each arm is expressed as the (statically
// unreachable) empty match on the payload, with the delegation call preserved
// verbatim in the comment above it.
pub enum Backend {
    Exec(Exec),
}

impl Backend {
    pub fn deinit(&mut self) {
        match self {
            // .exec => |*exec| exec.deinit(),
            Backend::Exec(exec) => match *exec {},
        }
    }

    pub fn initTerminal(&mut self, t: &mut Terminal) {
        let _ = t;
        match self {
            // .exec => |*exec| exec.initTerminal(t),
            Backend::Exec(exec) => match *exec {},
        }
    }

    pub fn threadEnter<M: AppMailbox>(
        &mut self,
        alloc: Allocator,
        io: &mut Termio<M>,
        td: &mut termio_impl::ThreadData<M>,
    ) -> Result<(), Error> {
        let _ = (alloc, io, td);
        match self {
            // .exec => |*exec| try exec.threadEnter(alloc, io, td),
            Backend::Exec(exec) => match *exec {},
        }
    }

    pub fn threadExit<M: AppMailbox>(&mut self, td: &mut termio_impl::ThreadData<M>) {
        let _ = td;
        match self {
            // .exec => |*exec| exec.threadExit(td),
            Backend::Exec(exec) => match *exec {},
        }
    }

    pub fn focusGained<M: AppMailbox>(
        &mut self,
        td: &mut termio_impl::ThreadData<M>,
        focused: bool,
    ) -> Result<(), Error> {
        let _ = (td, focused);
        match self {
            // .exec => |*exec| try exec.focusGained(td, focused),
            Backend::Exec(exec) => match *exec {},
        }
    }

    pub fn resize(&mut self, grid_size: GridSize, screen_size: ScreenSize) -> Result<(), Error> {
        let _ = (grid_size, screen_size);
        match self {
            // .exec => |*exec| try exec.resize(grid_size, screen_size),
            Backend::Exec(exec) => match *exec {},
        }
    }

    pub fn queueWrite<M: AppMailbox>(
        &mut self,
        alloc: Allocator,
        td: &mut termio_impl::ThreadData<M>,
        data: &[u8],
        linefeed: bool,
    ) -> Result<(), Error> {
        let _ = (alloc, td, data, linefeed);
        match self {
            // .exec => |*exec| try exec.queueWrite(alloc, td, data, linefeed),
            Backend::Exec(exec) => match *exec {},
        }
    }

    pub fn childExitedAbnormally(
        &mut self,
        gpa: Allocator,
        t: &mut Terminal,
        exit_code: u32,
        runtime_ms: u64,
    ) -> Result<(), Error> {
        let _ = (gpa, t, exit_code, runtime_ms);
        match self {
            // .exec => |*exec| try exec.childExitedAbnormally(
            //     gpa,
            //     t,
            //     exit_code,
            //     runtime_ms,
            // ),
            Backend::Exec(exec) => match *exec {},
        }
    }

    /// Get information about the process(es) attached to the backend. Returns
    /// `null` if there was an error getting the information or the information
    /// is not available on a particular platform.
    //
    // PORT NOTE: reference `pub fn getProcessInfo(self: *Backend, comptime
    // info: ProcessInfo) ?ProcessInfo.Type(info)` — Rust has no comptime
    // type-from-value, so the comptime-dispatched accessor splits into the two
    // concrete accessors below (`.foreground_pid` -> `u64`, `.tty_name` ->
    // `[:0]const u8` -> `&CStr`), exactly mirroring the rustty-os `pty.rs`
    // split of the same reference pattern.
    pub fn getForegroundPid(&mut self) -> Option<u64> {
        match self {
            // .exec => |*exec| exec.getProcessInfo(info),
            Backend::Exec(exec) => match *exec {},
        }
    }

    /// See [`Backend::getForegroundPid`] (the `.tty_name` half of the
    /// reference `getProcessInfo`).
    pub fn getTtyName(&mut self) -> Option<&std::ffi::CStr> {
        match self {
            // .exec => |*exec| exec.getProcessInfo(info),
            Backend::Exec(exec) => match *exec {},
        }
    }
}

/// Termio thread data. See termio.ThreadData for docs.
//
// PORT NOTE: reference `pub const ThreadData = union(Kind)`.
pub enum ThreadData {
    Exec(ExecThreadData),
}

impl ThreadData {
    pub fn deinit(&mut self, alloc: Allocator) {
        let _ = alloc;
        match self {
            // .exec => |*exec| exec.deinit(alloc),
            ThreadData::Exec(exec) => match *exec {},
        }
    }

    // pub fn changeConfig(self: *ThreadData, config: *termio.DerivedConfig) void {
    //     _ = self;
    //     _ = config;
    // }
    //
    // PORT NOTE: the reference body IS a no-op (`_ = self; _ = config;`);
    // ported faithfully.
    pub fn changeConfig(&mut self, config: &mut termio_impl::DerivedConfig) {
        let _ = self;
        let _ = config;
    }
}
