//! rustty-identity
//!
//! PORTS: NEW R2 crate (no single ghostty src file). Single source of truth for
//! the product *identity*: app name, version strings, `TERM`/`TERM_PROGRAM`,
//! terminfo names, config dir, and VT query responses (XTVERSION, Primary /
//! Secondary Device Attributes). See the crate-structure design Section 6 and
//! `.port/PORT_RULES.md` R2.
//!
//! PORT STATUS: done (rustty-only R2 crate, fully implemented, no live seams;
//! marker added per VALIDATION_REPORT.md §3b).
//!
//! # Dual profile (R2)
//!
//! All *published* rustty artifacts carry the **rustty** identity (clean
//! standalone, NOT a ghostty drop-in). The cargo feature **`identity-oracle`**
//! flips EVERY value below to the reference ghostty values; it is used **only**
//! by the parity / differential harness so port output matches the reference
//! oracle byte-for-byte (no canonicalization hacks).
//!
//! The reference values below were read from the READ-ONLY oracle tree at
//! `/Users/gael/projects/ghostty`:
//!   * app name / TERM_PROGRAM / config subdir / terminfo names — `ghostty`,
//!     `xterm-ghostty` (`src/build_config.zig`, `src/terminfo/ghostty.zig`,
//!     `src/config/file_load.zig`, `src/termio/Exec.zig`).
//!   * `TERM` default — `xterm-ghostty` (`src/config/Config.zig:3724`).
//!   * version — `1.3.2-dev` (`build.zig.zon`).
//!   * VT library version — `0.1.0-dev` (`build.zig:10`, `lib_version`; the
//!     reference deliberately versions libghostty separately from the app:
//!     "We use a separate version from the app").
//!   * bundle id — `com.mitchellh.ghostty` (`src/build_config.zig:58`).
//!   * XTVERSION wire format — `ESC P > | {name} {version} ESC \` from
//!     `src/termio/stream_handler.zig:reportXtversion`; the libghostty-vt
//!     fallback (no app effect) is the bare token `libghostty`
//!     (`src/terminal/stream_terminal.zig:reportXtversion`).
//!
//! Per R2 the identity-EQUIVALENCE gates (ghostty.h ABI match,
//! Swift-app-links-unchanged, terminfo==ghostty) are DROPPED; all BEHAVIORAL
//! parity gates remain in force. This crate is the seam that lets a single
//! codebase satisfy both: rustty by default, ghostty under the harness feature.
//!
//! NO dependency on the ghostty tree (R3) — the values are *copied constants*,
//! the reference is an external oracle only.

#![forbid(unsafe_code)]

/// Active identity profile. There is exactly one, chosen at compile time by the
/// `identity-oracle` cargo feature (R2). It is `Rustty` by default and
/// `Ghostty` under the parity harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// The published product identity (default).
    Rustty,
    /// The reference parity profile (feature `identity-oracle`).
    Ghostty,
}

/// The compile-time-selected active profile.
#[cfg(not(feature = "identity-oracle"))]
pub const PROFILE: Profile = Profile::Rustty;
/// The compile-time-selected active profile.
#[cfg(feature = "identity-oracle")]
pub const PROFILE: Profile = Profile::Ghostty;

// ===========================================================================
// Identity source-of-truth. Every value has a rustty default and a
// `identity-oracle`-gated reference value. Keep these two arms in lockstep:
// a value defined for one profile MUST be defined for the other.
// ===========================================================================

// ---- rustty defaults (published identity) ---------------------------------
#[cfg(not(feature = "identity-oracle"))]
mod values {
    /// Human-facing application name.
    pub const APP_NAME: &str = "rustty";

    /// The semantic version of this build (mirrors the reference scheme; the
    /// reference reads this from `build.zig.zon`).
    pub const VERSION: &str = "0.1.0";

    /// The version of the standalone VT library (librustty-vt), SEPARATE from
    /// the app `VERSION`. This mirrors the reference's deliberate app/lib
    /// version split — `build.zig:10`: `const lib_version = "0.1.0-dev";`
    /// ("Libghostty version. We use a separate version from the app.") —
    /// rustty's own vt library version is `0.1.0` (no pre-release tag).
    pub const VT_LIB_VERSION: &str = "0.1.0";

    /// Reverse-DNS bundle identifier (macOS `.app`, single-instance keys, etc.).
    /// Reference: `com.mitchellh.ghostty` (`src/build_config.zig:58`).
    pub const BUNDLE_ID: &str = "io.airely.rustty";

    /// Value of the `TERM` environment variable we export to the child PTY.
    /// Reference default: `xterm-ghostty` (`src/config/Config.zig:3724`).
    pub const TERM: &str = "xterm-rustty";

    /// Value of the `TERM_PROGRAM` environment variable.
    /// Reference: `ghostty` (`src/termio/Exec.zig:746`).
    pub const TERM_PROGRAM: &str = "rustty";

    /// Primary terminfo entry name (the value that must equal `TERM` so that
    /// tcell-based apps work; see `src/terminfo/ghostty.zig`).
    pub const TERMINFO_PRIMARY_NAME: &str = "xterm-rustty";

    /// Preferred terminfo entry name (the short, canonical name).
    pub const TERMINFO_PREFERRED_NAME: &str = "rustty";

    /// XDG config sub-directory name (`~/.config/<dir>`).
    /// Reference: `ghostty` (`src/config/file_load.zig:14`).
    pub const CONFIG_DIR: &str = "rustty";

    /// Default config file name within the config dir.
    /// Reference: `config.ghostty`.
    pub const CONFIG_FILE: &str = "config.rustty";

    /// The product token reported in the XTVERSION (`CSI > 0 q`) response.
    /// The full wire response is `ESC P > | {token} {version} ESC \`.
    /// Reference: `ghostty` (`src/termio/stream_handler.zig:1001`).
    pub const XTVERSION_NAME: &str = "rustty";

    /// The bare XTVERSION token reported by the standalone VT library when no
    /// application-level version effect is installed.
    /// Reference: `libghostty` (`src/terminal/stream_terminal.zig:376`).
    pub const LIBVT_XTVERSION_FALLBACK: &str = "librustty";

    /// Uppercase prefix for product environment variables (`{ENV_PREFIX}_RESOURCES_DIR`, …).
    pub const ENV_PREFIX: &str = "RUSTTY";

    /// Title-case ("formal") product name, e.g. the third terminfo entry name.
    /// Reference: `"Ghostty"` (`src/terminfo/ghostty.zig`, "Our formal name").
    pub const APP_NAME_TITLECASE: &str = "Rustty";

    /// URL prefix a source-commit hash is appended to, to form the web link for
    /// that commit. `+version` hyperlinks its version header with it (OSC 8).
    /// Reference: `https://github.com/ghostty-org/ghostty/commit/`
    /// (`src/cli/version.zig`). R2: the repo is product identity, so rustty's is
    /// rustty's own — the workspace `Cargo.toml` `repository` value.
    pub const COMMIT_URL_PREFIX: &str = "https://github.com/AirelyDev/rustty/commit/";
}

// ---- ghostty reference values (parity harness only) -----------------------
#[cfg(feature = "identity-oracle")]
mod values {
    /// Human-facing application name. Reference: `src/build_config.zig`.
    pub const APP_NAME: &str = "ghostty";

    /// The semantic version of the reference build, read from
    /// `/Users/gael/projects/ghostty/build.zig.zon` (`.version = "1.3.2-dev"`).
    pub const VERSION: &str = "1.3.2-dev";

    /// The version of the standalone VT library (libghostty-vt), SEPARATE from
    /// the app `VERSION`. Reference: `build.zig:10` — `const lib_version =
    /// "0.1.0-dev";` ("Libghostty version. We use a separate version from the
    /// app."), parsed via `std.SemanticVersion.parse` in
    /// `src/build/Config.zig:307` (major 0, minor 1, patch 0, pre `dev`).
    pub const VT_LIB_VERSION: &str = "0.1.0-dev";

    /// Reverse-DNS bundle identifier. Reference: `src/build_config.zig:58`.
    pub const BUNDLE_ID: &str = "com.mitchellh.ghostty";

    /// Value of the `TERM` environment variable. Reference:
    /// `src/config/Config.zig:3724` (`term: []const u8 = "xterm-ghostty"`).
    pub const TERM: &str = "xterm-ghostty";

    /// Value of the `TERM_PROGRAM` environment variable. Reference:
    /// `src/termio/Exec.zig:746` (`try env.put("TERM_PROGRAM", "ghostty")`).
    pub const TERM_PROGRAM: &str = "ghostty";

    /// Primary terminfo entry name. Reference: `src/terminfo/ghostty.zig`
    /// (first name in `.names`, `"xterm-ghostty"`).
    pub const TERMINFO_PRIMARY_NAME: &str = "xterm-ghostty";

    /// Preferred terminfo entry name. Reference: `src/terminfo/ghostty.zig`
    /// ("The preferred name", `"ghostty"`).
    pub const TERMINFO_PREFERRED_NAME: &str = "ghostty";

    /// XDG config sub-directory name. Reference:
    /// `src/config/file_load.zig:14` (`.subdir = "ghostty/config.ghostty"`).
    pub const CONFIG_DIR: &str = "ghostty";

    /// Default config file name. Reference: `config.ghostty`.
    pub const CONFIG_FILE: &str = "config.ghostty";

    /// XTVERSION product token. Reference:
    /// `src/termio/stream_handler.zig:1001` (the literal `"ghostty"`).
    pub const XTVERSION_NAME: &str = "ghostty";

    /// Standalone VT library XTVERSION fallback token. Reference:
    /// `src/terminal/stream_terminal.zig:376` (the literal `"libghostty"`).
    pub const LIBVT_XTVERSION_FALLBACK: &str = "libghostty";

    /// Uppercase prefix for product environment variables (`{ENV_PREFIX}_RESOURCES_DIR`, …).
    pub const ENV_PREFIX: &str = "GHOSTTY";

    /// Title-case ("formal") product name, e.g. the third terminfo entry name.
    /// Reference: `"Ghostty"` (`src/terminfo/ghostty.zig`, "Our formal name").
    pub const APP_NAME_TITLECASE: &str = "Ghostty";

    /// URL prefix a source-commit hash is appended to. Reference:
    /// `src/cli/version.zig` —
    /// `"\x1b]8;;https://github.com/ghostty-org/ghostty/commit/{s}\x1b\\"`.
    pub const COMMIT_URL_PREFIX: &str = "https://github.com/ghostty-org/ghostty/commit/";
}

// ---- public re-exports (the stable identity API) --------------------------

/// Human-facing application name (`rustty`, or `ghostty` under the harness).
pub const APP_NAME: &str = values::APP_NAME;
/// Semantic version string of this build.
pub const VERSION: &str = values::VERSION;
/// Semantic version string of the standalone VT library. Deliberately SEPARATE
/// from the app `VERSION`, mirroring the reference's `lib_version` split
/// (`build.zig:7-10`: "We use a separate version from the app").
pub const VT_LIB_VERSION: &str = values::VT_LIB_VERSION;
/// Reverse-DNS bundle identifier.
pub const BUNDLE_ID: &str = values::BUNDLE_ID;
/// `TERM` environment variable value.
pub const TERM: &str = values::TERM;
/// `TERM_PROGRAM` environment variable value.
pub const TERM_PROGRAM: &str = values::TERM_PROGRAM;
/// Primary terminfo entry name (must equal `TERM`).
pub const TERMINFO_PRIMARY_NAME: &str = values::TERMINFO_PRIMARY_NAME;
/// Preferred (short) terminfo entry name.
pub const TERMINFO_PREFERRED_NAME: &str = values::TERMINFO_PREFERRED_NAME;
/// XDG config sub-directory name.
pub const CONFIG_DIR: &str = values::CONFIG_DIR;
/// Default config file name.
pub const CONFIG_FILE: &str = values::CONFIG_FILE;
/// XTVERSION product token.
pub const XTVERSION_NAME: &str = values::XTVERSION_NAME;
/// Standalone VT-library XTVERSION fallback token.
pub const LIBVT_XTVERSION_FALLBACK: &str = values::LIBVT_XTVERSION_FALLBACK;
/// Uppercase prefix for product environment variables.
pub const ENV_PREFIX: &str = values::ENV_PREFIX;
/// Title-case ("formal") product name.
pub const APP_NAME_TITLECASE: &str = values::APP_NAME_TITLECASE;
/// URL prefix a source-commit hash is appended to (the `+version` OSC-8 link).
pub const COMMIT_URL_PREFIX: &str = values::COMMIT_URL_PREFIX;

/// The full XTVERSION (`CSI > 0 q`) wire response an application-level handler
/// reports: `ESC P > | {XTVERSION_NAME} {VERSION} ESC \`.
///
/// This mirrors the reference `reportXtversion` in
/// `src/termio/stream_handler.zig` (format `"\x1BP>|{s} {s}\x1B\\"`,
/// args `"ghostty"`, `build_config.version_string`). Under the default profile
/// the token/version are rustty's; under `identity-oracle` they are the
/// reference's, so the byte stream matches the oracle exactly.
pub fn xtversion_response() -> String {
    format!("\x1bP>|{XTVERSION_NAME} {VERSION}\x1b\\")
}

/// The bare XTVERSION (`CSI > 0 q`) wire response the standalone VT library
/// reports when no application-level version effect is installed:
/// `ESC P > | {LIBVT_XTVERSION_FALLBACK} ESC \`.
///
/// Mirrors `src/terminal/stream_terminal.zig:reportXtversion` (format
/// `"\x1BP>|{s}\x1B\\"`, fallback arg `"libghostty"`).
pub fn libvt_xtversion_response() -> String {
    format!("\x1bP>|{LIBVT_XTVERSION_FALLBACK}\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Under the default profile every value is the published rustty identity.
    #[cfg(not(feature = "identity-oracle"))]
    #[test]
    fn default_profile_is_rustty() {
        assert_eq!(PROFILE, Profile::Rustty);
        assert_eq!(APP_NAME, "rustty");
        assert_eq!(TERM, "xterm-rustty");
        assert_eq!(TERM_PROGRAM, "rustty");
        assert_eq!(TERMINFO_PRIMARY_NAME, "xterm-rustty");
        assert_eq!(TERMINFO_PREFERRED_NAME, "rustty");
        assert_eq!(CONFIG_DIR, "rustty");
        assert_eq!(CONFIG_FILE, "config.rustty");
        assert_eq!(XTVERSION_NAME, "rustty");
        assert_eq!(LIBVT_XTVERSION_FALLBACK, "librustty");
        assert_eq!(ENV_PREFIX, "RUSTTY");
        assert_eq!(APP_NAME_TITLECASE, "Rustty");
        assert_eq!(BUNDLE_ID, "io.airely.rustty");
        assert_eq!(
            COMMIT_URL_PREFIX,
            "https://github.com/AirelyDev/rustty/commit/"
        );
        // The vt library carries its OWN version (the reference's deliberate
        // app/lib split, build.zig:7-10); rustty's vt lib version has no
        // pre-release tag.
        assert_eq!(VT_LIB_VERSION, "0.1.0");
        // The primary terminfo name MUST equal TERM (tcell invariant).
        assert_eq!(TERMINFO_PRIMARY_NAME, TERM);
        // VT query wire strings.
        assert_eq!(xtversion_response(), "\x1bP>|rustty 0.1.0\x1b\\");
        assert_eq!(libvt_xtversion_response(), "\x1bP>|librustty\x1b\\");
        // Nothing rustty leaks the reference name.
        assert!(!APP_NAME.contains("ghostty"));
        assert!(!TERM.contains("ghostty"));
        assert!(!xtversion_response().contains("ghostty"));
    }

    /// Under the `identity-oracle` feature every value is the reference's.
    #[cfg(feature = "identity-oracle")]
    #[test]
    fn ghostty_profile_matches_reference() {
        assert_eq!(PROFILE, Profile::Ghostty);
        assert_eq!(APP_NAME, "ghostty");
        assert_eq!(VERSION, "1.3.2-dev");
        assert_eq!(TERM, "xterm-ghostty");
        assert_eq!(TERM_PROGRAM, "ghostty");
        assert_eq!(TERMINFO_PRIMARY_NAME, "xterm-ghostty");
        assert_eq!(TERMINFO_PREFERRED_NAME, "ghostty");
        assert_eq!(CONFIG_DIR, "ghostty");
        assert_eq!(CONFIG_FILE, "config.ghostty");
        assert_eq!(XTVERSION_NAME, "ghostty");
        assert_eq!(LIBVT_XTVERSION_FALLBACK, "libghostty");
        assert_eq!(ENV_PREFIX, "GHOSTTY");
        assert_eq!(APP_NAME_TITLECASE, "Ghostty");
        assert_eq!(BUNDLE_ID, "com.mitchellh.ghostty");
        // Exactly the URL the reference's `+version` OSC-8 hyperlink prefixes
        // the commit hash with (`src/cli/version.zig`).
        assert_eq!(
            COMMIT_URL_PREFIX,
            "https://github.com/ghostty-org/ghostty/commit/"
        );
        // The reference's SEPARATE libghostty version (build.zig:10). This is
        // exactly what the c-vt-build-info golden records ("Version:
        // 0.1.0-dev" / "Version pre  : dev"), and it must NOT equal the app
        // version — that inequality IS the split this constant ports.
        assert_eq!(VT_LIB_VERSION, "0.1.0-dev");
        assert_ne!(VT_LIB_VERSION, VERSION);
        // The primary terminfo name MUST equal TERM (tcell invariant).
        assert_eq!(TERMINFO_PRIMARY_NAME, TERM);
        // VT query wire strings must match the reference byte-for-byte. These
        // are exactly the strings the reference `reportXtversion` emits.
        assert_eq!(xtversion_response(), "\x1bP>|ghostty 1.3.2-dev\x1b\\");
        assert_eq!(libvt_xtversion_response(), "\x1bP>|libghostty\x1b\\");
    }
}
