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

/// Test builds only (N354, task W-5): record on the calling thread which step of
/// `threadEnter` (or of `Subprocess.start` under it) failed, and the error it
/// failed with. The reference returns a distinct error per step
/// (`error.ProcessNoPid`, `Command.start`'s error, the spawn's
/// `std.Thread.SpawnError`, ...); the port's `crate::Error` carries only the
/// members some caller dispatches on (S212-F1), so a test reads the step here.
/// Expands to nothing outside tests.
macro_rules! note_thread_enter_failure {
    ($step:expr, $detail:expr) => {{
        #[cfg(test)]
        tests::THREAD_ENTER_FAILURE.with(|f| {
            f.borrow_mut().push(format!("{}: {:?}", $step, $detail));
        });
        #[cfg(not(test))]
        let _ = &$detail;
    }};
}

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

    // PORT NOTE: reference defect (R13), UD367. Not in the reference. The io
    // thread's `threadEnter` runs `subprocess.start`, which writes
    // `subprocess.pty`; the main thread's `getProcessInfo` (the embedded
    // `rustty_surface_foreground_pid` / `rustty_surface_tty_name`, reached
    // from AppleScript `pid` / `tty` and the App Intents terminal entity)
    // reads it with no ordering, so a script asking for `tty of terminal 1`
    // right after `make new window` races the write. `threadEnter` stores
    // this flag `Release` once `start` has returned; the process-info
    // accessors load it `Acquire` and answer `None` until then, as the
    // reference does before `start`. It lives here, beside `subprocess`, not
    // inside it: `start` takes `&mut Subprocess`, whose retag covers every
    // byte of the subprocess, so a flag inside it would race with the main
    // thread's load (`.port/phase-2/mirihar-n444` `ud367_ready_in_subprocess`
    // UB under SB; `ud367_start_unordered` UB under SB and TB;
    // `ud367_ready_in_exec` CLEAN under both).
    pub pty_ready: core::sync::atomic::AtomicBool,
}

mod backend;

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
        // _ = posix.system.close(self.read_thread_pipe);
        #[cfg(not(windows))]
        unsafe {
            libc::close(self.read_thread_pipe)
        };
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD325. The
        // reference's `posix.system.close` is the C runtime's `close` (Ghostty
        // links libc), and on Windows `read_thread_pipe` is a Win32 HANDLE, which
        // the CRT rejects by fast-failing the process (0xC0000409). The port
        // calls `CloseHandle`. CORRECTION (task W-1b, review W-1 F1): the Zig
        // line above used to read `posix.close(self.read_thread_pipe);`, an older
        // shape; `Exec.zig:550` at the pin is `_ = posix.system.close(..)`.
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

mod exec_command;
mod kill;
mod read_thread;
mod subprocess;
pub use exec_command::*;
pub use kill::*;
pub use read_thread::*;
pub use subprocess::*;
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

// ---------------------------------------------------------------------------
// PORT NOTE: support shims (Rust-only glue).
// ---------------------------------------------------------------------------

/// PORT NOTE (Zig-std shim): `posix.system.close(fd)` (`Exec.zig:126-127`).
///
/// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD325. On Windows
/// `posix.fd_t` is a `HANDLE`, and `posix.system` is `std.c` (Ghostty links
/// libc), so the reference passes the HANDLE to the C runtime's `close`, which
/// fast-fails the process (0xC0000409). The port calls `CloseHandle`, as
/// `ThreadData::deinit` does for `read_thread_pipe`.
///
/// CORRECTION (task W-1b, review W-1 F1): this used to say "Zig's
/// `posix.close` is `CloseHandle`"; the reference calls `posix.system.close`,
/// not `posix.close`.
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
mod tests;
