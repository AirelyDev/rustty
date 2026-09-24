//! PORTS: `src/build/GitVersion.zig` (S64-F1).
//!
//! Compiled twice: into `build.rs` through `#[path]`, which is where it runs,
//! and into the library's tests, which is where it is tested. It uses only
//! `std`, so both builds see the same code.

use std::path::Path;
use std::process::{Command, Stdio};

/// `GitVersion.zig`'s version record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The short hash (7 characters) of the latest commit.
    pub short_hash: String,

    /// True if there was a diff at build time.
    pub changes: bool,

    /// The tag -- if any -- that this commit is a part of.
    pub tag: Option<String>,

    /// The branch that was checked out at the time of the build.
    pub branch: String,
}

/// `detect`'s error set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// `error.FileNotFound` from spawning `git`.
    GitNotFound,
    /// `rev-parse` exited non-zero.
    GitNotRepository,
    /// Any other failure, which the reference propagates (and so fails the
    /// build).
    Other(String),
}

/// The outcome of one `b.runAllowFail`: stdout on exit 0, else the error the
/// reference switches on.
enum Run {
    Ok(String),
    ExitCodeFailure,
}

fn run_allow_fail(root: &Path, args: &[&str]) -> Result<Run, Error> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        // `.ignore`: stderr is discarded.
        .stderr(Stdio::null())
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::GitNotFound,
            _ => Error::Other(format!("spawn git: {e}")),
        })?;
    if !out.status.success() {
        return Ok(Run::ExitCodeFailure);
    }
    String::from_utf8(out.stdout)
        .map(Run::Ok)
        .map_err(|e| Error::Other(format!("git output is not UTF-8: {e}")))
}

fn trim_end(s: &str) -> &str {
    s.trim_end_matches(['\r', '\n', ' '])
}

/// Replace characters that are not valid in semantic version pre-release
/// identifiers (which only allow [0-9A-Za-z-]). Slashes would also mess up
/// dist tarball paths.
pub fn sanitize_branch(branch: &str) -> String {
    branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// The branch name from `git rev-parse --abbrev-ref HEAD`'s raw stdout:
/// trimmed of `"\r\n "`, then sanitized.
//
// PORT NOTE: reference defect (R13), UD150. `GitVersion.zig:22-41` sanitizes
// the RAW output first, so git's trailing newline becomes `-`, and the
// `trimEnd(u8, branch, "\r\n ")` at `:84` then has nothing left to trim: every
// dev version reads `X.Y.Z-<branch>-+<hash>` (measured on the reference's own
// cached build options: `1.3.2-HEAD-+4540d499a`). The trim can only be meant
// to strip that newline, and before upstream `ad9225a4c` (which replaced the
// `/`-only rewrite with the full sanitizing loop) versions came out
// `X.Y.Z-main+hash`. Reachable on every untagged build from a git checkout.
// The port trims first and then sanitizes, which is the reference's own two
// steps in the order that makes the trim effective: `main\n` -> `main`.
pub fn branch_from_rev_parse(raw: &str) -> String {
    sanitize_branch(trim_end(raw))
}

/// Initialize the version and detect it from the Git environment rooted at
/// `root` (the reference's `b.build_root`).
pub fn detect(root: &Path) -> Result<Version, Error> {
    // Execute a bunch of git commands to determine the automatic version.
    let branch = match run_allow_fail(root, &["rev-parse", "--abbrev-ref", "HEAD"])? {
        Run::Ok(s) => branch_from_rev_parse(&s),
        Run::ExitCodeFailure => return Err(Error::GitNotRepository),
    };

    let short_hash = match run_allow_fail(
        root,
        &[
            "-c",
            "log.showSignature=false",
            "log",
            "--pretty=format:%h",
            "-n",
            "1",
        ],
    )? {
        Run::Ok(s) => trim_end(&s).to_string(),
        // `else => return err`: the reference fails the build here.
        Run::ExitCodeFailure => return Err(Error::Other("git log exited non-zero".into())),
    };

    let tag = match run_allow_fail(root, &["describe", "--exact-match", "--tags"])? {
        Run::Ok(s) => s,
        Run::ExitCodeFailure => String::new(), // expected
    };

    let changes = match run_allow_fail(root, &["diff", "--quiet", "--exit-code"])? {
        Run::Ok(_) => false,
        Run::ExitCodeFailure => true, // expected
    };

    Ok(Version {
        short_hash,
        changes,
        tag: if tag.is_empty() {
            None
        } else {
            Some(trim_end(&tag).to_string())
        },
        // The reference's `std.mem.trimEnd(u8, branch, "\r\n ")` sits here;
        // `branch_from_rev_parse` has already trimmed (UD150), so it is moved,
        // not dropped.
        branch,
    })
}
