//! The termio backend unions: the kinds of backends and their delegating
//! `Backend`/`ThreadData` wrappers.
//!
//! PORTS: ghostty src/termio/backend.zig (0 `test{}` blocks).
//!
//! PORT STATUS: ported (integration-tier stages 2+4). Stage 4 replaced the
//! stage-2 uninhabited `Exec` placeholders with the real `termio.Exec` types
//! (src/termio/Exec.zig — crate::exec), making the delegation arms live; the
//! Exec methods that are themselves D6-deferred (threadEnter/threadExit/
//! focusGained/queueWrite) delegate into their documented deferral markers.

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

use crate::exec;
use crate::termio_impl::{self, Termio};
use crate::Error;

// The preallocation size for the write request pool. This should be big
// enough to satisfy most write requests. It must be a power of 2.
//
// PORT NOTE: reference `const WRITE_REQ_PREALLOC = std.math.pow(usize, 2, 5);`
// (file-private and unreferenced in backend.zig; since e0ef934f7 replaced
// Exec.zig's SegmentedPools with `std.heap.MemoryPool`, this orphaned const is
// the only surviving mention of the prealloc size).
#[allow(dead_code)]
pub(crate) const WRITE_REQ_PREALLOC: usize = usize::pow(2, 5);

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
    // exec: termio.Exec.Config,
    Exec(exec::Config),
}

/// Backend implementations. A backend is responsible for owning the pty
/// behavior and providing read/write capabilities.
///
/// `Backend` has no `childExitedAbnormally`: as in the reference, where the
/// method delegates to one `termio.Exec` does not have, calling it does not
/// compile (S212-F3).
///
/// ```compile_fail,E0599
/// fn call(b: &mut rustty_termio::Backend) {
///     let _ = b.childExitedAbnormally(todo!(), todo!(), 1u32, 0u64);
/// }
/// ```
//
// PORT NOTE: reference `pub const Backend = union(Kind)`. Each method is the
// reference's exhaustive `switch (self.*)` delegation.
pub enum Backend {
    // exec: termio.Exec,
    Exec(exec::Exec),
}

impl Backend {
    pub fn deinit(&mut self) {
        match self {
            // .exec => |*exec| exec.deinit(),
            Backend::Exec(exec) => exec.deinit(),
        }
    }

    pub fn initTerminal(&mut self, t: &mut Terminal) {
        match self {
            // .exec => |*exec| exec.initTerminal(t),
            Backend::Exec(exec) => exec.initTerminal(t),
        }
    }

    // PORT NOTE ([ADV-FIX F2] raw-pointer discipline, xev Milestone B): the
    // reference `io` is `*termio.Termio` (a RAW pointer). The port keeps it raw
    // because `Exec.threadEnter` hands `io` to the spawned read thread as a
    // `*mut Termio<M>` (a `&mut` would be a protected `Unique` retag that aliases
    // the concurrent read thread), matching the reference `threadMainPosix(fd,
    // io: *termio.Termio, quit)`. `td` stays `&mut` for Milestone B (the
    // frame-pinned-local raw-threading of `td` lands with `Thread::threadMain` in
    // B4, per the blueprint).
    ///
    /// # Safety
    /// `io` must be a valid `Termio<M>` pointer that outlives the read thread.
    pub unsafe fn threadEnter<M: AppMailbox + 'static>(
        &mut self,
        alloc: Allocator,
        io: *mut Termio<M>,
        td: &mut termio_impl::ThreadData<M>,
    ) -> Result<(), Error> {
        match self {
            // .exec => |*exec| try exec.threadEnter(alloc, io, td),
            Backend::Exec(exec) => unsafe { exec.threadEnter(alloc, io, td) },
        }
    }

    pub fn threadExit<M: AppMailbox>(&mut self, td: &mut termio_impl::ThreadData<M>) {
        match self {
            // .exec => |*exec| exec.threadExit(td),
            Backend::Exec(exec) => exec.threadExit(td),
        }
    }

    pub fn focusGained<M: AppMailbox + 'static>(
        &mut self,
        td: &mut termio_impl::ThreadData<M>,
        focused: bool,
    ) -> Result<(), Error> {
        match self {
            // .exec => |*exec| try exec.focusGained(td, focused),
            Backend::Exec(exec) => exec.focusGained(td, focused),
        }
    }

    pub fn resize(&mut self, grid_size: GridSize, screen_size: ScreenSize) -> Result<(), Error> {
        match self {
            // .exec => |*exec| try exec.resize(grid_size, screen_size),
            Backend::Exec(exec) => exec.resize(grid_size, screen_size),
        }
    }

    pub fn queueWrite<M: AppMailbox>(
        &mut self,
        alloc: Allocator,
        td: &mut termio_impl::ThreadData<M>,
        data: &[u8],
        linefeed: bool,
    ) -> Result<(), Error> {
        match self {
            // .exec => |*exec| try exec.queueWrite(alloc, td, data, linefeed),
            Backend::Exec(exec) => exec.queueWrite(alloc, td, data, linefeed),
        }
    }

    // PORT NOTE (Zig lazy compilation, S212-F3): the reference delegates to
    // `exec.childExitedAbnormally(...)`, a method `termio.Exec` does NOT
    // declare at this reference version; `Backend.childExitedAbnormally` has
    // no callers, so Zig never analyses it, and any caller would be a compile
    // error. The port therefore has NO such method, which gives a Rust caller
    // the same compile error (see the `compile_fail` example on [`Backend`]).
    // CORRECTION (S212-F3): it used to be a public method whose body was
    // `unimplemented!()`, which compiled and panicked at run time. The
    // reference is UPSTREAM-DEFECTS.md UD206 (latent). Preserved verbatim:
    //
    // pub fn childExitedAbnormally(
    //     self: *Backend,
    //     gpa: Allocator,
    //     t: *terminal.Terminal,
    //     exit_code: u32,
    //     runtime_ms: u64,
    // ) !void {
    //     switch (self.*) {
    //         .exec => |*exec| try exec.childExitedAbnormally(
    //             gpa,
    //             t,
    //             exit_code,
    //             runtime_ms,
    //         ),
    //     }
    // }

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
            Backend::Exec(exec) => exec.getForegroundPid(),
        }
    }

    /// See [`Backend::getForegroundPid`] (the `.tty_name` half of the
    /// reference `getProcessInfo`).
    pub fn getTtyName(&mut self) -> Option<&std::ffi::CStr> {
        match self {
            // .exec => |*exec| exec.getProcessInfo(info),
            Backend::Exec(exec) => exec.getTtyName(),
        }
    }
}

/// Termio thread data. See termio.ThreadData for docs.
//
// PORT NOTE: reference `pub const ThreadData = union(Kind)`.
pub enum ThreadData {
    // exec: termio.Exec.ThreadData,
    Exec(exec::ThreadData),
}

impl ThreadData {
    /// PORT NOTE (Rust-only glue, xev B4): the backend-agnostic `undefined`
    /// placeholder `Termio.threadEnter` writes into `ThreadData.backend` before
    /// `self.backend.threadEnter` overwrites it. Since `Backend`/`ThreadData` have
    /// the single `Exec` variant, the only inhabited "undefined" value is
    /// `Exec(placeholder)` — see `exec::ThreadData::placeholder`.
    #[cfg(not(windows))]
    pub(crate) fn placeholder() -> ThreadData {
        ThreadData::Exec(exec::ThreadData::placeholder())
    }

    pub fn deinit(&mut self, alloc: Allocator) {
        match self {
            // .exec => |*exec| exec.deinit(alloc),
            ThreadData::Exec(exec) => exec.deinit(alloc),
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
