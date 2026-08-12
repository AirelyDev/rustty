//! config_diagnostics.rs — the `ghostty_config_get_diagnostic` C-ABI parity KAT
//! (codex-vet W93-B01-DIAG-01).
//!
//! `config/CApi.zig:128-133` returns `self._diagnostics.precompute.messages
//! .items[idx].ptr` for any in-range index, and `cli/diagnostics.zig:104-111`
//! makes `precompute_enabled` unconditionally TRUE for `build_config.artifact ==
//! .lib` (libghostty). This test lives in `rustty-apprt-c` — the librustty
//! cdylib/staticlib hub, i.e. the `.lib` artifact — because that is the crate
//! whose `rustty-build-config` edge turns on `artifact-lib`, so the whole build
//! graph resolves `build_config::artifact() == Lib` exactly as the reference's
//! comptime does for libghostty.
//!
//! Before the fix `Artifact::detect()` returned `Exe` in every non-wasm build,
//! so precompute was never populated outside `rustty-args`' own unit tests and
//! an in-range `rustty_config_get_diagnostic` handed C the empty default `""`
//! while `rustty_config_diagnostics_count` reported a nonzero count.

use std::ffi::{CStr, CString};
use std::io::Write;

use rustty_config::c_api::{
    rustty_config_diagnostics_count, rustty_config_free, rustty_config_get_diagnostic,
    rustty_config_load_file, rustty_config_new,
};

/// The `.lib` artifact fact this whole KAT rests on: `rustty-apprt-c` enables
/// `rustty-build-config/artifact-lib`, so the resolved build reports `.lib`.
#[test]
fn apprt_c_build_reports_the_lib_artifact() {
    assert_eq!(
        rustty_build_config::artifact(),
        rustty_build_config::Artifact::Lib,
        "the librustty C-ABI build must report build_config.artifact == .lib",
    );
}

/// Write an absolute-path config file (loadFile asserts the path is absolute)
/// containing a single unknown key, which the args parser records as a
/// diagnostic.
fn write_bad_config() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rustty_apprt_c_diag_{}_{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("config");
    let mut f = std::fs::File::create(&path).expect("create config file");
    f.write_all(b"this-key-does-not-exist = 1\n")
        .expect("write config file");
    path
}

#[test]
fn get_diagnostic_returns_the_precomputed_message_for_the_lib_artifact() {
    let path = write_bad_config();
    let c_path =
        CString::new(path.as_os_str().to_str().expect("utf8 temp path")).expect("no interior NUL");

    let cfg = rustty_config_new();
    assert!(!cfg.is_null());

    // SAFETY: `cfg` is a live pointer from `rustty_config_new`, `c_path` a live
    // NUL-terminated C string.
    unsafe { rustty_config_load_file(cfg, c_path.as_ptr()) };

    // SAFETY: `cfg` is live.
    let count = unsafe { rustty_config_diagnostics_count(cfg) };
    assert_eq!(
        count, 1,
        "the unknown key must produce exactly 1 diagnostic"
    );

    // The expected text is `Diagnostic.format` of the same diagnostic — what
    // the reference precomputes into `precompute.messages`.
    // SAFETY: `cfg` is live.
    let mut expected: Vec<u8> = Vec::new();
    unsafe { &*cfg }._diagnostics.items()[0].format(&mut expected);
    assert!(!expected.is_empty());

    // SAFETY: `cfg` is live; idx 0 is in range.
    let diag = unsafe { rustty_config_get_diagnostic(cfg, 0) };
    assert!(!diag.message.is_null());
    // SAFETY: the C contract is `[*:0]const u8` — the precompute stores the
    // `toOwnedSliceSentinel(0)` NUL, so `CStr` may walk it.
    let message = unsafe { CStr::from_ptr(diag.message) }.to_bytes();
    assert!(
        !message.is_empty(),
        "an in-range diagnostic must not return the empty default message",
    );
    assert_eq!(
        message,
        expected.as_slice(),
        "the C message must be the precomputed Diagnostic.format text",
    );

    // `if (idx >= items.len) return .{};` — out of range keeps the empty default.
    // SAFETY: `cfg` is live.
    let oob = unsafe { rustty_config_get_diagnostic(cfg, count) };
    assert!(!oob.message.is_null());
    // SAFETY: the default message is the `c""` static.
    assert_eq!(unsafe { CStr::from_ptr(oob.message) }.to_bytes(), b"");

    // SAFETY: `cfg` came from `rustty_config_new` and is freed exactly once.
    unsafe { rustty_config_free(cfg) };

    let _ = std::fs::remove_dir_all(path.parent().expect("temp dir"));
}
