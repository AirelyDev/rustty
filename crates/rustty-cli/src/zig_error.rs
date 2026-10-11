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
    // Windows (task W-2): `dirRealPathFileWindows` (Zig 0.16.0
    // `std/Io/Threaded.zig:6073-6100`): `sliceToPrefixedFileW` (not WTF-8 is
    // `error.BadPathName`), then `OpenFile`'s NTSTATUS mapping
    // (`Threaded.zig:18868-18885`). The port's open is `CreateFileW`, which
    // reports the NTSTATUS as its Win32 error (`RtlNtStatusToDosError`); each
    // arm names the status the error comes from. CORRECTION: this arm returned
    // `Unexpected` for every error.
    #[cfg(windows)]
    {
        if rustty_os::path::os_string_from_bytes(path).is_none() {
            return "BadPathName";
        }
        match e.raw_os_error() {
            // OBJECT_NAME_NOT_FOUND, OBJECT_PATH_NOT_FOUND
            Some(2) | Some(3) => "FileNotFound",
            // OBJECT_NAME_INVALID
            Some(123) => "BadPathName",
            // BAD_NETWORK_PATH, BAD_NETWORK_NAME
            Some(53) | Some(67) => "NetworkNotFound",
            // NO_MEDIA_IN_DEVICE
            Some(21) => "NoDevice",
            // ACCESS_DENIED (5), USER_MAPPED_FILE (ERROR_USER_MAPPED_FILE, 1224)
            Some(5) | Some(1224) => "AccessDenied",
            // PIPE_BUSY. (PIPE_NOT_AVAILABLE, Zig's `NoDevice`, also reaches
            // `CreateFileW` callers as 231 and cannot be told apart here.)
            Some(231) => "PipeBusy",
            // OBJECT_NAME_COLLISION
            Some(80) | Some(183) => "PathAlreadyExists",
            // NOT_A_DIRECTORY
            Some(267) => "NotDir",
            // SHARING_VIOLATION (after Zig's retries)
            Some(32) => "FileBusy",
            // VIRUS_INFECTED, VIRUS_DELETED
            Some(225) | Some(226) => "AntivirusInterference",
            // A name past `PATH_MAX_WIDE` (`wToPrefixedFileW`)
            Some(206) => "NameTooLong",
            _ => "Unexpected",
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = e;
        "Unexpected"
    }
}

/// `{}` of an error from `Config.loadFile` (the reference's `list_themes`
/// preview pane): `rustty_config::zig_error::load_error`.
//
// PORT NOTE (S-15, NEW-FINDINGS N140 (b)): the mapping moved down to
// `rustty-config` so its C API can print the same `error.<Name>` form; this
// wrapper keeps the CLI's call sites unchanged.
pub fn load_error(e: &rustty_config::config_struct::LoadError) -> String {
    rustty_config::zig_error::load_error(e)
}

/// `{t}` of a `std.posix.E` value, i.e. `@tagName(errno)`: the reference's
/// `edit_config` prints an `execve` failure as `(E{t})`. The tables are
/// `@typeInfo(std.posix.E).@"enum".fields` (Darwin) and
/// `@typeInfo(std.os.linux.E).@"enum".fields` (the generic Linux table that
/// x86_64 and aarch64 use; a libc-linked Linux build's `std.c.E` is it), each
/// printed by a Zig 0.16.0 program on this host (see the T-C task report), in
/// declaration order. `None` for any other target, and for a value the enum
/// does not name: both enums are non-exhaustive, and `@tagName` of an unnamed
/// value is illegal behaviour in Zig, which a kernel-returned `execve` errno
/// never produces.
///
/// POSIX-only: its one caller is `edit_config`'s `execve` failure report, which
/// Windows does not compile (task W-2).
#[cfg(unix)]
pub fn errno_tag_name(code: i32) -> Option<&'static str> {
    #[cfg(target_vendor = "apple")]
    const E: &[(i32, &str)] = &[
        (0, "SUCCESS"),
        (1, "PERM"),
        (2, "NOENT"),
        (3, "SRCH"),
        (4, "INTR"),
        (5, "IO"),
        (6, "NXIO"),
        (7, "2BIG"),
        (8, "NOEXEC"),
        (9, "BADF"),
        (10, "CHILD"),
        (11, "DEADLK"),
        (12, "NOMEM"),
        (13, "ACCES"),
        (14, "FAULT"),
        (15, "NOTBLK"),
        (16, "BUSY"),
        (17, "EXIST"),
        (18, "XDEV"),
        (19, "NODEV"),
        (20, "NOTDIR"),
        (21, "ISDIR"),
        (22, "INVAL"),
        (23, "NFILE"),
        (24, "MFILE"),
        (25, "NOTTY"),
        (26, "TXTBSY"),
        (27, "FBIG"),
        (28, "NOSPC"),
        (29, "SPIPE"),
        (30, "ROFS"),
        (31, "MLINK"),
        (32, "PIPE"),
        (33, "DOM"),
        (34, "RANGE"),
        (35, "AGAIN"),
        (36, "INPROGRESS"),
        (37, "ALREADY"),
        (38, "NOTSOCK"),
        (39, "DESTADDRREQ"),
        (40, "MSGSIZE"),
        (41, "PROTOTYPE"),
        (42, "NOPROTOOPT"),
        (43, "PROTONOSUPPORT"),
        (44, "SOCKTNOSUPPORT"),
        (45, "OPNOTSUPP"),
        (46, "PFNOSUPPORT"),
        (47, "AFNOSUPPORT"),
        (48, "ADDRINUSE"),
        (49, "ADDRNOTAVAIL"),
        (50, "NETDOWN"),
        (51, "NETUNREACH"),
        (52, "NETRESET"),
        (53, "CONNABORTED"),
        (54, "CONNRESET"),
        (55, "NOBUFS"),
        (56, "ISCONN"),
        (57, "NOTCONN"),
        (58, "SHUTDOWN"),
        (59, "TOOMANYREFS"),
        (60, "TIMEDOUT"),
        (61, "CONNREFUSED"),
        (62, "LOOP"),
        (63, "NAMETOOLONG"),
        (64, "HOSTDOWN"),
        (65, "HOSTUNREACH"),
        (66, "NOTEMPTY"),
        (67, "PROCLIM"),
        (68, "USERS"),
        (69, "DQUOT"),
        (70, "STALE"),
        (71, "REMOTE"),
        (72, "BADRPC"),
        (73, "RPCMISMATCH"),
        (74, "PROGUNAVAIL"),
        (75, "PROGMISMATCH"),
        (76, "PROCUNAVAIL"),
        (77, "NOLCK"),
        (78, "NOSYS"),
        (79, "FTYPE"),
        (80, "AUTH"),
        (81, "NEEDAUTH"),
        (82, "PWROFF"),
        (83, "DEVERR"),
        (84, "OVERFLOW"),
        (85, "BADEXEC"),
        (86, "BADARCH"),
        (87, "SHLIBVERS"),
        (88, "BADMACHO"),
        (89, "CANCELED"),
        (90, "IDRM"),
        (91, "NOMSG"),
        (92, "ILSEQ"),
        (93, "NOATTR"),
        (94, "BADMSG"),
        (95, "MULTIHOP"),
        (96, "NODATA"),
        (97, "NOLINK"),
        (98, "NOSR"),
        (99, "NOSTR"),
        (100, "PROTO"),
        (101, "TIME"),
        (103, "NOPOLICY"),
        (104, "NOTRECOVERABLE"),
        (105, "OWNERDEAD"),
        (106, "QFULL"),
    ];
    #[cfg(target_os = "linux")]
    const E: &[(i32, &str)] = &[
        (0, "SUCCESS"),
        (1, "PERM"),
        (2, "NOENT"),
        (3, "SRCH"),
        (4, "INTR"),
        (5, "IO"),
        (6, "NXIO"),
        (7, "2BIG"),
        (8, "NOEXEC"),
        (9, "BADF"),
        (10, "CHILD"),
        (11, "AGAIN"),
        (12, "NOMEM"),
        (13, "ACCES"),
        (14, "FAULT"),
        (15, "NOTBLK"),
        (16, "BUSY"),
        (17, "EXIST"),
        (18, "XDEV"),
        (19, "NODEV"),
        (20, "NOTDIR"),
        (21, "ISDIR"),
        (22, "INVAL"),
        (23, "NFILE"),
        (24, "MFILE"),
        (25, "NOTTY"),
        (26, "TXTBSY"),
        (27, "FBIG"),
        (28, "NOSPC"),
        (29, "SPIPE"),
        (30, "ROFS"),
        (31, "MLINK"),
        (32, "PIPE"),
        (33, "DOM"),
        (34, "RANGE"),
        (35, "DEADLK"),
        (36, "NAMETOOLONG"),
        (37, "NOLCK"),
        (38, "NOSYS"),
        (39, "NOTEMPTY"),
        (40, "LOOP"),
        (42, "NOMSG"),
        (43, "IDRM"),
        (44, "CHRNG"),
        (45, "L2NSYNC"),
        (46, "L3HLT"),
        (47, "L3RST"),
        (48, "LNRNG"),
        (49, "UNATCH"),
        (50, "NOCSI"),
        (51, "L2HLT"),
        (52, "BADE"),
        (53, "BADR"),
        (54, "XFULL"),
        (55, "NOANO"),
        (56, "BADRQC"),
        (57, "BADSLT"),
        (59, "BFONT"),
        (60, "NOSTR"),
        (61, "NODATA"),
        (62, "TIME"),
        (63, "NOSR"),
        (64, "NONET"),
        (65, "NOPKG"),
        (66, "REMOTE"),
        (67, "NOLINK"),
        (68, "ADV"),
        (69, "SRMNT"),
        (70, "COMM"),
        (71, "PROTO"),
        (72, "MULTIHOP"),
        (73, "DOTDOT"),
        (74, "BADMSG"),
        (75, "OVERFLOW"),
        (76, "NOTUNIQ"),
        (77, "BADFD"),
        (78, "REMCHG"),
        (79, "LIBACC"),
        (80, "LIBBAD"),
        (81, "LIBSCN"),
        (82, "LIBMAX"),
        (83, "LIBEXEC"),
        (84, "ILSEQ"),
        (85, "RESTART"),
        (86, "STRPIPE"),
        (87, "USERS"),
        (88, "NOTSOCK"),
        (89, "DESTADDRREQ"),
        (90, "MSGSIZE"),
        (91, "PROTOTYPE"),
        (92, "NOPROTOOPT"),
        (93, "PROTONOSUPPORT"),
        (94, "SOCKTNOSUPPORT"),
        (95, "OPNOTSUPP"),
        (96, "PFNOSUPPORT"),
        (97, "AFNOSUPPORT"),
        (98, "ADDRINUSE"),
        (99, "ADDRNOTAVAIL"),
        (100, "NETDOWN"),
        (101, "NETUNREACH"),
        (102, "NETRESET"),
        (103, "CONNABORTED"),
        (104, "CONNRESET"),
        (105, "NOBUFS"),
        (106, "ISCONN"),
        (107, "NOTCONN"),
        (108, "SHUTDOWN"),
        (109, "TOOMANYREFS"),
        (110, "TIMEDOUT"),
        (111, "CONNREFUSED"),
        (112, "HOSTDOWN"),
        (113, "HOSTUNREACH"),
        (114, "ALREADY"),
        (115, "INPROGRESS"),
        (116, "STALE"),
        (117, "UCLEAN"),
        (118, "NOTNAM"),
        (119, "NAVAIL"),
        (120, "ISNAM"),
        (121, "REMOTEIO"),
        (122, "DQUOT"),
        (123, "NOMEDIUM"),
        (124, "MEDIUMTYPE"),
        (125, "CANCELED"),
        (126, "NOKEY"),
        (127, "KEYEXPIRED"),
        (128, "KEYREVOKED"),
        (129, "KEYREJECTED"),
        (130, "OWNERDEAD"),
        (131, "NOTRECOVERABLE"),
        (132, "RFKILL"),
        (133, "HWPOISON"),
        (160, "NSRNODATA"),
        (161, "NSRFORMERR"),
        (162, "NSRSERVFAIL"),
        (163, "NSRNOTFOUND"),
        (164, "NSRNOTIMP"),
        (165, "NSRREFUSED"),
        (166, "NSRBADQUERY"),
        (167, "NSRBADNAME"),
        (168, "NSRBADFAMILY"),
        (169, "NSRBADRESP"),
        (170, "NSRCONNREFUSED"),
        (171, "NSRTIMEOUT"),
        (172, "NSROF"),
        (173, "NSRFILE"),
        (174, "NSRNOMEM"),
        (175, "NSRDESTRUCTION"),
        (176, "NSRQUERYDOMAINTOOLONG"),
        (177, "NSRCNAMELOOP"),
    ];
    // `std.posix.E` on FreeBSD (Zig 0.16.0 `std/c/freebsd.zig`), the same table
    // as `rustty-termio`'s `command.rs` (N559: this arm was missing, so the
    // CLI printed bare errno numbers on FreeBSD).
    #[cfg(target_os = "freebsd")]
    const E: &[(i32, &str)] = &[
        (0, "SUCCESS"),
        (1, "PERM"),
        (2, "NOENT"),
        (3, "SRCH"),
        (4, "INTR"),
        (5, "IO"),
        (6, "NXIO"),
        (7, "2BIG"),
        (8, "NOEXEC"),
        (9, "BADF"),
        (10, "CHILD"),
        (11, "DEADLK"),
        (12, "NOMEM"),
        (13, "ACCES"),
        (14, "FAULT"),
        (15, "NOTBLK"),
        (16, "BUSY"),
        (17, "EXIST"),
        (18, "XDEV"),
        (19, "NODEV"),
        (20, "NOTDIR"),
        (21, "ISDIR"),
        (22, "INVAL"),
        (23, "NFILE"),
        (24, "MFILE"),
        (25, "NOTTY"),
        (26, "TXTBSY"),
        (27, "FBIG"),
        (28, "NOSPC"),
        (29, "SPIPE"),
        (30, "ROFS"),
        (31, "MLINK"),
        (32, "PIPE"),
        (33, "DOM"),
        (34, "RANGE"),
        (35, "AGAIN"),
        (36, "INPROGRESS"),
        (37, "ALREADY"),
        (38, "NOTSOCK"),
        (39, "DESTADDRREQ"),
        (40, "MSGSIZE"),
        (41, "PROTOTYPE"),
        (42, "NOPROTOOPT"),
        (43, "PROTONOSUPPORT"),
        (44, "SOCKTNOSUPPORT"),
        (45, "OPNOTSUPP"),
        (46, "PFNOSUPPORT"),
        (47, "AFNOSUPPORT"),
        (48, "ADDRINUSE"),
        (49, "ADDRNOTAVAIL"),
        (50, "NETDOWN"),
        (51, "NETUNREACH"),
        (52, "NETRESET"),
        (53, "CONNABORTED"),
        (54, "CONNRESET"),
        (55, "NOBUFS"),
        (56, "ISCONN"),
        (57, "NOTCONN"),
        (58, "SHUTDOWN"),
        (59, "TOOMANYREFS"),
        (60, "TIMEDOUT"),
        (61, "CONNREFUSED"),
        (62, "LOOP"),
        (63, "NAMETOOLONG"),
        (64, "HOSTDOWN"),
        (65, "HOSTUNREACH"),
        (66, "NOTEMPTY"),
        (67, "PROCLIM"),
        (68, "USERS"),
        (69, "DQUOT"),
        (70, "STALE"),
        (71, "REMOTE"),
        (72, "BADRPC"),
        (73, "RPCMISMATCH"),
        (74, "PROGUNAVAIL"),
        (75, "PROGMISMATCH"),
        (76, "PROCUNAVAIL"),
        (77, "NOLCK"),
        (78, "NOSYS"),
        (79, "FTYPE"),
        (80, "AUTH"),
        (81, "NEEDAUTH"),
        (82, "IDRM"),
        (83, "NOMSG"),
        (84, "OVERFLOW"),
        (85, "CANCELED"),
        (86, "ILSEQ"),
        (87, "NOATTR"),
        (88, "DOOFUS"),
        (89, "BADMSG"),
        (90, "MULTIHOP"),
        (91, "NOLINK"),
        (92, "PROTO"),
        (93, "NOTCAPABLE"),
        (94, "CAPMODE"),
        (95, "NOTRECOVERABLE"),
        (96, "OWNERDEAD"),
        (97, "INTEGRITY"),
    ];
    #[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "freebsd")))]
    const E: &[(i32, &str)] = &[];

    E.iter().find(|(c, _)| *c == code).map(|(_, name)| *name)
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
    /// The name for a path through a regular file on Windows: NTFS answers the
    /// open with STATUS_OBJECT_PATH_NOT_FOUND (MEASURED on Windows 11, task W-2:
    /// `ERROR_PATH_NOT_FOUND`), which Zig maps to `error.FileNotFound`; POSIX
    /// gives `ENOTDIR` (`error.NotDir`).
    #[cfg(windows)]
    const WINDOWS_THROUGH_A_FILE: &str = "FileNotFound";

    // The Windows mapping on real paths (task W-2): a missing path, a path
    // through a regular file, and a byte string that is not WTF-8.
    #[cfg(windows)]
    #[test]
    fn realpath_error_names_follow_the_reference_mapping_on_windows() {
        let dir = std::env::temp_dir().join(format!("rustty-w2-realpath-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"x").unwrap();

        let missing = dir.join("nope").join("x");
        let e = std::fs::canonicalize(&missing).unwrap_err();
        let name = realpath_error_name(missing.as_os_str().as_encoded_bytes(), &e);
        let through = file.join("x");
        let e2 = std::fs::canonicalize(&through).unwrap_err();
        let name2 = realpath_error_name(through.as_os_str().as_encoded_bytes(), &e2);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!("FileNotFound", name, "{e:?}");
        assert_eq!(WINDOWS_THROUGH_A_FILE, name2, "{e2:?}");
        let e = io::Error::from(io::ErrorKind::InvalidInput);
        // An unpaired continuation byte is not WTF-8.
        assert_eq!("BadPathName", realpath_error_name(b"C:\\a\x80", &e));
    }

    #[cfg(unix)]
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-cli, parity tail C-1): `realpath` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
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
