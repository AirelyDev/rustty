//! rustty-os::stderr
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/stderr.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).
//!
//! Raw, best-effort stderr writing that bypasses `std.Io` entirely.
//!
//! `std.debug.lockStderr` (and everything layered on top of it) routes
//! through `std.Options.debug_io`, whose default implementation is
//! `std.Io.Threaded`. This keeps ~70KB of unreachable code in binaries.
//
// PORT NOTE: the reference imports `std` and `builtin` and switches on
// `builtin.os.tag`. In Rust the target switch is expressed with `#[cfg(...)]`
// attributes (R8): the whole `switch (builtin.os.tag)` collapses to three
// mutually-exclusive `imp` modules — the POSIX arm (`unix`), the Windows arm
// (`windows`), and the freestanding/other no-op arm (`not(any(unix, windows))`,
// which is where `wasm32-unknown-unknown` lands). The public `write` is a thin
// dispatcher so the module doc, signature, and `test {}` are shared.
//
// PORT NOTE: this is the "raw write" seam described in the rustty-vt `mem.rs`
// precedent: the syscall/symbol it needs is declared directly with `extern "C"`
// (POSIX) / `extern "system"` (Windows) and wrapped in one safe function, rather
// than routed through the `libc` crate's wrappers. It carries NO new crate edge
// — it is signal-safe and self-contained, which is the whole point of a
// panic/log stderr primitive.

/// Write bytes to stderr using the most primitive mechanism available
/// for the target. This is best-effort: errors are ignored, since this
/// is only used for diagnostics (logging and panic messages).
///
/// Freestanding targets (e.g. wasm) have no stderr, so this is a no-op
/// there.
pub fn write(bytes: &[u8]) {
    imp::write(bytes);
}

// PORT NOTE: `.else =>` arm (`builtin.os.tag` is neither `.freestanding`/`.other`
// nor `.windows`) — the POSIX path. The reference calls `posix.system.write`
// (the raw `write(2)`) in an `EINTR`-retry loop, ignoring every other error.
#[cfg(unix)]
mod imp {
    use core::ffi::{c_int, c_void};

    // PORT NOTE: `posix.STDERR_FILENO` and `posix.E.INTR`. Both are stable
    // across Linux/macOS/BSD (fd 2, errno 4), so they are declared as constants
    // rather than pulled from the `libc` crate.
    const STDERR_FILENO: c_int = 2;
    const EINTR: c_int = 4;

    extern "C" {
        // PORT NOTE: `posix.system.write(fd, ptr, len) -> isize`.
        #[link_name = "write"]
        fn libc_write(fd: c_int, buf: *const c_void, count: usize) -> isize;
    }

    // PORT NOTE: `posix.errno(rc)` reads the thread-local `errno` after a `-1`
    // return. The symbol that exposes it differs by libc: glibc/bionic use
    // `__errno_location`, Apple/BSD libc uses `__error`. Both return a pointer
    // to the current thread's `errno`.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    extern "C" {
        fn __errno_location() -> *mut c_int;
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    extern "C" {
        fn __error() -> *mut c_int;
    }

    /// Safe wrapper over the platform errno-location extern.
    #[inline]
    fn errno() -> c_int {
        // SAFETY(os/stderr.zig posix.errno / rustty-os stderr::imp::errno): the
        // libc errno-location function returns a valid, aligned pointer to the
        // calling thread's `errno` for the lifetime of the thread; a single
        // non-atomic read of that `c_int` is sound.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        unsafe {
            *__errno_location()
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        unsafe {
            *__error()
        }
    }

    pub fn write(bytes: &[u8]) {
        let mut i: usize = 0;
        while i < bytes.len() {
            // SAFETY(os/stderr.zig write / rustty-os stderr::imp::write):
            // `bytes[i..]` is valid for `bytes.len() - i` bytes (`i < len`);
            // `write(2)` only reads through the pointer and never retains it.
            let rc = unsafe {
                libc_write(
                    STDERR_FILENO,
                    bytes.as_ptr().add(i).cast::<c_void>(),
                    bytes.len() - i,
                )
            };
            // PORT NOTE: `switch (posix.errno(rc))` — `.SUCCESS` is any `rc >= 0`.
            if rc >= 0 {
                // PORT NOTE: `.SUCCESS => i += @intCast(rc)`.
                i += rc as usize;
            } else if errno() == EINTR {
                // PORT NOTE: `.INTR => continue`.
                continue;
            } else {
                // PORT NOTE: `else => return`.
                return;
            }
        }
    }
}

// PORT NOTE: `.windows =>` arm. The reference resolves the stderr handle from
// the PEB (`windows.peb().ProcessParameters.hStdError`) and writes it with the
// ntdll syscall `NtWriteFile`. Rust has no `std.os.windows.peb()`, and standing
// up a PEB/ntdll surface for a diagnostics-only path is not warranted, so this
// arm resolves the SAME stderr handle via `GetStdHandle(STD_ERROR_HANDLE)` and
// writes it with `WriteFile` — the equivalent primitive on the same DLL family
// the rest of `os/windows.zig` binds. Best-effort: the result is ignored, since
// this is only used for diagnostics.
#[cfg(windows)]
mod imp {
    use core::ffi::c_void;

    type HANDLE = *mut c_void;
    type DWORD = u32;
    type BOOL = i32;

    // (DWORD)-12 — the standard error device.
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

    pub fn write(bytes: &[u8]) {
        // SAFETY(os/stderr.zig write / rustty-os stderr::imp::write):
        // `GetStdHandle` returns the process's stderr handle (or an invalid
        // handle, which `WriteFile` rejects harmlessly); `WriteFile` reads at
        // most `bytes.len()` bytes through `bytes.as_ptr()` and writes the count
        // into the local `written`. The result is ignored (best-effort).
        unsafe {
            let handle = GetStdHandle(STD_ERROR_HANDLE);
            let mut written: DWORD = 0;
            let _ = WriteFile(
                handle,
                bytes.as_ptr().cast::<c_void>(),
                bytes.len() as DWORD,
                &mut written,
                core::ptr::null_mut(),
            );
        }
    }
}

// PORT NOTE: `.freestanding, .other => {}` — no stderr, so a no-op. This is the
// `wasm32-unknown-unknown` (and any other non-`unix`, non-`windows`) target.
#[cfg(not(any(unix, windows)))]
mod imp {
    pub fn write(_bytes: &[u8]) {}
}

#[cfg(test)]
mod tests {
    // PORT NOTE: `test write { write(""); }`. Named to match the reference's
    // `test write` (the decl it exercises); it reaches the module function via
    // `super::write` so the test's own name can stay `write`.
    #[test]
    fn write() {
        // Smoke test: must not crash. We can't assert on the output without
        // capturing stderr, which isn't worth the complexity here.
        super::write(b"");
    }
}
