//! rustty-os::file
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/file.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `posix = std.posix`, and
// `windows = @import("windows.zig")`. In Rust we use the standard library plus
// `libc` (R3: a binding to the SAME system library) for the `rlimit`
// get/setrlimit syscalls and the CSPRNG used by `randomBasename`.

#[cfg(not(windows))]
use std::ffi::c_int;

const LOG_SCOPE: &str = "os";

// PORT NOTE: `pub const rlimit = if (@hasDecl(posix.system, "rlimit")) posix.rlimit
// else struct {};` — on platforms without `rlimit` this is an empty struct. We
// reproduce that conditionally: on Unix it aliases `libc::rlimit`; elsewhere it
// is a zero-sized stand-in.
#[cfg(not(windows))]
#[allow(non_camel_case_types)]
pub type rlimit = libc::rlimit;
#[cfg(windows)]
#[derive(Clone, Copy)]
#[allow(non_camel_case_types)]
pub struct rlimit;

/// This maximizes the number of file descriptors we can have open. We
/// need to do this because each window consumes at least a handful of fds.
/// This is extracted from the Zig compiler source code.
#[allow(non_snake_case)]
pub fn fixMaxFiles() -> Option<rlimit> {
    #[cfg(windows)]
    {
        // `!@hasDecl(posix.system, "rlimit")` is true on Windows.
        None
    }
    #[cfg(not(windows))]
    {
        let old = match getrlimit_nofile() {
            Ok(old) => old,
            Err(_) => {
                log_warn("failed to query file handle limit, may limit max windows");
                return None; // Oh well; we tried.
            }
        };

        // If we're already at the max, we're done.
        if old.rlim_cur >= old.rlim_max {
            log_debug(&format!(
                "file handle limit already maximized value={}",
                old.rlim_cur
            ));
            return Some(old);
        }

        // Do a binary search for the limit.
        let mut lim = old;
        let mut min: rlim_t = lim.rlim_cur;
        let mut max: rlim_t = 1 << 20;
        // But if there's a defined upper bound, don't search, just set it.
        if lim.rlim_max != libc::RLIM_INFINITY {
            min = lim.rlim_max;
            max = lim.rlim_max;
        }

        loop {
            lim.rlim_cur = min + (max - min) / 2; // on freebsd rlim_t is signed
            if setrlimit_nofile(lim).is_ok() {
                min = lim.rlim_cur;
            } else {
                max = lim.rlim_cur;
            }
            if min + 1 >= max {
                break;
            }
        }

        log_debug(&format!("file handle limit raised value={}", lim.rlim_cur));
        Some(old)
    }
}

#[allow(non_snake_case)]
pub fn restoreMaxFiles(lim: rlimit) {
    #[cfg(windows)]
    {
        let _ = lim;
    }
    #[cfg(not(windows))]
    {
        let _ = setrlimit_nofile(lim);
    }
}

/// Return the recommended path for temporary files. Any trailing
/// path separator is stripped so callers can safely join with their
/// own separator (e.g. `"{tmp}/{name}"`).
///
/// On Windows this calls `GetTempPathW` and allocates a UTF-8 copy
/// (or duplicates a hard-fallback string if the syscall fails). On
/// POSIX this returns `$TMPDIR`/`$TMP` (or `"/tmp"` as a fallback)
/// without allocating. Always pair with `freeTmpDir` to release any
/// allocation.
//
// PORT NOTE (P3-W7 T3/T4 / area-E `E15` + `E20`): upstream's signature became
// `allocTmpDir(alloc, environ)` — the `TMPDIR`/`TMP` lookup reads the captured
// environment instead of the process environment — and `os/main.zig` STOPPED
// re-exporting `allocTmpDir`/`freeTmpDir` because `global.zig` became its only
// importer. rustty's equivalent of that single importer is
// `rustty_global::environ::set_tmp_dir_path`, which the app initializer feeds
// from here.
#[allow(non_snake_case)]
pub fn allocTmpDir(environ: &rustty_global::Environ) -> Vec<u8> {
    #[cfg(windows)]
    {
        let _ = environ;
        // GetTempPathW guarantees the result fits in MAX_PATH+1.
        let mut buf = [0u16; crate::windows::MAX_PATH + 1];
        let len = unsafe { windows_GetTempPathW(buf.len() as u32, buf.as_mut_ptr()) };
        if len > 0 {
            // Trim the UTF-16 string before encoding as UT8-8 so that the
            // returned slice's length matches its underlying allocation.
            let trimmed = trim_end_u16(&buf[0..len as usize], PATH_SEP_U16);
            match String::from_utf16(trimmed) {
                Ok(utf8) => return utf8.into_bytes(),
                Err(e) => log_warn(&format!(
                    "failed to convert temp dir path from windows string: {e}"
                )),
            }
        }
        b"C:\\Windows\\Temp".to_vec()
    }
    #[cfg(not(windows))]
    {
        // const tmpdir = environ.getPosix("TMPDIR") orelse environ.getPosix("TMP") orelse return "/tmp";
        // PORT NOTE (parity S111-F1): `orelse` falls through only on a MISSING
        // variable; a present-but-empty `TMPDIR` is returned as `""` and does
        // not fall through to `TMP`. (This used to filter out empty values.)
        let tmpdir = match environ
            .get_posix(b"TMPDIR")
            .or_else(|| environ.get_posix(b"TMP"))
        {
            Some(v) => v,
            None => return b"/tmp".to_vec(),
        };
        // return std.mem.trimEnd(u8, tmpdir, &.{std.fs.path.sep});
        // PORT NOTE (parity S111-F12): a POSIX path is bytes; the value is
        // returned byte-exact (this used to go through `String::from_utf8_lossy`,
        // which renamed a non-UTF-8 directory).
        //
        // PORT NOTE: reference defect (R13), UD211. `trimEnd` also strips the
        // root directory's only byte, so `TMPDIR=/` (or `//`) becomes `""`,
        // which is not a path: the reference's own `TempDir.init` then fails
        // with `FileNotFound` on `openDir("")` (measured with zig 0.16.0 on
        // macOS; `openDir("/")` succeeds). The doc comment above asks only
        // for a trailing separator to be stripped so callers can join with
        // their own. The port keeps the root: a value made only of
        // separators returns `/`. A present-but-empty value is not affected
        // and stays `""` (S111-F1: its intent is ambiguous, so it is
        // faithful). Same rule in `rustty-global`'s `posix_tmp_dir_path`.
        let trimmed = trim_end_bytes(tmpdir, PATH_SEP);
        if trimmed.is_empty() && !tmpdir.is_empty() {
            return vec![PATH_SEP];
        }
        trimmed.to_vec()
    }
}

/// Free a path returned by `allocTmpDir` if it allocated memory.
/// This is a no-op on POSIX.
#[allow(non_snake_case)]
pub fn freeTmpDir(_dir: &[u8]) {
    // PORT NOTE: in Rust the returned `String` owns its allocation on all platforms
    // and is freed by Drop; the reference frees only on Windows. The comment is
    // preserved verbatim above.
}

#[allow(non_upper_case_globals)]
const random_basename_bytes: usize = 16;

pub enum RandomBasenameError {
    BufferTooSmall,
}

// PORT NOTE: `std.base64.url_safe_no_pad.Encoder` — the URL-safe alphabet without
// padding. `calcSize` for N input bytes (no padding) is
// `N/3*4 + (N%3*8 + 5)/6`.
mod b64_encoder {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    // PARITY: this is the verbatim Zig `std.base64` no-pad `calcSize` formula.
    // clippy suggests `.div_ceil(6)` for the `(.. + 5) / 6` term, but rewriting
    // would diverge from the reference expression; kept as-is for fidelity.
    #[allow(clippy::manual_div_ceil)]
    pub const fn calc_size(source_len: usize) -> usize {
        source_len / 3 * 4 + (source_len % 3 * 8 + 5) / 6
    }

    /// Encode `source` into `dest` (which must be exactly `calc_size(source.len())`
    /// bytes) and return a slice over the written bytes.
    pub fn encode<'a>(dest: &'a mut [u8], source: &[u8]) -> &'a [u8] {
        let out_len = calc_size(source.len());
        let mut acc: u32 = 0;
        let mut acc_len: u32 = 0;
        let mut out_idx = 0;
        for &byte in source {
            acc = (acc << 8) | byte as u32;
            acc_len += 8;
            while acc_len >= 6 {
                acc_len -= 6;
                dest[out_idx] = ALPHABET[((acc >> acc_len) & 0x3f) as usize];
                out_idx += 1;
            }
        }
        if acc_len > 0 {
            dest[out_idx] = ALPHABET[((acc << (6 - acc_len)) & 0x3f) as usize];
            out_idx += 1;
        }
        debug_assert_eq!(out_idx, out_len);
        &dest[0..out_len]
    }
}

/// Length of the basename produced by `randomBasename`.
#[allow(non_upper_case_globals)]
pub const random_basename_len: usize = b64_encoder::calc_size(random_basename_bytes);

/// Write a random filesystem-safe base64 basename of length
/// `random_basename_len` into `buf` and return a slice over the
/// written bytes. Returns `error.BufferTooSmall` if `buf` is too
/// short.
#[allow(non_snake_case)]
pub fn randomBasename(buf: &mut [u8]) -> Result<&[u8], RandomBasenameError> {
    if buf.len() < random_basename_len {
        return Err(RandomBasenameError::BufferTooSmall);
    }
    let mut rand_buf = [0u8; random_basename_bytes];
    crypto_random_bytes(&mut rand_buf);
    Ok(b64_encoder::encode(
        &mut buf[0..random_basename_len],
        &rand_buf,
    ))
}

/// Return a freshly-allocated path of the form `{TMPDIR}/{prefix}{random}`.
/// The caller owns the returned slice and must free it with `allocator`.
///
/// Nothing is created on disk; this only builds the path string. Useful
/// for one-shot temporary file/socket paths where a full `TempDir` is
/// overkill.
#[allow(non_snake_case)]
pub fn randomTmpPath(prefix: &str) -> String {
    // PORT NOTE (P3-W7 T4 / area-E `E20`): this used to call
    // `allocTmpDir()` + `freeTmpDir()` on EVERY call. Upstream `e8525c0fd`
    // switched it to the temp directory discovered once at `global.init`:
    //   const tmp_dir = global.tmpDirPath();
    let tmp_dir = String::from_utf8_lossy(rustty_global::tmp_dir_path()).into_owned();
    let mut name_buf = [0u8; random_basename_len];
    let basename = randomBasename(&mut name_buf).unwrap_or_else(|_| unreachable!());
    // PORT NOTE: `std.fmt.allocPrint(.., "{s}{c}{s}{s}", .{ tmp_dir, sep, prefix, basename })`.
    let basename = std::str::from_utf8(basename).expect("base64 output is ASCII");
    format!("{tmp_dir}{}{prefix}{basename}", PATH_SEP as char)
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims for the std/posix facilities used above.
// ---------------------------------------------------------------------------

// `std.fs.path.sep`.
#[cfg(windows)]
const PATH_SEP: u8 = b'\\';
#[cfg(not(windows))]
const PATH_SEP: u8 = b'/';

#[cfg(not(windows))]
#[allow(non_camel_case_types)]
type rlim_t = libc::rlim_t;

#[cfg(not(windows))]
fn getrlimit_nofile() -> Result<rlimit, c_int> {
    let mut lim: rlimit = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(0));
    }
    Ok(lim)
}

#[cfg(not(windows))]
fn setrlimit_nofile(lim: rlimit) -> Result<(), c_int> {
    let rc = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lim) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(0));
    }
    Ok(())
}

// `std.crypto.random.bytes`: fill `buf` with cryptographically-secure random
// bytes from the OS CSPRNG.
//
// PORT NOTE (Rust-only glue, additive): `pub` — this is the shared
// `std.crypto.random` shim; rustty-core's `Surface.init` maps
// `std.crypto.random.int(u64)` (the surface id, Surface.zig:584) onto 8
// native-endian bytes from here.
pub fn crypto_random_bytes(buf: &mut [u8]) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // The libc `arc4random_buf` is the canonical macOS/BSD CSPRNG and
        // never fails.
        unsafe { libc::arc4random_buf(buf.as_mut_ptr().cast(), buf.len()) };
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", windows)))]
    {
        // POSIX `getentropy` (limited to 256 bytes per call; our buffers are
        // 16 bytes so a single call suffices).
        let rc = unsafe { libc::getentropy(buf.as_mut_ptr().cast(), buf.len()) };
        assert_eq!(rc, 0, "getentropy failed");
    }
    #[cfg(windows)]
    {
        let rc = unsafe { windows_RtlGenRandom(buf.as_mut_ptr(), buf.len() as u32) };
        assert_ne!(rc, 0, "RtlGenRandom failed");
    }
}

/// The platform has no CSPRNG source.
///
/// PORT NOTE (`dda8e6f31`): the reference spells this `error.EntropyUnavailable`
/// (`std.Io.RandomSecureError`); rustty-vt's `sys::RandomSecureError` is the
/// terminal-side enum and maps 1:1 onto this os-side half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntropyUnavailable;

// PORT NOTE (`dda8e6f31`, PORTS ghostty src/lib/TinyIo.zig `randomSecure` +
// its test): TinyIo itself is a NON-PORT (`.port/phase-4/NON_PORTS.md` — rustty
// has no `std.Io` model), but its `randomSecure` vtable entry is the PLATFORM
// ENTROPY DEFAULT behind rustty-vt's `sys::random_secure` override hook (the
// kitty clipboard OTP source), so that one entry is ported here os-side, next
// to the `crypto_random_bytes` CSPRNG glue this crate already wraps. The
// `_: ?*anyopaque` userdata parameter is the Zig vtable calling convention
// (TinyIo is stateless) and drops out.
//
// fn randomSecure(_: ?*anyopaque, buffer: []u8) Io.RandomSecureError!void {
pub fn random_secure(buffer: &mut [u8]) -> Result<(), EntropyUnavailable> {
    // if (buffer.len == 0) return;
    if buffer.is_empty() {
        return Ok(());
    }

    // The same sources as `std.Io.Threaded.randomSecure` minus
    // cancelation and the /dev/urandom fallback: arc4random_buf where
    // libc provides it (all the BSDs and Darwin, glibc 2.36+), otherwise
    // the getrandom syscall on Linux. Anything else has no entropy.
    //
    // PORT NOTE: the reference's `builtin.link_libc and
    // @TypeOf(posix.system.arc4random_buf) != void` comptime probe becomes a
    // target cfg. rustty's build matrix reaches the arc4random_buf arm on
    // Darwin (the `crypto_random_bytes` precedent above); the glibc-2.36+ case
    // collapses into the Linux getrandom arm below, because the Rust `libc`
    // floor cannot assume the symbol exists and getrandom is the same source
    // glibc's arc4random_buf draws from.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // posix.system.arc4random_buf(buffer.ptr, buffer.len);
        // return;
        //
        // SAFETY: `buffer` is a live, writable allocation of `buffer.len()`
        // bytes, the `arc4random_buf` contract; it never fails.
        unsafe { libc::arc4random_buf(buffer.as_mut_ptr().cast(), buffer.len()) };
        return Ok(());
    }

    // if (builtin.os.tag == .linux) {
    #[cfg(target_os = "linux")]
    {
        // const linux = std.os.linux;
        // var i: usize = 0;
        let mut i: usize = 0;
        // while (i < buffer.len) {
        while i < buffer.len() {
            // const rc = linux.getrandom(buffer[i..].ptr, buffer.len - i, 0);
            //
            // SAFETY: `buffer[i..]` is a live, writable region of
            // `buffer.len() - i` bytes, the `getrandom` contract; the kernel
            // writes at most that many bytes.
            let rc =
                unsafe { libc::getrandom(buffer[i..].as_mut_ptr().cast(), buffer.len() - i, 0) };
            // switch (linux.errno(rc)) {
            //     .SUCCESS => i += rc,
            //     .INTR => continue,
            //     else => return error.EntropyUnavailable,
            // }
            if rc >= 0 {
                i += rc as usize;
            } else {
                match std::io::Error::last_os_error().raw_os_error() {
                    Some(libc::EINTR) => continue,
                    _ => return Err(EntropyUnavailable),
                }
            }
        }
        // return;
        return Ok(());
    }

    // return error.EntropyUnavailable;
    //
    // PORT NOTE: reached on the targets where TinyIo is `!supported`
    // (`TinyIo.zig:35-38`: windows/wasi/freestanding — there its whole vtable
    // is `std.Io.failing`, so `randomSecure` fails the same way).
    #[allow(unreachable_code)]
    Err(EntropyUnavailable)
}

#[cfg(not(windows))]
fn trim_end_bytes(s: &[u8], byte: u8) -> &[u8] {
    let mut end = s.len();
    while end > 0 && s[end - 1] == byte {
        end -= 1;
    }
    &s[..end]
}

#[cfg(windows)]
const PATH_SEP_U16: u16 = PATH_SEP as u16;

#[cfg(windows)]
fn trim_end_u16(s: &[u16], byte: u16) -> &[u16] {
    let mut end = s.len();
    while end > 0 && s[end - 1] == byte {
        end -= 1;
    }
    &s[0..end]
}

#[cfg(windows)]
extern "system" {
    #[link_name = "GetTempPathW"]
    fn windows_GetTempPathW(buffer_length: u32, buffer: *mut u16) -> u32;
    #[link_name = "SystemFunction036"]
    fn windows_RtlGenRandom(buffer: *mut u8, length: u32) -> u8;
}

fn log_warn(msg: &str) {
    eprintln!("[warn] [{LOG_SCOPE}] {msg}");
}

#[cfg(not(windows))]
fn log_debug(msg: &str) {
    let _ = msg;
    #[cfg(debug_assertions)]
    eprintln!("[debug] [{LOG_SCOPE}] {msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P3-W7 T3/T4 (area-E `E15`/`E20`) — the two temp-dir discoveries must
    /// agree.
    ///
    /// `global.zig` imports `os/file.zig`'s `allocTmpDir` directly; rustty
    /// cannot (R4: `rustty-os` is ABOVE the leaf `rustty-global`), so
    /// `rustty_global::tmp_dir_path()` carries a fallback implementing the same
    /// rule for the case where nothing published a value — a `#[test]`, or a
    /// library consumer that never ran `init`. Two implementations of one rule
    /// is a divergence risk, so it is pinned by a test rather than by hope.
    #[test]
    #[allow(non_snake_case)]
    fn alloc_tmp_dir_agrees_with_global_fallback() {
        let from_os = allocTmpDir(&rustty_global::environ());
        let from_global = rustty_global::tmp_dir_path();
        assert_eq!(
            from_os.as_slice(),
            from_global,
            "rustty-os allocTmpDir and rustty-global's tmp_dir_path fallback diverged"
        );
    }

    /// R13 (UD211): the root directory survives the trailing-separator trim.
    /// The reference returns `""` for `TMPDIR=/`, which its `TempDir.init`
    /// cannot open. An empty value still comes back as `""` (S111-F1).
    #[test]
    #[cfg(not(windows))]
    #[allow(non_snake_case)]
    fn alloc_tmp_dir_keeps_the_root_directory() {
        let env = |entries: &[&[u8]]| {
            rustty_global::Environ::from_entries(entries.iter().map(|e| e.to_vec()).collect())
        };
        assert_eq!(allocTmpDir(&env(&[b"TMPDIR=/"])), b"/");
        assert_eq!(allocTmpDir(&env(&[b"TMPDIR=///"])), b"/");
        assert_eq!(allocTmpDir(&env(&[b"TMP=/"])), b"/");
        assert_eq!(allocTmpDir(&env(&[b"TMPDIR="])), b"");
        // The value is a directory the temp-dir code can open.
        let root = allocTmpDir(&env(&[b"TMPDIR=/"]));
        assert!(std::path::Path::new(std::str::from_utf8(&root).unwrap()).is_dir());
    }

    /// Parity S111-F1 / S111-F12: `allocTmpDir`'s POSIX arm is
    /// `getPosix("TMPDIR") orelse getPosix("TMP") orelse "/tmp"`, trimmed of
    /// trailing separators (`file.zig:85-86`). An empty `TMPDIR` is a value
    /// (`""`, with no fall-through to `TMP`), and a non-UTF-8 value comes back
    /// byte-exact.
    #[test]
    #[cfg(not(windows))]
    #[allow(non_snake_case)]
    fn alloc_tmp_dir_takes_empty_and_non_utf8_values_verbatim() {
        let env = |entries: &[&[u8]]| {
            rustty_global::Environ::from_entries(entries.iter().map(|e| e.to_vec()).collect())
        };
        assert_eq!(allocTmpDir(&env(&[b"TMPDIR="])), b"");
        assert_eq!(allocTmpDir(&env(&[b"TMPDIR=", b"TMP=/x"])), b"");
        assert_eq!(allocTmpDir(&env(&[b"TMP=/x//"])), b"/x");
        assert_eq!(allocTmpDir(&env(&[b"TMPDIR=/a\xff/"])), b"/a\xff");
        assert_eq!(allocTmpDir(&env(&[])), b"/tmp");
    }

    /// P3-W7 T4 (area-E `E20`) — `randomTmpPath` uses the CACHED temp dir and
    /// still produces a fresh, unguessable basename per call.
    ///
    /// `crypto_random_bytes` STAYS: upstream's switch to `global.io().random()`
    /// is a `std.Io` plumbing artifact, and the basename is what makes the temp
    /// path unguessable, so weakening it to a non-cryptographic RNG "for
    /// fidelity" would be a real security regression.
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS CSPRNG syscall
    #[allow(non_snake_case)]
    fn random_tmp_path_is_cached_and_fresh() {
        let a = randomTmpPath("w7-");
        let b = randomTmpPath("w7-");
        assert_ne!(a, b, "basename must differ per call");

        let dir = String::from_utf8_lossy(rustty_global::tmp_dir_path()).into_owned();
        let prefix = format!("{dir}{}w7-", PATH_SEP as char);
        assert!(a.starts_with(&prefix), "{a} does not start with {prefix}");
        assert_eq!(a.len(), prefix.len() + random_basename_len);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS CSPRNG syscall (getentropy/arc4random_buf)
    #[allow(non_snake_case)]
    fn random_basename() {
        let mut buf = [0u8; random_basename_len];
        let name = randomBasename(&mut buf).unwrap_or_else(|_| panic!());
        assert_eq!(random_basename_len, name.len());
        for &c in name {
            let ok = c.is_ascii_alphanumeric() || c == b'-' || c == b'_';
            assert!(ok);
        }

        let mut small = [0u8; random_basename_len - 1];
        assert!(matches!(
            randomBasename(&mut small),
            Err(RandomBasenameError::BufferTooSmall)
        ));
    }

    // PORTS: ghostty src/lib/TinyIo.zig `test "randomSecure fills with fresh
    // entropy"` (`dda8e6f31`).
    //
    // PORT NOTE: `if (comptime !supported) return error.SkipZigTest;` — TinyIo
    // is `!supported` on windows/wasi/freestanding (`TinyIo.zig:35-38`), so the
    // skip becomes the `cfg` gate. The reference draws through the vtable
    // (`tio.io()` / `test_io.randomSecure`); rustty has no `std.Io`, so the
    // calls go to the ported entry directly.
    // test "randomSecure fills with fresh entropy" {
    #[test]
    #[cfg(not(any(windows, target_family = "wasm")))]
    #[cfg_attr(miri, ignore)] // real-OS CSPRNG syscall (arc4random_buf/getrandom)
    fn random_secure_fills_with_fresh_entropy() {
        // var a: [32]u8 = @splat(0);
        // var b: [32]u8 = @splat(0);
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        // try test_io.randomSecure(&a);
        // try test_io.randomSecure(&b);
        random_secure(&mut a).unwrap();
        random_secure(&mut b).unwrap();

        // Non-zero and non-repeating. A zero fill is what `random` does
        // without a source, which would make every one-time password the
        // same; identical draws would mean the same thing.
        assert!(!a.iter().all(|&x| x == 0));
        assert!(!b.iter().all(|&x| x == 0));
        assert_ne!(a, b);

        // Zero-length is a no-op.
        random_secure(&mut a[0..0]).unwrap();
    }
}
