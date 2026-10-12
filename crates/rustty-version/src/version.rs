//! The app version, composed the way `src/build/Config.zig:276-339` does.
//!
//! The reference computes `config.version` at `zig build` time:
//!
//! - an explicit `-Dversion-string` is always used;
//! - a dependency build (`is_dep`) uses the bare `X.Y.Z` of the app version;
//! - otherwise `GitVersion.detect` runs. Without git, or outside a
//!   repository, the version is `X.Y.Z-dev+0000000`. A commit carrying a tag
//!   other than `tip` must be tagged exactly `vX.Y.Z` (else the build panics)
//!   and is then the bare `X.Y.Z`. Every other build is
//!   `X.Y.Z-<branch>+<short hash>`.
//!
//! The port's `build.rs` runs the git half (`GitVersion.detect`) and hands
//! the result over in `RUSTTY_VERSION_*` variables; the composition itself is
//! the `const fn` [`compose`] below, because the app version it starts from is
//! `rustty_identity::VERSION`, which differs between the default and the
//! `identity-oracle` profiles and which a build script cannot see (build
//! dependencies do not share the normal graph's features). S63-F2 / S64-F1.

/// The capacity of the composed version buffer. The reference formats
/// `version_string` into a 1024-byte buffer (`Config.zig:677-681`).
const CAP: usize = 1024;

/// A composed version: the bytes and their length.
pub struct Composed {
    buf: [u8; CAP],
    len: usize,
}

impl Composed {
    /// The version as a string. Every piece copied in is a `&str` (so UTF-8),
    /// and each is copied whole, so the prefix is valid UTF-8.
    pub const fn as_str(&self) -> &str {
        let (bytes, _) = self.buf.split_at(self.len);
        match core::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => panic!("composed version is not UTF-8"),
        }
    }

    const fn push(mut self, s: &str) -> Self {
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            assert!(self.len < CAP, "version string longer than 1024 bytes");
            self.buf[self.len] = b[i];
            self.len += 1;
            i += 1;
        }
        self
    }
}

/// What `build.rs` found, as the `RUSTTY_VERSION_KIND` value.
///
/// - `explicit`: `RUSTTY_VERSION_STRING` was set (`-Dversion-string`); the
///   version is `explicit` verbatim.
/// - `dep`: this crate is not inside its own workspace (`is_dep`).
/// - `nogit`: `GitNotFound` / `GitNotRepository`.
/// - `tag`: HEAD carries the tag `tag`.
/// - `git`: an untagged (or `tip`-tagged) commit on `branch` at `hash`.
pub const fn compose(
    app_version: &str,
    kind: &str,
    explicit: &str,
    tag: &str,
    branch: &str,
    hash: &str,
) -> Composed {
    let empty = Composed {
        buf: [0; CAP],
        len: 0,
    };
    if eq(kind, "explicit") {
        // `try std.SemanticVersion.parse(v)`: an invalid version fails the
        // build.
        assert!(
            is_semver(explicit),
            "RUSTTY_VERSION_STRING must be a semantic version"
        );
        return empty.push(explicit);
    }
    let core = core_version(app_version);
    if eq(kind, "dep") {
        return empty.push(core);
    }
    if eq(kind, "nogit") {
        // If Git isn't available we just make an unknown dev version.
        return empty.push(core).push("-dev+0000000");
    }
    if eq(kind, "tag") {
        // Tip releases behave just like any other pre-release so we skip.
        if !eq(tag, "tip") {
            let t = tag.as_bytes();
            if t.is_empty() || t[0] != b'v' || !bytes_eq(t.split_at(1).1, core.as_bytes()) {
                panic!("tagged releases must be in vX.Y.Z format matching build.zig");
            }
            return empty.push(core);
        }
    } else {
        assert!(eq(kind, "git"), "unknown RUSTTY_VERSION_KIND");
    }
    empty.push(core).push("-").push(branch).push("+").push(hash)
}

/// `MAJOR.MINOR.PATCH` of a semantic version: the reference rebuilds the
/// version from `app_version.major/.minor/.patch`, dropping any pre-release
/// or build metadata the app version itself carries.
pub const fn core_version(v: &str) -> &str {
    let b = v.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] != b'-' && b[i] != b'+' {
        i += 1;
    }
    let (head, _) = b.split_at(i);
    match core::str::from_utf8(head) {
        Ok(s) => s,
        Err(_) => panic!("app version is not UTF-8"),
    }
}

/// A minimal `MAJOR.MINOR.PATCH[-PRE][+BUILD]` shape check: three
/// dot-separated decimal numbers, then nothing, `-` or `+`.
const fn is_semver(v: &str) -> bool {
    let b = v.as_bytes();
    let mut i = 0;
    let mut part = 0;
    while part < 3 {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
        part += 1;
        if part < 3 {
            if i >= b.len() || b[i] != b'.' {
                return false;
            }
            i += 1;
        }
    }
    i == b.len() || b[i] == b'-' || b[i] == b'+'
}

const fn eq(a: &str, b: &str) -> bool {
    bytes_eq(a.as_bytes(), b.as_bytes())
}

const fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(kind: &str, explicit: &str, tag: &str, branch: &str, hash: &str) -> String {
        compose("1.3.2-dev", kind, explicit, tag, branch, hash)
            .as_str()
            .to_string()
    }

    /// Each arm of `Config.zig:283-338`.
    #[test]
    fn compose_follows_the_reference_arms() {
        assert_eq!(c("explicit", "2.0.0-rc1", "", "", ""), "2.0.0-rc1");
        assert_eq!(c("dep", "", "", "", ""), "1.3.2");
        assert_eq!(c("nogit", "", "", "", ""), "1.3.2-dev+0000000");
        assert_eq!(c("tag", "", "v1.3.2", "main", "abc1234"), "1.3.2");
        assert_eq!(c("tag", "", "tip", "main", "abc1234"), "1.3.2-main+abc1234");
        assert_eq!(c("git", "", "", "main", "abc1234"), "1.3.2-main+abc1234");
        // The app version's own pre-release is dropped (`.major/.minor/.patch`).
        assert_eq!(
            compose("1.3.2-dev", "git", "", "", "main", "abc1234").as_str(),
            "1.3.2-main+abc1234"
        );
    }

    #[test]
    #[should_panic(expected = "tagged releases must be in vX.Y.Z format matching build.zig")]
    fn compose_rejects_a_tag_that_is_not_the_app_version() {
        let _ = c("tag", "", "v9.9.9", "main", "abc1234");
    }

    #[test]
    #[should_panic(expected = "RUSTTY_VERSION_STRING must be a semantic version")]
    fn compose_rejects_an_invalid_explicit_version() {
        let _ = c("explicit", "banana", "", "", "");
    }
}
