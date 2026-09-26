//! xtask patchelf — PORTS `Config.zig`'s `patch-interp` / `patch-rpath`
//! options (`:371-406`) and `addPatchElf` (`:642-656`), S63-F4.
//!
//! On native Linux inside a Nix shell the reference patches the built binary
//! with `patchelf --set-interpreter <ld.so> --set-rpath <LD_LIBRARY_PATH>`, so
//! a binary built in the dev shell also launches outside it. `zig build`
//! runs that as part of installing the exe; cargo has no post-link hook, so
//! the port is this subcommand, run on the binary after `cargo build`:
//!
//! ```sh
//! cargo run -p xtask -- patchelf target/debug/rustty
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

/// `config.patchelf`: what to inject, if anything.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PatchElf {
    pub interp: Option<String>,
    pub rpath: Option<String>,
}

/// The environment the defaults are derived from.
pub struct Env<'a> {
    /// The target is Linux and the build is native (`isNativeCpu()`).
    pub native_linux: bool,
    /// `IN_NIX_SHELL` is set.
    pub in_nix_shell: bool,
    /// `realpath` of `ld.so` found on `PATH`, if any.
    pub ld_so: Option<&'a str>,
    /// `LD_LIBRARY_PATH`, if set.
    pub ld_library_path: Option<&'a str>,
}

/// The reference's option resolution: an explicit `--patch-interp` /
/// `--patch-rpath` always wins; otherwise, on native Linux in a Nix shell,
/// the interpreter defaults to the `ld.so` on `PATH` and the rpath to a
/// non-empty `LD_LIBRARY_PATH`.
pub fn resolve(patch_interp: Option<&str>, patch_rpath: Option<&str>, env: &Env) -> PatchElf {
    let nix = env.native_linux && env.in_nix_shell;
    let interp = match patch_interp {
        Some(i) => Some(i.to_string()),
        None if nix => env.ld_so.map(str::to_string),
        None => None,
    };
    let rpath = match patch_rpath {
        Some(r) => Some(r.to_string()),
        None if nix => env
            .ld_library_path
            .filter(|p| !p.is_empty())
            .map(str::to_string),
        None => None,
    };
    PatchElf { interp, rpath }
}

/// `addPatchElf`'s command: `patchelf [--set-interpreter <i>] [--set-rpath
/// <r>] <artifact>`, or `None` when there is nothing to patch.
pub fn command(p: &PatchElf, artifact: &Path) -> Option<Command> {
    if p.interp.is_none() && p.rpath.is_none() {
        return None;
    }
    let mut cmd = Command::new("patchelf");
    if let Some(i) = &p.interp {
        cmd.arg("--set-interpreter").arg(i);
    }
    if let Some(r) = &p.rpath {
        cmd.arg("--set-rpath").arg(r);
    }
    cmd.arg(artifact);
    Some(cmd)
}

/// `xtask patchelf [--patch-interp <path>] [--patch-rpath <paths>] <binary>`.
pub fn run(args: &mut impl Iterator<Item = String>) -> Result<Option<PathBuf>, String> {
    let mut patch_interp = None;
    let mut patch_rpath = None;
    let mut artifact = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--patch-interp" => {
                patch_interp = Some(args.next().ok_or("--patch-interp requires a value")?)
            }
            "--patch-rpath" => {
                patch_rpath = Some(args.next().ok_or("--patch-rpath requires a value")?)
            }
            _ if artifact.is_none() => artifact = Some(PathBuf::from(a)),
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    let artifact = artifact
        .ok_or("usage: patchelf [--patch-interp <path>] [--patch-rpath <paths>] <binary>")?;

    let ld_so = which("ld.so").and_then(|p| std::fs::canonicalize(p).ok());
    let ld_so = ld_so.as_ref().map(|p| p.to_string_lossy().into_owned());
    let ld_library_path = std::env::var("LD_LIBRARY_PATH").ok();
    let env = Env {
        // xtask runs on the host and patches a host build.
        native_linux: cfg!(target_os = "linux"),
        in_nix_shell: std::env::var_os("IN_NIX_SHELL").is_some(),
        ld_so: ld_so.as_deref(),
        ld_library_path: ld_library_path.as_deref(),
    };
    let p = resolve(patch_interp.as_deref(), patch_rpath.as_deref(), &env);
    let Some(mut cmd) = command(&p, &artifact) else {
        return Ok(None);
    };
    let status = cmd.status().map_err(|e| format!("spawn patchelf: {e}"))?;
    if !status.success() {
        return Err(format!("patchelf exited with {status}"));
    }
    Ok(Some(artifact))
}

/// `b.findProgram(&.{name}, &.{})`: the first `PATH` entry holding `name`.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(native_linux: bool, in_nix_shell: bool) -> Env<'a> {
        Env {
            native_linux,
            in_nix_shell,
            ld_so: Some("/nix/store/x-glibc/lib/ld-linux-x86-64.so.2"),
            ld_library_path: Some("/nix/store/a/lib:/nix/store/b/lib"),
        }
    }

    /// `Config.zig:371-406`: the Nix-shell defaults, and their guards.
    #[test]
    fn defaults_apply_only_on_native_linux_in_a_nix_shell() {
        let p = resolve(None, None, &env(true, true));
        assert_eq!(
            p.interp.as_deref(),
            Some("/nix/store/x-glibc/lib/ld-linux-x86-64.so.2")
        );
        assert_eq!(
            p.rpath.as_deref(),
            Some("/nix/store/a/lib:/nix/store/b/lib")
        );
        assert_eq!(resolve(None, None, &env(true, false)), PatchElf::default());
        assert_eq!(resolve(None, None, &env(false, true)), PatchElf::default());
        // An empty LD_LIBRARY_PATH is not an rpath.
        let e = Env {
            ld_library_path: Some(""),
            ..env(true, true)
        };
        assert_eq!(resolve(None, None, &e).rpath, None);
    }

    /// Explicit options win everywhere.
    #[test]
    fn explicit_options_win() {
        let p = resolve(Some("/i"), Some("/r"), &env(false, false));
        assert_eq!(p.interp.as_deref(), Some("/i"));
        assert_eq!(p.rpath.as_deref(), Some("/r"));
    }

    /// `Config.zig:642-656` `addPatchElf`.
    #[test]
    fn command_matches_the_reference_argv() {
        let p = PatchElf {
            interp: Some("/i".into()),
            rpath: Some("/r".into()),
        };
        let cmd = command(&p, Path::new("rustty")).unwrap();
        assert_eq!(cmd.get_program(), "patchelf");
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            args,
            ["--set-interpreter", "/i", "--set-rpath", "/r", "rustty"]
        );
        assert!(command(&PatchElf::default(), Path::new("rustty")).is_none());
    }
}
