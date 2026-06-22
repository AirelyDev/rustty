//! App-runtime backend selection.
//!
//! BREAK #3: the `Runtime` enum is DEFINED HERE (ported from the reference
//! `src/apprt/runtime.zig`) so that `rustty-apprt` imports it from build-config
//! rather than the reverse (R4). The `test { _ = Runtime; }` block from the
//! reference file is owned by the `rustty-apprt` crate per the Phase-0
//! inventory contract.

use crate::os::Target;

/// Runtime is the runtime to use for Ghostty. All runtimes do not provide
/// equivalent feature sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Runtime {
    /// Will not produce an executable at all when `zig build` is called.
    /// This is only useful if you're only interested in the lib only (macOS).
    #[default]
    None,

    /// GTK4. Rich windowed application. This uses a full GObject-based
    /// approach to building the application.
    Gtk,
}

impl Runtime {
    pub const fn default_for(target: Target) -> Runtime {
        match target.os {
            // The Linux and FreeBSD default is GTK because it is a full
            // featured application.
            crate::os::Os::Linux | crate::os::Os::Freebsd => Runtime::Gtk,
            // Otherwise, we do NONE so we don't create an exe and we create
            // libghostty. On macOS, Xcode is used to build the app that links
            // to libghostty.
            _ => Runtime::None,
        }
    }
}
