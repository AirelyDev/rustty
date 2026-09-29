//! `P7-W11 T1` fix round 1 — wire `bash_shell_integration_exit_status.sh`
//! into `cargo test` so a regression in `rustty.bash` is caught, instead of
//! only being provable by hand. Fix round 2 extended the script's own case
//! coverage (see its header comment) to also exercise `ed7f046ee`
//! (recognise `declare -ax` / attributed PROMPT_COMMAND arrays) and
//! `a4031c8e3` (detect our hook inside PROMPT_COMMAND arrays), which
//! previously had no behavioural test in this wired gate — only the
//! manually-run, `#[ignore]`d byte-parity KAT
//! (`xtask`'s `oracle_shell_integration_parity`).
//!
//! The script (`crates/rustty-extra/tests/bash_shell_integration_exit_status.sh`)
//! runs several cases and reports a single pass/fail/skip verdict:
//!   1. with `PROMPT_COMMAND` pre-declared as an empty exported array
//!      (`declare -ax`), a failing command must report exit status 1
//!      through the OSC 133 `D` mark;
//!   2. with `PROMPT_COMMAND` pre-declared as a NON-EMPTY exported array
//!      (`declare -ax PROMPT_COMMAND=(__prior_marker_hook)`), sourcing must:
//!      keep it an array, keep the prior entry byte-identical (not merged
//!      into a wrapped string), and not double-install the hook on a second
//!      source;
//!   3. the same, with TWO prior elements, to force the hook-present guard
//!      to search across a space-joined, multi-element `PROMPT_COMMAND[*]`
//!      rather than a single already-merged element.
//!
//! It needs a real bash >= 4.4 to exercise this hook-installer code path
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
fn declare_ax_prompt_command_is_installed_and_reports_status_correctly() {
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
                "skipping declare_ax_prompt_command_is_installed_and_reports_status_correctly: {stderr}"
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
