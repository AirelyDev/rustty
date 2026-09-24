//! rustty-os::open
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/open.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `Allocator = std.mem.Allocator`,
// `build_config = @import("../build_config.zig")`, and `apprt = @import("../apprt.zig")`,
// and uses `std.log.scoped(.@"os-open")`.
//
// R4 SEAM: the reference's `apprt` import is a cycle (apprt depends on os).
// Break #14 relocates `os.open` into `apprt`; this port keeps the file in
// rustty-os per the porting task but expresses the apprt dependency as a minimal
// local seam type (`Kind`) rather than depending on rustty-apprt-api. This adds
// nothing-lost (R1): it is the parity image of `apprt.action.OpenUrl.Kind`, and
// the `open` logic is ported verbatim. `build_config` maps to `rustty_build_config`.
//
// In Rust the child-process plumbing uses `std::process::Command`; the OS switch
// is `cfg!(target_os = ..)`; the stderr-reaping thread is a `std::thread`.

use rustty_build_config as build_config;

#[allow(dead_code)]
const LOG_SCOPE: &str = "os-open";

/// PORT NOTE (R4 seam): the parity image of `apprt.action.OpenUrl.Kind`. The type of
/// the data at the URL to open. This is used as a hint to potentially open the
/// URL in a different way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The type is unknown.
    Unknown,
    /// The URL is known to be a text file.
    Text,
    /// The URL is known to contain HTML content.
    Html,
    /// The URL came from an OSC 8 hyperlink. Application runtimes should
    /// treat this as untrusted terminal output and apply a platform-specific
    /// safe-opening policy.
    Osc8,
}

/// PORT NOTE: the reference's error union from `open`/`std.process.Child` operations.
#[derive(Debug)]
pub enum OpenError {
    Unimplemented,
    /// `error.UnsafeOSC8Link` (`77537c806`): a macOS OSC 8 target reached the
    /// generic opener fallback — fail closed instead of handing
    /// producer-controlled terminal output to `open`.
    UnsafeOSC8Link,
    Spawn(std::io::Error),
    /// `std.Thread.spawn`'s error: the reaper thread could not be created.
    ThreadSpawn(std::io::Error),
}

/// Open a URL in the default handling application.
///
/// Any output on stderr is logged as a warning in the application logs.
/// Output on stdout is ignored.
///
/// This function is purposely simple for the sake of providing some portable
/// way to open URLs. If you are implementing an apprt for Rustty, you should
/// consider doing something special-cased for your platform.
// PORT NOTE (R3): the reference is `url: []const u8` — raw bytes, never a
// lossy `String`. Landed originally as `&str`; retyped to `&[u8]` when core
// C13's `Surface.openUrl` (the first caller) arrived with byte URLs (OSC8
// URIs are arbitrary bytes). POSIX argv takes the bytes as `OsStr` directly
// (the `path.rs` `os_str_from_bytes` mapping; the Zig argv is the same raw
// bytes). The collapsed `alloc` param is the Rust `Command` ownership.
#[allow(unused_variables)]
pub fn open(kind: Kind, url: &[u8]) -> Result<(), OpenError> {
    // On macOS, the apprt handles OSC 8 targets before this fallback. Rustty's
    // native apprt applies its allowlist, confirmation, and file safety policy.
    // If a macOS embedder declines the action, fail closed rather than bypassing
    // that policy by handing producer-controlled terminal output to `open`.
    // if (comptime builtin.os.tag == .macos) {
    //     if (kind == .osc8) return error.UnsafeOSC8Link;
    // }
    #[cfg(target_os = "macos")]
    if kind == Kind::Osc8 {
        return Err(OpenError::UnsafeOSC8Link);
    }

    let mut exe: std::process::Command = {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            let mut c = std::process::Command::new("xdg-open");
            c.arg(crate::path::os_str_from_bytes(url));
            c
        }
        #[cfg(target_os = "windows")]
        {
            let mut c = std::process::Command::new("rundll32");
            c.arg("url.dll,FileProtocolHandler");
            // (S112-F4: a URL that is not WTF-8 is spawn's error.InvalidWtf8,
            // not an empty argument.)
            c.arg(crate::path::os_string_from_bytes(url).ok_or_else(|| {
                OpenError::Spawn(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "InvalidWtf8",
                ))
            })?);
            c
        }
        #[cfg(target_os = "macos")]
        {
            match kind {
                Kind::Text => {
                    let mut c = std::process::Command::new("open");
                    c.arg("-t");
                    c.arg(crate::path::os_str_from_bytes(url));
                    c
                }
                Kind::Html | Kind::Unknown => {
                    let mut c = std::process::Command::new("open");
                    c.arg(crate::path::os_str_from_bytes(url));
                    c
                }
                // .osc8 => unreachable,
                Kind::Osc8 => unreachable!(),
            }
        }
        #[cfg(target_os = "ios")]
        {
            return Err(OpenError::Unimplemented);
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "windows",
            target_os = "macos",
            target_os = "ios"
        )))]
        {
            compile_error!("unsupported OS")
        }
    };

    // Ignore anything from stdout. This must be set before spawning the
    // process.
    exe.stdout(std::process::Stdio::null());
    // Pipe stderr so we can log the stderr from the command. This must be set
    // before spawning the process.
    exe.stderr(std::process::Stdio::piped());

    if build_config::snap() {
        // In the snap on Linux the launcher exports LD_LIBRARY_PATH
        // pointing at the snap's bundled libraries. Leaking this into
        // child process can can be problematic, so let's drop it from the
        // env.
        //
        // Note that `spawn` copies the passed in `Environ.Map` into a
        // fresh `Environ` block, so this is safe to release immediately
        // after spawn.
        //
        // PORT NOTE (`e8525c0fd`): upstream switched from
        // `std.process.getEnvMap(alloc)` to `global.environMap()` and hands
        // the map to `spawn` explicitly. `std::process::Command` already
        // starts from the inherited environment and applies `env_remove` as an
        // override at spawn time, so there is no map to build or release —
        // the copy-into-a-fresh-block note is what `Command` does for us.
        exe.env_remove("LD_LIBRARY_PATH");
    }
    // Non-snap releases don't need to alter the env.

    // Spawn the process on our same thread so we can detect failure
    // quickly.
    let child = exe.spawn().map_err(OpenError::Spawn)?;

    // const thread = try std.Thread.spawn(.{}, openThread, .{ global.io(), exe });
    // thread.detach();
    spawn_open_thread(std::thread::Builder::new(), child)
}

/// `const thread = try std.Thread.spawn(.{}, openThread, .{ global.io(), exe });
/// thread.detach();` — the reaper thread for a spawned opener.
//
// PORT NOTE (S112-F7): the reference's `try` returns the spawn error to the
// caller. This used `std::thread::spawn`, which panics when the thread cannot
// be created; `Builder::spawn` returns the error, carried as
// `OpenError::ThreadSpawn`. The builder is a parameter so a test can hand in
// one that cannot spawn; `open` always passes `Builder::new()`, the reference's
// default `.{}` spawn config.
fn spawn_open_thread(
    builder: std::thread::Builder,
    mut child: std::process::Child,
) -> Result<(), OpenError> {
    let stderr = child.stderr.take();
    let thread = builder
        .spawn(move || openThread(stderr, child))
        .map_err(OpenError::ThreadSpawn)?;
    drop(thread);
    Ok(())
}

// test "macOS OSC 8 links have no generic opener fallback" {
//     if (builtin.os.tag != .macos) return error.SkipZigTest;
//
//     try std.testing.expectError(
//         error.UnsafeOSC8Link,
//         open(.osc8, "file:///tmp/payload.command"),
//     );
// }
//
// (the `SkipZigTest` non-macOS arm maps to the `cfg(target_os = "macos")`
// test gate.)
#[cfg(all(test, target_os = "macos"))]
mod osc8_tests {
    use super::*;

    #[test]
    fn macos_osc8_links_have_no_generic_opener_fallback() {
        assert!(matches!(
            open(Kind::Osc8, b"file:///tmp/payload.command"),
            Err(OpenError::UnsafeOSC8Link)
        ));
    }
}

#[allow(non_snake_case)]
fn openThread(stderr: Option<std::process::ChildStderr>, exe_: std::process::Child) {
    // Copy the exe so it is non-const. This is necessary because wait()
    // requires a mutable reference and we can't have one as a thread
    // param.
    let mut exe = exe_;
    if let Some(stderr) = stderr {
        log_stderr_lines(stderr, |line| {
            log_warn(&format!("open stderr={}", String::from_utf8_lossy(line)));
        });
    }
    let _ = exe.wait();
}

/// The stderr loop of `openThread`: one `log` call per line, a line longer
/// than the 256-byte buffer logged in 256-byte pieces.
//
// var buffer: [256]u8 = undefined;
// var stream = stderr.readerStreaming(io, &buffer);
// const reader = &stream.interface;
// while (true) {
//     const line = reader.takeDelimiterExclusive('\n') catch |outer| switch (outer) {
//         error.EndOfStream => break,
//         error.ReadFailed => break,
//         error.StreamTooLong => reader.take(buffer.len) catch |inner| switch (inner) {
//             error.ReadFailed => break,
//             error.EndOfStream => break,
//         },
//     };
//     log.warn("open stderr={s}", .{line});
// }
//
// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD200. In Zig 0.16,
// `takeDelimiterExclusive` advances "up to (but not past) the delimiter"
// (`std/Io/Reader.zig:852-876`), so after the first line every call finds the
// same `'\n'` at the read position and returns an empty slice: the loop logs
// `open stderr=` forever, never reads further, and never reaches `exe.wait()`,
// so the opener is never reaped and the thread spins. Reachable whenever the
// opener (`open`, `xdg-open`) writes anything to stderr followed by a newline.
// Measured with the reference loop under zig 0.16.0 on "hello\nworld\n": line
// 0 is "hello", lines 1.. are empty. The port does what the loop is for: it
// consumes the delimiter, so each line is logged once, and it keeps the
// 256-byte buffer's `StreamTooLong => take(buffer.len)` chunking of a long
// line. (S112-F7: the port had instead logged whole lines of any length.)
fn log_stderr_lines(stderr: impl std::io::Read, mut log: impl FnMut(&[u8])) {
    use std::io::BufRead;

    const BUFFER_LEN: usize = 256;
    let mut reader = std::io::BufReader::with_capacity(BUFFER_LEN, stderr);
    // `BufReader` refills only when EMPTY; the reference's reader compacts and
    // tops up a partial buffer before giving up on a delimiter. `pending`
    // holds the unfinished line so that a line which straddles two reads is
    // still split at 256 bytes from its own start.
    let mut pending: Vec<u8> = Vec::with_capacity(BUFFER_LEN);
    // (`Err` is error.ReadFailed => break.)
    while let Ok(available) = reader.fill_buf() {
        if available.is_empty() {
            // error.EndOfStream: `takeDelimiterExclusive` returns what is left
            // when it is not empty, and EndOfStream otherwise.
            if !pending.is_empty() {
                log(&pending);
            }
            break;
        }

        let room = BUFFER_LEN - pending.len();
        let window = &available[..available.len().min(room)];
        if let Some(i) = window.iter().position(|&b| b == b'\n') {
            pending.extend_from_slice(&window[..i]);
            reader.consume(i + 1);
            log(&pending);
            pending.clear();
        } else {
            pending.extend_from_slice(window);
            let n = window.len();
            reader.consume(n);
            if pending.len() == BUFFER_LEN {
                // error.StreamTooLong => reader.take(buffer.len)
                log(&pending);
                pending.clear();
            }
        }
    }
}

fn log_warn(msg: &str) {
    eprintln!("[warn] [{LOG_SCOPE}] {msg}");
}

#[cfg(all(test, unix))]
mod open_thread_tests {
    use super::*;

    /// Run `log_stderr_lines` over `input` and return the logged lines. More
    /// than 64 lines means the loop is not consuming its input (UD200: the
    /// reference logs an empty line forever), so it panics instead of hanging.
    fn lines_of(input: &[u8]) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = Vec::new();
        log_stderr_lines(input, |line| {
            assert!(
                out.len() < 64,
                "stderr loop does not advance past a delimiter"
            );
            out.push(line.to_vec());
        });
        out
    }

    /// S112-F7 / UD200: every line is logged once (the delimiter is consumed),
    /// a line longer than the 256-byte buffer is logged in 256-byte pieces, and
    /// a last line with no newline is still logged.
    #[test]
    fn stderr_lines_are_logged_once_each_in_256_byte_pieces() {
        let mut input = b"hello\nworld\n".to_vec();
        input.extend(std::iter::repeat_n(b'x', 300));
        input.extend_from_slice(b"\nend");
        assert_eq!(
            lines_of(&input),
            vec![
                b"hello".to_vec(),
                b"world".to_vec(),
                vec![b'x'; 256],
                vec![b'x'; 44],
                b"end".to_vec(),
            ]
        );
    }

    /// The buffer boundary: a 255-byte line plus its newline fits the 256-byte
    /// buffer and is one line; a 256-byte line fills it with no newline, so it
    /// is taken whole and the newline after it ends an empty line.
    #[test]
    fn stderr_line_at_the_buffer_boundary() {
        let mut input = vec![b'a'; 255];
        input.push(b'\n');
        assert_eq!(lines_of(&input), vec![vec![b'a'; 255]]);

        let mut input = vec![b'b'; 256];
        input.push(b'\n');
        assert_eq!(lines_of(&input), vec![vec![b'b'; 256], Vec::new()]);

        assert_eq!(lines_of(b""), Vec::<Vec<u8>>::new());
        assert_eq!(lines_of(b"\n\n"), vec![Vec::new(), Vec::new()]);
    }

    /// Whether `pid` still exists, as a process or as an unreaped zombie.
    fn exists(pid: libc::pid_t) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// UD200 end to end: an opener that writes lines to stderr is reaped by the
    /// thread `open` spawns, so the stderr loop ended and `wait` was reached.
    /// Only that thread waits on the child, so the pid going away proves it.
    #[test]
    fn open_thread_reaps_an_opener_that_writes_stderr() {
        let child = std::process::Command::new("/bin/sh")
            .args(["-c", "printf 'one\\ntwo\\n' >&2"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn /bin/sh");
        let pid = child.id() as libc::pid_t;
        spawn_open_thread(std::thread::Builder::new(), child).expect("reaper thread");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while exists(pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!exists(pid), "the opener was not reaped within 10 s");
    }

    /// S112-F7: a reaper thread that cannot be created is an error, not a
    /// panic. No stack as large as half the address space can be mapped.
    #[test]
    fn open_thread_spawn_failure_is_an_error() {
        let child = std::process::Command::new("/usr/bin/true")
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn /usr/bin/true");
        let pid = child.id() as libc::pid_t;
        let builder = std::thread::Builder::new().stack_size(usize::MAX / 2);
        let ret = spawn_open_thread(builder, child);
        assert!(
            matches!(ret, Err(OpenError::ThreadSpawn(_))),
            "expected ThreadSpawn, got {ret:?}"
        );
        // The failed spawn dropped the child without waiting; reap it here.
        let mut status = 0;
        assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
    }
}
