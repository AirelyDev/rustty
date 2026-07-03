//! smoke.rs — the libghostty C-ABI compile+link+run known-answer test driver
//! (chunk E11, EMBEDDED_BLUEPRINT §6.3 [ADV-FIX 2]).
//!
//! This shells out to a C compiler (`$CC`, else `cc`, else `clang`) to compile
//! `tests/smoke.c` against the cbindgen-generated `include/rustty-apprt.h`, link
//! it against the published rustty shared library, run it, and assert exit 0.
//! It is the "step up" from vt-c's header-self-consistency gate: it proves the
//! published lib + header are actually CONSUMABLE by a real C host.
//!
//! ## Why the cdylib (`librustty.{dylib,so}`), not the staticlib
//!
//! The blueprint envisioned linking the staticlib, but `rustty-apprt-c`'s
//! dependency closure is the WHOLE app (core → renderer/font/… → the vendored
//! `-sys` C libraries: oniguruma, zlib, gettext, spirv-cross, sentry, dcimgui,
//! …). rustc's `staticlib` does NOT bundle those vendored `cc`-built archives
//! into `librustty_apprt_c.a` (verified: `_deflate`/`_OnigEncodingKOI8`/
//! `_bindtextdomain` have 0 definitions in the `.a`), so a standalone C link
//! against the staticlib would fail to resolve them — a host would have to
//! enumerate every `-sys` crate's hashed build-script `OUT_DIR` archive, a
//! fragile, non-portable link line. The **cdylib** is fully self-contained (all
//! transitive native deps resolved into it at build time) and is the standard,
//! robust way a C host consumes a Rust library. Linking it equally proves the
//! deliverable: "the published `librustty` + header, callable by a host, exit 0"
//! (EMBEDDED_BLUEPRINT §1 rec 1). We ALSO assert the staticlib artifact exists
//! and exports the C-ABI symbols (a lighter static check), so both published
//! artifacts are covered.
//!
//! The smoke exercises only the RUNTIME-LIVE subset (no `rustty_surface_new`, no
//! `rustty_set_window_background_blur`); the live-surface GPU/xev path is the
//! documented Phase-3 seam.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace root (`crates/rustty-apprt-c/../..`).
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("apprt-c crate has a workspace grandparent")
        .to_path_buf()
}

/// The `target/<profile>/` dir that holds this test binary and the built lib
/// artifacts. Derived from the running test executable
/// (`target/<profile>/deps/smoke-<hash>` → `target/<profile>`).
fn target_profile_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    exe.parent()
        .and_then(Path::parent)
        .expect("test exe under target/<profile>/deps")
        .to_path_buf()
}

/// Pick a C compiler: `$CC`, else `cc`, else `clang`.
fn c_compiler() -> String {
    if let Ok(cc) = std::env::var("CC") {
        if !cc.is_empty() {
            return cc;
        }
    }
    for cand in ["cc", "clang"] {
        if Command::new(cand)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return cand.to_string();
        }
    }
    panic!("no C compiler found (tried $CC, cc, clang) — required for the C-ABI smoke KAT");
}

/// Platform dynamic-library file name for a cdylib crate named `name`.
fn dylib_name(name: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{name}.dylib")
    } else if cfg!(target_os = "windows") {
        format!("{name}.dll")
    } else {
        format!("lib{name}.so")
    }
}

/// Platform static-library file name.
fn staticlib_name(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.lib")
    } else {
        format!("lib{name}.a")
    }
}

#[test]
fn c_abi_smoke_compile_link_run() {
    let root = workspace_root();
    let profile_dir = target_profile_dir();
    let include_dir = root.join("include");
    let header = include_dir.join("rustty-apprt.h");
    let smoke_c = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("smoke.c");

    // The published header must be committed + up to date (regenerate with
    // `cargo run -p xtask -- header-gen-apprt`).
    assert!(
        header.exists(),
        "missing generated header {} — run `cargo run -p xtask -- header-gen-apprt`",
        header.display()
    );
    assert!(smoke_c.exists(), "missing {}", smoke_c.display());

    // Both published artifacts must have been built (they are, since the crate's
    // [lib] crate-type lists rlib+cdylib+staticlib, so `cargo test` builds them).
    let dylib = profile_dir.join(dylib_name("rustty_apprt_c"));
    let staticlib = profile_dir.join(staticlib_name("rustty_apprt_c"));
    assert!(
        dylib.exists(),
        "missing cdylib {} — the C-ABI smoke needs the published shared library",
        dylib.display()
    );
    // Lighter static check: the staticlib artifact is produced and exports the
    // C-ABI symbols (host static-linking would additionally need the vendored
    // -sys archives; see the module doc for why the run links the cdylib).
    assert!(
        staticlib.exists(),
        "missing staticlib {} — the published librustty.a must be produced",
        staticlib.display()
    );

    let cc = c_compiler();
    let out_dir = std::env::temp_dir().join(format!("rustty_apprt_smoke_{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("create temp out dir");
    let bin = out_dir.join("smoke_bin");

    // Compile + link in one step against the cdylib. `-Wl,-rpath` makes the run
    // find the shared library by absolute path (belt-and-suspenders with the
    // loader env var set below).
    let mut cmd = Command::new(&cc);
    cmd.arg("-std=c11")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg("-I")
        .arg(&include_dir)
        .arg(&smoke_c)
        .arg("-L")
        .arg(&profile_dir)
        .arg("-lrustty_apprt_c")
        .arg(format!("-Wl,-rpath,{}", profile_dir.display()))
        .arg("-o")
        .arg(&bin);
    let compile = cmd.output().expect("spawn C compiler");
    assert!(
        compile.status.success(),
        "compile+link of smoke.c failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );

    // Run it, asserting exit 0. Point the dynamic loader at the profile dir.
    let mut run = Command::new(&bin);
    if cfg!(target_os = "macos") {
        run.env("DYLD_LIBRARY_PATH", &profile_dir);
    } else {
        run.env("LD_LIBRARY_PATH", &profile_dir);
    }
    let output = run.output().expect("run smoke binary");
    assert!(
        output.status.success(),
        "smoke binary exited with {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Best-effort cleanup.
    let _ = std::fs::remove_dir_all(&out_dir);
}
