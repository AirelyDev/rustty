//! panic_policy.rs — the DG5 C-ABI panic-policy regression anchors
//! (PORT_RULES.md §DG5; VALIDATION_REPORT §3c "No catch_unwind in any of the
//! 271 exported C-ABI fns").
//!
//! POLICY (locked 2026-07-10): a Rust panic reaching a C-ABI boundary ABORTS
//! the process — the R1-faithful mirror of a Zig panic in a `callconv(.c)`
//! export (print + abort, never unwind into the caller, never an error code).
//! Documented placeholder leaves (`unimplemented!()` seams) panic→abort when
//! driven; that IS the policy. Mechanical application: every exported fn is
//! plain `extern "C"` — Rust aborts any panic escaping an `extern "C"`
//! boundary — so no per-fn `catch_unwind` wrappers exist or are wanted.
//!
//! Two pins:
//!  1. `exported_c_abi_panic_aborts_host` — a panicking `extern "C"` fn (the
//!     exact ABI string of all 271 exports) driven in a subprocess must die by
//!     SIGABRT: proves the abort-at-boundary mechanism the policy relies on.
//!  2. `no_c_unwind_in_c_api_sources` — the DG5 mechanical gate: zero
//!     `"C-unwind"` ABI strings in the two C-API crates' sources AND in
//!     `rustty-renderer` (whose display/callback fn-pointers are handed to
//!     foreign ObjC/Core Animation frames — the same boundary). An exported
//!     `extern "C-unwind" fn` would OPT OUT of the abort shim and let a panic
//!     unwind into the C host — a policy violation.

use std::path::{Path, PathBuf};

/// A panicking export-shaped fn: same ABI string (`extern "C"`), same
/// `#[no_mangle]` publication as the real 271 exports. It models any export
/// whose body panics (e.g. a driven placeholder leaf).
#[no_mangle]
pub extern "C" fn rustty_test_panicking_export() -> u32 {
    panic!("placeholder leaf driven (DG5 policy: this must ABORT the host)");
}

#[test]
fn exported_c_abi_panic_aborts_host() {
    const MARKER: &str = "RUSTTY_PANIC_POLICY_CHILD";
    if std::env::var_os(MARKER).is_some() {
        // Child: call the export exactly as a C host would. The panic must be
        // converted to an abort AT the extern "C" boundary — control must
        // never come back here.
        let _ = rustty_test_panicking_export();
        unreachable!("a panic escaping an extern \"C\" fn must abort");
    }
    let exe = std::env::current_exe().expect("current_exe");
    let out = std::process::Command::new(exe)
        .args(["--exact", "exported_c_abi_panic_aborts_host"])
        .env(MARKER, "1")
        .output()
        .expect("spawn child test process");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            Some(6), // SIGABRT
            out.status.signal(),
            "a panicking extern \"C\" export must abort (SIGABRT), not unwind or \
             exit; status={:?} stderr={}",
            out.status,
            String::from_utf8_lossy(&out.stderr),
        );
    }
    // Windows (task W-2): Rust's abort is `__fastfail`, which ends the process
    // with STATUS_STACK_BUFFER_OVERRUN (0xC0000409) whatever the fail code.
    #[cfg(windows)]
    assert_eq!(
        Some(0xC000_0409_u32 as i32),
        out.status.code(),
        "a panicking extern \"C\" export must abort (__fastfail), not unwind or \
         exit; status={:?} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );
}

/// The workspace root (`crates/rustty-apprt-c/../..`).
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("apprt-c crate has a workspace grandparent")
        .to_path_buf()
}

fn scan_dir_for_c_unwind(dir: &Path, hits: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            scan_dir_for_c_unwind(&path, hits);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let src = std::fs::read_to_string(&path).expect("read source");
            for (i, line) in src.lines().enumerate() {
                // The ABI string itself; comments discussing the policy are
                // fine, an `extern "C-unwind"` item is not.
                if line.contains("extern \"C-unwind\"") {
                    hits.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
                }
            }
        }
    }
}

#[test]
fn no_c_unwind_in_c_api_sources() {
    let root = workspace_root();
    let mut hits = Vec::new();
    // The two C-API crates plus rustty-renderer: PORT_RULES DG5 records the
    // renderer's C-ABI callback surface (display path) as verified C-unwind
    // free; keep that claim gated, not a one-time grep.
    //
    // WS14 adds `rustty-font` and `rustty-wasm`: both now carry `extern "C"`
    // surface bound by a JS host (`atlas_*` from `Atlas.zig`'s `Wasm` block, and
    // the `main_wasm.zig` retention module). `wasm32-unknown-unknown` has no
    // unwinder at all, so a `C-unwind` ABI there is not merely wrong per DG5 —
    // it cannot work. Gating it means a future edit cannot quietly introduce one.
    for krate in [
        "rustty-apprt-c",
        "rustty-vt-c",
        "rustty-renderer",
        "rustty-font",
        "rustty-wasm",
    ] {
        scan_dir_for_c_unwind(&root.join("crates").join(krate).join("src"), &mut hits);
    }
    assert!(
        hits.is_empty(),
        "DG5 violation: `extern \"C-unwind\"` in the C-API surface (a panic \
         would unwind into the C host instead of aborting):\n{}",
        hits.join("\n"),
    );
}
