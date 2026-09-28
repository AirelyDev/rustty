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
            pipe_cloexec_fallback(&LIBC_FD_OPS)
        }
    }
    #[cfg(windows)]
    {
        let mut read: crate::windows::HANDLE = std::ptr::null_mut();
        let mut write: crate::windows::HANDLE = std::ptr::null_mut();
        // SAFETY(ffi-boundary/stays-unsafe): `CreatePipe`/`GetLastError` are
        // `extern "system"` (windows.rs:207-208); `read`/`write` are valid
        // out-params and the attrs pointer is NULL (default security), matching
        // the reference `std.os.windows.CreatePipe` call. (C1, VALIDATION_REPORT_2.)
        if unsafe {
            crate::windows::exp::kernel32::CreatePipe(&mut read, &mut write, std::ptr::null(), 0)
        } == 0
        {
            return crate::windows::unexpectedError(unsafe {
                crate::windows::kernel32::GetLastError()
            });
        }

        Ok([read, write])
    }
}

/// The descriptor calls [`pipe_cloexec_fallback`] makes: `pipe`,
/// `fcntl(F_SETFD, FD_CLOEXEC)` and `close`. Production passes
/// [`LIBC_FD_OPS`], the libc calls themselves. It is a parameter only so a test
/// can make `F_SETFD` fail (it does not fail on a fresh pipe on its own) and see
/// which descriptors are closed.
#[cfg(all(
    not(windows),
    not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        target_os = "openbsd"
    ))
))]
struct FdOps {
    pipe: fn(&mut [libc::c_int; 2]) -> std::io::Result<()>,
    set_cloexec: fn(libc::c_int) -> std::io::Result<()>,
    close: fn(libc::c_int),
}

#[cfg(all(
    not(windows),
    not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        target_os = "openbsd"
    ))
))]
const LIBC_FD_OPS: FdOps = FdOps {
    pipe: |fds| {
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    },
    // `std.posix.system.fcntl(fd, std.posix.F.SETFD, @as(u32, std.posix.FD_CLOEXEC))`
    set_cloexec: |fd| {
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    },
    // `compat_fd.close(fd)`
    close: |fd| {
        unsafe { libc::close(fd) };
    },
};

/// `compat_fd.pipe2(.{ .CLOEXEC = true })` on a target without `pipe2`
/// (`src/lib/compat/fd.zig:42-63`): `pipe()`, then `FD_CLOEXEC` on both
/// descriptors with `F_SETFD`.
//
// PORT NOTE (S112-F6): the reference wraps the `F_SETFD` loop in
// `errdefer { close(fds[0]); close(fds[1]); }`, so a failing `fcntl` closes
// both new descriptors before the error returns. This used to return from
// inside the loop and leak them. It also read `F_GETFD` first and OR-ed
// `FD_CLOEXEC` into the result; the reference sets the descriptor-flag word to
// `FD_CLOEXEC` directly (`FD_CLOEXEC` is the only descriptor flag), so the
// `F_GETFD` read and its extra error arm are gone.
#[cfg(all(
    not(windows),
    not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        target_os = "openbsd"
    ))
))]
fn pipe_cloexec_fallback(ops: &FdOps) -> std::io::Result<[fd_t; 2]> {
    // const fds: [2]std.posix.fd_t = try pipe();
    let mut fds: [libc::c_int; 2] = [0; 2];
    (ops.pipe)(&mut fds)?;

    // CLOEXEC is special, it's a file descriptor flag and must be set using
    // F.SETFD.
    for &fd in &fds {
        if let Err(err) = (ops.set_cloexec)(fd) {
            // errdefer {
            //     close(fds[0]);
            //     close(fds[1]);
            // }
            (ops.close)(fds[0]);
            (ops.close)(fds[1]);
            return Err(err);
        }
    }
    Ok([fds[0], fds[1]])
}

#[cfg(test)]
#[cfg(all(
    not(windows),
    not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        target_os = "openbsd"
    ))
))]
mod fallback_tests {
    use super::*;
    use std::cell::RefCell;

    // Per test thread: the descriptors the recording `pipe` made, the ones
    // `close` was called with, and how many `F_SETFD` calls ran.
    thread_local! {
        static MADE: RefCell<Vec<libc::c_int>> = const { RefCell::new(Vec::new()) };
        static CLOSED: RefCell<Vec<libc::c_int>> = const { RefCell::new(Vec::new()) };
        static SETFD_CALLS: RefCell<usize> = const { RefCell::new(0) };
    }

    fn recording_pipe(fds: &mut [libc::c_int; 2]) -> std::io::Result<()> {
        (LIBC_FD_OPS.pipe)(fds)?;
        MADE.with(|m| m.borrow_mut().extend(*fds));
        Ok(())
    }

    fn recording_close(fd: libc::c_int) {
        CLOSED.with(|c| c.borrow_mut().push(fd));
        (LIBC_FD_OPS.close)(fd);
    }

    fn counting_setfd(fd: libc::c_int) -> std::io::Result<()> {
        SETFD_CALLS.with(|c| *c.borrow_mut() += 1);
        (LIBC_FD_OPS.set_cloexec)(fd)
    }

    /// `F_SETFD` succeeds on the first descriptor and fails on the second.
    fn setfd_fails_on_second(fd: libc::c_int) -> std::io::Result<()> {
        let n = SETFD_CALLS.with(|c| {
            *c.borrow_mut() += 1;
            *c.borrow()
        });
        if n == 2 {
            return Err(std::io::Error::from_raw_os_error(libc::EBADF));
        }
        (LIBC_FD_OPS.set_cloexec)(fd)
    }

    /// S112-F6: a failing `F_SETFD` closes BOTH new descriptors (the
    /// reference's `errdefer`) and returns the error.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-os, parity tail C-1): `fcntl(F_SETFD)` is not implemented by Miri; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn failing_setfd_closes_both_descriptors() {
        let ops = FdOps {
            pipe: recording_pipe,
            set_cloexec: setfd_fails_on_second,
            close: recording_close,
        };
        let err = pipe_cloexec_fallback(&ops).expect_err("the second F_SETFD fails");
        assert_eq!(err.raw_os_error(), Some(libc::EBADF));

        let made = MADE.with(|m| m.borrow().clone());
        // The loop ran: one pipe, and F_SETFD reached the second descriptor.
        assert_eq!(made.len(), 2, "one pipe() call");
        assert_eq!(SETFD_CALLS.with(|c| *c.borrow()), 2);
        let closed = CLOSED.with(|c| c.borrow().clone());
        assert_eq!(
            closed, made,
            "both new descriptors are closed, read end first"
        );
    }

    /// The success path sets `FD_CLOEXEC` on both ends and closes nothing.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-os, parity tail C-1): `fcntl(F_SETFD)` is not implemented by Miri; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn fallback_sets_cloexec_on_both_and_closes_nothing() {
        let ops = FdOps {
            pipe: recording_pipe,
            set_cloexec: counting_setfd,
            close: recording_close,
        };
        let fds = pipe_cloexec_fallback(&ops).expect("pipe");
        assert_eq!(SETFD_CALLS.with(|c| *c.borrow()), 2);
        for fd in fds {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            assert_eq!(flags & libc::FD_CLOEXEC, libc::FD_CLOEXEC);
        }
        assert!(CLOSED.with(|c| c.borrow().is_empty()));
        for fd in fds {
            unsafe { libc::close(fd) };
        }
    }
}
