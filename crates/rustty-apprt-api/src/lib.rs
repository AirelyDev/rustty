//! rustty-apprt-api
//!
//! PORTS: the ghostty apprt interface seam — the `action.zig` `Target`/`Action`
//! types + payloads, the `surface.zig` `Message`/target/mailbox seam, the
//! `structs.zig` value types, `ipc.zig`, and `runtime.zig` — extracted from
//! `src/apprt/` as a pure INTERFACE crate (break #8, breaks apprt<->core;
//! ambiguity A5 resolved here).
//!
//! PORT STATUS: in progress
//!
//! (The A5 interface seam is COMPLETE — all 5 reference files ported, all 20
//! reference test{} blocks pass. "in progress" — not "done" — mirrors the
//! renderer/config precedent for a crate carrying documented *additive*
//! deferrals to future milestones: the gtk `getGObjectType` GObject
//! registrations and the byte-exact `CValue` C-ABI pinning, both landing in the
//! rustty-apprt gtk backend / embedded C-API milestone. No reference logic,
//! comment, or test is dropped.)
//!
//! No whole reference file moves intact: this is the trait+types SEAM carved out
//! of `apprt/`. The bulk of `apprt/` (the concrete gtk/embedded backends,
//! `newConfig`, the GObject registrations, the byte-exact C-ABI projection) stays
//! for the future `rustty-apprt` crate.
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
//! DEFERRALS (additive, documented, R1-safe — see each module):
//! - GObject boxed-type registration (`getGObjectType` on InitialSize/Clipboard/
//!   ClipboardRequest/Message::ChildExited) lands in rustty-apprt's gtk backend.
//! - Byte-exact C-ABI pinning of the `CValue` unions to `rustty.h` (the vt-c D2
//!   cbindgen + pinned-padding pattern) is deferred to the apprt embedded C-API
//!   milestone; the `const _` size-asserts here pin the ACTUAL Rust layout only.
//! - `surface.newConfig` + the concrete `AppMailbox` queue types (`Timeout`/
//!   `Size`) trace to rustty-core and land when core does.

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
    Clipboard, ClipboardContent, ClipboardRequest, ClipboardRequestType, ColorScheme, ContentScale,
    CursorPos, IMEPos, Selection, SurfaceSize,
};
