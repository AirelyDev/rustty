//! Termio is responsible for "terminal IO." Specifically, this is the
//! reading and writing of bytes for the underlying pty or pty-like device.
//!
//! Termio is constructed of a few components:
//!   - Termio - The main shared struct that has common logic across all
//!     backends and mailboxes (defined below).
//!   - Backend - Responsible for the actual physical IO. For example, one
//!     implementation creates a subprocess, allocates and assigns a pty,
//!     and sets up a read thread on the pty.
//!   - Mailbox - Responsible for storing/dispensing event messages to
//!     the backend. This exists separately from backends because termio
//!     is built to be both single and multi-threaded.
//!
//! Termio supports (and recommends) multi-threaded operation. Multi-threading
//! enables the read/writes to generally happen on separate threads and
//! almost always improves throughput and latency under heavy IO load. To
//! enable threading, use the Thread struct. This wraps a Termio, requires
//! specific backend/mailbox capabilities, and sets up the necessary threads.
//!
//! rustty-termio
//!
//! PORTS: ghostty src/termio.zig (this file) + termio/{Options.zig, Termio.zig,
//! Thread.zig, backend.zig, mailbox.zig, message.zig, Exec.zig,
//! shell_integration.zig, stream_handler.zig} + the RELOCATED
//! src/Command.zig (break #13b: Command-in-core would cycle — termio/Exec
//! constructs+starts Command while core already deps termio).
//!
//! PORT STATUS: ported (integration-tier stages 2-4 + the D6 live-thread fill),
//! R1 contract 44 tests (Exec 10 + shell_integration 21 + message 2 + root 1 +
//! Command 10). The xev-loop/live-thread paths are LIVE — D6 was RESOLVED
//! 2026-07-03 (xev Milestone B: `Thread.run` + `Termio.init`/`threadEnter`/
//! `processOutput` + `Subprocess.start`/`ReadThread`/`queueWrite`, proven by the
//! live-shell KAT; the old "D6-deferred" sentence here was stale doc drift,
//! realigned per VALIDATION_REPORT.md §3b). The honest residual seams are the
//! platform arms OUTSIDE the macOS-first fork: the Windows spawn/read arms
//! (`exec.rs` `threadMainWindows` + windows `Subprocess.start`) and the
//! flatpak comptime-false branches — see DEFERRALS.md "rustty-termio stage 2/4".
//! Module notes below record each module's ORIGINAL stage state; the deferred
//! bodies named there are now live except the windows/flatpak arms:
//!
//! - `message` — termio/message.zig (2 tests): the IO-thread `Message` union.
//! - `options` — termio/Options.zig (0 tests): the `Options` struct shape.
//! - `termio_impl` — termio/Termio.zig (0 tests): the `Termio` struct,
//!   `DerivedConfig`/`ThreadEnterState`/`ThreadData`/`MutexState` + pure
//!   delegation; the live IO-thread bodies are deferred verbatim (DEFERRALS.md).
//! - `backend` — termio/backend.zig (0 tests): the `Kind`/`Config`/`Backend`/
//!   `ThreadData` delegating unions over the stage-4 `exec` payloads.
//! - `mailbox` — termio/mailbox.zig (0 tests): the SPSC `Mailbox` (+ the
//!   deferred `send` slow path).
//! - `thread` — termio/Thread.zig (0 tests): the writer-thread struct shape +
//!   init/deinit; the run loop is deferred verbatim (D6).
//! - `xev_seam` — the D6 placeholder surface for
//!   `@import("../global.zig").xev`; the handle types are hosted by the shared
//!   `rustty-xev` crate since core C0 (re-exported here so in-crate paths are
//!   unchanged; rustty-os cf_release_thread precedent).
//! - `command` — the RELOCATED src/Command.zig (10 tests, all ported): the
//!   fork/exec subprocess spawner (integration-tier stage 3, break #13b).
//! - `exec` — termio/Exec.zig (10 tests, all ported): the posix exec/pty
//!   backend — Subprocess argv+env assembly + execCommand LIVE; the xev
//!   wiring + Subprocess.start deferred verbatim (stage 4, DEFERRALS.md).
//! - `shell_integration` — termio/shell_integration.zig (21 tests, all
//!   ported): the byte-exact per-shell command/env generation (stage 4).
//! - `stream_handler` — termio/stream_handler.zig (0 tests): the vt-Stream
//!   `StreamHandler` (the `Handler` trait impl); the cross-thread writer slow
//!   paths deferred verbatim (stage 4, DEFERRALS.md).
//!
//! SEAMS: `apprt.surface.Mailbox` -> `rustty_apprt_api::surface::AppMailbox`
//! (termio is a generic consumer, `M: AppMailbox`); `renderer.Size`/`State`/
//! `Thread.Mailbox` -> rustty-renderer; `xev` -> `xev_seam` (LIVE over the
//! shared `rustty-xev` crate since D6/Milestone B resolved).

// const stream_handler = @import("termio/stream_handler.zig");
// TEST-ONLY (WS10 T20): the tally of every byte `Termio::processOutput` is
// handed, used by `readthread_pipeline_sustains_saturated_producer` to witness
// the pipeline's "no lost or duplicated batch" property. `#[cfg(test)]`, so it
// does not exist in any shipped build.
#[cfg(test)]
pub(crate) static PROCESSED_BYTES: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

// TEST-ONLY (WS10 T20): the number of `processOutput` CALLS, i.e. the number of
// batches the parse stage was handed. With `PROCESSED_BYTES` this gives the mean
// batch size, which is the pipeline's structural signature: the pre-pipeline
// loop read into a `[u8; 1024]`, so its batches CANNOT exceed 1 KiB, while the
// pipeline's are up to `BUFFER_CAPACITY` (64 KiB). Unlike a throughput number
// that is a timing measurement, this one is immune to machine contention.
#[cfg(test)]
pub(crate) static PROCESSED_CALLS: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

pub mod stream_handler;

// const message = @import("termio/message.zig");
// (PORT NOTE: Zig-private file imports widen to `pub mod` — the R1/G3b gates and
// in-crate consumers address items by module path; the reference's public
// surface is the explicit re-export list below.)
pub mod message;
// pub const backend = @import("termio/backend.zig");
pub mod backend;
// pub const mailbox = @import("termio/mailbox.zig");
pub mod mailbox;
// pub const Exec = @import("termio/Exec.zig");
pub mod exec;
pub use exec::Exec;
// (PORT NOTE: shell_integration.zig is file-private to the reference — imported
// only by Exec.zig and the root reify test; `pub mod` per the Zig-private
// widening rule above.)
pub mod shell_integration;
// pub const Options = @import("termio/Options.zig");
pub mod options;
pub use options::Options;
// pub const Termio = @import("termio/Termio.zig");
pub mod termio_impl;
pub use termio_impl::Termio;
// pub const Thread = @import("termio/Thread.zig");
pub mod thread;
pub use thread::Thread;
// pub const Backend = backend.Backend;
pub use backend::Backend;
// pub const DerivedConfig = Termio.DerivedConfig;
pub use termio_impl::DerivedConfig;
// pub const Mailbox = mailbox.Mailbox;
pub use mailbox::Mailbox;
// pub const Message = message.Message;
pub use message::Message;
// pub const StreamHandler = stream_handler.StreamHandler;
pub use stream_handler::StreamHandler;

// (No reference import line here: src/Command.zig is a src-ROOT file —
// termio/Exec.zig imports it as `@import("../Command.zig")` and core's
// Surface.zig as `@import("Command.zig")`. Break #13b relocates it INTO
// rustty-termio — core placement would cycle, blueprint §1 — and re-exports it
// at the crate root, preserving the reference's flat `Command` symbol: core's
// Surface reaches it as `rustty_termio::Command`.)
pub mod command;
pub use command::Command;

// The D6 xev placeholder seam (see its module doc) — since core C0 a re-export
// of the shared `rustty-xev` crate.
pub mod xev_seam;

// task-14e-1: process-wide capture `log::Log` for the restored `LOG`-class
// diagnostics (DECISION-logging.md §6/§6r4). Test-only — never compiled into
// the production library.
#[cfg(test)]
pub(crate) mod log_capture;

/// PORT NOTE (GROWTH POINT, Rust-only glue): the reference termio methods
/// return `!void`/`!T` — error unions whose error SETS are *inferred* from the
/// concrete backend (`termio.Exec`) and the xev loop. The port does not carry
/// every member; it carries the ones some caller dispatches on, and `Other`
/// for the rest (precedent: `rustty_apprt_api::ApprtError`).
///
/// The named members are the three `Thread.threadMain`'s failure screen
/// switches on (`Thread.zig:157-207`): `OpenptyFailed` (`pty.zig`'s
/// `Pty.open`), and `InputNotFound` / `InputFailed` (`Termio.threadEnter`'s
/// configured `input`).
//
// CORRECTION (S212-F1): this was the unit struct `pub struct Error;` with no
// discriminant, so `threadMain` could not tell the three apart and painted
// every failure with the general message. The unit struct was also a VALUE,
// `Error`, used as `Err(Error)` / `map_err(|_| Error)` at about 190 sites
// across the workspace; the `Error` constant below keeps those meaning
// `Error::Other`, so only the sites that produce a named member change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Any termio error `threadMain` does not single out.
    Other,
    /// `error.OpenptyFailed`: the system could not allocate a pty.
    OpenptyFailed,
    /// `error.InputNotFound`: a configured `input` path does not exist.
    InputNotFound,
    /// `error.InputFailed`: a configured `input` could not be read or queued.
    InputFailed,
}

/// `Error::Other`, under the name the unit struct had (see the CORRECTION on
/// [`Error`]).
#[allow(non_upper_case_globals)]
pub const Error: Error = Error::Other;

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Other => write!(f, "termio operation failed"),
            // `{}` of a Zig error prints its name.
            Error::OpenptyFailed => write!(f, "error.OpenptyFailed"),
            Error::InputNotFound => write!(f, "error.InputNotFound"),
            Error::InputFailed => write!(f, "error.InputFailed"),
        }
    }
}

impl std::error::Error for Error {}

/// The io threads' crash metadata: `crash.sentry.thread_state = .{ .type =
/// .io, .surface = io.surface_mailbox.surface };` on [`IoCrashThreadState::set`],
/// and its `defer crash.sentry.thread_state = null;` as `Drop`. Used by
/// `Thread.threadMain_` and both `ReadThread` mains (LEDGER S211-F2, S212-F2).
pub(crate) struct IoCrashThreadState(());

impl IoCrashThreadState {
    /// # Safety
    /// `io` points to a live `Termio` whose `surface_mailbox` is not being
    /// written (it is set once, in `Termio.init`).
    pub(crate) unsafe fn set<M: rustty_apprt_api::surface::AppMailbox>(
        io: *const termio_impl::Termio<M>,
    ) -> Self {
        // Only the field is borrowed, never the whole `Termio`, which other
        // threads reach through raw pointers too.
        let mailbox: &M = unsafe { &*std::ptr::addr_of!((*io).surface_mailbox) };
        rustty_crash::sentry::setThreadStateDyn(
            rustty_crash::sentry::Type::Io,
            mailbox.crash_surface(),
        );
        IoCrashThreadState(())
    }
}

impl Drop for IoCrashThreadState {
    fn drop(&mut self) {
        rustty_crash::sentry::clearThreadState();
    }
}

/// Test-only: the surface the crate's test `AppMailbox` doubles report from
/// `crash_surface` (they have no core `Surface`).
#[cfg(test)]
pub(crate) struct TestCrashSurface;

#[cfg(test)]
impl rustty_crash::sentry::ThreadStateSurface for TestCrashSurface {
    fn screen_size(&self) -> (u32, u32) {
        (0, 0)
    }
    fn grid_size(&self) -> (u32, u32) {
        (0, 0)
    }
    fn cell_size(&self) -> (u32, u32) {
        (0, 0)
    }
}

/// The one [`TestCrashSurface`]; tests compare `crash_surface` pointers to it.
#[cfg(test)]
pub(crate) static TEST_CRASH_SURFACE: TestCrashSurface = TestCrashSurface;

// (test-support, Rust-only glue) Serialize this binary's fork-spawning KATs so
// two never `fork()` concurrently: a `fork` snapshots EVERY open fd, so a
// concurrent fork can leak another test's pty master/slave fds into an
// unrelated child (the parallel-cargo-test isolation lesson). On macOS this is
// not just an fd-hygiene issue: a SIGHUP'd `login(1)` can wedge
// UNINTERRUPTIBLY (SIGKILL-immune, state `E`) in kernel exit draining pending
// pty slave output, and any process holding a leaked slave fd copy can wedge
// the same way when it exits — which is exactly the T8 CI hang
// (ci-run-2026-07-10.log: `command_rt_pre_exec_1` + the two exec KATs stuck
// >29min). The Zig reference never faces this because its test runner is
// single-threaded per binary; Rust's libtest forks from a threaded process.
// This is a serialization (every test still runs, nothing is skipped or
// weakened) — the in-crate convention formerly private to `exec::tests` as
// FORK_KAT_SERIAL, hoisted so `command::tests`' fork KATs share it.
#[cfg(all(test, not(windows)))]
pub(crate) static FORK_KAT_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

// test {
//     @import("std").testing.refAllDecls(@This());
//
//     _ = @import("termio/shell_integration.zig");
// }
#[cfg(test)]
mod tests {
    // PORT NOTE: the `_ = @import("termio/shell_integration.zig")` half of the
    // reference test reifies shell_integration.zig — landed (stage 4) as the
    // `shell_integration` module above with its own 21 tests; the refAllDecls
    // half is ported here.
    #[test]
    fn ref_all_decls() {
        // Reify the crate's public re-export surface (the refAllDecls analogue).
        let _ = std::any::type_name::<crate::Backend>();
        let _ = std::any::type_name::<crate::DerivedConfig>();
        let _ = std::any::type_name::<crate::Mailbox>();
        let _ = std::any::type_name::<crate::Message<'static>>();
        let _ = std::any::type_name::<crate::Thread>();
        let _ = std::any::type_name::<crate::backend::Kind>();
        let _ = std::any::type_name::<crate::backend::Config>();
        let _ = std::any::type_name::<crate::backend::ThreadData>();
        let _ = std::any::type_name::<crate::mailbox::Queue>();
        let _ = std::any::type_name::<crate::message::WriteReq<'static>>();
        let _ = std::any::type_name::<crate::message::SizeReport>();
        let _ = std::any::type_name::<crate::termio_impl::MutexState>();
        let _ = std::any::type_name::<crate::termio_impl::ThreadEnterState>();
        let _ = std::any::type_name::<crate::termio_impl::Input>();
        let _ = std::any::type_name::<crate::termio_impl::PrepareInputError>();
        let _ = std::any::type_name::<crate::thread::Coalesce>();
        let _ = std::any::type_name::<crate::thread::Flags>();
        let _ = std::any::type_name::<crate::xev_seam::Async>();
        let _ = std::any::type_name::<crate::Command<'static>>();
        let _ = std::any::type_name::<crate::Error>();
        // (stage 4: the Exec/shell_integration/StreamHandler surface.)
        let _ = std::any::type_name::<crate::Exec>();
        let _ = std::any::type_name::<crate::exec::Config>();
        let _ = std::any::type_name::<crate::exec::ThreadData>();
        let _ = std::any::type_name::<crate::exec::Subprocess>();
        let _ = std::any::type_name::<crate::shell_integration::Shell>();
        let _ = std::any::type_name::<crate::shell_integration::ShellIntegration>();
        // (`StreamHandler<M>` is generic over the AppMailbox seam and is
        // reified by its module's signatures; the module const stands in.)
        let _ = crate::stream_handler::TMUX_ENABLED;
        // Options/Termio/ThreadData are generic over the AppMailbox seam and
        // are reified by their modules' signatures.
        let _ = (
            crate::thread::Coalesce::MIN_MS,
            crate::thread::SYNC_RESET_MS,
            crate::thread::SELECTION_SCROLL_MS,
        );
    }
}
