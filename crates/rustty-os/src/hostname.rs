//! rustty-os::hostname
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/hostname.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT: the Zig reference imports `std`, `builtin`, and `posix = std.posix`. In
// Rust we use `libc` (R3: a binding to the SAME system library) for the POSIX
// `gethostname` syscall and constants used by `isLocal`.

#[derive(Debug, PartialEq, Eq)]
pub enum LocalHostnameValidationError {
    PermissionDenied,
    Unexpected,
}

// PORT: `posix.HOST_NAME_MAX` — Zig's per-OS value (lib/std/c.zig). macOS/iOS
// family = 72; Linux = 64 (`linux.HOST_NAME_MAX`); the BSD family = 255. We
// reproduce the exact value for the build target so the buffer matches.
#[cfg(not(windows))]
const HOST_NAME_MAX: usize = {
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "visionos"
    ))]
    {
        72
    }
    #[cfg(target_os = "linux")]
    {
        64
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "visionos",
        target_os = "linux"
    )))]
    {
        255
    }
};

/// Validates a hostname according to [RFC 1123](https://www.rfc-editor.org/rfc/rfc1123)
///
/// std.net.isValidHostname is (currently) too generous. It considers strings like
/// ".example.com", "exa..mple.com", and "-example.com" to be valid hostnames, which
/// is incorrect.
#[allow(non_snake_case)]
pub fn isValid(hostname: &[u8]) -> bool {
    if hostname.is_empty() {
        return false;
    }
    if hostname[0] == b'.' {
        return false;
    }

    // Ignore trailing dot (FQDN). It doesn't count toward our length.
    let end = if hostname[hostname.len() - 1] == b'.' {
        if hostname.len() == 1 {
            return false;
        }
        hostname.len() - 1
    } else {
        hostname.len()
    };

    if end > 253 {
        return false;
    }

    // Hostnames are divided into dot-separated "labels", which:
    //
    // - Start with a letter or digit
    // - Can contain letters, digits, or hyphens
    // - Must end with a letter or digit
    // - Have a minimum of 1 character and a maximum of 63
    let mut label_start: usize = 0;
    let mut label_len: usize = 0;
    for (i, &c) in hostname[0..end].iter().enumerate() {
        match c {
            b'.' => {
                if label_len == 0 || label_len > 63 {
                    return false;
                }
                if !hostname[label_start].is_ascii_alphanumeric() {
                    return false;
                }
                if !hostname[i - 1].is_ascii_alphanumeric() {
                    return false;
                }

                label_start = i + 1;
                label_len = 0;
            }
            b'-' => {
                label_len += 1;
            }
            _ => {
                if !c.is_ascii_alphanumeric() {
                    return false;
                }
                label_len += 1;
            }
        }
    }

    // Validate the final label
    if label_len == 0 || label_len > 63 {
        return false;
    }
    if !hostname[label_start].is_ascii_alphanumeric() {
        return false;
    }
    if !hostname[end - 1].is_ascii_alphanumeric() {
        return false;
    }

    true
}

/// Checks if a hostname is local to the current machine. This matches
/// both "localhost" and the current hostname of the machine (as returned
/// by `gethostname`).
#[allow(non_snake_case)]
pub fn isLocal(hostname: &[u8]) -> Result<bool, LocalHostnameValidationError> {
    // A 'localhost' hostname is always considered local.
    if hostname == b"localhost" {
        return Ok(true);
    }

    // If hostname is not "localhost" it must match our hostname.
    #[cfg(windows)]
    {
        // PORT: the Windows branch uses `windows.exp.kernel32.GetComputerNameA`.
        // It is reproduced faithfully against the Win32 API binding (R3).
        let mut buf = [0u8; 256];
        let mut n_size: u32 = buf.len() as u32;
        if unsafe { windows::GetComputerNameA(buf.as_mut_ptr(), &mut n_size) } == 0 {
            return Ok(false);
        }
        let our_hostname = &buf[0..n_size as usize];
        Ok(hostname == our_hostname)
    }
    #[cfg(not(windows))]
    {
        // posix.HOST_NAME_MAX
        let mut buf = [0u8; HOST_NAME_MAX];
        let our_hostname = gethostname(&mut buf)?;
        Ok(hostname == our_hostname)
    }
}

// PORT: `posix.gethostname(&buf)` — fills `buf` with the host name and returns
// the resulting slice. Reproduced over the POSIX `gethostname` binding (R3); on
// error it maps to the reference's `LocalHostnameValidationError` set.
#[cfg(not(windows))]
fn gethostname(buf: &mut [u8]) -> Result<&[u8], LocalHostnameValidationError> {
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if rc != 0 {
        let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(match errno {
            libc::EPERM | libc::EACCES => LocalHostnameValidationError::PermissionDenied,
            _ => LocalHostnameValidationError::Unexpected,
        });
    }
    // Find the NUL terminator (gethostname NUL-terminates within HOST_NAME_MAX).
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Ok(&buf[0..len])
}

#[cfg(windows)]
mod windows {
    use std::ffi::c_int;
    extern "system" {
        // BOOL GetComputerNameA(LPSTR lpBuffer, LPDWORD nSize)
        pub fn GetComputerNameA(lp_buffer: *mut u8, n_size: *mut u32) -> c_int;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT helper: build the repeated-string slices used by the `isValid` test.
    // Zig's `"a" ** 63` / `"a." ** 126 ++ "a"` are reproduced as runtime byte
    // vectors with identical contents (an independent construction, not derived
    // from `isValid` itself — preserving the test's discriminating power).
    fn rep(s: &str, n: usize) -> Vec<u8> {
        s.repeat(n).into_bytes()
    }
    fn cat(mut a: Vec<u8>, b: &[u8]) -> Vec<u8> {
        a.extend_from_slice(b);
        a
    }

    #[test]
    #[allow(non_snake_case)]
    fn is_valid() {
        // Valid hostnames
        assert!(isValid(b"example"));
        assert!(isValid(b"example.com"));
        assert!(isValid(b"www.example.com"));
        assert!(isValid(b"sub.domain.example.com"));
        assert!(isValid(b"example.com."));
        assert!(isValid(b"host-name.example.com."));
        assert!(isValid(b"123.example.com."));
        assert!(isValid(b"a-b.com"));
        assert!(isValid(b"a.b.c.d.e.f.g"));
        assert!(isValid(b"127.0.0.1")); // Also a valid hostname
        assert!(isValid(&cat(rep("a", 63), b".com"))); // Label exactly 63 chars (valid)
        assert!(isValid(&cat(rep("a.", 126), b"a"))); // Total length 253 (valid)

        // Invalid hostnames
        assert!(!isValid(b""));
        assert!(!isValid(b".example.com"));
        assert!(!isValid(b"example.com.."));
        assert!(!isValid(b"host..domain"));
        assert!(!isValid(b"-hostname"));
        assert!(!isValid(b"hostname-"));
        assert!(!isValid(b"a.-.b"));
        assert!(!isValid(b"host_name.com"));
        assert!(!isValid(b"."));
        assert!(!isValid(b".."));
        assert!(!isValid(&cat(rep("a", 64), b".com"))); // Label length 64 (too long)
        assert!(!isValid(&cat(rep("a.", 126), b"ab"))); // Total length 254 (too long)
    }

    #[test]
    fn islocal_returns_true_when_provided_hostname_is_localhost() {
        assert!(isLocal(b"localhost").unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS gethostname syscall
    fn islocal_returns_true_when_hostname_is_local() {
        #[cfg(windows)]
        {
            let mut buf = [0u8; 256];
            let mut n_size: u32 = buf.len() as u32;
            if unsafe { windows::GetComputerNameA(buf.as_mut_ptr(), &mut n_size) } == 0 {
                panic!("GetComputerNameFailed");
            }
            let local_hostname = &buf[0..n_size as usize];
            assert!(isLocal(local_hostname).unwrap());
        }
        #[cfg(not(windows))]
        {
            let mut buf = [0u8; HOST_NAME_MAX];
            let local_hostname = gethostname(&mut buf).unwrap();
            assert!(isLocal(local_hostname).unwrap());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS gethostname syscall
    #[allow(clippy::bool_assert_comparison)]
    fn islocal_returns_false_when_hostname_is_not_local() {
        assert_eq!(false, isLocal(b"not-the-local-hostname").unwrap());
    }
}
