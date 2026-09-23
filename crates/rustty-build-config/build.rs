//! build.rs — capture the Rust toolchain version at compile time.
//!
//! Reference analog: Zig exposes `builtin.zig_version_string` to every module;
//! `cli/version.zig` prints it as the toolchain line. Rust has no builtin, so
//! this build script asks the ACTUAL compiler cargo is driving (`$RUSTC`,
//! falling back to `rustc` on PATH) and bakes the answer into the crate env.

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let out = std::process::Command::new(&rustc)
        .arg("--version")
        .output()
        .expect("failed to run `rustc --version`");
    let full = String::from_utf8(out.stdout).expect("rustc --version is UTF-8");
    // "rustc 1.93.1 (hash date)" -> "1.93.1 (hash date)" (a bare version
    // string, mirroring `builtin.zig_version_string`).
    let trimmed = full.trim();
    let version = trimmed.strip_prefix("rustc ").unwrap_or(trimmed);
    println!("cargo:rustc-env=RUSTTY_RUSTC_VERSION={version}");
    println!("cargo:rerun-if-env-changed=RUSTC");

    // `builtin.mode` (parity S42-F3). Zig has four optimize modes and
    // `build_config.mode_string` reports whichever one the build used. The
    // crate can see `debug_assertions` on its own, but NOT the optimization
    // level — that is only visible here, as cargo's `OPT_LEVEL`, so bake it in.
    // Together the two reconstruct all four modes; see `mode_string`.
    let opt_level = std::env::var("OPT_LEVEL").unwrap_or_else(|_| "0".to_string());
    println!("cargo:rustc-env=RUSTTY_OPT_LEVEL={opt_level}");
    println!("cargo:rerun-if-env-changed=OPT_LEVEL");

    gen_version();
}

#[path = "src/git_version.rs"]
mod git_version;

/// The git half of `Config.zig:276-339` (S63-F2 / S64-F1): decide which arm
/// of the version composition applies and export what it needs. The
/// composition itself is `version::compose`, in the library, because it
/// starts from `rustty_identity::VERSION` (see that module's docs).
///
/// Exports `RUSTTY_VERSION_KIND` (`explicit` | `dep` | `nogit` | `tag` |
/// `git`) and `RUSTTY_VERSION_{EXPLICIT,TAG,BRANCH,HASH}`.
fn gen_version() {
    let manifest_dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let emit = |kind: &str, explicit: &str, tag: &str, branch: &str, hash: &str| {
        println!("cargo:rustc-env=RUSTTY_VERSION_KIND={kind}");
        println!("cargo:rustc-env=RUSTTY_VERSION_EXPLICIT={explicit}");
        println!("cargo:rustc-env=RUSTTY_VERSION_TAG={tag}");
        println!("cargo:rustc-env=RUSTTY_VERSION_BRANCH={branch}");
        println!("cargo:rustc-env=RUSTTY_VERSION_HASH={hash}");
    };

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
    // workspace (`crates/rustty-build-config` under a root `Cargo.toml` that
    // lists it). A registry or vendored copy does not.
    let root = manifest_dir.join("../..");
    let in_workspace = std::fs::read_to_string(root.join("Cargo.toml"))
        .map(|s| s.contains("\"crates/rustty-build-config\""))
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

/// Re-run when HEAD moves, the branch ref moves, or a tag is added. Only
/// existing paths are registered: `rerun-if-changed` on a missing path makes
/// cargo rebuild every time. `--git-path` resolves correctly inside a git
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
