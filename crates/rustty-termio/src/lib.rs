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
//! Thread.zig, backend.zig, mailbox.zig, message.zig}; termio/{Exec.zig,
//! shell_integration.zig, stream_handler.zig} + the relocated src/Command.zig
//! (break #13b) land in the next integration-tier stages.
//!
//! PORT STATUS: in progress (integration-tier stage 2 — dep wiring + struct
//! shapes + the message module):
//!
//! - `message` — termio/message.zig (2 tests): the IO-thread `Message` union.
//! - `options` — termio/Options.zig (0 tests): the `Options` struct shape.
//! - `termio_impl` — termio/Termio.zig (0 tests): the `Termio` struct,
//!   `DerivedConfig`/`ThreadEnterState`/`ThreadData`/`MutexState` + pure
//!   delegation; the live IO-thread bodies are deferred verbatim (DEFERRALS.md).
//! - `backend` — termio/backend.zig (0 tests): the `Kind`/`Config`/`Backend`/
//!   `ThreadData` delegating unions (Exec payloads are stage-4 placeholders).
//! - `mailbox` — termio/mailbox.zig (0 tests): the SPSC `Mailbox` (+ the
//!   deferred `send` slow path).
//! - `thread` — termio/Thread.zig (0 tests): the writer-thread struct shape +
//!   init/deinit; the run loop is deferred verbatim (D6).
//! - `xev_seam` — the termio-local D6 placeholder surface for
//!   `@import("../global.zig").xev` (rustty-os cf_release_thread precedent).
//!
//! SEAMS: `apprt.surface.Mailbox` -> `rustty_apprt_api::surface::AppMailbox`
//! (termio is a generic consumer, `M: AppMailbox`); `renderer.Size`/`State`/
//! `Thread.Mailbox` -> rustty-renderer; `xev` -> `xev_seam` (D6 deferred).

// const stream_handler = @import("termio/stream_handler.zig");
// (PORT, stage 4: stream_handler.zig lands with the termio testable-logic
// stage; its `StreamHandler.Stream` field type is the `termio_impl::
// StreamHandlerStream` placeholder until then.)

// const message = @import("termio/message.zig");
// (PORT: Zig-private file imports widen to `pub mod` — the R1/G3b gates and
// in-crate consumers address items by module path; the reference's public
// surface is the explicit re-export list below.)
pub mod message;
// pub const backend = @import("termio/backend.zig");
pub mod backend;
// pub const mailbox = @import("termio/mailbox.zig");
pub mod mailbox;
// pub const Exec = @import("termio/Exec.zig");
// (PORT, stage 4: Exec.zig — the posix exec/pty backend, 10 tests — lands with
// the termio testable-logic stage; see `backend::Exec`'s placeholder.)
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
// (PORT, stage 4: re-exported when stream_handler.zig lands.)

// The termio-local D6 xev placeholder seam (see its module doc).
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

// test {
//     @import("std").testing.refAllDecls(@This());
//
//     _ = @import("termio/shell_integration.zig");
// }
#[cfg(test)]
mod tests {
    // PORT NOTE: the `_ = @import("termio/shell_integration.zig")` half of the
    // reference test reifies shell_integration.zig, which lands in stage 4 with
    // its own 21 tests; the refAllDecls half is ported here.
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
        let _ = std::any::type_name::<crate::Error>();
        // Options/Termio/ThreadData are generic over the AppMailbox seam and
        // are reified by their modules' signatures.
        let _ = (
            crate::thread::Coalesce::MIN_MS,
            crate::thread::SYNC_RESET_MS,
            crate::thread::SELECTION_SCROLL_MS,
        );
    }
}
