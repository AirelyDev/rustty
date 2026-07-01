//! PORTS: ghostty `src/apprt/runtime.zig` — the `Runtime` enum that selects the
//! app runtime backend (`none` / `gtk`).
//!
//! PORT NOTE: `Runtime` (and its `default(target)`) was hoisted to
//! `rustty-build-config` (break #3 — it IS the comptime `build_config.app_runtime`
//! selector). It is re-exported here so the apprt interface names it at its
//! reference location `apprt.Runtime`. This crate adds only the reference's
//! reify test. The reference `runtime.zig` decl comments are preserved below
//! (verbatim, retargeted per R2) documenting the re-exported type.

/// Runtime is the runtime to use for rustty. All runtimes do not provide
/// equivalent feature sets.
//
//   Will not produce an executable at all when `zig build` is called.
//   This is only useful if you're only interested in the lib only (macOS).
//     -> Runtime::None
//
//   GTK4. Rich windowed application. This uses a full GObject-based
//   approach to building the application.
//     -> Runtime::Gtk
//
// pub fn default(target: std.Target) Runtime {
//     The Linux and FreeBSD default is GTK because it is a full
//     featured application.
//     Otherwise, we do NONE so we don't create an exe and we create
//     librustty. On macOS, Xcode is used to build the app that links
//     to librustty.
// }  (the `default(target)` selector lives in rustty-build-config, break #3)
pub use rustty_build_config::Runtime;

// test { _ = Runtime; }
#[cfg(test)]
mod tests {
    use super::Runtime;

    // PORT NOTE: reference `test { _ = Runtime; }` — a refAllDecls-style reify
    // that forces `Runtime` to be semantically analyzed. Ported as a smoke test
    // that touches the re-exported type (both variants).
    #[test]
    fn reify_runtime() {
        let _ = Runtime::None;
        let _ = Runtime::Gtk;
    }
}
