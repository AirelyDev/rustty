//! The crash package contains all the logic around crash handling,
//! whether that's setting up the system to catch crashes (Sentry client),
//! introspecting crash reports, writing crash reports to disk, etc.
//!
//! rustty-crash
//!
//! PORTS: ghostty src/crash/main.zig (this file) + dir.zig + sentry.zig +
//! sentry_envelope.zig; hosts the `pkg/sentry` dep-glue (`sentry_pkg`, over the
//! vendored sentry-sys) and the crash entry of the reference's generated
//! `build_options` blob (`build_options`). Since `15c50c1db` ("crash: do not
//! use global state") the reference imports no `global` at all: callers hand
//! `init`/`defaultDir` an owned/borrowed environment-map snapshot, and the
//! rustty-global edge is for the `EnvMap` TYPE only.
//!
//! PORT STATUS: done
//!
//! SEAM: `beforeSend`'s `ThreadState.surface` metadata block is behind the
//! local `sentry::ThreadStateSurface` trait (rustty-core's Surface implements
//! it later; see sentry.rs + DEFERRALS.md).

// const dir = @import("dir.zig");
// (PORT NOTE: Zig-private file imports widen to `pub mod` — the R1/G3b gates and
// in-crate consumers address items by module path; the reference's public
// surface is the explicit re-export list below.)
pub mod dir;
// const sentry_envelope = @import("sentry_envelope.zig");
pub mod sentry_envelope;

// PORT-hosted (not in src/crash/): the `build_options` blob entry + the
// `@import("sentry")` Zig package.
pub mod build_options;
pub mod sentry_pkg;

// pub const sentry = @import("sentry.zig");
pub mod sentry;
// pub const Envelope = sentry_envelope.Envelope;
pub use sentry_envelope::Envelope;
// pub const defaultDir = dir.defaultDir;
pub use dir::defaultDir;
// pub const Dir = dir.Dir;
pub use dir::Dir;
// pub const ReportIterator = dir.ReportIterator;
pub use dir::ReportIterator;
// pub const Report = dir.Report;
pub use dir::Report;

// The main init/deinit functions for global state.
// pub const init = sentry.init;
pub use sentry::init;
// pub const deinit = sentry.deinit;
pub use sentry::deinit;

// test {
//     @import("std").testing.refAllDecls(@This());
// }
#[cfg(test)]
mod tests {
    /// Faithful analogue of main.zig's `refAllDecls(@This())` reify test:
    /// reference every re-exported public declaration so the crate surface is
    /// exercised by the test build. No live Sentry client is touched — the
    /// reference gates `sentry_init` behind build options/threading and the
    /// vendored build is `transport = .none`; tests must not start it.
    #[test]
    fn ref_all_decls() {
        // pub const sentry = @import("sentry.zig");
        let _ = std::any::type_name::<crate::sentry::Type>();
        let _ = std::any::type_name::<crate::sentry::Transport>();
        // pub const Envelope = sentry_envelope.Envelope;
        let _ = std::any::type_name::<crate::Envelope>();
        // pub const defaultDir = dir.defaultDir;
        let _: fn(&rustty_global::EnvMap) -> Result<crate::Dir, rustty_os::xdg::DirError> =
            crate::defaultDir;
        // pub const Dir = dir.Dir; / ReportIterator / Report
        let _ = std::any::type_name::<crate::Dir>();
        let _ = std::any::type_name::<crate::ReportIterator>();
        let _ = std::any::type_name::<crate::Report>();
        // pub const init = sentry.init; / deinit = sentry.deinit;
        let _: fn(rustty_global::EnvMap) -> Result<(), crate::sentry::InitError> = crate::init;
        let _: fn() = crate::deinit;
    }
}
