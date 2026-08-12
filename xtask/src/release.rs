//! xtask release-info — the release pipeline's metadata step.
//!
//! PORTS: the `setup` job of ghostty `.github/workflows/release-tag.yml` (the
//! "Validate Version Input" + "Extract the Version" + "Extract build info"
//! steps) and the identical "Extract build info" step of
//! `.github/workflows/release-tip.yml`. Upstream that logic is inline `bash`
//! inside the workflow YAML, where it can only ever be exercised by pushing a
//! tag; here it lives in xtask so the version grammar, the ref parsing and the
//! output rendering are unit-testable on any host and the workflow YAML stays
//! a thin driver (W2-05).
//!
//! Emitted keys are exactly the oracle `setup` job's four outputs:
//!
//! ```text
//! version=1.2.3        # the tag version with the leading `v` stripped
//! build=1234           # git rev-list --count HEAD  (CFBundleVersion)
//! commit=abc1234       # git rev-parse --short HEAD
//! commit_long=abc…     # git rev-parse HEAD
//! ```
//!
//! They are written to `$GITHUB_OUTPUT` when that env var is set (exactly as
//! the oracle's `echo "version=$VERSION" >> $GITHUB_OUTPUT` steps do) and are
//! always echoed to stdout (the oracle's trailing `cat $GITHUB_OUTPUT`).
//!
//! ONE deliberate addition over the oracle: a Cargo-specific consistency
//! check. Ghostty's release build takes the version from the tag
//! (`-Dversion-string=$GHOSTTY_VERSION`), so the tag is the single source of
//! truth. rustty's version lives in `Cargo.toml`'s `[workspace.package]` and
//! `xtask dist` names the tarball from THAT (`dist::workspace_version`), so a
//! tag that disagrees with the manifest would silently produce
//! `rustty-<manifest>.tar.gz` for a `v<tag>` release. `release-info` therefore
//! fails when the two disagree, before any artifact is built. Pass
//! `--no-workspace-check` to bypass it (e.g. a re-run against an older tag).

use std::path::Path;
use std::process::Command;

use crate::dist::workspace_version;
use crate::workspace_root;

pub const USAGE: &str = "\
xtask release-info — emit release metadata for the release workflows

USAGE:
    cargo run -p xtask -- release-info [OPTIONS]

OPTIONS:
    --version <vX.Y.Z>      Version from a workflow_dispatch input. The leading
                            `v` is REQUIRED (the oracle's
                            ^v[0-9]+\\.[0-9]+\\.[0-9]+$ validation) and is
                            stripped from the emitted `version`.
    --ref <refs/tags/vX.Y.Z>
                            Version from a pushed tag ref (GITHUB_REF).
    --no-workspace-check    Do not require the version to equal Cargo.toml's
                            [workspace.package] version.
    --help                  Show this help

With neither --version nor --ref the version IS the workspace version (the
tip pipeline, which has no tag). Output is `key=value` lines on stdout, also
appended to $GITHUB_OUTPUT when that variable is set.
";

/// `[0-9]+\.[0-9]+\.[0-9]+` — the oracle's version grammar, minus the `v`.
///
/// Deliberately as permissive as the upstream regex: leading zeros and
/// arbitrarily long components are accepted, pre-release/build suffixes are
/// not.
fn is_version_triple(s: &str) -> bool {
    let mut parts = s.split('.');
    let (Some(a), Some(b), Some(c)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    [a, b, c]
        .iter()
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Port of the workflow's "Validate Version Input" + "Extract the Version"
/// (`workflow_dispatch` arm): require `vX.Y.Z`, return `X.Y.Z`.
pub fn version_from_input(input: &str) -> Result<String, String> {
    let rest = input
        .strip_prefix('v')
        .filter(|r| is_version_triple(r))
        .ok_or_else(|| {
            format!("Version must follow the format vX.Y.Z (e.g., v1.0.0), got `{input}`")
        })?;
    Ok(rest.to_string())
}

/// Port of the workflow's "Extract the Version" (`push` arm):
/// `VERSION=${GITHUB_REF#refs/tags/v}`.
///
/// Unlike the oracle's bare prefix-strip this also validates the result, so a
/// ref that is not a release tag fails loudly instead of flowing a garbage
/// version into the artifact names.
pub fn version_from_ref(git_ref: &str) -> Result<String, String> {
    let rest = git_ref
        .strip_prefix("refs/tags/v")
        .filter(|r| is_version_triple(r))
        .ok_or_else(|| format!("ref `{git_ref}` is not a release tag (refs/tags/vX.Y.Z)"))?;
    Ok(rest.to_string())
}

/// The tag version must equal `Cargo.toml`'s `[workspace.package]` version —
/// see the module docs for why this has no oracle counterpart.
pub fn check_workspace_match(version: &str, workspace: &str) -> Result<(), String> {
    if version == workspace {
        return Ok(());
    }
    Err(format!(
        "release version `{version}` does not match Cargo.toml \
         [workspace.package] version `{workspace}` — bump the manifest (or pass \
         --no-workspace-check); `xtask dist` names its tarball from the manifest, \
         so releasing as-is would publish rustty-{workspace}.tar.gz for tag v{version}"
    ))
}

/// The oracle's `GHOSTTY_BUILD` / `GHOSTTY_COMMIT` / `GHOSTTY_COMMIT_LONG`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    /// `git rev-list --count HEAD` — the monotonic build number the macOS app
    /// uses as `CFBundleVersion` and Sparkle uses as `sparkle:version`.
    pub build: String,
    /// `git rev-parse --short HEAD`.
    pub commit: String,
    /// `git rev-parse HEAD`.
    pub commit_long: String,
}

fn git_capture(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| format!("spawn git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} exited with {}: {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return Err(format!("git {} produced no output", args.join(" ")));
    }
    Ok(s)
}

/// Port of the workflow's "Extract build info" step.
///
/// NOTE (oracle parity): the upstream job checks out with `fetch-depth: 0`
/// "so that build number generation works" — `git rev-list --count HEAD` on a
/// shallow clone counts only the fetched commits. The release workflows here
/// carry the same `fetch-depth: 0`.
pub fn build_info(root: &Path) -> Result<BuildInfo, String> {
    Ok(BuildInfo {
        build: git_capture(root, &["rev-list", "--count", "HEAD"])?,
        commit: git_capture(root, &["rev-parse", "--short", "HEAD"])?,
        commit_long: git_capture(root, &["rev-parse", "HEAD"])?,
    })
}

/// Render `key=value` lines in the order the oracle's `setup` job declares its
/// outputs.
pub fn render_outputs(version: &str, info: &BuildInfo) -> String {
    format!(
        "version={version}\nbuild={}\ncommit={}\ncommit_long={}\n",
        info.build, info.commit, info.commit_long
    )
}

/// `release-info` subcommand entry point.
pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut input_version: Option<String> = None;
    let mut git_ref: Option<String> = None;
    let mut workspace_check = true;
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => {
                input_version = Some(args.next().ok_or("--version requires a value")?);
            }
            "--ref" => {
                git_ref = Some(args.next().ok_or("--ref requires a value")?);
            }
            "--no-workspace-check" => workspace_check = false,
            "--help" | "-h" => {
                print!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown `release-info` flag `{other}`")),
        }
    }

    let root = workspace_root();
    let manifest_version = workspace_version(&root)?;
    let version = match (input_version.as_deref(), git_ref.as_deref()) {
        (Some(_), Some(_)) => return Err("pass at most one of --version / --ref".into()),
        (Some(v), None) => version_from_input(v)?,
        (None, Some(r)) => version_from_ref(r)?,
        // No tag: the tip pipeline. The manifest IS the version.
        (None, None) => manifest_version.clone(),
    };
    if workspace_check {
        check_workspace_match(&version, &manifest_version)?;
    }

    let info = build_info(&root)?;
    let rendered = render_outputs(&version, &info);
    if let Some(path) = std::env::var_os("GITHUB_OUTPUT") {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("open $GITHUB_OUTPUT ({}): {e}", Path::new(&path).display()))?;
        f.write_all(rendered.as_bytes())
            .map_err(|e| format!("write $GITHUB_OUTPUT: {e}"))?;
    }
    print!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_version_requires_leading_v_and_three_components() {
        assert_eq!(version_from_input("v1.0.0").unwrap(), "1.0.0");
        assert_eq!(version_from_input("v10.20.30").unwrap(), "10.20.30");
        // Leading zeros are accepted by the oracle's [0-9]+ regex.
        assert_eq!(version_from_input("v0.01.0").unwrap(), "0.01.0");

        for bad in [
            "1.0.0",       // no `v`
            "v1.0",        // two components
            "v1.0.0.1",    // four
            "v1.0.0-beta", // pre-release suffix
            "v1.0.x",
            "v",
            "",
            "v1.0.0 ",
            " v1.0.0",
            "V1.0.0",
        ] {
            assert!(
                version_from_input(bad).is_err(),
                "expected `{bad}` to be rejected"
            );
        }
    }

    #[test]
    fn tag_ref_extraction_matches_the_oracle_strip() {
        assert_eq!(version_from_ref("refs/tags/v1.2.3").unwrap(), "1.2.3");
        for bad in [
            "refs/tags/1.2.3",
            "refs/heads/main",
            "refs/tags/vtip",
            "refs/tags/tip",
            "refs/tags/v1.2",
            "",
        ] {
            assert!(
                version_from_ref(bad).is_err(),
                "expected `{bad}` to be rejected"
            );
        }
    }

    #[test]
    fn workspace_mismatch_is_an_error() {
        assert!(check_workspace_match("0.1.0", "0.1.0").is_ok());
        let err = check_workspace_match("1.0.0", "0.1.0").unwrap_err();
        assert!(err.contains("rustty-0.1.0.tar.gz"), "{err}");
        assert!(err.contains("--no-workspace-check"), "{err}");
    }

    #[test]
    fn outputs_render_as_github_output_lines() {
        let info = BuildInfo {
            build: "1234".into(),
            commit: "abc1234".into(),
            commit_long: "abc1234def5678901234567890123456789012ab".into(),
        };
        assert_eq!(
            render_outputs("1.2.3", &info),
            "version=1.2.3\nbuild=1234\ncommit=abc1234\n\
             commit_long=abc1234def5678901234567890123456789012ab\n"
        );
    }

    /// The build info really is derivable from this checkout (the step the
    /// oracle runs on every release). Shells to git, which the repo always has.
    #[test]
    fn build_info_reads_this_checkout() {
        let root = workspace_root();
        let info = build_info(&root).expect("build info");
        assert!(
            info.build.bytes().all(|b| b.is_ascii_digit()),
            "build number is not numeric: {}",
            info.build
        );
        assert_eq!(info.commit_long.len(), 40, "{}", info.commit_long);
        assert!(
            info.commit_long.starts_with(&info.commit),
            "short commit {} is not a prefix of {}",
            info.commit,
            info.commit_long
        );
    }
}
