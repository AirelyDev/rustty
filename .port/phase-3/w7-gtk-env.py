#!/usr/bin/env python3
"""P3-W7 T4 Step 2 — the GTK `setenv`/`getenv` arm.

`gtk/class/application.rs` mutates `GDK_DEBUG`/`GDK_DISABLE`/`LANG` before GTK
init. Those are GENUINE process-environment mutations with no replacement in
`os/` after `src/os/env.zig` was deleted, so they follow upstream's own
`locale.zig` precedent: file-local `extern "C"` declarations, plus a
`global.syncEnviron()` call to re-capture the block afterwards (upstream's
`global.init` does exactly that after `internal_os.ensureLocale()`).

`P3-W8` owns the GTK side proper; this is the env-deletion half only.
"""
import sys

P = "crates/rustty-apprt/src/gtk/class/application.rs"
src = open(P, encoding="utf-8").read()

DECLS = '''
// PORT NOTE (P3-W7 T4): `src/os/env.zig` was DELETED upstream (`e8525c0fd`) and
// with it `internal_os.setenv`/`getenv`. The GDK/LANG mutations below are
// GENUINE process-environment mutations with no replacement in `os/`, so they
// follow upstream's own `locale.zig` precedent: file-local `extern "c"`
// declarations. Every mutation is followed by `rustty_global::sync_environ()`,
// which is what upstream's `global.init` calls after `ensureLocale()` to
// re-capture the block.
//
// These run BEFORE `gtk_init` and before any surface exists, i.e. before any
// other thread can be reading the environment — which is the only reason a
// process-wide `setenv` is defensible here at all (see
// `.port/tools/gates/check_no_test_setenv.sh` for why it is banned in tests).
mod env_c {
    use std::ffi::{c_char, c_int};

    extern "C" {
        pub fn getenv(name: *const c_char) -> *mut c_char;
        #[cfg(not(windows))]
        pub fn setenv(name: *const c_char, value: *const c_char, overwrite: c_int) -> c_int;
        #[cfg(windows)]
        pub fn _putenv_s(varname: *const c_char, value_string: *const c_char) -> c_int;
    }
}

/// `posix.getenv(key)` — the raw bytes, or `None` when unset.
fn env_getenv(key: &std::ffi::CStr) -> Option<Vec<u8>> {
    // SAFETY: `key` is NUL-terminated; the returned pointer, when non-null,
    // points at a NUL-terminated string owned by the C runtime that stays valid
    // until the next environment mutation, and we copy it immediately.
    unsafe {
        let p = env_c::getenv(key.as_ptr());
        if p.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr(p).to_bytes().to_vec())
    }
}

/// `setenv(key, value, 1)` followed by `global.syncEnviron()`.
fn env_setenv(key: &std::ffi::CStr, value: &std::ffi::CStr) -> std::ffi::c_int {
    // SAFETY: both pointers are NUL-terminated C strings.
    let rc = unsafe {
        #[cfg(windows)]
        {
            env_c::_putenv_s(key.as_ptr(), value.as_ptr())
        }
        #[cfg(not(windows))]
        {
            env_c::setenv(key.as_ptr(), value.as_ptr(), 1)
        }
    };
    // We need to re-sync the environment after this completes.
    rustty_global::sync_environ();
    rc
}
'''

EDITS = [
    (
        """            rustty_os::env::setenv(c"GDK_DEBUG", &v);""",
        """            env_setenv(c"GDK_DEBUG", &v);""",
    ),
    (
        """            rustty_os::env::setenv(c"GDK_DISABLE", &v);""",
        """            env_setenv(c"GDK_DISABLE", &v);""",
    ),
    (
        """            let old_language = match rustty_os::env::getenv(b"LANG") {
                Ok(Some(result)) => {
                    let dup = CString::new(result.value.clone()).ok();
                    result.deinit();
                    dup
                }
                _ => None,
            };
            if let Some(language) = &config.language {
                if let Ok(lang) = CString::new(language.clone()) {
                    rustty_os::env::setenv(c"LANG", &lang);
                }
            }""",
        """            let old_language = env_getenv(c"LANG").and_then(|v| CString::new(v).ok());
            if let Some(language) = &config.language {
                if let Ok(lang) = CString::new(language.clone()) {
                    env_setenv(c"LANG", &lang);
                }
            }""",
    ),
]

for old, new in EDITS:
    if old not in src:
        print("MISS: " + old.splitlines()[0][:80], file=sys.stderr)
        sys.exit(1)
    src = src.replace(old, new, 1)

# append the file-local decls at the end of the file
src = src.rstrip() + "\n" + DECLS
open(P, "w", encoding="utf-8").write(src)
print("patched", P)
