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
//! (`rustty +version 1</dev/null`: `EBADF`), and with a CLOSED descriptor
//! (`rustty +version >&-`). Rust's runtime reopens a closed fd 0-2 on
//! `/dev/null` before `main`, so a write would succeed; Zig's start code does
//! not, so a Zig 0.16.0 program writing through `File.stdout().writer(..)` gets
//! `EBADF` and exits 1 (MEASURED, review W-4). The pre-main probe below
//! (`CLOSED_AT_START`) records which of fd 1 and 2 were closed at exec, and a
//! write to such a stream fails with `EBADF`, as the reference's does (task
//! W-4b; CORRECTION: this said the closed case "cannot be matched").
//!
//! PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS UD346. The reference's
//! writer is POSITIONAL: Zig 0.16.0 `std/Io/File/Writer.zig` `init` starts in
//! `.positional` mode at `pos = 0`, so every `stdout.writer(io, &buffer)` in the
//! CLI writes with `pwrite` from offset 0 whatever the descriptor's offset or
//! `O_APPEND`: `(echo header; ghostty +version) > f` overwrites the header,
//! `ghostty +version >> f` overwrites the start of an existing file, and
//! `end()` (`+validate-config`; `+explain-config`'s usage on stderr) then
//! truncates the file to its own output, destroying a `>>` log (MEASURED with
//! Zig 0.16.0 probes using the reference's writer calls, review W-4). The port
//! writes at the descriptor's current offset (`write(2)` / `WriteFile`), which
//! is what a shell redirection means. Listed in `docs/ghostty-bug-fixes.md`.
//!
//! These writers are unbuffered (each `write` is one OS write). Callers wrap
//! them in a `BufWriter` of the reference's buffer size.

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

/// Which of fd 1 and 2 were closed when the process started (bit 1: stdout,
/// bit 2: stderr), recorded by [`probe_closed_std_fds`] before Rust's runtime
/// reopens them on `/dev/null`.
#[cfg(unix)]
static CLOSED_AT_START: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The pre-main probe: a C constructor, which the loader runs before Rust's
/// `lang_start` (where the runtime sanitises fds 0-2). `fcntl(fd, F_GETFD)`
/// fails with `EBADF` only for a closed descriptor.
#[cfg(unix)]
extern "C" fn probe_closed_std_fds() {
    let mut closed = 0u8;
    for (fd, bit) in [(1, 1u8), (2, 2u8)] {
        // SAFETY: `fcntl(F_GETFD)` reads the descriptor flags and has no other
        // effect; any `fd` value is allowed (a closed one returns -1).
        if unsafe { libc::fcntl(fd, libc::F_GETFD) } == -1 {
            closed |= bit;
        }
    }
    CLOSED_AT_START.store(closed, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "android"))]
#[used]
#[link_section = ".init_array"]
static PROBE_CLOSED_STD_FDS: extern "C" fn() = probe_closed_std_fds;

// N495: under AddressSanitizer LLVM wraps an instrumented global in a larger
// object with a trailing redzone. `.init_array` is exempt from that (LLVM skips
// globals in the ELF init/fini sections), but `__mod_init_func` is not, so the
// redzone becomes extra zero "initializer pointers" and ld64 refuses to link
// ("initializer pointer has no target"). The attribute keeps this one global
// uninstrumented; G-ASAN's `a_write_to_a_stdout_closed_at_exec_fails_with_ebadf`
// proves the constructor still runs in the ASan build.
#[cfg(target_vendor = "apple")]
#[used]
#[cfg_attr(rustty_asan, sanitize(address = "off"))]
#[link_section = "__DATA,__mod_init_func"]
static PROBE_CLOSED_STD_FDS: extern "C" fn() = probe_closed_std_fds;

// Other Unix targets: no constructor section is wired, so no stream is ever
// recorded as closed (the pre-W-4b behaviour).
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "android",
        target_vendor = "apple"
    ))
))]
static PROBE_CLOSED_STD_FDS: extern "C" fn() = probe_closed_std_fds;

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
        let (fd, bit): (c_int, u8) = match which {
            Which::Stdout => (1, 1),
            Which::Stderr => (2, 2),
        };
        // A stream that was closed at exec is `EBADF`, as in Zig (module doc).
        // Reading the probe's static through `black_box` also keeps its
        // constructor entry linked into every binary that writes here.
        let _ = core::hint::black_box(&super::PROBE_CLOSED_STD_FDS);
        if super::CLOSED_AT_START.load(std::sync::atomic::Ordering::Relaxed) & bit != 0 {
            return Err(io::Error::from_raw_os_error(libc::EBADF));
        }
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
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-cli, parity tail C-1): `open` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
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

    /// Re-runs `test` in a child of this test binary with `CHILD` set, the
    /// child's stdout set up by `stdout` / `pre_exec`, and its stderr captured.
    #[cfg(unix)]
    fn run_child(test: &str, stdout: std::process::Stdio, close_fd1: bool) -> std::process::Output {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact", test, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .stdin(std::process::Stdio::null())
            .stdout(stdout)
            .stderr(std::process::Stdio::piped());
        if close_fd1 {
            // SAFETY: `close(1)` in the forked child before exec is
            // async-signal-safe and touches no Rust state.
            unsafe {
                cmd.pre_exec(|| {
                    libc::close(1);
                    Ok(())
                });
            }
        }
        cmd.output().unwrap()
    }

    // Review W-4 F1 (N365): stdout CLOSED at exec (`>&-`). Rust's runtime has
    // reopened fd 1 on `/dev/null` before this code runs, so Rust's stdout
    // succeeds; the pre-main probe makes ours fail with `EBADF`, as Zig's does.
    #[cfg(unix)]
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-cli, parity tail C-1): `current_exe` (`_NSGetExecutablePath`) is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn a_write_to_a_stdout_closed_at_exec_fails_with_ebadf() {
        if std::env::var_os(CHILD).is_some() {
            let err = stdout().write_all(b"x").err();
            let std_ok = std::io::stdout().write_all(b"x").is_ok();
            eprintln!(
                "RESULT ebadf={} std_ok={std_ok}",
                err.and_then(|e| e.raw_os_error()) == Some(libc::EBADF)
            );
            return;
        }
        let out = run_child(
            "io_file::tests::a_write_to_a_stdout_closed_at_exec_fails_with_ebadf",
            std::process::Stdio::inherit(),
            true,
        );
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(
            text.contains("RESULT ebadf=true std_ok=true"),
            "the child's write to its closed stdout did not fail with EBADF: {text}"
        );
    }

    // UD346 (R13): a stdout opened for appending on a file with content keeps
    // the content; the reference's positional writer (offset 0) overwrites it.
    #[cfg(unix)]
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-cli, parity tail C-1): `mkdir` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn an_appended_stdout_keeps_the_existing_content() {
        if std::env::var_os(CHILD).is_some() {
            let r = stdout().write_all(b"zig\n");
            eprintln!("RESULT ok={}", r.is_ok());
            return;
        }
        let dir =
            std::env::temp_dir().join(format!("rustty-io-file-append-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log");
        std::fs::write(&path, b"existing-content\n").unwrap();
        let file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        let out = run_child(
            "io_file::tests::an_appended_stdout_keeps_the_existing_content",
            std::process::Stdio::from(file),
            false,
        );
        let got = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("RESULT ok=true"),
            "child: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // libtest's own report lines land in the file too; what matters is that
        // the existing content is intact and our bytes follow it.
        let got = String::from_utf8_lossy(&got);
        assert!(
            got.starts_with("existing-content\n")
                && got["existing-content\n".len()..].contains("zig\n"),
            "the append overwrote or lost the existing content: {got:?}"
        );
    }
}
