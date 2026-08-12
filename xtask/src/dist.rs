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
pub fn dist() -> Result<PathBuf, String> {
    let root = workspace_root();
    let version = workspace_version(&root)?;
    let name = format!("rustty-{version}");
    let prefix = format!("{name}/");

    let dist_dir = root.join("dist");
    std::fs::create_dir_all(&dist_dir).map_err(|e| format!("create dist/: {e}"))?;
    let out = dist_dir.join(format!("{name}.tar.gz"));

    // Staging dir (pid-keyed so concurrent runs don't collide).
    let stage = std::env::temp_dir().join(format!("rustty-dist-{version}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).map_err(|e| format!("create staging dir: {e}"))?;
    let result = dist_stage(&root, &stage, &name, &prefix, &version, &out);
    let _ = std::fs::remove_dir_all(&stage);
    result?;
    Ok(out)
}

/// The staged portion of [`dist`], split out so the staging dir is cleaned up
/// on every exit path.
fn dist_stage(
    root: &Path,
    stage: &Path,
    name: &str,
    prefix: &str,
    version: &str,
    out: &Path,
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

    // 3. Vendor the dependency tree INTO the staged source (W2-01 — the
    //    PACKAGING.md "Source Tarballs" contract: tarballs ship `vendor/` +
    //    a `.cargo/config.toml` redirecting crates.io to it, so the build
    //    runs fully offline). `cargo vendor` prints exactly that config
    //    snippet on stdout; run from the tree root so the emitted `directory =
    //    "vendor"` path is relative and survives extraction anywhere.
    let output = Command::new("cargo")
        .current_dir(&tree)
        .args(["vendor", "--locked", "vendor"])
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

    // Build the workspace from the extracted tree to prove it is
    // self-contained: `--offline --locked` with a FRESH `CARGO_HOME`, so
    // neither the network nor the developer's warm registry cache can stand in
    // for a broken vendor tree (W2-01 — the old plain `cargo build` here was
    // green even though no vendor tree existed at all).
    let cargo_home = checkdir.join("cargo-home");
    std::fs::create_dir_all(&cargo_home).map_err(|e| format!("create fresh CARGO_HOME: {e}"))?;
    let status = Command::new("cargo")
        .current_dir(&extracted)
        .env("CARGO_HOME", &cargo_home)
        .args(["build", "--workspace", "--offline", "--locked"])
        .status()
        .map_err(|e| format!("spawn cargo build: {e}"))?;
    if !status.success() {
        return Err(format!("distcheck build exited with {status}"));
    }

    let _ = std::fs::remove_dir_all(&checkdir);
    Ok(tarball)
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
