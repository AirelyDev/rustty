//! rustty-config::zig_error
//!
//! PORT NOTE (Zig-std shim): the config diagnostics interpolate a failed
//! `std.Io` call's error with `{}`, which Zig renders as `error.<Name>`, and a
//! file's kind with `@tagName(stat.kind)`. Rust's `io::Error` has neither form
//! (its `Display` is `Permission denied (os error 13)`), so the diagnostics that
//! quote them (`Config.zig:4413-4448`, `path.zig:176-231`, `theme.zig:129-277`)
//! render through these helpers instead (LEDGER S74-F4, S78-F4, S78-F5).
//!
//! Each `std.Io` call maps errno to its own error set; the tables below are the
//! pinned zig 0.16.0 `std/Io/Threaded.zig` switches, one per call the config
//! code makes. An errno a table does not list is `posix.unexpectedErrno`, i.e.
//! `error.Unexpected`. MEASURED against zig 0.16.0 on macOS arm64
//! (`openFileAbsolute`): an unreadable file is `error.AccessDenied`, a path
//! through a regular file `error.NotDir`, a symlink loop `error.SymLinkLoop`, a
//! missing file `error.FileNotFound`; `/dev/null`'s kind is `character_device`,
//! a FIFO's `named_pipe`, a directory's `directory`.

use std::io;

/// Which `std.Io` call failed. Each has its own errno switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Call {
    /// `Dir.openFile` / `Dir.openFileAbsolute` (`dirOpenFilePosix`).
    OpenFile,
    /// `File.stat` (`fileStatPosix`; `fileStatLinux` on Linux).
    Stat,
    /// `Dir.realPathFile` on an open directory (`dirRealPathFilePosix`: the
    /// `openat` switch, then `realPathPosix`).
    RealPathFile,
}

/// `@errorName(err)` for an `io::Error` returned where the reference made
/// `call`.
pub(crate) fn error_name(err: &io::Error, call: Call) -> &'static str {
    #[cfg(unix)]
    if let Some(errno) = err.raw_os_error() {
        return errno_name(errno, call);
    }
    // No errno: an error Rust raised before the syscall. The one reachable
    // here is an interior NUL in the path, which Zig's `pathToPosix` reports as
    // `error.BadPathName`.
    #[cfg(not(unix))]
    let _ = call;
    match err.kind() {
        io::ErrorKind::InvalidInput => "BadPathName",
        io::ErrorKind::NotFound => "FileNotFound",
        io::ErrorKind::PermissionDenied => "AccessDenied",
        _ => "Unexpected",
    }
}

/// `{}` of the Zig error: `error.<Name>`.
pub(crate) fn fmt_error(err: &io::Error, call: Call) -> String {
    format!("error.{}", error_name(err, call))
}

#[cfg(unix)]
fn errno_name(e: i32, call: Call) -> &'static str {
    match call {
        // `dirOpenFilePosix`, and the `openat` step of `dirRealPathFilePosix`
        // (identical switch minus the lock/`TXTBSY`/`ROFS`/`AGAIN` arms, which
        // an `O_PATH`/read-only open without locking cannot return).
        Call::OpenFile | Call::RealPathFile => match e {
            libc::EINVAL | libc::EILSEQ => "BadPathName",
            libc::EACCES => "AccessDenied",
            libc::EFBIG | libc::EOVERFLOW => "FileTooBig",
            libc::EISDIR => "IsDir",
            libc::ELOOP => "SymLinkLoop",
            libc::EMFILE => "ProcessFdQuotaExceeded",
            libc::ENAMETOOLONG => "NameTooLong",
            libc::ENFILE => "SystemFdQuotaExceeded",
            libc::ENODEV | libc::ENXIO => "NoDevice",
            libc::ENOENT | libc::ESRCH => "FileNotFound",
            libc::ENOMEM => "SystemResources",
            libc::ENOSPC => "NoSpaceLeft",
            libc::ENOTDIR => "NotDir",
            libc::EPERM => "PermissionDenied",
            libc::EEXIST => "PathAlreadyExists",
            libc::EBUSY => "DeviceBusy",
            libc::EOPNOTSUPP if call == Call::OpenFile => "FileLocksUnsupported",
            libc::EAGAIN if call == Call::OpenFile => "WouldBlock",
            libc::ETXTBSY if call == Call::OpenFile => "FileBusy",
            libc::EROFS if call == Call::OpenFile => "ReadOnlyFileSystem",
            _ => "Unexpected",
        },
        // `fileStatPosix` (`ACCES` is `errnoBug` in `fileStatLinux`).
        Call::Stat => match e {
            libc::ENOMEM => "SystemResources",
            #[cfg(not(target_os = "linux"))]
            libc::EACCES => "AccessDenied",
            _ => "Unexpected",
        },
    }
}

/// `@tagName(stat.kind)` — `std.Io.File.Kind` from the mode's `S_IFMT` bits
/// (`statFromPosix` / `statxKind`). `whiteout`, `door` and `event_port` come
/// only from `readdir` or illumos, never from a `stat` on the targets here.
pub(crate) fn file_kind_tag_name(ft: &std::fs::FileType) -> &'static str {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if ft.is_block_device() {
            return "block_device";
        }
        if ft.is_char_device() {
            return "character_device";
        }
        if ft.is_fifo() {
            return "named_pipe";
        }
        if ft.is_socket() {
            return "unix_domain_socket";
        }
    }
    if ft.is_dir() {
        "directory"
    } else if ft.is_symlink() {
        "sym_link"
    } else if ft.is_file() {
        "file"
    } else {
        "unknown"
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn errno_names_follow_each_calls_switch() {
        let e = |errno| io::Error::from_raw_os_error(errno);
        assert_eq!("AccessDenied", error_name(&e(libc::EACCES), Call::OpenFile));
        assert_eq!("NotDir", error_name(&e(libc::ENOTDIR), Call::RealPathFile));
        assert_eq!(
            "FileNotFound",
            error_name(&e(libc::ENOENT), Call::RealPathFile)
        );
        assert_eq!("WouldBlock", error_name(&e(libc::EAGAIN), Call::OpenFile));
        // The `realPathFile` open has no `AGAIN` arm.
        assert_eq!(
            "Unexpected",
            error_name(&e(libc::EAGAIN), Call::RealPathFile)
        );
        assert_eq!("Unexpected", error_name(&e(libc::EIO), Call::OpenFile));
        assert_eq!("SystemResources", error_name(&e(libc::ENOMEM), Call::Stat));
        assert_eq!(
            "error.SymLinkLoop",
            fmt_error(&e(libc::ELOOP), Call::OpenFile)
        );
        // An interior NUL never reaches the kernel: Zig's `pathToPosix`.
        let nul = std::fs::File::open("a\0b").unwrap_err();
        assert_eq!("BadPathName", error_name(&nul, Call::OpenFile));
    }

    #[test]
    fn file_kind_tag_names() {
        let kind = |p: &str| file_kind_tag_name(&std::fs::metadata(p).unwrap().file_type());
        assert_eq!("character_device", kind("/dev/null"));
        assert_eq!("directory", kind("/"));
        let exe = std::env::current_exe().unwrap();
        assert_eq!(
            "file",
            file_kind_tag_name(&std::fs::metadata(exe).unwrap().file_type())
        );
    }
}
