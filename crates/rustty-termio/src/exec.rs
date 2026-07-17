//! Exec implements the logic for starting and stopping a subprocess with a
//! pty as well as spinning up the necessary read thread to read from the
//! pty and forward it to the Termio instance.
//!
//! PORTS: ghostty src/termio/Exec.zig (10 `test{}` blocks — all ported below;
//! the 2 darwin-only and 4 windows-only tests keep the reference's
//! SkipZigTest cfg-gating, F3 DirectWrite / stage-3 Command precedent).
//!
//! PORT STATUS: ported (integration-tier stage 4) EXCEPT the xev event-loop
//! wiring (threadEnter/threadExit/focusGained/processExit*/termiosTimer/
//! queueWrite/ttyWrite/ReadThread — the D6 deferral seam, 0 reference tests)
//! and Subprocess.start (the fork/exec spawn whose stored `fork_exec: Command`
//! aliases the subprocess's own arena-owned args/env — see its PORT NOTE);
//! each deferred body is preserved verbatim (DEFERRALS.md, R5: no silent
//! drops). The testable core — Subprocess.init's argv+env assembly and
//! execCommand — is LIVE.

// PARITY: the reference's identifiers are camelCase (`initTerminal`,
// `execCommand`, `killPid`, ...); preserved verbatim per R1/R2.
#![allow(non_snake_case)]

// const Exec = @This();
//
// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const posix = std.posix;
// const xev = @import("../global.zig").xev;
// const apprt = @import("../apprt.zig");
// const build_config = @import("../build_config.zig");
// const configpkg = @import("../config.zig");
// const crash = @import("../crash/main.zig");
// const fastmem = @import("../fastmem.zig");
// const internal_os = @import("../os/main.zig");
// const renderer = @import("../renderer.zig");
// const shell_integration = @import("shell_integration.zig");
// const terminal = @import("../terminal/main.zig");
// const termio = @import("../termio.zig");
// const Command = @import("../Command.zig");
// const SegmentedPool = @import("../datastruct/main.zig").SegmentedPool;
// const ptypkg = @import("../pty.zig");
// const Pty = ptypkg.Pty;
// const EnvMap = std.process.EnvMap;
// const PasswdEntry = internal_os.passwd.Entry;
// const windows = internal_os.windows;
// const ProcessInfo = @import("../pty.zig").ProcessInfo;
//
// PORT NOTE: `xev` -> the crate::xev_seam D6 placeholder types; `apprt` is
// reached only through the `Command.RtPreExecInfo/RtPostForkInfo` fallbacks
// (crate::command, blueprint §1 — no apprt-api edge here); `crash`/`fastmem`
// are used only inside the D6-deferred ReadThread/queueWrite bodies (verbatim
// comments below), so no `use` is bound for them; `Command` is the RELOCATED
// crate::command (break #13b); `EnvMap` is the crate's stage-3
// `std.process.EnvMap` modeling.
use allocator_api2::alloc::Global as Allocator;
use rustty_config as configpkg;
use rustty_datastruct::SegmentedPool;
use rustty_os::pty as ptypkg;
use rustty_os::pty::Pty;
use rustty_renderer::size::{GridSize, ScreenSize};
use rustty_vt::terminal::Terminal;
use std::ffi::{CStr, CString};

// [ADV-FIX F3] the pty.slave File is wrapped NON-owningly so `Command` (which
// reads only `.as_raw_fd()`) never closes it; the single verbatim
// `posix.close(pty.slave)` in `Subprocess::start` does the one close. Posix-only
// (the slave-File spawn path).
#[cfg(not(windows))]
use std::fs::File;
#[cfg(not(windows))]
use std::mem::ManuallyDrop;
#[cfg(not(windows))]
use std::os::fd::FromRawFd as _;

use crate::command::{Command, EnvMap, RtPostForkInfo, RtPreExecInfo};
use crate::shell_integration::{self, env_name, env_string};
use crate::xev_seam as xev;
// The surface-mailbox messages the xev exit / termios callbacks push
// (`.child_exited` / `.password_input`), and the blocking-queue timeout.
use rustty_apprt_api::surface::{ChildExited, Message as SurfaceMessage};
use rustty_datastruct::blocking_queue::Timeout;

// const log = std.log.scoped(.io_exec);
//
// PORT NOTE: the reference's log call sites are preserved verbatim as comments at
// each site (termio stage-2 precedent).

/// The termios poll rate in milliseconds.
//
// (Consumed by the D6-deferred threadEnter/termiosTimer bodies below.)
#[allow(dead_code)]
const TERMIOS_POLL_MS: u64 = 200;

/// If we build with flatpak support then we have to keep track of
/// a potential execution on the host.
//
// const FlatpakHostCommand = if (!build_config.flatpak) struct {
//     pub const Completion = struct {};
// } else internal_os.FlatpakHostCommand;
//
// PORT NOTE: `build_config.flatpak` is const `false` in this build
// (rustty-build-config), so the comptime selection resolves to the zero-size
// fallback here; the real `internal_os.FlatpakHostCommand` (Linux flatpak DBus
// spawn) re-binds when that tier lands (DEFERRALS.md). The flatpak-gated
// branches below are `comptime`-false in the reference and preserved verbatim
// as comments.
pub struct FlatpakHostCommand {}

/// `FlatpakHostCommand.Completion` (nested type, hoisted to module level —
/// zero-size in the non-flatpak build).
#[derive(Default)]
pub struct FlatpakHostCommandCompletion {}

/// PORT NOTE (Rust-only glue): the reference `init` error set is inferred
/// (`!Exec`): shell_integration's error (the bash HISTFILE homedir path) +
/// execCommand's `error.SystemError`; the allocation half collapses.
#[derive(Debug, PartialEq, Eq)]
pub enum InitError {
    ShellIntegration(shell_integration::Error),
    /// `error.SystemError` (execCommand).
    SystemError,
}

/// PORT NOTE (Rust-only glue): the `(Allocator.Error || error{SystemError})`
/// set of `execCommand` — the `Allocator.Error` half collapses with Rust's
/// infallible-allocation model (its fallback arm is preserved verbatim at the
/// call site).
#[derive(Debug, PartialEq, Eq)]
pub enum ExecCommandError {
    SystemError,
}

/// The subprocess state for our exec backend.
pub struct Exec {
    // subprocess: Subprocess,
    pub subprocess: Subprocess,
}

impl Exec {
    /// Initialize the exec state. This will NOT start it, this only sets
    /// up the internal state necessary to start it later.
    //
    // pub fn init(
    //     alloc: Allocator,
    //     cfg: Config,
    // ) !Exec {
    pub fn init(alloc: Allocator, cfg: Config) -> Result<Exec, InitError> {
        // var subprocess = try Subprocess.init(alloc, cfg);
        // errdefer subprocess.deinit();
        // (PORT NOTE: on the error path the partially-built Subprocess drops.)
        let subprocess = Subprocess::init(alloc, cfg)?;

        // return .{ .subprocess = subprocess };
        Ok(Exec { subprocess })
    }

    // pub fn deinit(self: *Exec) void {
    pub fn deinit(&mut self) {
        self.subprocess.deinit();
    }

    /// Call to initialize the terminal state as necessary for this backend.
    /// This is called before any termio begins. This should not be called
    /// after termio begins because it may put the internal terminal state
    /// into a bad state.
    //
    // pub fn initTerminal(self: *Exec, term: *terminal.Terminal) void {
    pub fn initTerminal(&mut self, term: &mut Terminal) {
        // If we have an initial pwd requested by the subprocess, then we
        // set that on the terminal now. This allows rapidly initializing
        // new surfaces to use the proper pwd.
        // if (self.subprocess.cwd) |cwd| term.setPwd(cwd) catch |err| {
        //     log.warn("error setting initial pwd err={}", .{err});
        // };
        if let Some(cwd) = &self.subprocess.cwd {
            if term.set_pwd(cwd.to_bytes()).is_err() {
                // log.warn("error setting initial pwd err={}", .{err});
            }
        }

        // Setup our initial grid/screen size from the terminal. This
        // can't fail because the pty should not exist at this point.
        // self.resize(.{
        //     .columns = term.cols,
        //     .rows = term.rows,
        // }, .{
        //     .width = term.width_px,
        //     .height = term.height_px,
        // }) catch unreachable;
        self.resize(
            GridSize {
                columns: term.cols,
                rows: term.rows,
            },
            ScreenSize {
                width: term.width_px,
                height: term.height_px,
            },
        )
        .unwrap();
    }

    // DEFERRED (D6 xev event-loop wiring, 0 tests — R1-safe): the body starts
    // the subprocess and wires the xev process watcher / write stream /
    // termios timer / read thread. Preserved verbatim:
    //
    // pub fn threadEnter(
    //     self: *Exec,
    //     alloc: Allocator,
    //     io: *termio.Termio,
    //     td: *termio.Termio.ThreadData,
    // ) !void {
    //     // Start our subprocess
    //     const pty_fds = self.subprocess.start(alloc) catch |err| {
    //         // If we specifically got this error then we are in the forked
    //         // process and our child failed to execute. If we DIDN'T
    //         // get this specific error then we're in the parent and
    //         // we need to bubble it up.
    //         if (err != error.ExecFailedInChild) return err;
    //
    //         // We're in the child. Nothing more we can do but abnormal exit.
    //         // The Command will output some additional information.
    //         posix.exit(1);
    //     };
    //     errdefer self.subprocess.stop();
    //
    //     // Watcher to detect subprocess exit
    //     var process: ?xev.Process = if (self.subprocess.process) |v| switch (v) {
    //         .fork_exec => |cmd| try xev.Process.init(
    //             cmd.pid orelse return error.ProcessNoPid,
    //         ),
    //
    //         // If we're executing via Flatpak then we can't do
    //         // traditional process watching (its implemented
    //         // as a special case in os/flatpak.zig) since the
    //         // command is on the host.
    //         .flatpak => null,
    //     } else return error.ProcessNotStarted;
    //     errdefer if (process) |*p| p.deinit();
    //
    //     // Track our process start time for abnormal exits
    //     const process_start = try std.time.Instant.now();
    //
    //     // Create our pipe that we'll use to kill our read thread.
    //     // pipe[0] is the read end, pipe[1] is the write end.
    //     const pipe = try internal_os.pipe();
    //     errdefer posix.close(pipe[0]);
    //     errdefer posix.close(pipe[1]);
    //
    //     // Setup our stream so that we can write.
    //     var stream = xev.Stream.initFd(pty_fds.write);
    //     errdefer stream.deinit();
    //
    //     // Start our timer to read termios state changes. This is used
    //     // to detect things such as when password input is being done
    //     // so we can render the terminal in a different way.
    //     var termios_timer = try xev.Timer.init();
    //     errdefer termios_timer.deinit();
    //
    //     // Start our read thread
    //     const read_thread = try std.Thread.spawn(
    //         .{},
    //         if (builtin.os.tag == .windows) ReadThread.threadMainWindows else ReadThread.threadMainPosix,
    //         .{ pty_fds.read, io, pipe[0] },
    //     );
    //     read_thread.setName("io-reader") catch {};
    //
    //     // Setup our threadata backend state to be our own
    //     td.backend = .{ .exec = .{
    //         .start = process_start,
    //         .write_stream = stream,
    //         .process = process,
    //         .read_thread = read_thread,
    //         .read_thread_pipe = pipe[1],
    //         .read_thread_fd = pty_fds.read,
    //         .termios_timer = termios_timer,
    //     } };
    //
    //     // Start our process watcher. If we have an xev.Process use it.
    //     if (process) |*p| p.wait(
    //         td.loop,
    //         &td.backend.exec.process_wait_c,
    //         termio.Termio.ThreadData,
    //         td,
    //         processExit,
    //     ) else if (comptime build_config.flatpak) flatpak: {
    //         switch (self.subprocess.process orelse break :flatpak) {
    //             // If we're in flatpak and we have a flatpak command
    //             // then we can run the special flatpak logic for watching.
    //             .flatpak => |*c| c.waitXev(
    //                 td.loop,
    //                 &td.backend.exec.flatpak_wait_c,
    //                 termio.Termio.ThreadData,
    //                 td,
    //                 flatpakExit,
    //             ),
    //
    //             .fork_exec => {},
    //         }
    //     }
    //
    //     // Start our termios timer. We don't support this on Windows.
    //     // Fundamentally, we could support this on Windows so we're just
    //     // waiting for someone to implement it.
    //     if (comptime builtin.os.tag != .windows) {
    //         termios_timer.run(
    //             td.loop,
    //             &td.backend.exec.termios_timer_c,
    //             TERMIOS_POLL_MS,
    //             termio.Termio.ThreadData,
    //             td,
    //             termiosTimer,
    //         );
    //     }
    // }
    //
    // PORT NOTE ([ADV-FIX F2], xev Milestone B): `io` is a RAW `*mut Termio<M>`
    // (the reference `io: *termio.Termio`) — it is handed to the spawned read
    // thread, which aliases `*io` concurrently under the renderer mutex; a `&mut`
    // would be a whole-call protected `Unique` retag. `td` stays `&mut` for
    // Milestone B; a raw `td_ptr` is derived once for the loop-stored completion
    // pointers + userdata (the loop re-enters them across ticks — never a
    // long-lived `&mut`). The full frame-pinned-local raw threading of `td` lands
    // with `Thread::threadMain` in B4.
    ///
    /// # Safety
    /// `io` must be a valid `Termio<M>` pointer that outlives the read thread;
    /// `td` must be frame-pinned for the loop's life (see the blueprint §3).
    pub unsafe fn threadEnter<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        &mut self,
        alloc: Allocator,
        io: *mut crate::Termio<M>,
        td: &mut crate::termio_impl::ThreadData<M>,
    ) -> Result<(), crate::Error> {
        // A raw pointer to the frame-pinned `td` for the loop-stored completion
        // pointers / userdata (never a long-lived `&mut` — the loop re-enters
        // these across ticks; `rust-mut-args-strongly-protected`).
        let td_ptr: *mut crate::termio_impl::ThreadData<M> = td;

        // Start our subprocess
        // const pty_fds = self.subprocess.start(alloc) catch |err| {
        //     // If we specifically got this error then we are in the forked
        //     // process and our child failed to execute. If we DIDN'T
        //     // get this specific error then we're in the parent and
        //     // we need to bubble it up.
        //     if (err != error.ExecFailedInChild) return err;
        //     // We're in the child. Nothing more we can do but abnormal exit.
        //     // The Command will output some additional information.
        //     posix.exit(1);
        // };
        //
        // (PORT NOTE: the `error.ExecFailedInChild` -> `posix.exit(1)` child
        // branch is handled inside `Subprocess::start` — the one place `in_child`
        // is known, since the opaque `crate::Error` cannot carry the discriminant
        // here; see start's PORT NOTE. Every error that reaches here is a parent
        // error and bubbles up.)
        let pty_fds = self.subprocess.start(alloc)?;
        // errdefer self.subprocess.stop();
        // (PORT NOTE: no `errdefer`; each fallible step below runs the accumulated
        // cleanup explicitly, in reverse registration order, before returning.)

        // Watcher to detect subprocess exit
        // var process: ?xev.Process = if (self.subprocess.process) |v| switch (v) {
        //     .fork_exec => |cmd| try xev.Process.init(cmd.pid orelse return error.ProcessNoPid),
        //     .flatpak => null,
        // } else return error.ProcessNotStarted;
        //
        // (PORT NOTE: copy the pid out from behind the `&self.subprocess.process`
        // borrow first, so the error arms can call `&mut self.subprocess.stop()`
        // — the `errdefer self.subprocess.stop()` — without a borrow conflict.)
        enum ProcessWhich {
            NotStarted,
            Flatpak,
            ForkExec(Option<libc::pid_t>),
        }
        let which = match &self.subprocess.process {
            None => ProcessWhich::NotStarted,
            Some(Process::Flatpak(_)) => ProcessWhich::Flatpak,
            Some(Process::ForkExec(cmd)) => ProcessWhich::ForkExec(cmd.pid),
        };
        let mut process: Option<xev::Process> = match which {
            // } else return error.ProcessNotStarted;
            ProcessWhich::NotStarted => {
                self.subprocess.stop();
                return Err(crate::Error);
            }
            // .flatpak => null (traditional process watching not possible)
            ProcessWhich::Flatpak => None,
            // .fork_exec => |cmd| try xev.Process.init(cmd.pid orelse return error.ProcessNoPid)
            ProcessWhich::ForkExec(None) => {
                // return error.ProcessNoPid (errdefer stop fires)
                self.subprocess.stop();
                return Err(crate::Error);
            }
            ProcessWhich::ForkExec(Some(pid)) => match xev::Process::init(pid) {
                Ok(p) => Some(p),
                Err(_) => {
                    self.subprocess.stop();
                    return Err(crate::Error);
                }
            },
        };
        // errdefer if (process) |*p| p.deinit();

        // Track our process start time for abnormal exits
        // const process_start = try std.time.Instant.now();
        let process_start = std::time::Instant::now();

        // Create our pipe that we'll use to kill our read thread.
        // pipe[0] is the read end, pipe[1] is the write end.
        // const pipe = try internal_os.pipe();
        let pipe = match rustty_os::pipe::pipe() {
            Ok(p) => p,
            Err(_) => {
                // errdefer if (process) |*p| p.deinit(); errdefer self.subprocess.stop();
                if let Some(p) = &mut process {
                    p.deinit();
                }
                self.subprocess.stop();
                return Err(crate::Error);
            }
        };
        // errdefer posix.close(pipe[0]);
        // errdefer posix.close(pipe[1]);

        // Setup our stream so that we can write.
        // var stream = xev.Stream.initFd(pty_fds.write);
        let stream = xev::Stream::init_fd(pty_fds.write);
        // errdefer stream.deinit();

        // Start our timer to read termios state changes. This is used
        // to detect things such as when password input is being done
        // so we can render the terminal in a different way.
        // var termios_timer = try xev.Timer.init();
        let termios_timer = match xev::Timer::init() {
            Ok(t) => t,
            Err(_) => {
                // errdefer (reverse): stream.deinit(); close pipe[1]; close pipe[0];
                //          process.deinit(); self.subprocess.stop();
                stream.deinit();
                unsafe {
                    libc::close(pipe[1]);
                    libc::close(pipe[0]);
                }
                if let Some(p) = &mut process {
                    p.deinit();
                }
                self.subprocess.stop();
                return Err(crate::Error);
            }
        };
        // errdefer termios_timer.deinit();

        // Start our read thread
        // const read_thread = try std.Thread.spawn(
        //     .{},
        //     if (builtin.os.tag == .windows) ReadThread.threadMainWindows else ReadThread.threadMainPosix,
        //     .{ pty_fds.read, io, pipe[0] },
        // );
        // read_thread.setName("io-reader") catch {};
        //
        // (PORT NOTE: `io: *mut Termio<M>` is `!Send`, so the spawn captures it behind
        // the `ReadThreadArgs` Send wrapper — the `IoThreadArgs` precedent
        // (surface.rs), whose sole toucher of `*io` for `processOutput` is this
        // read thread, between spawn and `threadExit`'s `join()`; blueprint §3.)
        let read_args = ReadThreadArgs::<M>(pty_fds.read, io, pipe[0]);
        let read_thread = match std::thread::Builder::new()
            .name("io-reader".to_string())
            .spawn(move || {
                let args = read_args;
                // SAFETY: `args.1` (`*mut Termio<M>`) is valid for the read
                // thread's life; this thread is its sole `processOutput` toucher.
                ReadThread::threadMainPosix::<M>(args.0, args.1, args.2);
            }) {
            Ok(t) => t,
            Err(_) => {
                // errdefer (reverse): termios_timer.deinit(); stream.deinit();
                //          close pipe[1]; close pipe[0]; process.deinit();
                //          self.subprocess.stop();
                termios_timer.deinit();
                stream.deinit();
                unsafe {
                    libc::close(pipe[1]);
                    libc::close(pipe[0]);
                }
                if let Some(p) = &mut process {
                    p.deinit();
                }
                self.subprocess.stop();
                return Err(crate::Error);
            }
        };

        // Setup our threadata backend state to be our own
        // td.backend = .{ .exec = .{
        //     .start = process_start,
        //     .write_stream = stream,
        //     .process = process,
        //     .read_thread = read_thread,
        //     .read_thread_pipe = pipe[1],
        //     .read_thread_fd = pty_fds.read,
        //     .termios_timer = termios_timer,
        // } };
        //
        // PORT NOTE: the reference's `td.backend` was `undefined` (the
        // out-parameter protocol — `Termio.threadEnter` builds `ThreadData{ …,
        // .backend = undefined }`, and THIS is the Backend that fills it). Rust
        // has no `undefined`; the caller supplies a placeholder `backend`, and
        // this move-assign drops it (the placeholder is constructed to be
        // trivially droppable). The remaining exec fields take their reference
        // struct defaults (`write_req_pool`/`write_buf_pool` = `.{}` =
        // preallocated pools, `write_queue`/`*_c` = `.{}`, `termios_timer_running`
        // = true, `termios_mode` = `.{}`).
        td.backend = crate::backend::ThreadData::Exec(ThreadData {
            start: process_start,
            exited: false,
            write_stream: stream,
            process,
            write_req_pool: SegmentedPool::new(ThreadData::WRITE_REQ_PREALLOC),
            write_buf_pool: SegmentedPool::new(ThreadData::WRITE_REQ_PREALLOC),
            write_queue: xev::WriteQueue::default(),
            process_wait_c: xev::Completion::default(),
            flatpak_wait_c: FlatpakHostCommandCompletion::default(),
            read_thread,
            read_thread_pipe: pipe[1],
            read_thread_fd: pty_fds.read,
            termios_timer,
            termios_timer_c: xev::Completion::default(),
            termios_timer_running: true,
            termios_mode: ptypkg::Mode::default(),
        });

        // A raw pointer into the now-installed `td.backend.exec` for the
        // loop-stored completion pointers (`&td.backend.exec.*_c`) — stable for
        // the loop's life (frame-pinned `td`).
        // (PORT NOTE: `td.backend.exec` — a single-variant `union(Kind){exec}` field
        // access; the irrefutable `let` binds the sole `Exec` variant.)
        let crate::backend::ThreadData::Exec(exec_ptr) = &mut td.backend;
        let exec_ptr: *mut ThreadData = exec_ptr;

        // Start our process watcher. If we have an xev.Process use it.
        // if (process) |*p| p.wait(
        //     td.loop,
        //     &td.backend.exec.process_wait_c,
        //     termio.Termio.ThreadData,
        //     td,
        //     processExit,
        // ) else if (comptime build_config.flatpak) flatpak: {
        //     switch (self.subprocess.process orelse break :flatpak) {
        //         .flatpak => |*c| c.waitXev(
        //             td.loop, &td.backend.exec.flatpak_wait_c,
        //             termio.Termio.ThreadData, td, flatpakExit,
        //         ),
        //         .fork_exec => {},
        //     }
        // }
        //
        // (PORT NOTE: `build_config.flatpak` is const false — the `else if` flatpak arm
        // + `flatpakExit` are comptime-elided, exactly as Zig's lazy compilation
        // leaves them; verbatim in the reference blocks above.)
        if let Some(p) = process {
            // SAFETY: `td.loop_`/`process_wait_c`/`td_ptr` are all valid, stable
            // pointers held raw by the loop across ticks.
            unsafe {
                p.wait(
                    td.loop_,
                    std::ptr::addr_of_mut!((*exec_ptr).process_wait_c),
                    td_ptr,
                    Self::processExit::<M>,
                );
            }
        }

        // Start our termios timer. We don't support this on Windows.
        // Fundamentally, we could support this on Windows so we're just
        // waiting for someone to implement it.
        // if (comptime builtin.os.tag != .windows) {
        //     termios_timer.run(
        //         td.loop, &td.backend.exec.termios_timer_c, TERMIOS_POLL_MS,
        //         termio.Termio.ThreadData, td, termiosTimer,
        //     );
        // }
        #[cfg(not(windows))]
        unsafe {
            termios_timer.run(
                td.loop_,
                std::ptr::addr_of_mut!((*exec_ptr).termios_timer_c),
                TERMIOS_POLL_MS,
                td_ptr,
                Self::termiosTimer::<M>,
            );
        }

        Ok(())
    }

    // DEFERRED (D6 xev thread lifecycle, 0 tests — R1-safe): the body writes
    // the read-thread quit pipe and joins the read thread (join-by-value —
    // the D6 thread-lifecycle modeling owns it). Preserved verbatim:
    //
    // pub fn threadExit(self: *Exec, td: *termio.Termio.ThreadData) void {
    //     assert(td.backend == .exec);
    //     const exec = &td.backend.exec;
    //
    //     if (exec.exited) self.subprocess.externalExit();
    //     self.subprocess.stop();
    //
    //     // Quit our read thread after exiting the subprocess so that
    //     // we don't get stuck waiting for data to stop flowing if it is
    //     // a particularly noisy process.
    //     _ = posix.write(exec.read_thread_pipe, "x") catch |err| switch (err) {
    //         // BrokenPipe means that our read thread is closed already,
    //         // which is completely fine since that is what we were trying
    //         // to achieve.
    //         error.BrokenPipe => {},
    //
    //         else => log.warn(
    //             "error writing to read thread quit pipe err={}",
    //             .{err},
    //         ),
    //     };
    //
    //     if (comptime builtin.os.tag == .windows) {
    //         // Interrupt the blocking read so the thread can see the quit message
    //         if (windows.kernel32.CancelIoEx(exec.read_thread_fd, null) == 0) {
    //             switch (windows.kernel32.GetLastError()) {
    //                 .NOT_FOUND => {},
    //                 else => |err| log.warn("error interrupting read thread err={}", .{err}),
    //             }
    //         }
    //     }
    //
    //     exec.read_thread.join();
    // }
    pub fn threadExit<M: rustty_apprt_api::surface::AppMailbox>(
        &mut self,
        td: &mut crate::termio_impl::ThreadData<M>,
    ) {
        // assert(td.backend == .exec);
        // const exec = &td.backend.exec;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(exec) = &mut td.backend;

        // if (exec.exited) self.subprocess.externalExit();
        if exec.exited {
            self.subprocess.externalExit();
        }
        // self.subprocess.stop();
        self.subprocess.stop();

        // Quit our read thread after exiting the subprocess so that
        // we don't get stuck waiting for data to stop flowing if it is
        // a particularly noisy process.
        // _ = posix.write(exec.read_thread_pipe, "x") catch |err| switch (err) {
        //     // BrokenPipe means that our read thread is closed already,
        //     // which is completely fine since that is what we were trying
        //     // to achieve.
        //     error.BrokenPipe => {},
        //     else => log.warn("error writing to read thread quit pipe err={}", .{err}),
        // };
        //
        // SAFETY: `read_thread_pipe` is the write end of the quit pipe, valid
        // until `ThreadData::deinit` closes it. A closed read end yields EPIPE
        // (Rust ignores SIGPIPE), which — like the reference's `error.BrokenPipe`
        // — is fine (the read thread is already gone).
        let rc = unsafe {
            libc::write(
                exec.read_thread_pipe,
                b"x".as_ptr().cast::<std::ffi::c_void>(),
                1,
            )
        };
        if rc < 0 {
            // error.BrokenPipe => {} ; else => log.warn(...)
            // (either way nothing else to do here — the read thread is exiting.)
        }

        // if (comptime builtin.os.tag == .windows) {
        //     // Interrupt the blocking read so the thread can see the quit message
        //     if (windows.kernel32.CancelIoEx(exec.read_thread_fd, null) == 0) {
        //         switch (windows.kernel32.GetLastError()) {
        //             .NOT_FOUND => {},
        //             else => |err| log.warn("error interrupting read thread err={}", .{err}),
        //         }
        //     }
        // }
        #[cfg(windows)]
        {
            if unsafe {
                rustty_os::windows::exp::kernel32::CancelIoEx(
                    exec.read_thread_fd,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                // switch (GetLastError()) { .NOT_FOUND => {}, else => log.warn(...) }
            }
        }

        // exec.read_thread.join();
        //
        // (PORT NOTE: the reference `join()` returns `void`; Rust's returns a `Result`
        // that is `Err` if the read thread panicked — the reference ignores it,
        // so we do too. `JoinHandle::join` consumes the handle by value, but
        // `exec` is a `&mut` place, so the handle is moved out via `mem::replace`
        // and the slot is left holding a detached already-finished no-op thread —
        // keeping `exec` fully-initialized for its later `deinit`. `threadExit`
        // is terminal for the ThreadData; nothing reads `read_thread` again.)
        let read_thread = std::mem::replace(&mut exec.read_thread, std::thread::spawn(|| {}));
        let _ = read_thread.join();
    }

    // DEFERRED (D6 xev termios timer, 0 tests — R1-safe). Preserved verbatim:
    //
    // pub fn focusGained(
    //     self: *Exec,
    //     td: *termio.Termio.ThreadData,
    //     focused: bool,
    // ) !void {
    //     _ = self;
    //
    //     assert(td.backend == .exec);
    //     const execdata = &td.backend.exec;
    //
    //     // Windows has no termios, so there is nothing to poll.
    //     if (comptime builtin.os.tag == .windows) return;
    //
    //     if (!focused) {
    //         // Flag the timer to end on the next iteration. This is
    //         // a lot cheaper than doing full timer cancellation.
    //         execdata.termios_timer_running = false;
    //     } else {
    //         // Always set this to true. There is a race condition if we lose
    //         // focus and regain focus before the termios timer ticks where
    //         // if we don't set this unconditionally the timer will end on
    //         // the next iteration.
    //         execdata.termios_timer_running = true;
    //
    //         // If we're focused, we want to start our termios timer. We
    //         // only do this if it isn't already running. We use the termios
    //         // callback because that'll trigger an immediate state check AND
    //         // start the timer.
    //         if (execdata.termios_timer_c.state() != .active) {
    //             _ = termiosTimer(td, undefined, undefined, {});
    //         }
    //     }
    // }
    pub fn focusGained<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        &mut self,
        td: &mut crate::termio_impl::ThreadData<M>,
        focused: bool,
    ) -> Result<(), crate::Error> {
        // _ = self;
        let _ = self;

        // assert(td.backend == .exec);
        // const execdata = &td.backend.exec;
        let td_ptr: *mut crate::termio_impl::ThreadData<M> = td;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(execdata) = &mut td.backend;
        let execdata: *mut ThreadData = execdata;

        // Windows has no termios, so there is nothing to poll.
        // if (comptime builtin.os.tag == .windows) return;
        #[cfg(windows)]
        return Ok(());

        // if (!focused) {
        #[cfg(not(windows))]
        if !focused {
            // Flag the timer to end on the next iteration. This is
            // a lot cheaper than doing full timer cancellation.
            // execdata.termios_timer_running = false;
            unsafe {
                (*execdata).termios_timer_running = false;
            }
        } else {
            // Always set this to true. There is a race condition if we lose
            // focus and regain focus before the termios timer ticks where
            // if we don't set this unconditionally the timer will end on
            // the next iteration.
            // execdata.termios_timer_running = true;
            unsafe {
                (*execdata).termios_timer_running = true;
            }

            // If we're focused, we want to start our termios timer. We
            // only do this if it isn't already running. We use the termios
            // callback because that'll trigger an immediate state check AND
            // start the timer.
            // if (execdata.termios_timer_c.state() != .active) {
            //     _ = termiosTimer(td, undefined, undefined, {});
            // }
            //
            // SAFETY: `termios_timer_c` is a stable Completion in the frame-pinned
            // `td.backend.exec`; `Completion::state_of` only reads its state flag.
            if unsafe { (*execdata).termios_timer_c.state_of() } != xev::CompletionState::Active {
                // The reference passes `undefined` loop/completion — `termiosTimer`
                // ignores both (`_`); `{}` is the void success value `Ok(())`.
                let _ = Self::termiosTimer::<M>(
                    td_ptr,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    Ok(()),
                );
            }
        }

        #[cfg(not(windows))]
        return Ok(());
    }

    // pub fn resize(
    //     self: *Exec,
    //     grid_size: renderer.GridSize,
    //     screen_size: renderer.ScreenSize,
    // ) !void {
    pub fn resize(
        &mut self,
        grid_size: GridSize,
        screen_size: ScreenSize,
    ) -> Result<(), crate::Error> {
        // return try self.subprocess.resize(grid_size, screen_size);
        self.subprocess.resize(grid_size, screen_size)
    }

    // DEFERRED (D6 — called only from the xev processExit/flatpakExit
    // callbacks; pushes to the opaque-Timeout AppMailbox seam). Preserved
    // verbatim:
    //
    // fn processExitCommon(td: *termio.Termio.ThreadData, exit_code: u32) void {
    //     assert(td.backend == .exec);
    //     const execdata = &td.backend.exec;
    //     execdata.exited = true;
    //
    //     // Determine how long the process was running for.
    //     const runtime_ms: ?u64 = runtime: {
    //         const process_end = std.time.Instant.now() catch break :runtime null;
    //         const runtime_ns = process_end.since(execdata.start);
    //         const runtime_ms = runtime_ns / std.time.ns_per_ms;
    //         break :runtime runtime_ms;
    //     };
    //     log.debug(
    //         "child process exited status={} runtime={}ms",
    //         .{ exit_code, runtime_ms orelse 0 },
    //     );
    //
    //     // We always notify the surface immediately that the child has
    //     // exited and some metadata about the exit.
    //     _ = td.surface_mailbox.push(.{
    //         .child_exited = .{
    //             .exit_code = exit_code,
    //             .runtime_ms = runtime_ms orelse 0,
    //         },
    //     }, .{ .forever = {} });
    // }
    //
    // fn processExit(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Process.WaitError!u32,
    // ) xev.CallbackAction {
    //     const exit_code = r catch unreachable;
    //     processExitCommon(td_.?, exit_code);
    //     return .disarm;
    // }
    //
    // fn flatpakExit(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *FlatpakHostCommand.Completion,
    //     r: FlatpakHostCommand.WaitError!u8,
    // ) void {
    //     const exit_code = r catch unreachable;
    //     processExitCommon(td_.?, exit_code);
    // }
    //
    // fn termiosTimer(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Timer.RunError!void,
    // ) xev.CallbackAction {
    //     // log.debug("termios timer fired", .{});
    //
    //     // This should never happen because we guard starting our
    //     // timer on windows but we want this assertion to fire if
    //     // we ever do start the timer on windows.
    //     // TODO: support on windows
    //     if (comptime builtin.os.tag == .windows) {
    //         @panic("termios timer not implemented on Windows");
    //     }
    //
    //     _ = r catch |err| switch (err) {
    //         // This is sent when our timer is canceled. That's fine.
    //         error.Canceled => return .disarm,
    //
    //         else => {
    //             log.warn("error in termios timer callback err={}", .{err});
    //             @panic("crash in termios timer callback");
    //         },
    //     };
    //
    //     const td = td_.?;
    //     assert(td.backend == .exec);
    //     const exec = &td.backend.exec;
    //
    //     // This is kind of hacky but we rebuild a Pty struct to get the
    //     // termios data.
    //     const mode: ptypkg.Mode = (Pty{
    //         .master = exec.read_thread_fd,
    //         .slave = undefined,
    //     }).getMode() catch |err| err: {
    //         log.warn("error getting termios mode err={}", .{err});
    //
    //         // If we have an error we return the default mode values
    //         // which are the likely values.
    //         break :err .{};
    //     };
    //
    //     // If the mode changed, then we process it.
    //     if (!std.meta.eql(mode, exec.termios_mode)) mode_change: {
    //         log.debug("termios change mode={}", .{mode});
    //         exec.termios_mode = mode;
    //
    //         // We assume we're in some sort of password input if we're
    //         // in canonical mode and not echoing. This is a heuristic.
    //         const password_input = mode.canonical and !mode.echo;
    //
    //         // If our password input state changed on the terminal then
    //         // we notify the surface.
    //         {
    //             td.renderer_state.mutex.lock();
    //             defer td.renderer_state.mutex.unlock();
    //             const t = td.renderer_state.terminal;
    //             if (t.flags.password_input == password_input) {
    //                 break :mode_change;
    //             }
    //         }
    //
    //         // We have to notify the surface that we're in password input.
    //         // We must block on this because the balanced true/false state
    //         // of this is critical to apprt behavior.
    //         _ = td.surface_mailbox.push(.{
    //             .password_input = password_input,
    //         }, .{ .forever = {} });
    //     }
    //
    //     // Repeat the timer
    //     if (exec.termios_timer_running) {
    //         exec.termios_timer.run(
    //             td.loop,
    //             &exec.termios_timer_c,
    //             TERMIOS_POLL_MS,
    //             termio.Termio.ThreadData,
    //             td,
    //             termiosTimer,
    //         );
    //     }
    //
    //     return .disarm;
    // }
    //
    // The three exit / termios callbacks below fill the verbatim reference bodies
    // preserved in the comment block above. `flatpakExit` stays a comment
    // (uninstantiated — its only caller is the comptime-`false` flatpak arm of
    // `threadEnter`, matching Zig lazy compilation). Userdata is termio's
    // `ThreadData<M>` (the reference `termio.Termio.ThreadData`), so these are
    // generic over `M` and reference it via `Self::processExit::<M>` etc.

    // fn processExitCommon(td: *termio.Termio.ThreadData, exit_code: u32) void
    fn processExitCommon<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        td: *mut crate::termio_impl::ThreadData<M>,
        exit_code: u32,
    ) {
        // assert(td.backend == .exec);
        // const execdata = &td.backend.exec;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(execdata) = unsafe { &mut (*td).backend };
        let execdata: *mut ThreadData = execdata;
        // execdata.exited = true;
        unsafe {
            (*execdata).exited = true;
        }

        // Determine how long the process was running for.
        // const runtime_ms: ?u64 = runtime: {
        //     const process_end = std.time.Instant.now() catch break :runtime null;
        //     const runtime_ns = process_end.since(execdata.start);
        //     const runtime_ms = runtime_ns / std.time.ns_per_ms;
        //     break :runtime runtime_ms;
        // };
        //
        // (PORT NOTE: Rust's `Instant::now()` is infallible, so the `catch ... null`
        // arm never fires; `since` -> `duration_since`, `/ ns_per_ms` ->
        // `as_nanos() / 1_000_000`.)
        let runtime_ms: Option<u64> = {
            let process_end = std::time::Instant::now();
            let start = unsafe { (*execdata).start };
            let runtime_ns = process_end.duration_since(start);
            Some((runtime_ns.as_nanos() / 1_000_000) as u64)
        };
        // log.debug("child process exited status={} runtime={}ms", .{ exit_code, runtime_ms orelse 0 });

        // We always notify the surface immediately that the child has
        // exited and some metadata about the exit.
        // _ = td.surface_mailbox.push(.{ .child_exited = .{
        //     .exit_code = exit_code, .runtime_ms = runtime_ms orelse 0,
        // } }, .{ .forever = {} });
        let _ = unsafe {
            (*td).surface_mailbox.push(
                SurfaceMessage::ChildExited(ChildExited {
                    exit_code,
                    runtime_ms: runtime_ms.unwrap_or(0),
                }),
                Timeout::Forever,
            )
        };
    }

    // fn processExit(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Process.WaitError!u32,
    // ) xev.CallbackAction
    fn processExit<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        td_: *mut crate::termio_impl::ThreadData<M>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: core::result::Result<u32, xev::ProcessWaitError>,
    ) -> xev::CallbackAction {
        // const exit_code = r catch unreachable;
        let exit_code = r.unwrap();
        // processExitCommon(td_.?, exit_code);
        Self::processExitCommon::<M>(td_, exit_code);
        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn termiosTimer(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Timer.RunError!void,
    // ) xev.CallbackAction
    #[cfg_attr(windows, allow(unreachable_code, unused_variables))]
    fn termiosTimer<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        td_: *mut crate::termio_impl::ThreadData<M>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: core::result::Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // log.debug("termios timer fired", .{});

        // This should never happen because we guard starting our
        // timer on windows but we want this assertion to fire if
        // we ever do start the timer on windows.
        // TODO: support on windows
        // if (comptime builtin.os.tag == .windows) { @panic("termios timer not implemented on Windows"); }
        #[cfg(windows)]
        panic!("termios timer not implemented on Windows");

        // _ = r catch |err| switch (err) {
        //     error.Canceled => return .disarm,
        //     else => { log.warn(...); @panic("crash in termios timer callback"); },
        // };
        match r {
            Ok(()) => {}
            // This is sent when our timer is canceled. That's fine.
            Err(xev::TimerRunError::Canceled) => return xev::CallbackAction::Disarm,
            Err(_err) => {
                // log.warn("error in termios timer callback err={}", .{err});
                panic!("crash in termios timer callback");
            }
        }

        // const td = td_.?;
        let td = td_;
        // assert(td.backend == .exec);
        // const exec = &td.backend.exec;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(exec) = unsafe { &mut (*td).backend };
        let exec: *mut ThreadData = exec;

        // This is kind of hacky but we rebuild a Pty struct to get the
        // termios data.
        // const mode: ptypkg.Mode = (Pty{ .master = exec.read_thread_fd, .slave = undefined }).getMode() catch |err| err: {
        //     log.warn("error getting termios mode err={}", .{err});
        //     break :err .{};
        // };
        //
        // (PORT NOTE: Rust struct privacy forbids the inline `Pty{ .master, .slave =
        // undefined }` literal; `Pty::init_master_only` is that throwaway
        // master-only Pty — see its rustty-os PORT NOTE.)
        let master = unsafe { (*exec).read_thread_fd };
        // (PORT NOTE: kept as a `match` — not `.unwrap_or_default()` — to preserve the
        // reference's `catch |err| { log.warn(...); break :err .{} }` error arm,
        // which logs before falling back to the default `Mode`.)
        #[allow(clippy::manual_unwrap_or_default)]
        let mode: ptypkg::Mode = match Pty::init_master_only(master).getMode() {
            Ok(m) => m,
            Err(_) => {
                // log.warn("error getting termios mode err={}", .{err});
                // break :err .{};
                ptypkg::Mode::default()
            }
        };

        // If the mode changed, then we process it.
        // if (!std.meta.eql(mode, exec.termios_mode)) mode_change: {
        if mode != unsafe { (*exec).termios_mode } {
            'mode_change: {
                // log.debug("termios change mode={}", .{mode});
                // exec.termios_mode = mode;
                unsafe {
                    (*exec).termios_mode = mode;
                }

                // We assume we're in some sort of password input if we're
                // in canonical mode and not echoing. This is a heuristic.
                // const password_input = mode.canonical and !mode.echo;
                let password_input = mode.canonical && !mode.echo;

                // If our password input state changed on the terminal then
                // we notify the surface.
                // {
                //     td.renderer_state.mutex.lock();
                //     defer td.renderer_state.mutex.unlock();
                //     const t = td.renderer_state.terminal;
                //     if (t.flags.password_input == password_input) break :mode_change;
                // }
                {
                    // SAFETY: `renderer_state` is the live shared render State for
                    // the io thread's life; `.mutex` is the guard-less shared lock
                    // (the guard-less C0 model makes the `defer unlock` explicit).
                    let rs = unsafe { (*td).renderer_state };
                    unsafe {
                        (*(*rs).mutex).lock();
                    }
                    let t = unsafe { (*rs).terminal };
                    let same = unsafe { (*t).flags.password_input } == password_input;
                    unsafe {
                        (*(*rs).mutex).unlock();
                    }
                    if same {
                        break 'mode_change;
                    }
                }

                // We have to notify the surface that we're in password input.
                // We must block on this because the balanced true/false state
                // of this is critical to apprt behavior.
                // _ = td.surface_mailbox.push(.{ .password_input = password_input }, .{ .forever = {} });
                let _ = unsafe {
                    (*td).surface_mailbox.push(
                        SurfaceMessage::PasswordInput(password_input),
                        Timeout::Forever,
                    )
                };
            }
        }

        // Repeat the timer
        // if (exec.termios_timer_running) {
        if unsafe { (*exec).termios_timer_running } {
            // exec.termios_timer.run(
            //     td.loop, &exec.termios_timer_c, TERMIOS_POLL_MS,
            //     termio.Termio.ThreadData, td, termiosTimer,
            // );
            //
            // SAFETY: `td.loop_`/`&exec.termios_timer_c` are stable pointers held
            // raw by the loop; `td` is the frame-pinned userdata.
            unsafe {
                let timer = (*exec).termios_timer;
                let loop_ = (*td).loop_;
                timer.run(
                    loop_,
                    std::ptr::addr_of_mut!((*exec).termios_timer_c),
                    TERMIOS_POLL_MS,
                    td,
                    Self::termiosTimer::<M>,
                );
            }
        }

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // DEFERRED (D6 xev write stream/pools, 0 tests — R1-safe). Preserved
    // verbatim:
    //
    // pub fn queueWrite(
    //     self: *Exec,
    //     alloc: Allocator,
    //     td: *termio.Termio.ThreadData,
    //     data: []const u8,
    //     linefeed: bool,
    // ) !void {
    //     _ = self;
    //     const exec = &td.backend.exec;
    //
    //     // If our process is exited then we don't send any more writes.
    //     if (exec.exited) return;
    //
    //     // We go through and chunk the data if necessary to fit into
    //     // our cached buffers that we can queue to the stream.
    //     var i: usize = 0;
    //     while (i < data.len) {
    //         const req = try exec.write_req_pool.getGrow(alloc);
    //         const buf = try exec.write_buf_pool.getGrow(alloc);
    //         const slice = slice: {
    //             // The maximum end index is either the end of our data or
    //             // the end of our buffer, whichever is smaller.
    //             const max = @min(data.len, i + buf.len);
    //
    //             // Fast
    //             if (!linefeed) {
    //                 fastmem.copy(u8, buf, data[i..max]);
    //                 const len = max - i;
    //                 i = max;
    //                 break :slice buf[0..len];
    //             }
    //
    //             // Slow, have to replace \r with \r\n
    //             var buf_i: usize = 0;
    //             while (i < data.len and buf_i < buf.len - 1) {
    //                 const ch = data[i];
    //                 i += 1;
    //
    //                 if (ch != '\r') {
    //                     buf[buf_i] = ch;
    //                     buf_i += 1;
    //                     continue;
    //                 }
    //
    //                 // CRLF
    //                 buf[buf_i] = '\r';
    //                 buf[buf_i + 1] = '\n';
    //                 buf_i += 2;
    //             }
    //
    //             break :slice buf[0..buf_i];
    //         };
    //
    //         //for (slice) |b| log.warn("write: {x}", .{b});
    //
    //         exec.write_stream.queueWrite(
    //             td.loop,
    //             &exec.write_queue,
    //             req,
    //             .{ .slice = slice },
    //             termio.Exec.ThreadData,
    //             exec,
    //             ttyWrite,
    //         );
    //     }
    // }
    pub fn queueWrite<M: rustty_apprt_api::surface::AppMailbox>(
        &mut self,
        alloc: Allocator,
        td: &mut crate::termio_impl::ThreadData<M>,
        data: &[u8],
        linefeed: bool,
    ) -> Result<(), crate::Error> {
        // _ = self;
        let _ = self;
        let _ = alloc;
        // const exec = &td.backend.exec;
        //
        // Raw pointers: `exec` is `td.backend.exec` (an `exec::ThreadData`); the
        // `Stream::queue_write` engine stores `&req.completion` + the WriteQueue
        // raw into the loop and re-enters them across ticks, and `td.loop_` is the
        // shared loop — never a long-lived `&mut` (`rust-mut-args-strongly-
        // protected`).
        let loop_: *mut xev::Loop = td.loop_;
        // (PORT NOTE: single-variant `union(Kind){exec}` field access.)
        let crate::backend::ThreadData::Exec(exec) = &mut td.backend;
        let exec: *mut ThreadData = exec;

        // If our process is exited then we don't send any more writes.
        // if (exec.exited) return;
        if unsafe { (*exec).exited } {
            return Ok(());
        }

        // We go through and chunk the data if necessary to fit into
        // our cached buffers that we can queue to the stream.
        // var i: usize = 0;
        let mut i: usize = 0;
        // while (i < data.len) {
        while i < data.len() {
            // const req = try exec.write_req_pool.getGrow(alloc);
            let req: *mut xev::WriteRequest = unsafe {
                (*exec)
                    .write_req_pool
                    .get_grow()
                    .map_err(|_| crate::Error)?
            };
            // const buf = try exec.write_buf_pool.getGrow(alloc);
            // (PORT NOTE: the pool element is `WriteBuf([u8; 64])`; `buf` is the inner
            // 64-byte array — `buf.len` == 64.)
            let buf_wrap: *mut WriteBuf = unsafe {
                (*exec)
                    .write_buf_pool
                    .get_grow()
                    .map_err(|_| crate::Error)?
            };
            let buf: *mut [u8; 64] = unsafe { std::ptr::addr_of_mut!((*buf_wrap).0) };
            let buf_len = 64usize;

            // const slice = slice: {
            let slice: *const [u8] = {
                // The maximum end index is either the end of our data or
                // the end of our buffer, whichever is smaller.
                // const max = @min(data.len, i + buf.len);
                let max = std::cmp::min(data.len(), i + buf_len);

                // Fast
                // if (!linefeed) {
                if !linefeed {
                    // fastmem.copy(u8, buf, data[i..max]);
                    // const len = max - i;
                    let len = max - i;
                    // SAFETY: `buf` is a stable 64-byte pool buffer; `len <=
                    // buf_len`; `data[i..max]` is in-bounds.
                    unsafe {
                        std::ptr::copy_nonoverlapping(data[i..max].as_ptr(), buf.cast::<u8>(), len);
                    }
                    // i = max;
                    i = max;
                    // break :slice buf[0..len];
                    std::ptr::slice_from_raw_parts(buf.cast::<u8>(), len)
                } else {
                    // Slow, have to replace \r with \r\n
                    // var buf_i: usize = 0;
                    let mut buf_i: usize = 0;
                    // while (i < data.len and buf_i < buf.len - 1) {
                    while i < data.len() && buf_i < buf_len - 1 {
                        // const ch = data[i];
                        let ch = data[i];
                        // i += 1;
                        i += 1;

                        // if (ch != '\r') {
                        if ch != b'\r' {
                            // buf[buf_i] = ch;
                            unsafe {
                                (*buf)[buf_i] = ch;
                            }
                            // buf_i += 1;
                            buf_i += 1;
                            // continue;
                            continue;
                        }

                        // CRLF
                        // buf[buf_i] = '\r';
                        // buf[buf_i + 1] = '\n';
                        unsafe {
                            (*buf)[buf_i] = b'\r';
                            (*buf)[buf_i + 1] = b'\n';
                        }
                        // buf_i += 2;
                        buf_i += 2;
                    }

                    // break :slice buf[0..buf_i];
                    std::ptr::slice_from_raw_parts(buf.cast::<u8>(), buf_i)
                }
            };

            //for (slice) |b| log.warn("write: {x}", .{b});

            // exec.write_stream.queueWrite(
            //     td.loop, &exec.write_queue, req, .{ .slice = slice },
            //     termio.Exec.ThreadData, exec, ttyWrite,
            // );
            //
            // SAFETY: `loop_`/`&exec.write_queue`/`req` are stable pointers held
            // raw by the ordered-write engine; `slice` points into the stable
            // pool buffer (recycled by `ttyWrite` after the write completes); the
            // userdata is the `exec::ThreadData` pointer `ttyWrite` recovers.
            unsafe {
                let write_stream = (*exec).write_stream;
                write_stream.queue_write(
                    loop_,
                    std::ptr::addr_of_mut!((*exec).write_queue),
                    req,
                    xev::WriteBuffer::Slice(slice),
                    exec,
                    Self::ttyWrite,
                );
            }
        }

        Ok(())
    }

    // DEFERRED (D6 — the xev stream-write completion callback). Preserved
    // verbatim:
    //
    // fn ttyWrite(
    //     td_: ?*ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     _: xev.Stream,
    //     _: xev.WriteBuffer,
    //     r: xev.WriteError!usize,
    // ) xev.CallbackAction {
    //     const td = td_.?;
    //     td.write_req_pool.put();
    //     td.write_buf_pool.put();
    //
    //     const d = r catch |err| {
    //         log.err("write error: {}", .{err});
    //         return .disarm;
    //     };
    //     _ = d;
    //     //log.info("WROTE: {d}", .{d});
    //
    //     return .disarm;
    // }
    //
    // PORT NOTE: `td_` here is the `exec::ThreadData` (the reference `ThreadData`
    // in Exec.zig scope), NOT termio's — the reference passed `exec` as the
    // `queueWrite` userdata. Non-generic (no `M`).
    fn ttyWrite(
        td_: *mut ThreadData,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        _s: xev::Stream,
        _b: xev::WriteBuffer,
        r: core::result::Result<usize, xev::WriteError>,
    ) -> xev::CallbackAction {
        // const td = td_.?;
        let td = td_;
        // td.write_req_pool.put();
        // td.write_buf_pool.put();
        unsafe {
            (*td).write_req_pool.put();
            (*td).write_buf_pool.put();
        }

        // const d = r catch |err| { log.err("write error: {}", .{err}); return .disarm; };
        let _d = match r {
            Ok(d) => d,
            Err(_err) => {
                // log.err("write error: {}", .{err});
                return xev::CallbackAction::Disarm;
            }
        };
        // _ = d;
        //log.info("WROTE: {d}", .{d});

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    /// Get information about the process(es) running within the backend. Returns
    /// `null` if there was an error getting the information or the information is
    /// not available on a particular platform.
    //
    // PORT NOTE: reference `pub fn getProcessInfo(self: *Exec, comptime info:
    // ProcessInfo) ?ProcessInfo.Type(info) { return self.subprocess.getProcessInfo(info); }`
    // — split into the two concrete accessors (rustty-os pty.rs / backend.rs
    // precedent).
    pub fn getForegroundPid(&mut self) -> Option<u64> {
        self.subprocess.getForegroundPid()
    }

    /// See [`Exec::getForegroundPid`] (the `.tty_name` half of the reference
    /// `getProcessInfo`).
    pub fn getTtyName(&mut self) -> Option<&CStr> {
        self.subprocess.getTtyName()
    }
}

/// The thread local data for the exec implementation.
//
// pub const ThreadData = struct {
pub struct ThreadData {
    /// Process start time and boolean of whether its already exited.
    pub start: std::time::Instant,
    // exited: bool = false,
    pub exited: bool,

    /// The data stream is the main IO for the pty.
    pub write_stream: xev::Stream,

    /// The process watcher
    pub process: Option<xev::Process>,

    /// This is the pool of available (unused) write requests. If you grab
    /// one from the pool, you must put it back when you're done!
    // write_req_pool: SegmentedPool(xev.WriteRequest, WRITE_REQ_PREALLOC) = .{},
    pub write_req_pool: SegmentedPool<xev::WriteRequest>,

    /// The pool of available buffers for writing to the pty.
    // write_buf_pool: SegmentedPool([64]u8, WRITE_REQ_PREALLOC) = .{},
    //
    // (PORT NOTE: `WriteBuf` wraps the `[64]u8` element — see its note.)
    pub write_buf_pool: SegmentedPool<WriteBuf>,

    /// The write queue for the data stream.
    // write_queue: xev.WriteQueue = .{},
    pub write_queue: xev::WriteQueue,

    /// This is used for both waiting for the process to exit and then
    /// subsequently to wait for the data_stream to close.
    // process_wait_c: xev.Completion = .{},
    pub process_wait_c: xev::Completion,

    // The completion specific to Flatpak process waiting. If
    // we aren't compiling with Flatpak support this is zero-sized.
    // flatpak_wait_c: FlatpakHostCommand.Completion = .{},
    pub flatpak_wait_c: FlatpakHostCommandCompletion,

    /// Reader thread state
    // read_thread: std.Thread,
    pub read_thread: std::thread::JoinHandle<()>,
    // read_thread_pipe: posix.fd_t,
    // read_thread_fd: posix.fd_t,
    #[cfg(not(windows))]
    pub read_thread_pipe: std::os::fd::RawFd,
    #[cfg(not(windows))]
    pub read_thread_fd: std::os::fd::RawFd,
    // (PORT NOTE: on windows `posix.fd_t` is a HANDLE.)
    #[cfg(windows)]
    pub read_thread_pipe: rustty_os::windows::HANDLE,
    #[cfg(windows)]
    pub read_thread_fd: rustty_os::windows::HANDLE,

    /// The timer to detect termios state changes.
    pub termios_timer: xev::Timer,
    // termios_timer_c: xev.Completion = .{},
    pub termios_timer_c: xev::Completion,
    // termios_timer_running: bool = true,
    pub termios_timer_running: bool,

    /// The last known termios mode. Used for change detection
    /// to prevent unnecessary locking of expensive mutexes.
    // termios_mode: ptypkg.Mode = .{},
    pub termios_mode: ptypkg::Mode,
}

/// PORT NOTE (Rust-only glue): the `[64]u8` write-buffer element of
/// `write_buf_pool` (Rust arrays implement `Default` — the SegmentedPool
/// element bound — only up to length 32, so the 64-byte buffer wraps).
pub struct WriteBuf(pub [u8; 64]);

impl Default for WriteBuf {
    fn default() -> Self {
        WriteBuf([0u8; 64])
    }
}

impl ThreadData {
    // The preallocation size for the write request pool. This should be big
    // enough to satisfy most write requests. It must be a power of 2.
    //
    // const WRITE_REQ_PREALLOC = std.math.pow(usize, 2, 5);
    // (Consumed by the D6-deferred threadEnter construction — see the pool
    // fields' `= .{}` defaults above.)
    #[allow(dead_code)]
    pub(crate) const WRITE_REQ_PREALLOC: usize = usize::pow(2, 5);

    /// PORT NOTE (Rust-only glue, xev B4): the reference `Termio.threadEnter`
    /// initializes `ThreadData.backend = undefined` and `Exec.threadEnter` then
    /// OVERWRITES it (`td.backend = .{ .exec = ... }`). Zig assigns over
    /// `undefined` with no cleanup; Rust's `td.backend = ...` DROPS the old value
    /// (B2's chosen move-assign shape), so the slot must hold a *droppable*
    /// placeholder rather than genuine garbage. This is that inert placeholder —
    /// the exact shape the B2 write-path KAT constructs inline: an
    /// already-finished dummy thread + inert (`-1`) fds + empty pools, so its drop
    /// (on the `Exec.threadEnter` overwrite) is a cheap no-op. It is never observed
    /// (the write path only touches it after `Exec.threadEnter` has replaced it).
    #[cfg(not(windows))]
    pub(crate) fn placeholder() -> ThreadData {
        ThreadData {
            start: std::time::Instant::now(),
            exited: false,
            write_stream: xev::Stream::init_fd(-1),
            process: None,
            write_req_pool: SegmentedPool::new(0),
            write_buf_pool: SegmentedPool::new(0),
            write_queue: xev::WriteQueue::default(),
            process_wait_c: xev::Completion::default(),
            flatpak_wait_c: FlatpakHostCommandCompletion::default(),
            read_thread: std::thread::spawn(|| {}),
            read_thread_pipe: -1,
            read_thread_fd: -1,
            termios_timer: xev::Timer::init().expect("placeholder Timer::init"),
            termios_timer_c: xev::Completion::default(),
            termios_timer_running: false,
            termios_mode: ptypkg::Mode::default(),
        }
    }

    // pub fn deinit(self: *ThreadData, alloc: Allocator) void {
    pub fn deinit(&mut self, alloc: Allocator) {
        // posix.close(self.read_thread_pipe);
        #[cfg(not(windows))]
        unsafe {
            libc::close(self.read_thread_pipe)
        };
        #[cfg(windows)]
        let _ = rustty_os::windows::CloseHandle(self.read_thread_pipe);

        // Clear our write pools. We know we aren't ever going to do
        // any more IO since we stop our data stream below so we can just
        // drop this.
        // self.write_req_pool.deinit(alloc);
        // self.write_buf_pool.deinit(alloc);
        // (PORT NOTE: the pools' memory frees on Drop; the alloc param collapses.)
        let _ = alloc;

        // Stop our process watcher
        // if (self.process) |*p| p.deinit();
        if let Some(p) = &mut self.process {
            p.deinit();
        }

        // Stop our write stream
        self.write_stream.deinit();

        // Stop our termios timer
        self.termios_timer.deinit();
    }
}

// pub const Config = struct {
//
// PORT NOTE: the reference's borrowed `?[]const u8` fields (`working_directory`
// /`resources_dir`/`term`) are owned byte buffers here — the struct is built by
// core's Surface from config memory whose lifetime the borrow checker cannot
// thread through the backend union (arena-collapse precedent). Fields with Zig
// defaults are listed explicitly in Rust struct literals (Command precedent).
pub struct Config {
    // command: ?configpkg.Command = null,
    pub command: Option<configpkg::command::Command>,
    // env: EnvMap,
    pub env: EnvMap,
    // env_override: configpkg.RepeatableStringMap = .{},
    pub env_override: configpkg::RepeatableStringMap,
    // shell_integration: configpkg.Config.ShellIntegration = .detect,
    pub shell_integration: configpkg::value_types::ShellIntegration,
    // shell_integration_features: configpkg.Config.ShellIntegrationFeatures = .{},
    pub shell_integration_features: configpkg::value_types::ShellIntegrationFeatures,
    // cursor_blink: ?bool = null,
    pub cursor_blink: Option<bool>,
    // working_directory: ?[]const u8 = null,
    pub working_directory: Option<Vec<u8>>,
    // resources_dir: ?[]const u8,
    pub resources_dir: Option<Vec<u8>>,
    // term: []const u8,
    pub term: Vec<u8>,

    pub rt_pre_exec_info: RtPreExecInfo,
    pub rt_post_fork_info: RtPostForkInfo,
}

// const Subprocess = struct {
//
// PORT NOTE: reference file-private; `pub` because it appears in the `Exec`
// field shape (Zig-private widening rule).
pub struct Subprocess {
    // const c = @cImport({
    //     @cInclude("errno.h");
    //     @cInclude("signal.h");
    //     @cInclude("unistd.h");
    // });
    // (PORT NOTE: the `libc` binding — the SAME system headers; R3.)

    // arena: std.heap.ArenaAllocator,
    // (PORT NOTE: the arena collapses into the owned fields below — crash-tier
    // precedent.)
    // cwd: ?[:0]const u8,
    pub cwd: Option<CString>,
    // env: ?EnvMap,
    pub env: Option<EnvMap>,
    // args: []const [:0]const u8,
    pub args: Vec<CString>,
    pub grid_size: GridSize,
    pub screen_size: ScreenSize,
    // pty: ?Pty = null,
    pub pty: Option<Pty>,
    // process: ?Process = null,
    pub process: Option<Process>,

    pub rt_pre_exec_info: RtPreExecInfo,
    pub rt_post_fork_info: RtPostForkInfo,
}

/// Union that represents the running process type.
//
// const Process = union(enum) {
//
// PORT NOTE: the `fork_exec: Command` payload stores the reference's
// arena-pointer-backed Command by value; the Rust `Command<'static>` type
// carries the same shape (the deferred `start` is the only constructor — see
// its PORT NOTE).
pub enum Process {
    /// Standard POSIX fork/exec
    ForkExec(Command<'static>),

    /// Flatpak DBus command
    Flatpak(FlatpakHostCommand),
}

// const ArgsFormatter = struct {
//     args: []const [:0]const u8,
//
//     pub fn format(this: @This(), writer: *std.Io.Writer) std.Io.Writer.Error!void {
//         for (this.args, 0..) |a, i| {
//             if (i > 0) try writer.writeAll(", ");
//             try writer.print("`{s}`", .{a});
//         }
//     }
// };
//
// PORT NOTE: consumed only by `start`'s "starting command" debug log (the
// deferred body); ported as the Display impl for log parity.
// (dead_code: the only reference constructor is `start`'s debug log —
// deferred above.)
#[allow(dead_code)]
pub(crate) struct ArgsFormatter<'a> {
    pub args: &'a [CString],
}

impl core::fmt::Display for ArgsFormatter<'_> {
    fn fmt(&self, writer: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, a) in self.args.iter().enumerate() {
            if i > 0 {
                writer.write_str(", ")?;
            }
            write!(writer, "`{}`", String::from_utf8_lossy(a.to_bytes()))?;
        }
        Ok(())
    }
}

// The reference's `os_pre_exec` callback (an anonymous `struct { fn callback(cmd:
// *Command) ?u8 } `). It runs /in the forked child/ after fork but before exec.
// Ported as a free fn matching `PreExecFn`; it recovers the owning `Subprocess`
// from `cmd.data` (the raw `*mut Subprocess` self-pointer stored by `start`) and
// dereferences it ONLY here in the child — where `fork` has copied the parent's
// address space, so the parent-side borrows are irrelevant (the raw-ptr
// container_of discipline, vtc-effects-trampoline-seam).
//
//     .os_pre_exec = switch (comptime builtin.os.tag) {
//         .windows => null,
//         else => f: {
//             const f = struct {
//                 fn callback(cmd: *Command) ?u8 {
//                     const sp = cmd.getData(Subprocess) orelse unreachable;
//                     sp.childPreExec() catch |err| log.err(
//                         "error initializing child: {}",
//                         .{err},
//                     );
//                     return null;
//                 }
//             };
//             break :f f.callback;
//         },
//     },
#[cfg(not(windows))]
fn subprocess_pre_exec(cmd: &mut Command) -> Option<u8> {
    // const sp = cmd.getData(Subprocess) orelse unreachable;
    let sp = cmd.getData::<Subprocess>().expect("getData(Subprocess)");
    // sp.childPreExec() catch |err| log.err("error initializing child: {}", .{err});
    if unsafe { (*sp).childPreExec() }.is_err() {
        // log.err("error initializing child: {}", .{err});
    }
    // return null;
    None
}

impl Subprocess {
    /// Initialize the subprocess. This will NOT start it, this only sets
    /// up the internal state necessary to start it later.
    //
    // pub fn init(gpa: Allocator, cfg: Config) !Subprocess {
    pub fn init(_gpa: Allocator, cfg: Config) -> Result<Subprocess, InitError> {
        // We have a lot of maybe-allocations that all share the same lifetime
        // so use an arena so we don't end up in an accounting nightmare.
        // var arena = std.heap.ArenaAllocator.init(gpa);
        // errdefer arena.deinit();
        // const alloc = arena.allocator();
        // (PORT NOTE: the arena collapses into the owned fields below.)

        // Get our env. If a default env isn't provided by the caller
        // then we get it ourselves.
        // var env = cfg.env;
        let mut env = cfg.env;

        // If we have a resources dir then set our env var
        // if (cfg.resources_dir) |dir| {
        //     log.info("found Ghostty resources dir: {s}", .{dir});
        //     try env.put("GHOSTTY_RESOURCES_DIR", dir);
        // }
        if let Some(dir) = &cfg.resources_dir {
            // log.info("found Ghostty resources dir: {s}", .{dir});
            env.insert(env_name("RESOURCES_DIR"), env_string(dir));
        }

        // Set our TERM var. This is a bit complicated because we want to use
        // the ghostty TERM value but we want to only do that if we have
        // ghostty in the TERMINFO database.
        //
        // For now, we just look up a bundled dir but in the future we should
        // also load the terminfo database and look for it.
        // if (cfg.resources_dir) |base| {
        if let Some(base) = &cfg.resources_dir {
            // try env.put("TERM", cfg.term);
            env.insert("TERM".to_string(), env_string(&cfg.term));
            // try env.put("COLORTERM", "truecolor");
            env.insert("COLORTERM".to_string(), "truecolor".to_string());

            // Assume that the resources directory is adjacent to the terminfo
            // database
            // var buf: [std.fs.max_path_bytes]u8 = undefined;
            // const dir = try std.fmt.bufPrint(&buf, "{s}/terminfo", .{
            //     std.fs.path.dirname(base) orelse unreachable,
            // });
            let dir = format!("{}/terminfo", env_string(dirname(base).unwrap()));
            // try env.put("TERMINFO", dir);
            env.insert("TERMINFO".to_string(), dir);
        } else {
            // if (comptime builtin.target.os.tag.isDarwin()) {
            //     log.warn("ghostty terminfo not found, using xterm-256color", .{});
            //     log.warn("the terminfo SHOULD exist on macos, please ensure", .{});
            //     log.warn("you're using a valid app bundle.", .{});
            // }

            // try env.put("TERM", "xterm-256color");
            env.insert("TERM".to_string(), "xterm-256color".to_string());
            // try env.put("COLORTERM", "truecolor");
            env.insert("COLORTERM".to_string(), "truecolor".to_string());
        }

        // Add our binary to the path if we can find it.
        // ghostty_path: {
        'rustty_path: {
            // Skip this for flatpak since host cannot reach them
            // if ((comptime build_config.flatpak) and
            //     internal_os.isFlatpak())
            // {
            //     break :ghostty_path;
            // }
            if rustty_build_config::flatpak() && rustty_os::flatpak::isFlatpak() {
                break 'rustty_path;
            }

            // var exe_buf: [std.fs.max_path_bytes]u8 = undefined;
            // const exe_bin_path = std.fs.selfExePath(&exe_buf) catch |err| {
            //     log.warn("failed to get ghostty exe path err={}", .{err});
            //     break :ghostty_path;
            // };
            let exe_bin_path = match std::env::current_exe() {
                Ok(p) => p,
                Err(_err) => {
                    // log.warn("failed to get ghostty exe path err={}", .{err});
                    break 'rustty_path;
                }
            };
            // const exe_dir = std.fs.path.dirname(exe_bin_path) orelse break :ghostty_path;
            let exe_dir = match exe_bin_path.parent().and_then(|d| d.to_str()) {
                Some(d) => d,
                None => break 'rustty_path,
            };
            // log.debug("appending ghostty bin to path dir={s}", .{exe_dir});

            // We always set this so that if the shell overwrites the path
            // scripts still have a way to find the Ghostty binary when
            // running in Ghostty.
            // try env.put("GHOSTTY_BIN_DIR", exe_dir);
            env.insert(env_name("BIN_DIR"), exe_dir.to_string());

            // Append if we have a path. We want to append so that ghostty is
            // the last priority in the path. If we don't have a path set
            // then we just set it to the directory of the binary.
            // if (env.get("PATH")) |path| {
            if let Some(path) = env.get("PATH").cloned() {
                // Verify that our path doesn't already contain this entry
                // var it = std.mem.tokenizeScalar(u8, path, std.fs.path.delimiter);
                // while (it.next()) |entry| {
                //     if (std.mem.eql(u8, entry, exe_dir)) break :ghostty_path;
                // }
                let delimiter = if cfg!(windows) { ';' } else { ':' };
                for entry in path.split(delimiter).filter(|s| !s.is_empty()) {
                    if entry == exe_dir {
                        break 'rustty_path;
                    }
                }

                // try env.put(
                //     "PATH",
                //     try internal_os.appendEnv(alloc, path, exe_dir),
                // );
                env.insert(
                    "PATH".to_string(),
                    rustty_os::env::appendEnv(&path, exe_dir),
                );
            } else {
                // try env.put("PATH", exe_dir);
                env.insert("PATH".to_string(), exe_dir.to_string());
            }
        }

        // On macOS, export additional data directories from our
        // application bundle.
        // if (comptime builtin.target.os.tag.isDarwin()) darwin: {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        'darwin: {
            // const resources_dir = cfg.resources_dir orelse break :darwin;
            let resources_dir = match &cfg.resources_dir {
                Some(d) => d,
                None => break 'darwin,
            };

            // var buf: [std.fs.max_path_bytes]u8 = undefined;

            // const xdg_data_dir_key = "XDG_DATA_DIRS";
            let xdg_data_dir_key = "XDG_DATA_DIRS";
            // if (std.fmt.bufPrint(&buf, "{s}/..", .{resources_dir})) |data_dir| {
            //     try env.put(
            //         xdg_data_dir_key,
            //         try internal_os.appendEnv(
            //             alloc,
            //             env.get(xdg_data_dir_key) orelse "/usr/local/share:/usr/share",
            //             data_dir,
            //         ),
            //     );
            // } else |err| {
            //     log.warn("error building {s}; err={}", .{ xdg_data_dir_key, err });
            // }
            // (PORT NOTE: the bufPrint NoSpaceLeft error branch collapses with the
            // growable format.)
            let data_dir = format!("{}/..", env_string(resources_dir));
            let current = env
                .get(xdg_data_dir_key)
                .map(String::as_str)
                .unwrap_or("/usr/local/share:/usr/share")
                .to_string();
            env.insert(
                xdg_data_dir_key.to_string(),
                rustty_os::env::appendEnv(&current, &data_dir),
            );

            // const manpath_key = "MANPATH";
            let manpath_key = "MANPATH";
            // if (std.fmt.bufPrint(&buf, "{s}/../man", .{resources_dir})) |man_dir| {
            //     // Always append with colon in front, as it mean that if
            //     // `MANPATH` is empty, then it should be treated as an extra
            //     // path instead of overriding all paths set by OS.
            //     try env.put(
            //         manpath_key,
            //         try internal_os.appendEnvAlways(
            //             alloc,
            //             env.get(manpath_key) orelse "",
            //             man_dir,
            //         ),
            //     );
            // } else |err| {
            //     log.warn("error building {s}; man pages may not be available; err={}", .{ manpath_key, err });
            // }
            let man_dir = format!("{}/../man", env_string(resources_dir));
            let current = env
                .get(manpath_key)
                .map(String::as_str)
                .unwrap_or("")
                .to_string();
            env.insert(
                manpath_key.to_string(),
                rustty_os::env::appendEnvAlways(&current, &man_dir),
            );
        }

        // Set environment variables used by some programs (such as neovim) to detect
        // which terminal emulator and version they're running under.
        // try env.put("TERM_PROGRAM", "ghostty");
        // (R2 identity: the product name.)
        env.insert(
            "TERM_PROGRAM".to_string(),
            rustty_identity::TERM_PROGRAM.to_string(),
        );
        // try env.put("TERM_PROGRAM_VERSION", build_config.version_string);
        env.insert(
            "TERM_PROGRAM_VERSION".to_string(),
            rustty_build_config::VERSION_STRING.to_string(),
        );

        // VTE_VERSION is set by gnome-terminal and other VTE-based terminals.
        // We don't want our child processes to think we're running under VTE.
        // This is not apprt-specific, so we do it here.
        // env.remove("VTE_VERSION");
        env.remove("VTE_VERSION");

        // Setup our shell integration, if we can.
        // const shell_command: configpkg.Command = shell: {
        let shell_command: configpkg::command::Command = 'shell: {
            // const default_shell_command: configpkg.Command =
            //     cfg.command orelse .{ .shell = switch (builtin.os.tag) {
            //         .windows => "cmd.exe",
            //         else => "sh",
            //     } };
            let default_shell_command: configpkg::command::Command = match &cfg.command {
                Some(c) => c.clone(),
                None => configpkg::command::Command::Shell(if cfg!(windows) {
                    b"cmd.exe".to_vec()
                } else {
                    b"sh".to_vec()
                }),
            };

            // Always set up shell features (GHOSTTY_SHELL_FEATURES). These are
            // used by both automatic and manual shell integrations.
            // try shell_integration.setupFeatures(
            //     &env,
            //     cfg.shell_integration_features,
            //     cfg.cursor_blink orelse true,
            // );
            shell_integration::setupFeatures(
                &mut env,
                cfg.shell_integration_features,
                cfg.cursor_blink.unwrap_or(true),
            );

            // const force: ?shell_integration.Shell = switch (cfg.shell_integration) {
            let force: Option<shell_integration::Shell> = match cfg.shell_integration {
                // .none => {
                configpkg::value_types::ShellIntegration::None => {
                    // This is a source of confusion for users despite being
                    // opt-in since it results in some Ghostty features not
                    // working. We always want to log it.
                    // log.info("shell integration disabled by configuration", .{});
                    break 'shell default_shell_command;
                }

                // .detect => null,
                configpkg::value_types::ShellIntegration::Detect => None,
                // .bash => .bash,
                configpkg::value_types::ShellIntegration::Bash => {
                    Some(shell_integration::Shell::Bash)
                }
                // .elvish => .elvish,
                configpkg::value_types::ShellIntegration::Elvish => {
                    Some(shell_integration::Shell::Elvish)
                }
                // .fish => .fish,
                configpkg::value_types::ShellIntegration::Fish => {
                    Some(shell_integration::Shell::Fish)
                }
                // .nushell => .nushell,
                configpkg::value_types::ShellIntegration::Nushell => {
                    Some(shell_integration::Shell::Nushell)
                }
                // .zsh => .zsh,
                configpkg::value_types::ShellIntegration::Zsh => {
                    Some(shell_integration::Shell::Zsh)
                }
            };

            // const dir = cfg.resources_dir orelse {
            //     log.warn("no resources dir set, shell integration disabled", .{});
            //     break :shell default_shell_command;
            // };
            let dir = match &cfg.resources_dir {
                Some(d) => d,
                None => {
                    // log.warn("no resources dir set, shell integration disabled", .{});
                    break 'shell default_shell_command;
                }
            };

            // const integration = try shell_integration.setup(
            //     alloc,
            //     dir,
            //     default_shell_command,
            //     &env,
            //     force,
            // ) orelse {
            //     log.warn("shell could not be detected, no automatic shell integration will be injected", .{});
            //     break :shell default_shell_command;
            // };
            let integration =
                match shell_integration::setup(dir, &default_shell_command, &mut env, force)
                    .map_err(InitError::ShellIntegration)?
                {
                    Some(i) => i,
                    None => {
                        // log.warn("shell could not be detected, no automatic shell integration will be injected", .{});
                        break 'shell default_shell_command;
                    }
                };

            // log.info(
            //     "shell integration automatically injected shell={}",
            //     .{integration.shell},
            // );

            // break :shell integration.command;
            integration.command
        };

        // Add the environment variables that override any others.
        {
            // var it = cfg.env_override.iterator();
            // while (it.next()) |entry| try env.put(
            //     entry.key_ptr.*,
            //     entry.value_ptr.*,
            // );
            for (key, value) in cfg.env_override.iterator() {
                env.insert(env_string(key), env_string(value));
            }
        }

        // Build our args list
        // const args: []const [:0]const u8 = execCommand(
        //     alloc,
        //     shell_command,
        //     internal_os.passwd,
        // ) catch |err| switch (err) {
        //     // If we fail to allocate space for the command we want to
        //     // execute, we'd still like to try to run something so
        //     // Ghostty can launch (and maybe the user can debug this further).
        //     // Realistically, if you're getting OOM, I think other stuff is
        //     // about to crash, but we can try.
        //     error.OutOfMemory => oom: {
        //         log.warn("failed to allocate space for command args, falling back to basic shell", .{});
        //
        //         // The comptime here is important to ensure the full slice
        //         // is put into the binary data and not the stack.
        //         break :oom comptime switch (builtin.os.tag) {
        //             .windows => &.{"cmd.exe"},
        //             else => &.{"/bin/sh"},
        //         };
        //     },
        //
        //     // This logs on its own, this is a bad error.
        //     error.SystemError => return err,
        // };
        //
        // (PORT NOTE: the OutOfMemory arm collapses with Rust's infallible-
        // allocation model — preserved verbatim above; SystemError propagates.)
        let args: Vec<CString> = match execCommand::<OsPasswd>(&shell_command) {
            Ok(args) => args,
            Err(ExecCommandError::SystemError) => return Err(InitError::SystemError),
        };

        // We have to copy the cwd because there is no guarantee that
        // pointers in full_config remain valid.
        // const cwd: ?[:0]u8 = if (cfg.working_directory) |cwd|
        //     try alloc.dupeZ(u8, cwd)
        // else
        //     null;
        let cwd: Option<CString> = cfg.working_directory.as_deref().map(cstring);

        // Propagate the current working directory (CWD) to the shell, enabling
        // the shell to display the current directory name rather than the
        // resolved path for symbolic links. This is important and based
        // on the same behavior in Konsole and Kitty (see the linked issues):
        // https://bugs.kde.org/show_bug.cgi?id=242114
        // https://github.com/kovidgoyal/kitty/issues/1595
        // https://github.com/ghostty-org/ghostty/discussions/7769
        // if (cwd) |pwd| try env.put("PWD", pwd);
        if let Some(pwd) = &cwd {
            env.insert("PWD".to_string(), env_string(pwd.to_bytes()));
        }

        // return .{
        Ok(Subprocess {
            // .arena = arena, — (collapsed; see the struct PORT NOTE)
            env: Some(env),
            cwd,
            args,

            rt_pre_exec_info: cfg.rt_pre_exec_info,
            rt_post_fork_info: cfg.rt_post_fork_info,

            // (Zig field defaults `pty: ?Pty = null` / `process: ?Process =
            // null` are explicit in Rust struct literals — Command precedent.)
            pty: None,
            process: None,

            // Should be initialized with initTerminal call.
            // .grid_size = .{},
            grid_size: GridSize::default(),
            // .screen_size = .{ .width = 1, .height = 1 },
            screen_size: ScreenSize {
                width: 1,
                height: 1,
            },
        })
    }

    /// Clean up the subprocess. This will stop the subprocess if it is started.
    //
    // pub fn deinit(self: *Subprocess) void {
    pub fn deinit(&mut self) {
        // self.stop();
        self.stop();
        // if (self.pty) |*pty| pty.deinit();
        if let Some(pty) = &mut self.pty {
            pty.deinit();
        }
        // if (self.env) |*env| env.deinit();
        self.env = None;
        // self.arena.deinit();
        // self.* = undefined;
        // (PORT NOTE: the arena's memory is the owned fields, freed with the value;
        // the poison is Rust's "must not reuse after deinit" contract.)
    }

    // PORTED (xev B1, 0 reference tests — R1-safe): the spawn body is now live.
    // The former deferral point was the final `self.process = .{ .fork_exec =
    // cmd }` — the reference stores a Command whose fields are arena POINTERS
    // into the subprocess's own args/env (freed-on-success env included), which
    // Zig tolerates as raw pointers but which as Rust references would be exactly
    // the strongly-protected-reference UB the port forbids (memory
    // rust-mut-args-strongly-protected). RESOLUTION (blueprint §2, the "pid-only
    // handle" the deferral named): the transient argv/env/cwd borrows live ONLY
    // inside `start()`'s frame (read exactly once, by `cmd.start()`); the stored
    // `Process::ForkExec(Command)` has TRUE 'static empty reference fields (`c""`,
    // `&[]`, `None`) with only `pid` live, so no self-referential borrow exists at
    // rest (`killCommand` reads only `pid`). [ADV-FIX F3] the pty.slave File is
    // ManuallyDrop (Command reads only `.as_raw_fd()`) + the single verbatim
    // `posix.close`. [ADV-FIX F4] the raw `*mut Subprocess` self-pointer (`data`)
    // is derived via `addr_of_mut!` BEFORE any `&self.*` borrow and dereferenced
    // ONLY in the forked child (`subprocess_pre_exec`). Reference preserved
    // verbatim below (R5: no silent drop):
    //
    // /// Start the subprocess. If the subprocess is already started this
    // /// will crash.
    // pub fn start(self: *Subprocess, alloc: Allocator) !struct {
    //     read: Pty.Fd,
    //     write: Pty.Fd,
    // } {
    //     assert(self.pty == null and self.process == null);
    //
    //     // This function is funny because on POSIX systems it can
    //     // fail in the forked process. This is flipped to true if
    //     // we're in an error state in the forked process (child
    //     // process).
    //     var in_child: bool = false;
    //
    //     // Create our pty
    //     var pty = try Pty.open(.{
    //         .ws_row = @intCast(self.grid_size.rows),
    //         .ws_col = @intCast(self.grid_size.columns),
    //         .ws_xpixel = @intCast(self.screen_size.width),
    //         .ws_ypixel = @intCast(self.screen_size.height),
    //     });
    //     self.pty = pty;
    //     errdefer if (!in_child) {
    //         if (comptime builtin.os.tag != .windows) {
    //             _ = posix.close(pty.slave);
    //         }
    //
    //         pty.deinit();
    //         self.pty = null;
    //     };
    //
    //     // Cleanup we only run in our parent when we successfully start
    //     // the process.
    //     defer if (!in_child and self.process != null) {
    //         if (comptime builtin.os.tag != .windows) {
    //             // Once our subcommand is started we can close the slave
    //             // side. This prevents the slave fd from being leaked to
    //             // future children.
    //             _ = posix.close(pty.slave);
    //         }
    //
    //         // Successful start we can clear out some memory.
    //         if (self.env) |*env| {
    //             env.deinit();
    //             self.env = null;
    //         }
    //     };
    //
    //     log.debug("starting command command={f}", .{ArgsFormatter{ .args = self.args }});
    //
    //     // If we can't access the cwd, then don't set any cwd and inherit.
    //     // This is important because our cwd can be set by the shell (OSC 7)
    //     // and we don't want to break new windows.
    //     const cwd: ?[:0]const u8 = if (self.cwd) |proposed| cwd: {
    //         if ((comptime build_config.flatpak) and internal_os.isFlatpak()) {
    //             // Flatpak sandboxing prevents access to certain reserved paths
    //             // regardless of configured permissions. Perform a test spawn
    //             // to get around this problem
    //             //
    //             // https://docs.flatpak.org/en/latest/sandbox-permissions.html#reserved-paths
    //             log.info("flatpak detected, will use host command to verify cwd access", .{});
    //             const dev_null = try std.fs.cwd().openFile("/dev/null", .{ .mode = .read_write });
    //             defer dev_null.close();
    //             var cmd: internal_os.FlatpakHostCommand = .{
    //                 .argv = &[_][]const u8{
    //                     "/bin/sh",
    //                     "-c",
    //                     ":",
    //                 },
    //                 .cwd = proposed,
    //                 .stdin = dev_null.handle,
    //                 .stdout = dev_null.handle,
    //                 .stderr = dev_null.handle,
    //             };
    //             _ = cmd.spawn(alloc) catch |err| {
    //                 log.warn("cannot spawn command at cwd, ignoring: {}", .{err});
    //                 break :cwd null;
    //             };
    //             _ = try cmd.wait();
    //
    //             break :cwd proposed;
    //         }
    //
    //         if (std.fs.cwd().access(proposed, .{})) {
    //             break :cwd proposed;
    //         } else |err| {
    //             log.warn("cannot access cwd, ignoring: {}", .{err});
    //             break :cwd null;
    //         }
    //     } else null;
    //
    //     // In flatpak, we use the HostCommand to execute our shell.
    //     if (internal_os.isFlatpak()) flatpak: {
    //         if (comptime !build_config.flatpak) {
    //             log.warn("flatpak detected, but flatpak support not built-in", .{});
    //             break :flatpak;
    //         }
    //
    //         // Flatpak command must have a stable pointer.
    //         self.process = .{ .flatpak = .{
    //             .argv = self.args,
    //             .cwd = cwd,
    //             .env = if (self.env) |*env| env else null,
    //             .stdin = pty.slave,
    //             .stdout = pty.slave,
    //             .stderr = pty.slave,
    //         } };
    //         var cmd = &self.process.?.flatpak;
    //         const pid = try cmd.spawn(alloc);
    //         errdefer killCommandFlatpak(cmd);
    //
    //         log.info("started subcommand on host via flatpak API path={s} pid={}", .{
    //             self.args[0],
    //             pid,
    //         });
    //
    //         return .{
    //             .read = pty.master,
    //             .write = pty.master,
    //         };
    //     }
    //
    //     // Build our subcommand
    //     var cmd: Command = .{
    //         .path = self.args[0],
    //         .args = self.args,
    //         .env = if (self.env) |*env| env else null,
    //         .cwd = cwd,
    //         .stdin = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
    //         .stdout = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
    //         .stderr = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
    //         .pseudo_console = if (builtin.os.tag == .windows) pty.pseudo_console else {},
    //         .os_pre_exec = switch (comptime builtin.os.tag) {
    //             .windows => null,
    //             else => f: {
    //                 const f = struct {
    //                     fn callback(cmd: *Command) ?u8 {
    //                         const sp = cmd.getData(Subprocess) orelse unreachable;
    //                         sp.childPreExec() catch |err| log.err(
    //                             "error initializing child: {}",
    //                             .{err},
    //                         );
    //                         return null;
    //                     }
    //                 };
    //                 break :f f.callback;
    //             },
    //         },
    //         .rt_pre_exec = if (comptime @hasDecl(apprt.runtime, "pre_exec")) apprt.runtime.pre_exec.preExec else null,
    //         .rt_pre_exec_info = self.rt_pre_exec_info,
    //         .rt_post_fork = if (comptime @hasDecl(apprt.runtime, "post_fork")) apprt.runtime.post_fork.postFork else null,
    //         .rt_post_fork_info = self.rt_post_fork_info,
    //         .data = self,
    //     };
    //
    //     cmd.start(alloc) catch |err| {
    //         // We have to do this because start on Windows can't
    //         // ever return ExecFailedInChild
    //         const StartError = error{ExecFailedInChild} || @TypeOf(err);
    //         switch (@as(StartError, err)) {
    //             // If we fail in our child we need to flag it so our
    //             // errdefers don't run.
    //             error.ExecFailedInChild => {
    //                 in_child = true;
    //                 return err;
    //             },
    //
    //             else => return err,
    //         }
    //     };
    //     errdefer killCommand(&cmd) catch |err| {
    //         log.warn("error killing command during cleanup err={}", .{err});
    //     };
    //     log.info("started subcommand path={s} pid={?}", .{ self.args[0], cmd.pid });
    //
    //     self.process = .{ .fork_exec = cmd };
    //     return switch (builtin.os.tag) {
    //         .windows => .{
    //             .read = pty.out_pipe,
    //             .write = pty.in_pipe,
    //         },
    //
    //         else => .{
    //             .read = pty.master,
    //             .write = pty.master,
    //         },
    //     };
    // }
    // pub fn start(self: *Subprocess, alloc: Allocator) !struct {
    //     read: Pty.Fd,
    //     write: Pty.Fd,
    // } {
    pub fn start(&mut self, alloc: Allocator) -> Result<PtyFds, crate::Error> {
        // The windows spawn half (out_pipe/in_pipe + pseudo_console) stays
        // deferred (D6 windows; verbatim in the reference block above); termio's
        // pty/PtyFds surface is posix-typed.
        #[cfg(windows)]
        {
            let _ = alloc;
            unimplemented!("termio.Exec.Subprocess.start windows spawn deferred (D6 windows); verbatim body above")
        }

        #[cfg(not(windows))]
        {
            // assert(self.pty == null and self.process == null);
            assert!(self.pty.is_none() && self.process.is_none());

            // This function is funny because on POSIX systems it can
            // fail in the forked process. This is flipped to true if
            // we're in an error state in the forked process (child
            // process).
            // var in_child: bool = false;
            let mut in_child = false;

            // Create our pty
            // var pty = try Pty.open(.{ .ws_row = @intCast(self.grid_size.rows), ... });
            let pty = Pty::open(ptypkg::winsize {
                // .ws_row = @intCast(self.grid_size.rows),
                ws_row: self.grid_size.rows,
                // .ws_col = @intCast(self.grid_size.columns),
                ws_col: self.grid_size.columns,
                // .ws_xpixel = @intCast(self.screen_size.width),
                ws_xpixel: self.screen_size.width.try_into().unwrap(),
                // .ws_ypixel = @intCast(self.screen_size.height),
                ws_ypixel: self.screen_size.height.try_into().unwrap(),
            })
            .map_err(|_| crate::Error)?;
            // self.pty = pty;
            // (PORT NOTE: the Rust `Pty` is not `Copy`; capture the slave/master fds
            // before the move — the reference's errdefer/defer read the local
            // copy's `pty.slave`/`pty.master`.)
            let slave = pty.slave;
            let master = pty.master;
            self.pty = Some(pty);
            // errdefer if (!in_child) { ... posix.close(pty.slave); pty.deinit();
            //     self.pty = null; ... };
            // defer if (!in_child and self.process != null) { ...
            //     posix.close(pty.slave); ... env.deinit(); self.env = null; ... };
            // (PORT NOTE: Rust has no defer/errdefer; both are modeled explicitly at
            // the exit points below, gated by `in_child` / `self.process`.)

            // (PORT NOTE: `rt_*_info` are trivially-copyable ZSTs; read once for both
            // the spawn Command and the stored pid-only handle.)
            let rt_pre = self.rt_pre_exec_info;
            let rt_post = self.rt_post_fork_info;

            // [ADV-FIX F4] derive the write-capable raw self-pointer BEFORE any
            // `&self.*` field borrow, so it and the shared `&self.env` are never
            // simultaneously live; it is dereferenced only in the forked child.
            let self_ptr: *mut Subprocess = std::ptr::addr_of_mut!(*self);

            // log.debug("starting command command={f}", .{ArgsFormatter{ .args = self.args }});

            // If we can't access the cwd, then don't set any cwd and inherit.
            // This is important because our cwd can be set by the shell (OSC 7)
            // and we don't want to break new windows.
            // const cwd: ?[:0]const u8 = if (self.cwd) |proposed| cwd: {
            let cwd: Option<&[u8]> = if let Some(proposed) = &self.cwd {
                // if ((comptime build_config.flatpak) and internal_os.isFlatpak()) {
                //     ... flatpak test-spawn to verify cwd access ...
                // }
                // (build_config.flatpak is const false -> this comptime-false
                // branch is compiled out; verbatim in the reference block above.)

                // if (std.fs.cwd().access(proposed, .{})) {
                //     break :cwd proposed;
                // } else |err| {
                //     log.warn("cannot access cwd, ignoring: {}", .{err});
                //     break :cwd null;
                // }
                if unsafe { libc::access(proposed.as_ptr(), libc::F_OK) } == 0 {
                    Some(proposed.to_bytes())
                } else {
                    // log.warn("cannot access cwd, ignoring: {}", .{err});
                    None
                }
            } else {
                None
            };

            // In flatpak, we use the HostCommand to execute our shell.
            // if (internal_os.isFlatpak()) flatpak: {
            if rustty_os::flatpak::isFlatpak() {
                // if (comptime !build_config.flatpak) {
                //     log.warn("flatpak detected, but flatpak support not built-in", .{});
                //     break :flatpak;
                // }
                // (build_config.flatpak is const false -> the comptime-true
                // branch: warn + break; the comptime-false HostCommand spawn is
                // compiled out — verbatim in the reference block above.)
            }

            // [ADV-FIX F3] the pty.slave File is NON-owning: `Command` reads only
            // `.as_raw_fd()` and never closes it, and the slave fd is closed
            // EXACTLY ONCE by the ported `posix.close(pty.slave)` below. A
            // dropping `File` here would double-close (fd-reuse race).
            let pty_slave_file = ManuallyDrop::new(unsafe { File::from_raw_fd(slave) });

            // (PORT NOTE: the transient argv VIEW over self.args (Vec<CString>) — a
            // stack local, valid for this frame only; nothing post-start reads
            // it. This is the Rust materialization of the Zig `self.args` slice.)
            let argv: Vec<&CStr> = self.args.iter().map(|c| c.as_c_str()).collect();
            // .env = if (self.env) |*env| env else null,
            let env_ref: Option<&EnvMap> = self.env.as_ref();

            // Build our subcommand
            // var cmd: Command = .{
            let mut cmd: Command = Command {
                // .path = self.args[0],
                path: argv[0],
                // .args = self.args,
                args: argv.as_slice(),
                // .env = if (self.env) |*env| env else null,
                env: env_ref,
                // .cwd = cwd,
                cwd,
                // .stdin = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
                stdin: Some(&*pty_slave_file),
                // .stdout = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
                stdout: Some(&*pty_slave_file),
                // .stderr = if (builtin.os.tag == .windows) null else .{ .handle = pty.slave },
                stderr: Some(&*pty_slave_file),
                // .pseudo_console = if (windows) pty.pseudo_console else {},
                // (posix: the field does not exist — cfg(windows) only.)
                // .os_pre_exec = switch (comptime builtin.os.tag) { .windows => null, else => ... },
                os_pre_exec: Some(subprocess_pre_exec),
                // .rt_pre_exec = if (comptime @hasDecl(apprt.runtime, "pre_exec")) apprt.runtime.pre_exec.preExec else null,
                // (@hasDecl is comptime-false in this build — realized only by the
                // GTK apprt, blueprint §1.)
                rt_pre_exec: None,
                // .rt_pre_exec_info = self.rt_pre_exec_info,
                rt_pre_exec_info: rt_pre,
                // .rt_post_fork = if (comptime @hasDecl(apprt.runtime, "post_fork")) apprt.runtime.post_fork.postFork else null,
                rt_post_fork: None,
                // .rt_post_fork_info = self.rt_post_fork_info,
                rt_post_fork_info: rt_post,
                // .data = self,
                data: Some(self_ptr.cast::<std::ffi::c_void>()),
                // (.pid is set by cmd.start.)
                pid: None,
            };

            // The scoped spawn: `cmd`'s transient self-borrows (argv/env/cwd/the
            // slave File) are read ONLY inside `cmd.start()`; `pid` is the sole
            // field alive past here.
            let result: Result<PtyFds, crate::Error> = 'spawn: {
                // cmd.start(alloc) catch |err| {
                //     const StartError = error{ExecFailedInChild} || @TypeOf(err);
                //     switch (@as(StartError, err)) {
                //         error.ExecFailedInChild => { in_child = true; return err; },
                //         else => return err,
                //     }
                // };
                match cmd.start(alloc) {
                    // If we fail in our child we need to flag it so our
                    // errdefers don't run.
                    Err(crate::command::Error::ExecFailedInChild) => {
                        in_child = true;
                        break 'spawn Err(crate::Error);
                    }
                    Err(_e) => break 'spawn Err(crate::Error),
                    Ok(()) => {}
                }
                // errdefer killCommand(&cmd) catch |err| {
                //     log.warn("error killing command during cleanup err={}", .{err});
                // };
                // (PORT NOTE: there is no reachable error return between here and the
                // success `return` below — the straight-line body never triggers
                // this errdefer — so no cleanup is registered for it.)
                // log.info("started subcommand path={s} pid={?}", .{ self.args[0], cmd.pid });

                // (the ONLY field alive past here)
                let pid = cmd.pid;

                // self.process = .{ .fork_exec = cmd };
                // (PORT NOTE §2: store a pid-only handle — the stored Command's
                // reference fields are TRUE 'static empties, so no
                // self-referential borrow can exist at rest.)
                self.process = Some(Process::ForkExec(Command {
                    path: c"",
                    args: &[],
                    env: None,
                    cwd: None,
                    stdin: None,
                    stdout: None,
                    stderr: None,
                    os_pre_exec: None,
                    rt_pre_exec: None,
                    rt_pre_exec_info: rt_pre,
                    rt_post_fork: None,
                    rt_post_fork_info: rt_post,
                    data: None,
                    pid,
                }));

                // return switch (builtin.os.tag) {
                //     else => .{ .read = pty.master, .write = pty.master },
                // };
                Ok(PtyFds {
                    read: master,
                    write: master,
                })
            };

            // If we are in the forked child and exec failed (`in_child`), the
            // reference propagates `error.ExecFailedInChild` up to
            // `Exec.threadEnter`, which does `posix.exit(1)`:
            //     if (err != error.ExecFailedInChild) return err;
            //     posix.exit(1);
            //
            // PORT NOTE ([ADV-FIX], xev Milestone B): the port's opaque
            // `crate::Error` (a unit struct, B1) cannot carry the
            // `ExecFailedInChild` discriminant across the `Subprocess::start`
            // boundary, so `threadEnter` cannot tell the child apart from a
            // genuine parent error. Because the forked child MUST NOT return into
            // the parent's continuation (it would run the event loop as a
            // duplicate process), the `posix.exit(1)` is performed HERE — the one
            // place `in_child` is known — BEFORE the parent-only errdefer/defer
            // cleanup (which `in_child` already guards). `posix.exit ->
            // crate::command::posix_exit` (the libc `exit(3)` shim — NOT
            // `std::process::exit`, whose `rt::cleanup()` locks/allocates and
            // is illegal in the forked child; T8). Behaviorally identical to
            // the reference: the child exits 1; the parent bubbles every other
            // error.
            if in_child {
                crate::command::posix_exit(1);
            }

            // The reference errdefer/defer, now that the scoped spawn resolved:
            match &result {
                // defer if (!in_child and self.process != null) { ... };
                Ok(_) => {
                    if !in_child {
                        // Once our subcommand is started we can close the slave
                        // side. This prevents the slave fd from being leaked to
                        // future children.
                        // _ = posix.close(pty.slave);
                        unsafe {
                            libc::close(slave);
                        }

                        // Successful start we can clear out some memory.
                        // if (self.env) |*env| { env.deinit(); self.env = null; }
                        self.env = None;
                    }
                }
                // errdefer if (!in_child) { ... }; (defer no-ops: process == null)
                Err(_) => {
                    if !in_child {
                        // _ = posix.close(pty.slave);
                        unsafe {
                            libc::close(slave);
                        }
                        // pty.deinit();
                        if let Some(p) = &mut self.pty {
                            p.deinit();
                        }
                        // self.pty = null;
                        self.pty = None;
                    }
                }
            }

            result
        }
    }

    /// This should be called after fork but before exec in the child process.
    /// To repeat: this function RUNS IN THE FORKED CHILD PROCESS before
    /// exec is called; it does NOT run in the main Ghostty process.
    //
    // fn childPreExec(self: *Subprocess) !void {
    //     // Setup our pty
    //     try self.pty.?.childPreExec();
    // }
    // (The only reference caller is `start`'s os_pre_exec closure — the
    // `subprocess_pre_exec` trampoline, live in `start`; on windows os_pre_exec
    // is null, so the fn is unused there.)
    #[cfg_attr(windows, allow(dead_code))]
    fn childPreExec(&mut self) -> Result<(), crate::Error> {
        // Setup our pty
        self.pty
            .as_mut()
            .unwrap()
            .childPreExec()
            .map_err(|_| crate::Error)
    }

    /// Called to notify that we exited externally so we can unset our
    /// running state.
    //
    // pub fn externalExit(self: *Subprocess) void {
    pub fn externalExit(&mut self) {
        self.process = None;
    }

    /// Stop the subprocess. This is safe to call anytime. This will wait
    /// for the subprocess to register that it has been signalled, but not
    /// for it to terminate, so it will not block.
    /// This does not close the pty.
    //
    // pub fn stop(self: *Subprocess) void {
    pub fn stop(&mut self) {
        // switch (self.process orelse return) {
        if let Some(process) = &mut self.process {
            match process {
                // .fork_exec => |*cmd| {
                Process::ForkExec(cmd) => {
                    // Note: this will also wait for the command to exit, so
                    // DO NOT call cmd.wait
                    // killCommand(cmd) catch |err|
                    //     log.err("error sending SIGHUP to command, may hang: {}", .{err});
                    if killCommand(cmd).is_err() {
                        // log.err("error sending SIGHUP to command, may hang: {}", .{err});
                    }
                }

                // .flatpak => |*cmd| if (comptime build_config.flatpak) {
                //     killCommandFlatpak(cmd) catch |err|
                //         log.err("error sending SIGHUP to command, may hang: {}", .{err});
                //     _ = cmd.wait() catch |err|
                //         log.err("error waiting for command to exit: {}", .{err});
                // },
                //
                // (PORT NOTE: `build_config.flatpak` is const false — the comptime-
                // false arm body is compiled out, exactly as Zig's lazy
                // compilation leaves it unanalyzed; verbatim above.)
                Process::Flatpak(_cmd) => {}
            }
        } else {
            return;
        }

        self.process = None;
    }

    /// Resize the pty subprocess. This is safe to call anytime.
    //
    // pub fn resize(
    //     self: *Subprocess,
    //     grid_size: renderer.GridSize,
    //     screen_size: renderer.ScreenSize,
    // ) !void {
    pub fn resize(
        &mut self,
        grid_size: GridSize,
        screen_size: ScreenSize,
    ) -> Result<(), crate::Error> {
        self.grid_size = grid_size;
        self.screen_size = screen_size;

        // if (self.pty) |*pty| {
        if let Some(pty) = &mut self.pty {
            // It is theoretically possible for the grid or screen size to
            // exceed u16, although the terminal in that case isn't very
            // usable. This should be protected upstream but we still clamp
            // in case there is a bad caller which has happened before.
            // try pty.setSize(.{
            //     .ws_row = std.math.cast(u16, grid_size.rows) orelse std.math.maxInt(u16),
            //     .ws_col = std.math.cast(u16, grid_size.columns) orelse std.math.maxInt(u16),
            //     .ws_xpixel = std.math.cast(u16, screen_size.width) orelse std.math.maxInt(u16),
            //     .ws_ypixel = std.math.cast(u16, screen_size.height) orelse std.math.maxInt(u16),
            // });
            //
            // (PORT NOTE: `GridSize`'s units are already u16 — those casts are
            // identities; the pixel casts clamp.)
            pty.setSize(ptypkg::winsize {
                ws_row: grid_size.rows,
                ws_col: grid_size.columns,
                ws_xpixel: u16::try_from(screen_size.width).unwrap_or(u16::MAX),
                ws_ypixel: u16::try_from(screen_size.height).unwrap_or(u16::MAX),
            })
            .map_err(|_| crate::Error)?;
        }

        Ok(())
    }

    /// Get information about the process(es) running within the subprocess.
    /// Returns `null` if there was an error getting the information or the
    /// information is not available on a particular platform.
    //
    // pub fn getProcessInfo(self: *Subprocess, comptime info: ProcessInfo) ?ProcessInfo.Type(info) {
    //     const pty = &(self.pty orelse return null);
    //     return pty.getProcessInfo(info);
    // }
    //
    // PORT NOTE: split into the two concrete accessors (rustty-os pty.rs
    // precedent; see `Exec::getForegroundPid`).
    pub fn getForegroundPid(&mut self) -> Option<u64> {
        let pty = self.pty.as_mut()?;
        pty.getForegroundPid()
    }

    /// See [`Subprocess::getForegroundPid`].
    pub fn getTtyName(&mut self) -> Option<&CStr> {
        let pty = self.pty.as_mut()?;
        pty.getTtyName()
    }
}

/// PORT NOTE (Rust-only glue): the anonymous return struct of
/// `Subprocess.start` (`struct { read: Pty.Fd, write: Pty.Fd }`).
pub struct PtyFds {
    pub read: ptypkg::PosixPtyFd,
    pub write: ptypkg::PosixPtyFd,
}

/// PORT NOTE (Rust-only glue): the error set of `killCommand`/`killPid` —
/// `error.KillFailed` + the windows `unexpectedError`.
#[derive(Debug)]
pub enum KillError {
    KillFailed,
    #[allow(dead_code)]
    Unexpected(i32),
}

/// Kill the underlying subprocess. This sends a SIGHUP to the child
/// process. This also waits for the command to exit and will return the
/// exit code.
//
// fn killCommand(command: *Command) !void {
//
// PORT NOTE: reference file-private fn nested in Subprocess; hoisted to module
// level (Rust impl blocks cannot host the windows/posix cfg split as cleanly).
fn killCommand(command: &mut Command<'static>) -> Result<(), KillError> {
    // if (command.pid) |pid| {
    if let Some(pid) = command.pid {
        // switch (builtin.os.tag) {
        //     .windows => {
        //         if (windows.kernel32.TerminateProcess(pid, 0) == 0) {
        //             return windows.unexpectedError(windows.kernel32.GetLastError());
        //         }
        //
        //         _ = try command.wait(false);
        //     },
        //
        //     else => try killPid(pid),
        // }
        #[cfg(windows)]
        {
            if unsafe { rustty_os::windows::exp::kernel32::TerminateProcess(pid, 0) } == 0 {
                return Err(KillError::Unexpected(unsafe {
                    rustty_os::windows::kernel32::GetLastError() as i32
                }));
            }

            let _ = command.wait(false);
        }
        #[cfg(not(windows))]
        killPid(pid)?;
    }
    Ok(())
}

// fn killPid(pid: c.pid_t) !void {
#[cfg(not(windows))]
fn killPid(pid: libc::pid_t) -> Result<(), KillError> {
    // const pgid = getpgid(pid) orelse return;
    let pgid = match getpgid(pid) {
        Some(pgid) => pgid,
        None => return Ok(()),
    };

    // It is possible to send a killpg between the time that
    // our child process calls setsid but before or simultaneous
    // to calling execve. In this case, the direct child dies
    // but grandchildren survive. To work around this, we loop
    // and repeatedly kill the process group until all
    // descendents are well and truly dead. We will not rest
    // until the entire family tree is obliterated.
    // while (true) {
    loop {
        // switch (posix.errno(c.killpg(pgid, c.SIGHUP))) {
        //     .SUCCESS => log.debug("process group killed pgid={}", .{pgid}),
        //     else => |err| killpg: { ... },
        // }
        let rc = unsafe { libc::killpg(pgid, libc::SIGHUP) };
        if rc == 0 {
            // log.debug("process group killed pgid={}", .{pgid});
        } else {
            'killpg: {
                let err = errno();
                // if ((comptime builtin.target.os.tag.isDarwin()) and
                //     err == .PERM)
                // {
                //     log.debug("killpg failed with EPERM, expected on Darwin and ignoring", .{});
                //     break :killpg;
                // }
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                if err == libc::EPERM {
                    // log.debug("killpg failed with EPERM, expected on Darwin and ignoring", .{});
                    break 'killpg;
                }

                // log.warn("error killing process group pgid={} err={}", .{ pgid, err });
                let _ = err;
                return Err(KillError::KillFailed);
            }
        }

        // See Command.zig wait for why we specify WNOHANG.
        // The gist is that it lets us detect when children
        // are still alive without blocking so that we can
        // kill them again.
        // const res = posix.waitpid(pid, std.c.W.NOHANG);
        let res = crate::command::posix_waitpid(pid, libc::WNOHANG as u32);
        // log.debug("waitpid result={}", .{res.pid});
        // if (res.pid != 0) break;
        if res.pid != 0 {
            break;
        }
        // std.Thread.sleep(10 * std.time.ns_per_ms);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Ok(())
}

// fn getpgid(pid: c.pid_t) ?c.pid_t {
#[cfg(not(windows))]
fn getpgid(pid: libc::pid_t) -> Option<libc::pid_t> {
    // Get our process group ID. Before the child pid calls setsid
    // the pgid will be ours because we forked it. Its possible that
    // we may be calling this before setsid if we are killing a surface
    // VERY quickly after starting it.
    // const my_pgid = c.getpgid(0);
    let my_pgid = unsafe { libc::getpgid(0) };

    // We loop while pgid == my_pgid. The expectation if we have a valid
    // pid is that setsid will eventually be called because it is the
    // FIRST thing the child process does and as far as I can tell,
    // setsid cannot fail. I'm sure that's not true, but I'd rather
    // have a bug reported than defensively program against it now.
    // while (true) {
    loop {
        // const pgid = c.getpgid(pid);
        let pgid = unsafe { libc::getpgid(pid) };
        // if (pgid == my_pgid) {
        if pgid == my_pgid {
            // log.warn("pgid is our own, retrying", .{});
            std::thread::sleep(std::time::Duration::from_millis(10));
            continue;
        }

        // Don't know why it would be zero but its not a valid pid
        // if (pgid == 0) return null;
        if pgid == 0 {
            return None;
        }

        // If the pid doesn't exist then... we're done!
        // if (pgid == c.ESRCH) return null;
        if pgid == libc::ESRCH {
            return None;
        }

        // If we have an error we're done.
        // if (pgid < 0) {
        //     log.warn("error getting pgid for kill", .{});
        //     return null;
        // }
        if pgid < 0 {
            // log.warn("error getting pgid for kill", .{});
            return None;
        }

        return Some(pgid);
    }
}

/// Kill the underlying process started via Flatpak host command.
/// This sends a signal via the Flatpak API.
//
// fn killCommandFlatpak(command: *FlatpakHostCommand) !void {
//     try command.signal(c.SIGHUP, true);
// }
//
// PORT NOTE: `build_config.flatpak` is const false, so the reference never
// instantiates this (Zig lazy compilation; its only callers sit in comptime-
// false branches). The verbatim body binds `internal_os.FlatpakHostCommand`'s
// signal API, which lands with the Linux/flatpak tier (DEFERRALS.md).
#[allow(dead_code)]
fn killCommandFlatpak(_command: &mut FlatpakHostCommand) -> Result<(), crate::Error> {
    unimplemented!("flatpak build-config branch (const false); verbatim body above")
}

/// The read thread sits in a loop doing the following pseudo code:
///
///   while (true) { blocking_read(); exit_if_eof(); process(); }
///
/// Almost all terminal-modifying activity is from the pty read, so
/// putting this on a dedicated thread keeps performance very predictable
/// while also almost optimal. "Locking is fast, lock contention is slow."
/// and since we rarely have contention, this is fast.
///
/// This is also empirically fast compared to putting the read into
/// an async mechanism like io_uring/epoll because the reads are generally
/// small.
///
/// We use a basic poll syscall here because we are only monitoring two
/// fds and this is still much faster and lower overhead than any async
/// mechanism.
//
// pub const ReadThread = struct {
pub struct ReadThread;

/// The spawn arguments for [`ReadThread::threadMainPosix`], wrapped so the
/// `!Send` `*mut Termio<M>` can cross the `std::thread::spawn` boundary.
//
// PORT NOTE (blueprint §3): the reference spawns `std.Thread.spawn(...,
// threadMainPosix, .{ pty_fds.read, io, pipe[0] })`. The `io: *mut Termio<M>`
// raw pointer is `!Send`, so — exactly the `IoThreadArgs` (surface.rs) precedent
// for the writer thread — a tiny newtype carries it with an `unsafe impl Send`.
// The contract: the read thread is the SOLE toucher of `*io` (for
// `processOutput`) between the spawn and `Exec::threadExit`'s `join()`; the io
// thread owns it before the spawn and after the join.
pub(crate) struct ReadThreadArgs<M: rustty_apprt_api::surface::AppMailbox>(
    pub std::os::fd::RawFd,
    pub *mut crate::Termio<M>,
    pub std::os::fd::RawFd,
);

// SAFETY: see the newtype's contract — the wrapped `*mut Termio<M>` has a single
// toucher (the read thread) for the window it is shared, matching the landed
// `IoThreadArgs` unsafe-Send discipline (surface.rs).
unsafe impl<M: rustty_apprt_api::surface::AppMailbox> Send for ReadThreadArgs<M> {}

impl ReadThread {
    // DEFERRED (D6 thread lifecycle, 0 tests — R1-safe): spawned only by the
    // deferred threadEnter; drives the deferred `Termio.processOutput` and the
    // crash-sentry thread_state (whose `surface` field reads the reference
    // Mailbox's surface pointer — the AppMailbox seam exposes it when core
    // lands). Preserved verbatim:
    //
    // fn threadMainPosix(fd: posix.fd_t, io: *termio.Termio, quit: posix.fd_t) void {
    //     // Always close our end of the pipe when we exit.
    //     defer posix.close(quit);
    //
    //     // Right now, on Darwin, `std.Thread.setName` can only name the current
    //     // thread, and we have no way to get the current thread from within it,
    //     // so instead we use this code to name the thread instead.
    //     if (builtin.os.tag.isDarwin()) {
    //         internal_os.macos.pthread_setname_np(&"io-reader".*);
    //     }
    //
    //     // Setup our crash metadata
    //     crash.sentry.thread_state = .{
    //         .type = .io,
    //         .surface = io.surface_mailbox.surface,
    //     };
    //     defer crash.sentry.thread_state = null;
    //
    //     // First thing, we want to set the fd to non-blocking. We do this
    //     // so that we can try to read from the fd in a tight loop and only
    //     // check the quit fd occasionally.
    //     if (posix.fcntl(fd, posix.F.GETFL, 0)) |flags| {
    //         _ = posix.fcntl(
    //             fd,
    //             posix.F.SETFL,
    //             flags | @as(u32, @bitCast(posix.O{ .NONBLOCK = true })),
    //         ) catch |err| {
    //             log.warn("read thread failed to set flags err={}", .{err});
    //             log.warn("this isn't a fatal error, but may cause performance issues", .{});
    //         };
    //     } else |err| {
    //         log.warn("read thread failed to get flags err={}", .{err});
    //         log.warn("this isn't a fatal error, but may cause performance issues", .{});
    //     }
    //
    //     // Build up the list of fds we're going to poll. We are looking
    //     // for data on the pty and our quit notification.
    //     var pollfds: [2]posix.pollfd = .{
    //         .{ .fd = fd, .events = posix.POLL.IN, .revents = undefined },
    //         .{ .fd = quit, .events = posix.POLL.IN, .revents = undefined },
    //     };
    //
    //     var buf: [1024]u8 = undefined;
    //     while (true) {
    //         // We try to read from the file descriptor as long as possible
    //         // to maximize performance. We only check the quit fd if the
    //         // main fd blocks. This optimizes for the realistic scenario that
    //         // the data will eventually stop while we're trying to quit. This
    //         // is always true because we kill the process.
    //         while (true) {
    //             const n = posix.read(fd, &buf) catch |err| {
    //                 switch (err) {
    //                     // This means our pty is closed. We're probably
    //                     // gracefully shutting down.
    //                     error.NotOpenForReading,
    //                     error.InputOutput,
    //                     => {
    //                         log.info("io reader exiting", .{});
    //                         return;
    //                     },
    //
    //                     // No more data, fall back to poll and check for
    //                     // exit conditions.
    //                     error.WouldBlock => break,
    //
    //                     else => {
    //                         log.err("io reader error err={}", .{err});
    //                         unreachable;
    //                     },
    //                 }
    //             };
    //
    //             // This happens on macOS instead of WouldBlock when the
    //             // child process dies. To be safe, we just break the loop
    //             // and let our poll happen.
    //             if (n == 0) break;
    //
    //             // log.info("DATA: {d}", .{n});
    //             @call(.always_inline, termio.Termio.processOutput, .{ io, buf[0..n] });
    //         }
    //
    //         // Wait for data.
    //         _ = posix.poll(&pollfds, -1) catch |err| {
    //             log.warn("poll failed on read thread, exiting early err={}", .{err});
    //             return;
    //         };
    //
    //         // If our quit fd is set, we're done.
    //         if (pollfds[1].revents & posix.POLL.IN != 0) {
    //             log.info("read thread got quit signal", .{});
    //             return;
    //         }
    //
    //         // If our pty fd is closed, then we're also done with our
    //         // read thread.
    //         if (pollfds[0].revents & posix.POLL.HUP != 0) {
    //             log.info("pty fd closed, read thread exiting", .{});
    //             return;
    //         }
    //     }
    // }
    // PORTED (xev B3, 0 reference tests — R1-safe): the real raw `poll(2)`/`read`/
    // `processOutput` read loop, below the preserved verbatim comment above.
    #[cfg(not(windows))]
    fn threadMainPosix<M: rustty_apprt_api::surface::AppMailbox>(
        fd: std::os::fd::RawFd,
        io: *mut crate::Termio<M>,
        quit: std::os::fd::RawFd,
    ) {
        // Always close our end of the pipe when we exit.
        // defer posix.close(quit);
        //
        // PORT NOTE: `defer` -> a Drop guard (the `CloseHandleGuard` precedent) so the
        // quit-pipe read end is closed on EVERY return path below.
        struct QuitFdCloser(std::os::fd::RawFd);
        impl Drop for QuitFdCloser {
            fn drop(&mut self) {
                // posix.close(quit);
                unsafe {
                    libc::close(self.0);
                }
            }
        }
        let _quit_closer = QuitFdCloser(quit);

        // Right now, on Darwin, `std.Thread.setName` can only name the current
        // thread, and we have no way to get the current thread from within it,
        // so instead we use this code to name the thread instead.
        // if (builtin.os.tag.isDarwin()) {
        //     internal_os.macos.pthread_setname_np(&"io-reader".*);
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        unsafe {
            rustty_os::macos::pthread_setname_np(c"io-reader".as_ptr());
        }

        // Setup our crash metadata
        // crash.sentry.thread_state = .{
        //     .type = .io,
        //     .surface = io.surface_mailbox.surface,
        // };
        // defer crash.sentry.thread_state = null;
        //
        // DEFERRED (cross-crate crash-sentry seam, not part of B3): the read
        // thread's crash metadata sets `thread_state.surface = io.surface_mailbox
        // .surface`, but the `AppMailbox` trait does not yet expose a `.surface`
        // accessor (it lands with rustty-core's `Surface: ThreadStateSurface`
        // impl — see `crash.sentry` + the ReadThread deferral note above).
        // Preserved verbatim above; wired when core lands.

        // First thing, we want to set the fd to non-blocking. We do this
        // so that we can try to read from the fd in a tight loop and only
        // check the quit fd occasionally.
        // if (posix.fcntl(fd, posix.F.GETFL, 0)) |flags| {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if flags != -1 {
            // _ = posix.fcntl(
            //     fd,
            //     posix.F.SETFL,
            //     flags | @as(u32, @bitCast(posix.O{ .NONBLOCK = true })),
            // ) catch |err| {
            //     log.warn("read thread failed to set flags err={}", .{err});
            //     log.warn("this isn't a fatal error, but may cause performance issues", .{});
            // };
            let rc = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
            if rc == -1 {
                // log.warn("read thread failed to set flags err={}", .{err});
                // log.warn("this isn't a fatal error, but may cause performance issues", .{});
            }
        } else {
            // log.warn("read thread failed to get flags err={}", .{err});
            // log.warn("this isn't a fatal error, but may cause performance issues", .{});
        }

        // Build up the list of fds we're going to poll. We are looking
        // for data on the pty and our quit notification.
        // var pollfds: [2]posix.pollfd = .{
        //     .{ .fd = fd, .events = posix.POLL.IN, .revents = undefined },
        //     .{ .fd = quit, .events = posix.POLL.IN, .revents = undefined },
        // };
        let mut pollfds: [libc::pollfd; 2] = [
            libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: quit,
                events: libc::POLLIN,
                revents: 0,
            },
        ];

        // var buf: [1024]u8 = undefined;
        let mut buf = [0u8; 1024];
        // while (true) {
        loop {
            // We try to read from the file descriptor as long as possible
            // to maximize performance. We only check the quit fd if the
            // main fd blocks. This optimizes for the realistic scenario that
            // the data will eventually stop while we're trying to quit. This
            // is always true because we kill the process.
            // while (true) {
            'tight: loop {
                // const n = posix.read(fd, &buf) catch |err| {
                //     switch (err) {
                //         // This means our pty is closed. We're probably
                //         // gracefully shutting down.
                //         error.NotOpenForReading,
                //         error.InputOutput,
                //         => { log.info("io reader exiting", .{}); return; },
                //         // No more data, fall back to poll and check for
                //         // exit conditions.
                //         error.WouldBlock => break,
                //         else => { log.err("io reader error err={}", .{err}); unreachable; },
                //     }
                // };
                //
                // (PORT NOTE: `posix.read` wraps `read(2)` and retries EINTR internally
                // (Zig std) — modeled by the inner retry loop; the Zig error set
                // maps EBADF -> NotOpenForReading, EIO -> InputOutput, EAGAIN ->
                // WouldBlock.)
                let n: usize = 'read: loop {
                    let rc = unsafe {
                        libc::read(fd, buf.as_mut_ptr().cast::<std::ffi::c_void>(), buf.len())
                    };
                    if rc >= 0 {
                        break 'read rc as usize;
                    }
                    let e = errno();
                    // (posix.read retries on EINTR.)
                    if e == libc::EINTR {
                        continue 'read;
                    }
                    // error.NotOpenForReading, error.InputOutput => graceful shutdown.
                    if e == libc::EBADF || e == libc::EIO {
                        // log.info("io reader exiting", .{});
                        return;
                    }
                    // error.WouldBlock => break (fall back to poll below).
                    if e == libc::EAGAIN {
                        break 'tight;
                    }
                    // else => { log.err("io reader error err={}", .{err}); unreachable; }
                    unreachable!("io reader error");
                };

                // This happens on macOS instead of WouldBlock when the
                // child process dies. To be safe, we just break the loop
                // and let our poll happen.
                // if (n == 0) break;
                if n == 0 {
                    break 'tight;
                }

                // log.info("DATA: {d}", .{n});
                // @call(.always_inline, termio.Termio.processOutput, .{ io, buf[0..n] });
                //
                // SAFETY: `io` is the read thread's sole `processOutput` toucher
                // (the `ReadThreadArgs` contract); its `renderer_state` is live for
                // the pointer's lifetime (joined in `threadExit` before teardown).
                unsafe {
                    crate::Termio::processOutput(io, &buf[..n]);
                }
            }

            // Wait for data.
            // _ = posix.poll(&pollfds, -1) catch |err| {
            //     log.warn("poll failed on read thread, exiting early err={}", .{err});
            //     return;
            // };
            let rc = unsafe { libc::poll(pollfds.as_mut_ptr(), pollfds.len() as libc::nfds_t, -1) };
            if rc < 0 {
                // log.warn("poll failed on read thread, exiting early err={}", .{err});
                return;
            }

            // If our quit fd is set, we're done.
            // if (pollfds[1].revents & posix.POLL.IN != 0) {
            //     log.info("read thread got quit signal", .{});
            //     return;
            // }
            if pollfds[1].revents & libc::POLLIN != 0 {
                // log.info("read thread got quit signal", .{});
                return;
            }

            // If our pty fd is closed, then we're also done with our
            // read thread.
            // if (pollfds[0].revents & posix.POLL.HUP != 0) {
            //     log.info("pty fd closed, read thread exiting", .{});
            //     return;
            // }
            if pollfds[0].revents & libc::POLLHUP != 0 {
                // log.info("pty fd closed, read thread exiting", .{});
                return;
            }
        }
    }

    // DEFERRED (D6 thread lifecycle, windows half). Preserved verbatim:
    //
    // fn threadMainWindows(fd: posix.fd_t, io: *termio.Termio, quit: posix.fd_t) void {
    //     // Always close our end of the pipe when we exit.
    //     defer posix.close(quit);
    //
    //     // Setup our crash metadata
    //     crash.sentry.thread_state = .{
    //         .type = .io,
    //         .surface = io.surface_mailbox.surface,
    //     };
    //     defer crash.sentry.thread_state = null;
    //
    //     var buf: [1024]u8 = undefined;
    //     while (true) {
    //         while (true) {
    //             var n: windows.DWORD = 0;
    //             if (windows.kernel32.ReadFile(fd, &buf, buf.len, &n, null) == 0) {
    //                 const err = windows.kernel32.GetLastError();
    //                 switch (err) {
    //                     // Check for a quit signal
    //                     .OPERATION_ABORTED => break,
    //
    //                     else => {
    //                         log.err("io reader error err={}", .{err});
    //                         unreachable;
    //                     },
    //                 }
    //             }
    //
    //             @call(.always_inline, termio.Termio.processOutput, .{ io, buf[0..n] });
    //         }
    //
    //         var quit_bytes: windows.DWORD = 0;
    //         if (windows.exp.kernel32.PeekNamedPipe(quit, null, 0, null, &quit_bytes, null) == 0) {
    //             const err = windows.kernel32.GetLastError();
    //             log.err("quit pipe reader error err={}", .{err});
    //             unreachable;
    //         }
    //
    //         if (quit_bytes > 0) {
    //             log.info("read thread got quit signal", .{});
    //             return;
    //         }
    //     }
    // }
    #[cfg(windows)]
    #[allow(dead_code)]
    fn threadMainWindows<M: rustty_apprt_api::surface::AppMailbox>(
        _fd: rustty_os::windows::HANDLE,
        _io: *mut crate::Termio<M>,
        _quit: rustty_os::windows::HANDLE,
    ) {
        unimplemented!(
            "termio.Exec.ReadThread.threadMainWindows deferred (D6); verbatim body above"
        )
    }
}

/// PORT NOTE (Rust-only glue): the `comptime passwdpkg: type` parameter of
/// `execCommand` — "expected to have a single function `get(Allocator)` that
/// returns a passwd entry" — modeled as a trait with an inferred-error-set
/// associated type (the tests substitute their own impls exactly as the
/// reference passes anonymous structs). The `Allocator` param collapses.
pub trait PasswdPkg {
    type Error;
    fn get() -> Result<rustty_os::passwd::Entry, Self::Error>;
}

/// The real `internal_os.passwd` package (the production argument).
pub struct OsPasswd;

impl PasswdPkg for OsPasswd {
    type Error = rustty_os::passwd::Error;
    fn get() -> Result<rustty_os::passwd::Entry, Self::Error> {
        rustty_os::passwd::get()
    }
}

/// Builds the argv array for the process we should exec for the
/// configured command. This isn't as straightforward as it seems since
/// we deal with shell-wrapping, macOS login shells, etc.
///
/// The passwdpkg comptime argument is expected to have a single function
/// `get(Allocator)` that returns a passwd entry. This is used by macOS
/// to determine the username and home directory for the login shell.
/// It is unused on other platforms.
///
/// Memory ownership:
///
/// The allocator should be an arena, since the returned value may or
/// may not be allocated and args may or may not be allocated (or copied).
/// Pointers in the return value may point to pointers in the command
/// struct.
//
// fn execCommand(
//     alloc: Allocator,
//     command: configpkg.Command,
//     comptime passwdpkg: type,
// ) (Allocator.Error || error{SystemError})![]const [:0]const u8 {
//
// PORT NOTE: the returned `[]const [:0]const u8` is the owned `Vec<CString>`
// (each element carries the `[:0]` sentinel; the arena collapses).
fn execCommand<P: PasswdPkg>(
    command: &configpkg::command::Command,
) -> Result<Vec<CString>, ExecCommandError> {
    // If we're on macOS, we have to use `login(1)` to get all of
    // the proper environment variables set, a login shell, and proper
    // hushlogin behavior.
    // if (comptime builtin.target.os.tag.isDarwin()) darwin: {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    'darwin: {
        // const passwd = passwdpkg.get(alloc) catch |err| {
        //     log.warn("failed to read passwd, not using a login shell err={}", .{err});
        //     break :darwin;
        // };
        let passwd = match P::get() {
            Ok(p) => p,
            Err(_err) => {
                // log.warn("failed to read passwd, not using a login shell err={}", .{err});
                break 'darwin;
            }
        };

        // const username = passwd.name orelse {
        //     log.warn("failed to get username, not using a login shell", .{});
        //     break :darwin;
        // };
        let username = match passwd.name {
            Some(n) => n,
            None => {
                // log.warn("failed to get username, not using a login shell", .{});
                break 'darwin;
            }
        };

        // const hush = if (passwd.home) |home| hush: { ... } else false;
        let hush = if let Some(home) = &passwd.home {
            'hush: {
                // var dir = std.fs.openDirAbsolute(home, .{}) catch |err| {
                //     log.warn(
                //         "failed to open home dir, not checking for hushlogin err={}",
                //         .{err},
                //     );
                //     break :hush false;
                // };
                // defer dir.close();
                let home_path = std::path::Path::new(os_str_from_bytes(home.to_bytes()));
                match std::fs::read_dir(home_path) {
                    Ok(dir) => drop(dir), // defer dir.close();
                    Err(_err) => {
                        // log.warn("failed to open home dir, not checking for hushlogin err={}", .{err});
                        break 'hush false;
                    }
                }

                // break :hush if (dir.access(".hushlogin", .{})) true else |_| false;
                std::fs::metadata(home_path.join(".hushlogin")).is_ok()
            }
        } else {
            false
        };

        // If we made it this far we're going to start building
        // the actual command.
        // var args: std.ArrayList([:0]const u8) = try .initCapacity(
        //     alloc,
        //
        //     // This capacity is chosen based on what we'd need to
        //     // execute a shell command (very common). We can/will
        //     // grow if necessary for a longer command (uncommon).
        //     9,
        // );
        // defer args.deinit(alloc);
        let mut args: Vec<CString> = Vec::with_capacity(9);

        // The reason for executing login this way is unclear. This
        // comment will attempt to explain but prepare for a truly
        // unhinged reality.
        //
        // The first major issue is that on macOS, a lot of users
        // put shell configurations in ~/.bash_profile instead of
        // ~/.bashrc (or equivalent for another shell). This file is only
        // loaded for a login shell so macOS users expect all their terminals
        // to be login shells. No other platform behaves this way and its
        // totally braindead but somehow the entire dev community on
        // macOS has cargo culted their way to this reality so we have to
        // do it...
        //
        // To get a login shell, you COULD just prepend argv0 with a `-`
        // but that doesn't fully work because `getlogin()` C API will
        // return the wrong value, SHELL won't be set, and various
        // other login behaviors that macOS users expect.
        //
        // The proper way is to use `login(1)`. But login(1) forces
        // the working directory to change to the home directory,
        // which we may not want. If we specify "-l" then we can avoid
        // this behavior but now the shell isn't a login shell.
        //
        // There is another issue: `login(1)` on macOS 14.3 and earlier
        // checked for ".hushlogin" in the working directory. This means
        // that if we specify "-l" then we won't get hushlogin honored
        // if its in the home directory (which is standard). To get
        // around this, we check for hushlogin ourselves and if present
        // specify the "-q" flag to login(1).
        //
        // So to get all the behaviors we want, we specify "-l" but
        // execute "bash" (which is built-in to macOS). We then use
        // the bash builtin "exec" to replace the process with a login
        // shell ("-l" on exec) with the command we really want.
        //
        // We use "bash" instead of other shells that ship with macOS
        // because as of macOS Sonoma, we found with a microbenchmark
        // that bash can `exec` into the desired command ~2x faster
        // than zsh.
        //
        // To figure out a lot of this logic I read the login.c
        // source code in the OSS distribution Apple provides for
        // macOS.
        //
        // Awesome.
        // try args.append(alloc, "/usr/bin/login");
        args.push(c"/usr/bin/login".to_owned());
        // if (hush) try args.append(alloc, "-q");
        if hush {
            args.push(c"-q".to_owned());
        }
        // try args.append(alloc, "-flp");
        args.push(c"-flp".to_owned());
        // try args.append(alloc, username);
        args.push(username);

        // switch (command) {
        match command {
            // Direct args can be passed directly to login, since
            // login uses execvp we don't need to worry about PATH
            // searching.
            // .direct => |v| try args.appendSlice(alloc, v),
            configpkg::command::Command::Direct(v) => {
                for arg in v {
                    args.push(cstring(arg));
                }
            }

            // .shell => |v| {
            configpkg::command::Command::Shell(v) => {
                // Use "exec" to replace the bash process with
                // our intended command so we don't have a parent
                // process hanging around.
                // const cmd = try std.fmt.allocPrintSentinel(
                //     alloc,
                //     "exec -l {s}",
                //     .{v},
                //     0,
                // );
                let mut cmd_bytes = b"exec -l ".to_vec();
                cmd_bytes.extend_from_slice(v);
                let cmd = cstring(&cmd_bytes);

                // We execute bash with "--noprofile --norc" so that it doesn't
                // load startup files so that (1) our shell integration doesn't
                // break and (2) user configuration doesn't mess this process
                // up.
                // try args.append(alloc, "/bin/bash");
                args.push(c"/bin/bash".to_owned());
                // try args.append(alloc, "--noprofile");
                args.push(c"--noprofile".to_owned());
                // try args.append(alloc, "--norc");
                args.push(c"--norc".to_owned());
                // try args.append(alloc, "-c");
                args.push(c"-c".to_owned());
                // try args.append(alloc, cmd);
                args.push(cmd);
            }
        }

        // return try args.toOwnedSlice(alloc);
        return Ok(args);
    }

    // return switch (command) {
    match command {
        // We need to clone the command since there's no guarantee the config remains valid.
        // .direct => |_| (try command.clone(alloc)).direct,
        configpkg::command::Command::Direct(_) => match command.clone() {
            configpkg::command::Command::Direct(v) => {
                Ok(v.iter().map(|arg| cstring(arg)).collect())
            }
            configpkg::command::Command::Shell(_) => unreachable!(),
        },

        // .shell => |v| shell: {
        configpkg::command::Command::Shell(v) => {
            // var args: std.ArrayList([:0]const u8) = try .initCapacity(alloc, 4);
            // defer args.deinit(alloc);
            let mut args: Vec<CString> = Vec::with_capacity(4);

            // if (comptime builtin.os.tag == .windows) {
            #[cfg(windows)]
            {
                // On Windows we run the shell value directly rather than
                // wrapping in `cmd.exe /C <shell>`. An intermediate cmd
                // process is wasteful for the common case (`wsl ~`,
                // `pwsh -NoLogo`, etc.) and has visible side effects
                // (extra process in the tree, per-process cmd AutoRun
                // state not reaching the user's actual shell).
                //
                // Values with arguments are split on whitespace. This
                // does not honor Windows CLI quoting rules; users who
                // need quoted arguments should use the direct command
                // form, which takes an argv array as-is.
                //
                // Note we don't free any of the memory below since it is
                // allocated in the arena.
                // if (std.mem.indexOfAny(u8, v, " \t") == null) {
                if !v.iter().any(|&b| b == b' ' || b == b'\t') {
                    // No arguments. If the shell is literally "cmd.exe"
                    // (the default), resolve via %COMSPEC% which is the
                    // documented path to the current command processor.
                    // Other values are passed as-is and resolved by
                    // `internal_os.path.expand` in Command.startWindows.
                    // const argv0 = if (std.ascii.eqlIgnoreCase(v, "cmd.exe"))
                    //     std.process.getEnvVarOwned(alloc, "COMSPEC") catch
                    //         try alloc.dupe(u8, v)
                    // else
                    //     try alloc.dupe(u8, v);
                    let argv0: Vec<u8> = if v.eq_ignore_ascii_case(b"cmd.exe") {
                        match std::env::var("COMSPEC") {
                            Ok(comspec) => comspec.into_bytes(),
                            Err(_) => v.clone(),
                        }
                    } else {
                        v.clone()
                    };
                    // try args.append(alloc, try alloc.dupeZ(u8, argv0));
                    args.push(cstring(&argv0));
                } else {
                    // var it = std.mem.tokenizeAny(u8, v, " \t");
                    // while (it.next()) |tok| {
                    //     try args.append(alloc, try alloc.dupeZ(u8, tok));
                    // }
                    for tok in v
                        .split(|&b| b == b' ' || b == b'\t')
                        .filter(|s| !s.is_empty())
                    {
                        args.push(cstring(tok));
                    }
                }
                // break :shell try args.toOwnedSlice(alloc);
                return Ok(args);
            }
            #[cfg(not(windows))]
            {
                // We run our shell wrapped in `/bin/sh` so that we don't have
                // to parse the command line ourselves if it has arguments.
                // Additionally, some environments (NixOS, I found) use /bin/sh
                // to setup some environment variables that are important to
                // have set.
                // try args.append(alloc, "/bin/sh");
                args.push(c"/bin/sh".to_owned());
                // if (internal_os.isFlatpak()) try args.append(alloc, "-l");
                if rustty_os::flatpak::isFlatpak() {
                    args.push(c"-l".to_owned());
                }
                // try args.append(alloc, "-c");
                args.push(c"-c".to_owned());
            }

            // try args.append(alloc, v);
            args.push(cstring(v));
            // break :shell try args.toOwnedSlice(alloc);
            Ok(args)
        }
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims (Rust-only glue).
// ---------------------------------------------------------------------------

/// PORT NOTE (Zig-std shim): `posix.errno(rc)` — the calling thread's errno value.
#[cfg(not(windows))]
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// PORT NOTE (Zig-std shim): `std.fs.path.dirname` — the path up to (not
/// including) the final separator; `None` for a root/relative-basename path.
fn dirname(path: &[u8]) -> Option<&[u8]> {
    let sep = std::path::MAIN_SEPARATOR as u8;
    let idx = path.iter().rposition(|&b| b == sep)?;
    if idx == 0 {
        // ("/x" -> dirname "/"; the reference callers only see deep paths.)
        return Some(&path[..1]);
    }
    Some(&path[..idx])
}

/// PORT NOTE (Zig-std shim): the `[:0]const u8` sentinel model — an owned
/// NUL-terminated copy of the byte slice (Command.zig `&CStr` convention).
/// Interior NULs cannot occur in the reference's `[:0]`-typed sources.
fn cstring(bytes: &[u8]) -> CString {
    CString::new(bytes).expect("[:0]const u8 payload cannot contain interior NUL")
}

/// Convert raw path bytes to an `OsStr` (termio_impl precedent).
#[cfg(unix)]
#[allow(dead_code)]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
#[allow(dead_code)]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_os::passwd::Entry as PasswdEntry;

    // Serialize the pty/fork-spawning KATs so two do not `fork()` concurrently in
    // the same test binary (a `fork` snapshots every open fd; concurrent
    // pty-spawning tests can leak each other's master fd into a child — the
    // parallel-cargo-test isolation lesson). Every KAT that spawns a real process
    // takes this lock for its duration. (T8: hoisted to the crate root so
    // `command::tests`' fork KATs share it — see the doc there.)
    #[cfg(target_os = "macos")]
    use crate::FORK_KAT_SERIAL;

    // test "execCommand darwin: shell command" {
    #[test]
    fn exec_command_darwin_shell_command() {
        // if (comptime !builtin.os.tag.isDarwin()) return error.SkipZigTest;
        // (PORT NOTE: cfg-gating the body is the SkipZigTest analogue — stage-3
        // Command / F3 DirectWrite precedent.)
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            // var arena = ArenaAllocator.init(testing.allocator);
            // defer arena.deinit();
            // const alloc = arena.allocator();

            // const result = try execCommand(alloc, .{ .shell = "foo bar baz" }, struct {
            //     fn get(_: Allocator) !PasswdEntry {
            //         return .{
            //             .name = "testuser",
            //         };
            //     }
            // });
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = std::convert::Infallible;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    Ok(PasswdEntry {
                        name: Some(c"testuser".to_owned()),
                        shell: None,
                        home: None,
                    })
                }
            }
            let result = execCommand::<TestPasswd>(&configpkg::command::Command::Shell(
                b"foo bar baz".to_vec(),
            ))
            .unwrap();

            // try testing.expectEqual(8, result.len);
            assert_eq!(8, result.len());
            // try testing.expectEqualStrings(result[0], "/usr/bin/login");
            assert_eq!(result[0].to_bytes(), b"/usr/bin/login");
            // try testing.expectEqualStrings(result[1], "-flp");
            assert_eq!(result[1].to_bytes(), b"-flp");
            // try testing.expectEqualStrings(result[2], "testuser");
            assert_eq!(result[2].to_bytes(), b"testuser");
            // try testing.expectEqualStrings(result[3], "/bin/bash");
            assert_eq!(result[3].to_bytes(), b"/bin/bash");
            // try testing.expectEqualStrings(result[4], "--noprofile");
            assert_eq!(result[4].to_bytes(), b"--noprofile");
            // try testing.expectEqualStrings(result[5], "--norc");
            assert_eq!(result[5].to_bytes(), b"--norc");
            // try testing.expectEqualStrings(result[6], "-c");
            assert_eq!(result[6].to_bytes(), b"-c");
            // try testing.expectEqualStrings(result[7], "exec -l foo bar baz");
            assert_eq!(result[7].to_bytes(), b"exec -l foo bar baz");
        }
    }

    // test "execCommand darwin: direct command" {
    #[test]
    fn exec_command_darwin_direct_command() {
        // if (comptime !builtin.os.tag.isDarwin()) return error.SkipZigTest;
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            // const result = try execCommand(alloc, .{ .direct = &.{
            //     "foo",
            //     "bar baz",
            // } }, struct {
            //     fn get(_: Allocator) !PasswdEntry {
            //         return .{
            //             .name = "testuser",
            //         };
            //     }
            // });
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = std::convert::Infallible;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    Ok(PasswdEntry {
                        name: Some(c"testuser".to_owned()),
                        shell: None,
                        home: None,
                    })
                }
            }
            let result = execCommand::<TestPasswd>(&configpkg::command::Command::Direct(vec![
                b"foo".to_vec(),
                b"bar baz".to_vec(),
            ]))
            .unwrap();

            // try testing.expectEqual(5, result.len);
            assert_eq!(5, result.len());
            // try testing.expectEqualStrings(result[0], "/usr/bin/login");
            assert_eq!(result[0].to_bytes(), b"/usr/bin/login");
            // try testing.expectEqualStrings(result[1], "-flp");
            assert_eq!(result[1].to_bytes(), b"-flp");
            // try testing.expectEqualStrings(result[2], "testuser");
            assert_eq!(result[2].to_bytes(), b"testuser");
            // try testing.expectEqualStrings(result[3], "foo");
            assert_eq!(result[3].to_bytes(), b"foo");
            // try testing.expectEqualStrings(result[4], "bar baz");
            assert_eq!(result[4].to_bytes(), b"bar baz");
        }
    }

    // test "execCommand: shell command, empty passwd" {
    #[test]
    fn exec_command_shell_command_empty_passwd() {
        // if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // const result = try execCommand(
            //     alloc,
            //     .{ .shell = "foo bar baz" },
            //     struct {
            //         fn get(_: Allocator) !PasswdEntry {
            //             // Empty passwd entry means we can't construct a macOS
            //             // login command and falls back to POSIX behavior.
            //             return .{};
            //         }
            //     },
            // );
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = std::convert::Infallible;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    // Empty passwd entry means we can't construct a macOS
                    // login command and falls back to POSIX behavior.
                    Ok(PasswdEntry::default())
                }
            }
            let result = execCommand::<TestPasswd>(&configpkg::command::Command::Shell(
                b"foo bar baz".to_vec(),
            ))
            .unwrap();

            // try testing.expectEqual(3, result.len);
            assert_eq!(3, result.len());
            // try testing.expectEqualStrings(result[0], "/bin/sh");
            assert_eq!(result[0].to_bytes(), b"/bin/sh");
            // try testing.expectEqualStrings(result[1], "-c");
            assert_eq!(result[1].to_bytes(), b"-c");
            // try testing.expectEqualStrings(result[2], "foo bar baz");
            assert_eq!(result[2].to_bytes(), b"foo bar baz");
        }
    }

    // test "execCommand: shell command, error passwd" {
    #[test]
    fn exec_command_shell_command_error_passwd() {
        // if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // const result = try execCommand(
            //     alloc,
            //     .{ .shell = "foo bar baz" },
            //     struct {
            //         fn get(_: Allocator) !PasswdEntry {
            //             // Failed passwd entry means we can't construct a macOS
            //             // login command and falls back to POSIX behavior.
            //             return error.Fail;
            //         }
            //     },
            // );
            struct Fail;
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = Fail;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    // Failed passwd entry means we can't construct a macOS
                    // login command and falls back to POSIX behavior.
                    Err(Fail) // return error.Fail;
                }
            }
            let result = execCommand::<TestPasswd>(&configpkg::command::Command::Shell(
                b"foo bar baz".to_vec(),
            ))
            .unwrap();

            // try testing.expectEqual(3, result.len);
            assert_eq!(3, result.len());
            // try testing.expectEqualStrings(result[0], "/bin/sh");
            assert_eq!(result[0].to_bytes(), b"/bin/sh");
            // try testing.expectEqualStrings(result[1], "-c");
            assert_eq!(result[1].to_bytes(), b"-c");
            // try testing.expectEqualStrings(result[2], "foo bar baz");
            assert_eq!(result[2].to_bytes(), b"foo bar baz");
        }
    }

    // test "execCommand: direct command, error passwd" {
    #[test]
    fn exec_command_direct_command_error_passwd() {
        // if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // const result = try execCommand(alloc, .{
            //     .direct = &.{
            //         "foo",
            //         "bar baz",
            //     },
            // }, struct {
            //     fn get(_: Allocator) !PasswdEntry {
            //         // Failed passwd entry means we can't construct a macOS
            //         // login command and falls back to POSIX behavior.
            //         return error.Fail;
            //     }
            // });
            struct Fail;
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = Fail;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    // Failed passwd entry means we can't construct a macOS
                    // login command and falls back to POSIX behavior.
                    Err(Fail)
                }
            }
            let result = execCommand::<TestPasswd>(&configpkg::command::Command::Direct(vec![
                b"foo".to_vec(),
                b"bar baz".to_vec(),
            ]))
            .unwrap();

            // try testing.expectEqual(2, result.len);
            assert_eq!(2, result.len());
            // try testing.expectEqualStrings(result[0], "foo");
            assert_eq!(result[0].to_bytes(), b"foo");
            // try testing.expectEqualStrings(result[1], "bar baz");
            assert_eq!(result[1].to_bytes(), b"bar baz");
        }
    }

    // test "execCommand: direct command, config freed" {
    #[test]
    fn exec_command_direct_command_config_freed() {
        // if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
        #[cfg(not(windows))]
        {
            // var command_arena = ArenaAllocator.init(testing.allocator);
            // const command_alloc = command_arena.allocator();
            // const command = try (configpkg.Command{
            //     .direct = &.{
            //         "foo",
            //         "bar baz",
            //     },
            // }).clone(command_alloc);
            let command =
                configpkg::command::Command::Direct(vec![b"foo".to_vec(), b"bar baz".to_vec()])
                    .clone();

            struct Fail;
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = Fail;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    // Failed passwd entry means we can't construct a macOS
                    // login command and falls back to POSIX behavior.
                    Err(Fail)
                }
            }
            // const result = try execCommand(alloc, command, struct { ... });
            let result = execCommand::<TestPasswd>(&command).unwrap();

            // command_arena.deinit();
            //
            // (PORT NOTE: the reference frees the source command BEFORE reading the
            // result, proving execCommand cloned it; Rust's drop is the same
            // proof.)
            drop(command);

            // try testing.expectEqual(2, result.len);
            assert_eq!(2, result.len());
            // try testing.expectEqualStrings(result[0], "foo");
            assert_eq!(result[0].to_bytes(), b"foo");
            // try testing.expectEqualStrings(result[1], "bar baz");
            assert_eq!(result[1].to_bytes(), b"bar baz");
        }
    }

    // test "execCommand windows: bare cmd.exe resolves via COMSPEC" {
    #[test]
    fn exec_command_windows_bare_cmd_exe_resolves_via_comspec() {
        // if (comptime builtin.os.tag != .windows) return error.SkipZigTest;
        #[cfg(windows)]
        {
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = std::convert::Infallible;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    Ok(PasswdEntry::default()) // return .{};
                }
            }
            // const result = try execCommand(alloc, .{ .shell = "cmd.exe" }, struct { ... });
            let result =
                execCommand::<TestPasswd>(&configpkg::command::Command::Shell(b"cmd.exe".to_vec()))
                    .unwrap();

            // try testing.expectEqual(1, result.len);
            assert_eq!(1, result.len());

            // Expect COMSPEC if available, otherwise the documented fallback.
            // const expected = std.process.getEnvVarOwned(alloc, "COMSPEC") catch
            //     try alloc.dupe(u8, "C:\\Windows\\System32\\cmd.exe");
            let expected = std::env::var("COMSPEC")
                .unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".to_string());
            // try testing.expectEqualStrings(expected, result[0]);
            assert_eq!(expected.as_bytes(), result[0].to_bytes());
        }
    }

    // test "execCommand windows: bare non-cmd shell is passed through" {
    #[test]
    fn exec_command_windows_bare_non_cmd_shell_is_passed_through() {
        // if (comptime builtin.os.tag != .windows) return error.SkipZigTest;
        #[cfg(windows)]
        {
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = std::convert::Infallible;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    Ok(PasswdEntry::default())
                }
            }
            // const result = try execCommand(alloc, .{ .shell = "pwsh.exe" }, struct { ... });
            let result = execCommand::<TestPasswd>(&configpkg::command::Command::Shell(
                b"pwsh.exe".to_vec(),
            ))
            .unwrap();

            // try testing.expectEqual(1, result.len);
            assert_eq!(1, result.len());
            // try testing.expectEqualStrings("pwsh.exe", result[0]);
            assert_eq!(b"pwsh.exe", result[0].to_bytes());
        }
    }

    // test "execCommand windows: shell with args is split on whitespace" {
    #[test]
    fn exec_command_windows_shell_with_args_is_split_on_whitespace() {
        // if (comptime builtin.os.tag != .windows) return error.SkipZigTest;
        #[cfg(windows)]
        {
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = std::convert::Infallible;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    Ok(PasswdEntry::default())
                }
            }
            // const result = try execCommand(alloc, .{ .shell = "wsl ~" }, struct { ... });
            let result =
                execCommand::<TestPasswd>(&configpkg::command::Command::Shell(b"wsl ~".to_vec()))
                    .unwrap();

            // try testing.expectEqual(2, result.len);
            assert_eq!(2, result.len());
            // try testing.expectEqualStrings("wsl", result[0]);
            assert_eq!(b"wsl", result[0].to_bytes());
            // try testing.expectEqualStrings("~", result[1]);
            assert_eq!(b"~", result[1].to_bytes());
        }
    }

    // test "execCommand windows: direct command is passed through unchanged" {
    #[test]
    fn exec_command_windows_direct_command_is_passed_through_unchanged() {
        // if (comptime builtin.os.tag != .windows) return error.SkipZigTest;
        #[cfg(windows)]
        {
            struct TestPasswd;
            impl PasswdPkg for TestPasswd {
                type Error = std::convert::Infallible;
                fn get() -> Result<PasswdEntry, Self::Error> {
                    Ok(PasswdEntry::default())
                }
            }
            // const result = try execCommand(alloc, .{ .direct = &.{
            //     "C:\\tools\\foo.exe",
            //     "arg with spaces",
            // } }, struct { ... });
            let result = execCommand::<TestPasswd>(&configpkg::command::Command::Direct(vec![
                b"C:\\tools\\foo.exe".to_vec(),
                b"arg with spaces".to_vec(),
            ]))
            .unwrap();

            // try testing.expectEqual(2, result.len);
            assert_eq!(2, result.len());
            // try testing.expectEqualStrings("C:\\tools\\foo.exe", result[0]);
            assert_eq!(b"C:\\tools\\foo.exe", result[0].to_bytes());
            // try testing.expectEqualStrings("arg with spaces", result[1]);
            assert_eq!(b"arg with spaces", result[1].to_bytes());
        }
    }

    // KAT (rustty-only margin — blueprint §7 B1; NOT a mapped Zig `test{}`, so
    // the test-count contract is unaffected). Drives the real init -> start
    // fork/exec spawn on a real pty and proves the pid-only handle model:
    //   Subprocess::init -> start "sh -c 'exit 7'" -> assert PtyFds returned +
    //   process is ForkExec(pid) with 'static-empty reference fields -> stop()
    //   reaps (SIGHUP to the child pgroup + waitpid).
    // Real fork/exec + openpty syscalls -> Miri-excluded (MIRI_EXCEPTIONS.md,
    // "rustty-termio B1"); the parent-side scoped-borrow spawn is the
    // Stacked-Borrows surface (no `&'static`-to-non-static at rest; the sole raw
    // self-pointer derefs only in the forked child).
    #[test]
    #[cfg(target_os = "macos")]
    fn subprocess_start_spawns_and_reaps() {
        use allocator_api2::alloc::Global;

        // Serialize with the other fork/pty KATs (see FORK_KAT_SERIAL).
        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        // Minimal Config: `sh -c 'exit 7'`, no resources dir (shell integration
        // disabled), no cwd, xterm-256color.
        let cfg = Config {
            command: Some(configpkg::command::Command::Shell(b"exit 7".to_vec())),
            env: EnvMap::new(),
            env_override: configpkg::RepeatableStringMap::default(),
            shell_integration: configpkg::value_types::ShellIntegration::Detect,
            shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: RtPreExecInfo,
            rt_post_fork_info: RtPostForkInfo,
        };

        let mut subprocess = Subprocess::init(Global, cfg).expect("Subprocess::init");

        // start() on a real pty: fork + exec the (login-wrapped) shell.
        let fds = subprocess.start(Global).expect("Subprocess::start");

        // PtyFds returned: read == write == the pty master (a valid fd).
        assert!(fds.read >= 0, "PtyFds carries a valid master fd");
        assert_eq!(fds.read, fds.write, "read/write are both the pty master");

        // [T8] Drain the pty master concurrently — the role the REAL termio
        // read thread plays during `threadExit` (which calls `stop()` while the
        // read thread is still draining; ghostty Exec.zig does the same).
        // Without a reader, a SIGHUP'd `login(1)` (raced mid-startup by
        // `stop()`) wedges UNINTERRUPTIBLY in kernel exit (state `E`,
        // SIGKILL-immune) draining its pending slave output to the master, and
        // `killPid`'s faithful `waitpid(WNOHANG)` loop then spins forever (an
        // E-state proc is not reapable) — the T8 CI hang. Proven on macOS
        // 26.5.1: reading ~37 pending bytes un-wedges the child instantly.
        // The drainer reads until EOF (all slave refs closed at reap), exactly
        // the upstream runtime condition; nothing this KAT asserts changed.
        let master_fd = fds.read;
        let drainer = std::thread::spawn(move || loop {
            let mut buf = [0u8; 4096];
            let n = unsafe { libc::read(master_fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            if n <= 0 {
                break;
            }
        });

        // process is a pid-bearing ForkExec handle whose reference fields are
        // TRUE 'static empties (the pid-only handle, blueprint §2).
        match &subprocess.process {
            Some(Process::ForkExec(cmd)) => {
                assert!(cmd.pid.is_some(), "ForkExec handle carries a pid");
                assert!(cmd.args.is_empty(), "stored args are the 'static empty");
                assert!(cmd.env.is_none(), "stored env is None (no dangling borrow)");
                assert!(cmd.data.is_none(), "stored data self-pointer is cleared");
            }
            _ => panic!("expected process == ForkExec after start"),
        }

        // stop() sends SIGHUP to the child's process group and reaps it (must
        // NOT hang — the child setsid'd in childPreExec so getpgid resolves).
        subprocess.stop();
        assert!(subprocess.process.is_none(), "stop() clears the process");

        // The child is reaped, so every slave fd is closed and the drainer has
        // hit EOF; join it BEFORE closing the master (its fd copy).
        drainer.join().expect("drainer join");

        // Close the master (read/write) fd so the test leaks nothing.
        unsafe {
            libc::close(fds.read);
        }

        // deinit is safe post-stop.
        subprocess.deinit();
    }

    // A trivial `AppMailbox` test double: it accepts every push (returns a
    // non-zero queue size) and records nothing. Enough for the exit / termios /
    // write callbacks, which only `push(...)` to it.
    #[cfg(target_os = "macos")]
    struct TestMailbox;

    #[cfg(target_os = "macos")]
    impl rustty_apprt_api::surface::AppMailbox for TestMailbox {
        type Surface = ();

        #[allow(clippy::result_large_err)]
        fn push_or_return(
            &self,
            _msg: rustty_apprt_api::surface::Message<'static>,
            _timeout: rustty_datastruct::blocking_queue::Timeout,
        ) -> Result<
            rustty_datastruct::blocking_queue::Size,
            rustty_apprt_api::surface::Message<'static>,
        > {
            // Non-zero => "accepted" (matches a live queue's push return).
            Ok(1)
        }
    }

    // A heap-pinned `Termio<TestMailbox>` test harness (blueprint §5 shape,
    // constructed by hand because `Termio::init` is deferred to B4). It carries a
    // live renderer `State` (guard-less zigstd mutex + a real owned `Terminal`), a
    // real `StreamHandler` whose `terminal`/`size`/`termio_mailbox` self-pointers
    // are re-seated at the FINAL heap address (the [ADV-FIX F1] discipline), real
    // termio + renderer mailboxes, and a real renderer-wakeup `Async`. The backend
    // `Exec` is a required field that the read path never touches (no subprocess is
    // started through it). Shared by the B2 write-path + B3 read-path KATs — both
    // now drive the REAL `Termio::processOutput` (B3) so `io` must be a valid,
    // pinned `Termio`, not a placeholder.
    #[cfg(target_os = "macos")]
    struct TestTermio {
        io: *mut crate::Termio<TestMailbox>,
        state: *mut rustty_renderer::state::State,
        mutex: *mut rustty_zigstd::thread::Mutex,
        renderer_mailbox: *mut rustty_renderer::thread::Mailbox,
        renderer_wakeup: xev::Async,
    }

    #[cfg(target_os = "macos")]
    impl TestTermio {
        fn build() -> TestTermio {
            use allocator_api2::alloc::Global;

            // The shared render State (heap-pinned; `terminal` re-seated after the
            // Termio is pinned). `mutex` is the guard-less zigstd lock.
            let mutex: *mut rustty_zigstd::thread::Mutex =
                Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
            let state: *mut rustty_renderer::state::State =
                Box::into_raw(Box::new(rustty_renderer::state::State {
                    mutex,
                    terminal: std::ptr::null_mut(),
                    inspector: None,
                    inspector_ptr: None,
                    preedit: None,
                    mouse: rustty_renderer::state::Mouse::default(),
                }));

            // A throwaway renderer mailbox (pushes queue; notify fails-safe) and a
            // real renderer-wakeup Async (Copy; shared by the handler + Termio).
            let renderer_mailbox: *mut rustty_renderer::thread::Mailbox =
                Box::into_raw(rustty_renderer::thread::Mailbox::create());
            let renderer_wakeup = xev::Async::init().expect("Async::init");

            // The owned Terminal (80x24).
            let terminal = Terminal::init(
                Global,
                rustty_vt::terminal::Options {
                    cols: 80,
                    rows: 24,
                    max_scrollback: 1000,
                    colors: rustty_vt::terminal::Colors::default(),
                    default_modes: rustty_vt::modes::ModePacked::default(),
                    kitty_image_storage_limit: 0,
                    kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
                },
            )
            .expect("Terminal::init");

            // DerivedConfig from a default Config (only the handler's cursor/OSC
            // fields are read from it; `config` itself is untouched by the read
            // path).
            let full_config = configpkg::Config::default();
            let derived = crate::termio_impl::DerivedConfig::init(&full_config);

            // Backend (Exec; a required Termio field, never used by the read path —
            // no subprocess is started through it).
            let exec_cfg = Config {
                command: Some(configpkg::command::Command::Direct(vec![
                    b"/bin/cat".to_vec()
                ])),
                env: EnvMap::new(),
                env_override: configpkg::RepeatableStringMap::default(),
                shell_integration: configpkg::value_types::ShellIntegration::Detect,
                shell_integration_features:
                    configpkg::value_types::ShellIntegrationFeatures::default(),
                cursor_blink: None,
                working_directory: None,
                resources_dir: None,
                term: b"xterm-256color".to_vec(),
                rt_pre_exec_info: RtPreExecInfo,
                rt_post_fork_info: RtPostForkInfo,
            };
            let backend =
                crate::backend::Backend::Exec(Exec::init(Global, exec_cfg).expect("Exec::init"));

            // The termio mailbox (SPSC; its own wakeup Async).
            let mailbox = crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC");

            // Size (a required field, unread by the read path).
            let size = rustty_renderer::size::Size {
                screen: rustty_renderer::size::ScreenSize {
                    width: 800,
                    height: 480,
                },
                cell: rustty_renderer::size::CellSize {
                    width: 10,
                    height: 20,
                },
                padding: rustty_renderer::size::Padding::default(),
            };

            // The StreamHandler. Its `terminal`/`size`/`termio_mailbox` are raw
            // pointers back into the owning Termio — null here, re-seated at the
            // FINAL address after the Termio is heap-pinned (the F1 discipline).
            let handler = crate::stream_handler::StreamHandler::<TestMailbox> {
                alloc: Global,
                size: std::ptr::null_mut(),
                terminal: std::ptr::null_mut(),
                termio_mailbox: std::ptr::null_mut(),
                surface_mailbox: TestMailbox,
                renderer_state: state,
                renderer_mailbox,
                renderer_wakeup,
                default_cursor: true,
                default_cursor_style: derived.cursor_style,
                default_cursor_blink: derived.cursor_blink,
                enquiry_response: derived.enquiry_response.clone(),
                osc_color_report_format: derived.osc_color_report_format,
                clipboard_write: derived.clipboard_write,
                apc: rustty_vt::apc::Handler::default(),
                dcs: rustty_vt::dcs::Handler::default(),
                tmux_viewer: None,
                termio_messaged: false,
                seen_title: false,
            };
            let terminal_stream = crate::stream_handler::Stream::init(handler);

            // Assemble + heap-pin the Termio.
            let io: *mut crate::Termio<TestMailbox> = Box::into_raw(Box::new(crate::Termio {
                alloc: Global,
                backend,
                config: derived,
                terminal,
                renderer_state: state,
                renderer_wakeup,
                renderer_mailbox,
                surface_mailbox: TestMailbox,
                size,
                mailbox,
                terminal_stream,
                last_cursor_reset: None,
                thread_enter_state: None,
            }));

            // Re-seat the self-referential pointers at the final address (F1).
            unsafe {
                (*io).terminal_stream.handler.terminal = std::ptr::addr_of_mut!((*io).terminal);
                (*io).terminal_stream.handler.size = std::ptr::addr_of_mut!((*io).size);
                (*io).terminal_stream.handler.termio_mailbox =
                    std::ptr::addr_of_mut!((*io).mailbox);
                (*state).terminal = std::ptr::addr_of_mut!((*io).terminal);
            }

            TestTermio {
                io,
                state,
                mutex,
                renderer_mailbox,
                renderer_wakeup,
            }
        }

        // The active Screen text, read under the renderer mutex (matching the read
        // thread's `processOutput` lock discipline).
        fn screen_dump(&self) -> Vec<u8> {
            use allocator_api2::alloc::Global;
            unsafe { (*self.mutex).lock() };
            let dump = unsafe {
                (*(*self.io).terminal.screens.active).dump_string_alloc(
                    Global,
                    rustty_vt::point::Point::Active(rustty_vt::point::Coordinate::default()),
                )
            };
            unsafe { (*self.mutex).unlock() };
            dump.unwrap_or_default()
        }

        // Deinit everything. MUST be called only after every thread touching `io`
        // is joined (nothing dereferences `io` anymore): moving the Termio out of
        // the box invalidates its self-pointers, which is sound only post-join.
        fn teardown(self) {
            let termio = unsafe { *Box::from_raw(self.io) };
            termio.deinit();
            unsafe {
                drop(Box::from_raw(self.state));
                drop(Box::from_raw(self.mutex));
                drop(Box::from_raw(self.renderer_mailbox));
            }
            let mut w = self.renderer_wakeup;
            w.deinit();
        }
    }

    // KAT (rustty-only margin — blueprint §7 B2; NOT a mapped Zig `test{}`, so the
    // test-count contract is unaffected). Drives the xev write path end-to-end and
    // (now that B3 wires the REAL read loop) the full round trip:
    //   Exec::init(/bin/cat) -> threadEnter (real fork/exec on a pty + xev
    //   Process::wait / Stream / Timer wiring + a REAL read-thread spawn) ->
    //   queueWrite a marker -> bounded loop.run(NoWait) pump -> the read thread
    //   reads cat's echo off the pty master and `Termio::processOutput` lands it in
    //   the terminal `Screen` -> assert the Screen shows the marker -> threadExit
    //   reaps + joins.
    //
    // Real fork/exec + openpty + kqueue syscalls -> Miri-excluded
    // (MIRI_EXCEPTIONS.md, "rustty-termio B2"). Bounded (NoWait passes + a
    // wall-clock deadline + a tick cap + explicit teardown), so it never blocks on
    // the child's life. The `io: *mut Termio` handed to threadEnter is now a REAL,
    // heap-pinned `Termio` (B3's read thread dereferences it for `processOutput`;
    // the B2 read-thread stub that ignored it is gone). We assert on the Screen
    // (the sink the read thread commits to) rather than reading the master directly
    // — the real read thread is the master's sole reader now.
    #[test]
    #[cfg(target_os = "macos")]
    fn threadenter_queuewrite_writes_to_pty() {
        use allocator_api2::alloc::Global;
        use std::time::{Duration, Instant};

        // Serialize with the other fork/pty KATs (see FORK_KAT_SERIAL).
        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        // Config: run `/bin/cat` directly (a deterministic pty passthrough), no
        // resources dir (shell integration disabled), xterm-256color.
        let cfg = Config {
            command: Some(configpkg::command::Command::Direct(vec![
                b"/bin/cat".to_vec()
            ])),
            env: EnvMap::new(),
            env_override: configpkg::RepeatableStringMap::default(),
            shell_integration: configpkg::value_types::ShellIntegration::Detect,
            shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: RtPreExecInfo,
            rt_post_fork_info: RtPostForkInfo,
        };
        let mut exec = Exec::init(Global, cfg).expect("Exec::init");

        // The event loop (created here, driven here — the KAT stands in for the
        // io thread's `threadMain_` frame).
        let mut loop_ = xev::Loop::init(xev::Options::default()).expect("Loop::init");
        let loop_ptr: *mut xev::Loop = std::ptr::addr_of_mut!(loop_);

        // A REAL, heap-pinned `Termio<TestMailbox>` for the `io` arg: the read
        // thread now dereferences it via `processOutput` and deposits cat's echo
        // into its terminal `Screen`.
        let t = TestTermio::build();
        let io_ptr = t.io;

        // The frame-pinned ThreadData. `backend` is a placeholder that
        // `threadEnter` overwrites (the reference's `backend = undefined`
        // out-parameter protocol); it holds a trivial already-finished thread +
        // inert fds so its drop (on the overwrite) is a no-op. `renderer_state`
        // points at the harness State (unused by cat's write path, but valid).
        let mut td: crate::termio_impl::ThreadData<TestMailbox> = crate::termio_impl::ThreadData {
            alloc: Global,
            loop_: loop_ptr,
            renderer_state: t.state,
            surface_mailbox: TestMailbox,
            backend: crate::backend::ThreadData::Exec(ThreadData {
                start: Instant::now(),
                exited: false,
                write_stream: xev::Stream::init_fd(-1),
                process: None,
                write_req_pool: SegmentedPool::new(0),
                write_buf_pool: SegmentedPool::new(0),
                write_queue: xev::WriteQueue::default(),
                process_wait_c: xev::Completion::default(),
                flatpak_wait_c: FlatpakHostCommandCompletion::default(),
                read_thread: std::thread::spawn(|| {}),
                read_thread_pipe: -1,
                read_thread_fd: -1,
                termios_timer: xev::Timer::init().expect("placeholder Timer::init"),
                termios_timer_c: xev::Completion::default(),
                termios_timer_running: false,
                termios_mode: ptypkg::Mode::default(),
            }),
            mailbox: std::ptr::null_mut(),
        };

        // Start the subprocess + wire the xev handles (+ spawn the REAL read
        // thread). SAFETY: `io_ptr` outlives the read thread (joined in threadExit);
        // `td` is frame-pinned for the loop's life.
        unsafe { exec.threadEnter(Global, io_ptr, &mut td) }.expect("threadEnter");

        // The pty master (parent side; both read + write ends of PtyFds). The read
        // thread is its sole reader now; we only keep it to close at teardown.
        let master: std::os::fd::RawFd = {
            let crate::backend::ThreadData::Exec(e) = &td.backend;
            e.read_thread_fd
        };

        // Queue a marker write (goes: direct queueWrite -> chunk into the write
        // pools -> Stream::queue_write -> loop). cat echoes it back on the pty; the
        // read thread reads it and `processOutput` commits it to the Screen.
        const MARK: &[u8] = b"RUSTTY_B2_MARK\n";
        exec.queueWrite(Global, &mut td, MARK, false)
            .expect("queueWrite");

        // Bounded pump: NoWait ticks (to flush the write) + poll the Screen + a
        // wall-clock deadline.
        let needle = b"RUSTTY_B2_MARK";
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut found = false;
        for _ in 0..2000 {
            loop_.run(xev::RunMode::NoWait).expect("loop.run(NoWait)");

            let dump = t.screen_dump();
            if dump.windows(needle.len()).any(|w| w == needle) {
                found = true;
                break;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        assert!(
            found,
            "queued write did not round-trip the pty into the Screen within the deadline; screen saw: {:?}",
            String::from_utf8_lossy(&t.screen_dump())
        );

        // Teardown: reap cat + write the quit pipe + join the read thread (so
        // nothing dereferences `io` after this).
        exec.threadExit(&mut td);
        // Close the pty master (never closed by deinit) and free the ThreadData.
        unsafe {
            libc::close(master);
        }
        td.deinit();
        loop_.deinit();
        exec.deinit();
        // The read thread is joined; deinit the harness Termio last.
        t.teardown();
    }

    // KAT (rustty-only margin — blueprint §7 B3; NOT a mapped Zig `test{}`, so the
    // test-count contract is unaffected). Drives the B3 read path end-to-end,
    // WITHOUT a shell: a real `ReadThread::threadMainPosix` poll/read loop over a
    // real pty delivers a byte string fed to the slave into the terminal `Screen`
    // via the [ADV-FIX F2] raw-`*mut` `Termio::processOutput`/`processOutputLocked`.
    //
    //   build a heap-pinned Termio<TestMailbox> (its `StreamHandler` self-pointers
    //   re-seated at the final address, per F1) -> openpty -> spawn the read thread
    //   on the master -> write "RUSTTY_B3_READPATH" to the slave -> bounded poll of
    //   the active Screen under the renderer mutex -> quit-pipe teardown + join.
    //
    // Real openpty + poll/read syscalls + a spawned thread -> Miri-excluded
    // (MIRI_EXCEPTIONS.md, "rustty-termio B3"). Bounded (tick cap + a wall-clock
    // deadline + explicit teardown), so it never blocks. No fork here, but it holds
    // open pty fds while other in-process KATs may `fork()`, so it takes
    // FORK_KAT_SERIAL (a concurrent fork would snapshot these fds).
    #[test]
    #[cfg(target_os = "macos")]
    fn readthread_pty_bytes_land_in_screen() {
        use std::time::{Duration, Instant};

        // Serialize with the other fork/pty KATs (see FORK_KAT_SERIAL).
        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        // A real, heap-pinned Termio<TestMailbox> whose read path the read thread
        // drives via `processOutput`.
        let t = TestTermio::build();
        let io = t.io;

        // --- A real pty: the read thread reads the master; the test writes the
        //     slave (which appears on the master). ---
        let pty = Pty::open(ptypkg::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 800,
            ws_ypixel: 480,
        })
        .expect("Pty::open");
        let master = pty.master;
        let slave = pty.slave;

        // --- The quit pipe (read end -> read thread; write end -> teardown). ---
        let mut pipefds = [0 as std::os::fd::RawFd; 2];
        assert_eq!(unsafe { libc::pipe(pipefds.as_mut_ptr()) }, 0, "pipe()");
        let quit_read = pipefds[0];
        let quit_write = pipefds[1];

        // --- Spawn the read thread (the raw `io` ptr crosses via ReadThreadArgs). ---
        let read_args = ReadThreadArgs::<TestMailbox>(master, io, quit_read);
        let handle = std::thread::spawn(move || {
            let args = read_args;
            ReadThread::threadMainPosix::<TestMailbox>(args.0, args.1, args.2);
        });

        // --- Feed the pty: write the needle to the slave (no newline -> no OPOST
        //     translation). It appears on the master, which the read thread reads. ---
        const NEEDLE: &[u8] = b"RUSTTY_B3_READPATH";
        let w = unsafe {
            libc::write(
                slave,
                NEEDLE.as_ptr().cast::<std::ffi::c_void>(),
                NEEDLE.len(),
            )
        };
        assert_eq!(
            w,
            NEEDLE.len() as isize,
            "wrote the full needle to the pty slave"
        );

        // --- Bounded poll of the active Screen (under the renderer mutex, via the
        //     harness `screen_dump`). ---
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut found = false;
        for _ in 0..3000 {
            let dump = t.screen_dump();
            if dump.windows(NEEDLE.len()).any(|c| c == NEEDLE) {
                found = true;
                break;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        // --- Teardown: signal quit, join the read thread (which closes quit_read
        //     via its Drop guard), then close the remaining fds + deinit the Termio
        //     (safe now that the read thread — the sole `io` toucher — is joined). ---
        let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
        handle.join().expect("read thread join");
        unsafe {
            libc::close(quit_write);
            libc::close(master);
            libc::close(slave);
        }
        t.teardown();

        assert!(
            found,
            "pty bytes did not land in the terminal Screen within the deadline"
        );
    }
}
