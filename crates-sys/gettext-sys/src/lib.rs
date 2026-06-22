//! gettext-sys — FFI to the vendored GNU gettext gnulib `localename`
//! canonicalizer (`links = "intl"`).
//!
//! PORT STATUS: vendor slice (macOS `_libintl_locale_name_canonicalize`)
//!
//! R3: copies the SAME C library the reference uses (GNU gettext / gnulib) and
//! builds it standalone (`cc` + `bindgen`; see `build.rs`). No pure-Rust
//! substitution during parity phases. This crate exposes the single symbol
//! consumed by `os/i18n.zig` (`canonicalizeLocale`): the macOS
//! `gl_locale_name_canonicalize`, renamed to `_libintl_locale_name_canonicalize`
//! by `vendor/config.h`. It converts a macOS/BCP-47 locale name to a Unix
//! (XPG) locale name in place.
//!
//! macOS-only: the reference builds libintl from gettext only on macOS (Linux
//! provides libintl in libc). The FFI is therefore gated to `target_os =
//! "macos"`; on other targets this crate compiles to an empty surface.

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

#[cfg(target_os = "macos")]
mod bindings {
    // bindgen-generated FFI for `_libintl_locale_name_canonicalize`.
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

#[cfg(target_os = "macos")]
pub use bindings::_libintl_locale_name_canonicalize;

/// Canonicalize a macOS / BCP-47 locale `name` in place into a Unix (XPG)
/// locale name, using the vendored gnulib `gl_locale_name_canonicalize`.
///
/// `name` must be a NUL-terminated, writable buffer large enough to hold the
/// result (gnulib documents it as "a sufficiently large buffer"): the function
/// overwrites the buffer with the canonicalized name.
///
/// # Safety
/// `name` must point to a valid, writable, NUL-terminated C string with enough
/// trailing capacity for the canonicalized result.
#[cfg(target_os = "macos")]
pub unsafe fn locale_name_canonicalize(name: *mut std::os::raw::c_char) {
    // SAFETY: forwarded to the C function under the caller's contract above.
    unsafe { _libintl_locale_name_canonicalize(name) };
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::ffi::CStr;

    /// Canonicalize a NUL-terminated input into an owned Rust `String`, using a
    /// generously oversized buffer (gnulib writes the result in place).
    fn canon(input: &str) -> String {
        // gnulib writes the canonicalized name in place; size the buffer well
        // above any possible output for these short inputs.
        let mut buf: Vec<u8> = Vec::with_capacity(64);
        buf.extend_from_slice(input.as_bytes());
        buf.push(0);
        buf.resize(64, 0);
        // SAFETY: buf is a writable NUL-terminated C string with 64 bytes of
        // capacity, far larger than any canonicalized output here.
        unsafe { locale_name_canonicalize(buf.as_mut_ptr().cast()) };
        let cstr = CStr::from_bytes_until_nul(&buf).unwrap();
        cstr.to_str().unwrap().to_owned()
    }

    /// Link/known-answer test. Expected values are the documented gnulib
    /// `gl_locale_name_canonicalize` (macOS variant) mappings — the *raw* C
    /// function, NOT the higher-level `os/i18n.zig` `canonicalizeLocale` wrapper
    /// (which layers extra zh-region handling on top). They are independently
    /// sourced from gnulib upstream (lib/localename.c: the macOS script-suffix
    /// rules map a bare `zh-Hans`/`zh-Hant` via its script table and otherwise
    /// only rewrite `-` to `_`) — NOT re-derived from the code under test.
    /// Matching these proves the vendored C compiled, linked, and runs (R3
    /// vendor-toolchain proof) and exercises the macOS script-suffix handling.
    #[test]
    fn canonicalize_known_answers() {
        // Bare Simplified-Chinese script -> mainland China (gnulib script map).
        assert_eq!(canon("zh-Hans"), "zh_CN");
        // Bare Traditional-Chinese script -> Taiwan (gnulib script map).
        assert_eq!(canon("zh-Hant"), "zh_TW");
        // Script + explicit region: the raw function only rewrites `-` to `_`
        // (the script is NOT collapsed here; that is the Zig wrapper's job).
        assert_eq!(canon("zh-Hant-HK"), "zh_Hant_HK");
        // A plain Unix name with a dash separator is normalized to underscore.
        assert_eq!(canon("en-US"), "en_US");
    }
}
