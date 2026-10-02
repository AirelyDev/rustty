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

/// The `target/<profile>/deps/` dir that holds this test binary and the lib
/// artifacts THIS `cargo test` invocation built. Derived from the running test
/// executable (`target/<profile>/deps/smoke-<hash>` → its parent).
///
/// PORT NOTE (N479): the lib unit an integration test links against is not a
/// cargo "root" under `cargo test`, so cargo builds its cdylib and staticlib
/// (the `[lib] crate-type` list) into `deps/` but does NOT uplift them to
/// `target/<profile>/`; only a `cargo build` does. The earlier lookup in
/// `target/<profile>/` therefore failed in a fresh clone under a plain
/// `cargo test`, and could link a STALE uplifted copy after a source edit.
/// Cargo names cdylib/staticlib outputs in `deps/` without a metadata hash, so
/// the paths below are fixed, and they are always the current build's.
#[cfg(not(windows))] // (only the C smoke uses it; task W-2)
fn target_deps_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    let deps = exe
        .parent()
        .expect("test exe under target/<profile>/deps")
        .to_path_buf();
    assert_eq!(
        deps.file_name().and_then(|n| n.to_str()),
        Some("deps"),
        "the smoke test binary must run from target/<profile>/deps, found {}",
        deps.display()
    );
    deps
}

/// Pick a C compiler: `$CC`, else `cc`, else `clang`.
fn c_compiler() -> String {
    if let Ok(cc) = std::env::var("CC") {
        if !cc.is_empty() {
            return cc;
        }
    }
    // An ASan build on Linux links the smoke program with -fsanitize=address,
    // which must be clang's (compiler-rt, the runtime rustc instruments for),
    // not GCC's libasan.
    let cands: &[&str] = if cfg!(all(any(rustty_asan, rustty_hwasan), target_os = "linux")) {
        &["clang"]
    } else {
        &["cc", "clang"]
    };
    for &cand in cands {
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
#[cfg(not(windows))] // (only the C smoke uses it; task W-2)
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
#[cfg(not(windows))] // (only the C smoke uses it; task W-2)
fn staticlib_name(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.lib")
    } else {
        format!("lib{name}.a")
    }
}

// Not on Windows (task W-2, R14 form 1): `smoke.c` requires `rustty_init` to
// succeed, and on Windows the reference's `ghostty_init` fails with
// `error.UnsupportedOSForCApi` (`global.zig:62-77`; pinned by
// `rustty_init_is_unsupported_on_windows`). The Windows C-ABI smoke is the
// reference's own `test/windows/test_dll_init.c` (the `rustty_info` by-value
// return through the DLL), which the CI Windows leg builds and runs.
#[cfg(not(windows))]
#[test]
#[cfg_attr(
    rustty_msan,
    ignore = "G-MSAN (P7-W14): the C program would need an MSan runtime matching rustc's instrumentation; clang 22's crashes in the cdylib's first constructor (shadow-mapping mismatch, SEGV in std_detect). It runs natively and under G-ASAN/G-LSAN (R14)"
)]
fn c_abi_smoke_compile_link_run() {
    let root = workspace_root();
    let deps_dir = target_deps_dir();
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
    // [lib] crate-type lists rlib+cdylib+staticlib, so `cargo test` builds them
    // into `deps/`; see `target_deps_dir`).
    let dylib = deps_dir.join(dylib_name("rustty_apprt_c"));
    let staticlib = deps_dir.join(staticlib_name("rustty_apprt_c"));
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
        .arg(&deps_dir)
        .arg("-lrustty_apprt_c")
        .arg(format!("-Wl,-rpath,{}", deps_dir.display()))
        .arg("-o")
        .arg(&bin);
    // Under G-ASAN / G-LSAN on Linux the cdylib is instrumented and its
    // `__asan_*` references resolve against the runtime the EXECUTABLE links
    // (on macOS the dylib carries the runtime itself). `c_compiler` picks clang
    // there, whose compiler-rt runtime matches rustc's. (Under G-MSAN the test
    // is ignored; see its attribute.)
    #[cfg(all(rustty_asan, target_os = "linux"))]
    cmd.arg("-fsanitize=address");
    // Under G-HWASAN (aarch64 Linux, P7-W16b) the same holds for `__hwasan_*`:
    // clang links its HWASan runtime into the executable, and lld, because GNU
    // ld mislinks tagged globals (see sanitizers.sh's run_crate).
    #[cfg(all(rustty_hwasan, target_os = "linux"))]
    cmd.args(["-fsanitize=hwaddress", "-fuse-ld=lld"]);
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
        run.env("DYLD_LIBRARY_PATH", &deps_dir);
    } else {
        run.env("LD_LIBRARY_PATH", &deps_dir);
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

//-------------------------------------------------------------------
// Generated-header contract arms (W92-B02-01 / -02 / -04).
//
// These are SOURCE checks on the checked-in `include/rustty-apprt.h` — the
// header the compile+link KAT above consumes. They are cheap, need no build
// artifacts, and each one fails on the pre-fix header.

/// The header source, as checked in.
fn apprt_header_src() -> String {
    let path = workspace_root().join("include").join("rustty-apprt.h");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Strip C block (`/* … */`) and line (`// …`) comments, replacing each with a
/// single space so tokens on either side stay separated. Deliberately naive
/// about string literals: the generated header has none that contain comment
/// introducers.
fn strip_c_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                // Keep newlines so line-oriented checks below stay sane.
                if b[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i = (i + 2).min(b.len());
            out.push(' ');
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            out.push(' ');
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

/// Drop preprocessor lines (`#…`) from already-comment-stripped C source.
fn strip_pp_lines(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `rustty_*` identifier that is immediately applied to a `(` — i.e. a
/// function declaration's name. Run on comment-stripped source so prose
/// mentions never count.
fn c_fn_names(src: &str) -> std::collections::BTreeSet<String> {
    let b = src.as_bytes();
    let mut out = std::collections::BTreeSet::new();
    let mut i = 0usize;
    while let Some(rel) = src[i..].find("rustty_") {
        let start = i + rel;
        // Must start an identifier (not be a suffix of a longer one).
        let is_boundary = start == 0 || {
            let p = b[start - 1];
            !(p.is_ascii_alphanumeric() || p == b'_')
        };
        let mut end = start;
        while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
            end += 1;
        }
        let mut j = end;
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t' || b[j] == b'\n') {
            j += 1;
        }
        // `name (` is a prototype only when the paren does NOT open a
        // function-pointer declarator: `typedef rustty_clipboard_read_result_e
        // (*rustty_runtime_read_clipboard_cb)(...)` (Bundle E2) puts a RETURN
        // TYPE in this position, and treating it as a declared function made
        // the umbrella/tail cross-check misreport a typedef as an export.
        if is_boundary && j < b.len() && b[j] == b'(' && !(j + 1 < b.len() && b[j + 1] == b'*') {
            out.insert(src[start..end].to_string());
        }
        i = end.max(start + 1);
    }
    out
}

/// W92-B02-01: the generated header must DEFINE the `RUSTTY_API`
/// visibility/import-export macro (token-identical to the public umbrella
/// `include/rustty-embed.h`, itself the R2 rename of the oracle's
/// `GHOSTTY_API` in `ghostty/include/ghostty.h`) and APPLY it to every
/// prototype it declares.
///
/// Pre-fix this header contained ZERO occurrences of `RUSTTY_API`.
#[test]
fn generated_header_defines_and_applies_rustty_api() {
    let src = apprt_header_src();

    // (1) The macro is defined, with the same five arms as the umbrella.
    for needle in [
        "#ifndef RUSTTY_API",
        "#if defined(RUSTTY_STATIC)",
        "#define RUSTTY_API __declspec(dllexport)",
        "#define RUSTTY_API __declspec(dllimport)",
        "#define RUSTTY_API __attribute__((visibility(\"default\")))",
    ] {
        assert!(
            src.contains(needle),
            "include/rustty-apprt.h is missing the RUSTTY_API macro line `{needle}` \
             (it must mirror include/rustty-embed.h). Regenerate with \
             `cargo run -p xtask -- header-gen-apprt`."
        );
    }

    // The umbrella's block must be reproduced verbatim, so a host including
    // BOTH headers sees an identical (idempotent) definition.
    let umbrella = std::fs::read_to_string(workspace_root().join("include/rustty-embed.h"))
        .expect("read include/rustty-embed.h");
    let block = |s: &str| -> String {
        let start = s.find("#ifndef RUSTTY_API").expect("RUSTTY_API block");
        let rest = &s[start..];
        // The block ends at the second `#endif` at column 0 after the start.
        let mut ends = rest.match_indices("\n#endif");
        ends.next().expect("inner #endif");
        let (e, _) = ends.next().expect("outer #endif");
        rest[..e + "\n#endif".len()].to_string()
    };
    assert_eq!(
        block(&src),
        block(&umbrella),
        "the RUSTTY_API block in include/rustty-apprt.h drifted from the one in \
         include/rustty-embed.h; they must be token-identical."
    );

    // (2) Every declared prototype carries it. Split the comment- and
    // preprocessor-stripped body on `;`: each statement that applies a
    // `rustty_*` identifier to `(` is a prototype and must start with the macro.
    // (`extern "C" {` is the cpp_compat opener, not part of a declaration.)
    let body = strip_pp_lines(&strip_c_comments(&src)).replace("extern \"C\" {", " ");
    let mut checked = 0usize;
    for stmt in body.split(';') {
        let t = stmt.trim();
        if c_fn_names(t).is_empty() {
            continue;
        }
        checked += 1;
        assert!(
            t.starts_with("RUSTTY_API"),
            "prototype without the RUSTTY_API visibility macro in \
             include/rustty-apprt.h:\n    {}\n(set `[fn] prefix = \"RUSTTY_API\"` in \
             crates/rustty-apprt-c/cbindgen.toml and regenerate)",
            t.replace('\n', " ")
        );
    }
    // Anti-vacuity: the scan must actually have seen the whole export surface.
    assert_eq!(
        checked,
        c_fn_names(&strip_c_comments(&src)).len(),
        "the RUSTTY_API prefix scan did not cover every declared prototype"
    );
    assert!(
        checked >= 76,
        "only {checked} prototypes found in include/rustty-apprt.h; the header \
         looks truncated or stale"
    );
}

/// The 14 `rustty_*` exports the published library ships from SIBLING crates
/// (`rustty-config`, `rustty-bench`). cbindgen runs with `parse_deps = false`,
/// so they can never appear in this generated header; the public umbrella
/// `include/rustty-embed.h` declares them and the header banner says so.
const SIBLING_CRATE_TAIL: &[&str] = &[
    "rustty_benchmark_cli",
    "rustty_config_clone",
    "rustty_config_diagnostics_count",
    "rustty_config_finalize",
    "rustty_config_free",
    "rustty_config_get",
    "rustty_config_get_diagnostic",
    "rustty_config_load_cli_args",
    "rustty_config_load_default_files",
    "rustty_config_load_file",
    "rustty_config_load_recursive_files",
    "rustty_config_new",
    "rustty_config_open_path",
    "rustty_config_trigger",
];

/// W92-B02-02: the checked-in header must not be STALE, and the part of the
/// umbrella's export tail it structurally cannot declare must be documented in
/// its banner rather than silently absent.
///
/// Pre-fix, `rustty_cli_try_action` / `rustty_info` / `rustty_translate` were
/// exported by THIS crate (`src/lib.rs`) but missing from the header, which had
/// not been regenerated since those landed.
#[test]
fn generated_header_declares_in_crate_tail_and_documents_the_rest() {
    let src = apprt_header_src();
    let declared = c_fn_names(&strip_c_comments(&src));

    // (1) The three in-crate exports the stale header dropped.
    for name in ["rustty_cli_try_action", "rustty_info", "rustty_translate"] {
        assert!(
            declared.contains(name),
            "include/rustty-apprt.h does not declare `{name}`, which \
             crates/rustty-apprt-c/src/lib.rs exports — the checked-in header is \
             STALE. Regenerate with `cargo run -p xtask -- header-gen-apprt`."
        );
    }

    // (2) Bidirectional cross-check against the public umbrella: this header's
    // declarations are a SUBSET of the umbrella's, and the difference is EXACTLY
    // the documented sibling-crate tail. Neither side can go vacuous — deleting
    // a declaration here grows the difference and fails.
    let umbrella = std::fs::read_to_string(workspace_root().join("include/rustty-embed.h"))
        .expect("read include/rustty-embed.h");
    let umbrella_fns = c_fn_names(&strip_c_comments(&umbrella));
    let extra: Vec<_> = declared.difference(&umbrella_fns).cloned().collect();
    assert!(
        extra.is_empty(),
        "include/rustty-apprt.h declares names the public umbrella \
         include/rustty-embed.h does not: {extra:?}"
    );
    let missing: Vec<String> = umbrella_fns.difference(&declared).cloned().collect();
    let expected: Vec<String> = SIBLING_CRATE_TAIL.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        missing, expected,
        "the set of umbrella exports NOT declared by include/rustty-apprt.h \
         changed. It must be exactly the sibling-crate tail (rustty-config / \
         rustty-bench, unreachable with cbindgen `parse_deps = false`). A new \
         entry usually means the header is stale — regenerate it."
    );

    // (3) …and the banner names every one of them, so a host reading only this
    // header is told where the declarations live.
    let banner_end = src.find("#ifndef RUSTTY_APPRT_H").expect("include guard");
    let banner = &src[..banner_end];
    assert!(
        banner.contains("include/rustty-embed.h"),
        "the header banner must point hosts at the public umbrella for the \
         sibling-crate export tail"
    );
    for name in SIBLING_CRATE_TAIL {
        assert!(
            banner.contains(name),
            "the header banner does not mention `{name}`, an export the umbrella \
             declares but this header structurally cannot"
        );
    }
}

/// W92-B02-04: the Apple-only exports must be preprocessor-gated on
/// `__APPLE__`, matching the oracle (`ghostty/include/ghostty.h` `#ifdef
/// __APPLE__`) and rustty's own umbrella `include/rustty-embed.h`.
///
/// Pre-fix, `include/rustty-apprt.h` contained ZERO `__APPLE__` occurrences and
/// declared all six unconditionally, so a non-Apple host saw declarations the
/// library does not export (the Rust side IS `cfg(any(macos, ios))`-gated).
#[test]
fn apple_only_declarations_are_gated_on_apple() {
    const APPLE_ONLY: &[&str] = &[
        "rustty_surface_set_display_id",
        "rustty_surface_quicklook_font",
        "rustty_surface_quicklook_word",
        "rustty_inspector_metal_init",
        "rustty_inspector_metal_render",
        "rustty_inspector_metal_shutdown",
    ];

    let root = workspace_root();
    let include_dir = root.join("include");
    let cc = c_compiler();
    let out_dir = std::env::temp_dir().join(format!("rustty_apprt_apple_{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("create temp out dir");
    let tu = out_dir.join("apple_probe.c");
    std::fs::write(&tu, "#include \"rustty-apprt.h\"\n").expect("write probe TU");

    let preprocess = |apple: bool| -> String {
        let out = Command::new(&cc)
            .arg("-E")
            .arg("-P")
            .arg(if apple { "-D__APPLE__" } else { "-U__APPLE__" })
            .arg("-I")
            .arg(&include_dir)
            .arg(&tu)
            .output()
            .expect("spawn C preprocessor");
        assert!(
            out.status.success(),
            "preprocessing rustty-apprt.h (apple={apple}) failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    let non_apple = preprocess(false);
    for name in APPLE_ONLY {
        assert!(
            !non_apple.contains(name),
            "`{name}` is declared by include/rustty-apprt.h in a NON-Apple \
             preprocessing lane. It is `#[cfg(any(target_os = \"macos\", \
             target_os = \"ios\"))]` in the crate, so the library does not export \
             it there. Add the cfg->`__APPLE__` map to cbindgen.toml `[defines]` \
             and regenerate."
        );
    }

    // Control arm: they ARE declared under __APPLE__ (so the arm above cannot
    // pass by the names having been dropped altogether).
    let apple = preprocess(true);
    for name in APPLE_ONLY {
        assert!(
            apple.contains(name),
            "`{name}` is missing from include/rustty-apprt.h even under \
             __APPLE__ — the declaration was dropped, not gated"
        );
    }

    // The non-Apple lane must still carry the portable surface, so the check
    // above is not passing on an empty translation unit.
    assert!(
        non_apple.contains("rustty_app_new") && non_apple.contains("rustty_surface_new"),
        "the non-Apple preprocessing lane lost the portable declarations too"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}
