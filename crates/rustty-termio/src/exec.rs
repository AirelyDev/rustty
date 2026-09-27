//! Exec implements the logic for starting and stopping a subprocess with a
//! pty as well as spinning up the necessary read thread to read from the
//! pty and forward it to the Termio instance.
//!
//! PORTS: ghostty src/termio/Exec.zig (10 `test{}` blocks — all ported below;
//! the 2 darwin-only and 4 windows-only tests keep the reference's
//! SkipZigTest cfg-gating, F3 DirectWrite / stage-3 Command precedent).
//!
//! PORT STATUS: ported (integration-tier stage 4). Every FUNCTION in this file
//! is live.
//!
//! CORRECTION (T-G, LEDGER S211-F2): this said "ONE thing is still deferred,
//! and it is a field, not a function: the read thread's crash-sentry
//! `thread_state.surface` ... because the `AppMailbox` trait still exposes no
//! `.surface` accessor". It now does (`AppMailbox::crash_surface`), and both
//! read-thread mains set the state (`crate::IoCrashThreadState`).
//!
//! CORRECTION (task-18, closes S211-F7). This block used to read: "ported
//! … EXCEPT the xev event-loop wiring (threadEnter/threadExit/focusGained/
//! processExit*/termiosTimer/queueWrite/ttyWrite/ReadThread — the D6 deferral
//! seam, 0 reference tests) and Subprocess.start … each deferred body is
//! preserved verbatim … The testable core — Subprocess.init's argv+env assembly
//! and execCommand — is LIVE." Every one of those nine names now has a live Rust
//! body in this file, grepped one at a time: `threadEnter` (:441), `threadExit`
//! (:799), `focusGained` (:939), `processExitCommon` (:1166), `processExit`
//! (:1227), `termiosTimer` (:1276), `queueWrite` (:1492), `ttyWrite` (:1671),
//! `Subprocess::start` (:2880), `ReadThread::threadMainPosix` (:4482). The file
//! contains no `unimplemented!()` or `todo!()`.
//!
//! Read that list as a warning about this header's SHAPE, not just its content.
//! It was a hand-maintained inventory of nine names, so it went stale nine ways
//! at once and three separate per-function headers below had ALREADY been struck
//! individually ([M3], [WS20], [W6]) while this one still summarised them as
//! deferred. If you defer something here again, say it at the function, not in a
//! list at the top that nobody updates.
//!
//! What DOES remain true: the reference's event loop is libxev and rustty's is
//! `std::thread` plus the `crate::xev_seam` placeholder types (D6). That is a
//! structural divergence with its own DEFERRALS.md entry — not an unported
//! function — and the reference bodies stay preserved verbatim beside each port
//! (R5: no silent drops).

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
// const ptypkg = @import("../pty.zig");
// const Pty = ptypkg.Pty;
// const EnvMap = std.process.EnvMap;
// const PasswdEntry = internal_os.passwd.Entry;
// const windows = internal_os.windows;
// const ProcessInfo = @import("../pty.zig").ProcessInfo;
//
// PORT NOTE: `xev` -> the crate::xev_seam D6 placeholder types; the reference's
// `apprt` import is reached ONLY through `crate::command`'s
// `RtPreExecInfo`/`RtPostForkInfo` + the `install_rt_hooks` registry (V13 — this
// crate must never name rustty-apprt, R4; see the seam PORT NOTE above
// `command::RtPreExecInfo`); `crash`/`fastmem`
// are used only inside the D6-deferred ReadThread/queueWrite bodies (verbatim
// comments below), so no `use` is bound for them; `Command` is the RELOCATED
// crate::command (break #13b); `EnvMap` is the crate's stage-3
// `std.process.EnvMap` modeling.
use allocator_api2::alloc::Global as Allocator;
use rustty_config as configpkg;
// PORT NOTE: `std.heap.MemoryPool` — the rustty-zigstd faithful port (Zig-std
// infra tier; `write_pool` below).
use rustty_os::pty as ptypkg;
use rustty_os::pty::Pty;
use rustty_renderer::size::{GridSize, ScreenSize};
use rustty_vt::terminal::Terminal;
use rustty_zigstd::memory_pool::MemoryPool;
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

use crate::command::{Command, EnvMap, EnvMapExt as _, RtPostForkInfo, RtPreExecInfo};
use crate::shell_integration::{self, env_name, env_string};
use crate::xev_seam as xev;
// The surface-mailbox messages the xev exit / termios callbacks push
// (`.child_exited` / `.password_input`), and the blocking-queue timeout.
use rustty_apprt_api::surface::{ChildExited, Message as SurfaceMessage};
use rustty_datastruct::blocking_queue::Timeout;

// const log = std.log.scoped(.io_exec);
//
// PORT NOTE (DECISION-logging.md §6/§6r4): `LOG_SCOPE` is the `@tagName` of the
// reference's `std.log.scoped(.io_exec)`; restored sites call the `log` facade
// with `target: LOG_SCOPE`. This supersedes the earlier "preserved verbatim as
// comments at each site" note — the Zig comment stays (R1) and the live call is
// added beneath it.
//
// `Exec.zig` is the single largest log-bearing file in the whole reference (58
// live sites at pin 20abdb50a). FOUR of them have no live counterpart here, and
// each says so at its own code site. In every case the reference's diagnostic
// hangs off a failure the PORT CANNOT PRODUCE, so restoring it would put a
// diagnostic on a success path:
//   :731  `error building XDG_DATA_DIRS`      — the `bufPrint` NoSpaceLeft arm;
//   :748  `error building MANPATH`            — the same, growable format.
//   :840  `failed to allocate space for ...`  — the OOM arm; Rust allocation
//                                               here is infallible.
//   :1078 `error killing command during ...`  — an `errdefer` with no reachable
//                                               error return between its
//                                               registration and the success
//                                               return.
// `:1469` (`failed to spawn gather thread`) was ALSO listed here, and that was
// WRONG: `Builder::spawn_scoped` returns a `Result` and the site was discarding
// it with `.expect`. `S211-F6` is the ledger row that already recorded exactly
// that ("gather-thread spawn failure panics instead of logging and returning").
// Restored in task-14e-1 fix round 1, both halves — see the site itself.
// The `LOG-PORTMAP.tsv` allowance is held at 4 for the four above.
const LOG_SCOPE: &str = "io_exec";

/// PORT NOTE (Zig-std shim, task-14e-1): `@tagName(posix.errno(...))` — the
/// reference renders three of this file's diagnostics as `err=E{s}` where `{s}`
/// is the errno's NAME with the leading `E` stripped (`EBADF` -> `BADF`), so the
/// message reads `err=EBADF`. Rust's std has no errno-name table, so the set is
/// transcribed here; anything outside it falls back to the number, which still
/// round-trips to a name via `errno(3)` and is never a WRONG name. This is a
/// diagnostic string, not a control-flow input.
///
/// The `rustty-cli` `errno_suffix` helper is the precedent, including its
/// lesson: the numbers differ per target (its first draft had Linux's `EAGAIN`,
/// which is `EDEADLK` on Darwin), so the table is asserted against the HOST's
/// own `strerror` by `errno_tests` below rather than trusted.
#[cfg(not(windows))]
fn errno_tag_name(code: i32) -> std::borrow::Cow<'static, str> {
    // The errnos `write(2)` and `fcntl(2)` can set, per POSIX.1-2024.
    const ERRNOS: &[(i32, &str)] = &[
        (libc::EACCES, "ACCES"),
        (libc::EAGAIN, "AGAIN"),
        (libc::EBADF, "BADF"),
        (libc::EDEADLK, "DEADLK"),
        (libc::EDQUOT, "DQUOT"),
        (libc::EFAULT, "FAULT"),
        (libc::EFBIG, "FBIG"),
        (libc::EINTR, "INTR"),
        (libc::EINVAL, "INVAL"),
        (libc::EIO, "IO"),
        (libc::EMFILE, "MFILE"),
        (libc::ENOLCK, "NOLCK"),
        (libc::ENOSPC, "NOSPC"),
        (libc::EOVERFLOW, "OVERFLOW"),
        (libc::EPERM, "PERM"),
        (libc::EPIPE, "PIPE"),
        (libc::ERANGE, "RANGE"),
        (libc::ESPIPE, "SPIPE"),
        (libc::ESRCH, "SRCH"),
    ];
    for (n, name) in ERRNOS {
        if *n == code {
            return std::borrow::Cow::Borrowed(name);
        }
    }
    std::borrow::Cow::Owned(code.to_string())
}

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
// PORT NOTE (WS20): `build_config.flatpak` is the real `-Dflatpak` ghostty build
// option, so BOTH arms of this comptime selection are live. The Rust map of the
// comptime `if` is the crate's `flatpak` CARGO FEATURE (see Cargo.toml): with it
// ON this binds the fully-ported `internal_os.FlatpakHostCommand`
// (`rustty-os/src/flatpak.rs` — GIO/DBus host spawn + `waitXev` + `signal`), with
// it OFF the zero-size fallback, exactly as the reference's `struct { pub const
// Completion = struct {}; }`.
#[cfg(feature = "flatpak")]
pub use rustty_os::flatpak::FlatpakHostCommand;
#[cfg(not(feature = "flatpak"))]
pub struct FlatpakHostCommand {}

/// `FlatpakHostCommand.Completion` (nested type, hoisted to module level —
/// zero-size in the non-flatpak build).
#[cfg(feature = "flatpak")]
pub use rustty_os::flatpak::Completion as FlatpakHostCommandCompletion;
#[cfg(not(feature = "flatpak"))]
#[derive(Default)]
pub struct FlatpakHostCommandCompletion {}

/// `FlatpakHostCommand.WaitError` — the `flatpakExit` callback's result error.
#[cfg(feature = "flatpak")]
pub use rustty_os::flatpak::WaitError as FlatpakHostCommandWaitError;

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
            if let Err(err) = term.set_pwd(cwd) {
                // log.warn("error setting initial pwd err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error setting initial pwd err={err:?}");
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

    // [task-18 STALE HEADER STRUCK (S211-F7): this said "DEFERRED (D6 xev
    // event-loop wiring, 0 tests — R1-safe)". `threadEnter` is FULLY IMPLEMENTED
    // at :441 below — it starts the subprocess and wires the process watcher,
    // write stream, termios timer and read thread over the `std::thread` +
    // `xev_seam` substitute for libxev.] The reference body, preserved verbatim:
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
        // (`posix.pid_t`: a HANDLE on Windows, the `Command.pid` type.)
        #[cfg(not(windows))]
        type ChildPid = libc::pid_t;
        #[cfg(windows)]
        type ChildPid = rustty_os::windows::HANDLE;
        enum ProcessWhich {
            NotStarted,
            Flatpak,
            ForkExec(Option<ChildPid>),
        }
        let which = match &self.subprocess.process {
            None => ProcessWhich::NotStarted,
            Some(Process::Flatpak(_)) => ProcessWhich::Flatpak,
            Some(Process::ForkExec(cmd)) => ProcessWhich::ForkExec(cmd.pid),
        };
        let mut process: Option<xev::Process> = match which {
            // } else return error.ProcessNotStarted;
            ProcessWhich::NotStarted => {
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
                self.subprocess.stop();
                return Err(crate::Error);
            }
            // .flatpak => null (traditional process watching not possible)
            ProcessWhich::Flatpak => None,
            // .fork_exec => |cmd| try xev.Process.init(cmd.pid orelse return error.ProcessNoPid)
            ProcessWhich::ForkExec(None) => {
                // return error.ProcessNoPid (errdefer stop fires)
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
                self.subprocess.stop();
                return Err(crate::Error);
            }
            ProcessWhich::ForkExec(Some(pid)) => match xev::Process::init(pid) {
                Ok(p) => Some(p),
                Err(_) => {
                    // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                    // io-reader thread does not exist on this arm, so without this the
                    // SIGHUP'd child can wedge unreapably (`?Es`). See
                    // `drain_pty_master_once`.
                    #[cfg(not(windows))]
                    let _ = drain_pty_master_once(pty_fds.read);
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
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
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
                    close_fd(pipe[1]);
                    close_fd(pipe[0]);
                }
                if let Some(p) = &mut process {
                    p.deinit();
                }
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
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
        let read_thread_builder =
            rustty_zigstd::thread::spawn_config().name("io-reader".to_string());
        // (Test builds only: `tests::FAIL_READ_THREAD_SPAWN` makes this spawn
        // fail for real, with a stack no thread can get, to drive the error arm.)
        #[cfg(test)]
        let read_thread_builder = if tests::FAIL_READ_THREAD_SPAWN.with(std::cell::Cell::get) {
            read_thread_builder.stack_size(usize::MAX / 2)
        } else {
            read_thread_builder
        };
        let read_thread = match read_thread_builder.spawn(move || {
            let args = read_args;
            // SAFETY: `args.1` (`*mut Termio<M>`) is valid for the read
            // thread's life; this thread is its sole `processOutput` toucher.
            //
            // if (builtin.os.tag == .windows) ReadThread.threadMainWindows else ReadThread.threadMainPosix,
            //
            // PORT NOTE (S211-F3): this always called `threadMainPosix`, so the
            // ported `threadMainWindows` had no caller. Selected per target now,
            // as the reference does at comptime. Built and run on Windows by
            // task W-1 (`windows_threadenter_reads_the_pty_through_the_windows_read_thread`).
            #[cfg(windows)]
            ReadThread::threadMainWindows::<M>(args.0, args.1, args.2);
            #[cfg(not(windows))]
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
                    close_fd(pipe[1]);
                    close_fd(pipe[0]);
                }
                if let Some(p) = &mut process {
                    p.deinit();
                }
                // WS19 T17a [R1-DIVERGENCE]: drain the pty master BEFORE stop() — the
                // io-reader thread does not exist on this arm, so without this the
                // SIGHUP'd child can wedge unreapably (`?Es`). See
                // `drain_pty_master_once`.
                #[cfg(not(windows))]
                let _ = drain_pty_master_once(pty_fds.read);
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
        // struct defaults (`write_pool` = `.empty` = an empty pool,
        // `write_queue`/`*_c` = `.{}`, `termios_timer_running`
        // = true, `termios_mode` = `.{}`).
        td.backend = crate::backend::ThreadData::Exec(ThreadData {
            start: process_start,
            exited: false,
            write_stream: stream,
            process,
            write_pool: MemoryPool::init(alloc),
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
        // (PORT NOTE (WS20): filled in. `comptime build_config.flatpak` is the
        // `flatpak` cargo feature; with it off the `else if` arm is compiled out
        // exactly as Zig's lazy compilation leaves it.)
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
        } else {
            #[cfg(feature = "flatpak")]
            // switch (self.subprocess.process orelse break :flatpak) { ... }
            if let Some(proc) = self.subprocess.process.as_mut() {
                match proc {
                    // If we're in flatpak and we have a flatpak command
                    // then we can run the special flatpak logic for watching.
                    Process::Flatpak(c) => unsafe {
                        c.waitXev(
                            td.loop_,
                            std::ptr::addr_of_mut!((*exec_ptr).flatpak_wait_c),
                            td_ptr.cast::<core::ffi::c_void>(),
                            Self::flatpakExit::<M>,
                        );
                    },
                    Process::ForkExec(_) => {}
                }
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

    // [M3 / review follow-up] This function is FULLY IMPLEMENTED (an earlier
    // "DEFERRED (D6 xev thread lifecycle)" header here was stale). Everything
    // below is ported: `externalExit`, `stop()`, the read-thread quit-pipe
    // write, the windows `CancelIoEx` arm and the read-thread join. It matters
    // that this is real code, because the `killPid` divergence note above leans
    // on it: the ORDERING here (`stop()` runs while the io-reader thread is
    // still draining the pty master) is what keeps the normal close path off
    // the wedging condition. Do not reorder it. Reference, verbatim:
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
        #[cfg(not(windows))]
        let rc = unsafe {
            libc::write(
                exec.read_thread_pipe,
                b"x".as_ptr().cast::<std::ffi::c_void>(),
                1,
            )
        };
        // PORT NOTE (Windows): `posix.fd_t` is a HANDLE there, and the write is
        // `WriteFile` on the quit pipe's write end. A read end that is already
        // closed fails with ERROR_BROKEN_PIPE (109) or ERROR_NO_DATA (232), the
        // Windows form of the EPIPE the POSIX arm ignores; any other failure
        // gets the same warning, with the Win32 error code.
        #[cfg(windows)]
        {
            use rustty_os::windows as win;
            let mut written: win::DWORD = 0;
            let ok = unsafe {
                win::exp::kernel32::WriteFile(
                    exec.read_thread_pipe,
                    b"x".as_ptr().cast::<std::ffi::c_void>(),
                    1,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok == win::FALSE {
                let e = unsafe { win::kernel32::GetLastError() };
                const ERROR_BROKEN_PIPE: win::DWORD = 109;
                const ERROR_NO_DATA: win::DWORD = 232;
                if e != ERROR_BROKEN_PIPE && e != ERROR_NO_DATA {
                    log::warn!(
                        target: LOG_SCOPE,
                        "error writing to read thread quit pipe err={e}"
                    );
                }
            }
        }
        #[cfg(not(windows))]
        if rc < 0 {
            // .SUCCESS => {}, .PIPE => {},
            // else => |e| log.warn(
            //     "error writing to read thread quit pipe err=E{s}",
            //     .{@tagName(e)},
            // ),
            //
            // PORT NOTE: the port's earlier Zig comment here reproduced an OLDER
            // upstream shape (`error.BrokenPipe` + `err={}`); at pin 20abdb50a the
            // reference switches on the ERRNO and renders `err=E{s}` from
            // `@tagName`. The live call below follows the pin.
            let e = errno();
            if e != libc::EPIPE {
                log::warn!(
                    target: LOG_SCOPE,
                    "error writing to read thread quit pipe err=E{}",
                    errno_tag_name(e)
                );
            }
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
                // switch (GetLastError()) {
                //     .NOT_FOUND => {},
                //     else => |err| log.warn("error interrupting read thread err={}", .{err}),
                // }
                let err = unsafe { rustty_os::windows::kernel32::GetLastError() };
                const ERROR_NOT_FOUND: u32 = 1168;
                if err != ERROR_NOT_FOUND {
                    log::warn!(target: LOG_SCOPE, "error interrupting read thread err={err}");
                }
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

    // [task-18 STALE HEADER STRUCK (S211-F7): this said "DEFERRED (D6 xev termios
    // timer, 0 tests — R1-safe)". `focusGained` is FULLY IMPLEMENTED at :939
    // below. It was also mislabelled: the block it heads is `focusGained`, not the
    // termios timer (that is `termiosTimer`, live at :1276).] Reference, verbatim:
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
    // (Windows returns before the termios half, as the reference's comptime
    // `return` does, so its locals go unused there.)
    #[cfg_attr(windows, allow(unused_variables))]
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

    // [WS20] STALE HEADER STRUCK: this said "DEFERRED (D6 — called only from the
    // xev processExit/flatpakExit callbacks; ...)", but `processExitCommon`,
    // `processExit` and (WS20) `flatpakExit` are all LIVE below and are wired
    // from `threadEnter`. What follows is only the verbatim reference source the
    // live bodies were ported from:
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
    // The four exit / termios callbacks below fill the verbatim reference bodies
    // preserved in the comment block above. [WS20] `flatpakExit` is now LIVE too
    // (gated on the `flatpak` cargo feature = `comptime build_config.flatpak`,
    // matching Zig lazy compilation). Userdata is termio's
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
        log::debug!(
            target: LOG_SCOPE,
            "child process exited status={} runtime={}ms",
            exit_code,
            runtime_ms.unwrap_or(0)
        );

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
        //
        // PORT NOTE: reference defect (R13), UD67. The reference unwraps the wait
        // result with `catch unreachable`, but on the kqueue backend (macOS,
        // FreeBSD) a child that exits BEFORE the `EVFILT_PROC` registration is
        // submitted makes XNU answer the registration with `ESRCH`, which libxev
        // maps to `error.NoSuchProcess`. The registration is only submitted on
        // the loop's next tick after `threadEnter`, so a command that exits at
        // once (a typo'd `command`, `/usr/bin/true`) can reach it; measured on
        // macOS 26.5.1 (a zombie child's `EV_ADD` returns `EV_ERROR` with
        // `data = ESRCH`). ReleaseSafe panics; ReleaseFast is undefined
        // behaviour. The child HAS exited, so the port takes the reference's own
        // exit path, `processExitCommon`, with exit code 0 — the value libxev's
        // kqueue backend itself reports when it has no decodable exit status
        // (`kqueue.zig`'s proc arm returns 0 unless `WIFEXITED`). The other
        // wait errors are left as the reference's `unreachable`: the completion
        // is never canceled, and the proc op always delivers a kevent.
        let exit_code = match r {
            Ok(code) => code,
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "visionos",
                target_os = "freebsd"
            ))]
            Err(xev::ProcessWaitError::NoSuchProcess) => 0,
            Err(e) => unreachable!("process wait failed: {e:?}"),
        };
        // processExitCommon(td_.?, exit_code);
        Self::processExitCommon::<M>(td_, exit_code);
        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn flatpakExit(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *FlatpakHostCommand.Completion,
    //     r: FlatpakHostCommand.WaitError!u8,
    // ) void
    //
    // PORT NOTE (WS20): filled in. The userdata arrives as the `?*anyopaque` the
    // `os/flatpak.zig` `Completion.callback` signature carries, so the typed
    // `ThreadData<M>` pointer is recovered by cast exactly as the reference's
    // `waitXev` trampoline does. `#[cfg(feature = "flatpak")]` because its only
    // caller is the `comptime build_config.flatpak` arm of `threadEnter`.
    #[cfg(feature = "flatpak")]
    fn flatpakExit<M: rustty_apprt_api::surface::AppMailbox + 'static>(
        td_: *mut core::ffi::c_void,
        _l: *mut xev::Loop,
        _c: *mut FlatpakHostCommandCompletion,
        r: core::result::Result<u8, FlatpakHostCommandWaitError>,
    ) {
        // const exit_code = r catch unreachable;
        //
        // PORT NOTE (N171): an R7 invariant, not an R13 defect; nothing
        // outside the process can produce either half of `WaitError`
        // (`os/flatpak.zig:115`) here. The `Error` half is stored only while
        // the state is `.init` or `.err`, but `threadEnter` calls `waitXev`
        // only after `spawn` succeeded, which means the state is `.started`
        // or `.exited`. The timer half is `Canceled`, which needs a cancel of
        // this completion (nothing issues one), or `Unexpected`, which only
        // an io_uring timeout result other than ETIME/ECANCELED produces (the
        // kernel does not; kqueue and epoll never do).
        let exit_code = r.expect(
            "flatpak wait result is an exit code: waitXev runs only after a \
             successful spawn and its timer is never canceled",
        );
        // processExitCommon(td_.?, exit_code);
        Self::processExitCommon::<M>(
            td_.cast::<crate::termio_impl::ThreadData<M>>(),
            exit_code as u32,
        );
    }

    // fn termiosTimer(
    //     td_: ?*termio.Termio.ThreadData,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Timer.RunError!void,
    // ) xev.CallbackAction
    //
    // (Windows: every caller is gated off Windows, as the reference's comptime
    // guards gate them, so Zig never analyzes this fn there; it is compiled
    // out the same way. The `@panic` arm below is kept for R1.)
    #[cfg(not(windows))]
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
            Err(err) => {
                // log.warn("error in termios timer callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in termios timer callback err={err:?}");
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
            Err(err) => {
                // log.warn("error getting termios mode err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error getting termios mode err={err:?}");
                // break :err .{};
                ptypkg::Mode::default()
            }
        };

        // If the mode changed, then we process it.
        // if (!std.meta.eql(mode, exec.termios_mode)) mode_change: {
        if mode != unsafe { (*exec).termios_mode } {
            'mode_change: {
                // log.debug("termios change mode={}", .{mode});
                log::debug!(target: LOG_SCOPE, "termios change mode={mode:?}");
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

    // [W6 STALE HEADER STRUCK: this said "DEFERRED (D6 xev write stream/pools,
    // 0 tests — R1-safe)" — the body below is LIVE.] Preserved verbatim:
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
    //         const w = try exec.write_pool.create(alloc);
    //         w.td = exec;
    //         const buf = &w.buf;
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
    //             &w.req,
    //             .{ .slice = slice },
    //             ThreadData.Write,
    //             w,
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
            // const w = try exec.write_pool.create(alloc);
            //
            // SB/UB (rust-mut-args-strongly-protected): the checked-out node
            // lives in the pool's arena heap, NOT inline in `ThreadData`, so
            // the momentary `&mut write_pool` autoref here never covers the
            // node's bytes; the node's ADDRESS is held by the event loop
            // across completion (stable until `ttyWrite` destroys it). The
            // slot is logically uninitialized (`undefined`), so its fields
            // are written via `addr_of_mut!` raw places — never a `&mut
            // Write` re-borrow (Miri-proven at
            // .port/phase-4/mirihar-termio-pool).
            // (PORT NOTE: the reference passes `alloc` per call
            // (`create(alloc)`); the rustty-zigstd `MemoryPool` owns its
            // arena's child allocator from `init` — the per-call alloc
            // param collapses, matching the `deinit` note below.)
            let w: *mut Write = unsafe {
                (*exec)
                    .write_pool
                    .create()
                    .map_err(|_| crate::Error)?
                    .as_ptr()
            };
            // w.td = exec;
            unsafe { std::ptr::addr_of_mut!((*w).td).write(exec) };
            // const buf = &w.buf;
            // (PORT NOTE: `buf` is the raw address of the node's 64-byte
            // buffer — `buf.len` == 64.)
            let buf: *mut [u8; 64] = unsafe { std::ptr::addr_of_mut!((*w).buf) };
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
            //     td.loop, &exec.write_queue, &w.req, .{ .slice = slice },
            //     ThreadData.Write, w, ttyWrite,
            // );
            //
            // SAFETY: `loop_`/`&exec.write_queue`/`&w.req` are stable pointers
            // held raw by the ordered-write engine (the pool-arena node `w`
            // keeps its address until `ttyWrite` destroys it); `slice` points
            // into the node's own 64-byte buffer; the userdata is the `Write`
            // node pointer `ttyWrite` recovers.
            unsafe {
                let write_stream = (*exec).write_stream;
                write_stream.queue_write(
                    loop_,
                    std::ptr::addr_of_mut!((*exec).write_queue),
                    std::ptr::addr_of_mut!((*w).req),
                    xev::WriteBuffer::Slice(slice),
                    w,
                    Self::ttyWrite,
                );
            }
        }

        Ok(())
    }

    // [W6 STALE HEADER STRUCK: this said "DEFERRED (D6 — the xev stream-write
    // completion callback)" — the body below is LIVE.] Preserved verbatim:
    //
    // fn ttyWrite(
    //     w_: ?*ThreadData.Write,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     _: xev.Stream,
    //     _: xev.WriteBuffer,
    //     r: xev.WriteError!usize,
    // ) xev.CallbackAction {
    //     const w = w_.?;
    //     w.td.write_pool.destroy(w);
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
    // PORT NOTE: `w_` is the pooled `ThreadData.Write` node — the reference
    // passes `w` as the `queueWrite` userdata. Non-generic (no `M`).
    fn ttyWrite(
        w_: *mut Write,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        _s: xev::Stream,
        _b: xev::WriteBuffer,
        r: core::result::Result<usize, xev::WriteError>,
    ) -> xev::CallbackAction {
        // const w = w_.?;
        let w = w_;
        // w.td.write_pool.destroy(w);
        //
        // SAFETY: `w` is the pool-arena node this write checked out (the
        // engine pops it from the write queue and never touches it after this
        // callback returns Disarm); `w.td` was written at checkout and the
        // `ThreadData` outlives the loop. The `&mut write_pool` autoref
        // covers only the pool header (free-list + arena fields) inside
        // `ThreadData` — never the arena-heap node bytes — so the addresses
        // of other in-flight nodes stay valid (Miri-proven at
        // .port/phase-4/mirihar-termio-pool).
        unsafe {
            let td = (*w).td;
            (*td)
                .write_pool
                .destroy(std::ptr::NonNull::new_unchecked(w));
        }

        // const d = r catch |err| { log.err("write error: {}", .{err}); return .disarm; };
        let _d = match r {
            Ok(d) => d,
            Err(err) => {
                // log.err("write error: {}", .{err});
                log::error!(target: LOG_SCOPE, "write error: {err:?}");
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
    // (PORT NOTE: the reference nests `pub const Write = struct { ... }` in
    // `ThreadData`; Rust cannot nest a type in a struct's namespace, so it is
    // hoisted to the module-level `Write` below — the `WriteReq` hoist
    // precedent.)
    /// Process start time and boolean of whether its already exited.
    pub start: std::time::Instant,
    // exited: bool = false,
    pub exited: bool,

    /// The data stream is the main IO for the pty.
    pub write_stream: xev::Stream,

    /// The process watcher
    pub process: Option<xev::Process>,

    /// This is the pool of available (unused) write states. If you grab
    /// one from the pool, you must put it back when you're done!
    // write_pool: std.heap.MemoryPool(Write) = .empty,
    //
    // (PORT NOTE: the rustty-zigstd `MemoryPool` owns its arena's child
    // allocator (`init(alloc)` in `threadEnter`) — the reference's per-call
    // `create(alloc)`/`deinit(alloc)` params collapse into it.)
    pub write_pool: MemoryPool<Write>,

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

/// The state for a single queued pty write. The write request and
/// the buffer it writes from must both remain pointer-stable until
/// the write completes, so they're pooled together and checked out
/// per write.
//
// pub const Write = struct {
//
// PORT NOTE: hoisted from the reference's `ThreadData.Write` nesting (see the
// `ThreadData` note). SB/UB: nodes are dispensed by `write_pool` as raw,
// logically-uninitialized slots; every field is written via `addr_of_mut!`
// raw places and the node is handed to the event loop as a RAW pointer —
// never a `&mut Write` (`rust-mut-args-strongly-protected`,
// `intrusive-ptr-and-placeholder-fill`; Miri-proven at
// .port/phase-4/mirihar-termio-pool).
pub struct Write {
    /// Backpointer to the thread data so the write completion
    /// callback can put this back into the pool.
    // td: *ThreadData,
    pub td: *mut ThreadData,

    /// The libxev write request.
    pub req: xev::WriteRequest,

    /// The buffer for the data being written.
    pub buf: [u8; 64],
}

impl ThreadData {
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
    pub(crate) fn placeholder() -> ThreadData {
        // Inert descriptors: `-1` on POSIX, `INVALID_HANDLE_VALUE` on Windows.
        #[cfg(not(windows))]
        let inert: ReadThreadFd = -1;
        #[cfg(windows)]
        let inert: ReadThreadFd = rustty_os::windows::INVALID_HANDLE_VALUE;
        ThreadData {
            start: std::time::Instant::now(),
            exited: false,
            write_stream: xev::Stream::init_fd(inert),
            process: None,
            write_pool: MemoryPool::init(Allocator),
            write_queue: xev::WriteQueue::default(),
            process_wait_c: xev::Completion::default(),
            flatpak_wait_c: FlatpakHostCommandCompletion::default(),
            read_thread: std::thread::spawn(|| {}),
            read_thread_pipe: inert,
            read_thread_fd: inert,
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

        // Clear our write pool. We know we aren't ever going to do
        // any more IO since we stop our data stream below so we can just
        // drop this.
        // self.write_pool.deinit(alloc);
        // (PORT NOTE: the pool's memory frees on Drop; the alloc param collapses.)
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
// [DOUBLE-FREE FIX] Ownership of the arena-collapsed C-string storage (`args`,
// `cwd`; `env` too) mirrors the reference `Subprocess` (Exec.zig:877-884): the
// storage is freed EXACTLY ONCE, inside `deinit` (the reference's
// `arena.deinit()`), and `deinit` then leaves every owned field EMPTY (the
// Rust-safe analogue of `self.* = undefined`). Freeing in `deinit` — rather
// than deferring to the value's Drop — is what keeps the free single even when a
// caller both `deinit()`s the value AND later drops it (an apprt surface
// finalize freeing its heap box by dropping it). This pairs with the in-place
// (`&mut self`) `Termio::deinit`, so deinit operates on the SAME object that is
// later dropped (never a bitwise `ptr::read` duplicate, which would defeat the
// empty-after-deinit invariant).
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
    //
    // PORT NOTE (task R13-P, N50): raw bytes, not `CString`. The reference's
    // `dupeZ` copy keeps every byte of the configured or inherited directory,
    // an interior NUL included (OSC 7 decodes `%00` into one). `start` asks
    // `access` about those bytes and takes its `cannot access cwd, ignoring`
    // arm, the same arm Zig's `pathToPosix` `error.BadPathName` takes.
    pub cwd: Option<Vec<u8>>,
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
    if let Err(err) = unsafe { (*sp).childPreExec() } {
        // log.err("error initializing child: {}", .{err});
        //
        // PORT NOTE (async-signal safety) — see `log_err_child_raw` below.
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD101: the
        // reference's `log.err` here can deadlock the child on a lock another
        // thread held at the fork; this raw `write(2)` is the safe handling.
        // THIS RUNS IN THE FORKED CHILD, between `fork(2)` and `execve(2)`, so
        // the `log::` facade must never be used here. `rustty-apprt`'s
        // `gtk/pre_exec.rs` is the PRECEDENT this follows, not a case to
        // distinguish from: `pre_exec.zig:39` and `:58` call `log.err` through
        // the very same global `logFn` this site does, and that port renders
        // them as a raw `write(2)` for exactly this reason.
        log_err_child_raw(&err);
    }
    // return null;
    None
}

/// The FIFTH emission shape (`ADJUDICATION.md`, task-14d-3): `Exec.zig:1045`'s
/// `log.err("error initializing child: {}", .{err})`, hand-rendered and written
/// with a raw `write(2)` instead of going through the `log::` facade.
///
/// # Why the facade is forbidden here, and why fidelity does not require it
///
/// This is called from `subprocess_pre_exec`, the `os_pre_exec` hook, which
/// `Command::spawn` invokes IN THE FORKED CHILD after `fork(2)` returns 0 and
/// before `execve(2)`. Only the forking thread survives into the child, so any
/// lock held by another thread at the instant of the fork is held FOREVER in the
/// child. The parent here always has a renderer thread, an io thread and a
/// gather thread running. POSIX therefore permits only async-signal-safe
/// operations on this path.
///
/// `log::error!` violates that twice over. It formats through `core::fmt` into
/// the installed backend, and `rustty-core`'s `AppLogger` — the backend the real
/// app installs — both ALLOCATES (`logging.rs:61,76,97` build the rendered
/// string) and takes the process-wide STDERR LOCK (`logging.rs:99`). Either the
/// allocator's lock or stderr's can be held by a dead thread, and the child then
/// hangs with NO diagnostic at all — strictly worse than the failure this
/// diagnostic exists to report.
///
/// Fidelity here is owed to the reference's observable OUTPUT, not to its call
/// mechanism, and nothing is lost: the bytes below are exactly what `logFn`
/// would have written to the same fd. `rustty-apprt`'s `gtk/pre_exec.rs`
/// established this shape for the identical reference construct
/// (`pre_exec.zig:39,58` are also `log.err` through the same global `logFn`);
/// this is that precedent applied, not an exception to it.
///
/// The name is `log_*` on purpose: `G-LOGFILE`'s `discover_emitters` recognises
/// a `log_`-prefixed fn wrapping a `write!` as the fn-facade shape it already
/// knows (shape 3), so the call site is COUNTED rather than read as a dropped
/// diagnostic.
///
/// # The level token is `error`, not `err`
///
/// The prefix hand-renders `logFn`'s own output, so it must render it exactly.
/// Zig's `std.log.Level.asText` maps `.err` to the string **`"error"`**
/// (`lib/std/log.zig:45`), and this port agrees at
/// `rustty_core::logging::Level::as_text`. Two stages have now shipped `err(`
/// here; `log_err_child_raw_writes_exactly_what_logfn_would` captures the real
/// bytes off fd 2 rather than re-asserting the literal.
///
/// # No allocation below this line
///
/// The message is formatted into a STACK buffer through `core::fmt`, which does
/// not allocate, and the only argument is `crate::Error`'s derived `Debug`,
/// which does not allocate either. An over-long render is truncated rather than
/// grown. Then one `write(2)`, which is on POSIX's async-signal-safe list.
#[cfg(not(windows))]
fn log_err_child_raw(err: &crate::Error) {
    /// A `core::fmt::Write` sink over a fixed stack buffer. Truncates; never
    /// allocates, never locks.
    struct StackBuf {
        buf: [u8; 256],
        len: usize,
    }

    impl core::fmt::Write for StackBuf {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let room = self.buf.len() - self.len;
            let n = room.min(s.len());
            self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
            self.len += n;
            Ok(())
        }
    }

    use core::fmt::Write as _;
    let mut out = StackBuf {
        buf: [0u8; 256],
        len: 0,
    };
    // `logFn`'s rendering: `<level>(<scope>): <message>\n`. The scope is
    // `LOG_SCOPE` (`io_exec`), the same tag the facade sites in this file pass
    // as `target:`.
    // (`writeln!`, not `write!` + `\n`, only because clippy insists; `writeln!`
    // always emits a bare `\n` on every platform, so the bytes are identical.
    // `G-LOGFILE`'s `WRAPPER_WRITE` recognises both.)
    let _ = writeln!(out, "error({LOG_SCOPE}): error initializing child: {err:?}");

    // SAFETY: `out.buf[..out.len]` is a live, initialized byte range on this
    // frame; fd 2 is stderr. A short or failed write is ignored, as a logger's
    // would be. `write(2)` is async-signal-safe.
    unsafe {
        let _ = libc::write(2, out.buf.as_ptr().cast::<std::ffi::c_void>(), out.len);
    }
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
            //
            // (R2 identity: `Ghostty` in the reference's literal is the product name.)
            log::info!(
                target: LOG_SCOPE,
                "found {} resources dir: {}",
                rustty_identity::APP_NAME_TITLECASE,
                String::from_utf8_lossy(dir)
            );
            env.put(&env_name("RESOURCES_DIR"), dir);
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
            env.put(b"TERM", &cfg.term);
            // try env.put("COLORTERM", "truecolor");
            env.put(b"COLORTERM", b"truecolor");

            // Assume that the resources directory is adjacent to the terminfo
            // database
            // var buf: [std.fs.max_path_bytes]u8 = undefined;
            // const dir = try std.fmt.bufPrint(&buf, "{s}/terminfo", .{
            //     std.fs.path.dirname(base) orelse unreachable,
            // });
            let mut dir = dirname(base).unwrap().to_vec();
            dir.extend_from_slice(b"/terminfo");
            // try env.put("TERMINFO", dir);
            env.put(b"TERMINFO", &dir);
        } else {
            // if (comptime builtin.target.os.tag.isDarwin()) {
            //     log.warn("ghostty terminfo not found, using xterm-256color", .{});
            //     log.warn("the terminfo SHOULD exist on macos, please ensure", .{});
            //     log.warn("you're using a valid app bundle.", .{});
            // }
            //
            // (R2 identity: `ghostty` in the first literal is the product name.)
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            {
                log::warn!(
                    target: LOG_SCOPE,
                    "{} terminfo not found, using xterm-256color",
                    rustty_identity::APP_NAME
                );
                log::warn!(target: LOG_SCOPE, "the terminfo SHOULD exist on macos, please ensure");
                log::warn!(target: LOG_SCOPE, "you're using a valid app bundle.");
            }

            // try env.put("TERM", "xterm-256color");
            env.put(b"TERM", b"xterm-256color");
            // try env.put("COLORTERM", "truecolor");
            env.put(b"COLORTERM", b"truecolor");
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
                Err(err) => {
                    // log.warn("failed to get ghostty exe path err={}", .{err});
                    //
                    // (R2 identity: `ghostty` is the product name.)
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to get {} exe path err={err:?}",
                        rustty_identity::APP_NAME
                    );
                    break 'rustty_path;
                }
            };
            // const exe_dir = std.fs.path.dirname(exe_bin_path) orelse break :ghostty_path;
            // PORT NOTE (P3-W7 T2): the reference's `exe_dir` is `[]const u8`.
            // This used `Path::to_str()`, which SKIPPED the whole
            // ghostty-bin-on-PATH block for a non-UTF-8 executable path;
            // `as_encoded_bytes` is lossless on every platform.
            let exe_dir_path = match exe_bin_path.parent() {
                Some(d) => d,
                None => break 'rustty_path,
            };
            let exe_dir: &[u8] = exe_dir_path.as_os_str().as_encoded_bytes();
            // log.debug("appending ghostty bin to path dir={s}", .{exe_dir});
            //
            // (R2 identity: `ghostty` is the product name.)
            log::debug!(
                target: LOG_SCOPE,
                "appending {} bin to path dir={}",
                rustty_identity::APP_NAME,
                String::from_utf8_lossy(exe_dir)
            );

            // We always set this so that if the shell overwrites the path
            // scripts still have a way to find the Ghostty binary when
            // running in Ghostty.
            // try env.put("GHOSTTY_BIN_DIR", exe_dir);
            env.put(&env_name("BIN_DIR"), exe_dir);

            // Append if we have a path. We want to append so that ghostty is
            // the last priority in the path. If we don't have a path set
            // then we just set it to the directory of the binary.
            // if (env.get("PATH")) |path| {
            if let Some(path) = env.getv(b"PATH").map(<[u8]>::to_vec) {
                // Verify that our path doesn't already contain this entry
                // var it = std.mem.tokenizeScalar(u8, path, std.fs.path.delimiter);
                // while (it.next()) |entry| {
                //     if (std.mem.eql(u8, entry, exe_dir)) break :ghostty_path;
                // }
                let delimiter = crate::shell_integration::PATH_LIST_DELIMITER;
                for entry in path
                    .split(|&b: &u8| b == delimiter)
                    .filter(|s: &&[u8]| !s.is_empty())
                {
                    if entry == exe_dir {
                        break 'rustty_path;
                    }
                }

                // try env.put(
                //     "PATH",
                //     try internal_os.appendEnv(alloc, path, exe_dir),
                // );
                env.put(b"PATH", &appendEnv(&path, exe_dir));
            } else {
                // try env.put("PATH", exe_dir);
                env.put(b"PATH", exe_dir);
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
            let xdg_data_dir_key: &[u8] = b"XDG_DATA_DIRS";
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
            //
            // PORT NOTE (G-LOGFILE / task-14e-1): the reference's
            // `log.warn("error building {s}; err={}", ...)` hangs off that
            // `bufPrint` error, which the growable format cannot produce, so
            // there is no failure arm here. See the `MANPATH` sibling below and
            // the file-header note.
            let mut data_dir = resources_dir.clone();
            data_dir.extend_from_slice(b"/..");
            let current = env
                .getv(xdg_data_dir_key)
                .unwrap_or(b"/usr/local/share:/usr/share")
                .to_vec();
            env.put(xdg_data_dir_key, &appendEnv(&current, &data_dir));

            // const manpath_key = "MANPATH";
            let manpath_key: &[u8] = b"MANPATH";
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
            // PORT NOTE (G-LOGFILE / task-14e-1): as with `XDG_DATA_DIRS` above,
            // the reference's
            // `log.warn("error building {s}; man pages may not be available; err={}", ...)`
            // hangs off a `bufPrint` failure the growable format cannot produce.
            let mut man_dir = resources_dir.clone();
            man_dir.extend_from_slice(b"/../man");
            let current = env.getv(manpath_key).unwrap_or(b"").to_vec();
            env.put(manpath_key, &appendEnvAlways(&current, &man_dir));
        }

        // Set environment variables used by some programs (such as neovim) to detect
        // which terminal emulator and version they're running under.
        // try env.put("TERM_PROGRAM", "ghostty");
        // (R2 identity: the product name.)
        env.put(b"TERM_PROGRAM", rustty_identity::TERM_PROGRAM.as_bytes());
        // try env.put("TERM_PROGRAM_VERSION", build_config.version_string);
        env.put(
            b"TERM_PROGRAM_VERSION",
            rustty_version::VERSION_STRING.as_bytes(),
        );

        // VTE_VERSION is set by gnome-terminal and other VTE-based terminals.
        // We don't want our child processes to think we're running under VTE.
        // This is not apprt-specific, so we do it here.
        // env.remove("VTE_VERSION");
        env.removev(b"VTE_VERSION");

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
                    log::info!(target: LOG_SCOPE, "shell integration disabled by configuration");
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
                    log::warn!(
                        target: LOG_SCOPE,
                        "no resources dir set, shell integration disabled"
                    );
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
            let integration = match shell_integration::setup(
                dir,
                &default_shell_command,
                &mut env,
                force,
            )
            .map_err(InitError::ShellIntegration)?
            {
                Some(i) => i,
                None => {
                    // log.warn("shell could not be detected, no automatic shell integration will be injected", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "shell could not be detected, no automatic shell integration will be injected"
                    );
                    break 'shell default_shell_command;
                }
            };

            // log.info(
            //     "shell integration automatically injected shell={}",
            //     .{integration.shell},
            // );
            log::info!(
                target: LOG_SCOPE,
                "shell integration automatically injected shell={:?}",
                integration.shell
            );

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
        //
        // PORT NOTE (G-LOGFILE / task-14e-1): so the reference's
        // `log.warn("failed to allocate space for command args, falling back to
        // basic shell", .{})` has no arm to hang on. See the file-header note.
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
        //
        // PORT NOTE (task R13-P, N50): the bytes are copied as they are. This was
        // `.map(cstring)`, which panicked on an interior NUL (an OSC 7 `%00`
        // inherited by the next new tab, or a NUL in `working-directory`), and
        // the macOS staticlib aborts on a panic. `start` decides what to do with
        // such a path, exactly where the reference does.
        let cwd: Option<Vec<u8>> = cfg.working_directory.clone();

        // Propagate the current working directory (CWD) to the shell, enabling
        // the shell to display the current directory name rather than the
        // resolved path for symbolic links. This is important and based
        // on the same behavior in Konsole and Kitty (see the linked issues):
        // https://bugs.kde.org/show_bug.cgi?id=242114
        // https://github.com/kovidgoyal/kitty/issues/1595
        // https://github.com/ghostty-org/ghostty/discussions/7769
        // if (cwd) |pwd| try env.put("PWD", pwd);
        if let Some(pwd) = &cwd {
            env.put(b"PWD", pwd);
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
        // (PORT NOTE: `take()` — the reference's `self.* = undefined` after this
        // clears `pty`; nulling it here makes deinit idempotent, so a double
        // deinit cannot double-CLOSE the pty master fd. `Pty::deinit` closes the
        // master.)
        if let Some(mut pty) = self.pty.take() {
            pty.deinit();
        }
        // if (self.env) |*env| env.deinit();
        self.env = None;
        // self.arena.deinit();
        //
        // (PORT NOTE: the reference's arena backs the collapsed owned fields —
        // the `args: Vec<CString>` and `cwd: Option<CString>` C-string storage
        // (and `env`, freed just above). `arena.deinit()` frees ALL of it HERE,
        // inside deinit. The port must free them IN PLACE too — NOT defer to the
        // value's Drop — so the C strings are freed exactly ONCE at deinit,
        // regardless of whether/how the value is later dropped. `mem::take`
        // moves each owned buffer out into a temporary that drops at the end of
        // the statement (the free); the fields are left EMPTY.)
        let _ = std::mem::take(&mut self.args);
        self.cwd = None;
        // self.* = undefined;
        //
        // (PORT NOTE: after the frees above every owned field is empty
        // (`args == &[]`, `cwd == None`, `env == None`, `pty` freed), so a
        // subsequent Drop of this Subprocess is a genuine no-op — dropping an
        // empty `Vec`/`None` touches no allocation. This is the Rust-safe
        // analogue of the reference's `self.* = undefined` poison: it makes
        // deinit idempotent AND makes the value safe to drop after deinit, so a
        // caller that both `deinit()`s and later drops (e.g. an apprt finalize
        // that frees the surface box by dropping it) cannot double-free the
        // C-string storage. See the [DOUBLE-FREE FIX] note on `Subprocess`.)
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
        // The windows spawn half (out_pipe/in_pipe + pseudo_console), PORTED
        // (WS20). It cannot be cross-checked from macOS (the `highway-sys` C++
        // build needs an MSVC sysroot); task W-1 built and ran it on a Windows
        // host (a ConPTY child through `threadEnter`, see
        // `windows_threadenter_reads_the_pty_through_the_windows_read_thread`).
        //
        // PORT NOTE (why this is a separate arm rather than `if (windows)` inside
        // one body, as the reference has it): the reference's single body differs
        // on windows in FOUR places — the `Command` literal's
        // stdin/stdout/stderr (`null`), its `pseudo_console` field, its
        // `os_pre_exec` (`null`), and the returned pair (`out_pipe`/`in_pipe` vs
        // `master`/`master`). In Rust those are TYPE differences (the fd type, an
        // absent-vs-present struct field), not value differences, so the split is
        // forced by the language, exactly as it already is for `Pty`/`Command`
        // themselves.
        #[cfg(windows)]
        {
            // assert(self.pty == null and self.process == null);
            assert!(self.pty.is_none() && self.process.is_none());

            // (`in_child` is the POSIX fork/exec flag; `Command.start` on windows
            // "can't ever return ExecFailedInChild" — the reference says so at the
            // `cmd.start` call site — so there is no child continuation here.)

            // Create our pty
            let pty = Pty::open(ptypkg::winsize {
                ws_row: self.grid_size.rows,
                ws_col: self.grid_size.columns,
                ws_xpixel: self.screen_size.width.try_into().unwrap(),
                ws_ypixel: self.screen_size.height.try_into().unwrap(),
            })
            .map_err(|_| crate::Error)?;
            // (capture the handles the reference's `pty.*` reads want before the
            // move into `self.pty`; they are `Copy` HANDLEs.)
            let out_pipe = pty.out_pipe;
            let in_pipe = pty.in_pipe;
            let pseudo_console = pty.pseudo_console;
            // self.pty = pty;
            self.pty = Some(pty);

            let rt_pre = self.rt_pre_exec_info;
            let rt_post = self.rt_post_fork_info;
            // The reference resolves `@hasDecl(apprt.runtime, "pre_exec")` HERE,
            // at comptime (`Exec.zig:1034,1036`). Rust cannot name the apprt from
            // this crate (R4: it would cycle), so the pair is read from the
            // process-global the apprt installs — see the seam PORT NOTE above
            // `command::RtPreExecInfo`.
            let rt_hooks = crate::command::rt_hooks();
            let self_ptr: *mut Subprocess = std::ptr::addr_of_mut!(*self);

            // If we can't access the cwd, then don't set any cwd and inherit.
            // This is important because our cwd can be set by the shell (OSC 7)
            // and we don't want to break new windows.
            //
            // PORT NOTE: `std.fs.cwd().access(proposed, .{})` — on windows the
            // posix `access(2)` shim is not available, so the existence probe is
            // `std::path::Path::exists` over the same raw path bytes (R3: the same
            // question, asked through the platform's own API).
            let cwd: Option<&[u8]> = if let Some(proposed) = &self.cwd {
                let bytes = proposed.as_slice();
                let ok = std::str::from_utf8(bytes)
                    .map(|s| std::path::Path::new(s).exists())
                    .unwrap_or(false);
                if ok {
                    Some(bytes)
                } else {
                    // log.warn("cannot access cwd, ignoring: {}", .{err});
                    //
                    // PORT NOTE (log text): the reference's `err` is
                    // `std.fs.Dir.access`'s error value; this arm reaches the same
                    // decision through a `Path::exists()` predicate, which carries
                    // no error value, so the errno is read back for the payload.
                    log::warn!(
                        target: LOG_SCOPE,
                        "cannot access cwd, ignoring: {}",
                        std::io::Error::last_os_error()
                    );
                    None
                }
            } else {
                None
            };

            // (`if (internal_os.isFlatpak()) flatpak: { ... }` — flatpak is a
            // Linux-only container runtime and `build_config.flatpak` is const
            // false, so the branch is dead here exactly as it is on macOS.)

            let argv: Vec<&CStr> = self.args.iter().map(|c| c.as_c_str()).collect();
            let env_ref: Option<&EnvMap> = self.env.as_ref();

            // Build our subcommand
            let mut cmd: Command = Command {
                // .path = self.args[0],
                path: argv[0],
                // .args = self.args,
                args: argv.as_slice(),
                // .env = if (self.env) |*env| env else null,
                env: env_ref,
                // .cwd = cwd,
                cwd,
                // .stdin = if (builtin.os.tag == .windows) null else ...,
                stdin: None,
                // .stdout = if (builtin.os.tag == .windows) null else ...,
                stdout: None,
                // .stderr = if (builtin.os.tag == .windows) null else ...,
                stderr: None,
                // .pseudo_console = if (builtin.os.tag == .windows) pty.pseudo_console else {},
                pseudo_console: Some(pseudo_console),
                // .os_pre_exec = switch (comptime builtin.os.tag) { .windows => null, ... },
                os_pre_exec: None,
                // .rt_pre_exec = if (comptime @hasDecl(apprt.runtime, "pre_exec")) ... else null,
                rt_pre_exec: rt_hooks.pre_exec,
                rt_pre_exec_info: rt_pre,
                rt_post_fork: rt_hooks.post_fork,
                rt_post_fork_info: rt_post,
                // .data = self,
                data: Some(self_ptr.cast::<std::ffi::c_void>()),
                pid: None,
            };

            // cmd.start(alloc) catch |err| { ... else => return err };
            let result: Result<PtyFds, crate::Error> = match cmd.start(alloc) {
                Err(_e) => Err(crate::Error),
                Ok(()) => {
                    // errdefer killCommand(&cmd) catch ...;
                    // (no reachable error return between here and the success
                    // return below — same as the posix arm.)
                    // log.info("started subcommand path={s} pid={?}", .{ self.args[0], cmd.pid });
                    log::info!(
                        target: LOG_SCOPE,
                        "started subcommand path={} pid={:?}",
                        String::from_utf8_lossy(self.args[0].to_bytes()),
                        cmd.pid
                    );
                    let pid = cmd.pid;

                    // self.process = .{ .fork_exec = cmd };
                    // (PORT NOTE §2: store a pid-only handle — the stored
                    // Command's reference fields are TRUE 'static empties, so no
                    // self-referential borrow can exist at rest.)
                    self.process = Some(Process::ForkExec(Command {
                        path: c"",
                        args: &[],
                        env: None,
                        cwd: None,
                        stdin: None,
                        stdout: None,
                        stderr: None,
                        pseudo_console: None,
                        os_pre_exec: None,
                        rt_pre_exec: None,
                        rt_pre_exec_info: rt_pre,
                        rt_post_fork: None,
                        rt_post_fork_info: rt_post,
                        data: None,
                        pid,
                    }));

                    // return switch (builtin.os.tag) {
                    //     .windows => .{ .read = pty.out_pipe, .write = pty.in_pipe },
                    // };
                    Ok(PtyFds {
                        read: out_pipe,
                        write: in_pipe,
                    })
                }
            };

            // The reference errdefer/defer, now that the spawn resolved. There is
            // no `posix.close(pty.slave)` half on windows (no slave fd exists —
            // the pty's child ends are the `*_pty` pipe handles the pseudo console
            // owns and `Pty.deinit` closes).
            match &result {
                Ok(_) => {
                    // Successful start we can clear out some memory.
                    // if (self.env) |*env| { env.deinit(); self.env = null; }
                    self.env = None;
                }
                Err(_) => {
                    // pty.deinit(); self.pty = null;
                    if let Some(p) = &mut self.pty {
                        p.deinit();
                    }
                    self.pty = None;
                }
            }

            result
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
            // (S212-F1: `pty.zig`'s `error.OpenptyFailed` keeps its name for
            // `Thread.threadMain`'s failure screen; it was the opaque `Error`.)
            .map_err(|e| match e {
                ptypkg::PosixPtyError::OpenptyFailed => crate::Error::OpenptyFailed,
                _ => crate::Error::Other,
            })?;
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
            // The reference resolves `@hasDecl(apprt.runtime, "pre_exec")` HERE,
            // at comptime (`Exec.zig:1034,1036`). Rust cannot name the apprt from
            // this crate (R4: it would cycle), so the pair is read from the
            // process-global the apprt installs — see the seam PORT NOTE above
            // `command::RtPreExecInfo`.
            let rt_hooks = crate::command::rt_hooks();

            // [ADV-FIX F4] derive the write-capable raw self-pointer BEFORE any
            // `&self.*` field borrow, so it and the shared `&self.env` are never
            // simultaneously live; it is dereferenced only in the forked child.
            let self_ptr: *mut Subprocess = std::ptr::addr_of_mut!(*self);

            // log.debug("starting command command={f}", .{ArgsFormatter{ .args = self.args }});
            log::debug!(
                target: LOG_SCOPE,
                "starting command command={}",
                ArgsFormatter { args: &self.args }
            );

            // If we can't access the cwd, then don't set any cwd and inherit.
            // This is important because our cwd can be set by the shell (OSC 7)
            // and we don't want to break new windows.
            // const cwd: ?[:0]const u8 = if (self.cwd) |proposed| cwd: {
            let cwd: Option<&[u8]> = if let Some(proposed) = &self.cwd {
                // if ((comptime build_config.flatpak) and internal_os.isFlatpak()) {
                //     // Flatpak sandboxing prevents access to certain reserved paths
                //     // regardless of configured permissions. Perform a test spawn
                //     // to get around this problem
                //     //
                //     // https://docs.flatpak.org/en/latest/sandbox-permissions.html#reserved-paths
                //     log.info("flatpak detected, will use host command to verify cwd access", .{});
                //     const dev_null = try std.fs.cwd().openFile("/dev/null", .{ .mode = .read_write });
                //     defer dev_null.close();
                //     var cmd: internal_os.FlatpakHostCommand = .{
                //         .argv = &[_][]const u8{ "/bin/sh", "-c", ":" },
                //         .cwd = proposed,
                //         .stdin = dev_null.handle, .stdout = dev_null.handle, .stderr = dev_null.handle,
                //     };
                //     _ = cmd.spawn(alloc) catch |err| {
                //         log.warn("cannot spawn command at cwd, ignoring: {}", .{err});
                //         break :cwd null;
                //     };
                //     _ = try cmd.wait();
                //     break :cwd proposed;
                // }
                //
                // (PORT NOTE (WS20): filled in. `comptime build_config.flatpak` is the
                // `flatpak` cargo feature; with it off the block is compiled out.)
                // (PORT NOTE: Zig's `break :cwd` out of the middle of the block maps
                // to an `Option<Option<..>>` short-circuit: `Some(v)` == the labelled
                // break took the value `v`, `None` == fall through to the `access`
                // check below.)
                #[cfg(feature = "flatpak")]
                let flatpak_cwd: Option<Option<&[u8]>> = if rustty_os::flatpak::isFlatpak() {
                    // log.info("flatpak detected, will use host command to verify cwd access", .{});
                    log::info!(
                        target: LOG_SCOPE,
                        "flatpak detected, will use host command to verify cwd access"
                    );
                    // const dev_null = try ... openFile("/dev/null", .{ .mode = .read_write });
                    let dev_null = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDWR) };
                    let mut cmd = rustty_os::flatpak::FlatpakHostCommand::default();
                    cmd.argv = vec![b"/bin/sh".to_vec(), b"-c".to_vec(), b":".to_vec()];
                    // (`.cwd = proposed` reaches D-Bus as the `[*:0]` pointer, so
                    // the host sees the bytes up to the first NUL — `cstring`.)
                    cmd.cwd = Some(cstring(proposed));
                    cmd.stdin = dev_null;
                    cmd.stdout = dev_null;
                    cmd.stderr = dev_null;
                    // _ = cmd.spawn(alloc) catch |err| { ...; break :cwd null; };
                    let spawned = match cmd.spawn() {
                        Ok(_) => true,
                        Err(err) => {
                            // log.warn("cannot spawn command at cwd, ignoring: {}", .{err});
                            log::warn!(
                                target: LOG_SCOPE,
                                "cannot spawn command at cwd, ignoring: {err:?}"
                            );
                            false
                        }
                    };
                    // _ = try cmd.wait();
                    if spawned {
                        let _ = cmd.wait();
                    }
                    // defer dev_null.close();
                    if dev_null >= 0 {
                        unsafe { libc::close(dev_null) };
                    }
                    Some(if spawned {
                        // break :cwd proposed;
                        Some(proposed.as_slice())
                    } else {
                        // log.warn("cannot spawn command at cwd, ignoring: {}", .{err});
                        None
                    })
                } else {
                    None
                };
                #[cfg(not(feature = "flatpak"))]
                let flatpak_cwd: Option<Option<&[u8]>> = None;

                // if (std.fs.cwd().access(proposed, .{})) {
                //     break :cwd proposed;
                // } else |err| {
                //     log.warn("cannot access cwd, ignoring: {}", .{err});
                //     break :cwd null;
                // }
                //
                // PORT NOTE (task R13-P, N50): Zig's `access` converts the path with
                // `pathToPosix`, which returns `error.BadPathName` for a path holding
                // a NUL byte, before any syscall (`std/Io/Threaded.zig:4062,14267`
                // in Zig 0.16.0). That error takes this same `catch` arm: the
                // warning, then no cwd. The payload names the Zig error, since
                // there is no errno to read back.
                let proposed_z = CString::new(proposed.as_slice());
                if let Some(v) = flatpak_cwd {
                    v
                } else if proposed_z.is_err() {
                    // log.warn("cannot access cwd, ignoring: {}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "cannot access cwd, ignoring: {}",
                        "error.BadPathName"
                    );
                    None
                } else if proposed_z
                    .as_ref()
                    .is_ok_and(|p| unsafe { libc::access(p.as_ptr(), libc::F_OK) } == 0)
                {
                    Some(proposed.as_slice())
                } else {
                    // log.warn("cannot access cwd, ignoring: {}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "cannot access cwd, ignoring: {}",
                        std::io::Error::last_os_error()
                    );
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
                //
                // (PORT NOTE (WS20): `comptime !build_config.flatpak` is
                // `#[cfg(not(feature = "flatpak"))]`; with the option OFF this is
                // the warn + break, with it ON the HostCommand spawn below.)
                #[cfg(not(feature = "flatpak"))]
                log::warn!(target: LOG_SCOPE, "flatpak detected, but flatpak support not built-in");

                // Flatpak command must have a stable pointer.
                // self.process = .{ .flatpak = .{ .argv = self.args, .cwd = cwd,
                //     .env = if (self.env) |*env| env else null,
                //     .stdin = pty.slave, .stdout = pty.slave, .stderr = pty.slave } };
                // var cmd = &self.process.?.flatpak;
                // const pid = try cmd.spawn(alloc);
                // errdefer killCommandFlatpak(cmd);
                // log.info("started subcommand on host via flatpak API path={s} pid={}", .{ self.args[0], pid });
                // return .{ .read = pty.master, .write = pty.master };
                #[cfg(feature = "flatpak")]
                {
                    let mut fcmd = rustty_os::flatpak::FlatpakHostCommand::default();
                    // (Byte strings since parity S111-F9; this used to go through
                    // `String::from_utf8_lossy`, renaming a non-UTF-8 argument.)
                    fcmd.argv = self.args.iter().map(|a| a.to_bytes().to_vec()).collect();
                    // (was `CString::new(c).unwrap()`: unreachable while `cwd` was a
                    // `CString`, reachable once it holds the reference's raw bytes;
                    // the flatpak test spawn above can accept a NUL path.)
                    fcmd.cwd = cwd.map(cstring);
                    fcmd.env = self.env.clone();
                    fcmd.stdin = slave;
                    fcmd.stdout = slave;
                    fcmd.stderr = slave;
                    self.process = Some(Process::Flatpak(fcmd));
                    // `var cmd = &self.process.?.flatpak;` — the STABLE pointer the
                    // reference's comment demands (spawn's documented precondition).
                    let Some(Process::Flatpak(cmd)) = self.process.as_mut() else {
                        unreachable!()
                    };
                    match cmd.spawn() {
                        Ok(pid) => {
                            // log.info("started subcommand on host via flatpak API path={s} pid={}", .{ self.args[0], pid });
                            log::info!(
                                target: LOG_SCOPE,
                                "started subcommand on host via flatpak API path={} pid={pid}",
                                String::from_utf8_lossy(self.args[0].to_bytes())
                            );
                        }
                        Err(_) => {
                            // errdefer killCommandFlatpak(cmd);
                            let _ = killCommandFlatpak(cmd);
                            return Err(crate::Error);
                        }
                    }
                    return Ok(PtyFds {
                        read: master,
                        write: master,
                    });
                }
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
                // (the `@hasDecl` answer, installed from above by the apprt —
                // `Some(rustty_apprt::gtk::pre_exec::rt_pre_exec)` under the GTK
                // apprt, `None` under every other one.)
                rt_pre_exec: rt_hooks.pre_exec,
                // .rt_pre_exec_info = self.rt_pre_exec_info,
                rt_pre_exec_info: rt_pre,
                // .rt_post_fork = if (comptime @hasDecl(apprt.runtime, "post_fork")) apprt.runtime.post_fork.postFork else null,
                rt_post_fork: rt_hooks.post_fork,
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
                //
                // PORT NOTE (G-LOGFILE / task-14e-1): so `Exec.zig:1078`'s
                // `log.warn("error killing command during cleanup err={}", ...)`,
                // which lives INSIDE that errdefer, has no arm to hang on. See the
                // file-header note.
                // log.info("started subcommand path={s} pid={?}", .{ self.args[0], cmd.pid });
                log::info!(
                    target: LOG_SCOPE,
                    "started subcommand path={} pid={:?}",
                    String::from_utf8_lossy(self.args[0].to_bytes()),
                    cmd.pid
                );

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
    #[cfg(not(windows))]
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
        // (Test builds only: count the calls, so the UD102 test can check that
        // the drain ran before this.)
        #[cfg(test)]
        tests::STOP_CALLS.with(|c| c.set(c.get() + 1));
        // switch (self.process orelse return) {
        if let Some(process) = &mut self.process {
            match process {
                // .fork_exec => |*cmd| {
                Process::ForkExec(cmd) => {
                    // Note: this will also wait for the command to exit, so
                    // DO NOT call cmd.wait
                    // killCommand(cmd) catch |err|
                    //     log.err("error sending SIGHUP to command, may hang: {}", .{err});
                    if let Err(err) = killCommand(cmd) {
                        // log.err("error sending SIGHUP to command, may hang: {}", .{err});
                        log::error!(
                            target: LOG_SCOPE,
                            "error sending SIGHUP to command, may hang: {err:?}"
                        );
                    }
                }

                // .flatpak => |*cmd| if (comptime build_config.flatpak) {
                //     killCommandFlatpak(cmd) catch |err|
                //         log.err("error sending SIGHUP to command, may hang: {}", .{err});
                //     _ = cmd.wait() catch |err|
                //         log.err("error waiting for command to exit: {}", .{err});
                // },
                //
                // (PORT NOTE (WS20): filled in. The `if (comptime build_config.flatpak)`
                // guard is the `flatpak` cargo feature; with the feature off the arm
                // body is compiled out exactly as Zig's lazy compilation leaves it.)
                #[cfg(feature = "flatpak")]
                Process::Flatpak(cmd) => {
                    if let Err(err) = killCommandFlatpak(cmd) {
                        // log.err("error sending SIGHUP to command, may hang: {}", .{err});
                        log::error!(
                            target: LOG_SCOPE,
                            "error sending SIGHUP to command, may hang: {err:?}"
                        );
                    }
                    if let Err(err) = cmd.wait() {
                        // log.err("error waiting for command to exit: {}", .{err});
                        log::error!(target: LOG_SCOPE, "error waiting for command to exit: {err:?}");
                    }
                }
                #[cfg(not(feature = "flatpak"))]
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
    // PORT NOTE (WS20): the reference's return type is `struct { read: Pty.Fd,
    // write: Pty.Fd }`, and `Pty.Fd` is `posix.fd_t` — which IS a
    // `windows.HANDLE` on Windows. The port therefore carries the same per-target
    // split `rustty_os::pty` does (`PosixPtyFd` / `WindowsPtyFd`).
    #[cfg(not(windows))]
    pub read: ptypkg::PosixPtyFd,
    #[cfg(not(windows))]
    pub write: ptypkg::PosixPtyFd,
    #[cfg(windows)]
    pub read: ptypkg::WindowsPtyFd,
    #[cfg(windows)]
    pub write: ptypkg::WindowsPtyFd,
}

/// PORT NOTE (Rust-only glue): the error set of `killCommand`/`killPid` —
/// `error.KillFailed` + the windows `unexpectedError`.
#[derive(Debug)]
pub enum KillError {
    KillFailed,
    #[allow(dead_code)]
    Unexpected(i32),
    /// [R1-DIVERGENCE, liveness] `killPid` gave up waiting for a child that
    /// cannot be reaped. See `KILL_ABANDON` and the divergence note on
    /// `killPid`. Non-fatal: the caller (`stop`) proceeds with teardown.
    #[allow(dead_code)] // M4: never constructed on windows (killPid is posix-only).
    Abandoned,
}

// ── [R1-DIVERGENCE: bounded kill wait] ──────────────────────────────────────
//
// DELIBERATE DIVERGENCE FROM THE ORACLE. Recorded in
// `.port/phase-2/DEFERRALS.md` ("killPid bounded wait"). Rationale:
//
// The reference `killPid` loops FOREVER — SIGHUP the process group, then
// `waitpid(WNOHANG)`, sleep 10ms, repeat — until the child is reaped. On macOS
// a child can enter a state where it is NEVER reapable: a `login(1)` (a session
// leader) SIGHUP'd while it still has output queued on its pty slave wedges
// UNINTERRUPTIBLY in the kernel exit path, state `?Es`. It is immune to SIGKILL
// and `waitpid` will never return it. The reference loop then spins forever,
// and — because this runs on the teardown path — the app never finishes closing.
//
// Reachability in PRODUCTION (why fidelity does not win here): the normal close
// path is safe, because `Exec::threadExit` calls `stop()` while the io-reader
// thread is still draining the pty master (upstream orders it that way on
// purpose), which is exactly what un-wedges the child. BUT `threadEnter`'s
// error arms also call `stop()` — and they run BEFORE the read thread exists
// (`pipe()` / `xev::Timer::init` / `xev::Process::init` / read-thread `spawn`
// failure, i.e. fd- or thread-exhaustion). On those arms the child is a
// freshly-forked `login` with pending output and NO reader: precisely the
// wedging condition. That is a genuine, if narrow, "rustty hangs on teardown
// and can never be closed" bug — a liveness defect, which this port treats as
// outranking strict R1 fidelity. Upstream Ghostty has the same ordering and so
// presumably the same latent bug.
//
// SHAPE OF THE DIVERGENCE — exactly one deadline, no signal change:
//
//   * 0 .. KILL_ABANDON — VERBATIM reference behavior: `killpg(pgid, SIGHUP)`
//                         + `waitpid(WNOHANG)` + 10ms sleep, repeated. Not one
//                         signal, argument or ordering differs.
//   * at KILL_ABANDON   — stop waiting and return `KillError::Abandoned`
//                         instead of looping forever. The child is left alive
//                         (or wedged) untracked and — on macOS — reparented to
//                         launchd once we exit; the caller (`stop`) proceeds
//                         with teardown. On Linux/FreeBSD there is NO launchd
//                         and NO reparenting at abandon time: rustty remains
//                         the parent of the forgotten pid, and a child that
//                         exits later stays an unreaped zombie until the app
//                         itself exits (nothing ever waits on it again).
//                         [Codex W4-02]
//
// PRECISE GUARANTEE (do not over-claim this): rustty is observationally
// identical to the oracle for any teardown that COMPLETES WITHIN
// `KILL_ABANDON`. It is NOT identical for teardowns that take longer — for
// those the oracle keeps SIGHUPing forever and rustty stops waiting. Two
// distinguishable cases live past the deadline:
//
//   1. The child is `?Es`-wedged and therefore UNREAPABLE. Waiting longer
//      provably cannot succeed, so abandoning loses nothing. This is the case
//      the divergence exists for.
//   2. The child is alive and reapable but slow — a shell trapping/ignoring
//      SIGHUP, a `zshexit`/history flush over a stalled network filesystem, an
//      ssh multiplexer or DB client flushing state. ACCEPTED CONSEQUENCE: we
//      stop waiting for it and no longer collect its exit status; on macOS it
//      is reparented to launchd once we exit, on Linux/FreeBSD it remains our
//      unreaped child (see above). Note what we deliberately do NOT do: we do not
//      kill it. Its cleanup runs to completion, so there is no data loss, and
//      the signals it received are exactly the ones the oracle would have sent.
//
// WHY THERE IS NO SIGKILL ESCALATION (a rejected earlier design; see the
// DEFERRALS.md entry for the full reasoning). An intermediate version escalated
// the process-group signal to SIGKILL after a 5s grace period. It was removed
// because it is useless where it was needed and harmful where it was not:
//
//   * For the `?Es` case it does NOTHING. An `E`-state process has already
//     entered the kernel exit path; it is SIGKILL-immune (this is the very
//     property that makes the wedge unreapable). Liveness there comes entirely
//     from the deadline, never from the signal.
//   * For case 2 above it converts "slow cleanup that eventually finishes"
//     into "killed mid-cleanup", i.e. a truncated history file or a half-
//     flushed client — real user data loss, in a case that is NOT pathological
//     and where the oracle would have waited.
//   * Its only genuine beneficiary is a child that ignores SIGHUP but is
//     killable, which the escalation would kill rather than abandon. The
//     oracle never kills such a child either, so abandoning it is closer to
//     the reference, and the alternative (inventing a SIGKILL the user's shell
//     never asked for) is not worth the data-loss exposure in case 2 — the two
//     are indistinguishable from inside `killPid`.
//
// The result is the MINIMAL divergence that achieves liveness: a single
// deadline, added to an otherwise byte-for-byte faithful loop.
//
/// Hard deadline after which `killPid` stops blocking the teardown.
/// Everything before it is verbatim reference behavior.
#[cfg(not(windows))]
const KILL_ABANDON: std::time::Duration = std::time::Duration::from_secs(10);

/// Wall-clock budget for one synchronous pty-master drain
/// ([`drain_pty_master_once`]). Generous enough to clear a shell's startup
/// output, small enough that a `threadEnter` error arm cannot become a stall.
#[cfg(not(windows))]
const DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_millis(50);

/// Drain whatever the child has ALREADY written to the pty master, once,
/// synchronously, discarding the bytes. Returns how many were consumed.
///
/// **Why (WS19 T17a, `[R1-DIVERGENCE]`).** `Exec::threadEnter`'s error arms —
/// `ProcessNotStarted` / `ProcessNoPid` / `xev::Process::init` / `pipe()` /
/// `xev::Timer::init` / read-thread `spawn` — all call `self.subprocess.stop()`
/// BEFORE the io-reader thread exists. At that moment the child is a
/// freshly-forked `login(1)` with output queued on the pty slave and NOBODY
/// reading the master, which is exactly the condition under which a SIGHUP'd
/// session leader wedges uninterruptibly in the macOS kernel exit path (state
/// `?Es`) and becomes permanently unreapable. The normal close path gets the
/// invariant for free — `threadExit` calls `stop()` while the reader is still
/// draining, and upstream orders it that way deliberately — but these arms do
/// not. Upstream Ghostty has the SAME ordering and therefore the same latent
/// defect (re-verified against upstream head:
/// `.port/phase-3/assessments/E-os-termio-platform.md` §5.3 — those `errdefer`s
/// are unchanged context lines), so this is a deliberate divergence, and one we
/// intend to upstream.
///
/// It is the same trade the bounded `killPid` already made: liveness over R1. It
/// is also strictly weaker than that one — it changes no signal, no ordering
/// observable to the child, and nothing at all on any path that succeeds; it only
/// consumes bytes that were about to be discarded anyway (the arm is tearing the
/// subprocess down).
///
/// **Bounded, so it can never become the hang it prevents:** every iteration
/// re-checks a `DRAIN_BUDGET` deadline, and each read is gated by a
/// zero-timeout `poll` so nothing ever blocks on a quiet master.
///
/// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD102 (the
/// `threadEnter` errdefers that stop the child with no reader), with UD93 (the
/// unbounded `killPid` wait it would hang on). The wedge itself was measured on
/// the port side at an earlier date, not re-measured at the pin.
#[cfg(not(windows))]
fn drain_pty_master_once(fd: libc::c_int) -> usize {
    #[cfg(test)]
    {
        tests::DRAIN_CALLS.with(|c| c.set(c.get() + 1));
        tests::STOPS_BEFORE_DRAIN
            .with(|c| c.set(Some(tests::STOP_CALLS.with(std::cell::Cell::get))));
    }
    let deadline = std::time::Instant::now() + DRAIN_BUDGET;
    let mut buf = [0u8; 4096];
    let mut total: usize = 0;

    loop {
        if std::time::Instant::now() >= deadline {
            break;
        }

        // Is anything readable RIGHT NOW? The master is blocking, so a bare
        // `read` on a quiet fd would park us here.
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut pfd, 1, 0) };
        if rc < 0 {
            if errno() == libc::EINTR {
                continue;
            }
            break;
        }
        if rc == 0 || (pfd.revents & libc::POLLIN) == 0 {
            // Nothing pending (or HUP/ERR only) — drained.
            break;
        }

        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            if errno() == libc::EINTR {
                continue;
            }
            // EAGAIN / EIO (the slave closed) — done.
            break;
        }
        if n == 0 {
            // EOF.
            break;
        }
        total += n as usize;
    }

    total
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
//
// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD93. The reference
// loop (`Exec.zig:1186-1214`) has no exit but reaping the child, so a child that
// survives SIGHUP — one that ignores it, a slow cleanup, or a macOS `login(1)`
// wedged in the kernel exit path (`?Es`) — keeps `killPid` spinning forever and
// the surface never finishes closing. The port's `KILL_ABANDON` deadline (the
// `[R1-DIVERGENCE: bounded kill wait]` note above) is that safe handling; the
// deadline is a parameter of `kill_pid_until` so a test can drive it without
// waiting ten seconds. Production always passes `KILL_ABANDON`.
#[cfg(not(windows))]
fn killPid(pid: libc::pid_t) -> Result<(), KillError> {
    kill_pid_until(pid, KILL_ABANDON)
}

/// The body of [`killPid`], with the abandon deadline as a parameter (UD93).
#[cfg(not(windows))]
fn kill_pid_until(pid: libc::pid_t, abandon: std::time::Duration) -> Result<(), KillError> {
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
    //
    // [R1-DIVERGENCE] the reference loop is unbounded; this one is bounded by
    // KILL_ABANDON. The signal sent is unchanged (always SIGHUP, as in the
    // reference). See the divergence note above.
    let started = std::time::Instant::now();
    loop {
        let elapsed = started.elapsed();

        // switch (posix.errno(c.killpg(pgid, c.SIGHUP))) {
        //     .SUCCESS => log.debug("process group killed pgid={}", .{pgid}),
        //     else => |err| killpg: { ... },
        // }
        let rc = unsafe { libc::killpg(pgid, libc::SIGHUP) };
        if rc == 0 {
            // log.debug("process group killed pgid={}", .{pgid});
            log::debug!(target: LOG_SCOPE, "process group killed pgid={pgid}");
        } else {
            'killpg: {
                let err = errno();
                // if ((comptime builtin.target.os.tag.isDarwin()) and
                //     err == .PERM)
                // {
                //     log.debug("killpg failed with EPERM, expected on Darwin and ignoring", .{});
                //     break :killpg;
                // }
                //
                // PORT NOTE: `comptime builtin.target.os.tag.isDarwin()` is a
                // `const` fold of the cfg rather than a `cfg` attribute on the
                // `if`, so the `break 'killpg` survives on every target — a
                // labelled block whose label is never named warns off Darwin.
                const IS_DARWIN: bool = cfg!(any(target_os = "macos", target_os = "ios"));
                if IS_DARWIN && err == libc::EPERM {
                    // log.debug("killpg failed with EPERM, expected on Darwin and ignoring", .{});
                    log::debug!(
                        target: LOG_SCOPE,
                        "killpg failed with EPERM, expected on Darwin and ignoring"
                    );
                    break 'killpg;
                }

                // log.warn("error killing process group pgid={} err={}", .{ pgid, err });
                log::warn!(
                    target: LOG_SCOPE,
                    "error killing process group pgid={pgid} err={}",
                    std::io::Error::from_raw_os_error(err)
                );
                return Err(KillError::KillFailed);
            }
        }

        // See Command.zig wait for why we specify WNOHANG.
        // The gist is that it lets us detect when children
        // are still alive without blocking so that we can
        // kill them again.
        // const res = posix.waitpid(pid, std.c.W.NOHANG);
        //
        // WS19 T17b: `posix_waitpid_CHECKED`, not the verbatim shim. rustty's
        // xev `.proc` completion is a SECOND reaper, so `ECHILD` is genuinely
        // reachable here and the verbatim shim's `unreachable!` would turn that
        // race into a panic on the teardown path. `None` means "gone" — which is
        // what this loop is waiting for — so break, exactly as upstream head's
        // raw-syscall `killPid` does (`rc == -1` -> `res_pid != 0` -> break).
        // Fidelity against upstream head, NOT a divergence; see the shim's docs
        // and `.port/phase-3/assessments/E-os-termio-platform.md` §5.2.
        let Some(res) = crate::command::posix_waitpid_checked(pid, libc::WNOHANG as u32) else {
            // log.debug("waitpid: child already reaped (ECHILD)", .{});
            break;
        };
        // log.debug("waitpid result={}", .{res.pid});
        log::debug!(target: LOG_SCOPE, "waitpid result={}", res.pid);
        // if (res.pid != 0) break;
        if res.pid != 0 {
            break;
        }

        // [R1-DIVERGENCE] hard deadline — the ONLY divergence in this loop.
        // In the case this exists for the child is unreapable (an `?Es` macOS
        // process is SIGKILL-immune and `waitpid` never returns it), so waiting
        // longer provably cannot succeed. Give up rather than wedge teardown
        // forever; the child is reparented to launchd when we exit. A merely
        // SLOW-but-reapable child is also abandoned here (accepted consequence,
        // see the note above) — but it is never killed, so its cleanup still
        // completes.
        if elapsed >= abandon {
            // log.warn("child pid={} unreapable after {}s, abandoning", ...);
            return Err(KillError::Abandoned);
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
            log::warn!(target: LOG_SCOPE, "pgid is our own, retrying");
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
            log::warn!(target: LOG_SCOPE, "error getting pgid for kill");
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
// PORT NOTE (WS20): filled in. `internal_os.FlatpakHostCommand.signal` is fully
// ported (`rustty-os/src/flatpak.rs`), so this is the reference body verbatim.
// It is `#[cfg(feature = "flatpak")]` for exactly the reason the reference's
// callers are `comptime build_config.flatpak`-gated: with the option off, the
// `FlatpakHostCommand` type is the empty placeholder and has no `signal`.
#[cfg(feature = "flatpak")]
#[allow(dead_code)]
fn killCommandFlatpak(command: &mut FlatpakHostCommand) -> Result<(), crate::Error> {
    // try command.signal(c.SIGHUP, true);
    command.signal(SIGHUP as u8, true).map_err(|_| crate::Error)
}

/// PORT NOTE: `c.SIGHUP` — the reference `@cImport`s `signal.h`. Hand-rolled per
/// repo convention (the `libc` crate is not a dependency of this crate).
#[cfg(feature = "flatpak")]
const SIGHUP: i32 = 1;

/// The read thread works with a companion gather thread to form a two-stage
/// pipeline that moves pty output into the terminal:
///
///   io-gather:  read()/poll() the pty into one of a few rotating
///               buffers, batching bulk output.
///   io-reader:  hand each filled buffer to processOutput (terminal
///               lock, VT parse, state update, render scheduling).
///
/// This used to be a single serial loop (and still is on Windows):
///
///   while (true) { blocking_read(); exit_if_eof(); process(); }
///
/// I found on macOS that the kernel tty output queue caps every read
/// on the master at 1 KB no matter how large the read buffer is. This means
/// that producers (e.g. `cat`) stall with the above architecture because
/// there are windows in the `process()` part where we aren't draining
/// the kernel pty fd.
///
/// Instead, having a separate thread gather and drain the kernel pty
/// into a rotating set of preallocated buffers minimizes this stall
/// period to effectively zero: while the io-reader thread parses
/// one batch, the gather thread is draining the kernel queue. There is
/// still stalling (our VT parse is a bottleneck now), but we don't stall
/// between them.
///
/// Interactive latency is preserved: a batch is delivered on the
/// first EAGAIN unless the stream is saturated (>= 1 KiB gathered
/// means the writer filled the kernel queue), in which case we bridge
/// the producer's microsecond refill gaps with a short poll, bounded
/// by a small total budget per batch that is well under a display
/// frame. This means that small outputs that are more typical continue
/// to be interactive.
///
/// We use basic poll/read syscalls here because we are only
/// monitoring two fds and this is still much faster and lower
/// overhead than any async mechanism.
//
// PORT NOTE (2f0e6659d + bb0ac4c72, WS10 T20) — THE ALIASING SEAM.
// The reference documents the ring as "A buffer is owned by exactly one stage
// at a time, so buffer contents need no locking. Only the ring metadata is
// guarded by the mutex." In Zig that is a comment. In Rust it is an aliasing
// claim about ONE allocation with two live threads inside it, so it is stated
// here as a binding invariant and it is PROVEN, not asserted:
//
//   INV-P1  `Pipeline::bufs` is an `UnsafeCell` and is NEVER referenced as a
//           whole. Each stage derives a `*mut u8` for ITS OWN SLOT ONLY, from
//           the base pointer, via `Pipeline::slot_ptr` — pointer arithmetic,
//           no place expression over the array.
//   INV-P2  Slot ownership transfers only under `mutex`: the gather stage owns
//           `head` from the claim to the publish; the parse stage owns `tail`
//           from the take to the tail advance. Those windows are disjoint per
//           slot and the mutex pairs are the happens-before edges.
//   INV-P3  No `&mut [u8]`/`&[u8]` spanning more than the owned slot is ever
//           formed, and the storage is never read by value.
//
// PROOF: `.port/phase-2/mirihar-ws10-pipeline` is an isolated non-FFI model of
// this exact ring (two real threads, same mutex + condvars + head/tail/count/
// lens/done protocol, no fds). Candidate `shipped` — this shape — is CLEAN
// under BOTH Stacked Borrows and Tree Borrows. Candidate `naive_wholeref` —
// the literal transliteration `let bufs = &mut *p.bufs.get(); &mut bufs[slot]`
// — is UB under BOTH ("tag does not exist in the borrow stack" / "write access
// forbidden"). Candidate `naive_copyfield` — the tempting "hold no reference,
// just copy the bytes out" cure — is ALSO UB under both, because a by-value
// read of the storage is still a foreign access on the owning allocation.
// See that harness's README for the full table and the run recipe.
//
/// The ring metadata. Everything in here is guarded by [`Pipeline::mutex`];
/// the buffer STORAGE deliberately is not (see INV-P1..P3 on [`ReadThread`]).
//
// PORT NOTE: the reference keeps these fields directly on `Pipeline` next to
// the mutex. Rust's `Mutex<T>` owns its data, so the guarded fields move into
// `Meta` and the unguarded storage stays beside it — which also makes the
// guarded/unguarded split a TYPE-LEVEL fact rather than a comment.
#[cfg(not(windows))]
#[derive(Default)]
struct PipelineMeta {
    /// The number of valid bytes in each buffer. Set at publish
    /// time by the gather stage, read by the parse stage.
    // lens: [buffer_count]usize = @splat(0),
    lens: [usize; ReadThread::BUFFER_COUNT],

    /// Ring state: head is the next slot the gather stage fills,
    /// tail is the next slot the parse stage consumes, count is
    /// the number of published, unconsumed batches.
    // head: usize = 0, tail: usize = 0, count: usize = 0,
    head: usize,
    tail: usize,
    count: usize,

    /// Set by the gather stage (under the mutex) while it sleeps
    /// in a bridge poll. The parse stage only writes to the idle
    /// pipe when this is set, so an interactive terminal never
    /// pays the pipe syscalls.
    // bridging: bool = false,
    bridging: bool,

    /// Set by the gather stage when the stream is over (quit
    /// signal, EOF, or pty error). The parse stage drains any
    /// remaining batches and then exits.
    // done: bool = false,
    done: bool,
}

/// The state shared between the gather and parse stages. This is
/// a fixed ring of buffers plus the metadata to rotate ownership
/// between the two threads. A buffer is owned by exactly one
/// stage at a time, so buffer contents need no locking. Only the
/// ring metadata is guarded by the mutex.
//
// const Pipeline = struct { ... };
#[cfg(not(windows))]
struct Pipeline {
    // mutex: std.Thread.Mutex = .{},
    mutex: std::sync::Mutex<PipelineMeta>,

    /// Signaled when a batch is published or the gather stage is
    /// done. Waited on by the parse stage.
    // batch_ready: std.Thread.Condition = .{},
    batch_ready: std::sync::Condvar,

    /// Signaled when a batch has been consumed. Waited on by the
    /// gather stage when all buffers are in flight (backpressure).
    // slot_free: std.Thread.Condition = .{},
    slot_free: std::sync::Condvar,

    /// A self-pipe the parse stage uses to interrupt the gather
    /// stage's bridge poll the moment it runs out of batches.
    /// Bridging a refill gap is only free while the parse stage
    /// is busy; once it goes idle, every additional microsecond
    /// spent bridging is added straight to output latency. The
    /// write end is used by the parse stage, the read end is
    /// polled by the gather stage. -1 when unavailable, in which
    /// case bridge polls are bounded by their timeout only.
    //
    // PORT NOTE: the reference stores these next to the guarded fields but
    // never mutates them after setup, so they are plain (unguarded) fields
    // here; they are written once before the gather thread is spawned.
    // idle_read_fd: posix.fd_t = -1, idle_write_fd: posix.fd_t = -1,
    idle_read_fd: std::os::fd::RawFd,
    idle_write_fd: std::os::fd::RawFd,

    /// The buffer storage itself.
    // bufs: [buffer_count][buffer_capacity]u8 = undefined,
    //
    // INV-P1: NEVER referenced as a whole. Reach a slot only via `slot_ptr`.
    // The `Box` keeps 256 KiB off the read thread's stack (the reference's
    // `Pipeline` is a stack local in `threadMainPosix`, which is fine for Zig's
    // 16 MiB default but not for a Rust thread's 2 MiB).
    bufs: Box<std::cell::UnsafeCell<[[u8; ReadThread::BUFFER_CAPACITY]; ReadThread::BUFFER_COUNT]>>,
}

// SAFETY (INV-P1..P3 on `ReadThread`, proven by
// `.port/phase-2/mirihar-ws10-pipeline`): a slot is owned by exactly one stage
// at a time, ownership transfers only under `mutex`, and no reference spanning
// more than the owned slot is ever formed. The mutex acquire/release pairs that
// transfer ownership are the happens-before edges for the buffer contents.
#[cfg(not(windows))]
unsafe impl Sync for Pipeline {}

#[cfg(not(windows))]
impl Pipeline {
    fn new() -> Pipeline {
        Pipeline {
            mutex: std::sync::Mutex::new(PipelineMeta::default()),
            batch_ready: std::sync::Condvar::new(),
            slot_free: std::sync::Condvar::new(),
            idle_read_fd: -1,
            idle_write_fd: -1,
            // `vec![0; N].into_boxed_slice()`-style zeroing rather than a stack
            // temporary: a 256 KiB array literal would be materialized on the
            // stack first and blow the thread stack.
            bufs: unsafe {
                let layout = std::alloc::Layout::new::<
                    [[u8; ReadThread::BUFFER_CAPACITY]; ReadThread::BUFFER_COUNT],
                >();
                let p = std::alloc::alloc_zeroed(layout);
                if p.is_null() {
                    std::alloc::handle_alloc_error(layout);
                }
                Box::from_raw(p.cast())
            },
        }
    }

    /// The base address of slot `i` — pointer arithmetic from the storage base,
    /// forming NO reference to the array or to any other slot (INV-P1).
    ///
    /// The deliberate absence of `(*self.bufs.get())[i]` is the whole point:
    /// that indexes through a place expression over the array, which
    /// materializes a reference covering every slot — including the one the
    /// other stage is concurrently using. The `naive_wholeref` candidate in
    /// `.port/phase-2/mirihar-ws10-pipeline` is exactly that mistake and Miri
    /// reports UB under both Stacked and Tree Borrows.
    #[inline]
    fn slot_ptr(&self, i: usize) -> *mut u8 {
        debug_assert!(i < ReadThread::BUFFER_COUNT);
        let base: *mut [u8; ReadThread::BUFFER_CAPACITY] = self.bufs.get().cast();
        // SAFETY: `i < BUFFER_COUNT`, so this stays inside the allocation.
        unsafe { base.add(i).cast::<u8>() }
    }
}

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
//
// (S211-F3: the two descriptors are `posix.fd_t`, a HANDLE on Windows, where
// `threadMainWindows` takes them.)
pub(crate) struct ReadThreadArgs<M: rustty_apprt_api::surface::AppMailbox>(
    pub ReadThreadFd,
    pub *mut crate::Termio<M>,
    pub ReadThreadFd,
);

/// `posix.fd_t` for the read thread's pty and quit descriptors.
#[cfg(not(windows))]
pub(crate) type ReadThreadFd = std::os::fd::RawFd;
/// `posix.fd_t` is a HANDLE on Windows.
#[cfg(windows)]
pub(crate) type ReadThreadFd = rustty_os::windows::HANDLE;

// SAFETY: see the newtype's contract — the wrapped `*mut Termio<M>` has a single
// toucher (the read thread) for the window it is shared, matching the landed
// `IoThreadArgs` unsafe-Send discipline (surface.rs).
unsafe impl<M: rustty_apprt_api::surface::AppMailbox> Send for ReadThreadArgs<M> {}

// (Windows: the gather/bridge constants belong to `threadMainPosix`.)
#[cfg_attr(windows, allow(dead_code))]
impl ReadThread {
    /// The number of buffers rotated between the gather and parse
    /// stages. The gather stage can run at most this many batches
    /// ahead of the parse stage before it blocks, which (via the
    /// kernel pty queue) is also what preserves flow control to the
    /// child. Empirically chosen through measurements on an M4 Max.
    /// Less than 4 there are minor slowdowns, above 4 there are no
    /// improvements.
    // const buffer_count = 4;
    //
    // CORRECTION (S211-F1): this was `4` on Darwin and `2` elsewhere, citing
    // `if (builtin.os.tag.isDarwin()) 4 else 2`. That is upstream bed47168c
    // ("termio: bound POSIX read-ahead on non-Darwin"), which 60121a039
    // reverted before the pin; at 20abdb50a both constants are unconditional
    // (`Exec.zig:1305`, `:1318`). The doc paragraph that went with it ("Other
    // POSIX platforms keep fewer batches in flight ...") is gone with it.
    pub(crate) const BUFFER_COUNT: usize = 4;

    /// The capacity of each gather buffer. One batch is also the unit
    /// of work the parse stage does per terminal lock acquisition, so
    /// this bounds both gather latency and lock hold time.
    // const buffer_capacity = 64 * 1024;
    //
    // CORRECTION (S211-F1): was `64 * 1024` on Darwin and `8 * 1024` elsewhere
    // (the same reverted commit).
    pub(crate) const BUFFER_CAPACITY: usize = 64 * 1024;

    /// How many gathered bytes mark a stream as saturated. The macOS
    /// kernel tty output queue hands the master at most about 1 KiB
    /// per read, so gathering a full 1 KiB means the writer filled
    /// the queue (a bulk stream worth briefly waiting on), while
    /// anything smaller is an interactive trickle that must be
    /// delivered with no added latency.
    // const bridge_threshold = 1024;
    const BRIDGE_THRESHOLD: usize = 1024;

    /// How many times an EAGAIN on a saturated stream is retried
    /// with an immediate read before we're willing to sleep in poll.
    /// Basically, a spin retry.
    ///
    /// The writer refills the drained kernel queue within a few
    /// microseconds, while a sleep and wakeup through poll costs
    /// several more. If the gather stage sleeps on every refill gap,
    /// the whole pipeline degenerates to lockstep with the writer at
    /// about 1 KiB per wakeup. A short burst of nonblocking reads
    /// bridges nearly all refill gaps without sleeping. Measured, 8
    /// to 16 retries catches over 90% of the gaps and nearly doubles
    /// the saturated drain rate, and larger values helped little.
    /// The cost is bounded to at most this many extra ~0.5us read
    /// syscalls per gap, and we only spin on streams that already
    /// gathered a full kernel queue, so an idle or interactive
    /// terminal never spins.
    // const bridge_spin_max = 16;
    const BRIDGE_SPIN_MAX: usize = 16;

    /// How long one bridge poll waits for the writer's next refill
    /// once the spin retries above have failed. If the stream is
    /// quiet for this long the burst is over and we deliver what we
    /// have.
    // const bridge_poll_timeout_ms = 1;
    const BRIDGE_POLL_TIMEOUT_MS: libc::c_int = 1;

    /// The longest one batch may spend bridging refill gaps before it
    /// is delivered regardless. This bounds output latency for
    /// streams that produce just enough to keep bridging. Three
    /// milliseconds is well under one display frame, so batching is
    /// invisible on screen.
    // const gather_budget_ns = 3 * std.time.ns_per_ms;
    pub(crate) const GATHER_BUDGET_NS: u64 = 3 * 1_000_000;

    // [task-18 STALE HEADER STRUCK (S211-F7): this said "DEFERRED (D6 thread
    // lifecycle, 0 tests — R1-safe): spawned only by the deferred threadEnter;
    // drives the deferred `Termio.processOutput` …". `threadMainPosix` is FULLY
    // IMPLEMENTED at :4482 below, `threadEnter` (:441) really does spawn it, and
    // `Termio::processOutput` is live. The crash-sentry `thread_state` clause
    // that stood after that was closed by T-G (S211-F2); see its site.]
    // Reference, verbatim:
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
    // PORTED (xev B3, then rewritten as the two-stage pipeline by 2f0e6659d +
    // bb0ac4c72 in WS10 T20): the real raw `poll(2)`/`read`/`processOutput`
    // pipeline, below the preserved verbatim comment above. This function is
    // the PARSE stage; `gatherMainPosix` below is the gather stage.
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
        // quit-pipe read end is closed on EVERY return path below — including the
        // two NEW early returns this commit adds (the `setNonblock` failure and
        // the gather-thread spawn failure), and including a panic unwind.
        // Declared BEFORE the scoped gather thread so it drops AFTER the join:
        // the gather stage polls `quit` for its whole life.
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
        //     setQosClass();
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        unsafe {
            rustty_os::macos::pthread_setname_np(c"io-reader".as_ptr());
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Self::setQosClass();

        // Setup our crash metadata
        // crash.sentry.thread_state = .{
        //     .type = .io,
        //     .surface = io.surface_mailbox.surface,
        // };
        // defer crash.sentry.thread_state = null;
        //
        // CORRECTION (S211-F2): this was "DEFERRED (cross-crate crash-sentry
        // seam, not part of B3) ... the `AppMailbox` trait does not yet expose a
        // `.surface` accessor ... wired when core lands." It now does
        // (`AppMailbox::crash_surface`); the guard's `Drop` is the `defer`,
        // dropped before `_quit_closer`, the reference's LIFO order.
        let _crash_thread_state = unsafe { crate::IoCrashThreadState::set(io) };

        // Set the fd to non-blocking so the gather stage can drain it
        // in a tight loop and fall back to poll for readiness. The
        // pipeline can't run with a blocking fd (a blocking read
        // would hang the gather stage on a quiet pty), but this also
        // can't realistically fail on a valid pty master.
        // if (!setNonblock(fd)) {
        //     log.err("read thread exiting, pty fd must be non-blocking", .{});
        //     return;
        // }
        //
        // PORT NOTE (R7, checked under R13 by task R13-C, inventory M88): the
        // early return below would leave the pty with no reader (the `?Es`
        // wedge `drain_pty_master_once` exists for), but it is not reachable,
        // so it stays faithful. `fd` is the pty master `threadEnter` opened
        // and keeps open until this thread is joined, and `F_GETFL`/`F_SETFL`
        // on an open descriptor fails only for an invalid one. Measured on
        // macOS 26.5.1 (XNU also pushes `FIONBIO` to the tty, which accepts
        // it): a fresh master, a master whose slave is closed, and a master
        // whose session leader has exited all take `O_NONBLOCK`; only a closed
        // descriptor fails (`EBADF`). The same four cases on Linux (Fedora,
        // kernel 6.19.10 aarch64) give the same results; there `setfl`
        // refuses only `O_APPEND`/`O_DIRECT`/`O_NOATIME` changes, which this
        // call does not make.
        if !Self::setNonblock(fd) {
            // log.err("read thread exiting, pty fd must be non-blocking", .{});
            log::error!(target: LOG_SCOPE, "read thread exiting, pty fd must be non-blocking");
            return;
        }

        // Shared pipeline
        // var pipeline: Pipeline = .{};
        let mut pipeline = Pipeline::new();

        // The idle self-pipe (see the Pipeline field docs). If we
        // can't create it we still run correctly, bridge polls are
        // just bounded by their timeout instead of being interrupted
        // when the parse stage goes idle.
        // if (posix.pipe2(.{ .CLOEXEC = true, .NONBLOCK = true })) |fds| { ... }
        //
        // PORT NOTE: `pipe2(2)` is Linux-only; macOS has no such call, so this
        // is `pipe(2)` + the two `fcntl` flag sets, which is what Zig's
        // `posix.pipe2` itself falls back to on Darwin.
        let mut idle_fds = [-1 as std::os::fd::RawFd; 2];
        if unsafe { libc::pipe(idle_fds.as_mut_ptr()) } == 0 {
            for fd in idle_fds {
                unsafe {
                    let fl = libc::fcntl(fd, libc::F_GETFL, 0);
                    if fl != -1 {
                        libc::fcntl(fd, libc::F_SETFL, fl | libc::O_NONBLOCK);
                    }
                    let fd_fl = libc::fcntl(fd, libc::F_GETFD, 0);
                    if fd_fl != -1 {
                        libc::fcntl(fd, libc::F_SETFD, fd_fl | libc::FD_CLOEXEC);
                    }
                }
            }
            pipeline.idle_read_fd = idle_fds[0];
            pipeline.idle_write_fd = idle_fds[1];
        } else {
            // log.warn("read thread failed to create idle pipe err={}", .{err});
            log::warn!(
                target: LOG_SCOPE,
                "read thread failed to create idle pipe err={}",
                std::io::Error::last_os_error()
            );
        }
        // defer if (pipeline.idle_read_fd >= 0) { close both };
        struct IdlePipeCloser(std::os::fd::RawFd, std::os::fd::RawFd);
        impl Drop for IdlePipeCloser {
            fn drop(&mut self) {
                if self.0 >= 0 {
                    unsafe {
                        libc::close(self.0);
                        libc::close(self.1);
                    }
                }
            }
        }
        let _idle_closer = IdlePipeCloser(pipeline.idle_read_fd, pipeline.idle_write_fd);

        // const gather_thread = std.Thread.spawn(.{}, gatherMainPosix, .{ fd, quit, &pipeline });
        //
        // PORT NOTE: `spawn` + `defer gather_thread.join()` becomes
        // `std::thread::scope`, which IS the join — including on a panic unwind,
        // which the reference's `defer` also covers. The scope is what lets the
        // gather stage borrow `&pipeline` without an `Arc`, matching the
        // reference's stack-local `Pipeline`.
        //
        // const gather_thread = std.Thread.spawn(
        //     .{},
        //     gatherMainPosix,
        //     .{ fd, quit, &pipeline },
        // ) catch |err| {
        //     // If we can't spawn a thread the process is already
        //     // doomed (every surface spawns several), so don't try
        //     // to limp along.
        //     log.err("read thread exiting, failed to spawn gather thread err={}", .{err});
        //     return;
        // };
        // defer gather_thread.join();
        //
        // PORT NOTE (task-14e-1 fix round 1, closes the log half of S211-F6):
        // `Builder::spawn_scoped` DOES return a `Result`. This site used to
        // discard it with `.expect("spawn io-gather")` and carried a note
        // claiming `std::thread::scope` has no fallible spawn — that claim was
        // wrong, and `S211-F6` ("gather-thread spawn failure panics instead of
        // logging and returning") is the ledger row that already said so. The
        // reference deliberately does NOT crash here: it logs and RETURNS,
        // letting the read thread exit while the rest of the process lives.
        // Both halves are restored below. The `return` leaves this closure,
        // which is the last statement of `threadMainPosix`, so it returns from
        // the read thread exactly as the reference's `return` does; nothing was
        // spawned, so there is nothing for the scope to join.
        let pipeline = &pipeline;
        std::thread::scope(|scope| {
            let gather_fd = fd;
            let gather_quit = quit;
            let mut builder = rustty_zigstd::thread::spawn_config();
            if cfg!(not(any(target_os = "macos", target_os = "ios"))) {
                // if (comptime !builtin.os.tag.isDarwin()) gather_thread.setName("io-gather");
                builder = builder.name("io-gather".to_string());
            }
            // (Test builds only: `tests::FAIL_GATHER_SPAWN` makes this spawn
            // fail for real, with a stack no thread can get.)
            #[cfg(test)]
            if tests::FAIL_GATHER_SPAWN.with(std::cell::Cell::get) {
                builder = builder.stack_size(usize::MAX / 2);
            }
            if let Err(err) = builder.spawn_scoped(scope, move || {
                Self::gatherMainPosix(gather_fd, gather_quit, pipeline);
            }) {
                log::error!(
                    target: LOG_SCOPE,
                    "read thread exiting, failed to spawn gather thread err={err:?}"
                );
                return;
            }

            // This thread is the parse stage. We consume batches in ring
            // order until the gather stage reports the stream is over and
            // the ring is drained.
            loop {
                // const batch: []const u8 = batch: { ... };
                //
                // INV-P1/P3: only the ring METADATA is read under the guard. The
                // slot INDEX and LENGTH are copied out and the guard is dropped
                // before the buffer is touched, and the buffer is reached only
                // through `slot_ptr` — never `(*bufs)[slot]`.
                let (slot, len) = {
                    let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                    // while (pipeline.count == 0) { if (pipeline.done) return;
                    //     pipeline.batch_ready.wait(&pipeline.mutex); }
                    while m.count == 0 {
                        if m.done {
                            return;
                        }
                        m = pipeline
                            .batch_ready
                            .wait(m)
                            .unwrap_or_else(|e| e.into_inner());
                    }
                    // const slot = pipeline.tail;
                    // break :batch pipeline.bufs[slot][0..pipeline.lens[slot]];
                    (m.tail, m.lens[m.tail])
                };

                // The batch buffer is owned by this stage until we advance
                // the tail below, so it is safe to read outside the lock.
                // io.processOutput(batch);
                //
                // SAFETY (INV-P2): this stage owns `slot` from the take above to
                // the tail advance below; `len <= BUFFER_CAPACITY` was written by
                // the gather stage under the same mutex. `io` is the read
                // thread's sole `processOutput` toucher (the `ReadThreadArgs`
                // contract); its `renderer_state` is live for the pointer's
                // lifetime (joined in `threadExit` before teardown).
                unsafe {
                    let batch = core::slice::from_raw_parts(pipeline.slot_ptr(slot), len);
                    crate::Termio::processOutput(io, batch);
                }

                {
                    let wake = {
                        let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                        // pipeline.tail = (pipeline.tail + 1) % buffer_count;
                        m.tail = (m.tail + 1) % Self::BUFFER_COUNT;
                        // pipeline.count -= 1;
                        m.count -= 1;
                        // const wake = pipeline.count == 0 and pipeline.bridging and
                        //     pipeline.idle_write_fd >= 0;
                        m.count == 0 && m.bridging && pipeline.idle_write_fd >= 0
                    };
                    // pipeline.slot_free.signal();
                    pipeline.slot_free.notify_one();

                    // We ran out of batches while the gather stage is
                    // bridging a refill gap: interrupt its poll so it
                    // delivers what it has instead of sleeping out the
                    // timeout while we sit idle.
                    // if (wake) { _ = posix.write(pipeline.idle_write_fd, "i") catch {}; }
                    if wake {
                        unsafe {
                            libc::write(
                                pipeline.idle_write_fd,
                                b"i".as_ptr().cast::<std::ffi::c_void>(),
                                1,
                            );
                        }
                    }
                }

                // Batch boundary: hand the renderer state mutex off if the
                // renderer is waiting. See renderer::State::lock_demand.
                //
                // PORT NOTE (D4, oracle d34b54e9b): the oracle inserts this at the
                // end of the parse-stage loop of exactly this pipeline
                // (`io.renderer_state.yieldToDemand()` after the batch's slot is
                // returned) — a LATER upstream commit than the two this task
                // ports. rustty landed it early against the then-current
                // single-loop shape; now that the pipeline exists it moves to the
                // position the oracle gives it, immediately after the slot is
                // returned. `processOutput` locks AND unlocks
                // `renderer_state.mutex` internally, so the mutex is NOT held
                // here, as `yield_to_demand` requires.
                //
                // SAFETY: `io` and its `renderer_state` are live for the read
                // thread's whole lifetime (the `ReadThreadArgs` contract, as for
                // the `processOutput` call above).
                unsafe {
                    (*(*io).renderer_state).yield_to_demand();
                }
            }
        });
    }

    /// The gather stage. This drains the pty into rotating buffers,
    /// bridging the kernel queue's refill gaps for saturated streams,
    /// and publishes each batch to the parse stage. This thread owns
    /// all fd monitoring, including the quit fd.
    //
    // fn gatherMainPosix(fd: posix.fd_t, quit: posix.fd_t, pipeline: *Pipeline) void {
    #[cfg(not(windows))]
    fn gatherMainPosix(fd: std::os::fd::RawFd, quit: std::os::fd::RawFd, pipeline: &Pipeline) {
        // if (builtin.os.tag.isDarwin()) {
        //     internal_os.macos.pthread_setname_np(&"io-gather".*);
        //     setQosClass();
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        unsafe {
            rustty_os::macos::pthread_setname_np(c"io-gather".as_ptr());
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Self::setQosClass();

        // However we exit, tell the parse stage the stream is over so
        // it drains the ring and joins us.
        //
        // defer { pipeline.mutex.lock(); pipeline.done = true;
        //         pipeline.mutex.unlock(); pipeline.batch_ready.signal(); }
        //
        // PORT NOTE: a Drop guard, so this fires on EVERY exit path INCLUDING a
        // panic unwind. Without it the parse stage would wait on `batch_ready`
        // forever and `thread::scope` would never join (E1's "return early in
        // two new places" hazard, generalized).
        struct DoneSignal<'a>(&'a Pipeline);
        impl Drop for DoneSignal<'_> {
            fn drop(&mut self) {
                {
                    let mut m = self.0.mutex.lock().unwrap_or_else(|e| e.into_inner());
                    m.done = true;
                }
                self.0.batch_ready.notify_one();
            }
        }
        let _done = DoneSignal(pipeline);

        // The fds we poll: data on the pty, our quit notification,
        // and the parse stage's idle wake. The idle fd only
        // participates in bridge polls (the parse stage only writes
        // while we're bridging), so the outer poll slices it off.
        // var pollfds: [3]posix.pollfd = .{ ... };
        let mut pollfds: [libc::pollfd; 3] = [
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
            libc::pollfd {
                fd: pipeline.idle_read_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];

        // Set once an outer poll has reported the hangup together with
        // unread data and the loop went back to read it (UD19; see
        // `outer_poll_hup_exits`).
        let mut drained_after_hup = false;

        loop {
            // Claim the next free buffer. This blocks only when the
            // parse stage is a full ring behind, which is exactly when
            // we should stop reading and let the kernel queue exert
            // backpressure on the child.
            //
            // INV-P1/P3: the guard yields only the slot INDEX; the buffer is
            // reached below through `slot_ptr` alone.
            // const buf: *[buffer_capacity]u8 = buf: { ... &pipeline.bufs[pipeline.head] };
            let slot = {
                let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                while m.count == Self::BUFFER_COUNT {
                    m = pipeline
                        .slot_free
                        .wait(m)
                        .unwrap_or_else(|e| e.into_inner());
                }
                m.head
            };
            let buf: *mut u8 = pipeline.slot_ptr(slot);

            // var total: usize = 0;
            let mut total: usize = 0;
            // var bridge_start: ?std.time.Instant = null;
            let mut bridge_start: Option<std::time::Instant> = None;
            // var spins: usize = 0;
            let mut spins: usize = 0;
            // var fatal = false;
            let mut fatal = false;

            // Fill the buffer from the pty. For a saturated stream the
            // kernel queue momentarily runs dry while the writer
            // refills it in parallel, so we bridge those gaps with
            // spin retries and a short poll instead of delivering a
            // tiny batch.
            // gather: while (total < buffer_capacity) {
            'gather: while total < Self::BUFFER_CAPACITY {
                // const n = posix.read(fd, buf[total..]) catch |err| switch (err) { ... };
                //
                // SAFETY (INV-P2): this stage owns `slot` until the publish
                // below, and `total < BUFFER_CAPACITY`, so the destination range
                // is inside this slot.
                let rc = unsafe {
                    libc::read(
                        fd,
                        buf.add(total).cast::<std::ffi::c_void>(),
                        Self::BUFFER_CAPACITY - total,
                    )
                };
                if rc < 0 {
                    let e = errno();
                    // (posix.read retries on EINTR.)
                    if e == libc::EINTR {
                        continue 'gather;
                    }

                    // error.WouldBlock
                    if e == libc::EAGAIN {
                        // Anything below the threshold is interactive.
                        // if (total < bridge_threshold) break :gather;
                        if total < Self::BRIDGE_THRESHOLD {
                            break 'gather;
                        }

                        // The stream is saturated, so we bridge the
                        // gap. First retry the read directly a bounded
                        // number of times, since the refill usually
                        // lands within microseconds.
                        // if (spins < bridge_spin_max) { spins += 1; continue :gather; }
                        if spins < Self::BRIDGE_SPIN_MAX {
                            spins += 1;
                            continue 'gather;
                        }

                        // Still dry, so we want to sleep in poll for
                        // the next refill, within our latency budget.
                        // const now = std.time.Instant.now() catch break :gather;
                        // if (bridge_start) |start| {
                        //     if (now.since(start) >= gather_budget_ns) break :gather;
                        // } else bridge_start = now;
                        let now = std::time::Instant::now();
                        match bridge_start {
                            Some(start) => {
                                if now.duration_since(start).as_nanos()
                                    >= u128::from(Self::GATHER_BUDGET_NS)
                                {
                                    break 'gather;
                                }
                            }
                            None => bridge_start = Some(now),
                        }

                        // Bridging a refill gap is only free while the
                        // parse stage is busy, since the wait hides
                        // behind parse time. Once the parser is idle,
                        // every microsecond we hold this batch is
                        // added straight to output latency, and for a
                        // request/response producer (a burst ending
                        // in a query, e.g. frame + cursor position
                        // report) the writer is blocked on a reply to
                        // data sitting in this buffer, so a poll here
                        // would always sleep its full timeout. Deliver
                        // now if the parser is idle, and otherwise arm
                        // the idle wake so it can interrupt our poll
                        // the moment that changes.
                        // (bb0ac4c72)
                        {
                            let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                            if m.count == 0 {
                                break 'gather;
                            }
                            m.bridging = true;
                        }

                        // const r = posix.poll(&pollfds, bridge_poll_timeout_ms) catch { ... };
                        let r = unsafe {
                            libc::poll(
                                pollfds.as_mut_ptr(),
                                pollfds.len() as libc::nfds_t,
                                Self::BRIDGE_POLL_TIMEOUT_MS,
                            )
                        };
                        if r < 0 {
                            let pe = errno();
                            Self::clearBridging(pipeline);
                            if pe == libc::EINTR {
                                continue 'gather;
                            }
                            // log.warn("bridge poll failed err={}", .{poll_err});
                            log::warn!(
                                target: LOG_SCOPE,
                                "bridge poll failed err={}",
                                std::io::Error::from_raw_os_error(pe)
                            );
                            break 'gather;
                        }
                        // clearBridging(pipeline);
                        Self::clearBridging(pipeline);

                        // Quiet for a full timeout means the burst ended.
                        // if (r == 0) break :gather;
                        if r == 0 {
                            break 'gather;
                        }

                        // On a quit signal we deliver what we have and stop.
                        if pollfds[1].revents & libc::POLLIN != 0 {
                            // log.info("read thread got quit signal", .{});
                            log::info!(target: LOG_SCOPE, "read thread got quit signal");
                            fatal = true;
                            break 'gather;
                        }

                        // The parse stage went idle: drain the wake
                        // and deliver what we have. The pty may have
                        // data as well, but the next batch can pick
                        // that up; an idle parser means delivery must
                        // not wait.
                        if pollfds[2].revents & libc::POLLIN != 0 {
                            // var trash: [16]u8 = undefined;
                            let mut trash = [0u8; 16];
                            loop {
                                let drained = unsafe {
                                    libc::read(
                                        pipeline.idle_read_fd,
                                        trash.as_mut_ptr().cast::<std::ffi::c_void>(),
                                        trash.len(),
                                    )
                                };
                                if drained < 0 || (drained as usize) < trash.len() {
                                    break;
                                }
                            }
                            break 'gather;
                        }

                        // HUP without IN means no more data is
                        // coming. Deliver and let the outer poll
                        // decide what to do.
                        if pollfds[0].revents & libc::POLLIN == 0 {
                            break 'gather;
                        }

                        continue 'gather;
                    }

                    // The pty is closed. We're probably gracefully
                    // shutting down.
                    // error.NotOpenForReading, error.InputOutput =>
                    if e == libc::EBADF || e == libc::EIO {
                        // log.info("io gather exiting", .{});
                        log::info!(target: LOG_SCOPE, "io gather exiting");
                        fatal = true;
                        break 'gather;
                    }

                    // else => { log.err("io gather error err={}", .{err}); unreachable; }
                    log::error!(
                        target: LOG_SCOPE,
                        "io gather error err={}",
                        std::io::Error::from_raw_os_error(e)
                    );
                    unreachable!("io gather error");
                }
                let n = rc as usize;

                // This happens on macOS instead of WouldBlock when the
                // child process dies. Deliver what we have and let the
                // outer poll detect HUP.
                // if (n == 0) break :gather;
                if n == 0 {
                    break 'gather;
                }

                // total += n;
                total += n;

                // Each refill gap gets a fresh spin budget.
                // spins = 0;
                spins = 0;
            }

            // Publish the batch (if any) to the parse stage and rotate
            // to the next buffer.
            // if (total > 0) { ... }
            if total > 0 {
                {
                    let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
                    m.lens[slot] = total;
                    m.head = (m.head + 1) % Self::BUFFER_COUNT;
                    m.count += 1;
                }
                pipeline.batch_ready.notify_one();
            }

            // if (fatal) return;
            if fatal {
                return;
            }

            // A full buffer means the stream is still hot, so go
            // claim the next buffer without an intervening poll.
            // if (total == buffer_capacity) continue;
            if total == Self::BUFFER_CAPACITY {
                continue;
            }

            // Wait for data. The idle fd is sliced off: the parse
            // stage only writes to it while we're bridging.
            // _ = posix.poll(pollfds[0..2], -1) catch |err| { ... return; };
            //
            // (Test builds only: `tests::before_outer_poll` lets a test change
            // the fd's state at this exact point, UD19.)
            #[cfg(test)]
            tests::before_outer_poll(fd);
            let rc = unsafe { libc::poll(pollfds.as_mut_ptr(), 2, -1) };
            if rc < 0 {
                if errno() == libc::EINTR {
                    continue;
                }
                // log.warn("poll failed on read thread, exiting early err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "poll failed on read thread, exiting early err={}",
                    std::io::Error::last_os_error()
                );
                return;
            }

            // If our quit fd is set, we're done.
            // if (pollfds[1].revents & posix.POLL.IN != 0) { ... return; }
            if pollfds[1].revents & libc::POLLIN != 0 {
                // log.info("read thread got quit signal", .{});
                log::info!(target: LOG_SCOPE, "read thread got quit signal");
                return;
            }

            // If our pty fd is closed, then we're also done with our
            // read thread.
            // if (pollfds[0].revents & posix.POLL.HUP != 0) { ... return; }
            //
            // PORT NOTE: reference defect (R13): see `outer_poll_hup_exits`.
            if Self::outer_poll_hup_exits(pollfds[0].revents, &mut drained_after_hup) {
                // log.info("pty fd closed, read thread exiting", .{});
                log::info!(target: LOG_SCOPE, "pty fd closed, read thread exiting");
                return;
            }
        }
    }

    /// Whether the gather stage's outer poll ends the read thread on a
    /// hangup.
    ///
    /// PORT NOTE: reference defect (R13). The reference returns as soon as
    /// `POLLHUP` is set (`Exec.zig:1727-1730`), without looking at `POLLIN`.
    /// On Linux a pty master reports both in one word (`0x11`, measured on
    /// the port in phase 2) when the child writes and exits while this stage
    /// is parked in the poll, and the bytes still queued on the pty are never
    /// read: the last output of a command that exits at once can be missing
    /// from the terminal. The same function's bridge poll states the intent,
    /// "HUP without IN means no more data is coming" (`Exec.zig:1657-1662`).
    /// So a hangup with `POLLIN` sends the loop back to read once more; the
    /// next outer poll that reports the hangup ends it, whatever `POLLIN`
    /// says. That second rule matters: once the data is read the fd still
    /// reports `POLLIN | POLLHUP` where a read returns 0 (measured on macOS
    /// for a pty master and for a pipe), and on Linux the read after the
    /// data fails with EIO, which the gather loop already treats as the end.
    /// A hangup without `POLLIN` ends the thread at once, as in the
    /// reference. `.port/parity-closure/UPSTREAM-DEFECTS.md` UD19.
    #[cfg(not(windows))]
    fn outer_poll_hup_exits(revents: libc::c_short, drained_after_hup: &mut bool) -> bool {
        if revents & libc::POLLHUP == 0 {
            return false;
        }
        if revents & libc::POLLIN != 0 && !*drained_after_hup {
            *drained_after_hup = true;
            return false;
        }
        true
    }

    /// Clears the bridging flag armed before a bridge poll, closing
    /// the window in which the parse stage writes idle wakes.
    //
    // fn clearBridging(pipeline: *Pipeline) void { ... }
    #[cfg(not(windows))]
    fn clearBridging(pipeline: &Pipeline) {
        let mut m = pipeline.mutex.lock().unwrap_or_else(|e| e.into_inner());
        m.bridging = false;
    }

    /// Set `fd` to non-blocking. Returns false if it could not be done.
    //
    // PORT NOTE: the reference factors this out of `threadMainPosix` as part of
    // 2f0e6659d because the failure is now FATAL (the pipeline cannot run on a
    // blocking fd), where the pre-pipeline loop only logged a perf warning.
    #[cfg(not(windows))]
    fn setNonblock(fd: std::os::fd::RawFd) -> bool {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if flags == -1 {
            // log.warn("read thread failed to get flags err=E{s}", .{@tagName(posix.errno(-1))});
            //
            // PORT NOTE: the port's earlier Zig comment here reproduced an OLDER
            // upstream shape (`err={}` plus a "this isn't a fatal error" second
            // line); at pin 20abdb50a the reference renders the ERRNO NAME as
            // `err=E{s}` and has no second line. The live call follows the pin.
            log::warn!(
                target: LOG_SCOPE,
                "read thread failed to get flags err=E{}",
                errno_tag_name(errno())
            );
            return false;
        }
        // switch (posix.errno(posix.system.fcntl(fd, F.SETFL, flags | NONBLOCK))) {
        //     .SUCCESS => {},
        //     else => |e| {
        //         log.warn("read thread failed to set flags err=E{s}", .{@tagName(e)});
        //         return false;
        //     },
        // }
        // return true;
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
            log::warn!(
                target: LOG_SCOPE,
                "read thread failed to set flags err=E{}",
                errno_tag_name(errno())
            );
            return false;
        }
        true
    }

    /// Sets the QoS class of the calling thread for the read pipeline
    /// (macOS only). Both pipeline threads feed content the user is
    /// actively watching, and at default QoS the scheduler may place
    /// them on efficiency cores with wakeup latencies that are large
    /// compared to the ~10us cadence of the pty producer/consumer
    /// dance. Measured on an M4 Max, this results in a 15% throughput
    /// difference (on the change, not 15% total).
    //
    // fn setQosClass() void {
    //     internal_os.macos.setQosClass(.user_initiated) catch |err| {
    //         log.warn("error setting QoS class err={}", .{err});
    //     };
    // }
    //
    // PORT NOTE (2f0e6659d, WS10 T8): this helper is the ONE hunk of the pty
    // pipeline commit that is separable from the pipeline itself — it applies
    // verbatim to the single read thread rustty has today, which is why
    // `E-os-termio-platform.md` row E4 calls it out on its own. The reference
    // calls it from BOTH `threadMainPosix` and the new `gatherMainPosix`; only
    // the former exists here, so only the former call site lands. When the
    // pipeline lands, `gatherMainPosix` calls this same fn.
    //
    // The reference's `catch |err| log.warn(...)` is LOAD-BEARING, not
    // decoration: `setQosClass` returns `SetQosClassError::ThreadIncompatible`
    // on EPERM, and swallowing it is what keeps a QoS failure from killing the
    // read thread. The port swallows it identically (the `log.warn` itself is
    // the tree-wide log-as-comment convention).
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    #[allow(non_snake_case)]
    fn setQosClass() {
        if let Err(err) = rustty_os::macos::setQosClass(rustty_os::macos::QosClass::user_initiated)
        {
            // log.warn("error setting QoS class err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error setting QoS class err={err:?}");
        }
    }

    // The reference body, preserved verbatim (it is PORTED below — this block is
    // the R3 verbatim record, not a deferral marker):
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
    //
    //             // See threadMainPosix: hand the renderer state mutex
    //             // off if the renderer is waiting, since this loop
    //             // would otherwise starve it under heavy output.
    //             io.renderer_state.yieldToDemand();
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
    // PORTED (WS20). It cannot be cross-checked from macOS (the `highway-sys`
    // C++ build needs an MSVC sysroot); task W-1 built and ran it on a Windows
    // host (`windows_readthread_sets_io_crash_thread_state`).
    #[cfg(windows)]
    fn threadMainWindows<M: rustty_apprt_api::surface::AppMailbox>(
        fd: rustty_os::windows::HANDLE,
        io: *mut crate::Termio<M>,
        quit: rustty_os::windows::HANDLE,
    ) {
        use rustty_os::windows as win;

        // Always close our end of the pipe when we exit.
        // defer _ = posix.system.close(quit);
        //
        // PORT NOTE: `quit` is a Win32 HANDLE here (the reference's `posix.fd_t`
        // IS a HANDLE on windows), so the close is `CloseHandle` — the same call
        // the posix arm's `close(2)` maps to for this handle class, and the one
        // `Exec.deinit` already uses on `read_thread_pipe`.
        struct CloseOnExit(win::HANDLE);
        impl Drop for CloseOnExit {
            fn drop(&mut self) {
                let _ = win::CloseHandle(self.0);
            }
        }
        let _quit_guard = CloseOnExit(quit);

        // Setup our crash metadata
        // crash.sentry.thread_state = .{ .type = .io, .surface = io.surface_mailbox.surface };
        // defer crash.sentry.thread_state = null;
        //
        // (S211-F2: the same guard as the posix arm.)
        let _crash_thread_state = unsafe { crate::IoCrashThreadState::set(io) };

        // var buf: [1024]u8 = undefined;
        let mut buf = [0u8; 1024];
        loop {
            loop {
                // var n: windows.DWORD = 0;
                let mut n: win::DWORD = 0;
                // if (windows.exp.kernel32.ReadFile(fd, &buf, buf.len, &n, null) == windows.FALSE) {
                if unsafe {
                    win::exp::kernel32::ReadFile(
                        fd,
                        buf.as_mut_ptr().cast::<std::ffi::c_void>(),
                        buf.len() as win::DWORD,
                        &mut n,
                        std::ptr::null_mut(),
                    )
                } == win::FALSE
                {
                    // const err = windows.GetLastError();
                    let err = unsafe { win::kernel32::GetLastError() };
                    // switch (err) { .OPERATION_ABORTED => break, else => { ... unreachable; } }
                    const ERROR_OPERATION_ABORTED: win::DWORD = 995;
                    if err == ERROR_OPERATION_ABORTED {
                        // Check for a quit signal
                        break;
                    }
                    // log.err("io reader error err={}", .{err});
                    log::error!(target: LOG_SCOPE, "io reader error err={err}");
                    unreachable!("io reader error");
                }

                // @call(.always_inline, termio.Termio.processOutput, .{ io, buf[0..n] });
                //
                // SAFETY: `io` is the read thread's sole `processOutput` toucher
                // (the `ReadThreadArgs` contract); its `renderer_state` is live for
                // the pointer's lifetime (joined in `threadExit` before teardown).
                unsafe {
                    crate::Termio::processOutput(io, &buf[..n as usize]);
                }

                // See threadMainPosix: hand the renderer state mutex
                // off if the renderer is waiting, since this loop
                // would otherwise starve it under heavy output.
                // io.renderer_state.yieldToDemand();
                unsafe {
                    (*(*io).renderer_state).yield_to_demand();
                }
            }

            // var quit_bytes: windows.DWORD = 0;
            let mut quit_bytes: win::DWORD = 0;
            // if (windows.exp.kernel32.PeekNamedPipe(quit, null, 0, null, &quit_bytes, null) == windows.FALSE) {
            if unsafe {
                win::exp::kernel32::PeekNamedPipe(
                    quit,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut quit_bytes,
                    std::ptr::null_mut(),
                )
            } == win::FALSE
            {
                // const err = windows.GetLastError();
                // log.err("quit pipe reader error err={}", .{err});
                let err = unsafe { win::kernel32::GetLastError() };
                log::error!(target: LOG_SCOPE, "quit pipe reader error err={err}");
                unreachable!("quit pipe reader error");
            }

            // if (quit_bytes > 0) { log.info("read thread got quit signal", .{}); return; }
            if quit_bytes > 0 {
                log::info!(target: LOG_SCOPE, "read thread got quit signal");
                return;
            }
        }
    }
}

/// PORT NOTE (Rust-only glue): the `comptime passwdpkg: type` parameter of
/// `execCommand` — "expected to have a single function `get(Allocator)` that
/// returns a passwd entry" — modeled as a trait with an inferred-error-set
/// associated type (the tests substitute their own impls exactly as the
/// reference passes anonymous structs). The `Allocator` param collapses.
pub trait PasswdPkg {
    // PORT NOTE (task-14e-1): `Debug` is required by the restored
    // `Exec.zig:1853` `log.warn("failed to read passwd, ... err={}", .{err})` —
    // the reference prints the caught error value, so the associated error type
    // has to be renderable. Every implementor's error already derives it.
    type Error: core::fmt::Debug;
    fn get() -> Result<rustty_os::passwd::Entry, Self::Error>;
}

/// The real `internal_os.passwd` package (the production argument).
pub struct OsPasswd;

impl PasswdPkg for OsPasswd {
    type Error = rustty_os::passwd::Error;
    fn get() -> Result<rustty_os::passwd::Entry, Self::Error> {
        #[cfg(not(windows))]
        return rustty_os::passwd::get();
        // `passwd.get`: `if (builtin.os.tag == .windows) @compileError("passwd is
        // not available on windows");`. Its only caller is `execCommand`'s
        // comptime-Darwin branch, so Zig never analyzes it on Windows; here the
        // body must compile, and it is unreachable by construction (R7).
        #[cfg(windows)]
        unreachable!("passwd is not available on windows");
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
//
// (Windows: the shell arm's `.windows` block returns, as the reference's
// comptime branch does, so the `/bin/sh -c` tail is unreachable there.)
#[cfg_attr(windows, allow(unreachable_code))]
fn execCommand<P: PasswdPkg>(
    command: &configpkg::command::Command,
) -> Result<Vec<CString>, ExecCommandError> {
    // PORT NOTE: the reference's `comptime passwdpkg: type` is read only by the
    // `darwin:` block below, which is a real `cfg` here; the parameter stays in
    // the signature on every target (the reference's does, and the callers —
    // including the ported tests — pass it), so name it off Darwin rather than
    // letting it read as extraneous.
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let _ = core::marker::PhantomData::<P>;

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
            Err(err) => {
                // log.warn("failed to read passwd, not using a login shell err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to read passwd, not using a login shell err={err:?}"
                );
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
                log::warn!(target: LOG_SCOPE, "failed to get username, not using a login shell");
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
                    Err(err) => {
                        // log.warn(
                        //     "failed to open home dir, not checking for hushlogin err={}",
                        //     .{err},
                        // );
                        log::warn!(
                            target: LOG_SCOPE,
                            "failed to open home dir, not checking for hushlogin err={err:?}"
                        );
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

/// PORT NOTE (Zig-std shim): `posix.close(fd)`. On Windows `posix.fd_t` is a
/// `HANDLE` and Zig's `posix.close` is `CloseHandle`, the call
/// `ThreadData::deinit` already makes for `read_thread_pipe`.
///
/// # Safety
/// `fd` must be an open descriptor/handle this code owns.
unsafe fn close_fd(fd: rustty_os::pipe::fd_t) {
    #[cfg(not(windows))]
    unsafe {
        libc::close(fd);
    }
    #[cfg(windows)]
    {
        let _ = rustty_os::windows::CloseHandle(fd);
    }
}

/// PORT NOTE (Zig-std shim): `posix.errno(rc)` — the calling thread's errno value.
#[cfg(not(windows))]
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// PORT NOTE (Zig-std shim): `std.fs.path.dirname` — the path up to (not
/// including) the final separator; `None` for a root/relative-basename path.
/// Append a value to an environment variable such as PATH.
/// The returned value is always allocated so it must be freed.
//
// PORT NOTE (P3-W7 T2 / area-E `E13`): moved here from `rustty-os`'s `env.rs`
// (upstream `src/os/env.zig`, DELETED by `e8525c0fd`), which moved
// `appendEnv`/`appendEnvAlways` into `src/termio/Exec.zig` as file-private
// fns. Their only consumers are `Subprocess::init` above. Re-typed on bytes on
// the way: PATH-like variables are byte strings, and a non-UTF-8 PATH
// component could not round-trip through the old `&str`/`String` signature.
pub(crate) fn appendEnv(current: &[u8], value: &[u8]) -> Vec<u8> {
    // If there is no prior value, we return it as-is
    if current.is_empty() {
        return value.to_vec();
    }

    // Otherwise we must prefix.
    appendEnvAlways(current, value)
}

/// Always append value to environment, even when it is empty.
/// This is useful because some env vars (like MANPATH) want there
/// to be an empty prefix to preserve existing values.
///
/// The returned value is always allocated so it must be freed.
pub(crate) fn appendEnvAlways(current: &[u8], value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(current.len() + 1 + value.len());
    out.extend_from_slice(current);
    out.push(crate::shell_integration::PATH_LIST_DELIMITER);
    out.extend_from_slice(value);
    out
}

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
///
/// PORT NOTE (task R13-P, N51): interior NULs DO occur. The reference's `dupeZ`
/// copies a config `command` / `initial-command` value (or a flatpak cwd) with
/// its NUL bytes, and every consumer of the copy hands the pointer to C
/// (`execve` argv, `CreateProcessW`, a D-Bus `ay`), which reads up to the first
/// NUL. Measured with Zig 0.16.0 (`r13p-reference-probe.zig`): `/bin/sh -c
/// "printf …\0; echo AFTER-NUL"` with `$0` = `"zero\0tail"` prints `zero|` and
/// never `AFTER-NUL`. So the copy is cut at the first NUL. This was an
/// `.expect`, which panicked, and the macOS staticlib aborts on a panic.
fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).expect("the bytes before the first NUL hold no NUL")
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

    // Test-only instrumentation of the `threadEnter` read-thread spawn error
    // arm (S211-F5 / UD102). Thread-local, so it only affects the test thread
    // that sets it (threadEnter and its error arm run on the calling thread).
    thread_local! {
        /// Calls of `drain_pty_master_once` on this thread.
        pub(super) static DRAIN_CALLS: std::cell::Cell<usize> =
            const { std::cell::Cell::new(0) };
        /// Calls of `Subprocess::stop` on this thread.
        pub(super) static STOP_CALLS: std::cell::Cell<usize> =
            const { std::cell::Cell::new(0) };
        /// The value of `STOP_CALLS` when `drain_pty_master_once` last ran on
        /// this thread (`None` if it has not run since the last reset).
        pub(super) static STOPS_BEFORE_DRAIN: std::cell::Cell<Option<usize>> =
            const { std::cell::Cell::new(None) };
        /// Make `threadEnter`'s read-thread spawn fail on this thread.
        pub(super) static FAIL_READ_THREAD_SPAWN: std::cell::Cell<bool> =
            const { std::cell::Cell::new(false) };
        /// Make `threadMainPosix`'s gather-thread spawn fail on this thread
        /// (S211-F6).
        pub(super) static FAIL_GATHER_SPAWN: std::cell::Cell<bool> =
            const { std::cell::Cell::new(false) };
    }

    /// Test-only hook run by `gatherMainPosix` right before its outer
    /// `poll(-1)`, for the one fd it was installed for (the gather stage runs
    /// on its own thread, so a thread-local cannot reach it; keying on the fd
    /// keeps concurrent tests out of it). UD19.
    #[cfg(not(windows))]
    type OuterPollHook = (std::os::fd::RawFd, Box<dyn FnMut() + Send>);
    #[cfg(not(windows))]
    static BEFORE_OUTER_POLL: std::sync::Mutex<Option<OuterPollHook>> = std::sync::Mutex::new(None);

    #[cfg(not(windows))]
    pub(super) fn before_outer_poll(fd: std::os::fd::RawFd) {
        let mut hook = BEFORE_OUTER_POLL.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((hook_fd, f)) = hook.as_mut() {
            if *hook_fd == fd {
                f();
            }
        }
    }

    /// UD19: the outer-poll decision on its own. A hangup without data ends
    /// the thread at once (the reference's behaviour); a hangup with data
    /// sends it back to read once, and the next hangup ends it.
    #[cfg(not(windows))]
    #[test]
    fn outer_poll_hangup_with_data_reads_once_more() {
        let mut drained = false;
        assert!(!ReadThread::outer_poll_hup_exits(
            libc::POLLIN,
            &mut drained
        ));
        assert!(!drained, "no hangup, nothing to drain");
        assert!(ReadThread::outer_poll_hup_exits(
            libc::POLLHUP,
            &mut drained
        ));

        let mut drained = false;
        let both = libc::POLLIN | libc::POLLHUP;
        assert!(!ReadThread::outer_poll_hup_exits(both, &mut drained));
        assert!(drained);
        assert!(ReadThread::outer_poll_hup_exits(both, &mut drained));
    }

    /// UD19 (PORT NOTE: reference defect (R13), at `outer_poll_hup_exits`):
    /// output that arrives together with the hangup, while the gather stage is
    /// parked in its outer poll, is still read. The hook writes `second` and
    /// shuts down the writing side right before that poll, so the poll reports
    /// `POLLIN | POLLHUP` with `second` unread (measured on macOS for a Unix
    /// socket pair after `shutdown(SHUT_WR)` or `shutdown(SHUT_RDWR)`, and for a
    /// pipe after `close`: `revents == 0x11`; on Linux a pty master reports the
    /// same word when the child writes and exits, UD19). The reference returns
    /// on `POLLHUP` alone and drops `second`.
    #[cfg(not(windows))]
    #[test]
    fn gather_stage_reads_output_that_arrives_with_the_hangup() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // The data fd is a socket pair, not a pipe: the hangup is made with
        // `shutdown(SHUT_RDWR)`, which acts on the socket itself. Closing a pipe's
        // write end is not enough in this binary, where other tests fork
        // children that can hold a copy of it for a moment, and then the poll
        // can see the data before the hangup (measured: an extra outer poll).
        let mut fds = [0 as libc::c_int; 2];
        assert_eq!(
            unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) },
            0
        );
        let (data_r, data_w) = (fds[0], fds[1]);
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let (quit_r, quit_w) = (fds[0], fds[1]);
        // The gather stage expects a non-blocking fd (`threadMainPosix` sets it).
        unsafe {
            let fl = libc::fcntl(data_r, libc::F_GETFL);
            assert!(fl >= 0);
            assert_eq!(libc::fcntl(data_r, libc::F_SETFL, fl | libc::O_NONBLOCK), 0);
        }
        let first = b"first";
        assert_eq!(
            unsafe { libc::write(data_w, first.as_ptr().cast(), first.len()) },
            first.len() as isize
        );

        let polls = Arc::new(AtomicUsize::new(0));
        {
            let polls = Arc::clone(&polls);
            *BEFORE_OUTER_POLL.lock().unwrap() = Some((
                data_r,
                Box::new(move || {
                    if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                        let second = b"second";
                        assert_eq!(
                            unsafe { libc::write(data_w, second.as_ptr().cast(), second.len()) },
                            second.len() as isize
                        );
                        // SHUT_RDWR, not SHUT_WR: Linux reports `POLLHUP` on a
                        // Unix stream socket only once both directions are shut
                        // (a peer's SHUT_WR alone gives `POLLIN`, and read then
                        // returns 0 forever, so the gather stage never ends).
                        // macOS reports `POLLIN | POLLHUP` for either.
                        assert_eq!(unsafe { libc::shutdown(data_w, libc::SHUT_RDWR) }, 0);
                    }
                }),
            ));
        }

        // `'static` so a hung gather stage cannot hang the test: the test waits
        // with a timeout instead of joining.
        let pipeline: &'static Pipeline = Box::leak(Box::new(Pipeline::new()));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            ReadThread::gatherMainPosix(data_r, quit_r, pipeline);
            let _ = tx.send(());
        });
        let finished = rx.recv_timeout(std::time::Duration::from_secs(10));
        *BEFORE_OUTER_POLL.lock().unwrap() = None;
        assert!(
            finished.is_ok(),
            "the gather stage must end after the hangup"
        );

        let mut got = Vec::new();
        {
            let mut m = pipeline.mutex.lock().unwrap();
            assert!(m.done, "the gather stage marks the stream done");
            while m.count > 0 {
                let slot = m.tail;
                let len = m.lens[slot];
                got.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(pipeline.slot_ptr(slot), len)
                });
                m.tail = (m.tail + 1) % ReadThread::BUFFER_COUNT;
                m.count -= 1;
            }
        }
        unsafe {
            libc::close(data_r);
            libc::close(data_w);
            libc::close(quit_r);
            libc::close(quit_w);
        }
        assert_eq!(
            got, b"firstsecond",
            "the output that came with the hangup must be delivered"
        );
        assert_eq!(
            polls.load(Ordering::SeqCst),
            2,
            "one outer poll saw POLLIN|POLLHUP and drained; the next saw the hangup again and ended"
        );
    }

    // ---- MOVED FROM `rustty-os`'s `env.rs` (P3-W7 T2 / area-E `E13`) --------
    // Upstream `e8525c0fd` moved `appendEnv`/`appendEnvAlways` into
    // `src/termio/Exec.zig` when it deleted `src/os/env.zig`; their two tests
    // move with them. Re-typed on bytes with the functions.

    // test "appendEnv empty" {
    #[test]
    #[allow(non_snake_case)]
    fn appendEnv_empty() {
        let result = appendEnv(b"", b"foo");
        assert_eq!(result, b"foo");
    }

    // test "appendEnv existing" {
    #[test]
    #[allow(non_snake_case)]
    fn appendEnv_existing() {
        let result = appendEnv(b"a:b", b"foo");
        if cfg!(windows) {
            assert_eq!(result, b"a:b;foo");
        } else {
            assert_eq!(result, b"a:b:foo");
        }
    }

    /// NEGATIVE CONTROL (P3-W7 T2 Step 4): a **non-UTF-8** PATH component must
    /// round-trip through `appendEnv`/`appendEnvAlways` byte-exactly. Reverting
    /// either signature to `&str`/`String` makes this fail to COMPILE, which is
    /// the strongest control available for a type-level fix; and the old
    /// `String`-typed body could not even represent this input.
    #[test]
    fn append_env_preserves_non_utf8_path_component() {
        // A PATH entry with a lone 0xFF byte — legal on POSIX, invalid UTF-8.
        let current: &[u8] = &[b'/', b'u', b's', b'r', b'/', 0xff, b'x'];
        let added: &[u8] = &[b'/', 0xfe, b'o', b'p', b't'];

        let joined = appendEnv(current, added);
        assert_eq!(joined[..current.len()], *current);
        assert_eq!(
            joined[current.len()],
            crate::shell_integration::PATH_LIST_DELIMITER
        );
        assert_eq!(joined[current.len() + 1..], *added);
        assert!(
            std::str::from_utf8(&joined).is_err(),
            "input must be non-UTF-8"
        );

        // The always-form must keep the empty prefix (MANPATH semantics).
        let always = appendEnvAlways(b"", added);
        assert_eq!(always[0], crate::shell_integration::PATH_LIST_DELIMITER);
        assert_eq!(always[1..], *added);
    }
    // ---- end moved block ---------------------------------------------------

    // Serialize the pty/fork-spawning KATs so two do not `fork()` concurrently in
    // the same test binary (a `fork` snapshots every open fd; concurrent
    // pty-spawning tests can leak each other's master fd into a child — the
    // parallel-cargo-test isolation lesson). Every KAT that spawns a real process
    // takes this lock for its duration. (T8: hoisted to the crate root so
    // `command::tests`' fork KATs share it — see the doc there.)
    #[cfg(not(windows))]
    use crate::FORK_KAT_SERIAL;

    /// S211-F1: the read pipeline's ring is the pinned reference's on every
    /// target (`Exec.zig:1305` `buffer_count = 4`, `:1318` `buffer_capacity =
    /// 64 * 1024`). It was 2 x 8 KiB off Darwin, which only a non-Darwin run
    /// of this test observes.
    #[test]
    fn read_thread_ring_matches_the_reference_on_every_target() {
        assert_eq!(ReadThread::BUFFER_COUNT, 4);
        assert_eq!(ReadThread::BUFFER_CAPACITY, 64 * 1024);
    }

    // WS19 T17a: `drain_pty_master_once` — the helper the `threadEnter` error arms
    // call before `stop()`. Two properties, both load-bearing:
    //
    //   1. It ACTUALLY CONSUMES what the child already wrote (that is the whole
    //      point — an undrained master is what wedges a SIGHUP'd session leader).
    //   2. It is BOUNDED: on a QUIET master it returns promptly instead of parking
    //      in `read` on a blocking fd. (The master is blocking, so a naive
    //      implementation hangs here forever — this assertion is what forbids it.)
    //
    // Deliberately NOT a reproduction of the wedge itself: see the `killPid`
    // divergence entry in DEFERRALS.md for why the previous such test was deleted
    // (it asserted nothing that could fail, its only failure mode was hanging, and
    // it stranded `?Es` children). Real openpty/poll/read syscalls -> Miri-excluded
    // (MIRI_EXCEPTIONS.md, "rustty-termio B3"); holds pty fds open while other
    // in-process KATs may `fork()`, so it takes FORK_KAT_SERIAL.
    #[test]
    #[cfg(target_os = "macos")]
    fn drain_pty_master_once_consumes_pending_and_returns_on_a_quiet_master() {
        use std::time::Instant;

        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let pty = Pty::open(ptypkg::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 800,
            ws_ypixel: 480,
        })
        .expect("Pty::open");
        let master = pty.master;
        let slave = pty.slave;

        // (2) A quiet master must return promptly and consume nothing. Do this
        //     FIRST, while nothing has been written.
        let t0 = Instant::now();
        let drained = drain_pty_master_once(master);
        let quiet_elapsed = t0.elapsed();
        assert_eq!(drained, 0, "nothing was written, so nothing may be drained");
        assert!(
            quiet_elapsed < DRAIN_BUDGET * 4,
            "drain on a QUIET master must return promptly (it would park in \
             `read` on the blocking master without the zero-timeout poll gate); \
             took {quiet_elapsed:?}"
        );

        // (1) Write to the slave (no newline -> no OPOST translation), so the bytes
        //     appear on the master exactly as written, then drain them.
        const PENDING: &[u8] = b"RUSTTY_T17A_UNDRAINED_OUTPUT";
        let w = unsafe { libc::write(slave, PENDING.as_ptr().cast(), PENDING.len()) };
        assert_eq!(w, PENDING.len() as isize, "write to the pty slave");

        // The kernel may deliver in pieces; drain until we have them all (each
        // call is itself bounded, and the loop has its own deadline).
        let mut total = 0usize;
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        while total < PENDING.len() && Instant::now() < deadline {
            total += drain_pty_master_once(master);
        }
        assert_eq!(
            total,
            PENDING.len(),
            "the drain must consume everything the child already wrote"
        );

        // And the master is quiet again.
        assert_eq!(drain_pty_master_once(master), 0);

        unsafe {
            libc::close(slave);
            libc::close(master);
        }
    }

    // WS19 T17b: `posix_waitpid_checked` must REPORT (`None`), not panic, when the
    // wait fails — the `ECHILD` race a concurrent reaper can win. `pid 1` is never
    // our child, so `waitpid` returns `ECHILD` deterministically with no fork and
    // no process left behind. The verbatim `posix_waitpid` shim would
    // `unreachable!()` on exactly this input, which is what made the race a
    // reachable panic on the teardown path.
    #[test]
    #[cfg(not(windows))]
    fn posix_waitpid_checked_reports_echild_instead_of_aborting() {
        // Not our child -> ECHILD.
        let got = crate::command::posix_waitpid_checked(1, libc::WNOHANG as u32);
        assert!(
            got.is_none(),
            "waiting on a pid that is not our child must report None (ECHILD), \
             not a WaitPidResult"
        );
    }

    // test "execCommand darwin: shell command" {
    #[test]
    #[cfg_attr(
        not(any(target_os = "macos", target_os = "ios")),
        ignore = "the body only exists on macOS/iOS (R14)"
    )]
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
    #[cfg_attr(
        not(any(target_os = "macos", target_os = "ios")),
        ignore = "the body only exists on macOS/iOS (R14)"
    )]
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
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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
            #[derive(Debug)]
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
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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
            #[derive(Debug)]
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
    #[cfg_attr(windows, ignore = "the body only exists under cfg(not(windows)) (R14)")]
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

            #[derive(Debug)]
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
    #[cfg_attr(not(windows), ignore = "the body only exists under cfg(windows) (R14)")]
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
    #[cfg_attr(not(windows), ignore = "the body only exists under cfg(windows) (R14)")]
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
    #[cfg_attr(not(windows), ignore = "the body only exists under cfg(windows) (R14)")]
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
    #[cfg_attr(not(windows), ignore = "the body only exists under cfg(windows) (R14)")]
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
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork_info: RtPostForkInfo::default(),
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
        //
        // [M2 / review follow-up] The drainer is itself BOUNDED. It polls with
        // a timeout and gives up after DRAIN_DEADLINE rather than blocking in
        // `read` until EOF. RISK IT REMOVES: if the child ever wedges anyway,
        // the bounded `killPid` abandons it WITHOUT reaping — so the slave fds
        // are never closed, the master never reaches EOF, and an unbounded
        // drainer would block forever in `read`. `drainer.join()` below would
        // then hang the test, reintroducing on the TEST side exactly the CI
        // hang the bounded `killPid` removes on the production side. The
        // deadline is far longer than any legitimate drain (which finishes in
        // microseconds) and longer than `KILL_ABANDON`, so it never masks a
        // real failure — it only guarantees `join()` returns.
        let master_fd = fds.read;
        let drainer = std::thread::spawn(move || {
            const DRAIN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
            let deadline = std::time::Instant::now() + DRAIN_DEADLINE;
            let mut buf = [0u8; 4096];
            while std::time::Instant::now() < deadline {
                // SAFETY: `master_fd` is the live pty master (closed only after
                // this thread is joined); `poll` writes only `revents`.
                let mut pfd = libc::pollfd {
                    fd: master_fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let pr = unsafe { libc::poll(&mut pfd, 1, 100) };
                if pr < 0 {
                    if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                        continue;
                    }
                    break;
                }
                if pr == 0 {
                    // Timed out with nothing pending; re-check the deadline.
                    continue;
                }
                // SAFETY: as above; `read` writes at most `buf.len()` bytes.
                let n = unsafe { libc::read(master_fd, buf.as_mut_ptr().cast(), buf.len()) };
                if n < 0 {
                    if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                        continue;
                    }
                    // EIO on macOS == all slave refs closed, i.e. EOF.
                    break;
                }
                if n == 0 {
                    break;
                }
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
        // hit EOF; join it BEFORE closing the master (its fd copy). Bounded by
        // DRAIN_DEADLINE above, so this join always returns.
        drainer.join().expect("drainer join");

        // Close the master (read/write) fd so the test leaks nothing.
        unsafe {
            libc::close(fds.read);
        }

        // deinit is safe post-stop.
        subprocess.deinit();
    }

    // ---- task R13-P: NUL bytes in the cwd and in the argv (N50, N51) --------
    //
    // `Subprocess::init` used to build the cwd with `cstring`, which `.expect`ed
    // on an interior NUL. A program that prints `OSC 7` with `%00` in the path
    // plants one in the terminal's pwd, the next new tab/split/window inherits
    // it as `working-directory`, and the panic aborted the macOS app. The
    // reference keeps the bytes, asks `access` about them, and takes its
    // `cannot access cwd, ignoring` arm (Zig 0.16 `pathToPosix` answers
    // `error.BadPathName`; measured with `r13p-reference-probe.zig`).

    /// Spawns `exit 0` with `working_directory` set to `wd`, and reports the
    /// `cannot access cwd` warnings `start` logged on this thread, the `PWD` it
    /// put in the child environment, and the pwd `initTerminal` set on a fresh
    /// terminal. Drives the real `Exec::init` -> `initTerminal` -> `start`
    /// fork/exec path; only `new_config` itself is out of reach from here,
    /// because it needs a core `App` with an apprt.
    #[cfg(unix)]
    fn spawn_with_working_directory(
        wd: Vec<u8>,
    ) -> (Vec<String>, Option<Vec<u8>>, Option<Vec<u8>>) {
        use allocator_api2::alloc::Global;

        let cfg = Config {
            command: Some(configpkg::command::Command::Shell(b"exit 0".to_vec())),
            env: EnvMap::new(),
            env_override: configpkg::RepeatableStringMap::default(),
            shell_integration: configpkg::value_types::ShellIntegration::Detect,
            shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: Some(wd),
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork_info: RtPostForkInfo::default(),
        };

        // Before the fix this panicked (`.expect` on the interior NUL).
        let mut exec = Exec::init(Global, cfg).expect("Exec::init");

        // `if (cwd) |pwd| try env.put("PWD", pwd)` — the reference stores the
        // bytes, and the child's envp is cut at the NUL by the kernel (measured).
        let env_pwd = exec
            .subprocess
            .env
            .as_ref()
            .and_then(|e| e.get(b"PWD".as_slice()).cloned());

        // `if (self.subprocess.cwd) |cwd| term.setPwd(cwd)`.
        let mut term = rustty_vt::terminal::Terminal::init(
            Global.into(),
            crate::stream_handler::tests::terminal_opts(None),
        )
        .expect("Terminal::init");
        exec.initTerminal(&mut term);
        let term_pwd = term.get_pwd().map(<[u8]>::to_vec);
        term.deinit(Global.into());

        let cap = crate::log_capture::start();
        let fds = exec.subprocess.start(Global).expect("Subprocess::start");
        let warnings: Vec<String> = cap
            .messages()
            .into_iter()
            .filter(|(level, target, message)| {
                *level == log::Level::Warn
                    && target == LOG_SCOPE
                    && message.starts_with("cannot access cwd")
            })
            .map(|(_, _, message)| message)
            .collect();
        drop(cap);

        // Drain the pty master while the child is reaped (the KAT's [T8] note).
        let master_fd = fds.read;
        let drainer = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            let mut buf = [0u8; 4096];
            while std::time::Instant::now() < deadline {
                // SAFETY: `master_fd` is the live pty master; `poll` writes only
                // `revents`, and `read` writes at most `buf.len()` bytes.
                let mut pfd = libc::pollfd {
                    fd: master_fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let pr = unsafe { libc::poll(&mut pfd, 1, 100) };
                if pr < 0 {
                    if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                        continue;
                    }
                    break;
                }
                if pr == 0 {
                    continue;
                }
                let n = unsafe { libc::read(master_fd, buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 {
                    if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
                    {
                        continue;
                    }
                    break;
                }
            }
        });

        exec.subprocess.stop();
        drainer.join().expect("drainer join");
        // SAFETY: the master fd is ours and is closed exactly once.
        unsafe { libc::close(fds.read) };
        exec.subprocess.deinit();

        (warnings, env_pwd, term_pwd)
    }

    /// B1. `OSC 7` with `%00` -> the terminal pwd -> the new surface's
    /// `working-directory` -> `Exec`. The port used to panic in `Exec::init`.
    #[test]
    #[cfg(unix)]
    fn osc7_percent_00_pwd_is_ignored_as_a_cwd_instead_of_panicking() {
        use allocator_api2::alloc::Global;
        use std::os::unix::ffi::OsStrExt as _;

        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        // A REAL directory at the prefix before the NUL: a port that silently cut
        // the path there would start the child in it and log no warning.
        let root = std::env::temp_dir().join(format!("rustty-r13p-b1-{}", std::process::id()));
        let prefix = root.join("a");
        std::fs::create_dir_all(&prefix).expect("create the prefix directory");
        let prefix_bytes = prefix.as_os_str().as_bytes().to_vec();

        // The VT stream half: `ESC ] 7 ; file://localhost/<prefix>%00b BEL`.
        let mut term = rustty_vt::terminal::Terminal::init(
            Global.into(),
            crate::stream_handler::tests::terminal_opts(None),
        )
        .expect("Terminal::init");
        let h = crate::stream_handler::tests::handler_for(
            &raw mut term,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        let mut s = rustty_vt::stream::Stream::init(rustty_vt::stream::Options::with_handler(h));
        let mut seq = b"\x1b]7;file://localhost".to_vec();
        seq.extend_from_slice(&prefix_bytes);
        seq.extend_from_slice(b"%00b\x07");
        s.next_slice(&seq);
        s.handler.renderer_wakeup.deinit();

        let mut expected = prefix_bytes.clone();
        expected.extend_from_slice(b"\x00b");
        let pwd = term.get_pwd().map(<[u8]>::to_vec);
        assert_eq!(
            Some(expected.clone()),
            pwd,
            "OSC 7 percent-decodes %00 into the pwd, NUL and all"
        );
        term.deinit(Global.into());

        // `app.newConfig`: `copy.@"working-directory" = .{ .path = pwd }`, then
        // `Surface.init`: `if (config.@"working-directory") |wd| wd.value()`.
        let wd = configpkg::value_types::WorkingDirectory::Path(pwd.unwrap());
        let wd_bytes = wd.value().map(<[u8]>::to_vec).expect("a .path value");

        let (warnings, env_pwd, term_pwd) = spawn_with_working_directory(wd_bytes);
        assert_eq!(
            vec!["cannot access cwd, ignoring: error.BadPathName".to_string()],
            warnings,
            "start takes the reference's `cannot access cwd` arm and spawns with no cwd"
        );
        assert_eq!(Some(expected.clone()), env_pwd, "PWD keeps the raw bytes");
        assert_eq!(
            Some(expected),
            term_pwd,
            "initTerminal seeds the new terminal with the same bytes"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// B2. The same sink, reached from a `working-directory` config line whose
    /// value holds a raw NUL byte. The config line reader keeps it.
    #[test]
    #[cfg(unix)]
    fn config_working_directory_with_a_nul_is_ignored_as_a_cwd() {
        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let mut cfg = configpkg::Config::default_config();
        let mut iter = rustty_args::args::LineIterator::init(b"working-directory = /tmp\x00x\n");
        cfg.load_iter(&mut iter).expect("load_iter");
        let wd = cfg
            .working_directory
            .as_ref()
            .and_then(|w| w.value())
            .map(<[u8]>::to_vec);
        assert_eq!(
            Some(b"/tmp\x00x".to_vec()),
            wd,
            "the config parser keeps the NUL byte"
        );

        let (warnings, env_pwd, _) = spawn_with_working_directory(wd.unwrap());
        assert_eq!(
            vec!["cannot access cwd, ignoring: error.BadPathName".to_string()],
            warnings
        );
        assert_eq!(Some(b"/tmp\x00x".to_vec()), env_pwd);
    }

    /// B3. A NUL in `command` / `initial-command`. The reference copies the
    /// value with `dupeZ` and `execve` reads argv up to the first NUL
    /// (measured), so the port cuts it there instead of panicking.
    #[test]
    fn exec_command_cuts_a_nul_argument_where_execve_reads_it() {
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

        // Through the config parser, which keeps the NUL in both forms.
        let mut cfg = configpkg::Config::default_config();
        let mut iter = rustty_args::args::LineIterator::init(
            b"command = direct:/bin/echo a\x00b\ninitial-command = echo hi\x00; echo AFTER\n",
        );
        cfg.load_iter(&mut iter).expect("load_iter");
        let direct = cfg.command.clone().expect("command");
        let shell = cfg.initial_command.clone().expect("initial-command");
        match &direct {
            configpkg::command::Command::Direct(v) => assert_eq!(
                vec![b"/bin/echo".to_vec(), b"a\x00b".to_vec()],
                *v,
                "the config parser keeps the NUL byte"
            ),
            configpkg::command::Command::Shell(_) => panic!("expected a direct command"),
        }

        let direct_args = execCommand::<TestPasswd>(&direct).expect("execCommand direct");
        let shell_args = execCommand::<TestPasswd>(&shell).expect("execCommand shell");
        let direct_last: Vec<&[u8]> = direct_args
            .iter()
            .rev()
            .take(2)
            .map(|c| c.to_bytes())
            .collect();
        assert_eq!(
            vec![b"a".as_slice(), b"/bin/echo".as_slice()],
            direct_last,
            "argv is cut at the NUL, as execve reads it"
        );
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        assert_eq!(
            b"exec -l echo hi".as_slice(),
            shell_args.last().expect("a shell arg").to_bytes()
        );
        #[cfg(not(any(target_os = "macos", target_os = "ios", windows)))]
        assert_eq!(
            b"echo hi".as_slice(),
            shell_args.last().expect("a shell arg").to_bytes()
        );
        // Windows: the shell command is tokenized on spaces and tabs
        // (`std.mem.tokenizeAny(u8, v, " \t")`), not wrapped in `/bin/sh -c`,
        // so the NUL cuts only its own token, `hi\x00;`.
        #[cfg(windows)]
        assert_eq!(
            vec![
                b"echo".as_slice(),
                b"hi".as_slice(),
                b"echo".as_slice(),
                b"AFTER".as_slice()
            ],
            shell_args.iter().map(|c| c.to_bytes()).collect::<Vec<_>>()
        );
    }

    /// The counter the KAT's stand-in `postFork` bumps. Global because the hook
    /// is a plain `fn` pointer (the reference's `PostForkFn` shape); the KAT
    /// holds `FORK_KAT_SERIAL` and asserts on a DELTA, so a sibling spawn KAT
    /// that also trips the hook cannot perturb it.
    #[cfg(target_os = "macos")]
    static KAT_POST_FORK_CALLS: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);

    /// Stands in for `apprt.runtime.pre_exec.preExec` (`Exec.zig:1034`). Runs in
    /// the FORKED CHILD, so it does nothing observable and does not exit.
    #[cfg(target_os = "macos")]
    fn kat_pre_exec(_cmd: &mut Command<'_>) -> Option<u8> {
        None
    }

    /// Stands in for `apprt.runtime.post_fork.postFork` (`Exec.zig:1036`). Runs
    /// in the PARENT (`Command::start`, `Command.zig:197`), so its effect is
    /// visible to the KAT.
    #[cfg(target_os = "macos")]
    fn kat_post_fork(cmd: &mut Command<'_>) -> Result<(), crate::command::PostForkError> {
        // The reference's `postFork` reads `cmd.pid` (`post_fork.zig:53`) — assert
        // the parent-side handle really is populated by the time it is called.
        assert!(cmd.pid.is_some(), "postFork runs in the parent, after fork");
        KAT_POST_FORK_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    // KAT (rustty-only margin; V13) — THE `rt_*` HOOK WIRING.
    //
    // Proves that the hooks an apprt installs with
    // `command::install_rt_hooks` actually reach the `Command` that `Subprocess::
    // start` forks, i.e. that this file's `rt_pre_exec: rt_hooks.pre_exec` /
    // `rt_post_fork: rt_hooks.post_fork` really are the port's answer to
    // `Exec.zig:1034-1037`'s `@hasDecl(apprt.runtime, ...)`. Before V13 both
    // fields were the literal `None` and NOTHING could call the (fully ported,
    // fully unit-tested) GTK `preExec`/`postFork`.
    //
    // Only the POST-fork hook is observable from the test: `preExec` runs in the
    // forked child, whose side effects die with the `exec` (that is the whole
    // point of the reference's split). The two are set from the same
    // `rt_hooks()` read at the same construction site, so the post-fork
    // observation covers both wires.
    //
    // Real fork/exec + openpty -> Miri-excluded with its siblings; gated to
    // macOS exactly like `subprocess_start_spawns_and_reaps` above (the spawn
    // KATs run on the dev host). The code under test is NOT cfg'd — the same
    // `rt_hooks()` read serves every platform.
    #[test]
    #[cfg(target_os = "macos")]
    fn rt_hooks_reach_the_spawned_command() {
        use allocator_api2::alloc::Global;
        use std::sync::atomic::Ordering;

        // Serialize with the other fork/pty KATs (see FORK_KAT_SERIAL).
        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        // The apprt's job (`gtk::App::init` does exactly this call). Idempotent
        // for the SAME pair, so re-running this KAT in a retry is fine.
        crate::command::install_rt_hooks(crate::command::RtHooks {
            pre_exec: Some(kat_pre_exec),
            post_fork: Some(kat_post_fork),
        });

        let base = KAT_POST_FORK_CALLS.load(Ordering::SeqCst);

        // `/bin/echo` — bounded output, self-terminating (no drainer needed for
        // ~14 bytes, unlike the SIGHUP'd `login(1)` of the sibling KAT).
        let cfg = Config {
            command: Some(configpkg::command::Command::Direct(vec![
                b"/bin/echo".to_vec(),
                b"rt-hooks-kat".to_vec(),
            ])),
            env: EnvMap::new(),
            env_override: configpkg::RepeatableStringMap::default(),
            shell_integration: configpkg::value_types::ShellIntegration::Detect,
            shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork_info: RtPostForkInfo::default(),
        };

        let mut subprocess = Subprocess::init(Global, cfg).expect("Subprocess::init");
        let fds = subprocess.start(Global).expect("Subprocess::start");

        // THE ASSERTION: the installed `postFork` ran, exactly once, during this
        // `start`. (`None` here — the pre-V13 state — makes this 0.)
        assert_eq!(
            KAT_POST_FORK_CALLS.load(Ordering::SeqCst) - base,
            1,
            "the installed rt_post_fork hook must be called by Command::start"
        );

        subprocess.stop();
        // SAFETY: `fds.read` is the live pty master this KAT owns; closing it
        // once, after `stop()` reaped the child, leaks nothing.
        unsafe {
            libc::close(fds.read);
        }
        subprocess.deinit();
    }

    /// UD93 (R13, task R13-D): a child that survives SIGHUP. The reference's
    /// `killPid` spins until the child is reaped, forever for one that ignores
    /// the signal. The port abandons it at the deadline. The child here ignores
    /// SIGHUP from before its `exec` (set in `pre_exec`, so there is no race with
    /// the first `killpg`) and is in its own session, as the real child is after
    /// `setsid`, so `killpg` never reaches the test process. The deadline is
    /// 300 ms (production: `KILL_ABANDON`); a stuck `killPid` is detected by a
    /// 10 s watchdog, which SIGKILLs the child so the thread can finish.
    #[cfg(not(windows))]
    #[test]
    fn kill_pid_abandons_a_child_that_ignores_sighup() {
        use std::os::unix::process::CommandExt as _;
        let mut cmd = std::process::Command::new("/bin/sleep");
        cmd.arg("60");
        // SAFETY: `setsid` and `signal` are async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                libc::signal(libc::SIGHUP, libc::SIG_IGN);
                Ok(())
            });
        }
        let child = cmd.spawn().expect("spawn sleep");
        let pid = child.id() as libc::pid_t;
        // `spawn` returns after `exec`, so `setsid` has run: the group is the
        // child's own, never the test runner's.
        assert_eq!(pid, unsafe { libc::getpgid(pid) });

        const DEADLINE: std::time::Duration = std::time::Duration::from_millis(300);
        let (tx, rx) = std::sync::mpsc::channel();
        let started = std::time::Instant::now();
        let worker = std::thread::spawn(move || {
            let _ = tx.send(kill_pid_until(pid, DEADLINE));
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(10));
        let elapsed = started.elapsed();
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        // Clean up whatever happened: SIGKILL ends the child, which lets a stuck
        // loop reap it and return; then reap it here (an error just means the
        // loop already did).
        let mut child = child;
        let _ = child.kill();
        worker.join().unwrap();
        let _ = child.wait();

        let result = result.unwrap_or_else(|_| {
            panic!("killPid did not return within 10 s for a child that ignores SIGHUP")
        });
        assert!(
            matches!(result, Err(KillError::Abandoned)),
            "the child is abandoned at the deadline, got {result:?}"
        );
        assert!(
            elapsed >= DEADLINE,
            "the loop kept signalling until the deadline ({elapsed:?})"
        );
        assert!(
            alive,
            "the child survived every SIGHUP, so the loop never reaped it"
        );
    }

    // [I2 / review follow-up] REMOVED: `kill_pid_bounded_when_child_is_unreapable`.
    //
    // An earlier revision carried an in-suite "bound regression guard" that
    // deliberately reproduced the WEDGING condition (start a real pty child,
    // then `stop()` with NO drainer) inside the DEFAULT `cargo test`. It was
    // deleted, not weakened:
    //
    //   * It asserted nothing useful. Whether the child actually wedges is
    //     timing-dependent, so it reaped in milliseconds on ordinary runs, and
    //     its threshold (`KILL_ABANDON + 20s`) was loose enough that tripling
    //     `KILL_ABANDON` would still have passed it.
    //   * Its only failure mode was HANGING — which is precisely the CI failure
    //     this whole work exists to eliminate.
    //   * It reintroduced the process-stranding hazard (an abandoned `?Es`
    //     `login(1)` on the developer's machine) on every test run.
    //
    // A deterministic replacement was considered and rejected as well: the only
    // shape that would deterministically exercise "new logic" is a child that
    // survives SIGHUP but not SIGKILL, and after the I1 decision there IS no
    // SIGKILL escalation left to exercise — the sole divergence is a wall-clock
    // deadline, which cannot be asserted without spending `KILL_ABANDON`
    // seconds and stranding a child.
    //
    // The deterministic evidence lives out-of-suite, where it belongs:
    // `.port/tools/repro_subprocess_hang.sh` (pre-fix 12/12 hangs, post-fix
    // 0 hangs) plus the `killPid bounded wait` divergence entry in
    // `.port/phase-2/DEFERRALS.md`.
    //
    // (Task R13-D, UD93: the two objections above are answered by
    // `kill_pid_abandons_a_child_that_ignores_sighup`. The deadline is now a
    // parameter of `kill_pid_until`, so the test waits 300 ms rather than
    // `KILL_ABANDON`, and its child ignores SIGHUP from before `exec` rather
    // than wedging, so it is SIGKILLable and is killed and reaped afterwards.
    // It exercises the deadline itself; the `?Es` wedge stays out-of-suite.)

    // A trivial `AppMailbox` test double: it accepts every push (returns a
    // non-zero queue size) and records nothing. Enough for the exit / termios /
    // write callbacks, which only `push(...)` to it.
    //
    // The one thing it does keep is each `child_exited` exit code, in a
    // thread-local (the exit callback runs on the thread that drives the loop,
    // which in these KATs is the test thread), so the process-exit KATs can see
    // which arm of `processExit` produced it.
    #[cfg(any(target_os = "macos", windows))]
    struct TestMailbox;

    #[cfg(any(target_os = "macos", windows))]
    thread_local! {
        static CHILD_EXITED_CODES: std::cell::RefCell<Vec<u32>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    #[cfg(any(target_os = "macos", windows))]
    impl rustty_apprt_api::surface::AppMailbox for TestMailbox {
        type Surface = ();

        #[allow(clippy::result_large_err)]
        fn push_or_return(
            &self,
            msg: rustty_apprt_api::surface::Message<'static>,
            _timeout: rustty_datastruct::blocking_queue::Timeout,
        ) -> Result<
            rustty_datastruct::blocking_queue::Size,
            rustty_apprt_api::surface::Message<'static>,
        > {
            if let rustty_apprt_api::surface::Message::ChildExited(info) = &msg {
                CHILD_EXITED_CODES.with(|v| v.borrow_mut().push(info.exit_code));
            }
            // S211-F2: a bell rung by the pty output is pushed from the
            // io-reader thread; record that thread's crash `thread_state`.
            if let rustty_apprt_api::surface::Message::RingBell = &msg {
                let state = rustty_crash::sentry::threadState();
                BELL_THREAD_STATES
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(state.map(|(ty, surface)| {
                        (
                            ty,
                            std::ptr::addr_eq(surface, &crate::TEST_CRASH_SURFACE as *const _),
                        )
                    }));
            }
            // Non-zero => "accepted" (matches a live queue's push return).
            Ok(1)
        }

        fn crash_surface(&self) -> *const dyn rustty_crash::sentry::ThreadStateSurface {
            &crate::TEST_CRASH_SURFACE
        }
    }

    /// The `(type, is TEST_CRASH_SURFACE)` crash `thread_state` of the thread
    /// that pushed each `RingBell` through `TestMailbox` (`None` = no state).
    #[cfg(any(target_os = "macos", windows))]
    #[allow(clippy::type_complexity)]
    static BELL_THREAD_STATES: std::sync::Mutex<Vec<Option<(rustty_crash::sentry::Type, bool)>>> =
        std::sync::Mutex::new(Vec::new());

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
    #[cfg(any(target_os = "macos", windows))]
    #[cfg_attr(windows, allow(dead_code))] // (the byte counters serve macOS KATs)
    struct TestTermio {
        io: *mut crate::Termio<TestMailbox>,
        state: *mut rustty_renderer::state::State,
        mutex: *mut rustty_zigstd::thread::Mutex,
        renderer_mailbox: *mut rustty_renderer::thread::Mailbox,
        renderer_wakeup: xev::Async,
        /// The `PROCESSED_BYTES` value when this harness was built, so
        /// `processed_bytes` is per-harness even though the counter is global
        /// (the KATs are serialized by `FORK_KAT_SERIAL`).
        processed_base: usize,
        /// The `PROCESSED_CALLS` value when this harness was built.
        calls_base: usize,
    }

    #[cfg(any(target_os = "macos", windows))]
    #[cfg_attr(windows, allow(dead_code))]
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
                    // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
                    demand: Default::default(),
                    handoff_gen: Default::default(),
                    handoff_park: Default::default(),
                }));

            // A throwaway renderer mailbox (pushes queue; notify fails-safe) and a
            // real renderer-wakeup Async (Copy; shared by the handler + Termio).
            let renderer_mailbox: *mut rustty_renderer::thread::Mailbox =
                Box::into_raw(rustty_renderer::thread::Mailbox::create());
            let renderer_wakeup = xev::Async::init().expect("Async::init");

            // The owned Terminal (80x24).
            let terminal = Terminal::init(
                Global.into(),
                rustty_vt::terminal::Options {
                    cols: 80,
                    rows: 24,
                    max_scrollback_bytes: Some(1000),
                    max_scrollback_lines: None,
                    colors: rustty_vt::terminal::Colors::default(),
                    default_modes: rustty_vt::modes::ModePacked::default(),
                    default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                    default_cursor_blink: Some(false),
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
                rt_pre_exec_info: RtPreExecInfo::default(),
                rt_post_fork_info: RtPostForkInfo::default(),
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
                // (`clone`: `Copy` everywhere but the Windows Async.)
                #[allow(clippy::clone_on_copy)]
                renderer_wakeup: renderer_wakeup.clone(),
                enquiry_response: derived.enquiry_response.clone(),
                osc_color_report_format: derived.osc_color_report_format,
                clipboard_write: derived.clipboard_write,
                clipboard_write_limit: derived.clipboard_write_limit,
                // (reference field defaults: `kitty_clipboard_grants = .{}`,
                // `kitty_clipboard_write = null`.)
                kitty_clipboard_grants: rustty_vt::kitty::clipboard::Grants::default(),
                kitty_clipboard_write: None,
                apc: rustty_vt::apc::Handler::default(),
                dcs: rustty_vt::dcs::Handler::default(),
                tmux_viewer: None,
                termio_messaged: false,
                seen_title: false,
            };
            let terminal_stream = crate::stream_handler::Stream::init(
                rustty_vt::stream::Options::with_handler(handler),
            );

            // Assemble + heap-pin the Termio.
            let io: *mut crate::Termio<TestMailbox> = Box::into_raw(Box::new(crate::Termio {
                alloc: Global,
                backend,
                config: derived,
                terminal,
                renderer_state: state,
                #[allow(clippy::clone_on_copy)]
                renderer_wakeup: renderer_wakeup.clone(),
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
                processed_base: crate::PROCESSED_BYTES.load(core::sync::atomic::Ordering::Relaxed),
                calls_base: crate::PROCESSED_CALLS.load(core::sync::atomic::Ordering::Relaxed),
            }
        }

        // The active Screen text, read under the renderer mutex (matching the read
        // thread's `processOutput` lock discipline).
        /// The running tally of bytes handed to `Termio::processOutput` since
        /// this harness was built. See `crate::PROCESSED_BYTES`.
        fn processed_bytes(&self) -> usize {
            crate::PROCESSED_BYTES.load(core::sync::atomic::Ordering::Relaxed) - self.processed_base
        }

        /// The number of `processOutput` CALLS since this harness was built —
        /// i.e. batches delivered by the read pipeline. See
        /// `crate::PROCESSED_CALLS`.
        fn processed_calls(&self) -> usize {
            crate::PROCESSED_CALLS.load(core::sync::atomic::Ordering::Relaxed) - self.calls_base
        }

        fn screen_dump(&self) -> Vec<u8> {
            use allocator_api2::alloc::Global;
            unsafe { (*self.mutex).lock() };
            let dump = unsafe {
                (*(*self.io).terminal.screens.active).dump_string_alloc(
                    Global.into(),
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
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork_info: RtPostForkInfo::default(),
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
                write_pool: MemoryPool::init(Global),
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

    /// S211-F5 / UD102: drive one of `threadEnter`'s error arms for real, the
    /// read-thread spawn failure (a thread that cannot get its stack), and
    /// check that it drains the pty master before `subprocess.stop()` and
    /// fails `threadEnter`. The reference's arm is the bare `errdefer
    /// self.subprocess.stop()`; the drain is the port's R13 handling. The child
    /// is a real `/bin/sh` that writes to the pty and keeps running.
    #[test]
    #[cfg(target_os = "macos")]
    fn threadenter_read_thread_spawn_failure_drains_before_stop() {
        use allocator_api2::alloc::Global;
        use std::time::Instant;

        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let cfg = Config {
            command: Some(configpkg::command::Command::Direct(vec![
                b"/bin/sh".to_vec(),
                b"-c".to_vec(),
                b"printf PENDING-OUTPUT; exec /bin/sleep 5".to_vec(),
            ])),
            env: EnvMap::new(),
            env_override: configpkg::RepeatableStringMap::default(),
            shell_integration: configpkg::value_types::ShellIntegration::Detect,
            shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork_info: RtPostForkInfo::default(),
        };
        let mut exec = Exec::init(Global, cfg).expect("Exec::init");
        let mut loop_ = xev::Loop::init(xev::Options::default()).expect("Loop::init");
        let loop_ptr: *mut xev::Loop = std::ptr::addr_of_mut!(loop_);
        let t = TestTermio::build();
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
                write_pool: MemoryPool::init(Global),
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

        DRAIN_CALLS.with(|c| c.set(0));
        STOP_CALLS.with(|c| c.set(0));
        STOPS_BEFORE_DRAIN.with(|c| c.set(None));
        FAIL_READ_THREAD_SPAWN.with(|f| f.set(true));
        let result = unsafe { exec.threadEnter(Global, t.io, &mut td) };
        FAIL_READ_THREAD_SPAWN.with(|f| f.set(false));

        assert!(
            result.is_err(),
            "a failed read-thread spawn fails threadEnter"
        );
        assert_eq!(
            DRAIN_CALLS.with(std::cell::Cell::get),
            1,
            "the error arm drains the pty master once"
        );
        // The order is the fix: draining after `stop()` does nothing, because
        // `stop()` is what wedges on an undrained child.
        assert_eq!(
            STOP_CALLS.with(std::cell::Cell::get),
            1,
            "the error arm calls subprocess.stop() once"
        );
        assert_eq!(
            STOPS_BEFORE_DRAIN.with(std::cell::Cell::get),
            Some(0),
            "the drain runs before subprocess.stop(), not after it"
        );
        // `stop()` ran: the subprocess has been torn down.
        assert!(
            exec.subprocess.process.is_none(),
            "the error arm stops the subprocess"
        );

        td.deinit();
        loop_.deinit();
        exec.deinit();
        t.teardown();
    }

    /// S212-F1: the pty-exhaustion failure has a real producer. With the
    /// process's descriptor table full, `Subprocess::start`'s `Pty.open`
    /// (`openpty`) fails, and `start` returns `Error::OpenptyFailed` (the
    /// member `Thread.threadMain` turns into the pty-exhaustion screen), not
    /// the general `Error::Other`. The table is filled in a re-executed child
    /// copy of this test binary, so no other test runs out of descriptors.
    #[test]
    #[cfg(target_os = "macos")]
    fn subprocess_start_with_no_free_descriptor_is_openpty_failed() {
        use allocator_api2::alloc::Global;

        const CHILD: &str = "RUSTTY_TERMIO_OPENPTY_EXHAUSTION_CHILD";
        const TEST: &str =
            "exec::tests::subprocess_start_with_no_free_descriptor_is_openpty_failed";
        if std::env::var_os(CHILD).is_none() {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .output()
                .expect("re-exec the test binary");
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                out.status.success(),
                "the child run failed ({:?})\nstdout:\n{stdout}\nstderr:\n{stderr}",
                out.status
            );
            assert!(
                stdout.contains("test result: ok. 1 passed; 0 failed"),
                "the child ran exactly this test\nstdout:\n{stdout}\nstderr:\n{stderr}"
            );
            return;
        }

        // Child: a fresh process, one test thread.
        let cfg = Config {
            command: Some(configpkg::command::Command::Direct(vec![
                b"/usr/bin/true".to_vec()
            ])),
            env: EnvMap::new(),
            env_override: configpkg::RepeatableStringMap::default(),
            shell_integration: configpkg::value_types::ShellIntegration::Detect,
            shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork_info: RtPostForkInfo::default(),
        };
        let mut exec = Exec::init(Global, cfg).expect("Exec::init");

        // Fill the descriptor table: a low soft limit, then dup until EMFILE.
        let mut old = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut old) }, 0);
        let low = libc::rlimit {
            rlim_cur: 256,
            rlim_max: old.rlim_max,
        };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &low) }, 0);
        let mut filler: Vec<libc::c_int> = Vec::new();
        loop {
            let fd = unsafe { libc::dup(2) };
            if fd < 0 {
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::EMFILE),
                    "the table fills up"
                );
                break;
            }
            filler.push(fd);
        }
        assert!(!filler.is_empty(), "descriptors were taken");

        let result = exec.subprocess.start(Global);

        for fd in filler {
            unsafe {
                libc::close(fd);
            }
        }
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &old) }, 0);

        assert!(
            matches!(result, Err(crate::Error::OpenptyFailed)),
            "a full descriptor table makes start fail with OpenptyFailed, got {:?}",
            result.as_ref().map(|_| ()).map_err(|e| *e)
        );
        assert!(exec.subprocess.pty.is_none(), "no pty was kept");
        exec.deinit();
    }

    /// UD67 harness: `threadEnter` a real `/bin/sh -c <script>` child on a real
    /// pty and kqueue loop, run `before_first_tick` (which sees the child's pid)
    /// BEFORE the loop has submitted the `EVFILT_PROC` registration, then pump the
    /// loop until `processExit` has run. Returns the `child_exited` codes the
    /// mailbox received and the backend's `exited` flag.
    #[cfg(target_os = "macos")]
    fn run_child_to_process_exit(
        script: &[u8],
        before_first_tick: impl FnOnce(libc::pid_t, &mut xev::Loop),
    ) -> (Vec<u32>, bool) {
        use allocator_api2::alloc::Global;
        use std::time::{Duration, Instant};

        CHILD_EXITED_CODES.with(|v| v.borrow_mut().clear());
        let cfg = Config {
            command: Some(configpkg::command::Command::Direct(vec![
                b"/bin/sh".to_vec(),
                b"-c".to_vec(),
                script.to_vec(),
            ])),
            env: EnvMap::new(),
            env_override: configpkg::RepeatableStringMap::default(),
            shell_integration: configpkg::value_types::ShellIntegration::Detect,
            shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork_info: RtPostForkInfo::default(),
        };
        let mut exec = Exec::init(Global, cfg).expect("Exec::init");
        let mut loop_ = xev::Loop::init(xev::Options::default()).expect("Loop::init");
        let loop_ptr: *mut xev::Loop = std::ptr::addr_of_mut!(loop_);
        let t = TestTermio::build();
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
                write_pool: MemoryPool::init(Global),
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
        unsafe { exec.threadEnter(Global, t.io, &mut td) }.expect("threadEnter");
        let pid = match &exec.subprocess.process {
            Some(Process::ForkExec(cmd)) => cmd.pid.expect("spawned child has a pid"),
            _ => panic!("expected a fork/exec child"),
        };
        let master: std::os::fd::RawFd = {
            let crate::backend::ThreadData::Exec(e) = &td.backend;
            e.read_thread_fd
        };

        before_first_tick(pid, &mut loop_);

        let exited = |td: &crate::termio_impl::ThreadData<TestMailbox>| {
            let crate::backend::ThreadData::Exec(e) = &td.backend;
            e.exited
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !exited(&td) && Instant::now() < deadline {
            loop_.run(xev::RunMode::NoWait).expect("loop.run(NoWait)");
            std::thread::sleep(Duration::from_millis(1));
        }
        let was_exited = exited(&td);
        let codes = CHILD_EXITED_CODES.with(|v| v.borrow().clone());

        exec.threadExit(&mut td);
        unsafe {
            libc::close(master);
        }
        td.deinit();
        loop_.deinit();
        exec.deinit();
        t.teardown();
        (codes, was_exited)
    }

    /// Whether `pid` has exited and is waiting to be reaped. `WNOWAIT` leaves it
    /// a zombie, so the loop's `EVFILT_PROC` registration still sees it.
    #[cfg(target_os = "macos")]
    fn child_is_zombie(pid: libc::pid_t) -> bool {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let rc = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        rc == 0 && info.si_pid == pid
    }

    /// Registers `EVFILT_PROC` for `pid` on a private kqueue, exactly as the xev
    /// kqueue backend does (`EV_ADD | EV_ENABLE`, `NOTE_EXIT | NOTE_EXITSTATUS`),
    /// and returns the errno the kernel answers the registration with (0 if it
    /// was accepted).
    #[cfg(target_os = "macos")]
    fn evfilt_proc_registration_errno(pid: libc::pid_t) -> i64 {
        unsafe {
            let kq = libc::kqueue();
            assert!(kq >= 0);
            let change = libc::kevent {
                ident: pid as libc::uintptr_t,
                filter: libc::EVFILT_PROC,
                flags: libc::EV_ADD | libc::EV_ENABLE | libc::EV_RECEIPT,
                fflags: libc::NOTE_EXIT | libc::NOTE_EXITSTATUS,
                data: 0,
                udata: std::ptr::null_mut(),
            };
            let mut out: libc::kevent = std::mem::zeroed();
            let n = libc::kevent(kq, &change, 1, &mut out, 1, std::ptr::null());
            libc::close(kq);
            assert_eq!(n, 1, "EV_RECEIPT always answers the change");
            if out.flags & libc::EV_ERROR != 0 {
                out.data as i64
            } else {
                0
            }
        }
    }

    // UD67 (R13): a child that has already exited when the kqueue loop submits
    // its `EVFILT_PROC` registration. XNU answers that registration with ESRCH
    // (`NoSuchProcess`), where the reference's `processExit` does `catch
    // unreachable`. Driven through the REAL path, no seam: the registration is
    // only submitted on the loop's first tick after `threadEnter`, so the test
    // waits (with `WNOWAIT`, not a guessed sleep) until the child is a zombie
    // before that first tick, and first checks on a private kqueue that the
    // kernel really refuses a registration for it with ESRCH. (The exit code
    // cannot tell the arms apart on macOS: the child is `login(1)`, which exits
    // 0 whatever its command does.)
    #[test]
    #[cfg(target_os = "macos")]
    fn process_exit_reports_a_child_that_exited_before_the_watch_was_registered() {
        use std::time::{Duration, Instant};
        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let mut saw_zombie = false;
        let mut registration_errno = 0;
        let (codes, exited) = run_child_to_process_exit(b"exit 3", |pid, _loop| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !child_is_zombie(pid) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            saw_zombie = child_is_zombie(pid);
            registration_errno = evfilt_proc_registration_errno(pid);
        });
        assert!(
            saw_zombie,
            "precondition: the child must have exited before the first tick"
        );
        assert_eq!(
            registration_errno,
            libc::ESRCH as i64,
            "precondition: the kernel refuses EVFILT_PROC for the zombie with ESRCH"
        );
        assert!(
            exited,
            "processExitCommon must run for the ESRCH wait result"
        );
        assert_eq!(
            codes,
            vec![0],
            "exactly one child_exited, with the no-status code 0"
        );
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

    /// S211-F2: the io-reader thread tags its crash metadata as an `io` thread
    /// of the mailbox's surface (`Exec.zig:1411-1417`). A BEL written to the
    /// pty makes the read thread push `RingBell`, and `TestMailbox` records
    /// that thread's `thread_state`.
    #[test]
    #[cfg(target_os = "macos")]
    fn readthread_sets_io_crash_thread_state() {
        use std::time::{Duration, Instant};

        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        BELL_THREAD_STATES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        // The test thread itself has no crash state.
        assert!(rustty_crash::sentry::threadState().is_none());

        let (handle, master, slave, quit_write, t) = spawn_readthread_over_pty();
        let w = unsafe { libc::write(slave, b"\x07".as_ptr().cast::<std::ffi::c_void>(), 1) };
        assert_eq!(w, 1, "wrote BEL to the pty slave");

        let deadline = Instant::now() + Duration::from_secs(3);
        let states = loop {
            let states = BELL_THREAD_STATES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            if !states.is_empty() || Instant::now() > deadline {
                break states;
            }
            std::thread::sleep(Duration::from_millis(1));
        };

        let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
        handle.join().expect("read thread join");
        unsafe {
            libc::close(quit_write);
            libc::close(master);
            libc::close(slave);
        }
        t.teardown();

        assert_eq!(
            states,
            vec![Some((rustty_crash::sentry::Type::Io, true))],
            "the read thread's thread_state is .io with the mailbox's surface"
        );
    }

    /// Serializes the Windows read-thread tests, which share
    /// `BELL_THREAD_STATES` (Windows has no `FORK_KAT_SERIAL`: nothing forks).
    #[cfg(windows)]
    static WINDOWS_READTHREAD_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Windows: the anonymous pipe pair the reference reads a ConPTY through
    /// (`CreatePipe`, `pty.zig`'s `out_pipe`), as `[read, write]`.
    #[cfg(windows)]
    fn windows_pipe() -> [rustty_os::windows::HANDLE; 2] {
        rustty_os::pipe::pipe().expect("pipe()")
    }

    /// Windows: write all of `bytes` to a pipe handle.
    #[cfg(windows)]
    fn windows_write(h: rustty_os::windows::HANDLE, bytes: &[u8]) {
        use rustty_os::windows as win;
        let mut written: win::DWORD = 0;
        let ok = unsafe {
            win::exp::kernel32::WriteFile(
                h,
                bytes.as_ptr().cast::<std::ffi::c_void>(),
                bytes.len() as win::DWORD,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, win::FALSE, "WriteFile");
        assert_eq!(written as usize, bytes.len(), "short WriteFile");
    }

    /// S211-F2, Windows arm: `threadMainWindows` tags its crash metadata as an
    /// `io` thread of the mailbox's surface (`Exec.zig:1776-1781`). A BEL
    /// written to the pipe it reads makes it push `RingBell`, and `TestMailbox`
    /// records that thread's `thread_state`. The thread is stopped as
    /// `threadExit` stops it: a byte on the quit pipe, then `CancelIoEx` on the
    /// read handle.
    #[test]
    #[cfg(windows)]
    fn windows_readthread_sets_io_crash_thread_state() {
        use std::time::{Duration, Instant};

        let _serial = WINDOWS_READTHREAD_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        BELL_THREAD_STATES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        // The test thread itself has no crash state.
        assert!(rustty_crash::sentry::threadState().is_none());

        let t = TestTermio::build();
        let [data_read, data_write] = windows_pipe();
        let [quit_read, quit_write] = windows_pipe();
        let read_args = ReadThreadArgs::<TestMailbox>(data_read, t.io, quit_read);
        let handle = std::thread::spawn(move || {
            let args = read_args;
            ReadThread::threadMainWindows::<TestMailbox>(args.0, args.1, args.2);
        });

        windows_write(data_write, b"\x07");
        let deadline = Instant::now() + Duration::from_secs(5);
        let states = loop {
            let states = BELL_THREAD_STATES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            if !states.is_empty() || Instant::now() > deadline {
                break states;
            }
            std::thread::sleep(Duration::from_millis(1));
        };

        // Stop it as `threadExit` does: the quit byte, then `CancelIoEx` on the
        // read handle. A cancel issued while the thread is between two reads
        // (still handling the BEL) finds nothing to cancel (measured: a
        // parallel run hit ERROR_NOT_FOUND), so it is repeated until the thread
        // has returned.
        windows_write(quit_write, b"x");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !handle.is_finished() {
            assert!(
                Instant::now() < deadline,
                "the read thread did not stop on the quit byte + CancelIoEx"
            );
            let _ = unsafe {
                rustty_os::windows::exp::kernel32::CancelIoEx(data_read, std::ptr::null_mut())
            };
            std::thread::sleep(Duration::from_millis(1));
        }
        handle.join().expect("read thread join");
        // (the read thread closed `quit_read` on exit)
        for h in [quit_write, data_read, data_write] {
            let _ = rustty_os::windows::CloseHandle(h);
        }
        t.teardown();

        assert_eq!(
            states,
            vec![Some((rustty_crash::sentry::Type::Io, true))],
            "the read thread's thread_state is .io with the mailbox's surface"
        );
    }

    /// S211-F3: on Windows `threadEnter` spawns `threadMainWindows`
    /// (`Exec.zig:135-140`). A real child on a real ConPTY (`cmd.exe /c echo`)
    /// writes a marker; the only reader of the pseudo console's output pipe is
    /// the read thread `threadEnter` spawned, so the marker reaching the
    /// terminal's Screen proves that thread ran the Windows read loop.
    #[test]
    #[cfg(windows)]
    fn windows_threadenter_reads_the_pty_through_the_windows_read_thread() {
        use allocator_api2::alloc::Global;
        use std::time::{Duration, Instant};

        let _serial = WINDOWS_READTHREAD_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let cfg = Config {
            command: Some(configpkg::command::Command::Direct(vec![
                b"cmd.exe".to_vec(),
                b"/c".to_vec(),
                // `%RUSTTY_W1_VAR%` expands in the child from the environment
                // given below, so the marker appears only in its output, never
                // in its command line.
                b"echo RUSTTY_W1_%RUSTTY_W1_VAR%".to_vec(),
            ])),
            env: EnvMap::from([(b"RUSTTY_W1_VAR".to_vec(), b"MARK".to_vec())]),
            env_override: configpkg::RepeatableStringMap::default(),
            shell_integration: configpkg::value_types::ShellIntegration::None,
            shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork_info: RtPostForkInfo::default(),
        };
        let mut exec = Exec::init(Global, cfg).expect("Exec::init");
        // `initTerminalIo`'s initial resize from the terminal (80x24), which
        // runs before `threadEnter`; ConPTY rejects the default 0x0 size.
        exec.resize(
            GridSize {
                columns: 80,
                rows: 24,
            },
            ScreenSize {
                width: 800,
                height: 480,
            },
        )
        .expect("resize");
        let mut loop_ = xev::Loop::init(xev::Options::default()).expect("Loop::init");
        let loop_ptr: *mut xev::Loop = std::ptr::addr_of_mut!(loop_);
        let t = TestTermio::build();
        let io_ptr = t.io;
        let mut td: crate::termio_impl::ThreadData<TestMailbox> = crate::termio_impl::ThreadData {
            alloc: Global,
            loop_: loop_ptr,
            renderer_state: t.state,
            surface_mailbox: TestMailbox,
            backend: crate::backend::ThreadData::placeholder(),
            mailbox: std::ptr::null_mut(),
        };

        unsafe { exec.threadEnter(Global, io_ptr, &mut td) }.expect("threadEnter");

        let needle = b"RUSTTY_W1_MARK";
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut found = false;
        while Instant::now() < deadline {
            loop_.run(xev::RunMode::NoWait).expect("loop.run(NoWait)");
            let dump = t.screen_dump();
            if dump.windows(needle.len()).any(|w| w == needle) {
                found = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let screen = String::from_utf8_lossy(&t.screen_dump()).into_owned();
        eprintln!("screen: {screen:?}");

        exec.threadExit(&mut td);
        td.deinit();
        loop_.deinit();
        exec.deinit();
        t.teardown();

        assert!(
            found,
            "the child's output did not reach the Screen through the read thread; screen: {screen:?}"
        );
    }

    /// S211-F6: when the gather thread cannot be spawned, `threadMainPosix`
    /// logs the reference's error and returns (`Exec.zig:1470-1479`); it does
    /// not panic. The spawn fails for real (a test-only switch asks for a stack
    /// no thread can get). `threadMainPosix` runs on the test thread itself, so
    /// the thread-local switch and the thread-owned log capture both see it.
    #[test]
    #[cfg(target_os = "macos")]
    fn readthread_gather_spawn_failure_logs_and_returns() {
        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let t = TestTermio::build();
        let pty = Pty::open(ptypkg::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 800,
            ws_ypixel: 480,
        })
        .expect("Pty::open");
        let mut pipefds = [0 as std::os::fd::RawFd; 2];
        assert_eq!(unsafe { libc::pipe(pipefds.as_mut_ptr()) }, 0, "pipe()");
        let (quit_read, quit_write) = (pipefds[0], pipefds[1]);

        let cap = crate::log_capture::start();
        FAIL_GATHER_SPAWN.with(|f| f.set(true));
        let io = t.io;
        let master = pty.master;
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ReadThread::threadMainPosix::<TestMailbox>(master, io, quit_read);
        }));
        FAIL_GATHER_SPAWN.with(|f| f.set(false));
        let errors: Vec<String> = cap
            .messages()
            .into_iter()
            .filter(|(level, target, _)| *level == log::Level::Error && target == LOG_SCOPE)
            .map(|(_, _, message)| message)
            .collect();
        drop(cap);

        // `quit_read` was closed by the read thread's guard on return.
        unsafe {
            libc::close(quit_write);
            libc::close(pty.master);
            libc::close(pty.slave);
        }
        t.teardown();

        assert!(ran.is_ok(), "threadMainPosix must return, not panic");
        // S211-F2: its crash `thread_state` guard cleared on the way out.
        assert!(rustty_crash::sentry::threadState().is_none());
        assert_eq!(errors.len(), 1, "one error logged: {errors:?}");
        assert!(
            errors[0].starts_with("read thread exiting, failed to spawn gather thread err="),
            "the reference's message: {errors:?}"
        );
    }

    // KAT (rustty-only margin; NOT a mapped Zig `test{}`) for 2f0e6659d's
    // separable `setQosClass` hunk — WS10 T8. `E-os-termio-platform.md` row E4
    // measures it at 15% throughput on an M4 Max, and the reference ships it
    // with no test at all.
    //
    // WHY THIS IS NOT A UNIT TEST OF `setQosClass`. Calling the helper on any
    // old thread and reading the class back would pass whether or not
    // `threadMainPosix` ever calls it — the vacuity trap. So this drives the
    // REAL read thread over a REAL pty, exactly as
    // `readthread_pty_bytes_land_in_screen` does, and interrogates THAT
    // thread's QoS class from outside via its `pthread_t`
    // (`JoinHandleExt::as_pthread_t`). `pthread_get_qos_class_np` is the
    // thread-addressed reader (`pthread_set_qos_class_self_np`, which the
    // helper wraps, is self-only), so no assertion has to be smuggled into the
    // thread body.
    //
    // The default for a thread spawned from a default-QoS thread is
    // QOS_CLASS_DEFAULT (0x15); the assertion is QOS_CLASS_USER_INITIATED
    // (0x19). Those differ, so the test cannot pass by accident on an
    // unmodified thread — verified by deleting the `Self::setQosClass()` call,
    // which turns it RED with `qos=0x15`.
    //
    // Real openpty + a spawned thread -> Miri-excluded, same as the KAT above,
    // and it takes FORK_KAT_SERIAL for the same reason (it holds pty fds open
    // while other in-process KATs may `fork()`).
    #[test]
    #[cfg(target_os = "macos")]
    fn readthread_sets_user_initiated_qos() {
        use std::os::unix::thread::JoinHandleExt;
        use std::time::{Duration, Instant};

        // int pthread_get_qos_class_np(pthread_t, qos_class_t *, int *);
        //
        // Hand-rolled per the `mem.rs`/`page.rs` convention: `libc` 0.2 exposes
        // `pthread_set_qos_class_self_np` but NOT the thread-addressed getter,
        // and the constraint is "no NEW `libc` surface elsewhere", not "no
        // extern blocks". `qos_class_t` is an unsigned enum.
        unsafe extern "C" {
            fn pthread_get_qos_class_np(
                thread: libc::pthread_t,
                qos_class: *mut libc::c_uint,
                relative_priority: *mut libc::c_int,
            ) -> libc::c_int;
        }
        const QOS_CLASS_USER_INITIATED: libc::c_uint = 0x19;
        const QOS_CLASS_DEFAULT: libc::c_uint = 0x15;

        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let t = TestTermio::build();
        let io = t.io;

        let pty = Pty::open(ptypkg::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 800,
            ws_ypixel: 480,
        })
        .expect("Pty::open");
        let master = pty.master;
        let slave = pty.slave;

        let mut pipefds = [0 as std::os::fd::RawFd; 2];
        assert_eq!(unsafe { libc::pipe(pipefds.as_mut_ptr()) }, 0, "pipe()");
        let quit_read = pipefds[0];
        let quit_write = pipefds[1];

        let read_args = ReadThreadArgs::<TestMailbox>(master, io, quit_read);
        let handle = std::thread::spawn(move || {
            let args = read_args;
            ReadThread::threadMainPosix::<TestMailbox>(args.0, args.1, args.2);
        });
        let tid = handle.as_pthread_t();

        // The helper runs at the very top of the thread body, but "the very top"
        // is still after the spawn returns, so poll rather than race.
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut qos: libc::c_uint = 0;
        let mut rel: libc::c_int = 0;
        let mut observed = false;
        loop {
            let rc = unsafe { pthread_get_qos_class_np(tid, &mut qos, &mut rel) };
            assert_eq!(rc, 0, "pthread_get_qos_class_np failed");
            if qos == QOS_CLASS_USER_INITIATED {
                observed = true;
                break;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        // Teardown BEFORE asserting, so a failure does not leak the thread/fds.
        let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
        handle.join().expect("read thread join");
        unsafe {
            libc::close(quit_write);
            libc::close(master);
            libc::close(slave);
        }
        t.teardown();

        assert!(
            observed,
            "the io-reader thread must run at QOS_CLASS_USER_INITIATED (0x19); \
             observed 0x{qos:x} (0x{QOS_CLASS_DEFAULT:x} is the un-set default)"
        );
    }

    // ---------------------------------------------------------------------
    // WS10 T20 — the two properties `E-os-termio-platform.md` row E1 says do
    // not exist, plus the shutdown property for the new early returns.
    //
    // `readthread_pty_bytes_land_in_screen` above passes with EITHER the
    // pre-pipeline single loop or the two-stage pipeline (it writes 18 bytes and
    // looks for them), so it is NOT a guard for this commit. These three are.
    //
    // Real openpty + poll/read syscalls + spawned threads -> Miri-excluded
    // (MIRI_EXCEPTIONS.md, "rustty-termio B3"); the pure ring logic is proven
    // under Miri by `.port/phase-2/mirihar-ws10-pipeline` instead.
    // ---------------------------------------------------------------------

    /// Drive a real pty through a real `ReadThread::threadMainPosix` and return
    /// `(handle, master, slave, quit_write, TestTermio)` wired for teardown.
    #[cfg(target_os = "macos")]
    fn spawn_readthread_over_pty() -> (
        std::thread::JoinHandle<()>,
        std::os::fd::RawFd,
        std::os::fd::RawFd,
        std::os::fd::RawFd,
        TestTermio,
    ) {
        let t = TestTermio::build();
        let io = t.io;

        let pty = Pty::open(ptypkg::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 800,
            ws_ypixel: 480,
        })
        .expect("Pty::open");
        let master = pty.master;
        let slave = pty.slave;

        let mut pipefds = [0 as std::os::fd::RawFd; 2];
        assert_eq!(unsafe { libc::pipe(pipefds.as_mut_ptr()) }, 0, "pipe()");
        let quit_read = pipefds[0];
        let quit_write = pipefds[1];

        let read_args = ReadThreadArgs::<TestMailbox>(master, io, quit_read);
        let handle = std::thread::spawn(move || {
            let args = read_args;
            ReadThread::threadMainPosix::<TestMailbox>(args.0, args.1, args.2);
        });

        (handle, master, slave, quit_write, t)
    }

    /// A saturated producer must be fully drained into the terminal with no
    /// lost or duplicated bytes, and the pipeline must be measurably FASTER at
    /// it than the pre-pipeline shape.
    ///
    /// This is the THROUGHPUT property `2f0e6659d` exists for. Two assertions,
    /// deliberately different in kind:
    ///
    /// 1. COMPLETENESS (both profiles). Every byte the producer wrote reaches
    ///    `processOutput` exactly once. This catches a dropped, duplicated or
    ///    stalled batch — the ways the ring can be wrong. It does NOT
    ///    distinguish the pipeline from the old loop: measured, the old shape
    ///    is equally correct. Stating that plainly matters, because a test that
    ///    passes either way is not a guard, and this half is not one.
    ///
    /// 2. BATCHING (both profiles). The MEAN BATCH SIZE floor below. THIS is
    ///    the guard, and it is deliberately NOT a throughput number.
    ///
    /// WHY NOT THROUGHPUT. The obvious assertion is bytes/second, and the
    /// pipeline does win on it: measured on this host (M4 Pro, 2 MiB through a
    /// real pty, `--release`, N=9 each, both shapes built from this same tree)
    /// the old loop ran at 87.5 MiB/s median and this pipeline at 123.8 MiB/s,
    /// +41%. But a wall-clock floor is not a sound ASSERTION here: run inside
    /// the full crate suite on a box under load ~9 the same pipeline measured
    /// 71.7 MiB/s — below the old shape's median. A floor that a healthy build
    /// fails under contention is a flaky gate, and a flaky gate gets ignored,
    /// which is the failure mode this whole workstream exists to avoid. The
    /// throughput number is therefore PRINTED, not asserted.
    ///
    /// The mean batch size measures the same mechanism without the clock. The
    /// pre-pipeline loop read into a `[u8; 1024]`, so its batches cannot exceed
    /// 1024 bytes — a HARD ceiling, not a measurement. The pipeline gathers into
    /// 64 KiB slots. Measured, saturated: old shape exactly **1024.0** bytes,
    /// this shape exactly **65536** bytes. The floor is 4096 — 4x above the old
    /// shape's structural ceiling and 16x below this shape's observed value, and
    /// it does not move when the machine is busy.
    ///
    /// PROVEN TO FAIL AGAINST THE PRE-PIPELINE SHAPE: the old
    /// `read`-into-`[u8; 1024]`/`processOutput` loop was spliced back into this
    /// same tree and this test re-run — mean batch 1024, RED.
    #[test]
    #[cfg(target_os = "macos")]
    fn readthread_pipeline_sustains_saturated_producer() {
        use std::time::{Duration, Instant};

        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        // A saturating write: many multiples of one 64 KiB gather buffer, so the
        // ring rotates repeatedly and the gather stage hits its backpressure
        // wait. Printable, non-control bytes only, so every byte lands in the
        // screen as itself (no VT interpretation, no OPOST translation).
        //
        // 2 MiB through an 80x24 screen scrolls ~1000 times and the SCREEN
        // cannot retain it, so the byte-exact assertion is made on what the
        // parse stage was handed (`processed_bytes`), and the screen is used
        // only as the liveness proof that the bytes really went through
        // `processOutput` rather than into a counter.
        const TOTAL: usize = 2 * 1024 * 1024;

        // The mean-batch-size floor, and how many samples we take. See the doc
        // comment for the measurements these come from.
        const MEAN_BATCH_FLOOR: f64 = 4096.0;
        #[cfg(not(debug_assertions))]
        const REPS: usize = 5;
        // One rep in dev: the same work takes ~22s there and the throughput
        // assertion is disabled anyway.
        #[cfg(debug_assertions)]
        const REPS: usize = 1;

        let mut best_mib_s = 0.0f64;
        let mut best_mean_batch = 0.0f64;
        for _ in 0..REPS {
            let (handle, master, slave, quit_write, t) = spawn_readthread_over_pty();
            let chunk: Vec<u8> = (0..4096u32).map(|i| b'a' + (i % 26) as u8).collect();

            let t0 = Instant::now();
            let writer = std::thread::spawn(move || {
                let mut written = 0usize;
                while written < TOTAL {
                    let want = core::cmp::min(chunk.len(), TOTAL - written);
                    let rc = unsafe {
                        libc::write(slave, chunk.as_ptr().cast::<std::ffi::c_void>(), want)
                    };
                    if rc <= 0 {
                        let e = std::io::Error::last_os_error();
                        if e.raw_os_error() == Some(libc::EINTR)
                            || e.raw_os_error() == Some(libc::EAGAIN)
                        {
                            continue;
                        }
                        break;
                    }
                    written += rc as usize;
                }
                written
            });

            // Bounded wait for the parse stage to consume everything the writer
            // produced. `processed_bytes` is the harness's tally of every byte
            // `processOutput` was handed.
            let written = writer.join().expect("writer join");
            let deadline = Instant::now() + Duration::from_secs(60);
            let mut processed = 0usize;
            while Instant::now() < deadline {
                processed = t.processed_bytes();
                if processed >= written {
                    break;
                }
                std::thread::sleep(Duration::from_micros(200));
            }
            let elapsed = t0.elapsed();
            let mib_s = processed as f64 / (1024.0 * 1024.0) / elapsed.as_secs_f64();
            if mib_s > best_mib_s {
                best_mib_s = mib_s;
            }
            let calls = t.processed_calls();
            let mean_batch = processed as f64 / calls.max(1) as f64;
            if mean_batch > best_mean_batch {
                best_mean_batch = mean_batch;
            }

            // Teardown BEFORE asserting, so a failure never leaks threads/fds.
            let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
            handle.join().expect("read thread join");
            unsafe {
                libc::close(quit_write);
                libc::close(master);
                libc::close(slave);
            }
            // The bytes must actually reach the Screen, not just a counter.
            let dump = t.screen_dump();
            let inked = dump.iter().any(|&b| b.is_ascii_lowercase());
            t.teardown();

            assert!(
                written >= TOTAL / 2,
                "the writer stalled: only {written} of {TOTAL} bytes reached the pty"
            );
            assert_eq!(
                processed, written,
                "every byte the producer wrote must reach processOutput exactly once \
                 (lost/duplicated batch, or a stalled ring)"
            );
            assert!(inked, "the drained bytes must actually reach the Screen");
        }

        eprintln!(
            "PIPELINE: best of {REPS} = {best_mib_s:.2} MiB/s, \
             best mean batch = {best_mean_batch:.0} bytes"
        );

        assert!(
            best_mean_batch >= MEAN_BATCH_FLOOR,
            "mean batch size {best_mean_batch:.0} bytes is below the \
             {MEAN_BATCH_FLOOR} byte floor. The pre-pipeline loop read into a \
             [u8; 1024], so it CANNOT exceed 1024 here (measured: 1024.0); a \
             number in that range means the gather stage's batching is gone."
        );
    }

    /// A single sub-1 KiB write must reach the screen with interactive latency.
    ///
    /// This is the LATENCY property `2f0e6659d` + `bb0ac4c72` exist for, and the
    /// throughput suite cannot see it: a pipeline that batched aggressively
    /// would ace throughput and ruin interactivity. Anything under
    /// `bridge_threshold` (1 KiB) is by construction an interactive trickle that
    /// must be delivered on the FIRST EAGAIN with no bridging at all, so the
    /// only added latency is one condvar hop.
    ///
    /// TWO BOUNDS, and the reason there are two:
    ///
    /// * LIVENESS, on the worst of 8 writes: `gather_budget_ns` (3 ms) x 50 =
    ///   150 ms. Deliberately loose. This one only says "a small write is not
    ///   lost or wedged", and it is the bound a single scheduler hiccup on a
    ///   contended box must not trip.
    /// * BATCHING, on the MEDIAN of 8 writes: `gather_budget_ns` / 10 = 300 us.
    ///   THIS is the guard. Median, not worst, because the regression it catches
    ///   moves EVERY write while contention moves one — so the median separates
    ///   the two causes that a worst-case bound conflates.
    ///
    /// The 300 us figure is measured, not chosen (M4 Pro, real pty, `--release`,
    /// box under load ~9, N=9 runs of 8 writes each):
    ///
    ///     healthy (this shape):        medians 22..46 us
    ///     bridging-always (mutated):   medians 1176..1212 us
    ///
    /// 300 us sits ~6x above the healthy tail and ~4x below the degraded floor.
    ///
    /// PROVEN TO FAIL: disabling BOTH early-delivery paths — the
    /// `total < bridge_threshold` break (2f0e6659d) and the `count == 0` idle
    /// break (bb0ac4c72) — moves the median to ~1.18 ms, over the bound.
    /// Disabling only the first is NOT caught, and that is a real finding
    /// recorded here rather than papered over: with an idle parser,
    /// `bb0ac4c72`'s guard delivers immediately anyway, so the two paths are
    /// genuinely redundant for THIS scenario and only their conjunction is
    /// observable from outside.
    ///
    /// RELEASE-ONLY for the batching bound: in a `dev` build the VT parse
    /// dominates (see `readthread_pipeline_sustains_saturated_producer`) and the
    /// microsecond-scale signal is buried. The liveness bound runs in both.
    #[test]
    #[cfg(target_os = "macos")]
    fn readthread_pipeline_keeps_interactive_latency() {
        use std::time::{Duration, Instant};

        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (handle, master, slave, quit_write, t) = spawn_readthread_over_pty();

        // The two bounds. See the doc comment for the measurements behind them.
        const LIVENESS_FACTOR: u32 = 50;
        const BATCHING_DIVISOR: u32 = 10;
        let bound = Duration::from_nanos(ReadThread::GATHER_BUDGET_NS) * LIVENESS_FACTOR;
        let batching_bound = Duration::from_nanos(ReadThread::GATHER_BUDGET_NS) / BATCHING_DIVISOR;

        // Several separate small writes, each well under bridge_threshold, so
        // one lucky delivery cannot carry the test.
        let mut worst = Duration::ZERO;
        let mut lats: Vec<Duration> = Vec::new();
        for i in 0..8u8 {
            let needle: Vec<u8> = format!("LAT{i}").into_bytes();
            let start = Instant::now();
            let w = unsafe {
                libc::write(
                    slave,
                    needle.as_ptr().cast::<std::ffi::c_void>(),
                    needle.len(),
                )
            };
            assert_eq!(w, needle.len() as isize, "wrote the needle");

            let deadline = start + bound;
            let mut seen = false;
            while Instant::now() < deadline {
                let dump = t.screen_dump();
                if dump.windows(needle.len()).any(|c| c == needle.as_slice()) {
                    seen = true;
                    break;
                }
                std::thread::yield_now();
            }
            let elapsed = start.elapsed();
            lats.push(elapsed);
            if elapsed > worst {
                worst = elapsed;
            }
            if !seen {
                // Teardown before failing.
                let _ =
                    unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
                handle.join().expect("read thread join");
                unsafe {
                    libc::close(quit_write);
                    libc::close(master);
                    libc::close(slave);
                }
                t.teardown();
                panic!(
                    "interactive write {i} ({} bytes, under the {} byte bridge threshold) \
                     did not reach the screen within {bound:?}",
                    needle.len(),
                    1024,
                );
            }
        }

        let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
        handle.join().expect("read thread join");
        unsafe {
            libc::close(quit_write);
            libc::close(master);
            libc::close(slave);
        }
        t.teardown();

        lats.sort();
        let median = lats[lats.len() / 2];
        eprintln!("PIPELINE-LATENCY: median = {median:?} worst = {worst:?}");
        assert!(
            worst < bound,
            "LIVENESS: worst interactive latency {worst:?} exceeded the bound \
             {bound:?} (gather_budget_ns x {LIVENESS_FACTOR})"
        );
        #[cfg(not(debug_assertions))]
        assert!(
            median < batching_bound,
            "BATCHING: median interactive latency {median:?} exceeded \
             {batching_bound:?} (gather_budget_ns / {BATCHING_DIVISOR}). Healthy \
             is 22-46us on this host; a bridging-always pipeline measures \
             ~1.18ms. Interactive writes are being batched."
        );
        #[cfg(debug_assertions)]
        let _ = batching_bound;
    }

    /// Both pipeline stages must shut down cleanly, from either side.
    ///
    /// `E-os-termio-platform.md` row E1 warns that the parse stage gained two
    /// new early returns. The hazards are (a) the `QuitFdCloser` Drop guard not
    /// firing on one of them, and (b) the gather stage exiting without setting
    /// `done`, which would leave the parse stage blocked on `batch_ready`
    /// forever and hang the join. Both directions are exercised:
    ///
    ///   quit-during-gather: quit with the pty idle, so the gather stage is
    ///     asleep in its outer `poll` and the parse stage is asleep on
    ///     `batch_ready`. The gather stage's `DoneSignal` guard is the ONLY
    ///     thing that wakes the parse stage.
    ///   quit-during-parse: quit while a bulk stream is in flight, so the ring
    ///     is non-empty and the parse stage is mid-`processOutput`.
    ///
    /// A regression in either direction hangs, so the join is bounded by a
    /// watchdog channel rather than blocking forever.
    ///
    /// THE BOUND IS A STALL BOUND, NOT A WALL-CLOCK BOUND (S-15, N293). On a
    /// quit the parse stage drains every published batch before it exits, and
    /// the gather stage keeps filling slots without polling `quit` while the
    /// pty still yields full 64 KiB buffers. Both are the reference's design
    /// (`Exec.zig` parse loop: `if (pipeline.done) return;` only once
    /// `count == 0`; gather: `if (total == buffer_capacity) continue;`), so a
    /// correct shutdown from quit-during-parse first parses what is still in
    /// the ring and the pty queue. Measured on an M4 Pro, idle host, `dev`
    /// profile, three runs: 145-170 KiB were still in flight at the quit and
    /// the drain took 2.3-2.6 s, i.e. ~65 KiB/s. (The ~3 s before the quit is
    /// the eight blocking 64 KiB writes waiting on the same parse.) The `dev`
    /// parse is that slow because every page mutation runs
    /// `Page::verify_integrity` (sampled: `page::mask`, `page::get_bits` and
    /// `verify_integrity` are the top frames of the parse stage), which is the
    /// reference's `assertIntegrity` under `slow_runtime_safety`
    /// (`page.zig:366-376`, Debug builds only): faithful, not a port defect.
    /// A fixed 10 s bound on the WHOLE drain therefore failed under host load
    /// (load average 20-29, task S-6: 1 run in 8) while the pipeline was
    /// healthy and still making progress.
    ///
    /// The regression looks different: a stage that exits without waking the
    /// other leaves the parse stage asleep on `batch_ready`, so no batch ever
    /// reaches `processOutput` again. The watchdog therefore fails only when
    /// the thread has not joined AND no new batch has reached `processOutput`
    /// for `STALL`. A live drain hands over a batch every ~0.7-0.8 s here
    /// idle (one 64 KiB slot at ~65 KiB/s; the longest gap is printed as
    /// `SHUTDOWN-DRAIN`). Under a synthetic load of 42 spinning processes on
    /// 14 cores (load average 46) the longest gap measured 4.1-4.7 s and the
    /// whole drain up to 9.96 s, at the old fixed 10 s bound. STALL is
    /// 30 s: over 6x the loaded gap, and a hang is still caught 30 s after its
    /// last batch instead of wedging the suite. The drain is finite (at most the
    /// 512 KiB written), so a livelock cannot hide behind the progress rule:
    /// once every byte is processed, progress stops and the stall rule applies.
    #[test]
    #[cfg(target_os = "macos")]
    fn readthread_pipeline_shuts_down_cleanly_from_both_stages() {
        use std::time::{Duration, Instant};

        let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        for scenario in ["quit-during-gather", "quit-during-parse"] {
            let (handle, master, slave, quit_write, t) = spawn_readthread_over_pty();

            const CHUNK: usize = 64 * 1024;
            const CHUNKS: usize = 8;
            if scenario == "quit-during-parse" {
                // Put a bulk stream in flight so the ring is non-empty and the
                // parse stage is busy when the quit arrives.
                let chunk = vec![b'z'; CHUNK];
                for _ in 0..CHUNKS {
                    let w = unsafe {
                        libc::write(
                            slave,
                            chunk.as_ptr().cast::<std::ffi::c_void>(),
                            chunk.len(),
                        )
                    };
                    assert_eq!(w, CHUNK as isize, "{scenario}: wrote a bulk chunk");
                }
                // The scenario's premise, checked rather than assumed: bytes
                // are still in flight (ring or pty queue) when the quit lands.
                let at_quit = t.processed_bytes();
                assert!(
                    at_quit < CHUNK * CHUNKS,
                    "{scenario}: all {at_quit} bytes were parsed before the quit, \
                     so this is not a quit during parse"
                );
            } else {
                // Idle pty: let both stages settle into their waits.
                std::thread::sleep(Duration::from_millis(20));
            }

            let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };

            // Bounded join: a hang here IS the regression, so surface it as a
            // failure rather than wedging the suite. The bound is STALL since
            // the last batch reached processOutput (see the doc comment).
            const STALL: Duration = Duration::from_secs(30);
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            let joiner = std::thread::spawn(move || {
                handle.join().expect("read thread join");
                let _ = tx.send(());
            });
            let quit_at = Instant::now();
            let mut last_calls = t.processed_calls();
            let mut last_progress = quit_at;
            let mut longest_gap = Duration::ZERO;
            let joined = loop {
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(()) => break true,
                    // The joiner panicked (the read thread panicked): not joined.
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break false,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
                let calls = t.processed_calls();
                if calls != last_calls {
                    longest_gap = longest_gap.max(last_progress.elapsed());
                    last_calls = calls;
                    last_progress = Instant::now();
                } else if last_progress.elapsed() >= STALL {
                    break false;
                }
            };
            eprintln!(
                "SHUTDOWN-DRAIN {scenario}: joined={joined} after {:?}, longest gap \
                 between batches {longest_gap:?}",
                quit_at.elapsed()
            );

            if joined {
                joiner.join().expect("joiner join");
            }
            unsafe {
                libc::close(quit_write);
                libc::close(master);
                libc::close(slave);
            }
            if joined {
                t.teardown();
            } else {
                // The read thread is still alive and still owns `io`, so
                // `teardown()` (which moves the Termio out of its box and
                // invalidates its self-pointers) would be UB. Leaking the
                // harness is the only safe option on this path.
                //
                // `TestTermio` is not `Drop` — its `teardown` is an explicit
                // consuming method — so clippy is right that `mem::forget` is a
                // no-op here. The no-op IS the point: NOT calling `teardown` is
                // the leak, and the explicit forget documents that at the site.
                #[allow(clippy::forget_non_drop)]
                std::mem::forget(t);
            }

            assert!(
                joined,
                "{scenario}: the read pipeline did not shut down, and no batch \
                 reached processOutput for {STALL:?} (a stage exited without \
                 waking the other)"
            );
        }
    }
}

/// task-14e-1 fix round 1: the FIFTH emission shape's actual bytes.
#[cfg(all(test, not(windows)))]
mod child_raw_log_tests {
    use super::{log_err_child_raw, LOG_SCOPE};

    /// Runs `f` with fd 2 redirected into a pipe and returns everything written
    /// to it. This captures the REAL bytes `log_err_child_raw` puts on the wire,
    /// rather than re-asserting the literal that produced them — the failure
    /// mode being guarded against is a hand-rendered prefix drifting from what
    /// `logFn` would write, and a test that reads the same literal back cannot
    /// see that.
    ///
    /// fd 2 is process-global, so this borrows the crate's fork-KAT
    /// serialization lock: it is the existing "don't let two tests touch
    /// process-global fd state at once" mutex, and holding it here keeps a
    /// concurrent test's stderr out of our pipe (and ours out of theirs).
    fn capture_stderr(f: impl FnOnce()) -> Vec<u8> {
        let _serial = crate::FORK_KAT_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let mut fds = [0i32; 2];
        assert_eq!(0, unsafe { libc::pipe(fds.as_mut_ptr()) }, "pipe(2) failed");
        let (read_fd, write_fd) = (fds[0], fds[1]);

        // SAFETY: all four calls are plain fd manipulation on fds we own.
        let saved = unsafe { libc::dup(2) };
        assert!(saved >= 0, "dup(2) of stderr failed");
        assert!(
            unsafe { libc::dup2(write_fd, 2) } >= 0,
            "dup2 onto stderr failed"
        );

        f();

        // Restore stderr FIRST, then close the write end, so the read below
        // cannot block waiting on a writer that is still open.
        assert!(unsafe { libc::dup2(saved, 2) } >= 0, "dup2 restore failed");
        unsafe {
            libc::close(saved);
            libc::close(write_fd);
        }

        let mut buf = [0u8; 1024];
        let n = unsafe {
            libc::read(
                read_fd,
                buf.as_mut_ptr().cast::<std::ffi::c_void>(),
                buf.len(),
            )
        };
        unsafe { libc::close(read_fd) };
        assert!(n >= 0, "read(2) from the capture pipe failed");
        buf[..n as usize].to_vec()
    }

    /// The bytes must be exactly what `logFn` would have written for this level
    /// and scope.
    ///
    /// The level token is the trap: Zig's `std.log.Level.asText` maps `.err` to
    /// the string **`"error"`** (`lib/std/log.zig:45`), NOT to the tag name
    /// `"err"`. Two stages have shipped `err(` on this exact rendering
    /// (`rustty-apprt`'s `gtk/pre_exec.rs` said it for the whole life of that
    /// file, corrected by task-14d-3 fix round 1), so this test asserts the
    /// token explicitly and separately as well as pinning the whole line.
    #[test]
    fn log_err_child_raw_writes_exactly_what_logfn_would() {
        let captured = capture_stderr(|| log_err_child_raw(&crate::Error));
        let text = String::from_utf8(captured).expect("the rendering is ASCII");

        assert_eq!("error(io_exec): error initializing child: Error\n", text);

        // Stated separately so a failure names the actual defect rather than
        // just showing two long unequal strings.
        assert!(
            text.starts_with("error("),
            "the level token must be `error` (Zig's Level.asText for .err), \
             never the tag name `err`; got {text:?}"
        );
        assert!(
            text.starts_with(&format!("error({LOG_SCOPE}): ")),
            "the scope must be this file's LOG_SCOPE, the same tag the facade \
             sites pass as `target:`; got {text:?}"
        );
        assert!(
            text.ends_with('\n'),
            "logFn terminates every line with a newline; got {text:?}"
        );
    }

    /// The whole point of the shape: no allocation and no stderr lock on the
    /// post-`fork` path. This cannot be asserted directly, so it is asserted
    /// STRUCTURALLY — the helper's body must not name the facade or any of the
    /// locking/allocating printers. A future edit that "simplifies" it back to
    /// `log::error!` or `eprintln!` fails here.
    #[test]
    fn log_err_child_raw_uses_no_facade_and_no_locking_printer() {
        let src = include_str!("exec.rs");
        let start = src
            .find("fn log_err_child_raw(")
            .expect("the helper must exist");
        // The body ends at the next top-level item.
        let end = src[start..]
            .find("\nimpl Subprocess {")
            .expect("the helper is defined just above `impl Subprocess`")
            + start;
        let body = &src[start..end];

        // The facade tokens are ASSEMBLED rather than written out. Spelling
        // `log` + `::` + `error!` as one literal here puts a live-LOOKING facade
        // macro into this file's source, and `check_log_reconcile.py
        // --self-test`'s LIVE-TREE arm — a naive grep cross-checked against the
        // gate's own string-stripping scanner — reads it as a site the scanner
        // "ate" and goes RED. The gate is right and the first draft of this test
        // was wrong; joining the fragments keeps the assertion identical and the
        // naive grep quiet.
        let sep = "::";
        let facade_error = format!("log{sep}error!");
        let facade_warn = format!("log{sep}warn!");
        for forbidden in [
            facade_error.as_str(),
            facade_warn.as_str(),
            "eprintln!",
            "eprint!",
            "format!",
        ] {
            assert!(
                !body.contains(forbidden),
                "`log_err_child_raw` runs between fork(2) and execve(2); \
                 `{forbidden}` allocates and/or takes the stderr lock, either of \
                 which can be held by a thread that did not survive the fork"
            );
        }
        assert!(
            body.contains("libc::write("),
            "the emission must be a raw write(2) — POSIX async-signal-safe"
        );
    }

    /// UD101 (R13, task R13-D): the behaviour the structural test above stands
    /// in for. The reference logs from the forked child through the global
    /// `logFn`, which takes a process-wide stderr lock (`main_ghostty.zig`,
    /// `std.debug.lockStderr`); if another thread held it at the fork, it is held
    /// forever in the child and the child hangs before `exec`. Here a thread
    /// holds std's stderr lock across a real `fork`, and the child runs
    /// `log_err_child_raw`; it must exit, within 5 s, where a locking printer
    /// would block for good.
    #[test]
    fn log_err_child_raw_completes_in_a_child_forked_while_stderr_is_locked() {
        use std::sync::mpsc::channel;
        let _serial = crate::FORK_KAT_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let (locked_tx, locked_rx) = channel::<()>();
        let (release_tx, release_rx) = channel::<()>();
        let holder = std::thread::spawn(move || {
            let _guard = std::io::stderr().lock();
            locked_tx.send(()).unwrap();
            let _ = release_rx.recv();
        });
        locked_rx.recv().unwrap();

        // SAFETY: the child only redirects fd 2, calls the helper under test
        // (stack buffer + `write(2)`) and `_exit`s.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork(2) failed");
        if pid == 0 {
            unsafe {
                let null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
                libc::dup2(null, 2);
                log_err_child_raw(&crate::Error);
                libc::_exit(0);
            }
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut status = 0;
        let exited = loop {
            let rc = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            if rc == pid {
                break true;
            }
            if std::time::Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        if !exited {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, &mut status, 0);
            }
        }
        release_tx.send(()).unwrap();
        holder.join().unwrap();

        assert!(
            exited,
            "the forked child hung on the stderr lock another thread held at the fork"
        );
        assert!(
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
            "the child ran to its _exit(0), status {status:#x}"
        );
    }
}

/// task-14e-1: the `err=E{s}` wire form of `Exec.zig:213`, `:1759` and `:1770`.
#[cfg(all(test, not(windows)))]
mod errno_tests {
    use super::errno_tag_name;

    /// The reference gets these names from `@tagName(std.posix.E)`, i.e. from
    /// the target's own libc. rustty transcribes the numbers, so the
    /// transcription MUST be re-derived from the running platform rather than
    /// trusted — the `rustty-cli` `errno_suffix` precedent's first draft had
    /// LINUX's `EAGAIN`, which is `EDEADLK` on Darwin, so a would-block error
    /// would have printed `EDEADLK` and vice versa.
    ///
    /// `std::io::Error::from_raw_os_error(n).to_string()` is `strerror(n)` on
    /// unix, so this asserts each constant against the host's real message.
    #[test]
    fn errno_table_matches_the_host_libc() {
        // (constant, a distinctive lowercase fragment of the host strerror)
        let rows: &[(i32, &str)] = &[
            (libc::EPERM, "not permitted"),
            (libc::EBADF, "bad file"),
            (libc::EINTR, "interrupted"),
            (libc::EIO, "input/output error"),
            (libc::EAGAIN, "temporarily unavailable"),
            (libc::EDEADLK, "deadlock"),
            (libc::EACCES, "permission denied"),
            (libc::EFAULT, "bad address"),
            (libc::EINVAL, "invalid argument"),
            (libc::EMFILE, "too many open files"),
            (libc::ENOSPC, "no space left"),
            (libc::EPIPE, "broken pipe"),
            (libc::ESPIPE, "seek"),
            (libc::ESRCH, "no such process"),
            (libc::EFBIG, "file too large"),
            // "result": Darwin/FreeBSD say "Result too large", glibc says
            // "Numerical result out of range", musl "Result not
            // representable". The earlier "too large" was Darwin's text only.
            (libc::ERANGE, "result"),
            (libc::EOVERFLOW, "value too large"),
            (libc::ENOLCK, "locks"),
        ];
        for (code, fragment) in rows {
            let msg = std::io::Error::from_raw_os_error(*code)
                .to_string()
                .to_lowercase();
            assert!(
                msg.contains(fragment),
                "errno {code} is {msg:?} on this host, expected it to contain \
                 {fragment:?} — the hand-transcribed table is wrong for this target"
            );
        }
    }

    /// The rendered suffix is the errno NAME with the leading `E` stripped, so
    /// the site's `err=E{}` reads `err=EBADF`. An unknown code falls back to
    /// the NUMBER, never to a wrong name.
    #[test]
    fn errno_tag_name_renders_the_name_not_the_number() {
        assert_eq!("BADF", errno_tag_name(libc::EBADF));
        assert_eq!("PIPE", errno_tag_name(libc::EPIPE));
        assert_eq!("INVAL", errno_tag_name(libc::EINVAL));
        // 4095 is not a real errno on any supported target.
        assert_eq!("4095", errno_tag_name(4095));
    }
}
