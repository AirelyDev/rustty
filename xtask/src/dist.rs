//! xtask dist — source tarball.
//!
//! PORTS: ghostty `src/build/GhosttyDist.zig` — the `dist` + `distcheck` build
//! steps. Uses `git archive` so the tarball honors `.gitignore` and the
//! `export-ignore` gitattribute (dropping dev-only paths), with `--add-file`
//! injecting a `VERSION` file that isn't tracked in git. The archive is
//! prefixed `rustty-<version>/` (packagers like debhelper/rpmbuild expect a
//! `project-version/` root). `distcheck` extracts the tarball and builds it to
//! prove it is self-contained.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::workspace_root;

/// Parse the workspace version from the root Cargo.toml `[workspace.package]`
/// `version = "…"` line.
fn workspace_version(root: &Path) -> Result<String, String> {
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
pub fn dist() -> Result<PathBuf, String> {
    let root = workspace_root();
    let version = workspace_version(&root)?;
    let name = format!("rustty-{version}");
    let prefix = format!("{name}/");

    let dist_dir = root.join("dist");
    std::fs::create_dir_all(&dist_dir).map_err(|e| format!("create dist/: {e}"))?;
    let out = dist_dir.join(format!("{name}.tar.gz"));

    // Stage a VERSION file to inject at the archive root (not tracked in git).
    let stage = std::env::temp_dir().join(format!("rustty-dist-{version}"));
    std::fs::create_dir_all(&stage).map_err(|e| format!("create staging dir: {e}"))?;
    let version_file = stage.join("VERSION");
    std::fs::write(&version_file, format!("{version}\n"))
        .map_err(|e| format!("write VERSION: {e}"))?;

    // git archive honors .gitignore + the `export-ignore` attribute; --add-file
    // injects the staged VERSION at `<prefix>VERSION`.
    let status = Command::new("git")
        .current_dir(&root)
        .args([
            "archive",
            "--format=tar.gz",
            &format!("--prefix={prefix}"),
            &format!("--add-file={}", version_file.display()),
            "-o",
            &out.to_string_lossy(),
            "HEAD",
        ])
        .status()
        .map_err(|e| format!("spawn git archive: {e}"))?;
    let _ = std::fs::remove_dir_all(&stage);
    if !status.success() {
        return Err(format!("git archive exited with {status}"));
    }
    Ok(out)
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
    if !extracted.join("VERSION").is_file() {
        return Err("extracted tree missing injected VERSION file".into());
    }

    // Build the workspace from the extracted tree to prove it is self-contained.
    let status = Command::new("cargo")
        .current_dir(&extracted)
        .args(["build", "--workspace"])
        .status()
        .map_err(|e| format!("spawn cargo build: {e}"))?;
    if !status.success() {
        return Err(format!("distcheck build exited with {status}"));
    }

    let _ = std::fs::remove_dir_all(&checkdir);
    Ok(tarball)
}
