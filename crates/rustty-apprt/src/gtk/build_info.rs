//! rustty-apprt::gtk::build_info
//!
//! PORTS: ghostty `src/apprt/gtk/build/info.zig` (18 loc) — the GTK apprt's
//! application-id / resource-path / object-path constants.
//!
//! PORT STATUS: done (L5.0 T4). All four consts ported; the two build-mode
//! splits are live.
//!
//! # R2: identity comes from `rustty-identity`, not a hardcoded literal
//!
//! The reference hardcodes `base_application_id = "com.mitchellh.ghostty"` and
//! `resource_path = "/com/mitchellh/ghostty"` (`info.zig:4,12`). Per R2 the
//! product identity has ONE source of truth — [`rustty_identity::BUNDLE_ID`] —
//! so [`BASE_APPLICATION_ID`] *is* that constant and [`RESOURCE_PATH`] is derived
//! from it by the same reverse-DNS -> path transform the reference literal
//! encodes: `com.mitchellh.ghostty` -> `/com/mitchellh/ghostty`, i.e. a leading
//! `/` with every `.` replaced by `/`. Under the default profile the bundle id is
//! `io.airely.rustty`, so the path is `/io/airely/rustty`; under the
//! `identity-oracle` harness feature it collapses back to the reference values.
//!
//! # Build-mode split -> `cfg!(debug_assertions)`
//!
//! The reference switches on `builtin.mode` (`info.zig:7-10,15-17`):
//! `.Debug`/`.ReleaseSafe` append `-debug`/`_debug`; `.ReleaseFast`/`.ReleaseSmall`
//! do not. Rust has no `builtin.mode`; the faithful map is `debug_assertions`,
//! which is on for `dev`/`test` (the safe/debug builds) and off for `release`
//! (the fast/small builds). See the DEFERRALS note: `ReleaseSafe` (safety checks
//! ON, no `-debug` split difference from `Debug` here) has no exact cargo analogue
//! — both reference "safe" arms produce the suffixed id, and `debug_assertions`
//! is the closest single predicate that carries the suffix into non-optimized
//! builds and drops it in optimized ones.

#![allow(clippy::doc_markdown)]

/// Compile-time concatenation of two byte strings into a fixed-size buffer.
/// `N` must equal `a.len() + b.len()`; the caller supplies it as a const expr.
/// Rust's std has no const `&str` concat, so we build the bytes ourselves and
/// hand them back as an array a `const` item can then reinterpret as `&str`.
const fn concat_bytes<const N: usize>(a: &str, b: &str) -> [u8; N] {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut out = [0u8; N];
    let mut i = 0;
    while i < a.len() {
        out[i] = a[i];
        i += 1;
    }
    let mut j = 0;
    while j < b.len() {
        out[i + j] = b[j];
        j += 1;
    }
    out
}

/// Base application ID. Reference: `info.zig:4` (`base_application_id`), sourced
/// from [`rustty_identity::BUNDLE_ID`] per R2 (single source of truth).
pub const BASE_APPLICATION_ID: &str = rustty_identity::BUNDLE_ID;

// ---- GTK application ID (info.zig:7-10) -----------------------------------
// `.Debug`/`.ReleaseSafe => base ++ "-debug"`, else `base`.

#[cfg(debug_assertions)]
const APPLICATION_ID_BUF: [u8; BASE_APPLICATION_ID.len() + "-debug".len()] =
    concat_bytes(BASE_APPLICATION_ID, "-debug");

/// GTK application ID. Reference: `info.zig:7-10`. Carries the `-debug` suffix
/// in unoptimized (`debug_assertions`) builds, matching the reference's
/// `.Debug`/`.ReleaseSafe` arm.
#[cfg(debug_assertions)]
pub const APPLICATION_ID: &str = match core::str::from_utf8(&APPLICATION_ID_BUF) {
    Ok(s) => s,
    Err(_) => panic!("application id is not valid utf-8"),
};

/// GTK application ID. Reference: `info.zig:7-10` (`.ReleaseFast`/`.ReleaseSmall`
/// arm) — equal to the base id in optimized builds.
#[cfg(not(debug_assertions))]
pub const APPLICATION_ID: &str = BASE_APPLICATION_ID;

// ---- resource path (info.zig:12) ------------------------------------------
// Derived from the bundle id: leading `/`, every `.` -> `/`.

/// `/` + `BASE_APPLICATION_ID` with `.` replaced by `/`. Same length as the
/// bundle id plus the leading slash.
const fn resource_path_bytes() -> [u8; BASE_APPLICATION_ID.len() + 1] {
    let src = BASE_APPLICATION_ID.as_bytes();
    let mut out = [0u8; BASE_APPLICATION_ID.len() + 1];
    out[0] = b'/';
    let mut i = 0;
    while i < src.len() {
        out[i + 1] = if src[i] == b'.' { b'/' } else { src[i] };
        i += 1;
    }
    out
}

const RESOURCE_PATH_BUF: [u8; BASE_APPLICATION_ID.len() + 1] = resource_path_bytes();

/// GTK resource path. Reference: `info.zig:12` (the literal `/com/mitchellh/ghostty`),
/// here derived from [`BASE_APPLICATION_ID`] in reverse-DNS path form (R2).
pub const RESOURCE_PATH: &str = match core::str::from_utf8(&RESOURCE_PATH_BUF) {
    Ok(s) => s,
    Err(_) => panic!("resource path is not valid utf-8"),
};

// ---- GTK object path (info.zig:15-17) -------------------------------------
// `.Debug`/`.ReleaseSafe => resource_path ++ "_debug"`, else `resource_path`.

#[cfg(debug_assertions)]
const OBJECT_PATH_BUF: [u8; RESOURCE_PATH.len() + "_debug".len()] =
    concat_bytes(RESOURCE_PATH, "_debug");

/// GTK object path. Reference: `info.zig:15-17`. Carries the `_debug` suffix in
/// unoptimized (`debug_assertions`) builds.
#[cfg(debug_assertions)]
pub const OBJECT_PATH: &str = match core::str::from_utf8(&OBJECT_PATH_BUF) {
    Ok(s) => s,
    Err(_) => panic!("object path is not valid utf-8"),
};

/// GTK object path. Reference: `info.zig:15-17` (optimized arm) — equal to the
/// resource path.
#[cfg(not(debug_assertions))]
pub const OBJECT_PATH: &str = RESOURCE_PATH;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_id_is_identity_bundle() {
        assert_eq!(BASE_APPLICATION_ID, rustty_identity::BUNDLE_ID);
    }

    #[test]
    fn resource_path_is_bundle_in_path_form() {
        assert_eq!(
            RESOURCE_PATH,
            format!("/{}", rustty_identity::BUNDLE_ID.replace('.', "/"))
        );
        assert!(RESOURCE_PATH.starts_with('/'));
        assert!(!RESOURCE_PATH.contains('.'));
    }

    #[test]
    fn build_mode_split() {
        if cfg!(debug_assertions) {
            assert_eq!(APPLICATION_ID, format!("{BASE_APPLICATION_ID}-debug"));
            assert_eq!(OBJECT_PATH, format!("{RESOURCE_PATH}_debug"));
        } else {
            assert_eq!(APPLICATION_ID, BASE_APPLICATION_ID);
            assert_eq!(OBJECT_PATH, RESOURCE_PATH);
        }
    }
}
