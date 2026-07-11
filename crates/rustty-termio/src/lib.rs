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

/// PORT NOTE (GROWTH POINT, Rust-only glue): the reference termio methods
/// return `!void`/`!T` — error unions whose error SETS are *inferred* from the
/// concrete backend (`termio.Exec`, stage 4) and the xev loop (D6). Until they
/// land, this opaque marker error types the seams (precedent:
/// `rustty_apprt_api::ApprtError`); it gains the real error sets at the growth
/// point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error;

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "termio operation failed")
    }
}

impl std::error::Error for Error {}

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
