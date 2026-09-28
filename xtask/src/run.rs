//! xtask run — PORTS the `run` step of ghostty's `build.zig` (`:282-326`), N423.
//!
//! `zig build run` has two arms, picked by the app runtime:
//!
//! * a real app runtime (`config.app_runtime != .none`: GTK, the Linux and
//!   FreeBSD default) runs the built executable with the build's arguments and
//!   "the proper resources dir so things like shell integration work
//!   correctly. If we're running `zig build run` in Ghostty, this also ensures
//!   it overwrites the release one with our debug build":
//!   `run_cmd.setEnvironmentVariable("GHOSTTY_RESOURCES_DIR",
//!   b.getInstallPath(.prefix, "share/ghostty"))`. Here that is `cargo build -p
//!   rustty`, then `target/<profile>/rustty <args>` with
//!   `RUSTTY_RESOURCES_DIR=<prefix>/share/rustty` (R2), the prefix being
//!   `zig-out/` in the checkout, where `xtask resources` installs — like
//!   `zig build`'s, the run does not install the resources itself;
//! * `.none` on macOS (the default there) builds a NATIVE-only xcframework and
//!   the macOS app and opens it (`GhosttyXcodebuild.zig:126-163`). That arm is
//!   already ported as `macos/run-rustty.sh` (S64-F7); `run` executes it with
//!   `XCFRAMEWORK_TARGET=native` ("For "run" we always force a native-only
//!   build so that we can run as quickly as possible").
//!
//! On any other target (`.none`, not Darwin, e.g. Windows) the reference's run
//! step has no dependencies and does nothing; so does this.
//!
//! Before N423, `cargo run -p rustty` was the only way to run the app, and it
//! pointed at no resources directory.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// What `run` does on a target.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Build `cargo build -p rustty <cargo_args>`, then run `exe` with `args`
    /// and the resources-dir variable set.
    Exe {
        cargo_args: Vec<String>,
        exe: PathBuf,
        env: (String, PathBuf),
        args: Vec<String>,
    },
    /// Run `script` (the macOS app arm) with `env` and `args`.
    MacApp {
        script: PathBuf,
        env: Vec<(String, String)>,
        args: Vec<String>,
    },
    /// The reference's run step does nothing on this target.
    Nothing,
}

/// The parsed `run` flags.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Opts {
    /// `--profile <p>` / `--release` (the `-Doptimize` of the build).
    pub profile: Option<String>,
    /// `--prefix <dir>` (`zig build --prefix`); default `<root>/zig-out`.
    pub prefix: Option<PathBuf>,
    /// Everything after `--` (`b.args`).
    pub args: Vec<String>,
}

pub(crate) fn parse(args: &mut impl Iterator<Item = String>) -> Result<Opts, String> {
    let mut o = Opts::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--release" => o.profile = Some("release".into()),
            "--profile" => o.profile = Some(args.next().ok_or("--profile requires a value")?),
            "--prefix" => o.prefix = Some(args.next().ok_or("--prefix requires a value")?.into()),
            "--" => {
                o.args = args.collect();
                break;
            }
            other => {
                return Err(format!(
                    "unknown `run` flag `{other}` (app arguments go after `--`)"
                ))
            }
        }
    }
    Ok(o)
}

/// `run`'s plan for `target_os` (`std::env::consts::OS` spelling). `app` is
/// the product name (`crate::APP`), `root` the workspace root.
pub(crate) fn plan(target_os: &str, app: &str, root: &Path, opts: Opts) -> Plan {
    match target_os {
        // `apprt.Runtime.default`: `.linux, .freebsd => .gtk`.
        "linux" | "freebsd" => {
            let (cargo_args, dir) = match opts.profile.as_deref() {
                None | Some("dev") => (vec![], "debug".to_string()),
                Some("release") => (vec!["--release".to_string()], "release".to_string()),
                Some(p) => (vec!["--profile".to_string(), p.to_string()], p.to_string()),
            };
            let prefix = opts.prefix.unwrap_or_else(|| root.join("zig-out"));
            Plan::Exe {
                cargo_args,
                exe: root.join("target").join(dir).join(app),
                env: (
                    format!("{}_RESOURCES_DIR", app.to_uppercase()),
                    prefix.join("share").join(app),
                ),
                args: opts.args,
            }
        }
        // `.none` + Darwin + (xcframework or macOS app): the app arm.
        "macos" => Plan::MacApp {
            script: root.join("macos").join("run-rustty.sh"),
            env: vec![("XCFRAMEWORK_TARGET".into(), "native".into())],
            args: opts.args,
        },
        _ => Plan::Nothing,
    }
}

/// Run `exe` with `args` and `env`, returning its exit status as ours.
pub(crate) fn exec(
    exe: &Path,
    args: &[String],
    env: &[(String, String)],
) -> Result<ExitCode, String> {
    let status = Command::new(exe)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .status()
        .map_err(|e| format!("spawn {}: {e}", exe.display()))?;
    Ok(match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(c) => ExitCode::from(u8::try_from(c).unwrap_or(1)),
        None => ExitCode::FAILURE,
    })
}

pub fn run(args: &mut impl Iterator<Item = String>) -> Result<ExitCode, String> {
    let opts = parse(args)?;
    let root = crate::workspace_root();
    match plan(std::env::consts::OS, crate::APP, &root, opts) {
        Plan::Exe {
            cargo_args,
            exe,
            env,
            args,
        } => {
            let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
                .current_dir(&root)
                .args(["build", "-p", "rustty"])
                .args(&cargo_args)
                .status()
                .map_err(|e| format!("spawn cargo build: {e}"))?;
            if !status.success() {
                return Err(format!("cargo build -p rustty exited with {status}"));
            }
            exec(
                &exe,
                &args,
                &[(env.0, env.1.to_string_lossy().into_owned())],
            )
        }
        Plan::MacApp { script, env, args } => exec(&script, &args, &env),
        Plan::Nothing => Ok(ExitCode::SUCCESS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(v: &[&str]) -> Opts {
        parse(&mut v.iter().map(|s| s.to_string())).unwrap()
    }

    /// `build.zig:289-292`: the GTK arm points the app at `<prefix>/share/<app>`
    /// (`zig-out` by default) and passes the build's arguments through.
    #[test]
    fn gtk_arm_runs_the_exe_with_the_resources_dir() {
        let root = Path::new("/w");
        for os in ["linux", "freebsd"] {
            assert_eq!(
                plan(os, "rustty", root, opts(&["--", "+version", "-e"])),
                Plan::Exe {
                    cargo_args: vec![],
                    exe: root.join("target/debug/rustty"),
                    env: (
                        "RUSTTY_RESOURCES_DIR".into(),
                        root.join("zig-out/share/rustty")
                    ),
                    args: vec!["+version".into(), "-e".into()],
                },
                "{os}"
            );
        }
        // `--prefix` and the profile.
        assert_eq!(
            plan(
                "linux",
                "rustty",
                root,
                opts(&["--release", "--prefix", "/opt/p"])
            ),
            Plan::Exe {
                cargo_args: vec!["--release".into()],
                exe: root.join("target/release/rustty"),
                env: (
                    "RUSTTY_RESOURCES_DIR".into(),
                    PathBuf::from("/opt/p/share/rustty")
                ),
                args: vec![],
            }
        );
        let Plan::Exe {
            cargo_args, exe, ..
        } = plan(
            "linux",
            "rustty",
            root,
            opts(&["--profile", "release-safe"]),
        )
        else {
            panic!("the GTK arm");
        };
        assert_eq!(cargo_args, ["--profile", "release-safe"]);
        assert_eq!(exe, root.join("target/release-safe/rustty"));
        // The identity-oracle profile's names (R2).
        let Plan::Exe { env, exe, .. } = plan("linux", "ghostty", root, opts(&[])) else {
            panic!("the GTK arm");
        };
        assert_eq!(
            env,
            (
                "GHOSTTY_RESOURCES_DIR".into(),
                root.join("zig-out/share/ghostty")
            )
        );
        assert_eq!(exe, root.join("target/debug/ghostty"));
    }

    /// `build.zig:299-320`: macOS runs the native-only app build.
    #[test]
    fn macos_arm_runs_the_native_app() {
        let root = Path::new("/w");
        assert_eq!(
            plan("macos", "rustty", root, opts(&["--", "-e", "top"])),
            Plan::MacApp {
                script: root.join("macos/run-rustty.sh"),
                env: vec![("XCFRAMEWORK_TARGET".into(), "native".into())],
                args: vec!["-e".into(), "top".into()],
            }
        );
    }

    /// No app runtime and not Darwin: the reference's run step is empty.
    #[test]
    fn other_targets_do_nothing() {
        assert_eq!(
            plan("windows", "rustty", Path::new("/w"), opts(&[])),
            Plan::Nothing
        );
    }

    #[test]
    fn flags_are_strict_and_app_args_follow_the_separator() {
        assert!(parse(&mut ["+version".to_string()].into_iter()).is_err());
        assert!(parse(&mut ["--prefix".to_string()].into_iter()).is_err());
        assert_eq!(opts(&["--", "--release"]).args, ["--release"]);
        assert_eq!(opts(&["--", "--release"]).profile, None);
    }

    /// The executor really hands the variable and the arguments to the child
    /// and returns its exit status as its own.
    #[cfg(unix)]
    #[test]
    fn exec_passes_the_environment_and_the_exit_status() {
        let ok = exec(
            Path::new("/bin/sh"),
            &[
                "-c".into(),
                r#"test "$RUSTTY_RESOURCES_DIR" = /x/share/rustty"#.into(),
            ],
            &[("RUSTTY_RESOURCES_DIR".into(), "/x/share/rustty".into())],
        )
        .unwrap();
        assert_eq!(ok, ExitCode::SUCCESS);
        let bad = exec(Path::new("/bin/sh"), &["-c".into(), "exit 3".into()], &[]).unwrap();
        assert_eq!(bad, ExitCode::from(3));
    }
}
