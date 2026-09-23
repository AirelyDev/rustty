//! S61-F9: `+page-compression --mode=report` prints its per-page and total lines
//! with `std.debug.print` (`src/benchmark/PageCompression.zig:436-453` at
//! `20abdb50a`), which writes to STDERR. The port used `println!` (stdout) for both.
//!
//! Runs the shipped `rustty-bench` binary on a 3-page corpus and checks both
//! streams: the report lines must be on stderr (all four: three pages plus the
//! total) and stdout must carry none of them.

use std::process::Command;

#[test]
fn page_compression_report_prints_to_stderr_not_stdout() {
    let dir = std::env::temp_dir().join(format!(
        "rustty-bench-s61f9-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let data = dir.join("pages.raw");
    // Three chunks at --page-size=4096: two full pages and one short tail.
    let corpus: Vec<u8> = (0..(4096 * 2 + 100)).map(|i| (i % 251) as u8).collect();
    std::fs::write(&data, &corpus).expect("write corpus");

    let out = Command::new(env!("CARGO_BIN_EXE_rustty-bench"))
        .arg("+page-compression")
        .arg("--mode=report")
        .arg("--page-size=4096")
        .arg(format!("--data={}", data.display()))
        .output()
        .expect("run rustty-bench");
    let _ = std::fs::remove_dir_all(&dir);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "rustty-bench failed: status={:?} stderr={stderr}",
        out.status
    );

    // Positive: the report ran, and every line reached stderr.
    let page_lines = stderr
        .lines()
        .filter(|l| l.starts_with("page-compression page="))
        .count();
    let total_lines = stderr
        .lines()
        .filter(|l| l.starts_with("page-compression total pages=3 "))
        .count();
    assert_eq!(
        page_lines, 3,
        "three per-page lines on stderr; stderr={stderr}"
    );
    assert_eq!(total_lines, 1, "one total line on stderr; stderr={stderr}");

    // Negative: nothing of the report on stdout.
    assert!(
        !stdout.contains("page-compression"),
        "the report must not be on stdout (std.debug.print is stderr); stdout={stdout}"
    );
}
