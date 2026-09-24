//! rustty-apprt-api
//!
//! PORTS: the ghostty apprt interface seam — the `action.zig` `Target`/`Action`
//! types + payloads, the `surface.zig` `Message`/target/mailbox seam, the
//! `structs.zig` value types, `ipc.zig`, and `runtime.zig` — extracted from
//! `src/apprt/` as a pure INTERFACE crate (break #8, breaks apprt<->core;
//! ambiguity A5 resolved here).
//!
//! PORT STATUS: done
//!
//! (The A5 interface seam is COMPLETE — all 5 reference files ported, every
//! reference comment present (G3b 323/323), all 20 reference test{} blocks
//! pass. This crate was held at "in progress" for three *additive* deferrals
//! to later milestones; all three have since landed in other crates (R4), see
//! DEFERRALS below. Status realigned 2026-09-24, parity tail S-8a.)
//!
//! No whole reference file moves intact: this is the trait+types SEAM carved out
//! of `apprt/`. The bulk of `apprt/` (the concrete gtk/embedded backends, the
//! GObject registrations) lives in `rustty-apprt`, the C-ABI projection's
//! header freeze in `rustty-apprt-c`, and `newConfig` in `rustty-core`.
//!
//! THE CYCLE BREAK: rustty-core depends on THIS interface, never on the concrete
//! apprt. [`Target::cval`](action::Target::cval) reaches the opaque `rt_surface`
//! handle through the [`CoreSurfaceHandle`] trait (implemented by core's
//! `Surface`), so this crate never names rustty-core. `check_acyclic.sh` is the
//! hard proof. Cargo REFUSES to even build a cyclic graph, so a standalone
//! `cargo build -p rustty-apprt-api` is itself a cycle-freeness witness.
//!
//! GENERIC, NOT `dyn` (do not "fix" this): the [`App`] trait's
//! [`App::perform_action`] is generic over the surface-handle type — Zig used a
//! SINGLE comptime-selected apprt backend and never dynamic dispatch. [`App`] is
//! therefore deliberately NOT `dyn`-compatible (object-safe); monomorphization is
//! the faithful model. A `dyn App` would both misrepresent the reference and
//! forbid the generic `perform_action`.
//!
//! DEFERRALS (additive, documented, R1-safe) — all RESOLVED in other crates:
//! - GObject boxed/enum-type registration (`getGObjectType` on InitialSize/
//!   Clipboard/ClipboardRequest/Message::ChildExited): landed in rustty-apprt's
//!   gtk backend (`gtk/class/gtypes.rs`, `clipboard_confirmation_dialog.rs`
//!   `ClipboardRequestBoxed`, `surface_child_exited.rs`).
//! - C-ABI pinning of the `CValue` unions to the embedding header: the
//!   `rustty_target_s`/`rustty_action_u`/`rustty_action_s` sizes are frozen
//!   against `include/rustty-embed.h` (`rustty-apprt-c/tests/abi_layout.rs`
//!   `@embed-abi`, cross-checked by `check_embed_h_abi.sh`). Per-arm INTERIOR
//!   field offsets of the action union are not pinned; the reference asserts
//!   only the union size too, so that is an additive check beyond it (see
//!   `abi_layout.rs`).
//! - `surface.newConfig` traces to rustty-core: ported as
//!   `rustty_core::app::new_config`. (The `AppMailbox` queue types
//!   `Timeout`/`Size` were RESOLVED concrete by core C0 — CORE_BLUEPRINT §3:
//!   they are rustty-datastruct's module-level `blocking_queue::{Timeout, Size}`.)

pub mod action;
pub mod ipc;
pub mod runtime;
pub mod seam;
pub mod structs;
pub mod surface;

#[cfg(test)]
mod action_tests;

// ---- top-level re-exports (the reference `apprt` public surface) ----

pub use runtime::Runtime;

pub use action::{Action, Target};

pub use seam::{App, ApprtError, CoreSurfaceHandle, Surface};

pub use surface::{
    should_inherit_working_directory, AppMailbox, ChildExited, Message, NewSurfaceContext,
    ReportTitleStyle, SurfaceMessageEnvelope, WriteReq,
};

pub use structs::{
    Clipboard, ClipboardContent, ClipboardContentOwned, ClipboardReadResult, ClipboardRequest,
    ClipboardRequestType, ColorScheme, ContentScale, CursorPos, IMEPos, KittyRead, KittyWrite,
    Selection, SurfaceSize,
};
