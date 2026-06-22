//! rustty-os::homedir
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/homedir.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT: the Zig reference imports `std`, `builtin`, `passwd = @import("passwd.zig")`,
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
#[inline]
pub fn home(buf: &mut [u8]) -> Result<Option<&[u8]>, Error> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        homeUnix(buf)
    }

    #[cfg(windows)]
    {
        homeWindows(buf)
    }

    // iOS doesn't have a user-writable home directory
    #[cfg(target_os = "ios")]
    {
        let _ = buf;
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
fn homeUnix(buf: &mut [u8]) -> Result<Option<&[u8]>, Error> {
    // First: if we have a HOME env var, then we use that.
    if let Some(result) = posix_getenv("HOME") {
        if buf.len() < result.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[0..result.len()].copy_from_slice(&result);
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
    // PORT: the reference uses a `FixedBufferAllocator` over a 1024-byte stack
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
fn homeWindows(buf: &mut [u8]) -> Result<Option<&[u8]>, Error> {
    let drive_len = {
        let drive = match std::env::var("HOMEDRIVE") {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        let bytes = drive.as_bytes();
        if buf.len() < bytes.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[0..bytes.len()].copy_from_slice(bytes);
        bytes.len()
    };

    let path_len = {
        let homepath = match std::env::var("HOMEPATH") {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        let bytes = homepath.as_bytes();
        if buf[drive_len..].len() < bytes.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[drive_len..drive_len + bytes.len()].copy_from_slice(bytes);
        bytes.len()
    };

    Ok(Some(&buf[0..drive_len + path_len]))
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
pub fn expandHome<'a>(path: &'a [u8], buf: &'a mut [u8]) -> Result<&'a [u8], ExpandError> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        expandHomeUnix(path, buf)
    }

    // `~/` is not an idiom generally used on Windows
    #[cfg(windows)]
    {
        let _ = buf;
        Ok(path)
    }

    // iOS doesn't have a user-writable home directory
    #[cfg(target_os = "ios")]
    {
        let _ = buf;
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
fn expandHomeUnix<'a>(path: &'a [u8], buf: &'a mut [u8]) -> Result<&'a [u8], ExpandError> {
    if !path.starts_with(b"~/") {
        return Ok(path);
    }
    // PORT: the reference does `const home_dir = if (home(buf)) |home_| home_
    // orelse return error.HomeDetectionFailed else |_| return
    // error.HomeDetectionFailed;`. `home` writes into `buf`, so we compute the
    // home length first, then operate on `buf` directly.
    let home_dir_len = match home(buf) {
        Ok(Some(home_)) => home_.len(),
        Ok(None) => return Err(ExpandError::HomeDetectionFailed),
        Err(_) => return Err(ExpandError::HomeDetectionFailed),
    };
    let rest = &path[1..]; // Skip the ~
    let expanded_len = home_dir_len + rest.len();

    if expanded_len > buf.len() {
        return Err(ExpandError::BufferTooSmall);
    }
    // PORT: `@memcpy(buf[home_dir.len..expanded_len], rest);` — copy `rest`
    // after the home dir already written into `buf` by `home`. We copy from a
    // local snapshot of `rest` to avoid aliasing `buf` with itself.
    let rest_owned = rest.to_vec();
    buf[home_dir_len..expanded_len].copy_from_slice(&rest_owned);

    Ok(&buf[0..expanded_len])
}

// `posix.getenv`: returns the value bytes (no allocation) or `None`.
#[cfg(not(windows))]
fn posix_getenv(key: &str) -> Option<Vec<u8>> {
    std::env::var_os(key).map(|v| {
        use std::os::unix::ffi::OsStrExt;
        v.as_os_str().as_bytes().to_vec()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // env/passwd/objc home lookup hits real OS state
    #[allow(non_snake_case)]
    fn expandHomeUnix_() {
        if cfg!(windows) {
            return; // error.SkipZigTest
        }

        let mut buf = [0u8; libc::PATH_MAX as usize];
        let home_dir = expandHomeUnix(b"~/", &mut buf).unwrap().to_vec();
        // Joining the home directory `~` with the path `/`
        // the result should end with a separator here. (e.g. `/home/user/`)
        assert!(home_dir[home_dir.len() - 1] == SEP);

        let downloads = expandHomeUnix(b"~/Downloads/shader.glsl", &mut buf)
            .unwrap()
            .to_vec();
        let mut expected_downloads = home_dir.clone();
        expected_downloads.extend_from_slice(b"Downloads/shader.glsl");
        assert_eq!(expected_downloads, downloads);

        assert_eq!(b"~".as_slice(), expandHomeUnix(b"~", &mut buf).unwrap());
        assert_eq!(
            b"~abc/".as_slice(),
            expandHomeUnix(b"~abc/", &mut buf).unwrap()
        );
        assert_eq!(
            b"/home/user".as_slice(),
            expandHomeUnix(b"/home/user", &mut buf).unwrap()
        );
        assert_eq!(b"".as_slice(), expandHomeUnix(b"", &mut buf).unwrap());

        // Expect an error if the buffer is large enough to hold the home directory,
        // but not the expanded path
        let mut small_buf = vec![0u8; home_dir.len()];
        assert_eq!(
            Err(ExpandError::BufferTooSmall),
            expandHomeUnix(b"~/Downloads", &mut small_buf[..])
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)] // env/passwd/objc home lookup hits real OS state
    fn home_returns_value() {
        let mut buf = [0u8; 1024];
        let result = home(&mut buf).unwrap();
        assert!(result.is_some());
        assert!(!result.unwrap().is_empty());
    }

    // `std.fs.path.sep`.
    #[cfg(windows)]
    const SEP: u8 = b'\\';
    #[cfg(not(windows))]
    const SEP: u8 = b'/';
}
