//! WS12 T5 — `rustty +boo` end-to-end pty KAT.
//!
//! Run explicitly (both arms):
//! ```text
//! cargo build -p rustty --bin rustty
//! cargo test -p rustty-cli --test boo_pty_e2e -- --ignored --nocapture
//! ```
//!
//! WHY `#[ignore]` BY DEFAULT: each arm forks a pty and drives a real full-screen
//! animation loop for several seconds, and the negative-control arm deliberately ends in
//! a `SIGKILL`. That does not belong in the default `cargo test` lane. It is `#[ignore]`d
//! for RUNTIME, not because it is flaky or unproven — both arms are executed and their
//! output is recorded in the WS12 task log.
//!
//! WHY IT DOES NOT SELF-SKIP: a KAT that quietly passes when its subject is missing
//! asserts nothing. If the `rustty` binary is absent these tests **panic with build
//! instructions** rather than returning early.
//!
//! The two traps this KAT exists to encode, both found in WS12 planning:
//!
//! 1. A pty with no winsize renders NO cell content — only the empty per-frame
//!    synchronized-update wrapper. So Arm A asserts on content (frames, an `ESC[34m`
//!    colour run, and a far-right cursor column), never merely on "bytes were emitted".
//!
//! 2. ESC does quit, and shutdown then waits in `Loop::stop()`
//!    (`rustty-vaxis/src/loop_.rs`): `stop()` wakes its reader thread — blocked reading
//!    `/dev/tty` — with a DSR query `ESC[5n` and then `join()`s it. A terminal that
//!    ANSWERS with `ESC[0n` ends the read at once, as in libvaxis. In libvaxis a terminal
//!    that never answers makes `join()` hang forever; rustty does not port that deadlock
//!    (reference defect, R13, `.port/parity-closure/UPSTREAM-DEFECTS.md` UD246): after
//!    `STOP_DSR_GRACE` (1 s) `stop()` wakes the reader through a pipe it waits on beside
//!    the tty. So Arm A (the harness answers DSR) must exit well inside the grace, which
//!    attributes its exit to the ESC key plus the DSR reply, and Arm B (the reply
//!    suppressed) must exit only after the grace, which proves the stop is bounded and
//!    that it was the wake pipe, not the reply, that ended it.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    // crates/rustty-cli -> crates -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR must be <root>/crates/rustty-cli")
        .to_path_buf()
}

/// Locate the `rustty` binary. It is produced by the SEPARATE `rustty` bin crate
/// (`crates/rustty/`), so `env!("CARGO_BIN_EXE_rustty")` is not available from
/// `rustty-cli`'s test target — look it up under `target/<profile>/`.
fn rustty_binary() -> PathBuf {
    let root = repo_root();
    // CARGO_TARGET_DIR is honoured; worktrees in this repo share the main target/.
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    for profile in ["debug", "release"] {
        let candidate = target.join(profile).join("rustty");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "the `rustty` binary was not found under {}/{{debug,release}}/rustty.\n\
         This KAT drives the REAL binary and must not self-skip. Build it first:\n\
         \n    cargo build -p rustty --bin rustty\n",
        target.display()
    );
}

fn harness() -> PathBuf {
    let p = repo_root().join(".port/tools/boo_pty_kat.py");
    assert!(
        p.is_file(),
        "missing pty harness {} — it is committed alongside this test",
        p.display()
    );
    p
}

/// Run the harness and return `(script_exit_code, parsed key=value lines)`.
fn run_harness(extra: &[&str]) -> (i32, std::collections::HashMap<String, String>) {
    let binary = rustty_binary();
    let mut cmd = Command::new("python3");
    cmd.arg(harness()).arg(&binary).args(extra);
    let out = cmd.output().expect("failed to spawn python3 harness");

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    eprintln!("--- harness stdout ---\n{stdout}");
    if !stderr.trim().is_empty() {
        eprintln!("--- harness stderr ---\n{stderr}");
    }

    let mut kv = std::collections::HashMap::new();
    for line in stdout.lines() {
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    assert!(
        kv.contains_key("EXITED"),
        "harness produced no machine-readable output; it likely failed to start.\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    (out.status.code().unwrap_or(-1), kv)
}

fn secs(kv: &std::collections::HashMap<String, String>, key: &str) -> f64 {
    kv.get(key)
        .unwrap_or_else(|| panic!("harness output has no {key}"))
        .parse()
        .unwrap_or_else(|e| panic!("harness {key} is not a number: {e}"))
}

fn num(kv: &std::collections::HashMap<String, String>, key: &str) -> i64 {
    kv.get(key)
        .unwrap_or_else(|| panic!("harness output has no {key}"))
        .parse()
        .unwrap_or_else(|e| panic!("harness {key} is not an integer: {e}"))
}

/// ARM A — the positive arm. With a terminal that answers DSR, `+boo` renders real
/// content and ESC produces a clean exit.
#[test]
#[ignore = "forks a pty and runs the real +boo animation for several seconds; run with --ignored"]
fn boo_renders_frames_and_exits_cleanly_on_esc() {
    let (rc, kv) = run_harness(&[]);

    assert_eq!(
        num(&kv, "EXITED"),
        1,
        "+boo did not exit after ESC. If EXITED=0 with a healthy FRAMES count, suspect \
         the DSR wake in Loop::stop() (loop_.rs:215) — see this file's header."
    );
    assert_eq!(num(&kv, "EXIT_STATUS"), 0, "+boo exited non-zero after ESC");
    assert_eq!(rc, 0, "harness reported failure");
    // The DSR reply, not the UD246 grace, ended the read: well inside STOP_DSR_GRACE (1 s).
    assert!(
        secs(&kv, "ELAPSED_AFTER_QUIT") < 0.5,
        "+boo took {}s to exit although the harness answered ESC[5n; a terminal that \
         answers must shut down as in libvaxis, not wait for the UD246 grace",
        secs(&kv, "ELAPSED_AFTER_QUIT")
    );

    // CONTENT assertions. A 0x0-winsize run emits only empty synchronized-update pairs,
    // so frames alone are not enough — the colour run and the far-right cursor column are
    // what prove actual cells were drawn.
    assert!(
        num(&kv, "FRAMES") >= 10,
        "expected >= 10 synchronized-update frames (ESC[?2026h), got {}",
        num(&kv, "FRAMES")
    );
    assert!(
        num(&kv, "COLOR_RUNS") >= 1,
        "expected >= 1 ESC[34m colour run (the ghost outline); got {} — the screen was \
         addressed but nothing was actually painted",
        num(&kv, "COLOR_RUNS")
    );
    assert!(
        num(&kv, "MAX_COL") >= 60,
        "expected a cursor address at column >= 60 (the art is ~100 cols wide); max was \
         {} — this is the 0x0-winsize failure signature",
        num(&kv, "MAX_COL")
    );
}

/// ARM B — the terminal never answers `ESC[5n`. libvaxis's `Loop.stop` then joins
/// forever (UPSTREAM-DEFECTS.md UD246); rustty's `stop` wakes the reader through its
/// wake pipe after `STOP_DSR_GRACE` (1 s), so `+boo` must exit cleanly, and only after
/// the grace (which shows the wake pipe, not a reply, ended the read).
#[test]
#[ignore = "forks a pty and runs the real +boo animation with the DSR reply suppressed; run with --ignored"]
fn boo_without_dsr_reply_exits_after_the_grace() {
    let (rc, kv) = run_harness(&["--no-dsr"]);

    assert_eq!(
        num(&kv, "EXITED"),
        1,
        "with the ESC[5n reply suppressed +boo did not exit: Loop::stop() joined a reader \
         still blocked in read (the libvaxis deadlock, UD246)"
    );
    assert_eq!(num(&kv, "EXIT_STATUS"), 0, "+boo exited non-zero");
    assert_eq!(rc, 0, "harness reported failure");
    let elapsed = secs(&kv, "ELAPSED_AFTER_QUIT");
    assert!(
        elapsed >= 0.9,
        "+boo exited {elapsed}s after ESC with no DSR reply, before the 1 s grace: \
         something other than the UD246 wake ended the read (is the harness answering?)"
    );

    // The animation must still have RUN — otherwise the exit could be explained by the
    // process having died or stopped before rendering, which would prove nothing.
    assert!(
        num(&kv, "FRAMES") >= 10,
        "Arm B rendered only {} frames — it must exercise the same live \
         animation as Arm A, differing ONLY in the suppressed DSR reply",
        num(&kv, "FRAMES")
    );
    assert!(
        num(&kv, "BYTES") > 10_000,
        "Arm B captured only {} bytes; it must be a real run",
        num(&kv, "BYTES")
    );
}
