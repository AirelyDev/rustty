//! rustty-font options
//!
//! PORTS: ghostty `terminal_options` build-generated module, the subset the
//! `font` module reads (`@import("terminal_options").artifact`).
//!
//! PORT STATUS: done

// PORT NOTE: in the reference there is no `terminal_options.zig` source file —
// the module is *generated* by `build.zig` calling `build_options.Options.add`,
// and font code reads it via
//   if (comptime @import("terminal_options").artifact == .lib) return error.SkipZigTest;
// to skip tests that need the full Ghostty font fixtures in the lib-vt source
// archive build (which intentionally excludes those fixtures).
//
// This module is the ported equivalent of that generated module's `artifact`
// option for the `font` crate (mirroring `rustty-vt`'s `options`/`build_options`
// seam). rustty builds the application artifact (the reference `.ghostty`) and
// ships the font fixtures it needs (copied into this crate's `res/`), so the
// gate resolves to "not lib" and the gated tests run rather than skip.

/// The target artifact to build.
// PORT NOTE: the reference declares this as `pub const Artifact = enum` nested in
// `build_options.Options`; Rust has no nested type declarations inside a struct,
// so it is a sibling `pub enum` (referenced as `Options.Artifact` in Zig).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Artifact {
    /// Ghostty application
    Ghostty,

    /// libghostty-vt, Zig module
    Lib,
}

/// The target artifact to build. This will gate some functionality.
// PORT NOTE: `opts.addOption(Artifact, "artifact", self.artifact)`. rustty builds
// the application artifact (the reference `.ghostty`); the `lib` (libghostty-vt)
// artifact is selected by a later build seam. The lib-vt source archive that sets
// this to `.lib` intentionally excludes the full Ghostty font fixtures, which is
// the only thing the font tests gate on.
pub const ARTIFACT: Artifact = Artifact::Ghostty;
