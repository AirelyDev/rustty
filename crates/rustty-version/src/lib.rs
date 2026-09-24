//! rustty-version
//!
//! PORTS: `src/build_config.zig`'s `version`, `version_string` and
//! `release_channel`, whose values `src/build/Config.zig` computes at build time
//! (`:276-339` the version, `:693-701` the channel) from `src/build/GitVersion.zig`.
//!
//! PORT STATUS: done
//!
//! These three live in their own leaf crate, not in `rustty-build-config` with
//! the rest of `build_config.zig`, because they change on every commit (see
//! `Cargo.toml`). Only the crates that report the version depend on this one.
//! The values are the reference's: `X.Y.Z-<branch>+<short hash>` for an untagged
//! checkout, `X.Y.Z` for a `vX.Y.Z` tag, `X.Y.Z-dev+0000000` without git, and
//! `RUSTTY_VERSION_STRING` (the port of `-Dversion-string`) verbatim.
//!
//! NO dependency on the ghostty tree (R3). No upward deps (R4).

#![forbid(unsafe_code)]

#[cfg(test)]
mod git_version;
pub mod version;

use rustty_build_config::ReleaseChannel;

/// The semantic version of this build.
///
/// Reference: `pub const version = options.app_version;`. The app version
/// `X.Y.Z` comes from the `rustty-identity` source-of-truth crate (R2); the
/// pre-release and build metadata come from git, composed as `Config.zig`
/// does (see [`version`]). CORRECTION (S63-F2 / S64-F1, 2026-09-23): this used
/// to be `rustty_identity::VERSION` verbatim, so every build reported `0.1.0`
/// on the `stable` channel.
pub const VERSION: &str = VERSION_STRING;

/// The composed version (`Config.zig:276-339`), from what `build.rs` found.
const COMPOSED: version::Composed = version::compose(
    rustty_identity::VERSION,
    env!("RUSTTY_VERSION_KIND"),
    env!("RUSTTY_VERSION_EXPLICIT"),
    env!("RUSTTY_VERSION_TAG"),
    env!("RUSTTY_VERSION_BRANCH"),
    env!("RUSTTY_VERSION_HASH"),
);

/// The semantic version of this build, as a string.
///
/// Reference: `pub const version_string = options.app_version_string;`.
pub const VERSION_STRING: &str = COMPOSED.as_str();

/// The release channel for this build.
///
/// Reference: `release_channel = std.meta.stringToEnum(ReleaseChannel,
/// @tagName(options.release_channel)).?`, which `Config.zig:693-701` derives
/// from the composed version (see [`ReleaseChannel::from_version_pre`]): a
/// version with no pre-release component is `stable`, otherwise `tip`.
pub const fn release_channel() -> ReleaseChannel {
    ReleaseChannel::from_version_pre(version_pre())
}

/// Reference: `build_config.version.build`, the `+BUILD` metadata of the
/// composed version: the short commit hash for a git build, `0000000` without
/// git, whatever an explicit `RUSTTY_VERSION_STRING` carries, and `None` for a
/// tagged release (`X.Y.Z`).
///
/// CORRECTION (review T-B): `rustty-cli` used to capture this with its own
/// `git rev-parse --short HEAD` in its build script, beside the composed
/// version; it now reads it from the version it belongs to, as the reference
/// does.
pub const fn version_build() -> Option<&'static str> {
    let bytes = VERSION_STRING.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            let (_, rest) = VERSION_STRING.split_at(i + 1);
            return Some(rest);
        }
        i += 1;
    }
    None
}

/// Returns the pre-release component of [`VERSION`], if any.
///
/// A semantic version `MAJOR.MINOR.PATCH[-PRE][+BUILD]` has a pre-release
/// component only when a `-` separator appears before any `+`. This mirrors the
/// reference channel derivation in `build/Config.zig`'s `addOptions`:
/// `const pre = self.version.pre orelse break :channel .stable; if (pre.len ==
/// 0) break :channel .stable; break :channel .tip;`.
const fn version_pre() -> Option<&'static str> {
    // `const fn` can't iterate `str` ergonomically, so operate on bytes.
    let bytes = VERSION.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            // A build-metadata separator ends the search with no pre-release.
            b'+' => return None,
            // A pre-release separator: everything after it (up to a `+`) is the
            // pre-release component. We only need to know whether it's
            // non-empty, so report a non-empty marker.
            b'-' => {
                if i + 1 < bytes.len() {
                    return Some("pre");
                }
                return None;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use rustty_build_config::ReleaseChannel;

    /// S63-F2 / S64-F1: the version the build script composed reached the
    /// constants, in whichever arm this build took.
    ///
    /// In a git checkout, with no `RUSTTY_VERSION_STRING`, the version is
    /// `X.Y.Z-<branch>+<short hash>` from `GitVersion.detect`
    /// (`Config.zig:332-338`), and a pre-release makes the channel `tip`
    /// (`Config.zig:693-701`). The expectation is computed here by running the
    /// same detection. With `RUSTTY_VERSION_STRING` set (the explicit arm,
    /// `Config.zig:285-288`), the version is that string verbatim.
    #[test]
    // MIRI EXCEPTION (rustty-version, process/file-I/O class): runs the same
    // `git` detection as the build script, which Miri's isolation forbids. The
    // test is pure string comparison otherwise, with no `unsafe`. Registered in
    // .port/phase-2/MIRI_EXCEPTIONS.md (the rustty-cli row it replaces).
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-version): spawns `git`; \
         Miri isolation forbids it — see MIRI_EXCEPTIONS.md"
    )]
    fn version_is_derived_from_git() {
        assert_eq!(crate::VERSION, crate::VERSION_STRING);
        if let Some(explicit) = option_env!("RUSTTY_VERSION_STRING").filter(|v| !v.is_empty()) {
            assert_eq!(env!("RUSTTY_VERSION_KIND"), "explicit");
            assert_eq!(crate::VERSION_STRING, explicit);
            return;
        }
        let core = crate::version::core_version(rustty_identity::VERSION);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(v) = crate::git_version::detect(&root) else {
            assert_eq!(env!("RUSTTY_VERSION_KIND"), "nogit");
            assert_eq!(crate::VERSION_STRING, format!("{core}-dev+0000000"));
            assert_eq!(crate::version_build(), Some("0000000"));
            return;
        };
        let expected = match v.tag.as_deref() {
            Some(t) if t != "tip" => core.to_string(),
            _ => format!("{core}-{}+{}", v.branch, v.short_hash),
        };
        assert_eq!(crate::VERSION_STRING, expected);
        if expected.contains('-') {
            assert_eq!(crate::release_channel(), ReleaseChannel::Tip);
            assert_eq!(crate::version_build(), Some(v.short_hash.as_str()));
        } else {
            assert_eq!(crate::release_channel(), ReleaseChannel::Stable);
            assert_eq!(crate::version_build(), None);
        }
    }

    /// UD150 (R13): git's trailing newline is trimmed BEFORE the branch is
    /// sanitized, so it cannot turn into a trailing `-`. The reference
    /// sanitizes first and yields `feature-x-y-2-` (its cached build options
    /// carry `1.3.2-HEAD-+4540d499a`).
    #[test]
    fn git_branch_is_trimmed_before_it_is_sanitized() {
        assert_eq!(
            crate::git_version::branch_from_rev_parse("feature/x_y.2\n"),
            "feature-x-y-2"
        );
        assert_eq!(
            crate::git_version::branch_from_rev_parse("main\r\n"),
            "main"
        );
        // Interior characters are still sanitized (`ad9225a4c`'s purpose).
        assert_eq!(
            crate::git_version::branch_from_rev_parse("a b\tc\n"),
            "a-b-c"
        );
        // And the version this build composed carries no `-+`.
        assert!(
            !crate::VERSION_STRING.contains("-+"),
            "{}",
            crate::VERSION_STRING
        );
    }
}
