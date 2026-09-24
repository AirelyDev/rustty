//! RUSTTY-ONLY, TEST-ONLY (task T-F2): run one test in a re-executed copy of
//! this test binary, so it sees a fresh process. Never compiled into the
//! production library.
//!
//! CoreText keeps process-wide font state that outlives the fonts that
//! created it. After a test builds a variation instance of the embedded
//! JetBrains Mono (`deferred_face::tests::
//! load_coretext_deinits_the_face_when_set_variations_fails`), CoreText can
//! answer "JetBrains Mono" for the same font data with its `name` table
//! removed, so the nameless-font tests lose their precondition (measured: 20 of
//! 30 sequential runs of the pair). A lock does not help, because the state
//! persists after the polluting test ends. Those tests run their body in a
//! child process instead.
//!
//! The caller follows the repo's parent/child pattern (R14):
//!
//! ```ignore
//! const CHILD: &str = "RUSTTY_..._CHILD";
//! if std::env::var_os(CHILD).is_some() {
//!     body();
//!     return;
//! }
//! crate::test_child::run_isolated("module::tests::name", CHILD);
//! ```

/// Re-run the test `test_path` alone in a child copy of this binary with
/// `child_env` set, and assert that it ran exactly one test and that the test
/// passed. A filter that matches nothing reports "0 passed" and fails here.
pub(crate) fn run_isolated(test_path: &str, child_env: &str) {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test_path, "--nocapture", "--test-threads=1"])
        .env(child_env, "1")
        .output()
        .expect("re-exec the test binary");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the child run of {test_path} failed ({:?})\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status
    );
    assert!(
        stdout.contains("test result: ok. 1 passed; 0 failed"),
        "the child ran exactly one test, {test_path}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
