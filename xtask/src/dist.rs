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

pub fn dist_scoped(scope: Scope) -> Result<PathBuf, String> {
    let root = workspace_root();
    let version = workspace_version(&root)?;
    let name = match scope {
        Scope::Workspace => format!("rustty-{version}"),
        Scope::LibVt => format!("librustty-vt-{version}"),
    };
    let prefix = format!("{name}/");

    let dist_dir = root.join("dist");
    std::fs::create_dir_all(&dist_dir).map_err(|e| format!("create dist/: {e}"))?;
    let out = dist_dir.join(format!("{name}.tar.gz"));

    // Staging dir (pid-keyed so concurrent runs don't collide).
    let stage = std::env::temp_dir().join(format!("rustty-dist-{version}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).map_err(|e| format!("create staging dir: {e}"))?;
    let result = dist_stage(&root, &stage, &name, &prefix, &version, &out, scope);
    let _ = std::fs::remove_dir_all(&stage);
    result?;
    Ok(out)
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
        .args([
            "archive",
            "--format=tar",
            &format!("--prefix={prefix}"),
            "-o",
            &src_tar.to_string_lossy(),
            "HEAD",
        ])
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
    std::fs::write(tree.join("VERSION"), format!("{version}\n"))
        .map_err(|e| format!("write VERSION: {e}"))?;

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
    std::fs::write(cargo_dir.join("config.toml"), &output.stdout)
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

/// Build the tarball, extract it, and build it in isolation to prove it is a
/// self-contained source tree. Returns the tarball path on success.
pub fn distcheck() -> Result<PathBuf, String> {
    let root = workspace_root();
    let version = workspace_version(&root)?;
    let name = format!("rustty-{version}");

    let tarball = dist()?;

    let checkdir = std::env::temp_dir().join(format!("rustty-distcheck-{version}"));
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
    let status = Command::new("cargo")
        .current_dir(&extracted)
        .env("CARGO_HOME", &cargo_home)
        .args(["test", "--workspace", "--offline", "--locked"])
        .status()
        .map_err(|e| format!("spawn cargo test: {e}"))?;
    if !status.success() {
        return Err(format!("distcheck test exited with {status}"));
    }

    let _ = std::fs::remove_dir_all(&checkdir);
    Ok(tarball)
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
                // `kind` null = normal, "build" = build-dep. Dev-deps are
                // skipped: they pull in test-only crates a source tarball for
                // C consumers does not need.
                if d["path"].is_null() {
                    continue;
                }
                match d["kind"].as_str() {
                    None | Some("build") => {}
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
        let root = workspace_root();
        let version = workspace_version(&root).expect("workspace version");
        let name = format!("rustty-{version}");

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
        let _ = std::fs::remove_dir_all(&extract);
    }
}
