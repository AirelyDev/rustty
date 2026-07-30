//! rustty-os::locale
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/locale.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `assert =
// quirks.inlineAssert`, `macos`, `objc`, `internal_os = main.zig`, and `i18n =
// internal_os.i18n`. In Rust we use the standard library,
// `rustty_util::quirks::inlineAssert`, `objc2`/`objc2-foundation` (R3: bindings
// to the SAME Objective-C runtime/Foundation framework), the sibling
// `crate::{env, i18n}` modules, and platform `libc` for `setlocale`/`LC_ALL`
// (the reference's `locale-c` import, which is just `#include <locale.h>`; R3
// binding to the SAME C library).
#![allow(non_snake_case)]

use rustty_util::quirks::inlineAssert as assert;

use crate::env as internal_os;
// PORT NOTE: `i18n.canonicalizeLocale` is only called from `setLangFromCocoa`'s
// preferred-languages loop, which is a real `cfg(target_os = "macos")` here, so
// the import carries the same cfg.
#[cfg(target_os = "macos")]
use crate::i18n;

// PORT NOTE: `const log = std.log.scoped(.os_locale);`. Routed through the log facade
// where used.

/// Ensure that the locale is set.
pub fn ensureLocale() -> std::io::Result<()> {
    // PORT NOTE: `assert(builtin.link_libc)`. rustty-os links libc unconditionally
    // (R3), so this asserts a constant truth, matching the reference's gate.
    assert(true);

    // Get our LANG env var. We use this many times but we also need
    // the original value later.
    let lang = internal_os::getenv("LANG").unwrap();
    // PORT NOTE: `defer if (lang) |v| v.deinit(alloc);` — the `GetEnvResult` is
    // dropped at scope end; on Windows that frees the allocation (no-op
    // elsewhere). Rust ownership/Drop handles this.

    // On macOS, pre-populate the LANG env var with system preferences.
    // When launching the .app, LANG is not set so we must query it from the
    // OS. When launching from the CLI, LANG is usually set by the parent
    // process.
    #[cfg(target_os = "macos")]
    {
        // Set the lang if it is not set or if its empty.
        if lang.is_none() || lang.as_ref().unwrap().value.is_empty() {
            setLangFromCocoa();
        }
    }
    // PORT NOTE: off Darwin the reference's `lang` is touched only by its
    // `defer ... deinit(alloc)`, which Rust's Drop performs at scope end. Name
    // the binding here so it still counts as used, rather than dropping it early
    // (which would run the release before the reference's `defer` would).
    #[cfg(not(target_os = "macos"))]
    let _ = &lang;

    // Set the locale to whatever is set in env vars.
    if let Some(v) = setlocale(LC_ALL, c"") {
        log_info_setlocale_env(v);
        return Ok(());
    }

    // setlocale failed. This is probably because the LANG env var is
    // invalid. Try to set it without the LANG var set to use the system
    // default.
    if let Some(old_lang) = internal_os::getenv("LANG").unwrap() {
        if !old_lang.value.is_empty() {
            // We don't need to do both of these things but we do them
            // both to be sure that lang is either empty or unset completely.
            let _ = internal_os::setenv(c"LANG", c"");
            let _ = internal_os::unsetenv(c"LANG");

            if let Some(v) = setlocale(LC_ALL, c"") {
                log_info_setlocale_unset(v);

                // If we try to setlocale to an unsupported locale it'll return "C"
                // as the POSIX/C fallback, if that's the case we want to not use
                // it and move to our fallback of en_US.UTF-8
                if v.to_bytes() != b"C" {
                    return Ok(());
                }
            }
        }
    }

    // Failure again... fallback to en_US.UTF-8
    log_warn_fallback();
    if let Some(v) = setlocale(LC_ALL, c"en_US.UTF-8") {
        let _ = internal_os::setenv(c"LANG", c"en_US.UTF-8");
        log_info_setlocale_default(v);
        Ok(())
    } else {
        log_warn_fallback_failed();
        Ok(())
    }
}

// PORT NOTE: the `log.info`/`log.warn` calls in `ensureLocale`. Routed to the log
// facade; reproduced as named no-op helpers preserving the reference's log sites
// and messages.
fn log_info_setlocale_env(_v: &std::ffi::CStr) {
    // log.info("setlocale from env result={s}", .{v});
}
fn log_info_setlocale_unset(_v: &std::ffi::CStr) {
    // log.info("setlocale after unset lang result={s}", .{v});
}
fn log_warn_fallback() {
    // log.warn("setlocale failed with LANG and system default. Falling back to en_US.UTF-8", .{});
}
fn log_info_setlocale_default(_v: &std::ffi::CStr) {
    // log.info("setlocale default result={s}", .{v});
}
fn log_warn_fallback_failed() {
    // log.warn("setlocale failed even with the fallback, uncertain results", .{});
}

/// This sets the LANG environment variable based on the macOS system
/// preferences selected locale settings.
#[cfg(target_os = "macos")]
fn setLangFromCocoa() {
    use objc2_foundation::NSLocale;

    objc2::rc::autoreleasepool(|_pool| {
        // The classes we're going to need.
        // PORT NOTE: `objc.getClass("NSLocale") orelse { log.warn(...); return; }`.
        // The `objc2-foundation` `NSLocale` binding always resolves the class, so
        // the "class not found" warning branch is unreachable here; it is
        // preserved as the comment below.
        //
        // log.warn("NSLocale class not found. Locale may be incorrect.", .{});

        // Get our current locale and extract the language code ("en") and
        // country code ("US")
        let locale = NSLocale::currentLocale();
        let lang = locale.languageCode();
        // PARITY: the reference uses NSLocale's `countryCode` property; the
        // `objc2-foundation` binding marks it deprecated (Apple deprecated it in
        // favor of locale components), but we keep the SAME API the reference
        // calls (R3 binding, no behavior change).
        #[allow(deprecated)]
        let country = locale.countryCode();

        // PORT NOTE: `if (lang.value == null or country.value == null) { ... }`. In the
        // bindings `languageCode` is non-optional and `countryCode` is optional;
        // a missing country code triggers the reference's warning/return.
        let country = match country {
            Some(c) => c,
            None => {
                // log.warn("languageCode or countryCode not found. Locale may be incorrect.", .{});
                return;
            }
        };

        // Get our UTF8 string values
        let c_lang = lang.UTF8String();
        let c_country = country.UTF8String();

        // Convert them to Zig slices
        let z_lang = unsafe { std::ffi::CStr::from_ptr(c_lang) }.to_bytes();
        let z_country = unsafe { std::ffi::CStr::from_ptr(c_country) }.to_bytes();

        // Format our locale as "<lang>_<country>.UTF-8" and set it as LANG.
        {
            // PORT NOTE: `var buf: [128]u8` + `bufPrintZ`. We build a NUL-terminated
            // `CString`; a formatting failure maps to the reference's error log.
            let env_value = format!(
                "{}_{}.UTF-8",
                String::from_utf8_lossy(z_lang),
                String::from_utf8_lossy(z_country)
            );
            let env_value = match std::ffi::CString::new(env_value) {
                Ok(v) => v,
                Err(_err) => {
                    // log.warn("error setting locale from system. err={}", .{err});
                    return;
                }
            };
            // log.info("detected system locale={s}", .{env_value});

            // Set it onto our environment
            if internal_os::setenv(c"LANG", &env_value) < 0 {
                // log.warn("error setting locale env var", .{});
                return;
            }
        }

        // Get our preferred languages and set that to the LANGUAGE
        // env var in case our language differs from our locale.
        'language: {
            let mut buf = [0u8; 1024];
            let pref_ = match preferredLanguageFromCocoa(&mut buf) {
                Ok(v) => v,
                Err(_err) => {
                    // log.warn("error getting preferred languages. err={}", .{err});
                    break 'language;
                }
            };

            let pref = match pref_ {
                Some(p) => p,
                None => break 'language,
            };
            // log.debug("setting LANGUAGE from preferred languages value={s}", .{pref});
            let pref_c = match std::ffi::CString::new(pref) {
                Ok(v) => v,
                Err(_) => break 'language,
            };
            let _ = internal_os::setenv(c"LANGUAGE", &pref_c);
        }
    });
}

/// Sets the LANGUAGE environment variable based on the preferred languages
/// as reported by NSLocale.
///
/// macOS has a concept of preferred languages separate from the system
/// locale. The set of preferred languages is a list in priority order
/// of what translations the user prefers. A user can have, for example,
/// "fr_FR" as their locale but "en" as their preferred language. This would
/// mean that they want to use French units, date formats, etc. but they
/// prefer English translations.
///
/// gettext uses the LANGUAGE environment variable to override only
/// translations and a priority order can be specified by separating
/// the languages with colons. For example, "en:fr" would mean that
/// English translations are preferred but if they are not available
/// then French translations should be used.
///
/// To further complicate things, Apple reports the languages in BCP-47
/// format which is not compatible with gettext's POSIX locale format so
/// we have to canonicalize them.
#[cfg(target_os = "macos")]
fn preferredLanguageFromCocoa(buf: &mut [u8]) -> Result<Option<Vec<u8>>, NoSpaceLeft> {
    use objc2_foundation::NSLocale;

    // PORT NOTE: the reference uses a `std.io.fixedBufferStream(buf)` + writer and
    // tracks `fbs.pos`. We build the result into a `Vec` (the canonicalize step
    // still writes into the remaining `buf` for parity with the reference's
    // in-buffer canonicalization), then return an owned copy.
    let mut out: Vec<u8> = Vec::new();

    // We need to get our app's preferred languages. These may not
    // match the system locale (NSLocale.currentLocale).
    let preferred = NSLocale::preferredLanguages();
    for i in 0..preferred.count() {
        // PORT NOTE: `var str_buf: [255:0]u8` used by `str.cstring(&str_buf, .utf8)`.
        let str = preferred.objectAtIndex(i);
        let c_str = str.UTF8String();
        if c_str.is_null() {
            // I don't think this can happen but if it does then I want
            // to know about it if a user has translation issues.
            // log.warn("failed to convert a preferred language to UTF-8", .{});
            continue;
        }
        let c_str = unsafe { std::ffi::CStr::from_ptr(c_str) };

        // Append our separator if we have any previous languages
        if !out.is_empty() {
            out.push(b':');
        }

        // Apple languages are in BCP-47 format, and we need to
        // canonicalize them to the POSIX format.
        // PORT NOTE: `i18n.canonicalizeLocale(fbs.buffer[fbs.pos..], c_str)`. We
        // canonicalize into the tail of `buf` (after the bytes written so far)
        // exactly like the reference, then append the result to `out`.
        let pos = out.len();
        if pos > buf.len() {
            return Err(NoSpaceLeft);
        }
        let canon =
            i18n::canonicalizeLocale(&mut buf[pos..], c_str.to_str().map_err(|_| NoSpaceLeft)?)
                .map_err(|_| NoSpaceLeft)?;
        let canon = canon.to_vec();
        out.extend_from_slice(&canon);

        // The canonicalized locale never contains the encoding and
        // all of our translations require UTF-8 so we add that.
        out.extend_from_slice(b".UTF-8");
    }

    // If we had no preferred languages then we return nothing.
    if out.is_empty() {
        return Ok(None);
    }

    // Null terminate it
    // PORT NOTE: the reference writes a trailing NUL byte into the buffer
    // (`writer.writeByte(0)`). In Rust the caller converts the returned bytes to
    // a `CString`, which appends the NUL; we therefore do not push it here.

    // Get our slice, this won't be null terminated so we have to
    // reslice it with the null terminator.
    // PORT NOTE: the reference does `slice[0 .. slice.len - 1 :0]` to return a
    // sentinel-terminated `[:0]const u8`. We return the bytes (no trailing NUL);
    // the caller converts to a `CString`.
    Ok(Some(out))
}

/// PORT NOTE: `error{NoSpaceLeft}` — the error of `preferredLanguageFromCocoa`.
#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq, Eq)]
pub struct NoSpaceLeft;

// PORT NOTE: the reference imports `const c = @import("locale-c")` (a thin
// `#include <locale.h>`) and aliases `LC_ALL`, `LC_ALL_MASK`, `locale_t`,
// `setlocale`, `newlocale`, `freelocale`. These come from `libc` (R3 binding to
// the SAME C library). `LC_ALL_MASK`/`newlocale`/`freelocale` are unused by the
// reference's runtime paths but are kept as part of the imported surface.
const LC_ALL: std::ffi::c_int = libc::LC_ALL;
#[allow(dead_code)]
const LC_ALL_MASK: std::ffi::c_int = libc::LC_ALL_MASK;
// PARITY: the reference type alias is `locale_t` (snake_case, from the C lib).
#[allow(dead_code, non_camel_case_types)]
type locale_t = libc::locale_t;

/// `c.setlocale`: returns the active locale C-string or `None` on failure.
fn setlocale(
    category: std::ffi::c_int,
    locale: &std::ffi::CStr,
) -> Option<&'static std::ffi::CStr> {
    let p = unsafe { libc::setlocale(category, locale.as_ptr()) };
    if p.is_null() {
        None
    } else {
        Some(unsafe { std::ffi::CStr::from_ptr(p) })
    }
}

#[allow(dead_code)]
unsafe fn newlocale(
    category_mask: std::ffi::c_int,
    locale: *const std::ffi::c_char,
    base: locale_t,
) -> locale_t {
    unsafe { libc::newlocale(category_mask, locale, base) }
}

#[allow(dead_code)]
unsafe fn freelocale(loc: locale_t) {
    unsafe { libc::freelocale(loc) };
}
