//! `P7-W11 T1` fix round 1 — wire `bash_shell_integration_exit_status.sh`
//! into `cargo test` so a regression in `rustty.bash` is caught, instead of
//! only being provable by hand.
//!
//! The script (`crates/rustty-extra/tests/bash_shell_integration_exit_status.sh`)
//! proves: with `PROMPT_COMMAND` pre-declared as an exported array
//! (`declare -ax`), a failing command must report exit status 1 through the
//! OSC 133 `D` mark. It needs a real bash >= 4.4 to exercise that code path
//! (macOS ships bash 3.2 as `/bin/bash`, which takes a different branch), so
//! it exits 77 (the automake SKIP convention) when none is found, 0 on pass,
//! and any other non-zero on failure.

use std::path::PathBuf;
use std::process::Command;

/// Automake's convention exit code for "this test could not run", distinct
/// from both success (0) and failure (anything else non-zero). The shell
/// script under test uses it for exactly the "no bash >= 4.4" case.
const SKIP_EXIT_CODE: i32 = 77;

/// RUSTTY-ONLY strict mode (R14, follows the `RUSTTY_REQUIRE_GL`/
/// `RUSTTY_REQUIRE_DMABUF` precedent in `crates/rustty-renderer/src/opengl/gl/test_support.rs`):
/// `RUSTTY_REQUIRE_BASH44=1` in the environment turns the script's clean
/// skip (no bash >= 4.4 on this host) into a hard test failure. Without it,
/// a CI leg that is SUPPOSED to provide bash >= 4.4 could skip this test
/// forever and go green, proving nothing. Unset (the default, and any host
/// without a Homebrew/pkg bash) preserves the clean-skip contract.
fn bash44_required() -> bool {
    bash44_required_from(std::env::var_os("RUSTTY_REQUIRE_BASH44").as_deref())
}

/// The env-independent core of [`bash44_required`], split out so it can be
/// unit-tested without mutating process-global environment (`set_var` would
/// race any other test reading the same variable under the parallel libtest
/// harness).
fn bash44_required_from(value: Option<&std::ffi::OsStr>) -> bool {
    value == Some(std::ffi::OsStr::new("1"))
}

#[test]
fn declare_ax_prompt_command_reports_exit_status_through_osc_133_d() {
    let script: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "bash_shell_integration_exit_status.sh",
    ]
    .iter()
    .collect();
    assert!(
        script.is_file(),
        "test script not found: {}",
        script.display()
    );

    let output = Command::new(&script)
        .output()
        .unwrap_or_else(|err| panic!("failed to run {}: {err}", script.display()));

    match output.status.code() {
        Some(0) => {
            // PASS. The script already printed its own "PASS: ..." line to
            // stdout; nothing further to assert.
        }
        Some(SKIP_EXIT_CODE) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                !bash44_required(),
                "bash >= 4.4 required (RUSTTY_REQUIRE_BASH44=1) but unavailable: {stderr}"
            );
            // R14: no SILENT skip — print the script's own reason so a
            // `cargo test -p rustty-extra -- --nocapture` run (or a failed
            // build's captured stdout) shows this test never actually
            // exercised the array-PROMPT_COMMAND fix.
            println!(
                "skipping declare_ax_prompt_command_reports_exit_status_through_osc_133_d: {stderr}"
            );
        }
        other => {
            panic!(
                "{} exited {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                script.display(),
                other,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
    }
}

#[cfg(test)]
mod pivot_tests {
    use super::*;

    #[test]
    fn bash44_required_is_false_by_default() {
        assert!(!bash44_required_from(None));
        assert!(!bash44_required_from(Some(std::ffi::OsStr::new("0"))));
        assert!(!bash44_required_from(Some(std::ffi::OsStr::new(""))));
    }

    #[test]
    fn bash44_required_is_true_only_for_exactly_1() {
        assert!(bash44_required_from(Some(std::ffi::OsStr::new("1"))));
    }
}
