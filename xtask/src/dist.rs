//! xtask dist — source tarball.
//!
//! PORTS: ghostty `src/build/GhosttyDist.zig` — the `dist` + `distcheck` build
//! steps. Uses `git archive` so the tarball honors `.gitignore` and the
//! `export-ignore` gitattribute (dropping dev-only paths), then injects the
//! dist-only inputs the release tarball promises on top of the git tree
//! (GhosttyDist.zig's `extra_files` pattern of injecting generated inputs the
//! tarball must carry): a `VERSION` file, plus — W2-01 — the `vendor/`
//! dependency tree and the `.cargo/config.toml` source replacement that
//! PACKAGING.md's "Source Tarballs" contract documents (`cargo vendor
//! --locked`), so the shipped tarball really is self-contained and builds
//! `--offline`. The archive is prefixed `rustty-<version>/` (packagers like
//! debhelper/rpmbuild expect a `project-version/` root). `distcheck` extracts
//! the tarball, ASSERTS the promised dist-only inputs are present (the
//! `addCheckFile` assertions of GhosttyDist.zig), and builds it `--offline
//! --locked` under a FRESH `CARGO_HOME` — so a warm cache or the network can
//! never mask a missing vendor tree — to prove it is self-contained.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::workspace_root;

/// Parse the workspace version from the root Cargo.toml `[workspace.package]`
/// `version = "…"` line.
///
/// Shared with `release::run`, which refuses to cut a release whose tag
/// disagrees with this value — the tarball this module names is derived from
/// it (W2-05).
pub(crate) fn workspace_version(root: &Path) -> Result<String, String> {
    let cargo = root.join("Cargo.toml");
    let text =
        std::fs::read_to_string(&cargo).map_err(|e| format!("read {}: {e}", cargo.display()))?;
    let mut in_pkg = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_pkg = t == "[workspace.package]";
            continue;
        }
        if in_pkg {
            if let Some(rest) = t.strip_prefix("version") {
                // `version = "0.1.0"` (allow surrounding whitespace/quotes)
                if let Some(eq) = rest.find('=') {
                    let v = rest[eq + 1..].trim().trim_matches('"').to_string();
                    if !v.is_empty() {
                        return Ok(v);
                    }
                }
            }
        }
    }
    Err("could not find [workspace.package] version in Cargo.toml".into())
}

/// Build the source tarball at `dist/rustty-<version>.tar.gz`. Returns its path.
///
/// The dist-only inputs (`VERSION`, `vendor/`, `.cargo/config.toml`) cannot be
/// injected with `git archive --add-file` (the vendor tree is thousands of
/// files), so the git tree is extracted to a staging dir, the inputs are
/// materialized inside it, and the final tarball is re-packed from the stage —
/// same net result as GhosttyDist.zig's WriteFiles-then-tar pipeline.
/// Which tree the tarball carries.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The whole workspace — the product source release.
    Workspace,
    /// Only what `rustty-vt-c` needs: the headless C library.
    ///
    /// PORTS: ghostty's `zig build dist -Demit-lib-vt=true`, the artifact
    /// release-tip.yml's `source-tarball-lib-vt` job publishes as
    /// `libghostty-vt-source.tar.gz`. The reference's build system can scope a
    /// dist by artifact; a Cargo workspace cannot, so this SLICES the staged
    /// tree — see `prune_to_lib_vt`.
    LibVt,
}

pub fn dist() -> Result<PathBuf, String> {
    dist_scoped(Scope::Workspace)
}

pub fn dist_lib_vt() -> Result<PathBuf, String> {
    dist_scoped(Scope::LibVt)
}

/// The version a tarball is named after and carries in `VERSION`: the full
/// `cfg.version` (`GhosttyDist.zig:52-107` formats `{f}` of it into the
/// prefix, the file name and `VERSION`), i.e. the composed app version —
/// `X.Y.Z` on a `vX.Y.Z` tag, `X.Y.Z-<branch>+<short hash>` otherwise,
/// `RUSTTY_VERSION_STRING` when set (`-Dversion-string`). The lib-vt tarball
/// uses the same app version (`GhosttyDist.zig:22,56`), not the lib version.
/// N424; before it this was the bare `[workspace.package]` version.
pub(crate) const DIST_VERSION: &str = rustty_version::VERSION_STRING;

/// `{name}-{version}`: the archive prefix and file stem (`GhosttyDist.zig:22`:
/// `libghostty-vt` for lib-vt, else `ghostty`; R2 names).
pub(crate) fn dist_name(scope: Scope, version: &str) -> String {
    match scope {
        Scope::Workspace => format!("rustty-{version}"),
        Scope::LibVt => format!("librustty-vt-{version}"),
    }
}

pub fn dist_scoped(scope: Scope) -> Result<PathBuf, String> {
    let root = workspace_root();
    let version = DIST_VERSION;
    let name = dist_name(scope, version);
    let prefix = format!("{name}/");

    let dist_dir = root.join("dist");
    std::fs::create_dir_all(&dist_dir).map_err(|e| format!("create dist/: {e}"))?;
    let out = dist_dir.join(format!("{name}.tar.gz"));

    // Staging dir (pid-keyed so concurrent runs don't collide).
    let stage = std::env::temp_dir().join(format!("rustty-dist-{version}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).map_err(|e| format!("create staging dir: {e}"))?;
    let result = dist_stage(&root, &stage, &name, &prefix, version, &out, scope);
    let _ = std::fs::remove_dir_all(&stage);
    result?;
    Ok(out)
}

/// Paths to exclude from the dist archive when building for lib-vt only.
/// These are large files and directories that are not needed to build or
/// test librustty-vt, specified as git pathspec exclude patterns.
///
/// PORTS `GhosttyDist.zig:184-209` `lib_vt_excludes`, mapped onto the port's
/// tree. Of the reference's 17 entries, 11 map one-to-one below. The other 6
/// are not listed, each for a checked reason:
/// - `src/font/res`: the port's counterpart `crates/rustty-font/res` is
///   `include_bytes!`'d by `rustty-font`, which is a normal dependency of
///   `rustty-vt` (D1), so excluding it breaks the lib-vt build.
/// - `src/crash/testdata`, `pkg/breakpad/vendor`: their crates
///   (`rustty-crash`, `breakpad-sys`) are outside `rustty-vt-c`'s closure, so
///   `prune_to_lib_vt` already removes them.
/// - `pkg/wuffs/src/too_big.{jpg,png}`: no such files in the port.
/// - `vendor`: the port has no tracked top-level `vendor/`; the one in the
///   tarball is the `cargo vendor` output written after the archive.
const LIB_VT_EXCLUDES: &[&str] = &[
    // App and platform resources
    "images",
    "macos",
    "dist/doxygen",
    "dist/linux",
    "dist/macos",
    "dist/windows",
    "flatpak",
    "snap",
    "po",
    "example",
    // Test corpus (lib-vt tests use embedded testdata within the crates)
    "test",
];

/// The `git archive` argv for step 1 of [`dist_stage`]
/// (`GhosttyDist.zig:78-102`).
fn git_archive_args(prefix: &str, out: &Path, scope: Scope) -> Vec<String> {
    let mut args = vec![
        "archive".to_string(),
        "--format=tar".to_string(),
        format!("--prefix={prefix}"),
        "-o".to_string(),
        out.to_string_lossy().into_owned(),
        "HEAD".to_string(),
    ];

    // When building for lib-vt only, exclude large directories that
    // are not needed to build librustty-vt. This significantly reduces
    // the size of the resulting archive.
    if scope == Scope::LibVt {
        for exclude in LIB_VT_EXCLUDES {
            args.push(format!(":(exclude){exclude}"));
        }
    }
    args
}

/// The staged portion of [`dist`], split out so the staging dir is cleaned up
/// on every exit path.
#[allow(clippy::too_many_arguments)]
fn dist_stage(
    root: &Path,
    stage: &Path,
    name: &str,
    prefix: &str,
    version: &str,
    out: &Path,
    scope: Scope,
) -> Result<(), String> {
    // 1. git archive HEAD (honors .gitignore + `export-ignore`) extracted into
    //    the stage: `<stage>/<name>/…`.
    let src_tar = stage.join("src.tar");
    let status = Command::new("git")
        .current_dir(root)
        .args(git_archive_args(prefix, &src_tar, scope))
        .status()
        .map_err(|e| format!("spawn git archive: {e}"))?;
    if !status.success() {
        return Err(format!("git archive exited with {status}"));
    }
    let status = Command::new("tar")
        .current_dir(stage)
        .args(["xf", &src_tar.to_string_lossy()])
        .status()
        .map_err(|e| format!("spawn tar extract: {e}"))?;
    if !status.success() {
        return Err(format!("tar extract (staging) exited with {status}"));
    }
    let _ = std::fs::remove_file(&src_tar);
    let tree = stage.join(name);
    if !tree.join("Cargo.toml").is_file() {
        return Err(format!(
            "staged tree missing Cargo.toml at {}",
            tree.display()
        ));
    }

    // 2. Inject the VERSION file at the archive root (not tracked in git).
    // `b.fmt("{f}", .{cfg.version})` (`GhosttyDist.zig:52`): the version with no
    // trailing newline (N424).
    std::fs::write(tree.join("VERSION"), version).map_err(|e| format!("write VERSION: {e}"))?;

    // 2b. For a lib-vt tarball, slice the staged tree down to rustty-vt-c's
    //     workspace closure before vendoring, so `cargo vendor` resolves the
    //     PRUNED manifest and the tarball ships only the crates it needs.
    if scope == Scope::LibVt {
        prune_to_lib_vt(root, &tree)?;
    }

    // 3. Vendor the dependency tree INTO the staged source (W2-01 — the
    //    PACKAGING.md "Source Tarballs" contract: tarballs ship `vendor/` +
    //    a `.cargo/config.toml` redirecting crates.io to it, so the build
    //    runs fully offline). `cargo vendor` prints exactly that config
    //    snippet on stdout; run from the tree root so the emitted `directory =
    //    "vendor"` path is relative and survives extraction anywhere.
    // `--locked` only for the full workspace. A lib-vt tarball has REWRITTEN
    // `members`, so the staged Cargo.lock genuinely no longer matches and
    // --locked refuses to proceed:
    //   cannot update the lock file ... because --locked was passed
    // Re-resolving is correct there (the member set really did change) and it
    // is a strict subset, so nothing new enters the graph.
    let mut vendor_args: Vec<&str> = vec!["vendor"];
    if scope == Scope::Workspace {
        vendor_args.push("--locked");
    }
    vendor_args.push("vendor");
    let output = Command::new("cargo")
        .current_dir(&tree)
        .args(&vendor_args)
        .output()
        .map_err(|e| format!("spawn cargo vendor: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo vendor exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if output.stdout.is_empty() {
        return Err("cargo vendor printed no source-replacement config".into());
    }
    let cargo_dir = tree.join(".cargo");
    std::fs::create_dir_all(&cargo_dir).map_err(|e| format!("create .cargo/: {e}"))?;
    // N422: the archive already carries the TRACKED `.cargo/config.toml` (the
    // wasm `simd128` default), so the source replacement is appended to it,
    // never written over it — a tarball build must keep the checkout's
    // defaults.
    let config_path = cargo_dir.join("config.toml");
    let tracked = match std::fs::read(&config_path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("read .cargo/config.toml: {e}")),
    };
    std::fs::write(
        &config_path,
        dist_cargo_config(tracked.as_deref(), &output.stdout),
    )
    .map_err(|e| format!("write .cargo/config.toml: {e}"))?;

    // 4. Re-pack the stage into the final tarball.
    let status = Command::new("tar")
        .args([
            "czf",
            &out.to_string_lossy(),
            "-C",
            &stage.to_string_lossy(),
            name,
        ])
        .status()
        .map_err(|e| format!("spawn tar create: {e}"))?;
    if !status.success() {
        return Err(format!("tar create exited with {status}"));
    }
    Ok(())
}

/// The tarball's `.cargo/config.toml`: the tracked checkout config (if the
/// archive carried one) followed by `cargo vendor`'s source replacement.
fn dist_cargo_config(tracked: Option<&[u8]>, vendor: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(t) = tracked {
        out.extend_from_slice(t);
        if !t.ends_with(b"\n") {
            out.push(b'\n');
        }
        out.push(b'\n');
    }
    out.extend_from_slice(vendor);
    out
}

/// Build the tarball, extract it, and build it in isolation to prove it is a
/// self-contained source tree. Returns the tarball path on success.
pub fn distcheck() -> Result<PathBuf, String> {
    distcheck_scoped(Scope::Workspace)
}

/// The `-Demit-lib-vt=true` arm of the reference's check step.
///
/// PORTS: `GhosttyDist.zig:126-147, 156-171` (parity S63-F7). The reference's
/// check step is scoped by artifact: for `emit_lib_vt` it runs
/// `zig build test-lib-vt -Demit-lib-vt=true` inside the extracted tree — not
/// the full suite — and then ALSO `cmake -B cmake-build` + `cmake --build`
/// from that tree, both with `expectExitCode(0)`. `distcheck()` only ever
/// exercised the Workspace tarball, so the lib-vt tarball — the one published
/// as `librustty-vt-source.tar.gz` — shipped with neither guarantee.
pub fn distcheck_lib_vt() -> Result<PathBuf, String> {
    distcheck_scoped(Scope::LibVt)
}

fn distcheck_scoped(scope: Scope) -> Result<PathBuf, String> {
    let root = workspace_root();
    let name = dist_name(scope, DIST_VERSION);

    let tarball = dist_scoped(scope)?;

    let checkdir = std::env::temp_dir().join(format!("rustty-distcheck-{name}"));
    let _ = std::fs::remove_dir_all(&checkdir);
    std::fs::create_dir_all(&checkdir).map_err(|e| format!("create checkdir: {e}"))?;

    // Extract.
    let status = Command::new("tar")
        .current_dir(&checkdir)
        .args(["xzf", &tarball.to_string_lossy()])
        .status()
        .map_err(|e| format!("spawn tar: {e}"))?;
    if !status.success() {
        return Err(format!("tar extract exited with {status}"));
    }

    let extracted = checkdir.join(&name);
    if !extracted.join("Cargo.toml").is_file() {
        return Err(format!(
            "extracted tree missing Cargo.toml at {}",
            extracted.display()
        ));
    }
    // The promised dist-only inputs must be PRESENT in the extracted archive —
    // the `addCheckFile` assertions of GhosttyDist.zig (inject AND assert;
    // W2-01).
    if !extracted.join("VERSION").is_file() {
        return Err("extracted tree missing injected VERSION file".into());
    }
    if !extracted.join("vendor").is_dir() {
        return Err("extracted tree missing vendored dependency tree (vendor/)".into());
    }
    if !extracted.join(".cargo/config.toml").is_file() {
        return Err("extracted tree missing .cargo/config.toml source replacement".into());
    }
    // N422: the checkout's own config (the wasm `simd128` default) must have
    // survived the source replacement.
    if let Ok(tracked) = std::fs::read(root.join(".cargo/config.toml")) {
        let shipped = std::fs::read(extracted.join(".cargo/config.toml"))
            .map_err(|e| format!("read extracted .cargo/config.toml: {e}"))?;
        if !shipped.starts_with(&tracked) {
            return Err("extracted .cargo/config.toml lost the tracked checkout config".into());
        }
    }
    // The lib-vt tarball's whole reason to exist is the C-consumer story, so
    // the CMake trio must be there before the CMake build below can mean
    // anything (the reference's `addCheckFile` over `resources`).
    if scope == Scope::LibVt {
        for p in ["CMakeLists.txt", "include", "dist/cmake"] {
            if !extracted.join(p).exists() {
                return Err(format!("extracted lib-vt tree missing {p}"));
            }
        }
    }

    // Run the TEST SUITE from the extracted tree — not just a build (parity
    // S63-F8). The reference's check step is
    // `step = b.addSystemCommand(&.{"zig", "build", "test"}); step.setCwd(
    // extract_dir); step.expectExitCode(0);` (GhosttyDist.zig:128-131), i.e.
    // the guarantee the tarball ships with is "its tests pass where it was
    // unpacked", not merely "it compiles there". A source tarball that builds
    // and then fails its own tests is exactly what this step exists to catch,
    // and `cargo build` could not see it.
    //
    // `--offline --locked` with a FRESH `CARGO_HOME` stays, so neither the
    // network nor the developer's warm registry cache can stand in for a
    // broken vendor tree (W2-01 — the old plain `cargo build` here was green
    // even though no vendor tree existed at all). Note this also raises the
    // bar on the vendor tree itself: it must now carry the DEV-dependencies
    // too, which a build-only check never exercised.
    //
    // Like the reference, this is not fully hermetic (it shares the host
    // toolchain and target dir policy) and is slow; the reference documents the
    // same trade-off in the comment above its own check step.
    let cargo_home = checkdir.join("cargo-home");
    std::fs::create_dir_all(&cargo_home).map_err(|e| format!("create fresh CARGO_HOME: {e}"))?;
    // BUILD FIRST, then TEST. Not belt-and-braces: `cargo test` does not
    // produce the `cdylib` artifacts, and `rustty-apprt-c`'s C-ABI smoke test
    // links against `target/<profile>/librustty_apprt_c.{dylib,so}`. In a
    // developer's tree that file is already there from an earlier
    // `cargo build`, so the test passes; in a freshly extracted tarball it is
    // not, and the test fails on its absence. Zig's single `zig build test`
    // builds what its tests need, so this pair is what reproduces it.
    //
    // SCOPE: the reference runs `zig build test` for the workspace tarball and
    // `zig build test-lib-vt -Demit-lib-vt=true` for the lib-vt one. The
    // lib-vt tree has been SLICED to `rustty-vt-c`'s closure, so
    // `--workspace` there is already that closure — but naming the two crates
    // explicitly says which guarantee is being made and fails loudly if the
    // slice ever stops containing them.
    let steps: &[(&str, &[&str])] = match scope {
        Scope::Workspace => &[
            ("build", &["build", "--workspace", "--offline", "--locked"]),
            ("test", &["test", "--workspace", "--offline", "--locked"]),
        ],
        Scope::LibVt => &[
            (
                "build",
                &[
                    "build",
                    "-p",
                    "rustty-vt",
                    "-p",
                    "rustty-vt-c",
                    "--offline",
                    "--locked",
                ],
            ),
            (
                "test",
                &[
                    "test",
                    "-p",
                    "rustty-vt",
                    "-p",
                    "rustty-vt-c",
                    "--offline",
                    "--locked",
                ],
            ),
        ],
    };
    for (step, args) in steps {
        let status = distcheck_cargo(&extracted, &cargo_home, args)
            .status()
            .map_err(|e| format!("spawn cargo {step}: {e}"))?;
        if !status.success() {
            return Err(format!("distcheck {step} exited with {status}"));
        }
    }

    // For lib-vt, also verify the CMake build works from the tarball:
    //
    //   cmake_configure = b.addSystemCommand(&.{ "cmake", "-B" }); ... setCwd(extract_dir)
    //   cmake_build     = b.addSystemCommand(&.{ "cmake", "--build" });
    //
    // both `expectExitCode(0)` (GhosttyDist.zig:156-171). This is what proves
    // the SHIPPED `CMakeLists.txt` configures and builds from a clean extract,
    // which nothing in the port checked: `check_dist.sh`'s Debian arm runs a
    // bare `cargo build` and `grep -n cmake` over it returns nothing.
    if scope == Scope::LibVt {
        let build_dir = extracted.join("cmake-build");
        for (step, args) in [
            (
                "configure",
                vec!["-B".to_string(), "cmake-build".to_string()],
            ),
            (
                "build",
                vec![
                    "--build".to_string(),
                    build_dir.to_string_lossy().into_owned(),
                ],
            ),
        ] {
            let status = Command::new("cmake")
                .current_dir(&extracted)
                .args(&args)
                .status()
                .map_err(|e| format!("spawn cmake {step}: {e}"))?;
            if !status.success() {
                return Err(format!("distcheck cmake {step} exited with {status}"));
            }
        }
    }

    let _ = std::fs::remove_dir_all(&checkdir);
    Ok(tarball)
}

/// One `cargo` step of [`distcheck_scoped`], run in the extracted tree with a
/// fresh `CARGO_HOME`.
fn distcheck_cargo(extracted: &Path, cargo_home: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(extracted)
        .env("CARGO_HOME", cargo_home)
        // N197: the tarball ships no `.port/`, so the fuzz-corpus replay
        // tests cannot find their corpus here. `ci.sh` exports
        // RUSTTY_REQUIRE_FUZZ_CORPUS=1, which would turn that expected
        // absence into a distcheck failure.
        .env_remove("RUSTTY_REQUIRE_FUZZ_CORPUS")
        .args(args);
    cmd
}

/// Slice a staged workspace down to `rustty-vt-c`'s closure.
///
/// A Cargo workspace cannot be partially built: every path in `members` must
/// exist. So rather than deleting crates and leaving dangling members, this
/// computes the closure from `cargo metadata`, REWRITES the `members` list to
/// it, and removes the directories that fall out. The `dist/cmake` +
/// `CMakeLists` + `include` trio stays because the C-consumer story is what
/// this tarball is for.
fn prune_to_lib_vt(root: &Path, tree: &Path) -> Result<(), String> {
    // The closure is computed from `cargo metadata`, NOT from `cargo tree`.
    //
    // `cargo tree` reports the FEATURE-RESOLVED graph, and a workspace needs
    // the manifest of every path dependency a member DECLARES — including
    // optional ones no feature turns on. rustty-vt's optional `dcimgui-sys`
    // (its `imgui` feature) is exactly that: absent from every `cargo tree`
    // spelling tried here, yet cargo refuses to load the workspace without it:
    //
    //   failed to load manifest for dependency `dcimgui-sys`
    //
    // `cargo tree` also names crates.io packages that merely END in `-sys`
    // (clang-sys), which a name-shape filter would wrongly treat as workspace
    // members. metadata gives the member set and the declared deps exactly.
    let output = Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|e| format!("spawn cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("parse cargo metadata: {e}"))?;
    let packages = meta["packages"]
        .as_array()
        .ok_or("cargo metadata: `packages` is not an array")?;

    // name -> the path deps it declares (workspace members only).
    let mut member_names: Vec<String> = Vec::new();
    let mut deps_of: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for pkg in packages {
        let Some(name) = pkg["name"].as_str() else {
            continue;
        };
        member_names.push(name.to_string());
        let mut deps = Vec::new();
        if let Some(list) = pkg["dependencies"].as_array() {
            for d in list {
                // A `path` field is what makes it an in-workspace dependency;
                // `kind` null = normal, "build" = build-dep, "dev" = dev-dep.
                // ALL THREE are closure edges.
                //
                // CORRECTION (S63-F7 / N37): this used to skip dev-deps, with
                // the comment "they pull in test-only crates a source tarball
                // for C consumers does not need". That premise is wrong twice.
                // (1) Cargo cannot even LOAD a workspace whose member declares
                // a path dev-dependency that is not on disk — it fails with
                // `failed to load manifest for dependency`, which is how this
                // was found: `xtask dist-lib-vt` has been BROKEN since
                // rustty-vt gained its `rustty-synthetic` dev-dependency, and
                // `xtask dist-lib-vt` is what G-DIST runs at its DEFAULT depth.
                // (2) The reference's check step for this very tarball is
                // `zig build test-lib-vt` — it RUNS THE TESTS in the extracted
                // tree, so the test-only crates are exactly what it needs.
                if d["path"].is_null() {
                    continue;
                }
                match d["kind"].as_str() {
                    None | Some("build") | Some("dev") => {}
                    _ => continue,
                }
                if let Some(dn) = d["name"].as_str() {
                    deps.push(dn.to_string());
                }
            }
        }
        deps_of.insert(name.to_string(), deps);
    }

    // Transitive closure from rustty-vt-c.
    let mut keep: Vec<String> = Vec::new();
    let mut queue = vec!["rustty-vt-c".to_string()];
    while let Some(n) = queue.pop() {
        if keep.contains(&n) {
            continue;
        }
        if !member_names.contains(&n) {
            continue; // a crates.io dep; `cargo vendor` handles it
        }
        keep.push(n.clone());
        if let Some(ds) = deps_of.get(&n) {
            for d in ds {
                queue.push(d.clone());
            }
        }
    }
    // Rewrite [workspace] members.
    let manifest_path = tree.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read staged Cargo.toml: {e}"))?;
    let start = manifest
        .find("members = [")
        .ok_or("staged Cargo.toml has no `members = [`")?;
    // Find the CLOSING bracket by scanning lines, not by `find(']')`: the
    // members list is full of trailing `# 11  <- crown jewel`-style comments,
    // and one of them contains a `]`. Taking the first bracket sliced the
    // manifest mid-comment and produced a TOML parse error out of cargo
    // vendor, three steps downstream.
    let end = {
        let mut idx = start;
        let mut found = None;
        for line in manifest[start..].split_inclusive('\n') {
            let trimmed = line.trim_start();
            if idx != start && (trimmed.starts_with(']') || trimmed.starts_with("] ")) {
                found = Some(idx + line.find(']').unwrap() + 1);
                break;
            }
            idx += line.len();
        }
        found.ok_or("staged Cargo.toml `members` list is unterminated")?
    };
    let mut members = String::from("members = [\n");
    for k in &keep {
        let dir = member_dir(root, k)?;
        members.push_str(&format!("    \"{dir}\",\n"));
    }
    members.push(']');
    let manifest = format!("{}{}{}", &manifest[..start], members, &manifest[end..]);
    std::fs::write(&manifest_path, manifest)
        .map_err(|e| format!("write staged Cargo.toml: {e}"))?;

    // Remove the crate directories that are no longer members.
    for (subdir, prefix) in [("crates", "rustty-"), ("crates-sys", "")] {
        let dir = tree.join(subdir);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !entry.path().is_dir() {
                continue;
            }
            let _ = prefix;
            if !keep.contains(&name) {
                std::fs::remove_dir_all(entry.path())
                    .map_err(|e| format!("prune {subdir}/{name}: {e}"))?;
            }
        }
    }
    Ok(())
}

/// Workspace-relative directory of a member, derived from its manifest path
/// rather than guessed from its name.
fn member_dir(root: &Path, name: &str) -> Result<String, String> {
    for sub in ["crates", "crates-sys", "."] {
        let candidate = root.join(sub).join(name);
        if candidate.join("Cargo.toml").is_file() {
            return Ok(if sub == "." {
                name.to_string()
            } else {
                format!("{sub}/{name}")
            });
        }
    }
    Err(format!("cannot locate workspace member `{name}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// List the paths `git archive` would put in a tarball of `scope`, using
    /// the real argv from [`git_archive_args`] but writing to stdout.
    fn archive_listing(scope: Scope) -> Vec<String> {
        let tag = match scope {
            Scope::Workspace => "ws",
            Scope::LibVt => "libvt",
        };
        let tarball = std::env::temp_dir().join(format!(
            "xtask-archive-test-{tag}-{}.tar",
            std::process::id()
        ));
        let status = Command::new("git")
            .current_dir(workspace_root())
            .args(git_archive_args("p/", &tarball, scope))
            .status()
            .expect("spawn git archive");
        assert!(status.success());
        let out = Command::new("tar")
            .arg("tf")
            .arg(&tarball)
            .output()
            .expect("spawn tar tf");
        let _ = std::fs::remove_file(&tarball);
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim_start_matches("p/").to_string())
            .collect()
    }

    /// N197: the extracted tarball carries no `.port/`, so the fuzz-corpus
    /// replay tests can never find their corpus there. `ci.sh` exports
    /// `RUSTTY_REQUIRE_FUZZ_CORPUS=1`, which turns that absence into a
    /// failure, so the distcheck cargo steps must not inherit it.
    #[test]
    fn distcheck_cargo_does_not_inherit_the_fuzz_corpus_requirement() {
        let cmd = distcheck_cargo(Path::new("x"), Path::new("h"), &["test"]);
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(
            envs.contains(&(std::ffi::OsStr::new("RUSTTY_REQUIRE_FUZZ_CORPUS"), None)),
            "{envs:?}"
        );
        // The step still runs with its fresh CARGO_HOME.
        assert!(
            envs.contains(&(
                std::ffi::OsStr::new("CARGO_HOME"),
                Some(std::ffi::OsStr::new("h"))
            )),
            "{envs:?}"
        );
    }

    /// `GhosttyDist.zig:95-102, 184-209`: a lib-vt archive passes one
    /// `:(exclude)<path>` pathspec per `lib_vt_excludes` entry (S63-F10). The
    /// workspace archive still carries those trees, and the lib-vt archive
    /// still carries what the library build needs, including the font
    /// resources `rustty-font` embeds (D1 puts that crate below `rustty-vt`).
    #[test]
    fn lib_vt_archive_applies_the_reference_excludes() {
        // The list itself: the reference's 17 (`GhosttyDist.zig:184-209`, in
        // order) minus exactly the six omissions `LIB_VT_EXCLUDES` documents.
        // Asserted as a whole, because the archive checks below iterate the
        // port's own list and so cannot notice an entry dropped from it.
        const REFERENCE: [&str; 17] = [
            "images",
            "macos",
            "dist/doxygen",
            "dist/linux",
            "dist/macos",
            "dist/windows",
            "flatpak",
            "snap",
            "po",
            "example",
            "test",
            "src/font/res",
            "src/crash/testdata",
            "pkg/wuffs/src/too_big.jpg",
            "pkg/wuffs/src/too_big.png",
            "pkg/breakpad/vendor",
            "vendor",
        ];
        const OMITTED: [&str; 6] = [
            "src/font/res",
            "src/crash/testdata",
            "pkg/wuffs/src/too_big.jpg",
            "pkg/wuffs/src/too_big.png",
            "pkg/breakpad/vendor",
            "vendor",
        ];
        let expected: Vec<&str> = REFERENCE
            .into_iter()
            .filter(|e| !OMITTED.contains(e))
            .collect();
        assert_eq!(LIB_VT_EXCLUDES, expected.as_slice());
        let args = git_archive_args("p/", Path::new("o.tar"), Scope::LibVt);
        for e in &expected {
            assert!(args.contains(&format!(":(exclude){e}")), "{args:?}");
        }

        let lib_vt = archive_listing(Scope::LibVt);
        let full = archive_listing(Scope::Workspace);
        for dir in LIB_VT_EXCLUDES {
            let under = |l: &String| l.starts_with(&format!("{dir}/"));
            assert!(full.iter().any(under), "workspace archive lacks {dir}/");
            assert!(!lib_vt.iter().any(under), "lib-vt archive ships {dir}/");
        }
        for kept in [
            "Cargo.toml",
            "CMakeLists.txt",
            "crates/rustty-vt-c/Cargo.toml",
            "dist/cmake/",
            "crates/rustty-font/res/JetBrainsMono-Regular.ttf",
        ] {
            assert!(
                lib_vt.iter().any(|l| l == kept),
                "lib-vt archive lacks {kept}"
            );
        }
    }

    /// W2-01 proving test: the tarball `dist` builds carries the promised
    /// offline inputs — `VERSION`, the `vendor/` dependency tree, and the
    /// `.cargo/config.toml` source replacement (PACKAGING.md "Source
    /// Tarballs"). Before the fix the archive was a bare `git archive` and
    /// this failed on all but `VERSION`.
    ///
    /// Ignored by default: it shells to git/tar and runs `cargo vendor` over
    /// the full workspace (minutes + network on a cold registry cache). Run
    /// explicitly:
    ///
    /// ```sh
    /// cargo test -p xtask dist_tarball_ships_vendor_inputs -- --ignored
    /// ```
    ///
    /// (`distcheck` in the packaging lane additionally proves the offline
    /// `--locked` build under a fresh CARGO_HOME.)
    #[test]
    #[ignore = "shells to git/cargo: cargo vendor over the full workspace"]
    fn dist_tarball_ships_vendor_inputs() {
        let version = DIST_VERSION;
        let name = dist_name(Scope::Workspace, version);

        let tarball = dist().expect("dist");

        let output = Command::new("tar")
            .arg("tzf")
            .arg(&tarball)
            .output()
            .expect("spawn tar tzf");
        assert!(output.status.success(), "tar tzf failed");
        let listing = String::from_utf8_lossy(&output.stdout);

        for required in [
            format!("{name}/VERSION"),
            format!("{name}/.cargo/config.toml"),
        ] {
            assert!(
                listing.lines().any(|l| l == required),
                "tarball missing promised dist input {required}"
            );
        }
        assert!(
            listing
                .lines()
                .any(|l| l.starts_with(&format!("{name}/vendor/"))),
            "tarball missing promised vendor/ dependency tree"
        );
        // The source replacement must actually point crates.io at vendor/.
        let extract =
            std::env::temp_dir().join(format!("rustty-dist-test-{version}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&extract);
        std::fs::create_dir_all(&extract).unwrap();
        let status = Command::new("tar")
            .current_dir(&extract)
            .args([
                "xzf",
                &tarball.to_string_lossy(),
                &format!("{name}/.cargo/config.toml"),
            ])
            .status()
            .expect("spawn tar xzf");
        assert!(status.success());
        let config =
            std::fs::read_to_string(extract.join(&name).join(".cargo/config.toml")).unwrap();
        assert!(
            config.contains("[source.crates-io]") && config.contains("vendor"),
            "config.toml is not a vendor source replacement:\n{config}"
        );
        // N422: and it still carries the checkout's wasm simd128 default.
        assert!(
            config.contains("target-feature=+simd128"),
            "config.toml lost the tracked wasm simd128 default:\n{config}"
        );
        let _ = std::fs::remove_dir_all(&extract);
    }

    /// N424, `GhosttyDist.zig:22,52-107`: both tarballs are named — file,
    /// archive prefix and `VERSION` — after the full app version the build
    /// reports (`rustty_version::VERSION_STRING`, the `cfg.version` port), not
    /// the bare manifest version; lib-vt uses the same app version.
    #[test]
    fn tarballs_are_named_after_the_full_app_version() {
        assert_eq!(DIST_VERSION, rustty_version::VERSION_STRING);
        assert_eq!(
            dist_name(Scope::Workspace, "1.2.3-main+abc1234"),
            "rustty-1.2.3-main+abc1234"
        );
        assert_eq!(
            dist_name(Scope::LibVt, "1.2.3-main+abc1234"),
            "librustty-vt-1.2.3-main+abc1234"
        );
        // The composed version starts with the manifest's X.Y.Z (every arm but
        // an explicit RUSTTY_VERSION_STRING, which replaces it wholesale).
        if option_env!("RUSTTY_VERSION_STRING").is_none_or(str::is_empty) {
            let manifest = workspace_version(&workspace_root()).unwrap();
            assert!(
                DIST_VERSION.starts_with(&manifest),
                "{DIST_VERSION} vs {manifest}"
            );
        }
    }

    /// N422: the tarball config is the tracked config, then the vendor
    /// replacement; a tracked config is never overwritten.
    #[test]
    fn dist_config_appends_the_vendor_replacement_to_the_tracked_config() {
        let vendor = b"[source.crates-io]\nreplace-with = \"vendored-sources\"\n";
        assert_eq!(dist_cargo_config(None, vendor), vendor.to_vec());
        let merged = dist_cargo_config(Some(b"[a]\nx = 1"), vendor);
        assert_eq!(
            merged,
            [&b"[a]\nx = 1\n\n"[..], &vendor[..]].concat(),
            "{}",
            String::from_utf8_lossy(&merged)
        );
        let tracked = std::fs::read(workspace_root().join(".cargo/config.toml"))
            .expect("the checkout tracks .cargo/config.toml (N422)");
        assert!(dist_cargo_config(Some(&tracked), vendor).starts_with(&tracked));
    }

    /// N422, `Config.zig:113-130`: every wasm target defaults to the `simd128`
    /// CPU feature. The tracked config must apply it to exactly the wasm32
    /// targets, as target rustflags (so a RUSTFLAGS of the caller's, the
    /// `-Dcpu` analog, replaces it).
    #[test]
    fn checkout_config_defaults_wasm_to_simd128() {
        let config = std::fs::read_to_string(workspace_root().join(".cargo/config.toml"))
            .expect("read .cargo/config.toml");
        let lines: Vec<&str> = config
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        let table = lines
            .iter()
            .position(|l| *l == "[target.'cfg(target_arch = \"wasm32\")']")
            .expect("a wasm32 target table");
        assert_eq!(
            lines.get(table + 1),
            Some(&"rustflags = [\"-C\", \"target-feature=+simd128\"]"),
            "{lines:?}"
        );
        // Nothing else sets rustflags (no non-wasm target is touched).
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("rustflags")).count(),
            1,
            "{lines:?}"
        );
    }
}
