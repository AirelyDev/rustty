//! Runs esctest (https://github.com/ThomasDickey/esctest2) against
//! librustty-vt, with no GUI in the way.
//!
//! The build installs esctest in share/esctest beside the bin directory
//! holding this executable. It runs under a pty. Everything it writes is
//! fed through a librustty-vt terminal, and the terminal's replies
//! (DECRQCRA checksums, device attributes, cursor position and size
//! reports, and so on) are written back to it. When esctest exits, its
//! log is copied to stdout.
//!
//! PORTS: ghostty `test/esctest/src/main.zig` (438d584f0, edcaf6670).
//!
//! PORT NOTE: "the build installs esctest" is `fetch.sh` in this port (Cargo
//! has no install step); it puts esctest in `target/share/esctest`, which is
//! `share/esctest` beside `target/debug` and `target/release`.

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    match unix::main() {
        Ok(code) => std::process::ExitCode::from(code),
        // Zig prints an error returned from `main` as `error: Name` and
        // exits 1.
        Err(err) => {
            eprintln!("error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

// PORT NOTE: upstream builds on unix only (forkpty). The port's package
// still compiles elsewhere, so it says why it cannot run.
#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("esctest-librustty needs a unix pty (forkpty); it is unix only");
    std::process::ExitCode::FAILURE
}

#[cfg(unix)]
mod unix {
    use std::ffi::CString;
    use std::io::Write as _;
    use std::os::raw::{c_char, c_int};
    use std::os::unix::ffi::OsStringExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

    use rustty_vt::alloc::Global;
    use rustty_vt::device_attributes;
    use rustty_vt::size_report;
    use rustty_vt::stream_terminal::Handler;
    use rustty_vt::terminal::{Colors, Options, Terminal};
    use rustty_vt::xt_checksum;

    /// The size esctest resets the terminal to before every test, and so the
    /// size it starts at.
    const ROWS: u16 = 25;
    const COLS: u16 = 80;

    /// The xterm checksum calculation esctest expects with --xterm-checksum
    /// 334 and later, which is xterm's `checksumExtension: 23`.
    const XT_CHECKSUM: xt_checksum::Flags = xt_checksum::Flags::from_bits(0)
        .with_positive(true)
        .with_no_attributes(true)
        .with_no_trim(true)
        .with_full(true);

    /// Arguments always passed to esctest, before the user's own so that
    /// theirs win. esctest only knows how to check a terminal that answers
    /// like some real one, and xterm is the closest to librustty-vt.
    const ESCTEST_ARGS: [&str; 4] = [
        "--expected-terminal=xterm",
        "--xterm-checksum=411",
        "--xterm-reverse-wrap=411",
        "--timeout=0.2",
    ];

    const USAGE: &str = "\
Usage: esctest-librustty [--verbose] [esctest arguments...]

Arguments are passed to esctest, for example --include=DECSTR.
The log goes to stdout.

--verbose shows librustty-vt's own log on stderr, which names each
sequence it ignores.
";

    /// Whether to show librustty-vt's log. Without --verbose there is a
    /// warning for every sequence esctest sends that isn't implemented,
    /// which buries everything else.
    static VERBOSE: AtomicBool = AtomicBool::new(false);

    // pub const std_options: std.Options = .{ .logFn = logFn };
    // PORT NOTE: Zig's `std_options.logFn` is a `log` crate backend here,
    // installed first thing in `main`. Its output is `std.log.defaultLog`'s:
    // `<level>(<scope>): <message>` on stderr, the scope being the `log`
    // target.
    struct Logger;

    impl log::Log for Logger {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            VERBOSE.load(Ordering::Relaxed) || metadata.level() == log::Level::Error
        }

        fn log(&self, record: &log::Record<'_>) {
            if !self.enabled(record.metadata()) {
                return;
            }
            let level = match record.level() {
                log::Level::Error => "error",
                log::Level::Warn => "warning",
                log::Level::Info => "info",
                log::Level::Debug | log::Level::Trace => "debug",
            };
            eprintln!("{level}({}): {}", record.target(), record.args());
        }

        fn flush(&self) {}
    }

    static LOGGER: Logger = Logger;

    // extern "c" fn forkpty(...) and extern "c" fn execvp(...)
    // PORT NOTE: declared here as upstream declares them, rather than taken
    // from the `libc` crate, whose `forkpty` differs per target in the
    // constness of its pointers.
    extern "C" {
        fn forkpty(
            amaster: *mut c_int,
            name: *mut c_char,
            termp: *const libc::c_void,
            winp: *const libc::winsize,
        ) -> libc::pid_t;

        fn execvp(file: *const c_char, argv: *const *const c_char) -> c_int;
    }

    /// The pty master. Effects only get the handler, so this is how replies
    /// reach esctest.
    static PTY: AtomicI32 = AtomicI32::new(-1);

    pub fn main() -> Result<u8, String> {
        // PORT NOTE: the log backend and its level. Zig's default
        // `std.options.log_level` is `.debug` in Debug builds and `.info`
        // otherwise; `logFn` only sees messages at or above it.
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        });

        let all: Vec<String> = std::env::args().collect();
        let mut args: &[String] = &all[1..];
        if !args.is_empty() && args[0] == "--verbose" {
            VERBOSE.store(true, Ordering::Relaxed);
            args = &args[1..];
        }
        if !args.is_empty() && args[0] == "--help" {
            eprint!("{USAGE}");
            return Ok(1);
        }

        // Without this check, a missing python3 only shows up as esctest
        // writing no log, since the child's own error goes to the pty.
        let path = std::env::var_os("PATH").unwrap_or_default();
        if !on_path(path.as_encoded_bytes(), b"python3") {
            eprintln!("warning: python3 was not found on PATH, and esctest needs it");
            return Ok(1);
        }

        let exe_dir = executable_dir().map_err(|err| format!("ExecutableDir: {err}"))?;
        let esctest_dir: PathBuf = resolve(&exe_dir.join("..").join("share").join("esctest"));
        let esctest_dir_c = CString::new(esctest_dir.as_os_str().as_encoded_bytes())
            .map_err(|_| "InvalidPath".to_string())?;

        // esctest writes its log to a file rather than the terminal, so we
        // give it one to copy from afterwards.
        // PORT NOTE (UD378): upstream uses the fixed name
        // `/tmp/esctest-libghostty-{pid}.log` in the shared /tmp, where another
        // local user can create it first, or a symlink by that name, so the
        // log is written (and later unlinked) through it (CWE-377). The port
        // puts the log in a fresh private directory from mkdtemp (mode 0700),
        // checks it, and removes both afterwards, on every return path.
        let log_dir = LogDir::create()?;
        let log_path = log_dir.log_path();

        let mut argv_owned: Vec<CString> = Vec::new();
        let cstr = |s: &str| CString::new(s).map_err(|_| "InvalidArgument".to_string());
        argv_owned.push(cstr("python3")?);
        argv_owned.push(cstr("esctest.py")?);
        for arg in ESCTEST_ARGS {
            argv_owned.push(cstr(arg)?);
        }
        argv_owned.push(cstr(&format!("--logfile={log_path}"))?);
        for arg in args {
            argv_owned.push(cstr(arg)?);
        }
        let mut argv: Vec<*const c_char> = argv_owned.iter().map(|a| a.as_ptr()).collect();
        argv.push(core::ptr::null());

        let ws = libc::winsize {
            ws_row: ROWS,
            ws_col: COLS,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let mut pty: c_int = -1;
        // SAFETY: `pty` and `ws` are live for the call; a null name and
        // termios are allowed.
        let pid = unsafe { forkpty(&mut pty, core::ptr::null_mut(), core::ptr::null(), &ws) };
        if pid < 0 {
            return Err("ForkptyFailed".to_string());
        }
        PTY.store(pty, Ordering::Relaxed);
        if pid == 0 {
            // SAFETY: the child of a single-threaded process; every argument
            // was built before the fork.
            unsafe {
                if libc::chdir(esctest_dir_c.as_ptr()) != 0 {
                    eprintln!("cannot change to directory {}", esctest_dir.display());
                    libc::_exit(127);
                }
                execvp(c"python3".as_ptr(), argv.as_ptr());
                eprintln!("cannot run python3");
                libc::_exit(127);
            }
        }

        run()?;

        let mut status: c_int = 0;
        // SAFETY: `status` is live for the call.
        unsafe { libc::waitpid(pid, &mut status, 0) };
        let exit_status = libc::WEXITSTATUS(status);

        // The child's own message about this went to the pty, not to us.
        if exit_status == 127 {
            eprintln!(
                "warning: couldn't start esctest with python3 in {}",
                esctest_dir.display()
            );
            return Ok(1);
        }

        copy_log(&log_path)?;
        Ok(exit_status as u8)
    }

    /// The private directory holding esctest's log (UD378). Dropping it is
    /// upstream's `defer _ = std.c.unlink(log_path);`, plus removing the
    /// directory.
    struct LogDir {
        dir: PathBuf,
    }

    impl LogDir {
        /// The log file's name inside the directory.
        const LOG_NAME: &'static str = "esctest.log";

        /// A new directory `$TMPDIR/esctest-librustty-XXXXXX` (`/tmp` when
        /// TMPDIR is unset), made by mkdtemp and checked by `check_private`.
        fn create() -> Result<LogDir, String> {
            let base = std::env::var_os("TMPDIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            let template = base.join("esctest-librustty-XXXXXX");
            let mut template = CString::new(template.into_os_string().into_encoded_bytes())
                .map_err(|_| "InvalidPath".to_string())?
                .into_bytes_with_nul();
            // SAFETY: `template` is a writable NUL-terminated buffer ending in
            // `XXXXXX`, which mkdtemp replaces in place.
            let made = unsafe { libc::mkdtemp(template.as_mut_ptr().cast()) };
            if made.is_null() {
                return Err(format!(
                    "MakeLogDirFailed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            template.pop(); // the NUL
            let dir = PathBuf::from(std::ffi::OsString::from_vec(template));
            let log_dir = LogDir { dir };
            check_private(&log_dir.dir)?;
            Ok(log_dir)
        }

        fn log_path(&self) -> String {
            self.dir.join(Self::LOG_NAME).to_string_lossy().into_owned()
        }
    }

    impl Drop for LogDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.dir.join(Self::LOG_NAME));
            let _ = std::fs::remove_dir(&self.dir);
        }
    }

    /// The directory is a real directory (not a symlink), owned by this
    /// user, with mode 0700, so nobody else can create or replace the log
    /// inside it (UD378).
    fn check_private(dir: &Path) -> Result<(), String> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let meta = std::fs::symlink_metadata(dir)
            .map_err(|err| format!("LogDirUnchecked: {}: {err}", dir.display()))?;
        // SAFETY: geteuid has no preconditions.
        let uid = unsafe { libc::geteuid() };
        let mode = meta.permissions().mode() & 0o777;
        if !meta.file_type().is_dir() || meta.uid() != uid || mode != 0o700 {
            return Err(format!(
                "LogDirNotPrivate: {} (directory={}, owner={}, mode={mode:o})",
                dir.display(),
                meta.file_type().is_dir(),
                meta.uid()
            ));
        }
        Ok(())
    }

    /// `std.process.executableDirPathAlloc`: the directory of this
    /// executable, with symlinks resolved.
    fn executable_dir() -> std::io::Result<PathBuf> {
        let exe = std::env::current_exe()?.canonicalize()?;
        Ok(exe.parent().map(Path::to_path_buf).unwrap_or(exe))
    }

    /// `std.fs.path.resolve`: drop `.` and fold `..` lexically.
    fn resolve(path: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for c in path.components() {
            match c {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    out.pop();
                }
                other => out.push(other),
            }
        }
        out
    }

    /// Whether an executable with this name is in one of the directories of
    /// a PATH value.
    fn on_path(path: &[u8], name: &[u8]) -> bool {
        // std.mem.tokenizeScalar(u8, path, ':') skips empty entries.
        for dir in path.split(|&b| b == b':').filter(|d| !d.is_empty()) {
            let mut candidate = dir.to_vec();
            candidate.push(b'/');
            candidate.extend_from_slice(name);
            let Ok(candidate) = CString::new(candidate) else {
                continue;
            };
            // SAFETY: a NUL-terminated path.
            if unsafe { libc::access(candidate.as_ptr(), libc::X_OK) } == 0 {
                return true;
            }
        }
        false
    }

    /// Feed esctest's output through the terminal until it exits.
    fn run() -> Result<(), String> {
        // PORT NOTE: Zig's `Options` fields have defaults; Rust's do not, so
        // every field is spelled with the reference default except
        // `default_xt_checksum`, the one upstream sets. The runner imports
        // the `ghostty-vt` module, which is built as the `.lib` artifact, so
        // the Kitty image storage default is the lib's.
        let mut t = Terminal::init(
            Global,
            Options {
                cols: COLS,
                rows: ROWS,
                max_scrollback_bytes: Some(10_000),
                max_scrollback_lines: None,
                colors: Colors::default(),
                default_modes: Default::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                default_xt_checksum: XT_CHECKSUM,
                kitty_image_storage_limit: Options::kitty_image_storage_limit_default(
                    rustty_vt::build_options::Artifact::Lib,
                ),
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .map_err(|_| "OutOfMemory".to_string())?;

        let result = {
            let mut stream = t.vt_stream();
            stream.handler.xt_checksum_report = true;
            stream.handler.effects.write_pty = Some(write_pty);
            stream.handler.effects.size = Some(size);
            stream.handler.effects.device_attributes = Some(device_attributes);

            let result = read_loop(|data| stream.next_slice(data));
            // defer stream.deinit();
            stream.deinit();
            result
        };
        // defer t.deinit(init.gpa);
        t.deinit(Global);
        result
    }

    fn read_loop(mut feed: impl FnMut(&[u8])) -> Result<(), String> {
        let pty = PTY.load(Ordering::Relaxed);
        let mut buf = [0u8; 4096];
        loop {
            // SAFETY: `buf` is live and `buf.len()` bytes long.
            let n = unsafe { libc::read(pty, buf.as_mut_ptr().cast(), buf.len()) };
            if n > 0 {
                feed(&buf[..n as usize]);
                continue;
            }

            // Linux reports the other end closing as EIO rather than EOF.
            if n == 0 {
                return Ok(());
            }
            let err = std::io::Error::last_os_error();
            match err.raw_os_error() {
                Some(libc::EINTR) => continue,
                Some(libc::EIO) => return Ok(()),
                _ => {
                    eprintln!("reading the pty failed: {err}");
                    return Err("ReadFailed".to_string());
                }
            }
        }
    }

    fn write_pty(_: &mut Handler<'_>, data: &[u8]) {
        let pty = PTY.load(Ordering::Relaxed);
        let mut rest = data;
        while !rest.is_empty() {
            // SAFETY: `rest` is live and `rest.len()` bytes long.
            let n = unsafe { libc::write(pty, rest.as_ptr().cast(), rest.len()) };
            if n < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                return;
            }
            rest = &rest[n as usize..];
        }
    }

    fn size(h: &mut Handler<'_>) -> Option<size_report::Size> {
        // SAFETY: the handler's terminal is the live `Terminal` its stream
        // was made from; only two fields are read.
        let (rows, columns) = unsafe { ((*h.terminal).rows, (*h.terminal).cols) };
        Some(size_report::Size {
            rows,
            columns,
            cell_width: 10,
            cell_height: 20,
        })
    }

    /// Answer as a VT520, the terminal esctest's highest VT level tests.
    fn device_attributes(_: &mut Handler<'_>) -> device_attributes::Attributes {
        device_attributes::Attributes {
            primary: device_attributes::Primary {
                conformance_level: device_attributes::ConformanceLevel::Level5,
                features: &[device_attributes::Feature::AnsiColor],
            },
            secondary: device_attributes::Secondary {
                device_type: device_attributes::DeviceType::VT520,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn copy_log(path: &str) -> Result<(), String> {
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("esctest wrote no log to {path}: {err}");
                return Ok(());
            }
        };

        let stdout = std::io::stdout();
        let mut writer = stdout.lock();
        std::io::copy(&mut file, &mut writer).map_err(|err| format!("WriteFailed: {err}"))?;
        writer
            .flush()
            .map_err(|err| format!("WriteFailed: {err}"))?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        // UD378: the log is not a fixed name in the shared temporary
        // directory but a file inside a fresh 0700 directory of its own,
        // and both are gone afterwards.
        #[test]
        fn log_path_is_inside_a_fresh_private_directory() {
            let a = LogDir::create().unwrap();
            let b = LogDir::create().unwrap();
            assert_ne!(a.dir, b.dir);
            for d in [&a, &b] {
                let mode = std::fs::metadata(&d.dir).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o700);
                let path = PathBuf::from(d.log_path());
                assert_eq!(path.parent(), Some(d.dir.as_path()));
                let name = d.dir.file_name().unwrap().to_string_lossy().into_owned();
                assert!(name.starts_with("esctest-librustty-"), "{name}");
            }
            std::fs::write(a.log_path(), b"log").unwrap();
            let dir = a.dir.clone();
            drop(a);
            assert!(!dir.exists());
        }

        // The check refuses a directory others can write to, and a symlink
        // to a private directory.
        #[test]
        fn check_private_rejects_shared_directories_and_symlinks() {
            let d = LogDir::create().unwrap();
            let shared = d.dir.join("shared");
            std::fs::create_dir(&shared).unwrap();
            std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o777)).unwrap();
            assert!(check_private(&shared).is_err());
            std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
            assert!(check_private(&shared).is_ok());
            let link = d.dir.join("link");
            std::os::unix::fs::symlink(&shared, &link).unwrap();
            assert!(check_private(&link).is_err());
            std::fs::remove_file(&link).unwrap();
            std::fs::remove_dir(&shared).unwrap();
        }
    }
}
