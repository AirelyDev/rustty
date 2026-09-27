//! How `rustty.exe` behaves when it is "opened" on Windows, measured against
//! Ghostty's exe built at the pin (task W-4, `.port/tools/windows/exe-differential.ps1`).
//!
//! At the pin the Windows exe is the helper CLI (app runtime `.none`) and a
//! GUI-subsystem binary, so a shell start (a double-click, `Start-Process`,
//! `cmd`'s `start`) gives it NO standard handles, and a start from a console
//! gives it console handles a GUI process cannot write to. The reference's
//! writes then fail (`error.NotOpenForWriting`) and its `try`s turn that into
//! exit status 1, with nothing shown. MEASURED on Windows 11 with the reference
//! exe: 1 for no arguments and for `+version`, both ways. Rust's stdio reports
//! success on such a handle, so the port exited 0 until `rustty-cli::io_file`
//! (NEW-FINDINGS N365).
//!
//! Windows-only: the reference's `.windows` arms (the subsystem, the handles)
//! exist on no other OS (R14 form 1). The Unix form of the error path is
//! `rustty-cli`'s `io_file` unit test.
#![cfg(windows)]

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_rustty");

/// `CREATE_NO_WINDOW`: the outer `cmd` gets no window. The `start`ed process
/// still gets its own (minimised) console, as from Explorer or a shell.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Runs `cmd /v:on /c <line> & exit !errorlevel!` and returns its exit code:
/// the ERRORLEVEL after the line (`start` itself leaves `cmd /c` exiting 0).
/// Delayed expansion reads it when `exit` runs, after `start /wait` set it.
fn cmd(line: &str) -> i32 {
    let status = Command::new("cmd.exe")
        .raw_arg("/v:on /c")
        .raw_arg(format!("{line} & exit !errorlevel!"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .expect("cmd.exe runs");
    status.code().expect("cmd.exe exits with a code")
}

/// A shell start: `start "" /wait /min <exe> <args>`. `start` launches the exe
/// the way Explorer does (no standard handles for a GUI-subsystem program) and
/// its `/wait` leaves the exe's exit code in ERRORLEVEL.
fn shell_start(args: &str) -> i32 {
    cmd(&format!("start \"\" /wait /min \"{EXE}\" {args}"))
}

/// A start from a console: a new `cmd` in its own console runs the exe, which
/// inherits that console's handles (unusable by a GUI-subsystem process);
/// `cmd /c` waits for it and exits with its code, which `start /wait` passes
/// back. `tail` is appended to the inner command line (a redirection).
fn console_start(args: &str, tail: &str) -> i32 {
    cmd(&format!(
        "start \"\" /wait /min cmd.exe /c \"\"{EXE}\" {args}{tail}\""
    ))
}

#[test]
fn opening_it_with_no_arguments_exits_1_like_the_reference() {
    assert_eq!(1, shell_start(""), "shell start, no arguments");
    assert_eq!(1, console_start("", ""), "console start, no arguments");
}

#[test]
fn opening_it_with_version_exits_1_like_the_reference() {
    assert_eq!(1, shell_start("+version"), "shell start, +version");
    assert_eq!(1, console_start("+version", ""), "console start, +version");
}

// Positive control for the two tests above: the same starts with the output
// redirected to a file succeed, so the 1 above is the write failing and not
// the start itself.
#[test]
fn opening_it_with_its_output_redirected_exits_0() {
    let dir = std::env::temp_dir().join(format!("rustty-w4-launch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out.txt");
    let code = console_start("+version", &format!(" > \"{}\"", out.display()));
    let text = std::fs::read_to_string(&out).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(0, code, "redirected start");
    assert!(
        text.starts_with("Rustty "),
        "the version reached the file: {text:?}"
    );
}

// UD341 (R13): `+validate-config` of a valid config exits 0 when its stdout is
// a pipe. The reference exits 1 there (`File.Writer.end()` calls `setLength`
// on the pipe, `error.FileTooBig`; MEASURED on the reference exe, task W-4).
#[test]
fn validate_config_on_a_pipe_reports_a_valid_config_as_valid() {
    let dir = std::env::temp_dir().join(format!("rustty-w4-validate-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("appdata")).unwrap();
    let conf = dir.join("valid.conf");
    std::fs::write(&conf, "font-size = 13\n").unwrap();
    let out = Command::new(EXE)
        .arg("+validate-config")
        .arg(format!("--config-file={}", conf.display()))
        .env("LOCALAPPDATA", dir.join("appdata"))
        .stdin(Stdio::null())
        .output()
        .expect("rustty runs");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(Some(0), out.status.code(), "stdout: {:?}", out.stdout);
    assert!(out.stdout.is_empty());
}
