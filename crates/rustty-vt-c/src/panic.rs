//! rustty-vt-c panic
//!
//! PORTS: ghostty `src/lib_vt.zig` `pub const panic` and `tinyPanicImpl`
//! (parity S43-F6).
//!
//! PORT NOTE: Zig selects a root module's panic handler at compile time:
//!
//!     pub const panic: type = if (debug_machinery)
//!         std.debug.FullPanic(std.debug.defaultPanic)
//!     else
//!         std.debug.FullPanic(tinyPanicImpl);
//!
//! Rust has no per-artifact panic handler for a `std` library; the analog is a
//! process panic hook, installed by [`install`] when `DEBUG_MACHINERY` is false
//! (and left as std's default otherwise, the `defaultPanic` arm). A hook can only
//! run once a C entry point has installed it, so [`install`] is called from
//! `sys::install_log_backend`, which every `*_new` entry point already calls
//! first. The `rustty-vt-c` library is linked by C hosts only (no workspace
//! crate depends on it except as a comment), so the hook reaches no Rust host.

/// Installs [`tiny_panic_impl`] as the panic hook in builds without the debug
/// machinery. Idempotent.
pub fn install() {
    // wasm has no stderr (`os/stderr.zig` `.freestanding => {}`), and a wasm
    // panic already ends in a trap.
    if crate::DEBUG_MACHINERY || cfg!(target_arch = "wasm32") {
        return;
    }
    install_unconditionally();
}

/// The hook installation itself, without the build gate (the tests drive it
/// in a child process, where `DEBUG_MACHINERY` is always true).
fn install_unconditionally() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let msg = info.payload_as_str().unwrap_or("Box<dyn Any>");
            tiny_panic_impl(msg)
        }));
    });
}

/// Prints the panic message to stderr (best-effort) and traps.
///
/// This intentionally avoids `std.debug.lockStderr`, which routes through
/// `std.Options.debug_io` and would keep the entire `std.Io.Threaded`
/// vtable alive in the binary which takes up hundreds of KB.
///
/// This is safe to call from any thread (and even from signal handlers):
/// it takes no locks, performs no allocation, and touches no shared
/// mutable state. The message is emitted with a single raw write so that
/// concurrent stderr output doesn't interleave with it.
// PORT NOTE: the std-machinery reason above is Zig's; the port keeps its
// behaviour (no stderr lock, no allocation, one write when it fits). `@trap()`
// is `std::process::abort()`, the stable Rust way to end the process with a
// crash signal that a debugger or crash reporter catches; `ra` has no analog
// (the hook is not given a return address).
fn tiny_panic_impl(msg: &str) -> ! {
    // 256 bytes is enough for most messages, so try that first
    // so that we can try to write in a single syscall.
    let mut buf = [0u8; 256];
    let prefix = b"panic: ";
    let len = prefix.len() + msg.len() + 1;
    if len <= buf.len() {
        buf[..prefix.len()].copy_from_slice(prefix);
        buf[prefix.len()..len - 1].copy_from_slice(msg.as_bytes());
        buf[len - 1] = b'\n';
        stderr_write(&buf[..len]);
    } else {
        stderr_write(b"panic: ");
        stderr_write(msg.as_bytes());
        stderr_write(b"\n");
    }

    // Trap forces a standard crash that embedder-provided debuggers
    // or environments can catch.
    std::process::abort()
}

/// `os/stderr.zig` `write`: raw, best-effort, unlocked write of `bytes` to
/// the process's stderr. Errors are ignored.
fn stderr_write(bytes: &[u8]) {
    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::fd::FromRawFd as _;
        // SAFETY: fd 2 is borrowed for this call only; `ManuallyDrop` keeps it
        // open. `File::write` is a plain write(2), with no lock, and
        // `write_all` retries EINTR and stops on any other error.
        let mut f = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(2) });
        let _ = f.write_all(bytes);
    }
    #[cfg(not(unix))]
    {
        // PORT NOTE: the reference's Windows arm is an `NtWriteFile` on the
        // PEB's stderr handle. std's `Stderr` is unbuffered but takes a
        // reentrant lock; this arm has no Windows build lane to test a raw
        // handle write on.
        use std::io::Write as _;
        let _ = std::io::stderr().write_all(bytes);
    }
}

#[cfg(all(test, unix))]
mod tests {
    const CHILD: &str = "RUSTTY_T_H_TINY_PANIC_CHILD";
    const NAME: &str = "panic::tests::tiny_panic_prints_only_the_message_and_aborts";

    /// Parity S43-F6: with the tiny handler installed, a panic writes exactly
    /// `panic: <msg>\n` (and nothing of std's `thread '…' panicked at`
    /// output) and ends the process with an abort. A message too long for the
    /// 256-byte buffer produces the same bytes through the piecewise arm. The
    /// gated `install()` does nothing under test (`DEBUG_MACHINERY` holds
    /// `is_test`), so std's default message appears there.
    #[test]
    fn tiny_panic_prints_only_the_message_and_aborts() {
        if let Some(mode) = std::env::var_os(CHILD) {
            // Child arm: the parent asserts on this process's stderr and exit.
            match mode.to_str() {
                Some("gated") => super::install(),
                _ => super::install_unconditionally(),
            }
            let msg = match mode.to_str() {
                Some("long") => "x".repeat(300),
                _ => "boom".to_string(),
            };
            std::panic::panic_any(msg);
        }

        use std::os::unix::process::ExitStatusExt as _;
        let run = |mode: &str| {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
                .env(CHILD, mode)
                .output()
                .expect("spawn the child test")
        };

        let out = run("short");
        assert_eq!(out.status.signal(), Some(6), "not an abort: {out:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            "panic: boom\n",
            "stderr must be exactly the tiny handler's line"
        );

        let out = run("long");
        assert_eq!(out.status.signal(), Some(6), "not an abort: {out:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            format!("panic: {}\n", "x".repeat(300))
        );

        assert!(
            crate::DEBUG_MACHINERY,
            "tests always keep the debug machinery"
        );
        let out = run("gated");
        assert_eq!(out.status.signal(), None);
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("panicked at"),
            "std's default hook must still be in place: {out:?}"
        );
    }
}
