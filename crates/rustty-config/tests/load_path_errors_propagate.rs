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

    // The error is the reference's own, `homedir.zig`'s `error.BufferTooSmall`
    // (`try homedir.home(...)`, xdg.zig:92). Until LEDGER S112-F11 the port
    // reported it as `NoHomeDir` (NEW-FINDINGS N140), and this test pinned that.
    assert!(
        matches!(helper, Err(FileLoadError::BufferTooSmall)),
        "the XDG lookup must fail for a {}-byte HOME, got {helper:?}",
        1101
    );
    let result = result.expect("load_default_files must not panic on a path-lookup error");
    assert!(
        matches!(
            result,
            Err(LoadError::FileLoad(FileLoadError::BufferTooSmall))
        ),
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

/// NEW-FINDINGS N140 (a), task S-15. Reference `theme.zig:31-58` (pin
/// `20abdb50a`): `Location.dir`'s `.user` arm maps `xdg.config`'s
/// `error.BufferTooSmall` (a `$HOME` longer than the 1024-byte lookup buffer,
/// `xdg.zig:91-92`) to `error.OutOfMemory`, and every `try` above it passes that
/// on: `LocationIterator.next` (`try location.dir(..)`), `open` (`while (try
/// it.next())`), `Config.loadTheme` (`try themepkg.open(..)`) and
/// `Config.finalize` (`try self.loadTheme(theme.*)`). The port returned `None`
/// ("the directory doesn't exist") and finalized successfully.
#[test]
fn theme_user_dir_buffer_too_small_is_out_of_memory() {
    use rustty_config::theme::{self, Location, LocationIterator};
    use rustty_config::value_types::Theme;

    let _guard = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let saved = rustty_global::environ::environ().clone();
    let home = format!("HOME=/{}", "h".repeat(1100));
    rustty_global::environ::set_environ(rustty_global::environ::Environ::from_entries(vec![
        home.into_bytes()
    ]));

    // The trigger is real: the XDG lookup itself fails with BufferTooSmall.
    let xdg = rustty_config::file_load::legacy_default_xdg_path();
    let dir = Location::User.dir();
    let mut it = LocationIterator::new();
    let next = it.next().map(|loc| loc.map(|l| l.location));
    let mut diags = rustty_args::diagnostics::DiagnosticList::default();
    let opened = theme::open(b"some-theme", &mut diags).map(|f| f.is_some());
    let mut cfg = Config::default_config();
    cfg.theme = Some(Theme {
        light: b"some-theme".to_vec(),
        dark: b"some-theme".to_vec(),
    });
    let finalized = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cfg.finalize()));
    rustty_global::environ::set_environ(saved);

    assert!(
        matches!(xdg, Err(FileLoadError::BufferTooSmall)),
        "the XDG lookup must fail with BufferTooSmall for a 1101-byte HOME, got {xdg:?}"
    );
    assert_eq!(Err(theme::Error::OutOfMemory), dir, "Location.dir(.user)");
    assert_eq!(
        Err(theme::Error::OutOfMemory),
        next,
        "LocationIterator.next"
    );
    assert_eq!(Err(theme::Error::OutOfMemory), opened, "theme.open");
    // `try` returns before any diagnostic is appended.
    assert!(
        diags.items().is_empty(),
        "open must not add a diagnostic on OOM"
    );
    let finalized = finalized.expect("finalize must not panic on a theme-dir error");
    assert!(
        matches!(finalized, Err(LoadError::OutOfMemory)),
        "Config.finalize must return OutOfMemory, got {finalized:?}"
    );
    // The C API's `err={}` renders it as Zig does (NEW-FINDINGS N140 (b)).
    assert_eq!(
        "error.OutOfMemory",
        rustty_config::zig_error::load_error(&finalized.unwrap_err())
    );
}

/// The control for the test above: with a short `HOME` the same calls succeed,
/// so the error there comes from the long `HOME` and nothing else.
#[test]
fn theme_user_dir_resolves_with_a_short_home() {
    use rustty_config::theme::{Location, LocationIterator};

    let _guard = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let saved = rustty_global::environ::environ().clone();
    rustty_global::environ::set_environ(rustty_global::environ::Environ::from_entries(vec![
        b"HOME=/rustty-s15-home".to_vec(),
    ]));
    let dir = Location::User.dir();
    let mut it = LocationIterator::new();
    let first = it.next().map(|r| r.map(|l| (l.location, l.dir)));
    rustty_global::environ::set_environ(saved);

    let want = format!(
        "/rustty-s15-home/.config/{}/themes",
        rustty_identity::CONFIG_DIR
    )
    .into_bytes();
    assert_eq!(Ok(Some(want.clone())), dir);
    assert_eq!(Ok(Some((Location::User, want))), first);
}
