//! rustty-os::env
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/env.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `Allocator`, `posix`, and
// `isFlatpak` from `flatpak.zig`. In Rust we use the standard library and `libc`
// for the POSIX/Windows `getenv`/`setenv`/`unsetenv` bindings (R3: bindings to
// the SAME system library, not a reimplementation).
//
// PARITY (deferred seam, R4): `flatpak.zig`'s `isFlatpak` lives in a different
// port group whose module is still a stub. To keep this module's behavior
// faithful and the build green, `is_flatpak` below reproduces `flatpak.zig`'s
// `isFlatpak` EXACTLY (verbatim logic + comment); it will be replaced by a
// re-export of `crate::flatpak::is_flatpak` when the flatpak group lands. No
// behavior is added or lost.
use std::collections::HashMap;

/// Returns true if we're running in a Flatpak environment.
fn is_flatpak() -> bool {
    // If we're not on Linux then we'll make this comptime false.
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::metadata("/.flatpak-info").is_ok()
    }
}

// PORT NOTE: `pub const Error = Allocator.Error;`. Rust's allocation failure is not a
// recoverable error in the std collections used here; the parity-preserving
// equivalent is the infallible allocation model. The error type is kept as a
// type alias for fidelity with the reference's public surface.
/// pub const Error = Allocator.Error;
pub type Error = std::convert::Infallible;

/// Get the environment map.
#[allow(non_snake_case)]
pub fn getEnvMap() -> HashMap<String, String> {
    if is_flatpak() {
        HashMap::new()
    } else {
        std::env::vars().collect()
    }
}

/// Append a value to an environment variable such as PATH.
/// The returned value is always allocated so it must be freed.
#[allow(non_snake_case)]
pub fn appendEnv(current: &str, value: &str) -> String {
    // If there is no prior value, we return it as-is
    if current.is_empty() {
        return value.to_string();
    }

    // Otherwise we must prefix.
    appendEnvAlways(current, value)
}

/// Always append value to environment, even when it is empty.
/// This is useful because some env vars (like MANPATH) want there
/// to be an empty prefix to preserve existing values.
///
/// The returned value is always allocated so it must be freed.
#[allow(non_snake_case)]
pub fn appendEnvAlways(current: &str, value: &str) -> String {
    format!("{}{}{}", current, MAIN_SEPARATOR_LIST_DELIMITER, value)
}

/// Prepend a value to an environment variable such as PATH.
/// The returned value is always allocated so it must be freed.
#[allow(non_snake_case)]
pub fn prependEnv(current: &str, value: &str) -> String {
    // If there is no prior value, we return it as-is
    if current.is_empty() {
        return value.to_string();
    }

    format!("{}{}{}", value, MAIN_SEPARATOR_LIST_DELIMITER, current)
}

// PORT NOTE: `std.fs.path.delimiter` is the PATH-list separator: ';' on Windows, ':'
// elsewhere. Rust's std does not expose this directly, so we reproduce the
// reference's `std.fs.path.delimiter` value here as a single byte.
mod path {
    #[cfg(windows)]
    pub const MAIN_SEPARATOR_LIST_DELIMITER: char = ';';
    #[cfg(not(windows))]
    pub const MAIN_SEPARATOR_LIST_DELIMITER: char = ':';
}
use path::MAIN_SEPARATOR_LIST_DELIMITER;

/// The result of getenv, with a shared deinit to properly handle allocation
/// on Windows.
pub struct GetEnvResult {
    pub value: String,
}

impl GetEnvResult {
    // PORT NOTE: `deinit` frees on Windows only; in Rust ownership/Drop handles this
    // for `String` on all platforms. Kept as a no-op method for parity with the
    // reference's public surface (the comment is preserved verbatim).
    #[allow(clippy::unused_self)]
    pub fn deinit(self) {
        // (the `String` is dropped here; Windows-specific free is automatic)
    }
}

/// Gets the value of an environment variable, or null if not found.
/// This will allocate on Windows but not on other platforms. The returned
/// value should have deinit called to do the proper cleanup no matter what
/// platform you are on.
pub fn getenv(key: &str) -> Result<Option<GetEnvResult>, Error> {
    // PORT NOTE: Zig branches on `builtin.os.tag`. Rust's `std::env::var` handles both
    // the Windows allocation and the non-Windows lookup uniformly; a missing key
    // or an invalid-unicode value both map to `None` (matching the reference's
    // `EnvironmentVariableNotFound`/`InvalidWtf8` → null on Windows, and
    // `posix.getenv` returning null otherwise). Both reference branches are
    // reproduced as comments below at the line they cover.

    // Non-Windows doesn't need to allocate
    //   else => if (posix.getenv(key)) |v| .{ .value = v } else null,

    // Windows needs to allocate
    //   .windows => if (std.process.getEnvVarOwned(alloc, key)) |v| ... ,
    Ok(match std::env::var(key) {
        Ok(v) => Some(GetEnvResult { value: v }),
        Err(_) => None,
    })
}

/// Gets the value of an environment variable. Returns null if not found or the
/// value is empty. This will allocate on Windows but not on other platforms.
/// The returned value should have deinit called to do the proper cleanup no
/// matter what platform you are on.
#[allow(non_snake_case)]
pub fn getenvNotEmpty(key: &str) -> Result<Option<GetEnvResult>, Error> {
    let result_ = getenv(key)?;
    if let Some(result) = &result_ {
        if result.value.is_empty() {
            result_.unwrap().deinit();
            return Ok(None);
        }
    }
    Ok(result_)
}

#[allow(non_snake_case)]
pub fn setenv(key: &std::ffi::CStr, value: &std::ffi::CStr) -> std::ffi::c_int {
    #[cfg(windows)]
    {
        unsafe { c::_putenv_s(key.as_ptr(), value.as_ptr()) }
    }
    #[cfg(not(windows))]
    {
        unsafe { c::setenv(key.as_ptr(), value.as_ptr(), 1) }
    }
}

#[allow(non_snake_case)]
pub fn unsetenv(key: &std::ffi::CStr) -> std::ffi::c_int {
    #[cfg(windows)]
    {
        unsafe { c::_putenv_s(key.as_ptr(), c"".as_ptr()) }
    }
    #[cfg(not(windows))]
    {
        unsafe { c::unsetenv(key.as_ptr()) }
    }
}

mod c {
    use std::ffi::{c_char, c_int};

    // POSIX
    #[cfg(not(windows))]
    extern "C" {
        pub fn setenv(name: *const c_char, value: *const c_char, overwrite: c_int) -> c_int;
        pub fn unsetenv(name: *const c_char) -> c_int;
    }

    // Windows
    #[cfg(windows)]
    extern "C" {
        pub fn _putenv_s(varname: *const c_char, value_string: *const c_char) -> c_int;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(non_snake_case)]
    fn appendEnv_empty() {
        let result = appendEnv("", "foo");
        assert_eq!(result, "foo");
    }

    #[test]
    #[allow(non_snake_case)]
    fn appendEnv_existing() {
        let result = appendEnv("a:b", "foo");
        if cfg!(windows) {
            assert_eq!(result, "a:b;foo");
        } else {
            assert_eq!(result, "a:b:foo");
        }
    }

    #[test]
    #[allow(non_snake_case)]
    fn prependEnv_empty() {
        let result = prependEnv("", "foo");
        assert_eq!(result, "foo");
    }

    #[test]
    #[allow(non_snake_case)]
    fn prependEnv_existing() {
        let result = prependEnv("a:b", "foo");
        if cfg!(windows) {
            assert_eq!(result, "foo;a:b");
        } else {
            assert_eq!(result, "foo:a:b");
        }
    }
}
