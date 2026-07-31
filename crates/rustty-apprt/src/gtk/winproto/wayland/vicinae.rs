//! rustty-apprt::gtk::winproto::wayland::vicinae
//!
//! Generated client bindings for `vicinae_hotkey_v1`, the protocol behind
//! [`super::hotkeys::Hotkeys`].
//!
//! PORTS: the reference's `build.zig` `wayland_scanner.addCustomProtocol(
//! b.path("src/apprt/gtk/winproto/wayland/protocols/vicinae-hotkey-v1.xml"))` step,
//! which is what makes `wayland.client.vicinae` exist for `Hotkeys.zig:7`. The XML
//! itself is vendored VERBATIM (305 lines, byte-identical to the oracle's copy,
//! copyright header intact — MIT, © 2026 Aurelien Brabant) at
//! `crates/rustty-apprt/protocols/vicinae-hotkey-v1.xml`.
//!
//! # Why the protocol is vendored rather than pulled from a crate
//!
//! `vicinae_hotkey_v1` is **not** in `wayland-protocols`. It is authored by the
//! vicinae launcher project and is, by its own XML description, *"currently in the
//! testing phase. Backward incompatible changes may be added together with the
//! corresponding interface version bump."* There is no crates.io binding for it,
//! exactly as there is no `zig-wayland` package for it upstream — both trees
//! generate from a vendored copy. **Expect a future incompatible bump**: when the
//! oracle's XML changes, re-vendor it verbatim and re-run the gates; do not patch
//! this file.
//!
//! No mainstream compositor (mutter, KWin, wlroots) implements it today, so on
//! every currently available host `Globals` never binds the manager, [`super::App`]'s
//! `bind_global_shortcuts` returns `false`, and the XDG portal path runs unchanged.
//! The arm is inert, not dead: it is selected per-session, at runtime, purely on
//! whether the compositor advertises `vicinae_hotkey_manager_v1`.
//!
//! # Codegen mechanism
//!
//! wayland-rs 0.31 scans XML in the `wayland_scanner::generate_interfaces!` /
//! `generate_client_code!` PROC MACROS (paths resolved against
//! `CARGO_MANIFEST_DIR`) rather than in a build script — the same mechanism the
//! `wayland-protocols` crate uses for every protocol it ships. `build.rs` still
//! emits the `cargo:rerun-if-changed` edge for the XML, because a proc macro that
//! reads a file gets none automatically.

// The generated code is machine-written: it does not carry rustdoc in the shape
// this crate's lints demand, and it deliberately emits items rustty never calls
// (the full request/event surface of both interfaces, e.g. the `released` event
// and `HotkeyManagerV1::destroy`). Fidelity means generating the WHOLE protocol,
// so the unused half is silenced here rather than trimmed.
#![allow(missing_docs, dead_code, clippy::all, clippy::pedantic)]

use wayland_client;
use wayland_client::protocol::*;

pub mod __interfaces {
    use wayland_client::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("protocols/vicinae-hotkey-v1.xml");
}
use self::__interfaces::*;

wayland_scanner::generate_client_code!("protocols/vicinae-hotkey-v1.xml");
