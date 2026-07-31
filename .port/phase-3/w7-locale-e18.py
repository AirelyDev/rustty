#!/usr/bin/env python3
"""P3-W7 T3 Step 6 (area-E `E18`) — locale.zig's two new invariants:
the `assert(!builtin.is_test)` tripwire and the Windows early-return arm; plus
the switch from `internal_os.getenv` to a file-local `extern "C" getenv`,
following upstream's own `locale.zig` precedent of declaring file-local
`setenv`/`unsetenv` after `os/env.zig` was deleted."""
import sys

P = "crates/rustty-os/src/locale.rs"
src = open(P, encoding="utf-8").read()

EDITS = [
    (
        """use crate::env as internal_os;
""",
        """// PORT NOTE (P3-W7 T3 / area-E `E18`): upstream's `locale.zig` stopped
// importing `os/env.zig` (which it deleted) and declared its own file-local
// `extern "c" fn setenv/unsetenv` instead. rustty follows that precedent for
// the whole trio (`getenv` included) — see the `c` module at the bottom of this
// file. The old `use crate::env as internal_os;` is gone.
""",
    ),
    (
        """pub fn ensureLocale() -> std::io::Result<()> {
    // PORT NOTE: `assert(builtin.link_libc)`. rustty-os links libc unconditionally
    // (R3), so this asserts a constant truth, matching the reference's gate.
    assert(true);
""",
        """pub fn ensureLocale() -> std::io::Result<()> {
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
""",
    ),
    (
        """    let lang = internal_os::getenv(b"LANG").unwrap();""",
        """    let lang = c_getenv(c"LANG");""",
    ),
    (
        """        if lang.is_none() || lang.as_ref().unwrap().value.is_empty() {""",
        """        if lang.is_none() || lang.as_ref().unwrap().is_empty() {""",
    ),
    (
        """    // Set the locale to whatever is set in env vars.
    if let Some(v) = setlocale(LC_ALL, c"") {
        log_info_setlocale_env(v);
        return Ok(());
    }
""",
        """    // Set the locale to whatever is set in env vars.
    if let Some(v) = setlocale(LC_ALL, c"") {
        log_info_setlocale_env(v);
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
        log_info_setlocale_windows_todo();
        return Ok(());
    }
""",
    ),
    (
        """    if let Some(old_lang) = internal_os::getenv(b"LANG").unwrap() {
        if !old_lang.value.is_empty() {
            // We don't need to do both of these things but we do them
            // both to be sure that lang is either empty or unset completely.
            let _ = internal_os::setenv(c"LANG", c"");
            let _ = internal_os::unsetenv(c"LANG");""",
        """    #[allow(unreachable_code)]
    if let Some(old_lang) = c_getenv(c"LANG") {
        if !old_lang.is_empty() {
            // We don't need to do both of these things but we do them
            // both to be sure that lang is either empty or unset completely.
            let _ = c_setenv(c"LANG", c"", 1);
            let _ = c_unsetenv(c"LANG");""",
    ),
    (
        """        let _ = internal_os::setenv(c"LANG", c"en_US.UTF-8");""",
        """        let _ = c_setenv(c"LANG", c"en_US.UTF-8", 1);""",
    ),
    (
        """fn log_warn_fallback_failed() {
    // log.warn("setlocale failed even with the fallback, uncertain results", .{});
}""",
        """fn log_warn_fallback_failed() {
    // log.warn("setlocale failed even with the fallback, uncertain results", .{});
}
#[cfg_attr(not(windows), allow(dead_code))]
fn log_info_setlocale_windows_todo() {
    // log.info("TODO: setlocale failed on Windows, implement better fallbacks", .{});
}

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
}""",
    ),
]

for old, new in EDITS:
    if old not in src:
        print("MISS: " + old.splitlines()[0][:80], file=sys.stderr)
        sys.exit(1)
    src = src.replace(old, new, 1)

open(P, "w", encoding="utf-8").write(src)
print("patched", P)
