//! NEW-FINDINGS N380 (review W-4): the `os`-tier diagnostics reach stderr only
//! through the log facade, so a CLI action (whose stderr logging the reference
//! turns off, `global.zig`: `if (self.action != null) self.logging.stderr =
//! false`) prints none of them, and `RUSTTY_LOG=stderr` prints them in the
//! reference's `level(scope): message` form.
//!
//! Before the fix, rustty-os's `log_*` helpers were `eprintln!`s: on macOS
//! `rustty +show-config` printed `[err] [os] NSEvent class not found. Can't get
//! click interval.` (`os/mouse.zig` `log.err`) and, in a debug build,
//! `[debug] [os] file handle limit raised value=...` (`os/file.zig`
//! `log.debug`), where the reference prints nothing.
//!
//! macOS only (R14 form 1): the `NSEvent` lookup that fails in the CLI is the
//! macOS arm of `os/mouse.zig`, and this is the host the difference was
//! measured on; the other hosts' CLI output is covered by their own lanes.
#![cfg(target_os = "macos")]

use std::process::{Command, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_rustty");

fn show_config(log: Option<&str>) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!(
        "rustty-log-facade-{}-{}",
        std::process::id(),
        log.unwrap_or("off")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cmd = Command::new(EXE);
    cmd.args(["+show-config", "--no-pager"])
        .env("HOME", &dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env_remove("RUSTTY_LOG")
        .stdin(Stdio::null());
    if let Some(v) = log {
        cmd.env("RUSTTY_LOG", v);
    }
    let out = cmd.output().expect("rustty runs");
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn an_action_prints_no_os_diagnostics_by_default() {
    let out = show_config(None);
    assert_eq!(Some(0), out.status.code());
    assert_eq!(
        "",
        String::from_utf8_lossy(&out.stderr),
        "stderr must be empty with logging at its action default"
    );
}

#[test]
fn rustty_log_stderr_prints_them_in_the_references_form() {
    let out = show_config(Some("stderr"));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("error(os): NSEvent class not found. Can't get click interval.\n"),
        "{err}"
    );
    assert!(!err.contains("[err] [os]"), "{err}");
}
