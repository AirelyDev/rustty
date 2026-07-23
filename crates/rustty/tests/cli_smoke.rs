//! The headless "it runs" KAT (CLI_BLUEPRINT.md §7).
//!
//! A bin-crate integration test (NOT a unit test in the 45/6 test-count contract):
//! the deliverables proof that the headless `rustty` product actually EXECUTES.
//! It spawns the BUILT binary via `env!("CARGO_BIN_EXE_rustty")` (Cargo sets this
//! for `tests/`) with `std::process::Command` and asserts real end-to-end behavior
//! for every path the C6 dispatch wires:
//!
//!   * `+version`  -> exit 0, stdout starts `rustty `. **ADV-FIX 1**: does NOT
//!     assert the renderer/libxev line CONTENT (those are stubbed until R-d/D6).
//!   * `--version` -> exit 0 (the `detectSpecialCase` `--version` -> Version).
//!   * `+list-actions` -> exit 0, non-empty stdout (the keybind action list).
//!   * `+validate-config` -> exit 0 on a valid `--config-file`; exit 1 + a
//!     diagnostic on a deliberately-broken one.
//!   * `+ssh-cache` (XDG_STATE_HOME -> tmp) -> exit 0; after `--add=…` the listing
//!     shows the entry.
//!   * `+new-window` -> exit 1 + stderr "not supported on this platform" (the
//!     faithful `none`-backend IPC behavior — proves the seam is wired, ADV-FIX 4).
//!   * `rustty` (no args) -> exit 0 + the headless usage banner (`.none` build).
//!   * `+bogus` -> exit 1 + the "unknown CLI action" message.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// Path to the built `rustty` binary (Cargo exports this for integration tests).
const BIN: &str = env!("CARGO_BIN_EXE_rustty");

/// Run `rustty <args...>` with an extra environment, capturing the result.
fn run(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("failed to spawn the rustty binary")
}

/// Create a fresh, unique temp directory for a test (no `tempfile` dep — the
/// disk_cache tests' `std::env::temp_dir` precedent). Parallel `cargo test` must
/// not share a fixed path (recall the onig_initialize parallel-init lesson —
/// isolate per-test file state), so each dir is keyed by pid + a process-unique
/// counter + a nanosecond stamp.
fn unique_tmp_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "rustty-cli-smoke-{tag}-{}-{n}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

/// Write `contents` to `path` (a config fixture).
fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("failed to write fixture file");
}

// `rustty +version` -> exit 0, stdout starts `Rustty ` (the title-case product
// name). WS3-B: the renderer / libxev / toolchain lines now carry REAL values
// (ADV-FIX 1 retired) — assert their content and the absence of every stub.
#[test]
fn version_plus() {
    let out = run(&["+version"], &[]);
    assert!(out.status.success(), "+version should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("Rustty "),
        "+version stdout should start with `Rustty `, got: {:?}",
        stdout
    );
    assert!(
        !stdout.contains("STUB") && !stdout.contains("deferred"),
        "+version must carry no stub lines, got: {stdout}"
    );
    assert!(
        stdout.contains("  - Rust version  : 1."),
        "toolchain line missing/wrong: {stdout}"
    );
    #[cfg(target_os = "macos")]
    {
        assert!(
            stdout.contains("  - renderer      : renderer.generic.Renderer(renderer.Metal)"),
            "renderer line missing/wrong: {stdout}"
        );
        assert!(
            stdout.contains("  - libxev        : kqueue"),
            "libxev line missing/wrong: {stdout}"
        );
    }
    #[cfg(target_os = "linux")]
    {
        assert!(
            stdout.contains("  - renderer      : renderer.generic.Renderer(renderer.OpenGL)"),
            "renderer line missing/wrong: {stdout}"
        );
        assert!(
            stdout.contains("  - libxev        : epoll")
                || stdout.contains("  - libxev        : io_uring"),
            "libxev line missing/wrong: {stdout}"
        );
    }
}

// `rustty --version` -> exit 0 (special-case, same output as `+version`).
#[test]
fn version_dashes() {
    let out = run(&["--version"], &[]);
    assert!(out.status.success(), "--version should exit 0");
    assert!(
        out.stdout.starts_with(b"Rustty "),
        "--version stdout should start with `Rustty `",
    );
}

// `rustty +list-actions` -> exit 0, non-empty stdout (keybind action list).
#[test]
fn list_actions_nonempty() {
    let out = run(&["+list-actions"], &[]);
    assert!(out.status.success(), "+list-actions should exit 0");
    assert!(
        !out.stdout.is_empty(),
        "+list-actions stdout should be non-empty",
    );
}

// `rustty +validate-config` -> exit 0 on a valid tmp `--config-file=…`.
#[test]
fn validate_config_valid() {
    let dir = unique_tmp_dir("validate-ok");
    let cfg = dir.join("good.conf");
    write_file(&cfg, "font-size = 13\n");

    let out = run(
        &[
            "+validate-config",
            &format!("--config-file={}", cfg.display()),
        ],
        &[],
    );
    // The diagnostic sink is stdout; a valid file prints none and exits 0.
    // (The config crate's own `info(config):` log lines go to stderr and do not
    // affect the exit code or the stdout diagnostic stream.)
    assert!(
        out.status.success(),
        "+validate-config on a valid file should exit 0; stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// `rustty +validate-config` -> exit 1 + a diagnostic line on a deliberately-broken
// config file.
#[test]
fn validate_config_broken() {
    let dir = unique_tmp_dir("validate-bad");
    let cfg = dir.join("bad.conf");
    // An invalid value for a known key -> a parse diagnostic (printed to stdout).
    write_file(&cfg, "font-size = notanumber\n");

    let out = run(
        &[
            "+validate-config",
            &format!("--config-file={}", cfg.display()),
        ],
        &[],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "+validate-config on a broken file should exit 1",
    );
    // The diagnostic is written to stdout (`runInner` prints `{diag}\n`).
    assert!(
        !out.stdout.is_empty(),
        "+validate-config on a broken file should print a diagnostic to stdout; stderr={:?}",
        String::from_utf8_lossy(&out.stderr),
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// `rustty +ssh-cache` (XDG_STATE_HOME -> tmp) -> exit 0, and after
// `--add=user@example.com` the listing shows the entry.
#[test]
fn ssh_cache_add_then_list() {
    let dir = unique_tmp_dir("ssh-cache");
    let xdg = dir.to_str().expect("temp dir path is valid UTF-8");

    // An empty cache lists nothing and exits 0.
    let empty = run(&["+ssh-cache"], &[("XDG_STATE_HOME", xdg)]);
    assert!(empty.status.success(), "+ssh-cache (empty) should exit 0");

    // Adding a destination succeeds.
    let add = run(
        &["+ssh-cache", "--add=user@example.com"],
        &[("XDG_STATE_HOME", xdg)],
    );
    assert!(add.status.success(), "+ssh-cache --add should exit 0");

    // Listing now shows the added destination.
    let list = run(&["+ssh-cache"], &[("XDG_STATE_HOME", xdg)]);
    assert!(list.status.success(), "+ssh-cache (list) should exit 0");
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(
        stdout.contains("example.com"),
        "+ssh-cache listing should show the added entry, got: {stdout:?}",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// `rustty +new-window` -> exit 1 + stderr "not supported on this platform" (the
// faithful headless `none`-backend IPC behavior — the seam is wired, not stubbed).
#[test]
fn new_window_not_supported() {
    let out = run(&["+new-window"], &[]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "+new-window should exit 1 over the headless none backend",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not supported on this platform"),
        "+new-window stderr should say it is not supported, got: {stderr:?}",
    );
}

// `rustty` (no args) -> exit 0 + the headless usage banner.
//
// Scoped to the headless `.none` build (the current darwin default): a GUI
// `.gtk` build would take the Phase-3 GUI leaf (`unimplemented!()`) instead of
// printing the banner, so the no-args behavior is only defined for `.none`.
#[test]
fn no_args_usage_banner() {
    if rustty_build_config::app_runtime() != rustty_build_config::Runtime::None {
        // GUI build: no-args launches the app (Phase-3 leaf), not the banner.
        return;
    }

    let out = run(&[], &[]);
    assert!(
        out.status.success(),
        "no-args should exit 0 on the .none build"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Usage: rustty +<action>"),
        "no-args should print the usage banner, got: {stdout:?}",
    );
}

// `rustty +bogus` -> exit 1 + the "unknown CLI action" message (stderr).
#[test]
fn bogus_unknown_action() {
    let out = run(&["+bogus"], &[]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "+bogus should exit 1 (InvalidAction)",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown CLI action"),
        "+bogus stderr should say unknown CLI action, got: {stderr:?}",
    );
}
