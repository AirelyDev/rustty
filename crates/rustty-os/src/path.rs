//! rustty-os::path
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/path.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `Allocator = std.mem.Allocator`,
// and `testing = std.testing`. In Rust we use the standard library plus `libc`
// (R3: a binding to the SAME system library) for `std.fs.max_path_bytes`
// (`PATH_MAX`) and the executable permission bits.

#[cfg(not(windows))]
use std::path::Path;

/// Errors that can be produced by [`expand`].
#[derive(Debug, PartialEq, Eq)]
pub enum ExpandError {
    PathTooLong,
    AccessDenied,
    /// Any other OS error encountered while opening/statting a candidate path.
    Other(i32),
    /// Windows only: `error.InvalidWtf8`, a `PATH` entry plus `cmd` that is
    /// not WTF-8 and so cannot be converted to a WTF-16 path.
    #[cfg(windows)]
    InvalidWtf8,
}

/// Search for "cmd" in the PATH and return the absolute path. This will
/// always allocate if there is a non-null result. The caller must free the
/// resulting value.
//
// PORT NOTE (P3-W7 T3 / area-E `E15`): upstream's signature became
// `expand(io, alloc, environ_map, cmd)` — the PATH lookup reads a
// caller-supplied map instead of the process environment. `io`/`alloc` are
// `std.Io`/allocator plumbing with no Rust counterpart; the `environ_map`
// parameter is the semantic half and is ported.
//
// PORT NOTE (S112-F4): `cmd` is `[]const u8` in the reference and `&[u8]` here;
// it was `&str`, which cannot name a command that is not UTF-8.
pub fn expand(
    environ_map: &rustty_global::EnvMap,
    cmd: &[u8],
) -> Result<Option<Vec<u8>>, ExpandError> {
    // If the command already contains a slash, then we return it as-is
    // because it is assumed to be absolute or relative.
    if cmd.contains(&b'/') {
        return Ok(Some(cmd.to_vec()));
    }

    // PORT NOTE: the Zig reference branched on `builtin.os.tag`: on Windows it read
    // `PATH` via `getenvW` and converted UTF-16→UTF-8; everywhere else it used
    // `posix.getenvZ("PATH")`. Upstream `e8525c0fd` collapsed both into a single
    // `environ_map.get("PATH")` (a missing variable maps to the reference's
    // `orelse return null`), which is what this reads now.
    let path_bytes: &[u8] = match environ_map.get(&b"PATH"[..]) {
        Some(p) => p,
        None => return Ok(None),
    };

    let mut path_buf: [u8; MAX_PATH_BYTES] = [0; MAX_PATH_BYTES];
    let mut seen_eacces = false;
    for search_path in tokenize_path(path_bytes) {
        // We need enough space in our path buffer to store this
        let path_len = search_path.len() + cmd.len() + 1;
        if path_buf.len() < path_len {
            return Err(ExpandError::PathTooLong);
        }

        // Copy in the full path
        path_buf[0..search_path.len()].copy_from_slice(search_path);
        path_buf[search_path.len()] = PATH_SEP;
        path_buf[search_path.len() + 1..][0..cmd.len()].copy_from_slice(cmd);
        path_buf[path_len] = 0;
        let full_path = &path_buf[0..path_len];

        // Stat it
        let f = match open_file(full_path) {
            Ok(f) => f,
            Err(err) => match err {
                OpenError::FileNotFound => continue,
                OpenError::AccessDenied => {
                    // Accumulate this and return it later so we can try other
                    // paths that we have access to.
                    seen_eacces = true;
                    continue;
                }
                OpenError::Other(code) => return Err(ExpandError::Other(code)),
                #[cfg(windows)]
                OpenError::InvalidWtf8 => return Err(ExpandError::InvalidWtf8),
            },
        };
        let stat = stat_file(&f).map_err(ExpandError::Other)?;
        if stat.kind != FileKind::Directory && isExecutable(stat.mode) {
            return Ok(Some(full_path.to_vec()));
        }
    }

    if seen_eacces {
        return Err(ExpandError::AccessDenied);
    }

    Ok(None)
}

#[allow(non_snake_case)]
fn isExecutable(mode: Mode) -> bool {
    #[cfg(windows)]
    {
        let _ = mode;
        true
    }
    #[cfg(not(windows))]
    {
        mode & 0o0111 != 0
    }
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims for the std types used by `expand`.
//
// The reference relies on `std.fs` (`cwd().openFile`, `File.stat`,
// `File.Mode`, `File.Kind`), `std.fs.path.{sep,delimiter}`, and
// `std.fs.max_path_bytes`. These are reproduced over `libc`/`std::fs` so the
// logic and error mapping match the reference exactly (R3 binding, not a
// reimplementation).
// ---------------------------------------------------------------------------

// `std.fs.max_path_bytes` (zig std/fs.zig): `posix.PATH_MAX` on the unix-ish
// tags, and on windows `windows.PATH_MAX_WIDE * 3 + 1` — each WTF-16LE code
// unit may expand to 3 WTF-8 bytes (a surrogate pair needing 4 bytes is
// (over)accounted as 3+3), +1 for the trailing NUL. `PATH_MAX_WIDE` is 32767
// (zig std/os/windows.zig), so the windows value is 98302. The reference's
// `expand` stack-allocates exactly this many bytes on every target
// (`ghostty src/os/path.zig:23`), including windows.
#[cfg(unix)]
pub const MAX_PATH_BYTES: usize = libc::PATH_MAX as usize;
#[cfg(windows)]
pub const MAX_PATH_BYTES: usize = 32767 * 3 + 1;

// `std.fs.path.sep`: '/' on POSIX, '\\' on Windows.
#[cfg(windows)]
const PATH_SEP: u8 = b'\\';
#[cfg(not(windows))]
const PATH_SEP: u8 = b'/';

// `std.fs.File.Mode`.
type Mode = u32;

#[derive(PartialEq, Eq)]
enum FileKind {
    Directory,
    Other,
}

struct Stat {
    kind: FileKind,
    mode: Mode,
}

enum OpenError {
    FileNotFound,
    AccessDenied,
    Other(i32),
    /// Windows only: the candidate path is not WTF-8 (`error.InvalidWtf8`).
    #[cfg(windows)]
    InvalidWtf8,
}

fn open_file(full_path: &[u8]) -> Result<std::fs::File, OpenError> {
    #[cfg(not(windows))]
    let path = Path::new(os_str_from_bytes(full_path));
    #[cfg(windows)]
    let path = match os_string_from_bytes(full_path) {
        Some(p) => std::path::PathBuf::from(p),
        // error.InvalidWtf8 (not FileNotFound/AccessDenied: `expand` returns it)
        None => return Err(OpenError::InvalidWtf8),
    };
    match std::fs::File::open(path) {
        Ok(file) => Ok(file),
        Err(e) => Err(match e.kind() {
            std::io::ErrorKind::NotFound => OpenError::FileNotFound,
            std::io::ErrorKind::PermissionDenied => OpenError::AccessDenied,
            _ => OpenError::Other(e.raw_os_error().unwrap_or(0)),
        }),
    }
}

fn stat_file(f: &std::fs::File) -> Result<Stat, i32> {
    let md = f.metadata().map_err(|e| e.raw_os_error().unwrap_or(0))?;
    let kind = if md.is_dir() {
        FileKind::Directory
    } else {
        FileKind::Other
    };
    #[cfg(not(windows))]
    let mode = {
        use std::os::unix::fs::MetadataExt;
        md.mode()
    };
    #[cfg(windows)]
    let mode: Mode = 0;
    Ok(Stat { kind, mode })
}

// `std.mem.tokenizeScalar(u8, PATH, delimiter)`: split on the PATH-list
// delimiter (';' on Windows, ':' elsewhere) skipping empty tokens.
fn tokenize_path(path: &[u8]) -> impl Iterator<Item = &[u8]> {
    #[cfg(windows)]
    const DELIM: u8 = b';';
    #[cfg(not(windows))]
    const DELIM: u8 = b':';
    path.split(|&c| c == DELIM).filter(|s| !s.is_empty())
}

// PORT NOTE (P3-W7 T3): the `os_str_as_bytes` shim is GONE — `PATH` comes from
// the caller-supplied byte-typed `EnvMap` now, so there is no `OsStr` to
// convert (and no lossy Windows conversion to make).

// (pub(crate): shared with `open.rs` — the `open(url: []const u8)` argv
// bytes take the same POSIX bytes-are-OsStr mapping.)
#[cfg(not(windows))]
pub(crate) fn os_str_from_bytes(b: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(b)
}

/// Zig's `realpath` (`std.Io.Dir.realPath*`, `std.posix.realpath`): the
/// canonical absolute path. On Windows Zig opens the path and asks
/// `GetFinalPathNameByHandle(.{ .volume_name = .Dos })`, which gives the DOS form
/// (`C:\..`, `\\server\share\..`), where `std::fs::canonicalize` gives the
/// verbatim `\\?\` form; this converts it (`tempdir::dos_form`).
///
/// Task W-2: the realpath sites that print, compare or store the result (the
/// CLI's `--working-directory`, the kitty temp-file check, the config loader)
/// used `std::fs::canonicalize` directly and so kept the verbatim prefix on
/// Windows.
pub fn realpath(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    let canonical = crate::tempdir::dos_form(canonical);
    Ok(canonical)
}

/// Windows: `[]const u8` paths and argv are WTF-8 in Zig, converted to WTF-16
/// when handed to the OS (`std.unicode.wtf8ToWtf16Le`), and a byte string that
/// is not WTF-8 fails with `error.InvalidWtf8`. `None` is that error.
//
// PORT NOTE (S112-F4): this was `OsStr::new(str::from_utf8(b).unwrap_or(""))`,
// which turned a WTF-8 path (an unpaired surrogate, as Windows file names may
// hold) into `""`, so `expand` opened `""` and skipped a real candidate.
//
// (pub: the renderer's shader-file open takes the same conversion.)
#[cfg(windows)]
pub fn os_string_from_bytes(b: &[u8]) -> Option<std::ffi::OsString> {
    use std::os::windows::ffi::OsStringExt;
    Some(std::ffi::OsString::from_wide(&wtf8_to_wtf16le(b)?))
}

// PORT NOTE: `std.fs.path.dirnamePosix` is Zig stdlib and pure, so it lives in
// `rustty_zigstd::fs_path` (compiled on every target, including wasm32, where
// `i18n` also reaches it) and is re-exported here for `resourcesdir`.
#[cfg_attr(windows, allow(unused_imports))]
pub(crate) use rustty_zigstd::fs_path::dirname_posix;

/// zig 0.16.0 `std.unicode.wtf8ToWtf16Le` (`std/unicode.zig:1812`): UTF-8 with
/// surrogate code points U+D800..U+DFFF allowed as 3-byte sequences (paired
/// or not; `wtf8ValidateSlice` "does not check that all surrogate halves are
/// unpaired"), each surrogate becoming one code unit. `None` is
/// `error.InvalidWtf8`: an illegal lead byte, a bad continuation byte, a
/// truncated sequence, an overlong form, or a value above U+10FFFF.
#[cfg(any(windows, test))]
fn wtf8_to_wtf16le(wtf8: &[u8]) -> Option<Vec<u16>> {
    let mut out: Vec<u16> = Vec::with_capacity(wtf8.len());
    let mut i = 0;
    while i < wtf8.len() {
        let b0 = wtf8[i];
        let (len, init, min) = match b0 {
            0x00..=0x7F => {
                out.push(u16::from(b0));
                i += 1;
                continue;
            }
            0xC2..=0xDF => (2, u32::from(b0 & 0x1F), 0x80),
            0xE0..=0xEF => (3, u32::from(b0 & 0x0F), 0x800),
            0xF0..=0xF4 => (4, u32::from(b0 & 0x07), 0x1_0000),
            _ => return None,
        };
        let seq = wtf8.get(i..i + len)?;
        let mut cp = init;
        for &b in &seq[1..] {
            if b & 0xC0 != 0x80 {
                return None;
            }
            cp = (cp << 6) | u32::from(b & 0x3F);
        }
        if cp < min || cp > 0x10_FFFF {
            return None;
        }
        if cp >= 0x1_0000 {
            let v = cp - 0x1_0000;
            out.push(0xD800 | (v >> 10) as u16);
            out.push(0xDC00 | (v & 0x3FF) as u16);
        } else {
            out.push(cp as u16);
        }
        i += len;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `std.fs.max_path_bytes` shim (not a reference `test {}` — a guard on
    /// a shim this port had to spell out per-target). On unix it must BE the
    /// platform `PATH_MAX`, which is what `std.fs.max_path_bytes` resolves to
    /// (`posix.PATH_MAX`); on windows it must be Zig's WTF-8 worst case
    /// `PATH_MAX_WIDE * 3 + 1` = 98302. Compiled per-target: the unix arm runs
    /// on the macOS host and the Linux VM, the windows arm on any future
    /// windows test lane.
    #[test]
    fn max_path_bytes_matches_reference() {
        #[cfg(unix)]
        assert_eq!(MAX_PATH_BYTES, libc::PATH_MAX as usize);
        #[cfg(windows)]
        assert_eq!(MAX_PATH_BYTES, 98302);
    }

    // `uname -n` is the *nix equivalent of `hostname.exe` on Windows
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS PATH lookup + openFile/stat syscalls
    fn expand_hostname() {
        let executable = if cfg!(windows) {
            "hostname.exe"
        } else {
            "uname"
        };
        let path = expand(&rustty_global::environ_map(), executable.as_bytes())
            .unwrap()
            .unwrap();
        assert!(path.len() > executable.len());
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS PATH lookup + openFile/stat syscalls
    fn expand_does_not_exist() {
        let path = expand(
            &rustty_global::environ_map(),
            b"thisreallyprobablydoesntexist123",
        )
        .unwrap();
        assert!(path.is_none());
    }

    /// S112-F4: `cmd` is bytes, as in the reference; a command that is not
    /// UTF-8 is handled like any other (here, returned as-is for its `/`).
    #[test]
    fn expand_takes_a_non_utf8_command() {
        let path = expand(&rustty_global::environ_map(), b"./bin/\xff-cmd")
            .unwrap()
            .unwrap();
        assert_eq!(path, b"./bin/\xff-cmd");
    }

    /// The rows of zig 0.16.0 `std/unicode.zig`'s `testValidateWtf8Slice`
    /// (`:1648-1678`), verbatim: `wtf8_to_wtf16le` accepts exactly the
    /// strings `wtf8ValidateSlice` accepts. S112-F4's Windows conversion.
    #[test]
    fn wtf8_to_wtf16le_accepts_what_wtf8_validate_slice_accepts() {
        let valid = |s: &[u8]| wtf8_to_wtf16le(s).is_some();
        // These are valid/invalid under both UTF-8 and WTF-8 rules.
        assert!(valid(b"abc"));
        assert!(valid(b"abc\xdf\xbf"));
        assert!(valid(b""));
        assert!(valid(b"a"));
        assert!(valid(b"abc"));
        assert!(valid("Ж".as_bytes()));
        assert!(valid("ЖЖ".as_bytes()));
        assert!(valid("брэд-ЛГТМ".as_bytes()));
        assert!(valid("☺☻☹".as_bytes()));
        assert!(valid("a\u{fffdb}".as_bytes()));
        assert!(valid(b"\xf4\x8f\xbf\xbf"));
        assert!(valid(b"abc\xdf\xbf"));

        assert!(!valid(b"abc\xc0"));
        assert!(!valid(b"abc\xc0abc"));
        assert!(!valid(b"aa\xe2"));
        assert!(!valid(b"\x42\xfa"));
        assert!(!valid(b"\x42\xfa\x43"));
        assert!(!valid(b"abc\xc0"));
        assert!(!valid(b"abc\xc0abc"));
        assert!(!valid(b"\xf4\x90\x80\x80"));
        assert!(!valid(b"\xf7\xbf\xbf\xbf"));
        assert!(!valid(b"\xfb\xbf\xbf\xbf\xbf"));
        assert!(!valid(b"\xc0\x80"));

        // But surrogate codepoints are only valid in WTF-8.
        assert!(valid(b"\xed\xa0\x80"));
        assert!(valid(b"\xed\xbf\xbf"));
    }

    /// The code units `wtf8_to_wtf16le` produces, each measured with zig
    /// 0.16.0 `std.unicode.wtf8ToWtf16Le`. The last row is an overlong 3-byte
    /// form: Zig's validator lets it through in WTF-8 mode and its decoder then
    /// hits `catch unreachable` (UD201); the port reports it as invalid.
    #[test]
    fn wtf8_to_wtf16le_matches_zig() {
        let f = wtf8_to_wtf16le;
        assert_eq!(f(b"a\xed\xa0\x80b"), Some(vec![97, 0xD800, 98]));
        assert_eq!(f(b"\xed\xa0\xbd\xed\xb2\xa9"), Some(vec![0xD83D, 0xDCA9]));
        assert_eq!(f(b"\xf0\x9f\x92\xa9"), Some(vec![0xD83D, 0xDCA9]));
        assert_eq!(f(b"\xd0\x96"), Some(vec![1046]));
        assert_eq!(f(b"\xed\xa0"), None);
        assert_eq!(f(b"\xe0\x9f\xbf"), None);
    }

    /// UD201 / S112-F4, Windows: a `PATH` entry holding the overlong form
    /// `E0 9F BF`, which Zig's WTF-8 validator accepts and its decoder then
    /// traps on (`catch unreachable`), makes `expand` fail with
    /// `error.InvalidWtf8` through the conversion `openFile` does.
    #[cfg(windows)]
    #[test]
    fn expand_reports_invalid_wtf8_for_an_overlong_path_entry() {
        let mut env = rustty_global::EnvMap::new();
        env.insert(b"PATH".to_vec(), b"C:\\bad\xe0\x9f\xbf".to_vec());
        assert!(matches!(
            expand(&env, b"hostname.exe"),
            Err(ExpandError::InvalidWtf8)
        ));
    }

    #[test]
    fn expand_slash() {
        let path = expand(&rustty_global::environ_map(), b"foo/env")
            .unwrap()
            .unwrap();
        assert!(path.len() == 7);
    }
}
