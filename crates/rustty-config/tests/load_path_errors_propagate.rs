//! LEDGER S74-F1 and S74-F3: `Config::load_default_files` and
//! `Config::load_cli_args` must RETURN the error when a base path cannot be
//! resolved, as the reference's `try` does, instead of carrying on with an empty
//! path (which then trips `load_file`'s `assert!(is_absolute)` or expands CLI
//! paths against no base at all).
//!
//! Both tests change process-global state (the captured environment block, the
//! current directory), so they live in their own test binary and run one after
//! the other under a lock rather than beside the crate's unit tests.

use std::sync::Mutex;

use rustty_config::config_struct::LoadError;
use rustty_config::file_load::FileLoadError;
use rustty_config::Config;

static SERIAL: Mutex<()> = Mutex::new(());

/// Reference: `Config.zig:4172-4175` `const legacy_xdg_path = try
/// file_load.legacyDefaultXdgPath(alloc); ... const xdg_path = try
/// file_load.defaultXdgPath(alloc);`. With `XDG_CONFIG_HOME` unset, the XDG
/// lookup reads `$HOME` into a 1024-byte buffer (`xdg.zig` `dir`, `var buf:
/// [1024]u8`), so a longer `HOME` makes the lookup fail. The port used to replace
/// each `try` with `.unwrap_or_default()` and then panicked in `load_file` on the
/// empty path.
#[test]
fn load_default_files_returns_the_xdg_path_error() {
    let _guard = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let saved = rustty_global::environ::environ().clone();
    let home = format!("HOME=/{}", "h".repeat(1100));
    rustty_global::environ::set_environ(rustty_global::environ::Environ::from_entries(vec![
        home.into_bytes()
    ]));

    // The trigger is real: the path helper itself fails in this environment.
    let helper = rustty_config::file_load::legacy_default_xdg_path();
    let mut cfg = Config::default_config();
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cfg.load_default_files()));
    rustty_global::environ::set_environ(saved);

    assert!(
        matches!(helper, Err(FileLoadError::NoHomeDir)),
        "the XDG lookup must fail for a {}-byte HOME, got {helper:?}",
        1101
    );
    let result = result.expect("load_default_files must not panic on a path-lookup error");
    assert!(
        matches!(result, Err(LoadError::FileLoad(FileLoadError::NoHomeDir))),
        "expected the lookup error to propagate, got {result:?}"
    );
}

/// Reference: `Config.zig:4333-4336` `try self.expandPaths(buf[0..try
/// std.Io.Dir.cwd().realPathFile(global.io(), ".", &buf)]);`. A current
/// directory that no longer exists makes `realpath(".")` fail, and the reference
/// returns that error. The port used `.ok()...unwrap_or_default()` and expanded
/// against an empty base.
#[cfg(unix)]
#[test]
fn load_cli_args_returns_the_cwd_realpath_error() {
    let _guard = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let saved_cwd = std::env::current_dir().expect("current_dir");
    let tmp = std::env::temp_dir().join(format!("rustty-s74f3-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("create temp dir");
    std::env::set_current_dir(&tmp).expect("enter temp dir");
    std::fs::remove_dir(&tmp).expect("remove the current directory");

    // The trigger is real: the current directory cannot be resolved.
    let probe = std::fs::canonicalize(".");

    let mut cfg = Config::default_config();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cfg.load_cli_args()));
    std::env::set_current_dir(&saved_cwd).expect("restore cwd");

    assert!(
        probe.is_err(),
        "realpath(\".\") must fail in a deleted directory"
    );
    let result = result.expect("load_cli_args must not panic on a cwd error");
    assert!(
        matches!(result, Err(LoadError::FileNotFound) | Err(LoadError::Io(_))),
        "expected the cwd realpath error to propagate, got {result:?}"
    );
    // Nothing was expanded: the reference returns before `expandPaths` runs,
    // so no `Expand` replay step was recorded.
    assert!(
        !format!("{:?}", cfg._replay_steps).contains("Expand"),
        "no expand step may be recorded when the cwd cannot be resolved"
    );
}
