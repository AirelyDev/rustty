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
//! 2. ESC does quit, but shutdown then blocks in `Loop::stop()`
//!    (`rustty-vaxis/src/loop_.rs:215`): `stop()` wakes its reader thread — blocked in
//!    `read(/dev/tty)` — with a DSR query `ESC[5n` and then `join()`s it. Only a terminal
//!    that ANSWERS `ESC[5n` with `ESC[0n` unblocks that read. A dumb pty never answers, so
//!    `join()` hangs forever. This is upstream-faithful libvaxis behaviour, NOT a rustty
//!    defect, and it is why the harness must impersonate a terminal. Arm B suppresses only
//!    that one reply and asserts the process does NOT exit — which is what makes Arm A's
//!    clean exit attributable to the ESC key rather than to a timeout or a crash.

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

/// ARM B — the NEGATIVE CONTROL. Same run with the `ESC[5n` reply suppressed: the child
/// must NOT exit. This is what makes Arm A's exit attributable to the ESC key.
///
/// Upstream-faithful: `Loop::stop()` wakes its blocked reader with a DSR query that only a
/// replying terminal answers, so against a non-answering terminal `join()` never returns.
#[test]
#[ignore = "negative control: deliberately wedges +boo and SIGKILLs it; run with --ignored"]
fn boo_without_dsr_reply_does_not_exit() {
    let (rc, kv) = run_harness(&["--no-dsr"]);

    assert_eq!(
        num(&kv, "EXITED"),
        0,
        "the no-DSR negative control EXITED — so Arm A's clean exit is NOT evidence that \
         the ESC key is what shut +boo down (it would exit either way). Either \
         Loop::stop()'s DSR wake stopped being load-bearing, or the harness is answering \
         ESC[5n when it should not."
    );
    assert_ne!(
        rc, 0,
        "the harness must report failure when the child does not exit"
    );

    // The animation must still have RUN — otherwise "did not exit" could be explained by
    // the process having died or hung before rendering, which would prove nothing.
    assert!(
        num(&kv, "FRAMES") >= 10,
        "the negative control rendered only {} frames — it must exercise the same live \
         animation as Arm A, differing ONLY in the suppressed DSR reply",
        num(&kv, "FRAMES")
    );
    assert!(
        num(&kv, "BYTES") > 10_000,
        "the negative control captured only {} bytes; it must be a real run",
        num(&kv, "BYTES")
    );
}
