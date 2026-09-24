//! build.rs — the git half of `src/build/Config.zig:276-339` (S63-F2 / S64-F1).
//!
//! Decides which arm of the version composition applies and exports what it
//! needs. The composition itself is `version::compose`, in the library, because
//! it starts from `rustty_identity::VERSION`, which a build script cannot see
//! (see `src/version.rs`).
//!
//! Exports `RUSTTY_VERSION_KIND` (`explicit` | `dep` | `nogit` | `tag` | `git`)
//! and `RUSTTY_VERSION_{EXPLICIT,TAG,BRANCH,HASH}`.

#[path = "src/git_version.rs"]
mod git_version;

fn main() {
    let manifest_dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let emit = |kind: &str, explicit: &str, tag: &str, branch: &str, hash: &str| {
        println!("cargo:rustc-env=RUSTTY_VERSION_KIND={kind}");
        println!("cargo:rustc-env=RUSTTY_VERSION_EXPLICIT={explicit}");
        println!("cargo:rustc-env=RUSTTY_VERSION_TAG={tag}");
        println!("cargo:rustc-env=RUSTTY_VERSION_BRANCH={branch}");
        println!("cargo:rustc-env=RUSTTY_VERSION_HASH={hash}");
    };
    // Without this, cargo re-runs the script on any change in the package; with
    // it, only on the watched git paths and variables.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/git_version.rs");

    // `-Dversion-string`: "A specific version string to use for the build.
    // If not specified, git will be used. This must be a semantic version."
    println!("cargo:rerun-if-env-changed=RUSTTY_VERSION_STRING");
    if let Ok(v) = std::env::var("RUSTTY_VERSION_STRING") {
        if !v.is_empty() {
            // If an explicit version is given, we always use it.
            return emit("explicit", &v, "", "", "");
        }
    }

    // Is rustty a dependency? If so, skip git detection. The reference asks
    // the build graph (`b.dep_prefix.len > 0`); cargo gives a build script no
    // such fact, so the port's test is whether this crate sits inside its own
    // workspace (`crates/rustty-version` under a root `Cargo.toml` that lists
    // it). A registry or vendored copy does not.
    let root = manifest_dir.join("../..");
    let in_workspace = std::fs::read_to_string(root.join("Cargo.toml"))
        .map(|s| s.contains("\"crates/rustty-version\""))
        .unwrap_or(false);
    if !in_workspace {
        return emit("dep", "", "", "", "");
    }

    // If no explicit version is given, we try to detect it from git.
    let vsn = match git_version::detect(&root) {
        Ok(v) => v,
        // If Git isn't available we just make an unknown dev version.
        Err(git_version::Error::GitNotFound) | Err(git_version::Error::GitNotRepository) => {
            return emit("nogit", "", "", "", "");
        }
        Err(git_version::Error::Other(e)) => panic!("git version detection failed: {e}"),
    };
    watch_git(&root);
    // `changes` is detected (`GitVersion.zig:66-78`) but `Config.zig` never
    // reads it; neither does the port.
    let _ = vsn.changes;
    match &vsn.tag {
        Some(tag) => emit("tag", "", tag, &vsn.branch, &vsn.short_hash),
        None => emit("git", "", "", &vsn.branch, &vsn.short_hash),
    }
}

/// Re-run when HEAD moves, the branch ref moves, or a tag is added.
///
/// `logs/HEAD` (the HEAD reflog) is appended to on every commit, checkout,
/// reset and rebase step, so it catches a new commit even when the branch's
/// loose ref did not exist at the last run: after `git pack-refs` / `git gc`
/// the branch lives only in `packed-refs`, the next commit re-creates the loose
/// ref at a path that was never registered, and neither `HEAD` (still
/// `ref: refs/heads/<b>`) nor `packed-refs` changes. (Not the `refs/heads`
/// directory: it is shared by every worktree, so another worktree's commit would
/// dirty this one. A worktree's `logs/HEAD` is its own.)
///
/// Only existing paths are registered: `rerun-if-changed` on a missing path
/// makes cargo rebuild every time. `--git-path` resolves correctly inside a git
/// worktree, where `.git` is a file.
fn watch_git(root: &std::path::Path) {
    let git = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let mut watch = vec![
        git(&["rev-parse", "--git-path", "HEAD"]),
        git(&["rev-parse", "--git-path", "logs/HEAD"]),
        git(&["rev-parse", "--git-path", "packed-refs"]),
        git(&["rev-parse", "--git-path", "refs/tags"]),
    ];
    if let Some(r) = git(&["rev-parse", "--symbolic-full-name", "HEAD"]) {
        watch.push(git(&["rev-parse", "--git-path", &r]));
    }
    for p in watch.into_iter().flatten() {
        let p = if std::path::Path::new(&p).is_absolute() {
            std::path::PathBuf::from(p)
        } else {
            root.join(p)
        };
        if p.exists() {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
}
