//! Parity S62-F3, S62-F4, S62-F5 and N180: every benchmark line the reference
//! writes with `std.debug.print` goes to STDERR (`std/debug.zig:307-312`), so a
//! caller that captures stdout (hyperfine does) sees nothing from them.
//!
//! These run the real `rustty-bench` binary and check both streams: the line
//! must be on stderr (which shows the report step ran) and stdout must be empty.

use std::path::PathBuf;
use std::process::Command;

/// A small VT corpus in a per-test file, removed on drop.
struct Corpus(PathBuf);

impl Corpus {
    fn new(tag: &str) -> Corpus {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "rustty-bench-stderr-{tag}-{}.vt",
            std::process::id()
        ));
        let mut bytes = Vec::new();
        for i in 0..200 {
            bytes.extend_from_slice(format!("line {i} \x1b[1mbold\x1b[0m text\r\n").as_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        Corpus(path)
    }

    fn arg(&self) -> String {
        format!("--data={}", self.0.display())
    }
}

impl Drop for Corpus {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Runs the binary and returns (stdout, stderr), asserting it succeeded.
fn run(args: &[&str]) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_rustty-bench"))
        .args(args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{args:?} failed: {stderr}");
    (stdout, stderr)
}

fn assert_on_stderr_only(args: &[&str], prefixes: &[&str]) {
    let (stdout, stderr) = run(args);
    for prefix in prefixes {
        assert!(
            stderr.lines().any(|l| l.starts_with(prefix)),
            "{args:?}: no stderr line starts with {prefix:?}; stderr was:\n{stderr}"
        );
    }
    assert_eq!("", stdout, "{args:?} wrote to stdout");
}

// S62-F3: ScrollbackCompression.zig:257.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-bench, parity tail C-1): `open` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
)]
fn scrollback_compression_report_goes_to_stderr() {
    let corpus = Corpus::new("scrollback");
    assert_on_stderr_only(
        &["+scrollback-compression", "--mode=report", &corpus.arg()],
        &["scrollback-compression compressed="],
    );
}

// S62-F4: TerminalFormatter.zig:330 and :351.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-bench, parity tail C-1): `open` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
)]
fn terminal_formatter_report_goes_to_stderr() {
    let corpus = Corpus::new("formatter-report");
    assert_on_stderr_only(
        &[
            "+terminal-formatter",
            "--mode=report",
            "--pin-map=true",
            "--loops=1",
            &corpus.arg(),
        ],
        &[
            "terminal-formatter emit=",
            "terminal-formatter-pins points=",
        ],
    );
}

// S62-F4: TerminalFormatter.zig:405.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-bench, parity tail C-1): `open` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
)]
fn terminal_formatter_roundtrip_goes_to_stderr() {
    let corpus = Corpus::new("formatter-roundtrip");
    assert_on_stderr_only(
        &["+terminal-formatter", "--mode=roundtrip", &corpus.arg()],
        &["terminal-formatter roundtrip emit="],
    );
}

// S62-F5: TerminalSnapshot.zig:300 and :306.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-bench, parity tail C-1): `open` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
)]
fn terminal_snapshot_report_goes_to_stderr() {
    let corpus = Corpus::new("snapshot");
    assert_on_stderr_only(
        &["+terminal-snapshot", "--mode=report", &corpus.arg()],
        &["terminal-snapshot tag=", "terminal-snapshot total records="],
    );
}

// N180: PageCompression.zig:436 and :442 (same class, not in the audit).
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-bench, parity tail C-1): `open` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
)]
fn page_compression_report_goes_to_stderr() {
    let corpus = Corpus::new("page");
    assert_on_stderr_only(
        &["+page-compression", "--mode=report", &corpus.arg()],
        &["page-compression page=0 ", "page-compression total pages="],
    );
}
