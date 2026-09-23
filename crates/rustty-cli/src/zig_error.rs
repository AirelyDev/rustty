//! rustty-cli::zig_error
//!
//! Zig prints an error value with `{}` as `error.<Name>` (MEASURED with Zig
//! 0.16.0: `std.debug.print("{}", .{error.FileNotFound})` prints
//! `error.FileNotFound`). Several CLI messages print a reference error that
//! way; these helpers give the port's error types the same rendering. Each
//! helper names the reference error set it reproduces.

use std::io;

/// `{}` of a `cli.args.parse` error (`args.Error` plus the `CommaSplitter`
/// errors a field can propagate). The variant names are the reference's.
pub fn args_error(e: &rustty_args::args::Error) -> String {
    format!("error.{e:?}")
}

/// The error name `std.Io.Dir.cwd().realPathFile(io, path, buf)` returns for
/// a failed path, as Zig 0.16.0 `Io/Threaded.zig` `dirRealPathFilePosix`
/// produces it in a libc-linked build with the cwd handle (the reference's
/// `new_tab`/`new_window` `checkArg`): `pathToPosix` first (an embedded NUL
/// is `BadPathName`, a path of `PATH_MAX` bytes or more is `NameTooLong`),
/// then libc `realpath`, whose errno maps as below; any other errno is
/// `posix.unexpectedErrno`, i.e. `error.Unexpected`.
pub fn realpath_error_name(path: &[u8], e: &io::Error) -> &'static str {
    // if (std.mem.containsAtLeastScalar2(u8, file_path, 0, 1)) return error.BadPathName;
    if path.contains(&0) {
        return "BadPathName";
    }
    // if (file_path.len >= buffer.len) return error.NameTooLong;
    #[cfg(unix)]
    if path.len() >= libc::PATH_MAX as usize {
        return "NameTooLong";
    }
    #[cfg(unix)]
    match e.raw_os_error() {
        Some(libc::EACCES) => "AccessDenied",
        Some(libc::ENOENT) => "FileNotFound",
        Some(libc::EOPNOTSUPP) => "OperationUnsupported",
        Some(libc::ENOTDIR) => "NotDir",
        Some(libc::ENAMETOOLONG) => "NameTooLong",
        Some(libc::ELOOP) => "SymLinkLoop",
        Some(libc::EIO) => "InputOutput",
        _ => "Unexpected",
    }
    #[cfg(not(unix))]
    {
        let _ = e;
        "Unexpected"
    }
}

/// `{}` of an error from `Config.loadFile` (the reference's `list_themes`
/// preview pane). Its reachable errors are `file_load.open`'s
/// `OpenFileError.{FileNotFound, FileIsEmpty, FileOpenFailed}` (`NotAFile` is
/// handled inside `loadFile`) and whatever `loadIter`'s parse propagates. The
/// port carries `FileIsEmpty`/`FileOpenFailed` as an `io::Error` whose message
/// is the variant name (`LoadError::from_open`).
//
// PORT NOTE: an `Io` error with any other message is a read failure after
// the file opened. The reference cannot surface one here (its `LineIterator`
// logs a failed read and stops, so `loadFile` returns normally); a Zig
// `std.Io.Reader` failure is `error.ReadFailed`, which is the name used.
pub fn load_error(e: &rustty_config::config_struct::LoadError) -> String {
    use rustty_config::config_struct::LoadError;
    match e {
        LoadError::FileNotFound => "error.FileNotFound".to_string(),
        LoadError::Io(io) => {
            let msg = io.to_string();
            match msg.as_str() {
                "FileIsEmpty" | "FileOpenFailed" | "NotAFile" => format!("error.{msg}"),
                _ => "error.ReadFailed".to_string(),
            }
        }
        LoadError::Parse(p) => args_error(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_error_renders_as_zig_error_value() {
        assert_eq!(
            "error.InvalidField",
            args_error(&rustty_args::args::Error::InvalidField)
        );
        assert_eq!(
            "error.UnclosedQuote",
            args_error(&rustty_args::args::Error::UnclosedQuote)
        );
    }

    // Each name is what `realPathFile` returns for the same condition
    // (MEASURED with Zig 0.16.0 on macOS, see the T-C task report).
    #[cfg(unix)]
    #[test]
    fn realpath_error_names_follow_the_reference_mapping() {
        let missing = b"/nonexistent-rustty-t-c/x";
        let e =
            std::fs::canonicalize(std::ffi::OsStr::new("/nonexistent-rustty-t-c/x")).unwrap_err();
        assert_eq!("FileNotFound", realpath_error_name(missing, &e));

        let notdir = b"/etc/hosts/x";
        let e = std::fs::canonicalize("/etc/hosts/x").unwrap_err();
        assert_eq!("NotDir", realpath_error_name(notdir, &e));

        let e = io::Error::from(io::ErrorKind::InvalidInput);
        assert_eq!("BadPathName", realpath_error_name(b"a\0b", &e));
    }
}
