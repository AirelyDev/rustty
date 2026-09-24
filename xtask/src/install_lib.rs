//! xtask install-lib — PORTS the non-Darwin `librustty-internal` install of
//! `build.zig:219-239` with `src/build/GhosttyLib.zig`'s `install`,
//! `installHeader` and `pkgConfigFiles` (`:213-300`), S63-F9.
//!
//! On a non-Darwin target the reference's `app_runtime == .none` build
//! installs the embedding library (`src/main_c.zig`, the port's
//! `rustty-apprt-c`) as a shared and a static library, its header, and a
//! pkg-config pair. "This is NOT libghostty (even though its named that for
//! historical reasons). It is just the glue between Ghostty GUI on macOS and
//! the full Ghostty GUI core." On Darwin the reference ships it only inside
//! the xcframework (`xtask xcframework`), and so does this command.
//!
//! ```sh
//! cargo run -p xtask -- install-lib --prefix /usr/local
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::workspace_root;

/// The installed library names for a target OS (`build.zig:231-237`).
pub fn installed_names(target_os: &str) -> (&'static str, &'static str) {
    if target_os == "windows" {
        ("rustty-internal.dll", "rustty-internal-static.lib")
    } else {
        ("rustty-internal.so", "rustty-internal.a")
    }
}

/// cargo's artifact names for `rustty-apprt-c` on a target (the build-side
/// names the install renames).
pub fn cargo_names(target_os: &str, target_env: &str) -> (&'static str, &'static str) {
    match (target_os, target_env) {
        ("windows", "msvc") => ("rustty_apprt_c.dll", "rustty_apprt_c.lib"),
        ("windows", _) => ("rustty_apprt_c.dll", "librustty_apprt_c.a"),
        _ => ("librustty_apprt_c.so", "librustty_apprt_c.a"),
    }
}

/// `pkgConfigFiles` (`GhosttyLib.zig:245-286`): the shared and static `.pc`
/// bodies, with `prefix` the install prefix.
pub fn pkg_config_files(prefix: &str, version: &str, target_os: &str) -> (String, String) {
    let (shared, static_) = installed_names(target_os);
    let body = |name: &str, description: &str, lib: &str| {
        format!(
            "prefix={prefix}\n\
             includedir=${{prefix}}/include\n\
             libdir=${{prefix}}/lib\n\
             \n\
             Name: {name}\n\
             URL: https://github.com/AirelyDev/rustty\n\
             Description: {description}\n\
             Version: {version}\n\
             Cflags: -I${{includedir}}\n\
             Libs: ${{libdir}}/{lib}\n\
             Libs.private:\n\
             Requires.private:"
        )
    };
    (
        body(
            "rustty-internal",
            "Rustty internal library (not for external use)",
            shared,
        ),
        body(
            "rustty-internal-static",
            "Rustty internal library, static (not for external use)",
            static_,
        ),
    )
}

/// `xtask install-lib [--prefix <dir>] [--target <triple>] [--profile <p>]`.
pub fn run(args: &mut impl Iterator<Item = String>) -> Result<PathBuf, String> {
    let root = workspace_root();
    let mut prefix = root.join("zig-out");
    let mut target: Option<String> = None;
    let mut profile = "release".to_string();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--prefix" => prefix = PathBuf::from(args.next().ok_or("--prefix requires a value")?),
            "--target" => target = Some(args.next().ok_or("--target requires a value")?),
            "--profile" => profile = args.next().ok_or("--profile requires a value")?,
            other => return Err(format!("unknown `install-lib` flag `{other}`")),
        }
    }
    let triple = target.clone().unwrap_or_else(host_triple);
    let (target_os, target_env) = os_env(&triple);
    if target_os == "macos" || target_os == "ios" {
        return Err(
            "the embedding library is shipped in the xcframework on Apple targets; use `xtask xcframework`"
                .into(),
        );
    }

    let mut cmd = Command::new("cargo");
    cmd.current_dir(&root)
        .args(["build", "-p", "rustty-apprt-c", "--profile", &profile]);
    if let Some(t) = &target {
        cmd.args(["--target", t]);
    }
    let status = cmd.status().map_err(|e| format!("spawn cargo: {e}"))?;
    if !status.success() {
        return Err(format!("cargo build exited with {status}"));
    }

    let profile_dir = if profile == "dev" {
        "debug"
    } else {
        profile.as_str()
    };
    let mut out = root.join("target");
    if let Some(t) = &target {
        out = out.join(t);
    }
    let out = out.join(profile_dir);

    let (cargo_shared, cargo_static) = cargo_names(target_os, target_env);
    let (shared, static_) = installed_names(target_os);
    let lib = prefix.join("lib");
    let include = prefix.join("include");
    let pkgconfig = prefix.join("share/pkgconfig");
    for d in [&lib, &include, &pkgconfig] {
        std::fs::create_dir_all(d).map_err(|e| format!("mkdir {}: {e}", d.display()))?;
    }
    copy(&out.join(cargo_shared), &lib.join(shared))?;
    copy(&out.join(cargo_static), &lib.join(static_))?;
    // `installHeader`: "Only need one header". The port's embedding header is
    // `include/rustty-embed.h` (`include/rustty.h` is librustty-vt's).
    copy(
        &root.join("include/rustty-embed.h"),
        &include.join("rustty-embed.h"),
    )?;

    let (pc, pc_static) = pkg_config_files(&prefix.to_string_lossy(), pc_version(), target_os);
    write(&pkgconfig.join("rustty-internal.pc"), &pc)?;
    write(&pkgconfig.join("rustty-internal-static.pc"), &pc_static)?;
    Ok(prefix)
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("copy {} -> {}: {e}", from.display(), to.display()))
}

fn write(path: &Path, body: &str) -> Result<(), String> {
    std::fs::write(path, format!("{body}\n")).map_err(|e| format!("write {}: {e}", path.display()))
}

/// The version the `.pc` files carry: `deps.config.version`
/// (`GhosttyLib.zig:265`, `:279`), which is the app version composed from the
/// checkout (`Config.zig:276-339`), pre-release and build metadata included:
/// `rustty_version::VERSION_STRING`.
//
// CORRECTION (review T-B): this was the workspace `Cargo.toml` version, the bare
// `X.Y.Z`, which only matches the reference for a `vX.Y.Z`-tagged build.
fn pc_version() -> &'static str {
    rustty_version::VERSION_STRING
}

/// The host triple, from `rustc -vV`.
fn host_triple() -> String {
    let out = Command::new("rustc").arg("-vV").output();
    out.ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        })
        .unwrap_or_default()
}

/// The OS and environment parts of a target triple, in cargo's cfg spelling.
fn os_env(triple: &str) -> (&'static str, &'static str) {
    let os = if triple.contains("-apple-darwin") {
        "macos"
    } else if triple.contains("-apple-ios") {
        "ios"
    } else if triple.contains("-windows") {
        "windows"
    } else if triple.contains("-freebsd") {
        "freebsd"
    } else {
        "linux"
    };
    let env = if triple.ends_with("-msvc") {
        "msvc"
    } else if triple.ends_with("-gnu") {
        "gnu"
    } else {
        ""
    };
    (os, env)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `build.zig:231-237`.
    #[test]
    fn installed_names_match_the_reference() {
        assert_eq!(
            installed_names("linux"),
            ("rustty-internal.so", "rustty-internal.a")
        );
        assert_eq!(
            installed_names("windows"),
            ("rustty-internal.dll", "rustty-internal-static.lib")
        );
        assert_eq!(
            cargo_names("linux", "gnu"),
            ("librustty_apprt_c.so", "librustty_apprt_c.a")
        );
        assert_eq!(os_env("x86_64-unknown-linux-gnu"), ("linux", "gnu"));
        assert_eq!(os_env("x86_64-pc-windows-msvc"), ("windows", "msvc"));
        assert_eq!(os_env("aarch64-apple-darwin").0, "macos");
    }

    /// `GhosttyLib.zig:245-286`, field for field.
    #[test]
    fn pkg_config_files_match_the_reference_fields() {
        let (pc, pcs) = pkg_config_files("/usr/local", "0.1.0", "linux");
        assert_eq!(
            pc,
            "prefix=/usr/local\n\
             includedir=${prefix}/include\n\
             libdir=${prefix}/lib\n\
             \n\
             Name: rustty-internal\n\
             URL: https://github.com/AirelyDev/rustty\n\
             Description: Rustty internal library (not for external use)\n\
             Version: 0.1.0\n\
             Cflags: -I${includedir}\n\
             Libs: ${libdir}/rustty-internal.so\n\
             Libs.private:\n\
             Requires.private:"
        );
        assert!(pcs.contains("Name: rustty-internal-static\n"));
        assert!(pcs.contains("Libs: ${libdir}/rustty-internal.a\n"));
        let (_, win) = pkg_config_files("C:/p", "0.1.0", "windows");
        assert!(win.contains("Libs: ${libdir}/rustty-internal-static.lib\n"));
    }

    /// The `.pc` `Version:` is the composed app version, not the workspace
    /// `X.Y.Z` (`GhosttyLib.zig:265`: `deps.config.version`).
    #[test]
    fn pc_version_is_the_composed_app_version() {
        assert_eq!(pc_version(), rustty_version::VERSION_STRING);
        let (pc, _) = pkg_config_files("/p", pc_version(), "linux");
        assert!(
            pc.contains(&format!("\nVersion: {}\n", rustty_version::VERSION_STRING)),
            "{pc}"
        );
    }

    #[test]
    fn apple_targets_are_refused() {
        let mut args = ["--target", "aarch64-apple-darwin"]
            .into_iter()
            .map(String::from);
        let err = run(&mut args).unwrap_err();
        assert!(err.contains("xcframework"), "{err}");
    }
}
