//! rustty-cli::io_file
//!
//! Zig's `std.Io.File.stdout()` / `std.Io.File.stderr()` as the CLI actions and
//! `main_ghostty.zig` use them: a file over the process's standard handle whose
//! writes REPORT the operating system's error.
//!
//! PORT NOTE (task W-4, NEW-FINDINGS N365): Rust's `std::io::stdout()` /
//! `stderr()` are not that. When the standard handle is missing or unusable,
//! Rust's stdio discards the bytes and reports success (library/std
//! `sys/stdio`: `ERROR_INVALID_HANDLE` on Windows and `EBADF` on Unix become
//! `Ok`). Zig's `File` writes through the raw handle (`NtWriteFile`, POSIX
//! `write`) and returns `error.NotOpenForWriting`. The helper CLI's exit status
//! hangs on that error: every reference action `try`s its writes, so a failed
//! write makes the action fail (exit 1), and the no-action usage banner's
//! failed `try stdout.print` makes `main` return the error (exit 1).
//!
//! It is reachable on every Windows launch that gives the exe no usable
//! handle. The exe is a GUI-subsystem binary (`GhosttyExe.zig`), so a shell
//! start (a double-click, `Start-Process`) passes NULL handles, and
//! `cmd /c rustty +version` in a console passes console handles the process
//! cannot write to. MEASURED on Windows 11 (task W-4, the reference exe built at
//! the pin): Ghostty exits 1 in all four cases ({no args, `+version`} x {shell
//! start, console}), and the port exited 0 before this module. On Unix the same
//! difference shows with a descriptor that is open but not writable
//! (`rustty +version 1</dev/null`: `EBADF`). A CLOSED descriptor
//! (`rustty +version >&-`) still differs and cannot be matched here: Rust's
//! runtime reopens a closed fd 0-2 on `/dev/null` before `main`, so the
//! writes succeed; Zig's start code does not, so Ghostty gets `EBADF` (N365).
//!
//! These writers are unbuffered (each `write` is one OS write, like
//! `File.Writer` in `.streaming` mode). Callers wrap them in a `BufWriter`
//! where the reference gives the writer a buffer.

use std::io;

/// Which standard handle a [`StdFile`] writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Stdout,
    Stderr,
}

/// A process standard handle opened as a file (Zig's `std.Io.File` for
/// `stdout()`/`stderr()`). It never owns or closes the handle.
#[derive(Debug)]
pub struct StdFile {
    which: Which,
}

/// `std.Io.File.stdout()`.
pub fn stdout() -> StdFile {
    StdFile {
        which: Which::Stdout,
    }
}

/// `std.Io.File.stderr()`.
pub fn stderr() -> StdFile {
    StdFile {
        which: Which::Stderr,
    }
}

impl StdFile {
    /// `File.isTty()`. Reads the same handle as Rust's `IsTerminal` on the
    /// matching `std::io` stream.
    pub fn is_terminal(&self) -> bool {
        use std::io::IsTerminal;
        match self.which {
            Which::Stdout => io::stdout().is_terminal(),
            Which::Stderr => io::stderr().is_terminal(),
        }
    }
}

impl io::Write for StdFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        imp::write(self.which, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        // Unbuffered: every `write` already reached the handle.
        Ok(())
    }
}

#[cfg(unix)]
mod imp {
    use super::Which;
    use core::ffi::{c_int, c_void};
    use std::io;

    extern "C" {
        // `posix.system.write(fd, ptr, len) -> isize` (the raw `write(2)`,
        // which `std.Io.File` calls on POSIX; the `rustty-os::stderr` precedent).
        #[link_name = "write"]
        fn libc_write(fd: c_int, buf: *const c_void, count: usize) -> isize;
    }

    pub fn write(which: Which, buf: &[u8]) -> io::Result<usize> {
        let fd: c_int = match which {
            Which::Stdout => 1,
            Which::Stderr => 2,
        };
        // SAFETY: `buf` is valid for `buf.len()` bytes; `write(2)` only reads
        // through the pointer and does not retain it. A closed or invalid `fd`
        // is reported by `write(2)` as `EBADF`, never undefined behaviour.
        let rc = unsafe { libc_write(fd, buf.as_ptr().cast::<c_void>(), buf.len()) };
        if rc < 0 {
            // `EINTR` comes back as `ErrorKind::Interrupted`, which `write_all`
            // retries, as Zig's `.INTR => continue`.
            Err(io::Error::last_os_error())
        } else {
            Ok(rc as usize)
        }
    }
}

#[cfg(windows)]
#[allow(clippy::upper_case_acronyms)] // the Win32 spellings, as in rustty-os
mod imp {
    use super::Which;
    use core::ffi::c_void;
    use std::io;

    type HANDLE = *mut c_void;
    type DWORD = u32;
    type BOOL = i32;

    // (DWORD)-11 and (DWORD)-12.
    const STD_OUTPUT_HANDLE: DWORD = 0xFFFF_FFF5;
    const STD_ERROR_HANDLE: DWORD = 0xFFFF_FFF4;

    extern "system" {
        fn GetStdHandle(nStdHandle: DWORD) -> HANDLE;
        fn WriteFile(
            hFile: HANDLE,
            lpBuffer: *const c_void,
            nNumberOfBytesToWrite: DWORD,
            lpNumberOfBytesWritten: *mut DWORD,
            lpOverlapped: *mut c_void,
        ) -> BOOL;
    }

    pub fn write(which: Which, buf: &[u8]) -> io::Result<usize> {
        let id = match which {
            Which::Stdout => STD_OUTPUT_HANDLE,
            Which::Stderr => STD_ERROR_HANDLE,
        };
        // One `WriteFile` moves at most `DWORD::MAX` bytes; `write_all` loops.
        let len = DWORD::try_from(buf.len()).unwrap_or(DWORD::MAX);
        let mut written: DWORD = 0;
        // SAFETY: `GetStdHandle` returns the process's standard handle (the PEB
        // value Zig's `File.stdout()` reads), possibly NULL or unusable, which
        // `WriteFile` rejects with `ERROR_INVALID_HANDLE` (the error Zig maps to
        // `NotOpenForWriting`). `WriteFile` reads at most `len <= buf.len()`
        // bytes through `buf.as_ptr()` and stores the count in `written`; no
        // OVERLAPPED is passed, so the call is synchronous.
        let ok = unsafe {
            WriteFile(
                GetStdHandle(id),
                buf.as_ptr().cast::<c_void>(),
                len,
                &mut written,
                core::ptr::null_mut(),
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(written as usize)
        }
    }
}

// Freestanding targets (wasm) have no standard handles: Zig's `File.stdout()`
// has no backing there either, and the helper CLI is not built for them.
#[cfg(not(any(unix, windows)))]
mod imp {
    use super::Which;
    use std::io;

    pub fn write(_which: Which, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // A write through a usable handle succeeds and reports its length. (libtest
    // does not capture raw handle writes, so the byte lands in the test log.)
    #[test]
    fn a_write_to_the_process_stderr_reports_its_length() {
        let n = stderr()
            .write(b" ")
            .expect("stderr is open under cargo test");
        assert_eq!(1, n);
    }

    // N365: the error path. A child whose stdout is open but not writable must
    // see the write FAIL (`EBADF`, Zig's `NotOpenForWriting`), where Rust's
    // `std::io::stdout()` reports success. The child is this test binary re-run
    // with fd 1 opened read-only on `/dev/null`. The Windows form (a NULL or
    // unusable stdout handle) is exercised end to end on the exe by
    // `crates/rustty/tests/windows_launch.rs`.
    #[cfg(unix)]
    const CHILD: &str = "RUSTTY_IO_FILE_CHILD";

    #[cfg(unix)]
    #[test]
    fn a_write_to_an_unwritable_stdout_fails_where_std_reports_success() {
        if std::env::var_os(CHILD).is_some() {
            // Child arm: report both outcomes on stderr for the parent.
            let ours = stdout().write_all(b"x").is_err();
            let std_ok = std::io::stdout().write_all(b"x").is_ok();
            eprintln!("RESULT ours_failed={ours} std_ok={std_ok}");
            return;
        }
        let read_only = std::fs::File::open("/dev/null").unwrap();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "io_file::tests::a_write_to_an_unwritable_stdout_fails_where_std_reports_success",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .stdin(std::process::Stdio::null())
            .stdout(read_only)
            .stderr(std::process::Stdio::piped())
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(
            text.contains("RESULT ours_failed=true std_ok=true"),
            "the child did not see the write fail where std reports success: {text}"
        );
    }
}
