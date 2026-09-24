//! rustty-vt-c panic
//!
//! PORTS: ghostty `src/lib_vt.zig` `pub const panic` and `tinyPanicImpl`
//! (parity S43-F6).
//!
//! PORT NOTE: Zig selects a root module's panic handler at compile time:
//!
//! ```text
//! pub const panic: type = if (debug_machinery)
//!     std.debug.FullPanic(std.debug.defaultPanic)
//! else
//!     std.debug.FullPanic(tinyPanicImpl);
//! ```
//!
//! Rust has no per-artifact panic handler for a `std` library; the analog is a
//! process panic hook, installed by [`install`] when `DEBUG_MACHINERY` is false
//! (and left as std's default otherwise, the `defaultPanic` arm). A hook can only
//! run once a C entry point has installed it, so [`install`] is called from
//! `sys::install_log_backend`, which every `*_new` entry point already calls
//! first.
//!
//! PORT NOTE (divergences, review-T-H S43-F6). CORRECTION: this note used to
//! say no workspace crate depends on `rustty-vt-c`, so the hook reaches no
//! Rust host. `rustty-web-canvas` does depend on it (as an rlib); that is
//! harmless today only because [`install`] does nothing on wasm32, and that
//! crate is itself a lib-vt artifact. What actually differs from the
//! reference:
//!
//! - **Scope.** The reference's handler applies "when this file is the root
//!   module" (`lib_vt.zig:495`): a Zig program that imports the module as a
//!   dependency keeps its own root's handler. The port's hook is
//!   process-wide. In a cdylib or staticlib that is still the library's own
//!   concern, because the library carries its own copy of Rust `std` and a
//!   C or Swift host has no Rust hook to lose; both handlers end in an
//!   abort. But a Rust program that links the `rlib` and calls any `*_new`
//!   in a build without debug assertions has its own panic hook replaced
//!   without notice. No such host exists outside wasm.
//! - **Timing.** The reference's handler is chosen at compile time and
//!   covers every panic. The port's covers only panics after the first
//!   `*_new`; entry points that need no terminal (paste, focus, size report,
//!   unicode, build info, `sys_log_stderr`) keep std's hook until then. The
//!   hook is deliberately not installed at load time (a constructor in
//!   `__mod_init_func`/`.init_array`): replacing a host's hook merely because
//!   the library was loaded would widen the scope divergence above to every
//!   process that links the library, including ones that never call it
//!   (controller ruling on the review).
//! - **Build mode.** `DEBUG_MACHINERY` maps Zig's Debug/ReleaseSafe to
//!   `debug_assertions`. So the plain `release` profile, which DG2 keeps at
//!   ReleaseSafe's overflow-trap parity, gets the tiny handler, and only the
//!   `release-safe` profile (debug assertions on) keeps std's, as the
//!   reference's ReleaseSafe does.
//!
//! Should the hook also require that this process is the lib artifact
//! (`rustty_vt::options::runtime_artifact() == Lib`, parity S6-F2)? It
//! already does, by construction: `sys::install_log_backend` marks the
//! process as the lib artifact and then calls [`install`], and nothing else
//! calls either. The run-time artifact is the port's record of "this process
//! is libghostty-vt", which is the closest thing it has to the reference's
//! "`lib_vt.zig` is the root module", and the hook exists exactly when that
//! record says `Lib`. A check inside the hook could never fail, so none is
//! added; the gate that restricts the artifact setter to this crate
//! (`check_runtime_artifact_setter.py`) is what keeps the two tied.

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
