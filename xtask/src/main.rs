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

mod dist;

mod release;

mod archives;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod main_build_data;
mod res_editor;
mod res_linux;
mod res_misc;
mod res_shell;
mod res_shell_integration;
mod res_terminfo;
mod res_themes;
mod translations;

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
    xcframework-vt     Package rustty-vt-c into dist/rustty-vt.xcframework (the VT library for
                       Swift consumers; module RusttyVt). Port of ghostty's
                       ghostty-vt.xcframework — the artifact release-tip.yml's
                       build-lib-vt-xcframework job ships and example/swift-vt-xcframework
                       consumes.
    resources [--target-os <os>] [--prefix <prefix>] [--flatpak] [--debug] [--system-package]
                       Generate zig-out/share/{terminfo,<app>,vim,nvim,fish,zsh,bash-completion,man,locale,bat} for the macOS Copy-Resources phase (<app> = rustty by default, ghostty under identity-oracle).
                       --target-os (linux|macos|freebsd|windows|…, default: host) selects the
                       TARGET layout like ghostty's cfg.target: freebsd puts the terminfo tree
                       under share/site-terminfo, windows emits only the terminfo source, and
                       linux ALSO stages the desktop-app resources (dist/linux templates +
                       hicolor icons — GhosttyResources.zig addLinuxAppResources). For the
                       linux arm: --prefix is the FINAL install prefix baked into Exec= lines
                       (default /usr), --flatpak swaps the D-Bus service + caps icons at 512,
                       --debug uses the -debug app id, --system-package puts the systemd user
                       unit under zig-out/lib instead of zig-out/share.
    update-translations
                       Regenerate po/io.airely.rustty.pot from the GTK-apprt + Nautilus
                       sources and msgmerge every po/*.po catalog (needs xgettext+msgmerge
                       on PATH; on macOS prepend /opt/homebrew/opt/gettext/bin). LIVE since
                       WS13: after running it, `cargo test -p rustty-apprt --test
                       i18n_msgid_coverage` must stay green (it gates msgid-set drift).
    gen <+action>      Emit one build-time data artifact to stdout: bash, fish, zsh,
                       sublime, vim-syntax, vim-ftdetect, vim-ftplugin, vim-compiler,
                       terminfo. Port of ghostty's `src/main_build_data.zig` `+gen-*`
                       CLI; the same bytes `resources` writes into zig-out/share.
    dist               Build the source tarball at dist/rustty-<version>.tar.gz (git archive)
    distcheck          Build the source tarball, extract it, and build it to validate self-containment
    release-info [--version <vX.Y.Z>] [--ref <refs/tags/vX.Y.Z>] [--no-workspace-check]
                       Emit the release metadata (version, build, commit, commit_long)
                       the .github/workflows/release-{tag,tip}.yml pipelines consume,
                       to stdout and to $GITHUB_OUTPUT. Port of those workflows'
                       `setup` job. `--help` for details.
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
        Some("xcframework-vt") => match xcframework_vt() {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: xcframework-vt failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("gen") => match main_build_data::run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xtask: gen failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("resources") => match resources(&mut args) {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: resources failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("update-translations") => match translations::update_translations() {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: update-translations failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("dist") => match dist::dist() {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: dist failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("distcheck") => match dist::distcheck() {
            Ok(out) => {
                println!("distcheck OK: {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: distcheck failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("release-info") => match release::run(&mut args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xtask: release-info failed: {e}");
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
pub(crate) fn workspace_root() -> PathBuf {
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
        // Run from the WORKSPACE ROOT (not the crate dir): `--crate
        // rustty-vt-c` is resolved against the Cargo metadata of the working
        // directory, so the root is where it is reliably found. It also matches
        // the documented manual `cbindgen --config ... --crate rustty-vt-c
        // --output include/rustty.h` invocation, and the A5 arm (a3) that
        // re-runs cbindgen and diffs the result against the checked-in header —
        // both run at the root, so all three produce the SAME bytes.
        // (W12-02: this note used to say `parse_deps = true`; the config has set
        // `parse_deps = false` with the cross-crate C-ABI layouts hand-pinned in
        // `after_includes` since well before the note was written.)
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

/// What distinguishes one xcframework from another.
///
/// The pipeline (build a staticlib, fold in the vendored C archives cargo
/// declared, normalize with ranlib, libtool-combine, stage headers,
/// `xcodebuild -create-xcframework`) is IDENTICAL for both frameworks rustty
/// ships. Only these five things differ, so they are parameters rather than a
/// second copy of 180 lines.
struct XcframeworkSpec {
    /// Cargo package to build (`-p`).
    krate: &'static str,
    /// Combined-archive basename, also the libtool output stem.
    lib_stem: &'static str,
    /// Clang module name a Swift consumer imports.
    module: &'static str,
    /// Output bundle name under `dist/`.
    bundle: &'static str,
    /// How the public headers are staged for the clang module map.
    headers: HeaderLayout,
}

/// The two shapes rustty's public headers take.
enum HeaderLayout {
    /// A single umbrella header copied to `<headers>/<dst>`; the module's
    /// `umbrella header` is that file. This is the embedding API, whose
    /// `include/rustty-embed.h` already includes everything.
    SingleFile {
        src: &'static str,
        dst: &'static str,
    },
    /// A whole include subtree copied verbatim, with `umbrella` naming the
    /// entry point RELATIVE to the headers root. The VT library's public
    /// surface is `include/rustty/vt.h` plus `include/rustty/vt/*.h`, and C
    /// consumers spell it `#include <rustty/vt.h>` — so the tree has to keep
    /// its `rustty/` prefix rather than being flattened.
    Tree {
        src: &'static str,
        umbrella: &'static str,
    },
}

/// The embedding framework the macOS Swift app links (`import RusttyKit`).
const XCF_KIT: XcframeworkSpec = XcframeworkSpec {
    krate: "rustty-apprt-c",
    lib_stem: "libRusttyKit",
    module: "RusttyKit",
    bundle: "RusttyKit.xcframework",
    headers: HeaderLayout::SingleFile {
        src: "include/rustty-embed.h",
        dst: "rustty.h",
    },
};

/// PORTS: ghostty's `ghostty-vt.xcframework` (GhosttyXCFramework.zig's lib-vt
/// arm, shipped by release-tip.yml's `build-lib-vt-xcframework` job and
/// consumed by example/swift-vt-xcframework). rustty built the embedding
/// framework only, so neither the release artifact nor a Swift consumer of the
/// VT library had anything to point at.
const XCF_VT: XcframeworkSpec = XcframeworkSpec {
    krate: "rustty-vt-c",
    lib_stem: "librustty-vt",
    module: "RusttyVt",
    bundle: "rustty-vt.xcframework",
    headers: HeaderLayout::Tree {
        src: "include/rustty",
        umbrella: "rustty/vt.h",
    },
};

/// Copy a directory tree verbatim. `std::fs` has no recursive copy and the
/// header trees are small and shallow, so a plain recursion is enough.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
    for entry in std::fs::read_dir(src).map_err(|e| format!("read_dir {}: {e}", src.display()))? {
        let entry = entry.map_err(|e| format!("read_dir entry in {}: {e}", src.display()))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| format!("copy {}: {e}", from.display()))?;
        }
    }
    Ok(())
}

fn xcframework() -> Result<PathBuf, String> {
    build_xcframework(&XCF_KIT)
}

fn xcframework_vt() -> Result<PathBuf, String> {
    build_xcframework(&XCF_VT)
}

fn build_xcframework(spec: &XcframeworkSpec) -> Result<PathBuf, String> {
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
    let json = archives::cargo_build_json(
        &root,
        &[
            "-p",
            spec.krate,
            "--profile",
            "xcframework",
            // SIMD on for the shipped product library, matching upstream
            // Ghostty's default-on lib and the CMake wrapper's RUSTTY_VT_SIMD
            // default. (rustty's plain-`cargo build` default stays SIMD-off.)
            "--features",
            "simd",
            "--target",
            target,
        ],
        &[("MACOSX_DEPLOYMENT_TARGET", MACOS_DEPLOYMENT_TARGET)],
    )?;

    // 1b. Combine the Rust staticlib with its vendored C archives into ONE
    // self-contained lib (ghostty's CombineArchivesStep). A Rust `staticlib`
    // records its C link deps as metadata but does NOT bundle the -sys crates'
    // static archives (zlib/gettext/harfbuzz/freetype/oniguruma/…), so the final
    // xcodebuild link would report undefined `_deflate`/`_dgettext`/… . libtool
    // folds rustty's OWN vendored C archives (R3 — not Homebrew/system) into the
    // shipped lib so the Swift app links against a single self-contained archive.
    //
    // WS9: the set is DERIVED FROM CARGO (the `--message-format=json` stream
    // above), not from a filesystem glob — the port of GhosttyLib.zig's
    // enumerated `deps.add(lib)` list. A declared archive that cannot be located
    // is a hard error, so a nested OUT_DIR can no longer be silently missed and
    // stale hash-suffixed build dirs can no longer be folded in
    // (INTEGRATION_PARITY §2.4).
    let parsed = archives::parse_artifacts(&json)?;
    let rust_lib = match parsed.staticlibs.as_slice() {
        [one] => one.clone(),
        [] => {
            return Err(format!(
                "cargo reported no staticlib artifact for {}",
                spec.krate
            ))
        }
        many => {
            return Err(format!(
                "cargo reported {} staticlib artifacts; expected exactly one: {many:?}",
                many.len()
            ))
        }
    };
    let mut archives_list: Vec<PathBuf> = vec![rust_lib.clone()];
    // `resolve_under` (not `resolve`): an archive that resolves outside the
    // workspace's `target/` tree is a SYSTEM archive (Homebrew's libintl.a /
    // libz.a being the obvious ones) and folding it in would quietly undo the
    // self-containment this whole step exists to guarantee — while still
    // passing the no-Homebrew gate. Fail closed instead.
    let build_root = root.join("target");
    for req in &parsed.native {
        archives_list.push(req.resolve_under(&build_root)?);
    }
    eprintln!(
        "xcframework: folding {} archives ({} Rust staticlib + {} vendored C)",
        archives_list.len(),
        1,
        parsed.native.len()
    );

    // WS9: normalize EVERY source archive with ranlib before libtool sees it
    // (ghostty LibtoolStep.zig `normalizeArchive`) — newer Xcode libtool can
    // silently drop 64-bit members from cc/ar-produced archives
    // (INTEGRATION_PARITY §2.5).
    let norm_dir = root.join(format!("target/{target}/xcframework/normalized"));
    let _ = std::fs::remove_dir_all(&norm_dir);
    let mut normalized: Vec<PathBuf> = Vec::with_capacity(archives_list.len());
    for (i, a) in archives_list.iter().enumerate() {
        let dst = norm_dir.join(format!("{i}-{}.a", spec.lib_stem));
        archives::normalize_archive(a, &dst)?;
        normalized.push(dst);
    }

    let lib = root.join(format!("target/{target}/xcframework/{}.a", spec.lib_stem));
    let _ = std::fs::remove_file(&lib);
    let mut lt = Command::new("libtool");
    lt.arg("-static").arg("-o").arg(&lib);
    for a in &normalized {
        lt.arg(a);
    }
    let status = lt
        .current_dir(&root)
        .status()
        .map_err(|e| format!("spawn libtool: {e}"))?;
    if !status.success() {
        return Err(format!(
            "libtool combine exited with {status} ({} archives)",
            normalized.len()
        ));
    }
    if !lib.exists() {
        return Err(format!("combined lib not found at {}", lib.display()));
    }

    // 2. Stage the headers dir + the clang module map.
    let dist = root.join("dist");
    let headers = dist.join(format!("xcframework-headers-{}", spec.module));
    let _ = std::fs::remove_dir_all(&headers);
    std::fs::create_dir_all(&headers).map_err(|e| format!("mkdir headers: {e}"))?;
    let umbrella = match &spec.headers {
        HeaderLayout::SingleFile { src, dst } => {
            let src_h = root.join(src);
            if !src_h.exists() {
                return Err(format!("umbrella header missing: {}", src_h.display()));
            }
            std::fs::copy(&src_h, headers.join(dst)).map_err(|e| format!("copy {dst}: {e}"))?;
            (*dst).to_string()
        }
        HeaderLayout::Tree { src, umbrella } => {
            let src_dir = root.join(src);
            if !src_dir.is_dir() {
                return Err(format!("header tree missing: {}", src_dir.display()));
            }
            // Keep the `rustty/` prefix: consumers spell it
            // `#include <rustty/vt.h>`, so the tree is copied UNDER a directory
            // of the same name rather than flattened into the headers root.
            let leaf = src_dir
                .file_name()
                .ok_or_else(|| format!("header tree has no basename: {src}"))?;
            copy_dir_recursive(&src_dir, &headers.join(leaf))?;
            if !headers.join(umbrella).exists() {
                return Err(format!(
                    "umbrella header {umbrella} not found after staging {src}"
                ));
            }
            (*umbrella).to_string()
        }
    };
    // Umbrella module map — same shape as upstream's XCFrameworks.
    std::fs::write(
        headers.join("module.modulemap"),
        format!(
            "// {module} module for rustty (canonical rustty_* ABI).\n\
             module {module} {{\n    umbrella header \"{umbrella}\"\n    export *\n}}\n",
            module = spec.module,
        ),
    )
    .map_err(|e| format!("write modulemap: {e}"))?;

    // 3. xcodebuild -create-xcframework.
    let out = dist.join(spec.bundle);
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
/// every reference. As of WS1 every subtree carries REAL generated/vendored
/// content — no placeholder DEFERRALS notes are emitted anywhere (R5).
///
/// The generation is TARGET-AWARE like ghostty's `cfg.target`-driven install
/// (`--target-os`, defaulting to the host): FreeBSD selects the
/// `share/site-terminfo` layout, Windows emits only the terminfo source
/// (W2-03), and a `linux` target ALSO stages the desktop-application
/// resources — templates, actions, icons — exactly as `GhosttyResources.zig`
/// gates `addLinuxAppResources` on `os == .linux` (F-018). The macOS default
/// keeps the Xcode Copy-Resources tree byte-identical (no new dirs).
fn resources(args: &mut impl Iterator<Item = String>) -> Result<PathBuf, String> {
    // Defaults mirror ghostty's: target = host, prefix = the standard system
    // prefix packagers install under, release, no flatpak, no system-package.
    let mut target_os = std::env::consts::OS.to_string();
    let mut prefix = String::from("/usr");
    let mut flatpak = false;
    let mut debug = false;
    let mut system_package = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target-os" => {
                target_os = args.next().ok_or("--target-os requires a value")?;
            }
            "--prefix" => {
                prefix = args.next().ok_or("--prefix requires a value")?;
            }
            "--flatpak" => flatpak = true,
            "--debug" => debug = true,
            "--system-package" => system_package = true,
            other => return Err(format!("unknown `resources` flag `{other}`")),
        }
    }

    let root = workspace_root();
    let share = root.join("zig-out/share");
    std::fs::create_dir_all(&share).map_err(|e| format!("create zig-out/share/: {e}"))?;

    res_terminfo::gen_terminfo(&share, &target_os).map_err(|e| format!("gen_terminfo: {e}"))?;
    res_shell::gen_shell(&share).map_err(|e| format!("gen_shell: {e}"))?;
    res_editor::gen_editor(&share).map_err(|e| format!("gen_editor: {e}"))?;
    res_misc::gen_misc(&share).map_err(|e| format!("gen_misc: {e}"))?;
    res_themes::gen_themes(&share).map_err(|e| format!("gen_themes: {e}"))?;
    res_shell_integration::gen_shell_integration(&share)
        .map_err(|e| format!("gen_shell_integration: {e}"))?;

    // App (Linux) — GhosttyResources.zig gates addLinuxAppResources on
    // `cfg.target.result.os.tag == .linux` (F-018).
    if target_os == "linux" {
        res_linux::gen_linux_app_resources(
            &share,
            &res_linux::Options {
                exe_abs_path: format!("{prefix}/bin/{APP}"),
                flatpak,
                debug,
                system_package_mode: system_package,
            },
        )
        .map_err(|e| format!("gen_linux_app_resources: {e}"))?;
    }

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
