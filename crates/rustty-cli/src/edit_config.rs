//! rustty-cli::edit_config
//!
//! PORTS: ghostty src/cli/edit_config.zig (0t) — the `+edit-config` action:
//! open the user config file in `$VISUAL`/`$EDITOR` via a `/bin/sh -c` exec.
//! Backed by rustty-config (`Config.load`, `preferredDefaultFilePath`) and
//! rustty-os (`getConfigEditCommand` — the `0a183c923` shared editor-command
//! resolver — and the POSIX exec). Byte-faithful (R1);
//! the editor / path bytes stay raw (R3). R2: the `ghostty +edit-config` binary
//! name, the "Ghostty configuration" brand, and the illustrative config paths are
//! retargeted to the Rustty product identity via rustty-identity. 0 tests.

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../quirks.zig").inlineAssert;
// const args = @import("args.zig");
use rustty_args::args;
// const Allocator = std.mem.Allocator;
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const configpkg = @import("../config.zig");
// const Config = configpkg.Config;
use rustty_config::file_load::preferred_default_file_path;
use rustty_config::Config;
// const internal_os = @import("../os/main.zig");
use rustty_os::edit::{DefaultEditor, Options as EditOptions};

use std::io::Write;

// pub const Options = struct {
#[derive(Default)]
pub struct Options {}

impl Options {
    // pub fn deinit(self: Options) void { _ = self; }
    pub fn deinit(&self) {
        let _ = self;
    }

    // /// Enables `-h` and `--help` to work.
    // pub fn help(self: Options) !void { _ = self; return Action.help_error; }
    //   -> the `args::Target::help`/`has_help_decl` hooks below.
}

impl args::Target for Options {
    // The reference `Options` has no non-`_` fields, so `parseIntoField` always
    // falls through to `return error.InvalidField`.
    fn parse_into_field(
        &mut self,
        _key: &[u8],
        _value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        // return error.InvalidField;
        Err(args::Error::InvalidField)
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    // pub fn help(self: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), args::ParseError>> {
        let _ = Action::Help;
        Some(Err(args::Error::ActionHelpRequested))
    }
}

/// The `edit-config` command opens the Rustty configuration file in the
/// editor specified by the `$VISUAL` or `$EDITOR` environment variables.
///
/// IMPORTANT: This command will not reload the configuration after
/// editing. You will need to manually reload the configuration using the
/// application menu, configured keybind, or by restarting Rustty. We
/// plan to auto-reload in the future, but Rustty isn't capable of
/// this yet.
///
/// The filepath opened is the default user-specific configuration
/// file, which is typically located at `$XDG_CONFIG_HOME/rustty/config.rustty`.
/// On macOS, this may also be located at
/// `~/Library/Application Support/io.airely.rustty/config.rustty`.
/// On macOS, whichever path exists and is non-empty will be prioritized,
/// prioritizing the Application Support directory if neither are
/// non-empty.
///
/// This command prefers the `$VISUAL` environment variable over `$EDITOR`,
/// if both are set. If neither are set, it will print an error
/// and exit.
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // Implementation note (by @mitchellh): I do proper memory cleanup
    // throughout this command, even though we plan on doing `exec`.
    // I do this out of good hygiene in case we ever change this to
    // not using `exec` anymore and because this command isn't performance
    // critical where setting up the defer cleanup is a problem.

    // var buffer: [1024]u8 = undefined;
    // var stderr_writer = std.fs.File.stderr().writer(&buffer);
    // const stderr = &stderr_writer.interface;
    let mut stderr = std::io::BufWriter::new(std::io::stderr().lock());

    // var opts: Options = .{};
    // defer opts.deinit();
    let mut opts = Options::default();

    {
        // var iter = try args.argsIterator(alloc);
        // defer iter.deinit();
        // try args.parse(Options, alloc, &opts, &iter);
        let mut iter = args::args_iterator()?;
        let result = args::parse(&mut opts, &mut iter);
        iter.deinit();
        result?;
    }

    // const result = runInner(alloc, stderr);
    let result = run_inner(&mut stderr);
    // Flushing *shouldn't* fail but...
    // stderr.flush() catch {};
    let _ = stderr.flush();

    opts.deinit();

    // return result;
    result
}

// fn runInner(alloc: Allocator, stderr: *std.Io.Writer) !u8 {
fn run_inner<We: Write>(stderr: &mut We) -> Result<u8, Error> {
    // We load the configuration once because that will write our
    // default configuration files to disk. We don't use the config.
    // var config = try Config.load(alloc);
    // defer config.deinit();
    //   PORT NOTE: loaded only for its disk side-effect; `_config` drops at scope
    //   end (the reference's `defer config.deinit()`), and Rustty's
    //   `Config::deinit` resets the arena flag only.
    let _config = Config::load()?;

    // Find the preferred path.
    // const path = try configpkg.preferredDefaultFilePath(alloc);
    // defer alloc.free(path);
    //   (W94-B02-01: `preferredDefaultFilePath` returns raw path bytes; every
    //   `{s}` site below writes them raw.)
    let path: Vec<u8> = preferred_default_file_path()?;

    // We don't currently support Windows because we use the exec syscall.
    // if (comptime builtin.os.tag == .windows) {
    #[cfg(windows)]
    {
        // try stderr.print(
        //     \\The `ghostty +edit-config` command is not supported on Windows.
        //     \\Please edit the configuration file manually at the following path:
        //     \\
        //     \\
        // , .{});
        //   R2: the `ghostty` binary name -> the Rustty product name.
        //   (`0a183c923` dropped the `{s}` path interpolation from this arm.)
        write!(
            stderr,
            "The `{app} +edit-config` command is not supported on Windows.\n\
             Please edit the configuration file manually at the following path:\n\
             \n\
             \n",
            app = rustty_identity::APP_NAME,
        )?;
        // return 1;
        return Ok(1);
    }

    // const command = internal_os.getConfigEditCommand(alloc, path, .{ .default_editor = .failure }) catch |err| {
    //     switch (err) {
    //         error.NoEditorConfigured => { ... return 1; },
    //         error.WriteFailed,
    //         error.OutOfMemory,
    //         => |e| return e,
    //     }
    // };
    // defer alloc.free(command);
    //
    // PORT NOTE (`0a183c923`): the VISUAL/EDITOR resolution + shell-escaped
    // command build moved into the shared `internal_os.getConfigEditCommand`
    // (rustty-os `edit.rs`, landed W1). Its rustty error set is only
    // `NoEditorConfigured` (the `WriteFailed`/`OutOfMemory` propagation arms
    // collapse — the allocating writer is infallible, the edit.rs Error PORT
    // NOTE), so the `catch` is a single-arm match. The returned bytes carry
    // the `toOwnedSliceSentinel(0)` trailing NUL (the edit.rs contract).
    let command: Vec<u8> = match rustty_os::getConfigEditCommand(
        &path,
        &EditOptions {
            default_editor: DefaultEditor::Failure,
        },
    ) {
        Ok(v) => v,
        Err(rustty_os::edit::Error::NoEditorConfigured) => {
            // try stderr.print(
            //     \\The $EDITOR or $VISUAL environment variable is not set or is empty.
            //     \\This environment variable is required to edit the Ghostty configuration
            //     \\via this CLI command.
            //     \\
            //     \\Please set the environment variable to your preferred terminal
            //     \\text editor and try again.
            //     \\
            //     \\If you prefer to edit the configuration file another way,
            //     \\you can find the configuration file at the following path:
            //     \\
            //     \\
            // , .{});
            //   R2: "Ghostty configuration" -> the Rustty product name
            //   (APP_NAME_TITLECASE; the oracle prints the capitalized product name).
            write!(
                stderr,
                "The $EDITOR or $VISUAL environment variable is not set or is empty.\n\
                 This environment variable is required to edit the {app} configuration\n\
                 via this CLI command.\n\
                 \n\
                 Please set the environment variable to your preferred terminal\n\
                 text editor and try again.\n\
                 \n\
                 If you prefer to edit the configuration file another way,\n\
                 you can find the configuration file at the following path:\n\
                 \n\
                 \n",
                app = rustty_identity::APP_NAME_TITLECASE,
            )?;

            // Output the path using the OSC8 sequence so that it is linked.
            // try stderr.print(
            //     "\x1b]8;;file://{s}\x1b\\{s}\x1b]8;;\x1b\\\n",
            //     .{ path, path },
            // );
            //   (W94-B02-01: `{s}` writes the RAW path bytes; segmented `write_all`
            //   keeps that byte-exact for a non-UTF-8 path.)
            stderr.write_all(b"\x1b]8;;file://")?;
            stderr.write_all(&path)?;
            stderr.write_all(b"\x1b\\")?;
            stderr.write_all(&path)?;
            stderr.write_all(b"\x1b]8;;\x1b\\\n")?;

            // return 1;
            return Ok(1);
        }
    };
    // We require libc because we want to use std.c.environ for envp
    // and not have to build that ourselves. We can remove this
    // limitation later but Ghostty already heavily requires libc
    // so this is not a big deal.
    // comptime assert(builtin.link_libc);
    //   PORT NOTE: Rust always links libc; the compile-time assertion holds
    //   trivially and needs no runtime check.
    //
    // (the reference hoists this assert ABOVE the config load at END STATE;
    // kept at the exec site — it is a comptime no-op either way.)

    #[cfg(unix)]
    {
        exec_editor(stderr, &command, &path)
    }
    #[cfg(not(unix))]
    {
        // Non-POSIX, non-Windows targets have no `exec` syscall; the reference
        // `comptime assert(builtin.link_libc)` + `execvpeZ` are POSIX-only.
        let _ = &command;
        unreachable!("edit-config requires a POSIX exec syscall")
    }
}

/// The tail of the reference's `runInner` from the `execve` on
/// (`edit_config.zig:129-157`): replace the process with `/bin/sh -c
/// <command>`, and if that returns, report the failure and return 1. A
/// separate function so the tests can drive both the exec and the failure
/// report; `run_inner` calls it and nothing else does.
///
/// `command` is `getConfigEditCommand`'s result, which carries the
/// `toOwnedSliceSentinel(0)` trailing NUL (the edit.rs contract).
#[cfg(unix)]
fn exec_editor<We: Write>(stderr: &mut We, command: &[u8], path: &[u8]) -> Result<u8, Error> {
    // The reference's `command` is a `[:0]const u8`: `command.ptr` is the C
    // string execve reads (up to its first NUL), and `{s}` prints the slice,
    // which excludes the sentinel.
    let command_bytes: &[u8] = match command.split_last() {
        Some((0, rest)) => rest,
        _ => command,
    };
    let command_z: std::borrow::Cow<'_, [u8]> = if command.last() == Some(&0) {
        std::borrow::Cow::Borrowed(command)
    } else {
        std::borrow::Cow::Owned([command, &[0]].concat())
    };

    // Run/replace process (using execve)
    // const argv = [_:null]?[*:0]const u8{
    //     "/bin/sh",
    //     "-c",
    //     command.ptr,
    // };
    let argv: [*const libc::c_char; 4] = [
        c"/bin/sh".as_ptr(),
        c"-c".as_ptr(),
        command_z.as_ptr().cast(),
        std::ptr::null(),
    ];
    // const envp = std.c.environ;
    let envp = c_environ();
    // const err = std.posix.errno(std.posix.system.execve(argv[0].?, &argv, envp));
    //   PORT NOTE (parity S67-F11): a raw execve(2), as the reference. The port
    //   used to call `std::process::Command::exec`, whose child setup resets
    //   SIGPIPE to SIG_DFL before exec, so the editor started with SIGPIPE at
    //   its default where the reference's editor inherits the SIG_IGN that
    //   `GlobalState.initSignals` installs (execve keeps ignored signals
    //   ignored). execve returns only on failure, so `rc` is -1 and errno is
    //   the reason (`std.posix.errno` of a libc call reads errno when rc is -1).
    // SAFETY: `argv` is a null-terminated array of NUL-terminated strings that
    // outlive the call (`command_z` lives to the end of this function), and
    // `envp` is the process's own environment block.
    let rc = unsafe { libc::execve(argv[0], argv.as_ptr(), envp) };
    let err = std::io::Error::last_os_error();
    debug_assert_eq!(-1, rc);

    // If we reached this point then exec failed.
    // try stderr.print(
    //     \\Failed to execute the editor (E{t}).
    //     \\
    //     \\This is usually due to the executable path not existing, invalid
    //     \\permissions, or the shell environment not being set up
    //     \\correctly.
    //     \\
    //     \\Command: {s}
    //     \\
    // , .{
    //     err,
    //     command,
    // });
    //   (parity S67-F12: `{s}` writes the RAW command bytes, like the path
    //   lines below; the port printed them through a lossy UTF-8 view.)
    write!(
        stderr,
        "Failed to execute the editor (E{err}).\n\
         \n\
         This is usually due to the executable path not existing, invalid\n\
         permissions, or the shell environment not being set up\n\
         correctly.\n\
         \n\
         Command: ",
        err = errno_suffix(&err),
    )?;
    stderr.write_all(command_bytes)?;
    stderr.write_all(b"\n")?;
    // Output the path using the OSC8 sequence so that it is linked.
    // try stderr.print(
    //     "Path: \x1b]8;;file://{s}\x1b\\{s}\x1b]8;;\x1b\\\n",
    //     .{ path, path },
    // );
    //   (segmented `write_all` keeps the raw path bytes byte-exact, the
    //   NoEditorConfigured arm's precedent.)
    stderr.write_all(b"Path: \x1b]8;;file://")?;
    stderr.write_all(path)?;
    stderr.write_all(b"\x1b\\")?;
    stderr.write_all(path)?;
    stderr.write_all(b"\x1b]8;;\x1b\\\n")?;
    // return 1;
    Ok(1)
}

/// PORT NOTE (Zig-std shim): `std.c.environ`, the libc environment block. On
/// Apple platforms the dylib-safe accessor is `_NSGetEnviron`; elsewhere the
/// global `environ` symbol (rustty-termio's `command.rs` shim).
#[cfg(unix)]
fn c_environ() -> *const *const libc::c_char {
    #[cfg(target_vendor = "apple")]
    // SAFETY: `_NSGetEnviron` returns a valid pointer to the process's
    // `environ` variable.
    unsafe {
        (*libc::_NSGetEnviron()).cast_const().cast()
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        extern "C" {
            static environ: *const *const libc::c_char;
        }
        // SAFETY: libc's `environ`; read once, nothing writes it concurrently
        // here.
        unsafe { environ }
    }
}

/// PORTS the `{t}` of the reference's `(E{t})`: `@tagName(errno)` over
/// `std.posix.E`, which renders e.g. `NOENT`, so the message reads `(ENOENT)`.
///
/// PORT NOTE (parity S67-F12): the names come from the whole `std.posix.E`
/// enum of the target (`crate::zig_error::errno_tag_name`, generated from Zig
/// 0.16.0). The port used to transcribe only the 17 POSIX `execve(2)` errors,
/// so an errno outside them, such as macOS's `EBADARCH` for a binary of the
/// wrong architecture, printed as a number. A value the enum does not name
/// (not something a kernel returns from execve; `@tagName` of it is illegal
/// behaviour in Zig) and a target without a table print the number.
#[cfg(unix)]
fn errno_suffix(err: &std::io::Error) -> String {
    match err.raw_os_error() {
        Some(code) => {
            crate::zig_error::errno_tag_name(code).map_or_else(|| code.to_string(), str::to_string)
        }
        None => err.to_string(),
    }
}

#[cfg(all(test, unix))]
mod exec_tests {
    use super::{errno_suffix, exec_editor};

    /// The Zig table is keyed by the target's own errno numbers; spot-check it
    /// against libc's constants, including the values that differ between
    /// Darwin and Linux.
    #[test]
    fn errno_names_match_the_host_libc_numbers() {
        let rows: &[(i32, &str)] = &[
            (libc::EPERM, "PERM"),
            (libc::ENOENT, "NOENT"),
            (libc::E2BIG, "2BIG"),
            (libc::ENOEXEC, "NOEXEC"),
            (libc::EACCES, "ACCES"),
            (libc::ETXTBSY, "TXTBSY"),
            (libc::EAGAIN, "AGAIN"),
            (libc::ELOOP, "LOOP"),
            (libc::ENAMETOOLONG, "NAMETOOLONG"),
            (libc::EDEADLK, "DEADLK"),
        ];
        for (code, name) in rows {
            let e = std::io::Error::from_raw_os_error(*code);
            assert_eq!(*name, errno_suffix(&e), "errno {code}");
        }
    }

    /// parity S67-F12: an errno outside the POSIX execve list still prints its
    /// `@tagName`. Each name is `std.posix.E`'s at Zig 0.16.0.
    #[test]
    fn errno_suffix_names_errnos_outside_the_posix_execve_list() {
        #[cfg(target_vendor = "apple")]
        let rows: &[(i32, &str)] = &[
            (libc::EBADARCH, "BADARCH"),
            (libc::EBADEXEC, "BADEXEC"),
            (libc::ESHLIBVERS, "SHLIBVERS"),
            (libc::EBADMACHO, "BADMACHO"),
        ];
        #[cfg(target_os = "linux")]
        let rows: &[(i32, &str)] = &[(libc::ELIBBAD, "LIBBAD"), (libc::EOPNOTSUPP, "OPNOTSUPP")];
        #[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
        let rows: &[(i32, &str)] = &[];
        assert!(!rows.is_empty(), "no errno table for this target");
        for (code, name) in rows {
            let e = std::io::Error::from_raw_os_error(*code);
            assert_eq!(*name, errno_suffix(&e), "errno {code}");
        }

        // A value the enum does not name prints the number. 4095 is not an
        // errno on any supported target.
        let e = std::io::Error::from_raw_os_error(4095);
        assert_eq!("4095", errno_suffix(&e));
    }

    /// parity S67-F12: the failure report prints the command's raw bytes. A
    /// 4 MiB argument makes execve fail with E2BIG on every target (Darwin's
    /// ARG_MAX is 1 MiB; Linux caps one argument at 128 KiB), so this runs in
    /// the test process.
    #[test]
    fn exec_failure_report_prints_the_raw_command_bytes() {
        let mut command = b"\xff\xfe \xc3(".to_vec();
        command.resize(4 << 20, b'x');
        command.push(0);
        let path = b"/p\xffq";
        let mut out = Vec::new();
        let code = exec_editor(&mut out, &command, path).unwrap();
        assert_eq!(1, code);

        let mut want = b"Failed to execute the editor (E2BIG).\n\
            \n\
            This is usually due to the executable path not existing, invalid\n\
            permissions, or the shell environment not being set up\n\
            correctly.\n\
            \n\
            Command: "
            .to_vec();
        want.extend_from_slice(&command[..command.len() - 1]);
        want.extend_from_slice(b"\nPath: \x1b]8;;file:///p\xffq\x1b\\/p\xffq\x1b]8;;\x1b\\\n");
        assert!(out == want, "the report differs from the reference's bytes");
    }

    /// parity S67-F11: the editor inherits the process's SIGPIPE disposition,
    /// as it does through the reference's raw execve. The process ignores
    /// SIGPIPE (the reference's `GlobalState.initSignals`; set explicitly
    /// here), so a shell pipeline in the editor sees EPIPE instead of dying.
    ///
    /// A successful exec replaces the process, so the body runs in a
    /// re-executed copy of this test binary. The "editor" is a shell script
    /// whose pipeline writer loops until the reader has gone: with SIGPIPE
    /// ignored its `echo` fails and it writes `ignored`; with SIGPIPE at its
    /// default the writer is killed. Either way the script then writes `ran`.
    #[test]
    fn the_editor_inherits_the_ignored_sigpipe() {
        const CHILD: &str = "RUSTTY_T_C_EDIT_CONFIG_EXEC_CHILD";
        if let Some(out) = std::env::var_os(CHILD) {
            exec_child(std::path::Path::new(&out));
        }

        let dir =
            std::env::temp_dir().join(format!("rustty-t-c-edit-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out");
        let _ = std::fs::remove_file(&out);
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "edit_config::exec_tests::the_editor_inherits_the_ignored_sigpipe",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &out)
            .output()
            .expect("re-exec the test binary");
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(child.status.success(), "the child failed:\n{log}");
        let got = std::fs::read_to_string(&out).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            got.ends_with("ran\n"),
            "the editor script did not run to its end: {got:?}\n{log}"
        );
        assert!(
            got.contains("ignored\n"),
            "the editor's pipeline writer was killed by SIGPIPE: {got:?}\n{log}"
        );
    }

    fn exec_child(out: &std::path::Path) -> ! {
        let script = out.with_file_name("editor.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\n\
             { while :; do echo x || { echo ignored > \"$1\"; exit 0; }; done; } | head -n 1 > /dev/null\n\
             echo ran >> \"$1\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        // SAFETY: plain sigaction on SIGPIPE with SIG_IGN, before any other
        // work in this single-test child.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = libc::SIG_IGN;
            libc::sigemptyset(&mut sa.sa_mask);
            assert_eq!(0, libc::sigaction(libc::SIGPIPE, &sa, std::ptr::null_mut()));
        }

        let mut command = Vec::new();
        command.push(b'\'');
        command.extend_from_slice(script.as_os_str().as_encoded_bytes());
        command.extend_from_slice(b"' '");
        command.extend_from_slice(out.as_os_str().as_encoded_bytes());
        command.extend_from_slice(b"'\0");
        let mut stderr = Vec::new();
        let r = exec_editor(&mut stderr, &command, b"/unused");
        panic!(
            "execve returned: {r:?}: {}",
            String::from_utf8_lossy(&stderr)
        );
    }
}
