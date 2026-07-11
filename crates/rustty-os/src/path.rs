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

use std::path::Path;

/// Errors that can be produced by [`expand`].
#[derive(Debug, PartialEq, Eq)]
pub enum ExpandError {
    PathTooLong,
    AccessDenied,
    /// Any other OS error encountered while opening/statting a candidate path.
    Other(i32),
}

/// Search for "cmd" in the PATH and return the absolute path. This will
/// always allocate if there is a non-null result. The caller must free the
/// resulting value.
pub fn expand(cmd: &str) -> Result<Option<Vec<u8>>, ExpandError> {
    // If the command already contains a slash, then we return it as-is
    // because it is assumed to be absolute or relative.
    if cmd.as_bytes().contains(&b'/') {
        return Ok(Some(cmd.as_bytes().to_vec()));
    }

    // PORT NOTE: the Zig reference branches on `builtin.os.tag`: on Windows it reads
    // `PATH` via `getenvW` and converts UTF-16→UTF-8; everywhere else it uses
    // `posix.getenvZ("PATH")`. In Rust `std::env::var_os` covers both uniformly
    // (a missing variable maps to the reference's `orelse return null`).
    let path = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return Ok(None),
    };

    let mut path_buf: [u8; libc::PATH_MAX as usize] = [0; libc::PATH_MAX as usize];
    let mut seen_eacces = false;
    let path_bytes = os_str_as_bytes(&path);
    for search_path in tokenize_path(&path_bytes) {
        // We need enough space in our path buffer to store this
        let path_len = search_path.len() + cmd.len() + 1;
        if path_buf.len() < path_len {
            return Err(ExpandError::PathTooLong);
        }

        // Copy in the full path
        path_buf[0..search_path.len()].copy_from_slice(search_path);
        path_buf[search_path.len()] = PATH_SEP;
        path_buf[search_path.len() + 1..][0..cmd.len()].copy_from_slice(cmd.as_bytes());
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
        return true;
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
}

fn open_file(full_path: &[u8]) -> Result<std::fs::File, OpenError> {
    let path = Path::new(os_str_from_bytes(full_path));
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

#[cfg(not(windows))]
fn os_str_as_bytes(s: &std::ffi::OsStr) -> std::borrow::Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    std::borrow::Cow::Borrowed(s.as_bytes())
}

#[cfg(windows)]
fn os_str_as_bytes(s: &std::ffi::OsStr) -> std::borrow::Cow<'_, [u8]> {
    std::borrow::Cow::Owned(s.to_string_lossy().into_owned().into_bytes())
}

// (pub(crate): shared with `open.rs` — the `open(url: []const u8)` argv
// bytes take the same POSIX bytes-are-OsStr mapping.)
#[cfg(not(windows))]
pub(crate) fn os_str_from_bytes(b: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(b)
}

#[cfg(windows)]
pub(crate) fn os_str_from_bytes(b: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(b).unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    // `uname -n` is the *nix equivalent of `hostname.exe` on Windows
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS PATH lookup + openFile/stat syscalls
    fn expand_hostname() {
        let executable = if cfg!(windows) {
            "hostname.exe"
        } else {
            "uname"
        };
        let path = expand(executable).unwrap().unwrap();
        assert!(path.len() > executable.len());
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS PATH lookup + openFile/stat syscalls
    fn expand_does_not_exist() {
        let path = expand("thisreallyprobablydoesntexist123").unwrap();
        assert!(path.is_none());
    }

    #[test]
    fn expand_slash() {
        let path = expand("foo/env").unwrap().unwrap();
        assert!(path.len() == 7);
    }
}
