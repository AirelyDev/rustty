//! xtask — build orchestration for rustty.
//!
//! PORTS: ghostty src/build/ (34 files) + helpgen.zig — build orchestration:
//! cbindgen headers, terminfo compilation, shell-integration/theme bundling,
//! man pages, i18n, dist tarball, macOS xcframework/.app.
//!
//! PORT STATUS: scaffold (only the `header-gen` subcommand is wired so far).
//!
//! Independent of the parity port order (R4). NO ghostty Cargo dep (R3).

mod wasm_patch_growable_table;

use std::path::PathBuf;
use std::process::{Command, ExitCode};

mod res_editor;
mod res_misc;
mod res_shell;
mod res_shell_integration;
mod res_terminfo;
mod res_themes;

/// Single source of truth for the resource-tree OUTPUT name (rebrand Task 14):
/// `rustty` by default, `ghostty` under this crate's own `identity-oracle`
/// feature. Every `res_*` module derives its `share/{APP}/…` tree, man pages,
/// and completion filenames from this constant, so the whole `zig-out/share/`
/// output flips atomically between profiles. NOT used for values that are
/// pinned regardless of this profile (documentation/content data-identity,
/// the compiled terminfo entry's primary name, or filenames whose format
/// independently requires the reference spelling — see the per-module notes,
/// e.g. `res_editor.rs`'s vim/nvim filetype-plugin filenames).
///
/// This is a LOCAL `cfg`-gated mirror of `rustty_identity::APP_NAME`'s two
/// values, NOT a call into that crate. Historically this dodged a real
/// feature-unification bug: this crate's `rustty-terminfo` dependency used to
/// force `identity-oracle` on UNCONDITIONALLY, which (via Cargo feature
/// unification on the shared `rustty-identity` package) made
/// `rustty_identity::APP_NAME` resolve to `"ghostty"` even in the default
/// profile. Task 18 removed that unconditional force (`rustty-terminfo`'s
/// `identity-oracle` is now forwarded from this crate's OWN feature of the
/// same name — see `xtask/Cargo.toml`), so the poisoning no longer occurs;
/// this local mirror is kept as-is (harmless, and touching every `res_*`
/// module that reads it is not worth the churn) rather than switched back to
/// a direct `rustty_identity::APP_NAME` call.
#[cfg(not(feature = "identity-oracle"))]
pub(crate) const APP: &str = "rustty";
/// See the `not(feature = "identity-oracle")` arm above for the full doc.
#[cfg(feature = "identity-oracle")]
pub(crate) const APP: &str = "ghostty";

const USAGE: &str = "\
xtask — rustty build orchestration

USAGE:
    cargo run -p xtask -- <SUBCOMMAND>

SUBCOMMANDS:
    header-gen         Generate include/rustty.h from rustty-vt-c via cbindgen (R2)
    header-gen-apprt   Generate include/rustty-apprt.h from rustty-apprt-c via cbindgen (R2)
    xcframework        Package rustty-apprt-c into dist/RusttyKit.xcframework (macOS Swift-app tier)
    resources          Generate zig-out/share/{terminfo,<app>,vim,nvim,fish,zsh,bash-completion,man,locale,bat} for the macOS Copy-Resources phase (<app> = rustty by default, ghostty under identity-oracle)
    wasm-patch-growable-table <in.wasm> <out.wasm>
                       Patch a WASM binary's table section to be growable (R2)
    help               Show this help
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("header-gen") => match header_gen() {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: header-gen failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("header-gen-apprt") => match header_gen_apprt() {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: header-gen-apprt failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("xcframework") => match xcframework() {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: xcframework failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("resources") => match resources() {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: resources failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("wasm-patch-growable-table") => match wasm_patch_growable_table_cmd(&mut args) {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: wasm-patch-growable-table failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("help") | Some("--help") | Some("-h") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("xtask: unknown subcommand `{other}`\n");
            print!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Workspace root = the parent of this crate's directory (`xtask/..`).
fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("xtask has a parent dir")
        .to_path_buf()
}

/// Generate `include/rustty.h` from the `rustty-vt-c` crate via the cbindgen
/// CLI (R2: `rustty_*` symbols, `include/rustty.h` + `rustty/vt/*`).
///
/// Per R2 the reference `ghostty.h` ABI-equivalence gate is DROPPED; the gate
/// here is header self-consistency (cbindgen succeeds + the header compiles as
/// C) plus behavioral parity via the `identity-oracle` harness.
///
/// This shells out to the installed `cbindgen` CLI rather than linking the
/// cbindgen crate, keeping xtask light. Equivalent manual command:
///
/// ```sh
/// cbindgen \
///   --config crates/rustty-vt-c/cbindgen.toml \
///   --crate rustty-vt-c \
///   --output include/rustty.h
/// ```
fn header_gen() -> Result<PathBuf, String> {
    let root = workspace_root();
    let config = root.join("crates/rustty-vt-c/cbindgen.toml");
    let include_dir = root.join("include");
    let output = include_dir.join("rustty.h");

    std::fs::create_dir_all(&include_dir).map_err(|e| format!("create include/: {e}"))?;

    let status = Command::new("cbindgen")
        .arg("--config")
        .arg(&config)
        .arg("--crate")
        .arg("rustty-vt-c")
        .arg("--output")
        .arg(&output)
        // Run from the WORKSPACE ROOT (not the crate dir). With `parse_deps =
        // true` cbindgen must resolve the sibling workspace crates that define
        // the cross-crate C-ABI types (rustty-vt / rustty-lib /
        // rustty-render-types); resolving those is reliable only from the root
        // (matches the documented manual `cbindgen --config ... --crate
        // rustty-vt-c --output include/rustty.h` invocation run at the root).
        .current_dir(&root)
        .status()
        .map_err(|e| format!("spawn cbindgen (install via `cargo install cbindgen`): {e}"))?;

    if !status.success() {
        return Err(format!("cbindgen exited with {status}"));
    }
    Ok(output)
}

/// Generate `include/rustty-apprt.h` from the `rustty-apprt-c` crate via the
/// cbindgen CLI (R2: the libghostty embedding C ABI, `rustty_*` symbols, chunk
/// E11 of EMBEDDED_BLUEPRINT §3/§6).
///
/// Like [`header_gen`] the reference `ghostty.h` ABI-equivalence gate is DROPPED
/// (R2); the gate is header self-consistency (cbindgen succeeds + the header
/// compiles standalone as **C and C++**, `cpp_compat = true`) PLUS the C
/// compile+link smoke KAT (`crates/rustty-apprt-c/tests/smoke.rs` drives
/// `smoke.c` against the staticlib). cbindgen's `Can't find` warnings for the
/// cross-crate types are EXPECTED — `parse_deps = false`, so those layouts are
/// hand-pinned in the crate's `cbindgen.toml` `after_includes` (rustty-vt-c
/// precedent); cbindgen references them by name only.
///
/// Equivalent manual command (run from the workspace root):
///
/// ```sh
/// cbindgen \
///   --config crates/rustty-apprt-c/cbindgen.toml \
///   --crate rustty-apprt-c \
///   --output include/rustty-apprt.h
/// ```
fn header_gen_apprt() -> Result<PathBuf, String> {
    let root = workspace_root();
    let config = root.join("crates/rustty-apprt-c/cbindgen.toml");
    let include_dir = root.join("include");
    let output = include_dir.join("rustty-apprt.h");

    std::fs::create_dir_all(&include_dir).map_err(|e| format!("create include/: {e}"))?;

    let status = Command::new("cbindgen")
        .arg("--config")
        .arg(&config)
        .arg("--crate")
        .arg("rustty-apprt-c")
        .arg("--output")
        .arg(&output)
        // Run from the WORKSPACE ROOT (matches the documented manual invocation).
        .current_dir(&root)
        .status()
        .map_err(|e| format!("spawn cbindgen (install via `cargo install cbindgen`): {e}"))?;

    if !status.success() {
        return Err(format!("cbindgen exited with {status}"));
    }
    Ok(output)
}

/// Package `rustty-apprt-c` into `dist/RusttyKit.xcframework` for the macOS
/// Swift-app tier (plan `2026-07-12-macos-swift-app-milestone` Task B1).
///
/// Mirrors ghostty's `src/build/GhosttyXCFramework.zig` recipe: build the
/// per-arch static lib, stage a headers dir (rustty's validated embedding
/// umbrella `include/rustty-embed.h`, staged as `rustty.h`, + a `RusttyKit`
/// module map — the Swift app links the canonical `rustty_*` ABI directly),
/// then `xcodebuild -create-xcframework`. The staticlib is built under the
/// `xcframework` profile (`panic = "abort"`, PREPORT_SWIFT_APP §4) using the
/// DEFAULT feature set — the `identity-oracle` alias layer was deleted
/// (rebrand Task 1), so only `rustty_*` symbols are exported.
///
/// macOS-arm64 slice only (the host target). A universal slice would add
/// `x86_64-apple-darwin` via `lipo`; deferred until that target is installed.
///
/// R3: uses rustty's OWN validated `include/rustty-embed.h` (the frozen
/// ghostty.h token-renamed and checked against the pinned upstream, per
/// `check_ghostty_h_abi.sh`), never the ghostty checkout.
/// The minimum macOS version the shipped staticlib targets. Matches the floor
/// of the reused Swift app's `MACOSX_DEPLOYMENT_TARGET` in
/// `macos/Rustty.xcodeproj` (the main app + most configs are 13.0; a few
/// helpers are 13.1/15.5). Pinning the Rust/C build to this floor makes the
/// object files compatible with every app target and silences the ld
/// "object file was built for newer macOS version than being linked" warnings
/// that appear when the host toolchain defaults to the running OS (e.g. 26.x).
const MACOS_DEPLOYMENT_TARGET: &str = "13.0";

fn xcframework() -> Result<PathBuf, String> {
    let root = workspace_root();
    let target = "aarch64-apple-darwin";

    // 1. Build the DEFAULT-profile staticlib under the abort profile.
    //
    // The shipped lib exports ONLY the canonical `rustty_*` ABI — the
    // `identity-oracle` alias feature was deleted (rebrand Task 1), so it is
    // deliberately NOT passed here; the Swift app links `rustty_*` directly.
    //
    // MACOSX_DEPLOYMENT_TARGET pins the minimum-OS the compiler stamps into
    // every object: rustc passes it to LLVM's target triple and the `cc`-driven
    // -sys builds (zlib/freetype/harfbuzz/…) forward it to clang's
    // `-mmacosx-version-min`. Without it the host toolchain stamps the running
    // OS, so the app (deployment target 13.0) gets "built for newer macOS"
    // linker warnings. Set as an env on the Command so it reaches BOTH rustc and
    // every build-script `cc` invocation this cargo build spawns.
    let status = Command::new("cargo")
        .args([
            "build",
            "-p",
            "rustty-apprt-c",
            "--profile",
            "xcframework",
            "--target",
            target,
        ])
        .env("MACOSX_DEPLOYMENT_TARGET", MACOS_DEPLOYMENT_TARGET)
        .current_dir(&root)
        .status()
        .map_err(|e| format!("spawn cargo build: {e}"))?;
    if !status.success() {
        return Err(format!("cargo build exited with {status}"));
    }
    let rust_lib = root.join(format!("target/{target}/xcframework/librustty_apprt_c.a"));
    if !rust_lib.exists() {
        return Err(format!("staticlib not found at {}", rust_lib.display()));
    }

    // 1b. Combine the Rust staticlib with its vendored C archives into ONE
    // self-contained lib (ghostty's CombineArchivesStep). A Rust `staticlib`
    // records its C link deps as metadata but does NOT bundle the -sys crates'
    // static archives (zlib/gettext/harfbuzz/freetype/oniguruma/…), so the final
    // xcodebuild link would report undefined `_deflate`/`_dgettext`/… . libtool
    // folds rustty's OWN vendored C archives (R3 — not Homebrew/system) into the
    // shipped lib so the Swift app links against a single self-contained archive.
    let build_dir = root.join(format!("target/{target}/xcframework/build"));
    let mut archives: Vec<PathBuf> = vec![rust_lib.clone()];
    if let Ok(entries) = std::fs::read_dir(&build_dir) {
        for e in entries.flatten() {
            let out = e.path().join("out");
            if let Ok(outs) = std::fs::read_dir(&out) {
                for f in outs.flatten() {
                    let p = f.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("a") {
                        archives.push(p);
                    }
                }
            }
        }
    }
    let lib = root.join(format!("target/{target}/xcframework/libRusttyKit.a"));
    let _ = std::fs::remove_file(&lib);
    let mut lt = Command::new("libtool");
    lt.arg("-static").arg("-o").arg(&lib);
    for a in &archives {
        lt.arg(a);
    }
    let status = lt
        .current_dir(&root)
        .status()
        .map_err(|e| format!("spawn libtool: {e}"))?;
    if !status.success() {
        return Err(format!(
            "libtool combine exited with {status} ({} archives)",
            archives.len()
        ));
    }
    if !lib.exists() {
        return Err(format!("combined lib not found at {}", lib.display()));
    }

    // 2. Stage the headers dir: the rustty embedding umbrella staged as
    // rustty.h + the RusttyKit module map.
    let dist = root.join("dist");
    let headers = dist.join("xcframework-headers");
    let _ = std::fs::remove_dir_all(&headers);
    std::fs::create_dir_all(&headers).map_err(|e| format!("mkdir headers: {e}"))?;
    let src_h = root.join("include/rustty-embed.h");
    if !src_h.exists() {
        return Err(format!("embedding umbrella header missing: {}", src_h.display()));
    }
    std::fs::copy(&src_h, headers.join("rustty.h"))
        .map_err(|e| format!("copy rustty.h: {e}"))?;
    // Umbrella module map — same shape as upstream GhosttyKit.xcframework.
    std::fs::write(
        headers.join("module.modulemap"),
        "// RusttyKit module for the rustty macOS Swift app (canonical rustty_* ABI).\n\
         module RusttyKit {\n    umbrella header \"rustty.h\"\n    export *\n}\n",
    )
    .map_err(|e| format!("write modulemap: {e}"))?;

    // 3. xcodebuild -create-xcframework.
    let out = dist.join("RusttyKit.xcframework");
    let _ = std::fs::remove_dir_all(&out);
    let status = Command::new("xcodebuild")
        .args(["-create-xcframework", "-library"])
        .arg(&lib)
        .arg("-headers")
        .arg(&headers)
        .arg("-output")
        .arg(&out)
        .current_dir(&root)
        .status()
        .map_err(|e| format!("spawn xcodebuild (need Xcode command-line tools): {e}"))?;
    if !status.success() {
        return Err(format!("xcodebuild exited with {status}"));
    }
    Ok(out)
}

/// Generate the `zig-out/share/` resource tree the macOS Xcode Copy-Resources
/// phase copies into `Rustty.app` (referenced as `../zig-out/share/<name>`
/// from `macos/Rustty.xcodeproj`).
///
/// Ports the resource branch of ghostty's `zig build install` (the
/// `GhosttyResources.zig` / `GhosttyDocs.zig` / `GhosttyI18n.zig` install steps
/// that populate `zig-out/share/`). Each subtree is produced by its own
/// disjoint generator module driving the already-ported pure Rust generators
/// (`rustty-terminfo`, `rustty-extra`, `rustty-helpgen`, …):
///
///   * [`res_terminfo::gen_terminfo`] — `share/{terminfo,<app>}` (runtime-critical).
///   * [`res_shell::gen_shell`]       — `share/{bash-completion,zsh,fish}`.
///   * [`res_editor::gen_editor`]     — `share/{vim,nvim}`.
///   * [`res_misc::gen_misc`]         — `share/{man,locale,bat}`.
///
/// All ten folders end up present so the Xcode Copy-Resources phase resolves
/// every reference (R5: any not-yet-ported generator still leaves a minimal
/// VALID folder + a DEFERRALS entry, never fake content).
fn resources() -> Result<PathBuf, String> {
    let root = workspace_root();
    let share = root.join("zig-out/share");
    std::fs::create_dir_all(&share).map_err(|e| format!("create zig-out/share/: {e}"))?;

    res_terminfo::gen_terminfo(&share).map_err(|e| format!("gen_terminfo: {e}"))?;
    res_shell::gen_shell(&share).map_err(|e| format!("gen_shell: {e}"))?;
    res_editor::gen_editor(&share).map_err(|e| format!("gen_editor: {e}"))?;
    res_misc::gen_misc(&share).map_err(|e| format!("gen_misc: {e}"))?;
    res_themes::gen_themes(&share).map_err(|e| format!("gen_themes: {e}"))?;
    res_shell_integration::gen_shell_integration(&share)
        .map_err(|e| format!("gen_shell_integration: {e}"))?;

    Ok(share)
}

/// Port of ghostty `src/build/wasm_patch_growable_table.zig`'s `main()`: read
/// `<input.wasm>`, patch its table section to be growable, write `<output.wasm>`.
///
/// The pure LEB128/section-rewrite logic lives in
/// [`wasm_patch_growable_table::patch_table_growable`].
fn wasm_patch_growable_table_cmd(
    args: &mut impl Iterator<Item = String>,
) -> Result<PathBuf, String> {
    // Parse args: input output
    let input = args
        .next()
        .ok_or("usage: wasm-patch-growable-table <input.wasm> <output.wasm>")?;
    let output = args
        .next()
        .ok_or("usage: wasm-patch-growable-table <input.wasm> <output.wasm>")?;

    // Patch the file.
    let bytes = std::fs::read(&input).map_err(|e| format!("read {input}: {e}"))?;
    let patched = wasm_patch_growable_table::patch_table_growable(&bytes)
        .map_err(|e| format!("patch failed: {e:?}"))?;

    // Write our output
    let output = PathBuf::from(output);
    std::fs::write(&output, patched.as_ref())
        .map_err(|e| format!("write {}: {e}", output.display()))?;
    Ok(output)
}
