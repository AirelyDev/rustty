//! rustty-os::homedir
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/homedir.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `passwd = @import("passwd.zig")`,
// `posix = std.posix`, and `objc`. In Rust we use the ported `passwd` sibling
// module, the standard library, and `objc2`/`objc2-foundation` (R3: bindings to
// the SAME Objective-C runtime/Foundation framework the reference uses).

#[cfg(not(target_os = "macos"))]
use crate::passwd;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The buffer used for output is not large enough to store the value.
    BufferTooSmall,
}

/// Determine the home directory for the currently executing user. This
/// is generally an expensive process so the value should be cached.
//
// PORT NOTE (P3-W7 T3 / area-E `E15`): upstream gained an explicit
// `environ_map: *const std.process.Environ.Map` first parameter so `HOME` (and
// `HOMEDRIVE`/`HOMEPATH` on Windows) is read from a caller-supplied map rather
// than the process environment. See `desktop.rs` for why that matters more in
// Rust than in Zig.
#[inline]
pub fn home<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        homeUnix(environ_map, buf)
    }

    #[cfg(windows)]
    {
        homeWindows(environ_map, buf)
    }

    // iOS doesn't have a user-writable home directory
    #[cfg(target_os = "ios")]
    {
        let _ = (environ_map, buf);
        Ok(None)
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "macos",
        windows,
        target_os = "ios"
    )))]
    {
        compile_error!("unimplemented")
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
#[allow(non_snake_case)]
// PARITY: the reference's `homeUnix` is a sequence of early `return`s; on macOS
// the last reachable `return` becomes a tail expression after the non-macOS
// allocation path is `#[cfg]`-excluded, which clippy flags. The explicit
// `return` is kept to mirror the reference control flow verbatim.
#[allow(clippy::needless_return)]
fn homeUnix<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {
    // First: if we have a HOME env var, then we use that.
    if let Some(result) = environ_map.get(&b"HOME"[..]) {
        if buf.len() < result.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[0..result.len()].copy_from_slice(result);
        let n = result.len();
        return Ok(Some(&buf[0..n]));
    }

    // On macOS: [NSFileManager defaultManager].homeDirectoryForCurrentUser.path
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::NSFileManager;

        let manager = NSFileManager::defaultManager();
        let home_url = manager.homeDirectoryForCurrentUser();
        let home_path = home_url.path();

        let result_string = home_path.expect("homeDirectoryForCurrentUser path");
        let c_str = result_string.UTF8String();
        let result = unsafe { std::ffi::CStr::from_ptr(c_str) }.to_bytes();

        if buf.len() < result.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[0..result.len()].copy_from_slice(result);
        let n = result.len();
        return Ok(Some(&buf[0..n]));
    }

    // Everything below here will require some allocation
    // PORT NOTE: the reference uses a `FixedBufferAllocator` over a 1024-byte stack
    // buffer. In Rust `passwd.get`/`Command` allocate via the global allocator;
    // no separate fixed buffer is needed to preserve behavior.

    // We try passwd. This doesn't work on multi-user mac but we try it anyways.
    #[cfg(not(target_os = "macos"))]
    {
        let pw = passwd::get().map_err(|_| Error::BufferTooSmall)?;
        if let Some(result) = pw.home {
            let result = result.as_bytes();
            if buf.len() < result.len() {
                return Err(Error::BufferTooSmall);
            }
            buf[0..result.len()].copy_from_slice(result);
            let n = result.len();
            return Ok(Some(&buf[0..n]));
        }

        // If all else fails, have the shell tell us...
        let run = std::process::Command::new("/bin/sh")
            .args(["-c", "cd && pwd"])
            .output();

        if let Ok(run) = run {
            if run.status.success() {
                let result = trimSpace(&run.stdout);
                if buf.len() < result.len() {
                    return Err(Error::BufferTooSmall);
                }
                buf[0..result.len()].copy_from_slice(result);
                let n = result.len();
                return Ok(Some(&buf[0..n]));
            }
        }

        Ok(None)
    }
}

#[cfg(windows)]
#[allow(non_snake_case)]
fn homeWindows<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {
    // PORT NOTE: the reference uses a `std.heap.FixedBufferAllocator.init(buf)` and
    // allocates `HOMEDRIVE` (then `HOMEPATH`) into it via
    // `std.process.getEnvVarOwned`, so the env var bytes are written *in place*
    // starting at `buf.ptr`. `getEnvVarOwned` maps `error.OutOfMemory` (the FBA
    // ran out of room) to `Error.BufferTooSmall`, and
    // `error.InvalidWtf8`/`error.EnvironmentVariableNotFound` to `null`. We
    // reproduce that in-place fill with `fixed_buffer_env` below, which writes the
    // var into the head of the given buffer slice and returns the written
    // sub-slice (whose `.as_ptr()` equals the slice's `.as_ptr()`), preserving the
    // reference's pointer invariant for the `@panic("codebug")` guards.
    let buf_ptr = buf.as_ptr();

    let drive_len = {
        let drive = match fixed_buffer_env(environ_map, buf, b"HOMEDRIVE") {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(_) => return Err(Error::BufferTooSmall),
        };
        // could shift the contents if this ever happens
        if drive.as_ptr() != buf_ptr {
            panic!("codebug");
        }
        drive.len()
    };

    let path_len = {
        let path_buf_ptr = buf[drive_len..].as_ptr();
        let homepath = match fixed_buffer_env(environ_map, &mut buf[drive_len..], b"HOMEPATH") {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(_) => return Err(Error::BufferTooSmall),
        };
        // could shift the contents if this ever happens
        if homepath.as_ptr() != path_buf_ptr {
            panic!("codebug");
        }
        homepath.len()
    };

    Ok(Some(&buf[0..drive_len + path_len]))
}

// PORT NOTE: the equivalent of `std.process.getEnvVarOwned(fba, key)` where `fba` is a
// `FixedBufferAllocator` over `dst`: writes the value of the environment variable
// `key` into the head of `dst` in place, returning the written sub-slice. Returns
// `Ok(None)` for a missing variable (`error.EnvironmentVariableNotFound`) and
// `Err(())` when the value does not fit in `dst` (`error.OutOfMemory`). The
// reference also folds `error.InvalidWtf8` into `null`; Windows env values are
// WTF-16 and `var_os` always yields an `OsString`, so the invalid-WTF8 case maps
// to a successful read here.
#[cfg(windows)]
fn fixed_buffer_env<'a>(
    environ_map: &rustty_global::EnvMap,
    dst: &'a mut [u8],
    key: &[u8],
) -> Result<Option<&'a [u8]>, ()> {
    let bytes: &[u8] = match environ_map.get(key) {
        Some(v) => v,
        None => return Ok(None),
    };
    if dst.len() < bytes.len() {
        return Err(());
    }
    dst[0..bytes.len()].copy_from_slice(bytes);
    Ok(Some(&dst[0..bytes.len()]))
}

#[cfg(not(target_os = "macos"))]
#[allow(non_snake_case)]
fn trimSpace(input: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = input.len();
    let is_trim = |c: u8| c == b' ' || c == b'\n' || c == b'\t';
    while start < end && is_trim(input[start]) {
        start += 1;
    }
    while end > start && is_trim(input[end - 1]) {
        end -= 1;
    }
    &input[start..end]
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExpandError {
    HomeDetectionFailed,
    BufferTooSmall,
}

/// Expands a path that starts with a tilde (~) to the home directory of
/// the current user.
///
/// Errors if `home` fails or if the size of the expanded path is larger
/// than `buf.len`.
#[allow(non_snake_case)]
pub fn expandHome<'a>(
    environ_map: &rustty_global::EnvMap,
    path: &'a [u8],
    buf: &'a mut [u8],
) -> Result<&'a [u8], ExpandError> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        expandHomeUnix(environ_map, path, buf)
    }

    // `~/` is not an idiom generally used on Windows
    #[cfg(windows)]
    {
        let _ = (environ_map, buf);
        Ok(path)
    }

    // iOS doesn't have a user-writable home directory
    #[cfg(target_os = "ios")]
    {
        let _ = (environ_map, buf);
        Ok(path)
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "macos",
        windows,
        target_os = "ios"
    )))]
    {
        compile_error!("unimplemented")
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
#[allow(non_snake_case)]
fn expandHomeUnix<'a>(
    environ_map: &rustty_global::EnvMap,
    path: &'a [u8],
    buf: &'a mut [u8],
) -> Result<&'a [u8], ExpandError> {
    if !path.starts_with(b"~/") {
        return Ok(path);
    }
    // PORT NOTE: the reference does `const home_dir = if (home(buf)) |home_| home_
    // orelse return error.HomeDetectionFailed else |_| return
    // error.HomeDetectionFailed;`. `home` writes into `buf`, so we compute the
    // home length first, then operate on `buf` directly.
    let home_dir_len = match home(environ_map, buf) {
        Ok(Some(home_)) => home_.len(),
        Ok(None) => return Err(ExpandError::HomeDetectionFailed),
        Err(_) => return Err(ExpandError::HomeDetectionFailed),
    };
    let rest = &path[1..]; // Skip the ~
    let expanded_len = home_dir_len + rest.len();

    if expanded_len > buf.len() {
        return Err(ExpandError::BufferTooSmall);
    }
    // PORT NOTE: `@memcpy(buf[home_dir.len..expanded_len], rest);` — copy `rest`
    // after the home dir already written into `buf` by `home`. We copy from a
    // local snapshot of `rest` to avoid aliasing `buf` with itself.
    let rest_owned = rest.to_vec();
    buf[home_dir_len..expanded_len].copy_from_slice(&rest_owned);

    Ok(&buf[0..expanded_len])
}

// PORT NOTE (P3-W7 T3): the `posix_getenv` shim is GONE — `HOME` is read from
// the caller-supplied `EnvMap` now, matching upstream after `e8525c0fd`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // env/passwd/objc home lookup hits real OS state
    #[allow(non_snake_case)]
    fn expand_home_unix() {
        if cfg!(windows) {
            return; // error.SkipZigTest
        }

        // PORT NOTE (P3-W7 T3): upstream's test now builds the map from
        // `testing.environ`; rustty's equivalent is the captured process
        // environment, and the `crate::env_test_lock()` serialization is gone
        // because no test mutates the process environment any more.
        let environ_map = rustty_global::environ_map();
        let mut buf = [0u8; libc::PATH_MAX as usize];
        let home_dir = expandHomeUnix(&environ_map, b"~/", &mut buf)
            .unwrap()
            .to_vec();
        // Joining the home directory `~` with the path `/`
        // the result should end with a separator here. (e.g. `/home/user/`)
        assert!(home_dir[home_dir.len() - 1] == SEP);

        let downloads = expandHomeUnix(&environ_map, b"~/Downloads/shader.glsl", &mut buf)
            .unwrap()
            .to_vec();
        let mut expected_downloads = home_dir.clone();
        expected_downloads.extend_from_slice(b"Downloads/shader.glsl");
        assert_eq!(expected_downloads, downloads);

        assert_eq!(
            b"~".as_slice(),
            expandHomeUnix(&environ_map, b"~", &mut buf).unwrap()
        );
        assert_eq!(
            b"~abc/".as_slice(),
            expandHomeUnix(&environ_map, b"~abc/", &mut buf).unwrap()
        );
        assert_eq!(
            b"/home/user".as_slice(),
            expandHomeUnix(&environ_map, b"/home/user", &mut buf).unwrap()
        );
        assert_eq!(
            b"".as_slice(),
            expandHomeUnix(&environ_map, b"", &mut buf).unwrap()
        );

        // Expect an error if the buffer is large enough to hold the home directory,
        // but not the expanded path
        let mut small_buf = vec![0u8; home_dir.len()];
        assert_eq!(
            Err(ExpandError::BufferTooSmall),
            expandHomeUnix(&environ_map, b"~/Downloads", &mut small_buf[..])
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)] // env/passwd/objc home lookup hits real OS state
    fn home_returns_value() {
        let environ_map = rustty_global::environ_map();
        let mut buf = [0u8; 1024];
        let result = home(&environ_map, &mut buf).unwrap();
        assert!(result.is_some());
        assert!(!result.unwrap().is_empty());
    }

    // `std.fs.path.sep`.
    #[cfg(windows)]
    const SEP: u8 = b'\\';
    #[cfg(not(windows))]
    const SEP: u8 = b'/';
}
