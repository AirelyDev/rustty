//! Parity S62-F2: `benchmark/cli.zig:55` starts `main` with
//! `try global.init(.{ .tool = minimal })`. The port's `cli::main` runs
//! `rustty_apprt::init_with_action(false)`, the `.tool` arm. One observable
//! piece of that init is `ensureLocale`, which, for a valid `LANG`, logs
//! `setlocale from env result=<locale>` (`os/locale.zig:31`) at info level
//! through the log backend the binary installs.
//!
//! This runs the real `rustty-bench` binary with a fixed locale environment and
//! checks that line reaches stderr and the benchmark still succeeds.

use std::process::Command;

#[test]
fn global_init_runs_before_the_benchmark() {
    let mut corpus = std::env::temp_dir();
    corpus.push(format!(
        "rustty-bench-global-init-{}.vt",
        std::process::id()
    ));
    std::fs::write(&corpus, b"hello\r\n\x1b[1mworld\x1b[0m\r\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rustty-bench"))
        .arg("+terminal-parser")
        .arg(format!("--data={}", corpus.display()))
        .env("LANG", "en_US.UTF-8")
        .env_remove("LC_ALL")
        .env_remove("LC_CTYPE")
        .env_remove("RUSTTY_LOG")
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&corpus);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "rustty-bench failed: {stderr}");
    assert!(
        stderr.contains("setlocale from env result=en_US.UTF-8"),
        "global.init's ensureLocale did not run; stderr was:\n{stderr}"
    );
}
