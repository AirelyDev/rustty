//! rustty-os::passwd
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/passwd.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `internal_os = @import("main.zig")`,
// `build_config`, `Allocator`, `ArenaAllocator`, and `posix = std.posix`. In Rust
// we use `libc` (R3: a binding to the SAME system library) for the
// `getpwuid_r`/`getuid` C API and `rustty-build-config` for the `flatpak`
// build flag.

// Its only use (`build_config::flatpak()`) lives inside `get`'s flatpak block,
// which is absent on windows along with `get` itself (see the WI-4 note below).
#[cfg(not(windows))]
use rustty_build_config as build_config;

const LOG_SCOPE: &str = "passwd";

// We want to be extra sure since this will force bad symbols into our import table
// PORT NOTE: `comptime { if (builtin.target.cpu.arch.isWasm()) @compileError("passwd
// is not available for wasm"); }`.
#[cfg(target_family = "wasm")]
compile_error!("passwd is not available for wasm");

// PORT NOTE: `const c = if (builtin.os.tag != .windows) @cImport({ sys/types.h;
// unistd.h; pwd.h }) else {};` — used to determine the default shell and
// directory on Unixes. In Rust the `libc` crate provides `passwd`, `getpwuid_r`,
// and `getuid` for the same system headers.
/// Used to determine the default shell and directory on Unixes.
#[cfg(not(windows))]
use libc as c;

// Entry that is retrieved from the passwd API. This only contains the fields
// we care about.
#[derive(Default)]
pub struct Entry {
    pub shell: Option<std::ffi::CString>,
    pub home: Option<std::ffi::CString>,
    pub name: Option<std::ffi::CString>,
}

/// Errors produced by [`get`].
//
// PORT NOTE (S112-F5): the reference's inferred set carries each failing
// call's own error through its `try`: `Pty.open`, `FlatpakHostCommand.spawn`
// and `.wait`, and `posix.read`. The first three were all reported as
// `OutOfMemory`; they now keep their errors (`PtyOpen`, `HostCommand`).
#[derive(Debug)]
pub enum Error {
    /// An OS error encountered while shelling out in the flatpak path
    /// (`posix.read` on the pty master).
    Os(i32),
    /// Allocation/format error in the flatpak path.
    OutOfMemory,
    /// `try Pty.open(.{})` in the flatpak path.
    #[cfg(all(feature = "flatpak", not(windows)))]
    PtyOpen(crate::pty::PosixPtyError),
    /// `try cmd.spawn(alloc)` / `try cmd.wait()` in the flatpak path.
    #[cfg(feature = "flatpak")]
    HostCommand(crate::flatpak::Error),
}

// PORT NOTE (S112-F5): the reference gates the flatpak host-command block on
// `build_config.flatpak` alone; the port also needs this crate's `flatpak`
// feature, which compiles the real `FlatpakHostCommand` in. The two must never
// differ (rustty-termio's `flatpak` feature turns both on, and
// rustty-build-config's Cargo.toml says so): a build with
// `build_config::flatpak()` true but no host command would silently skip the
// block, neither logging "this build doesn't contain flatpak support" nor
// asking the host. This makes that build fail instead.
#[cfg(all(not(windows), not(feature = "flatpak")))]
const _: () = assert!(
    !build_config::flatpak(),
    "rustty-build-config's `flatpak` needs rustty-os's `flatpak` feature (enable rustty-termio/flatpak)"
);

/// Get the passwd entry for the currently executing user.
#[cfg(not(windows))]
pub fn get() -> Result<Entry, Error> {
    let mut buf: [u8; 1024] = [0; 1024];
    let mut pw: c::passwd = unsafe { std::mem::zeroed() };
    let mut pw_ptr: *mut c::passwd = std::ptr::null_mut();
    let res = unsafe {
        c::getpwuid_r(
            c::getuid(),
            &mut pw,
            buf.as_mut_ptr().cast(),
            buf.len(),
            &mut pw_ptr,
        )
    };
    if res != 0 {
        log_warn(&format!("error retrieving pw entry code={res}"));
        return Ok(Entry::default());
    }

    if pw_ptr.is_null() {
        // Future: let's check if a better shell is available like zsh
        log_warn("no pw entry to detect default shell, will default to 'sh'");
        return Ok(Entry::default());
    }

    let mut result: Entry = Entry::default();

    // If we're in flatpak then our entry is always empty so we grab it
    // by shelling out to the host. note that we do HAVE an entry in the
    // sandbox but only the username is correct.
    if is_flatpak() {
        'flatpak: {
            if !build_config::flatpak() {
                log_warn("flatpak detected, but this build doesn't contain flatpak support");
                break 'flatpak;
            }

            #[cfg(feature = "flatpak")]
            {
                log_info("flatpak detected, will use host command to get our entry");

                // Note: we wrap our getent call in a /bin/sh login shell because
                // some operating systems (NixOS tested) don't set the PATH for various
                // utilities properly until we get a login shell.
                use crate::pty::Pty;
                // var pty = try Pty.open(.{});
                // defer pty.deinit();
                //
                // PORT NOTE (S112-F5): the `defer` closes the pty on EVERY exit,
                // including the `try`s below. This called `deinit` only after the
                // read loop, so a failing spawn, wait or read leaked both fds; the
                // guard's `Drop` is the `defer`.
                struct PtyDeinit(Pty);
                impl Drop for PtyDeinit {
                    fn drop(&mut self) {
                        self.0.deinit();
                    }
                }
                let pty = PtyDeinit(Pty::open(Default::default()).map_err(Error::PtyOpen)?);
                let pty = &pty.0;
                // PORT NOTE: the reference is a struct literal with the remaining
                // fields defaulted. `FlatpakHostCommand`'s state fields are private
                // (they are the `state_mutex`-guarded internals), so the Rust map is
                // `Default::default()` + field assignment — same resulting value.
                let mut cmd = crate::flatpak::FlatpakHostCommand::default();
                cmd.argv = vec![
                    "/bin/sh".to_string(),
                    "-l".to_string(),
                    "-c".to_string(),
                    format!("getent passwd {}", slice_to_str(pw.pw_name),),
                ];
                cmd.stdin = pty.slave;
                cmd.stdout = pty.slave;
                cmd.stderr = pty.slave;
                cmd.spawn().map_err(Error::HostCommand)?;
                cmd.wait().map_err(Error::HostCommand)?;

                // Once started, we can close the child side. We do this after
                // wait right now but that is fine too. This lets us read the
                // parent and detect EOF.
                unsafe { libc::close(pty.slave) };

                // Read all of our output
                let output = {
                    let mut output: Vec<u8> = Vec::new();
                    loop {
                        let n =
                            unsafe { libc::read(pty.master, buf.as_mut_ptr().cast(), buf.len()) };
                        if n < 0 {
                            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                            match errno {
                                // EIO is triggered at the end since we closed our
                                // child side. This is just EOF for this. I'm not sure
                                // if I'm doing this wrong.
                                libc::EIO => break,
                                _ => return Err(Error::Os(errno)),
                            }
                        }
                        let n = n as usize;

                        output.extend_from_slice(&buf[0..n]);

                        // Max total size is buf.len. We can do better here by trimming
                        // the front and continuing reading but we choose to just exit.
                        if output.len() > buf.len() {
                            break;
                        }
                    }

                    output
                };

                // Shell and home are the last two entries
                let trimmed = trim_right(&output, b" \r\n");
                let mut it = trimmed.rsplit(|&c| c == b':');
                // PORT NOTE (task R13-P, N51; LEDGER S112-F5's third sub-defect):
                // the reference `alloc.dupeZ` copies the field with any NUL byte
                // in it, and every consumer passes the `[:0]` pointer to C (the
                // shell becomes an `execve` argv entry), which reads up to that
                // NUL. This was `.unwrap()`, which panicked on host output the
                // terminal does not control. Reaching it needs a NUL in the host
                // `getent passwd` output, which glibc's own C-string formatting
                // cannot produce, so this is a hardening fix, not a fixed crash.
                let to_first_nul = |v: &[u8]| {
                    let end = v.iter().position(|&b| b == 0).unwrap_or(v.len());
                    std::ffi::CString::new(&v[..end])
                        .expect("the bytes before the first NUL hold no NUL")
                };
                result.shell = it.next().map(to_first_nul);
                result.home = it.next().map(to_first_nul);
                return Ok(result);
            }
        }
    }

    if !pw.pw_shell.is_null() {
        let source = unsafe { std::ffi::CStr::from_ptr(pw.pw_shell) };
        let value = source.to_owned();
        result.shell = Some(value);
    }

    if !pw.pw_dir.is_null() {
        let source = unsafe { std::ffi::CStr::from_ptr(pw.pw_dir) };
        let value = source.to_owned();
        result.home = Some(value);
    }

    if !pw.pw_name.is_null() {
        let source = unsafe { std::ffi::CStr::from_ptr(pw.pw_name) };
        let value = source.to_owned();
        result.name = Some(value);
    }

    Ok(result)
}

// PORT NOTE: `if (builtin.os.tag == .windows) @compileError("passwd is not
// available on windows");` (`ghostty src/os/passwd.zig:36`). Zig's
// `@compileError` here is LAZY: it fires only when `get` is analyzed from a
// windows-reachable call site, and the reference's windows builds pass
// precisely because no such call site exists (`passwd.zig:20-24` even gates the
// `pwd.h` cImport off windows, and the file's `test {}` self-skips at `:148`).
// Rust's `compile_error!` is EAGER — a `#[cfg(windows)] fn get` whose body is
// `compile_error!` breaks EVERY windows build unconditionally, which is a
// strictly stronger (and non-faithful) rule than the reference's.
//
// The Rust construct with the reference's exact semantics is the ABSENCE of the
// item on windows: a windows call site is then a compile error AT THE CALL SITE
// (E0425), and a windows build with no call site compiles. A runtime
// `unreachable!()` body was considered and REJECTED — it would let a windows
// call site COMPILE, which is strictly weaker than the reference's guarantee.
// Precedent for mapping a windows-unreachable `@compileError` decl to absence:
// `rustty-xev/src/iocp_watcher.rs`. `Entry`/`Error` stay windows-visible,
// matching the reference's windows-visible `Entry` struct.
// See `.port/codex-vet/designs/W2D_WINDOWS_OS.md` WI-4.

// PORT NOTE: `std.mem.sliceTo(pw.pw_name, 0)` rendered as a `&str` for the flatpak
// host command formatting.
#[cfg(all(not(windows), feature = "flatpak"))]
fn slice_to_str(ptr: *mut std::ffi::c_char) -> String {
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(all(not(windows), feature = "flatpak"))]
fn trim_right<'a>(input: &'a [u8], cutset: &[u8]) -> &'a [u8] {
    let mut end = input.len();
    while end > 0 && cutset.contains(&input[end - 1]) {
        end -= 1;
    }
    &input[0..end]
}

// PARITY (deferred seam, R4): `internal_os.isFlatpak` re-exports
// `flatpak.zig`'s `isFlatpak`, whose module is still a stub in a different port
// group. To keep this module's behavior faithful and the build green,
// `is_flatpak` below reproduces `flatpak.zig`'s `isFlatpak` EXACTLY (verbatim
// logic + comment); it will be replaced by a re-export of
// `crate::flatpak::is_flatpak` when the flatpak group lands. No behavior is
// added or lost.
/// Returns true if we're running in a Flatpak environment.
fn is_flatpak() -> bool {
    // If we're not on Linux then we'll make this comptime false.
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::metadata("/.flatpak-info").is_ok()
    }
}

fn log_warn(msg: &str) {
    eprintln!("[warn] [{LOG_SCOPE}] {msg}");
}

#[cfg(all(not(windows), feature = "flatpak"))]
fn log_info(msg: &str) {
    eprintln!("[info] [{LOG_SCOPE}] {msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS getpwuid_r syscall
    #[cfg_attr(windows, ignore = "SkipZigTest on Windows (R14)")]
    fn passwd_entry() {
        // We should be able to get an entry
        let entry = get().unwrap();
        assert!(entry.shell.is_some());
        assert!(entry.home.is_some());
    }
}
