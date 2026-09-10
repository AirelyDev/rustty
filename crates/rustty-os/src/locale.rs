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

// PORT NOTE (P3-W7 T3 / area-E `E18`): upstream's `locale.zig` stopped
// importing `os/env.zig` (which it deleted) and declared its own file-local
// `extern "c" fn setenv/unsetenv` instead. rustty follows that precedent for
// the whole trio (`getenv` included) — see the `c` module at the bottom of this
// file. The old `use crate::env as internal_os;` is gone.
// PORT NOTE: `i18n.canonicalizeLocale` is only called from `setLangFromCocoa`'s
// preferred-languages loop, which is a real `cfg(target_os = "macos")` here, so
// the import carries the same cfg.
#[cfg(target_os = "macos")]
use crate::i18n;

// PORT NOTE: `const log = std.log.scoped(.os_locale);`. The reference's scope tag,
// carried through the `log` facade as `target: LOG_SCOPE`
// (DECISION-logging.md §6r4).
const LOG_SCOPE: &str = "os_locale";

/// Ensure that the locale is set.
pub fn ensureLocale() -> std::io::Result<()> {
    // This does a lot of in-process mutation of the environment and can't be
    // run in a test as a result.
    //
    // PORT NOTE (P3-W7 T3 / area-E `E18`): upstream added
    // `assert(!builtin.is_test)` here. `builtin.is_test` is whole-compilation
    // in Zig; Rust's `cfg(test)` is NOT transitive, so the literal translation
    // would be silently vacuous whenever `rustty-os` is built as a dependency
    // of another crate's test binary. The invariant is therefore expressed
    // through the crate's own tripwire convention: it fires under
    // `cfg(test)` for THIS crate (where the hazard actually is — a `#[test]`
    // in `rustty-os` wiring this in), and is `debug_assertions`-gated
    // elsewhere, matching `rustty_util::tripwire`'s `enabled()` semantics.
    assert(!cfg!(test));

    // PORT NOTE: `assert(builtin.link_libc)`. rustty-os links libc unconditionally
    // (R3), so this asserts a constant truth, matching the reference's gate.
    assert(true);

    // Get our LANG env var. We use this many times but we also need
    // the original value later.
    let lang = c_getenv(c"LANG");
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
        if lang.is_none() || lang.as_ref().unwrap().is_empty() {
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
        // log.info("setlocale from env result={s}", .{v});
        log::info!(target: LOG_SCOPE, "setlocale from env result={}", v.to_string_lossy());
        return Ok(());
    }

    // Exit early for Windows.
    //
    // NOTE: There currently is no official Windows version of Rustty,
    // either by way of an official Windows port or through use of
    // librustty-internal. As such this function is likely unused on those
    // platforms, so this serves as a TODO stub to support more robust
    // locale support on Windows. setlocale on Windows sets an
    // "implementation-defined native environment" with an empty string, so
    // we are currently just doing best-effort for now.
    //
    // PORT NOTE (P3-W7 T3 / area-E `E18`): NEW ARM from upstream `e8525c0fd`.
    // Without it, Windows falls through to the LANG-unset retry and the
    // `en_US.UTF-8` fallback, neither of which means anything there.
    #[cfg(windows)]
    {
        // log.info("TODO: setlocale failed on Windows, implement better fallbacks", .{});
        log::info!(target: LOG_SCOPE, "TODO: setlocale failed on Windows, implement better fallbacks");
        return Ok(());
    }

    // setlocale failed. This is probably because the LANG env var is
    // invalid. Try to set it without the LANG var set to use the system
    // default.
    #[allow(unreachable_code)]
    if let Some(old_lang) = c_getenv(c"LANG") {
        if !old_lang.is_empty() {
            // We don't need to do both of these things but we do them
            // both to be sure that lang is either empty or unset completely.
            let _ = c_setenv(c"LANG", c"", 1);
            let _ = c_unsetenv(c"LANG");

            if let Some(v) = setlocale(LC_ALL, c"") {
                // log.info("setlocale after unset lang result={s}", .{v});
                log::info!(target: LOG_SCOPE, "setlocale after unset lang result={}", v.to_string_lossy());

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
    // log.warn("setlocale failed with LANG and system default. Falling back to en_US.UTF-8", .{});
    log::warn!(
        target: LOG_SCOPE,
        "setlocale failed with LANG and system default. Falling back to en_US.UTF-8"
    );
    if let Some(v) = setlocale(LC_ALL, c"en_US.UTF-8") {
        let _ = c_setenv(c"LANG", c"en_US.UTF-8", 1);
        // log.info("setlocale default result={s}", .{v});
        log::info!(target: LOG_SCOPE, "setlocale default result={}", v.to_string_lossy());
        Ok(())
    } else {
        // } else log.warn("setlocale failed even with the fallback, uncertain results", .{});
        log::warn!(target: LOG_SCOPE, "setlocale failed even with the fallback, uncertain results");
        Ok(())
    }
}

// PORT NOTE (task-14e-2, S111-F5): the six EMPTY `log_*` helpers that used to
// stand here are gone. A named, empty-bodied helper reads as an emitter and is
// exactly as silent as no call at all — `check_log_reconcile.py` counts such a
// call site as a DROPPED diagnostic, which is the correct reading. Each
// reference call is now emitted inline where the reference emits it, with the
// reference call preserved as the comment above it.

// PORT NOTE (P3-W7 T3 / area-E `E18`): the file-local libc environment
// bindings. Upstream's `locale.zig` declares its own
// `extern "c" fn setenv(...)`/`unsetenv(...)` after `os/env.zig` was deleted;
// `getenv` joins them here because rustty's `env.rs` `getenv` reads the
// CAPTURED environment block, and `ensureLocale` must observe its own in-process
// `setenv`/`unsetenv` mutations immediately — it is the one place that
// legitimately talks to the live process environment.
//
// `global::sync_environ()` is what re-captures the block afterwards; the app
// initializer calls it right after `ensure_locale()`, exactly as upstream's
// `global.init` does.
mod c {
    use std::ffi::{c_char, c_int};

    extern "C" {
        pub fn getenv(name: *const c_char) -> *mut c_char;
        #[cfg(not(windows))]
        pub fn setenv(name: *const c_char, value: *const c_char, overwrite: c_int) -> c_int;
        #[cfg(not(windows))]
        pub fn unsetenv(name: *const c_char) -> c_int;
        #[cfg(windows)]
        pub fn _putenv_s(varname: *const c_char, value_string: *const c_char) -> c_int;
    }
}

/// `std.posix.system.getenv(key)` — the raw bytes, or `None` when unset.
fn c_getenv(key: &std::ffi::CStr) -> Option<Vec<u8>> {
    // SAFETY: `key` is NUL-terminated; the returned pointer, when non-null,
    // points at a NUL-terminated string owned by the C runtime that stays
    // valid until the next environment mutation, and we copy it immediately.
    unsafe {
        let p = c::getenv(key.as_ptr());
        if p.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr(p).to_bytes().to_vec())
    }
}

fn c_setenv(key: &std::ffi::CStr, value: &std::ffi::CStr, overwrite: std::ffi::c_int) -> i32 {
    #[cfg(windows)]
    {
        let _ = overwrite;
        // SAFETY: both pointers are NUL-terminated C strings.
        unsafe { c::_putenv_s(key.as_ptr(), value.as_ptr()) }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: both pointers are NUL-terminated C strings.
        unsafe { c::setenv(key.as_ptr(), value.as_ptr(), overwrite) }
    }
}

fn c_unsetenv(key: &std::ffi::CStr) -> i32 {
    #[cfg(windows)]
    {
        // SAFETY: the pointer is a NUL-terminated C string.
        unsafe { c::_putenv_s(key.as_ptr(), c"".as_ptr()) }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: the pointer is a NUL-terminated C string.
        unsafe { c::unsetenv(key.as_ptr()) }
    }
}

/// This sets the LANG environment variable based on the macOS system
/// preferences selected locale settings.
#[cfg(target_os = "macos")]
fn setLangFromCocoa() {
    use objc2_foundation::NSLocale;

    objc2::rc::autoreleasepool(|_pool| {
        // The classes we're going to need.
        // const NSLocale = objc.getClass("NSLocale") orelse {
        //     log.warn("NSLocale class not found. Locale may be incorrect.", .{});
        //     return;
        // };
        //
        // PORT NOTE (task-14e-2, S111-F5): the reference does a RUNTIME class
        // lookup and has a real failure arm for it. An earlier note here
        // claimed the arm was unreachable because the `objc2-foundation`
        // `NSLocale` binding always resolves — true of the binding, but it is
        // not what the reference does, and it turned a live diagnostic into a
        // comment. The reference's own lookup is performed here (same runtime,
        // same class name, same failure arm); the typed binding below then
        // talks to that same class.
        if objc2::runtime::AnyClass::get(c"NSLocale").is_none() {
            // log.warn("NSLocale class not found. Locale may be incorrect.", .{});
            log::warn!(target: LOG_SCOPE, "NSLocale class not found. Locale may be incorrect.");
            return;
        }

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
                log::warn!(
                    target: LOG_SCOPE,
                    "languageCode or countryCode not found. Locale may be incorrect."
                );
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
                Err(err) => {
                    // log.warn("error setting locale from system. err={}", .{err});
                    //
                    // PORT NOTE (N11): Zig's `{}` on an error renders
                    // `error.<Name>`; `{err:?}` renders the Rust error's own
                    // Debug. Workspace-dominant rendering; N11's scope.
                    log::warn!(target: LOG_SCOPE, "error setting locale from system. err={err:?}");
                    return;
                }
            };
            // log.info("detected system locale={s}", .{env_value});
            log::info!(
                target: LOG_SCOPE,
                "detected system locale={}",
                env_value.to_string_lossy()
            );

            // Set it onto our environment
            if c_setenv(c"LANG", &env_value, 1) < 0 {
                // log.warn("error setting locale env var", .{});
                log::warn!(target: LOG_SCOPE, "error setting locale env var");
                return;
            }
        }

        // Get our preferred languages and set that to the LANGUAGE
        // env var in case our language differs from our locale.
        'language: {
            let mut buf = [0u8; 1024];
            let pref_ = match preferredLanguageFromCocoa(&mut buf) {
                Ok(v) => v,
                Err(err) => {
                    // log.warn("error getting preferred languages. err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error getting preferred languages. err={err:?}");
                    break 'language;
                }
            };

            let pref = match pref_ {
                Some(p) => p,
                None => break 'language,
            };
            // log.debug("setting LANGUAGE from preferred languages value={s}", .{pref});
            log::debug!(
                target: LOG_SCOPE,
                "setting LANGUAGE from preferred languages value={}",
                String::from_utf8_lossy(&pref)
            );
            let pref_c = match std::ffi::CString::new(pref) {
                Ok(v) => v,
                Err(_) => break 'language,
            };
            let _ = c_setenv(c"LANGUAGE", &pref_c, 1);
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
            log::warn!(target: LOG_SCOPE, "failed to convert a preferred language to UTF-8");
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
//
// PORT NOTE (§3.4 laziness, windows arm — W2D_WINDOWS_OS WI-3): this file IS
// windows-reachable (`ghostty src/os/locale.zig:35-46` is `ensureLocale`'s
// windows arm), but the aliases at `locale.zig:224-229` are LAZY decls: on
// windows Zig only ever analyzes the two `ensureLocale` names — `LC_ALL`
// (`:224`) and `setlocale` (`:227`). `LC_ALL_MASK`/`locale_t`/`newlocale`/
// `freelocale` are POSIX-only symbols the Windows UCRT does not export, and
// the reference never analyzes them there, so their Rust image is cfg'd to
// unix (the analog of not being analyzed). `LC_ALL` and `setlocale` must still
// compile on windows, hence the split below.
#[cfg(unix)]
const LC_ALL: std::ffi::c_int = libc::LC_ALL;
// `LC_ALL` as the ucrt's <locale.h> defines it (0 on every Windows CRT —
// MSVC/mingw both, and the `libc` crate's own `windows` module agrees).
#[cfg(windows)]
const LC_ALL: std::ffi::c_int = 0;
#[cfg(unix)]
#[allow(dead_code)]
const LC_ALL_MASK: std::ffi::c_int = libc::LC_ALL_MASK;
// PARITY: the reference type alias is `locale_t` (snake_case, from the C lib).
#[cfg(unix)]
#[allow(dead_code, non_camel_case_types)]
type locale_t = libc::locale_t;

// R3: bind the SAME CRT entry point the reference's `c.setlocale` resolves to
// on windows (ucrtbase's `setlocale`), hand-rolled per the `windows.rs`
// pattern, because the `libc` dependency is unix-gated in Cargo.toml.
#[cfg(windows)]
mod c_locale {
    extern "C" {
        pub fn setlocale(
            category: std::ffi::c_int,
            locale: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char;
    }
}

/// `c.setlocale`: returns the active locale C-string or `None` on failure.
fn setlocale(
    category: std::ffi::c_int,
    locale: &std::ffi::CStr,
) -> Option<&'static std::ffi::CStr> {
    #[cfg(unix)]
    let p = unsafe { libc::setlocale(category, locale.as_ptr()) };
    #[cfg(windows)]
    let p = unsafe { c_locale::setlocale(category, locale.as_ptr()) };
    if p.is_null() {
        None
    } else {
        Some(unsafe { std::ffi::CStr::from_ptr(p) })
    }
}

// `c.newlocale` / `c.freelocale`: unix-only, per the laziness note above.
#[cfg(unix)]
#[allow(dead_code)]
unsafe fn newlocale(
    category_mask: std::ffi::c_int,
    locale: *const std::ffi::c_char,
    base: locale_t,
) -> locale_t {
    unsafe { libc::newlocale(category_mask, locale, base) }
}

#[cfg(unix)]
#[allow(dead_code)]
unsafe fn freelocale(loc: locale_t) {
    unsafe { libc::freelocale(loc) };
}

#[cfg(test)]
mod tests {
    /// P3-W7 T3 Step 7 — NEGATIVE CONTROL for area-E `E18`'s new invariant.
    ///
    /// Upstream added `assert(!builtin.is_test)` to `ensureLocale` with the
    /// comment *"This does a lot of in-process mutation of the environment and
    /// can't be run in a test as a result."* This test proves the tripwire is
    /// live: calling `ensureLocale` from a `#[test]` in THIS crate must panic
    /// rather than silently mutate `LANG` out from under every other test
    /// running in parallel.
    ///
    /// Deleting the `assert(!cfg!(test))` line makes this test fail (the call
    /// then succeeds and returns `Ok`), which is the control.
    ///
    /// `cfg(debug_assertions)`, and NOT because release is less important:
    /// `inlineAssert` is a `debug_assert!` (DG4 — matching Zig, where `assert`
    /// is a no-op in ReleaseFast), so under `--release` the tripwire is compiled
    /// out and there is nothing to observe. Worse, the call would then SUCCEED
    /// and do exactly what the tripwire exists to prevent — mutate `LANG` out
    /// from under every other test in the process. So the test must not merely
    /// be skipped in release, it must not be BUILT to run there.

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "assertion failed")]
    fn ensure_locale_refuses_to_run_under_test() {
        let _ = super::ensureLocale();
    }

    /// task-14e-2 / S111-F5 — all fourteen `os/locale.zig` diagnostics.
    ///
    /// STRUCTURAL, and reported as such. `ensureLocale` is TRIPWIRED against
    /// running under `cfg(test)` — the test directly above this one exists to
    /// prove that — because it mutates the process environment; six of the
    /// fourteen sites are inside it. `setLangFromCocoa` is the same hazard
    /// unguarded (it `setenv`s LANG/LANGUAGE), and this repository has a
    /// `check_no_test_setenv.sh` gate. The remaining arms need a null
    /// `-[NSString UTF8String]`, a CoreFoundation class lookup to fail, or an
    /// interior NUL in a system locale string — none provokable in-process.
    ///
    /// So this pins what can be observed: that every one of the fourteen
    /// emissions is live, at the reference's level, under the reference's
    /// scope, with the reference's message text.
    #[test]
    fn all_fourteen_reference_diagnostics_are_live_at_the_reference_level_and_scope() {
        const SRC: &str = include_str!("locale.rs");
        let src = crate::log_capture::collapse_ws(SRC);

        assert_eq!(
            1,
            src.matches("const LOG_SCOPE: &str = \"os_locale\";")
                .count(),
            "the reference's scope tag is not declared exactly once"
        );

        for (level, message) in [
            // :31  log.info("setlocale from env result={s}", .{v});
            ("info", "setlocale from env result={}"),
            // :45  log.info("TODO: setlocale failed on Windows, implement better fallbacks", .{});
            (
                "info",
                "TODO: setlocale failed on Windows, implement better fallbacks",
            ),
            // :60  log.info("setlocale after unset lang result={s}", .{v});
            ("info", "setlocale after unset lang result={}"),
            // :71  log.warn("setlocale failed with LANG and system default. Falling back to en_US.UTF-8", .{});
            (
                "warn",
                "setlocale failed with LANG and system default. Falling back to en_US.UTF-8",
            ),
            // :74  log.info("setlocale default result={s}", .{v});
            ("info", "setlocale default result={}"),
            // :76  log.warn("setlocale failed even with the fallback, uncertain results", .{});
            (
                "warn",
                "setlocale failed even with the fallback, uncertain results",
            ),
            // :87  log.warn("NSLocale class not found. Locale may be incorrect.", .{});
            ("warn", "NSLocale class not found. Locale may be incorrect."),
            // :98  log.warn("languageCode or countryCode not found. Locale may be incorrect.", .{});
            (
                "warn",
                "languageCode or countryCode not found. Locale may be incorrect.",
            ),
            // :114 log.warn("error setting locale from system. err={}", .{err});
            ("warn", "error setting locale from system. err={err:?}"),
            // :117 log.info("detected system locale={s}", .{env_value});
            ("info", "detected system locale={}"),
            // :121 log.warn("error setting locale env var", .{});
            ("warn", "error setting locale env var"),
            // :134 log.warn("error getting preferred languages. err={}", .{err});
            ("warn", "error getting preferred languages. err={err:?}"),
            // :139 log.debug("setting LANGUAGE from preferred languages value={s}", .{pref});
            (
                "debug",
                "setting LANGUAGE from preferred languages value={}",
            ),
            // :188 log.warn("failed to convert a preferred language to UTF-8", .{});
            ("warn", "failed to convert a preferred language to UTF-8"),
        ] {
            let wanted = crate::log_capture::emission_fragment(level, message);
            assert_eq!(
                1,
                src.matches(&wanted).count(),
                "expected exactly one live emission `{wanted}`"
            );
        }
    }

    /// The `NSLocale class not found` guard is a REAL runtime lookup, not a
    /// tautology: assert the reference's own `objc.getClass("NSLocale")`
    /// resolves here, so the `is_none()` arm is a genuine failure arm rather
    /// than dead code. (A misspelt class name would make it fire on every
    /// launch; this is the control against that.)
    #[test]
    #[cfg(target_os = "macos")]
    fn the_nslocale_class_lookup_resolves() {
        assert!(objc2::runtime::AnyClass::get(c"NSLocale").is_some());
        assert!(objc2::runtime::AnyClass::get(c"NSLocaleTypoGuard").is_none());
    }
}
