//! rustty-os::homedir
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/homedir.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `passwd = @import("passwd.zig")`,
// `posix = std.posix`, and `objc`. In Rust we use the ported `passwd` sibling
// module, the standard library, and `objc2`/`objc2-foundation` (R3: bindings to
// the SAME Objective-C runtime/Foundation framework the reference uses).

// PORT NOTE (W2D_WINDOWS_OS WI-4): EXTENDS the existing macOS exclusion rather
// than replacing it. `passwd::get` has no windows image (the reference's
// `@compileError` mapped to absence), and `homeWindows` reads
// `USERPROFILE`/`GetTempPathW` and never touches passwd — so on windows this
// import would be unused. The macOS arm of the gate must stay: `home()`'s
// passwd block below is cfg'd out there too.
#[cfg(not(any(target_os = "macos", windows)))]
use crate::passwd;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The buffer used for output is not large enough to store the value.
    BufferTooSmall,
    // PORT NOTE (parity S111-F11): the two variants below are the part of
    // `home`'s INFERRED error set that `try std.process.run(...)` contributes
    // (`std.process.RunError`, zig 0.16.0 `std/process.zig:454-456`). They are
    // reached only from the shell fallback.
    /// `error.StreamTooLong`: the shell's stdout or stderr exceeded its limit.
    StreamTooLong,
    /// A `SpawnError` (or a read/wait failure) running the shell.
    Spawn(std::io::ErrorKind),
}

/// Determine the home directory for the currently executing user. This
/// is generally an expensive process so the value should be cached.
//
// PORT NOTE (P3-W7 T3 / area-E `E15`): upstream gained an explicit
// `environ_map: *const std.process.Environ.Map` first parameter so `HOME` (and
// `HOMEDRIVE`/`HOMEPATH` on Windows) is read from a caller-supplied map rather
// than the process environment. See `desktop.rs` for why that matters more in
// Rust than in Zig.
#[inline]
pub fn home<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        homeUnix(environ_map, buf)
    }

    #[cfg(windows)]
    {
        homeWindows(environ_map, buf)
    }

    // iOS doesn't have a user-writable home directory
    #[cfg(target_os = "ios")]
    {
        let _ = (environ_map, buf);
        Ok(None)
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "macos",
        windows,
        target_os = "ios"
    )))]
    {
        compile_error!("unimplemented")
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
#[allow(non_snake_case)]
// PARITY: the reference's `homeUnix` is a sequence of early `return`s; on macOS
// the last reachable `return` becomes a tail expression after the non-macOS
// allocation path is `#[cfg]`-excluded, which clippy flags. The explicit
// `return` is kept to mirror the reference control flow verbatim.
#[allow(clippy::needless_return)]
fn homeUnix<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {
    // First: if we have a HOME env var, then we use that.
    if let Some(result) = environ_map.get(&b"HOME"[..]) {
        if buf.len() < result.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[0..result.len()].copy_from_slice(result);
        let n = result.len();
        return Ok(Some(&buf[0..n]));
    }

    // On macOS: [NSFileManager defaultManager].homeDirectoryForCurrentUser.path
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::NSFileManager;

        let manager = NSFileManager::defaultManager();
        let home_url = manager.homeDirectoryForCurrentUser();
        let home_path = home_url.path();

        let result_string = home_path.expect("homeDirectoryForCurrentUser path");
        let c_str = result_string.UTF8String();
        let result = unsafe { std::ffi::CStr::from_ptr(c_str) }.to_bytes();

        if buf.len() < result.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[0..result.len()].copy_from_slice(result);
        let n = result.len();
        return Ok(Some(&buf[0..n]));
    }

    // Everything below here will require some allocation
    // PORT NOTE: the reference uses a `FixedBufferAllocator` over a 1024-byte stack
    // buffer. In Rust `passwd.get`/`Command` allocate via the global allocator;
    // no separate fixed buffer is needed to preserve behavior.

    // We try passwd. This doesn't work on multi-user mac but we try it anyways.
    #[cfg(not(target_os = "macos"))]
    {
        let pw = passwd::get().map_err(|_| Error::BufferTooSmall)?;
        if let Some(result) = pw.home {
            let result = result.as_bytes();
            if buf.len() < result.len() {
                return Err(Error::BufferTooSmall);
            }
            buf[0..result.len()].copy_from_slice(result);
            let n = result.len();
            return Ok(Some(&buf[0..n]));
        }

        // If all else fails, have the shell tell us...
        home_from_shell(environ_map, buf)
    }
}

/// The shell arm of `homeUnix`:
///
/// ```text
/// fba.reset();
/// const run = try std.process.run(fba.allocator(), io, .{
///     .argv = &[_][]const u8{ "/bin/sh", "-c", "cd && pwd" },
///     .environ_map = environ_map,
///     .stdout_limit = .limited(fba.buffer.len / 2),
///     .stderr_limit = .limited(fba.buffer.len / 2),
/// });
/// ```
///
/// PORT NOTE (P4-E33, `15c50c1db`): upstream's `std.process.run` gained
/// `.environ_map = environ_map` — the child's environment IS the
/// caller-supplied snapshot (not the live process environment), so this
/// fallback cannot race a concurrent `setenv`. `env_clear` + `envs` is
/// the Rust image of a full environment replacement.
///
/// PORT NOTE (parity S111-F11): `fba.buffer.len / 2` is 512 for each stream,
/// and `run` returns `error.StreamTooLong` as soon as either stream holds more
/// than that; the `try` makes that, and a spawn failure, an error from `home`.
/// Each pipe is read to at most `limit + 1` bytes; past that the shell is
/// killed and `StreamTooLong` returned. CORRECTION: this used to call
/// `Command::output()` (unbounded) and turn a spawn failure into `Ok(None)`.
/// `.stdin = .ignore` in `run` is `Stdio::null()`.
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
#[cfg_attr(target_os = "macos", allow(dead_code))] // tested on macOS; reached on Linux/FreeBSD
fn home_from_shell<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {
    use std::io::Read as _;
    use std::os::unix::ffi::OsStrExt;
    use std::process::Stdio;

    // var tempBuf: [1024]u8 = undefined; ... .limited(fba.buffer.len / 2)
    const LIMIT: u64 = 1024 / 2;

    let mut child = std::process::Command::new("/bin/sh")
        .args(["-c", "cd && pwd"])
        .env_clear()
        .envs(environ_map.iter().map(|(k, v)| {
            (
                std::ffi::OsStr::from_bytes(k),
                std::ffi::OsStr::from_bytes(v),
            )
        }))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Spawn(e.kind()))?;

    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    // Both pipes are drained at once, as `MultiReader` does, so neither can
    // fill up and stall the shell while the other is read.
    let err_reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        stderr.take(LIMIT + 1).read_to_end(&mut out).map(|_| out)
    });
    let mut out = Vec::new();
    let out_res = stdout.take(LIMIT + 1).read_to_end(&mut out);
    let err_res = err_reader.join().expect("stderr reader thread");

    let too_long = out.len() as u64 > LIMIT || matches!(&err_res, Ok(e) if e.len() as u64 > LIMIT);
    if too_long {
        // defer child.kill(io);
        let _ = child.kill();
        let _ = child.wait();
        return Err(Error::StreamTooLong);
    }
    if let Err(e) = out_res.and(err_res.map(|_| ())) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Error::Spawn(e.kind()));
    }
    let status = child.wait().map_err(|e| Error::Spawn(e.kind()))?;

    // if (run.term == .exited and run.term.exited == 0) {
    if status.success() {
        let result = trimSpace(&out);
        if buf.len() < result.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[0..result.len()].copy_from_slice(result);
        let n = result.len();
        return Ok(Some(&buf[0..n]));
    }

    Ok(None)
}

#[cfg(windows)]
#[allow(non_snake_case)]
fn homeWindows<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {
    // PORT NOTE: the reference uses a `std.heap.FixedBufferAllocator.init(buf)` and
    // allocates `HOMEDRIVE` (then `HOMEPATH`) into it via
    // `std.process.getEnvVarOwned`, so the env var bytes are written *in place*
    // starting at `buf.ptr`. `getEnvVarOwned` maps `error.OutOfMemory` (the FBA
    // ran out of room) to `Error.BufferTooSmall`, and
    // `error.InvalidWtf8`/`error.EnvironmentVariableNotFound` to `null`. We
    // reproduce that in-place fill with `fixed_buffer_env` below, which writes the
    // var into the head of the given buffer slice and returns the written
    // sub-slice (whose `.as_ptr()` equals the slice's `.as_ptr()`), preserving the
    // reference's pointer invariant for the `@panic("codebug")` guards.
    let buf_ptr = buf.as_ptr();

    let drive_len = {
        let drive = match fixed_buffer_env(environ_map, buf, b"HOMEDRIVE") {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(_) => return Err(Error::BufferTooSmall),
        };
        // could shift the contents if this ever happens
        if drive.as_ptr() != buf_ptr {
            panic!("codebug");
        }
        drive.len()
    };

    let path_len = {
        let path_buf_ptr = buf[drive_len..].as_ptr();
        let homepath = match fixed_buffer_env(environ_map, &mut buf[drive_len..], b"HOMEPATH") {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(_) => return Err(Error::BufferTooSmall),
        };
        // could shift the contents if this ever happens
        if homepath.as_ptr() != path_buf_ptr {
            panic!("codebug");
        }
        homepath.len()
    };

    Ok(Some(&buf[0..drive_len + path_len]))
}

// PORT NOTE: the equivalent of `std.process.getEnvVarOwned(fba, key)` where `fba` is a
// `FixedBufferAllocator` over `dst`: writes the value of the environment variable
// `key` into the head of `dst` in place, returning the written sub-slice. Returns
// `Ok(None)` for a missing variable (`error.EnvironmentVariableNotFound`) and
// `Err(())` when the value does not fit in `dst` (`error.OutOfMemory`). The
// reference also folds `error.InvalidWtf8` into `null`; Windows env values are
// WTF-16 and `var_os` always yields an `OsString`, so the invalid-WTF8 case maps
// to a successful read here.
#[cfg(windows)]
fn fixed_buffer_env<'a>(
    environ_map: &rustty_global::EnvMap,
    dst: &'a mut [u8],
    key: &[u8],
) -> Result<Option<&'a [u8]>, ()> {
    let bytes: &[u8] = match environ_map.get(key) {
        Some(v) => v,
        None => return Ok(None),
    };
    if dst.len() < bytes.len() {
        return Err(());
    }
    dst[0..bytes.len()].copy_from_slice(bytes);
    Ok(Some(&dst[0..bytes.len()]))
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
#[cfg_attr(target_os = "macos", allow(dead_code))]
#[allow(non_snake_case)]
fn trimSpace(input: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = input.len();
    let is_trim = |c: u8| c == b' ' || c == b'\n' || c == b'\t';
    while start < end && is_trim(input[start]) {
        start += 1;
    }
    while end > start && is_trim(input[end - 1]) {
        end -= 1;
    }
    &input[start..end]
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExpandError {
    HomeDetectionFailed,
    BufferTooSmall,
}

/// Expands a path that starts with a tilde (~) to the home directory of
/// the current user.
///
/// Errors if `home` fails or if the size of the expanded path is larger
/// than `buf.len`.
#[allow(non_snake_case)]
pub fn expandHome<'a>(
    environ_map: &rustty_global::EnvMap,
    path: &'a [u8],
    buf: &'a mut [u8],
) -> Result<&'a [u8], ExpandError> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        expandHomeUnix(environ_map, path, buf)
    }

    // `~/` is not an idiom generally used on Windows
    #[cfg(windows)]
    {
        let _ = (environ_map, buf);
        Ok(path)
    }

    // iOS doesn't have a user-writable home directory
    #[cfg(target_os = "ios")]
    {
        let _ = (environ_map, buf);
        Ok(path)
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "macos",
        windows,
        target_os = "ios"
    )))]
    {
        compile_error!("unimplemented")
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
#[allow(non_snake_case)]
fn expandHomeUnix<'a>(
    environ_map: &rustty_global::EnvMap,
    path: &'a [u8],
    buf: &'a mut [u8],
) -> Result<&'a [u8], ExpandError> {
    if !path.starts_with(b"~/") {
        return Ok(path);
    }
    // PORT NOTE: the reference does `const home_dir = if (home(buf)) |home_| home_
    // orelse return error.HomeDetectionFailed else |_| return
    // error.HomeDetectionFailed;`. `home` writes into `buf`, so we compute the
    // home length first, then operate on `buf` directly.
    let home_dir_len = match home(environ_map, buf) {
        Ok(Some(home_)) => home_.len(),
        Ok(None) => return Err(ExpandError::HomeDetectionFailed),
        Err(_) => return Err(ExpandError::HomeDetectionFailed),
    };
    let rest = &path[1..]; // Skip the ~
    let expanded_len = home_dir_len + rest.len();

    if expanded_len > buf.len() {
        return Err(ExpandError::BufferTooSmall);
    }
    // PORT NOTE: `@memcpy(buf[home_dir.len..expanded_len], rest);` — copy `rest`
    // after the home dir already written into `buf` by `home`. We copy from a
    // local snapshot of `rest` to avoid aliasing `buf` with itself.
    let rest_owned = rest.to_vec();
    buf[home_dir_len..expanded_len].copy_from_slice(&rest_owned);

    Ok(&buf[0..expanded_len])
}

// PORT NOTE (P3-W7 T3): the `posix_getenv` shim is GONE — `HOME` is read from
// the caller-supplied `EnvMap` now, matching upstream after `e8525c0fd`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // env/passwd/objc home lookup hits real OS state
    #[allow(non_snake_case)]
    #[cfg_attr(windows, ignore = "SkipZigTest on Windows (R14)")]
    fn expand_home_unix() {
        // PORT NOTE (P3-W7 T3): upstream's test now builds the map from
        // `testing.environ`; rustty's equivalent is the captured process
        // environment, and the `crate::env_test_lock()` serialization is gone
        // because no test mutates the process environment any more.
        let environ_map = rustty_global::environ_map();
        let mut buf = [0u8; libc::PATH_MAX as usize];
        let home_dir = expandHomeUnix(&environ_map, b"~/", &mut buf)
            .unwrap()
            .to_vec();
        // Joining the home directory `~` with the path `/`
        // the result should end with a separator here. (e.g. `/home/user/`)
        assert!(home_dir[home_dir.len() - 1] == SEP);

        let downloads = expandHomeUnix(&environ_map, b"~/Downloads/shader.glsl", &mut buf)
            .unwrap()
            .to_vec();
        let mut expected_downloads = home_dir.clone();
        expected_downloads.extend_from_slice(b"Downloads/shader.glsl");
        assert_eq!(expected_downloads, downloads);

        assert_eq!(
            b"~".as_slice(),
            expandHomeUnix(&environ_map, b"~", &mut buf).unwrap()
        );
        assert_eq!(
            b"~abc/".as_slice(),
            expandHomeUnix(&environ_map, b"~abc/", &mut buf).unwrap()
        );
        assert_eq!(
            b"/home/user".as_slice(),
            expandHomeUnix(&environ_map, b"/home/user", &mut buf).unwrap()
        );
        assert_eq!(
            b"".as_slice(),
            expandHomeUnix(&environ_map, b"", &mut buf).unwrap()
        );

        // Expect an error if the buffer is large enough to hold the home directory,
        // but not the expanded path
        let mut small_buf = vec![0u8; home_dir.len()];
        assert_eq!(
            Err(ExpandError::BufferTooSmall),
            expandHomeUnix(&environ_map, b"~/Downloads", &mut small_buf[..])
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)] // env/passwd/objc home lookup hits real OS state
    fn home_returns_value() {
        let environ_map = rustty_global::environ_map();
        let mut buf = [0u8; 1024];
        let result = home(&environ_map, &mut buf).unwrap();
        assert!(result.is_some());
        assert!(!result.unwrap().is_empty());
    }

    /// A fresh directory under the system temp dir, removed on drop.
    #[cfg(unix)]
    struct ScratchDir(std::path::PathBuf);

    #[cfg(unix)]
    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    fn scratch_dir(tag: &str) -> ScratchDir {
        let p = std::env::temp_dir().join(format!("rustty-t-h-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        ScratchDir(p.canonicalize().unwrap())
    }

    #[cfg(unix)]
    fn env_with_home(home: &[u8]) -> rustty_global::EnvMap {
        let mut m = rustty_global::EnvMap::new();
        m.insert(b"HOME".to_vec(), home.to_vec());
        m
    }

    /// Parity S111-F11: the shell fallback caps stdout and stderr at 512 bytes
    /// each (`fba.buffer.len / 2`) and fails with `StreamTooLong` past that,
    /// which `try` propagates out of `home`. Positive control first: a short
    /// home comes back.
    #[test]
    #[cfg(unix)]
    #[cfg_attr(miri, ignore)] // spawns /bin/sh
    fn home_from_shell_limits_stdout_and_stderr_to_512_bytes() {
        let dir = scratch_dir("home-shell");
        let short = dir.0.as_os_str().as_encoded_bytes().to_vec();
        assert!(
            short.len() < 400,
            "temp dir too long for the positive control"
        );
        let mut buf = [0u8; 4096];
        assert_eq!(
            home_from_shell(&env_with_home(&short), &mut buf),
            Ok(Some(&short[..]))
        );

        // A real directory whose path is over 512 bytes: `pwd` prints it on
        // stdout.
        let mut long = dir.0.clone();
        while long.as_os_str().len() <= 600 {
            long.push("d".repeat(100));
        }
        std::fs::create_dir_all(&long).unwrap();
        let long = long.as_os_str().as_encoded_bytes().to_vec();
        assert_eq!(
            home_from_shell(&env_with_home(&long), &mut buf),
            Err(Error::StreamTooLong)
        );

        // A missing directory with a name over 512 bytes: `cd` fails and
        // prints the name on stderr, nothing reaches stdout.
        let mut missing = short.clone();
        missing.extend_from_slice(b"/");
        missing.extend(std::iter::repeat_n(b'm', 200));
        missing.extend_from_slice(b"/");
        missing.extend(std::iter::repeat_n(b'n', 200));
        missing.extend_from_slice(b"/");
        missing.extend(std::iter::repeat_n(b'o', 200));
        assert_eq!(
            home_from_shell(&env_with_home(&missing), &mut buf),
            Err(Error::StreamTooLong)
        );
    }

    // `std.fs.path.sep`.
    #[cfg(windows)]
    const SEP: u8 = b'\\';
    #[cfg(not(windows))]
    const SEP: u8 = b'/';
}
