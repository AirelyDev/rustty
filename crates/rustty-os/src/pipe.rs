//! rustty-os::pipe
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/pipe.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `windows = @import("windows.zig")`,
// and `posix = std.posix`. In Rust we use `libc` (R3: a binding to the SAME
// system library `std.posix` targets) for the POSIX `pipe2` syscall and the
// `windows` module's `CreatePipe`/error helpers for the Windows path. `fd_t`
// is the platform file-descriptor type (`c_int` on POSIX, `HANDLE` on Windows).

#[cfg(not(windows))]
#[allow(non_camel_case_types)]
pub type fd_t = libc::c_int;
#[cfg(windows)]
#[allow(non_camel_case_types)]
pub type fd_t = crate::windows::HANDLE;

/// pipe() that works on Windows and POSIX. For POSIX systems, this sets
/// CLOEXEC on the file descriptors.
pub fn pipe() -> std::io::Result<[fd_t; 2]> {
    #[cfg(not(windows))]
    {
        // PORT NOTE: `posix.pipe2(.{ .CLOEXEC = true })`. On platforms with the
        // `pipe2` syscall (Linux/BSD) we pass `O_CLOEXEC` directly, matching the
        // syscall the reference issues. On macOS there is no `pipe2`, so — exactly
        // as Zig's std does for `posix.pipe2` on such targets — we `pipe()` then
        // set `FD_CLOEXEC` on both descriptors with `fcntl`. CLOEXEC is set on the
        // file descriptors either way, preserving the documented behavior.
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "dragonfly",
            target_os = "openbsd"
        ))]
        {
            let mut fds: [libc::c_int; 2] = [0; 2];
            let rc = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
            if rc != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok([fds[0], fds[1]])
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "dragonfly",
            target_os = "openbsd"
        )))]
        {
            let mut fds: [libc::c_int; 2] = [0; 2];
            let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
            if rc != 0 {
                return Err(std::io::Error::last_os_error());
            }
            for &fd in &fds {
                let flags = unsafe { libc::fcntl(fd, libc::F_GETFD, 0) };
                if flags < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok([fds[0], fds[1]])
        }
    }
    #[cfg(windows)]
    {
        let mut read: crate::windows::HANDLE = std::ptr::null_mut();
        let mut write: crate::windows::HANDLE = std::ptr::null_mut();
        if crate::windows::exp::kernel32::CreatePipe(&mut read, &mut write, std::ptr::null(), 0)
            == 0
        {
            return crate::windows::unexpectedError(crate::windows::kernel32::GetLastError());
        }

        Ok([read, write])
    }
}
