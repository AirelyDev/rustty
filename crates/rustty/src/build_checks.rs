//! The build-time decisions of `src/build/GhosttyExe.zig` for the `rustty`
//! executable, as pure functions.
//!
//! Compiled into `build.rs` through `#[path]`, where it runs, and into the
//! binary's unit tests, where it is tested. `std` only.

use std::path::Path;
use std::process::Command;

/// `GhosttyExe.zig:66-106` `checkNixShell`: "If we're in NixOS but not in the
/// shell environment then we issue a warning because the rpath may not be
/// setup properly." The reference reports it with `exe.step.addError`, which
/// fails the build; the port fails the build script with the same text. The
/// text is the reference's with the product name (R2) and the build command
/// (`cargo build`, its flags, `target/`) substituted.
///
/// `target_os` is cargo's `CARGO_CFG_TARGET_OS`; `native` is whether the
/// build targets the host (the reference's `isNativeCpu() and isNativeOs()`);
/// `etc_nixos` is whether `/etc/NIXOS` exists; `in_nix_shell` is whether
/// `IN_NIX_SHELL` is set.
pub fn nix_shell_error(
    target_os: &str,
    native: bool,
    etc_nixos: bool,
    in_nix_shell: bool,
) -> Option<String> {
    // Non-Linux doesn't have rpath issues.
    if target_os != "linux" {
        return None;
    }

    // When cross-compiling, we don't need to worry about matching our
    // Nix shell rpath since the resulting binary will be run on a
    // separate system.
    if !native {
        return None;
    }

    // Verify we're in NixOS
    if !etc_nixos {
        return None;
    }

    // If we're in a nix shell, not a problem
    if in_nix_shell {
        return None;
    }

    Some(
        "\x1b[33m\x1b[2mDetected building on and for NixOS outside of the Nix shell environment.\n\
         \n\
         The resulting rustty binary will likely fail on launch because it is\n\
         unable to dynamically load the windowing libs (X11, Wayland, etc.).\n\
         We highly recommend running only within the Nix build environment\n\
         and the resulting binary will be portable across your system.\n\
         \n\
         To run in the Nix build environment, use the following command.\n\
         Append any additional options like (`--release` flags). The resulting\n\
         binary will be in target as usual.\n\
         \n\
         \x20 nix develop -c cargo build\n\
         \x1b[0m"
            .to_string(),
    )
}

/// `build/Config.zig:132-141` (task S-1d, NEW-FINDINGS N313): "The full Ghostty
/// build no longer supports iOS; Fail early with a clear message rather than
/// partway through the build." The reference's condition is `os.tag == .ios and
/// !emit_lib_vt`; this crate IS the full build (librustty-vt is the separate
/// `rustty-vt-c` crate), so only the target OS decides. `target_os` is cargo's
/// `CARGO_CFG_TARGET_OS`. The build script logs the reference's
/// `std.log.err(...)` and fails (`return error.UnsupportedTarget`).
pub fn full_build_rejects_target(target_os: &str) -> bool {
    target_os == "ios"
}

/// `GhosttyExe.zig:43-49`: on Windows the executable gets the Windows GUI
/// subsystem (`src/main.rs`'s `windows_subsystem` attribute) and
/// `dist/windows/rustty.rc` (icon, manifest, version info) compiled in. This
/// returns the resource-compiler command for a Windows target and the object
/// it writes, which the build script links into the binary: MSVC's `rc.exe`
/// for the `msvc` environment, else GNU `windres` (`WINDRES`, else
/// `<arch>-w64-mingw32-windres`). The command runs in the `.rc` file's
/// directory, where its relative `rustty.manifest` / `rustty.ico` resolve.
pub fn windows_resource_command(
    target_env: &str,
    target_arch: &str,
    windres: Option<&str>,
    rc: &Path,
    out_dir: &Path,
) -> (Command, std::path::PathBuf) {
    let dir = rc.parent().unwrap_or(Path::new("."));
    if target_env == "msvc" {
        let out = out_dir.join("rustty.res");
        let mut cmd = Command::new("rc.exe");
        cmd.current_dir(dir)
            .arg("/nologo")
            .arg("/fo")
            .arg(&out)
            .arg(rc);
        (cmd, out)
    } else {
        let out = out_dir.join("rustty-res.o");
        let tool = windres
            .map(str::to_string)
            .unwrap_or_else(|| format!("{target_arch}-w64-mingw32-windres"));
        let mut cmd = Command::new(tool);
        cmd.current_dir(dir)
            .arg("-i")
            .arg(rc)
            .arg("-O")
            .arg("coff")
            .arg("-o")
            .arg(&out);
        (cmd, out)
    }
}

/// `-Dpie` (`build/Config.zig:408-412`, N422/N377):
///
/// ```text
/// config.pie = b.option(
///     bool,
///     "pie",
///     "Build a Position Independent Executable. Default true for system packages.",
/// ) orelse system_package;
/// ```
///
/// and `GhosttyExe.zig:29-30`: "Set PIE if requested", `if (cfg.pie) exe.pie =
/// true;`. Zig links a Linux executable as `ET_EXEC` unless `pie` is set;
/// rustc's Linux and FreeBSD targets link position-independent (`-pie`) by
/// default. The option is the environment variable `RUSTTY_PIE` (`true` /
/// `false`; unset or empty = not passed), like `RUSTTY_I18N`. The port has no
/// `--system` switch, so "not passed" takes the reference's non-system default,
/// false, and the packaging that passes `--system` upstream (nix, flatpak) sets
/// `RUSTTY_PIE=true`. Any other value is refused, as `zig build` refuses a
/// non-bool `-Dpie`.
///
/// Returns whether the `rustty` executable links with `-no-pie`: when the
/// option resolves to false on a target whose rustc default is PIE (Linux,
/// FreeBSD). rustc passes a user link arg after its own `-pie`, and the C
/// compiler driver takes the last of the two. Mach-O (arm64 macOS requires
/// PIE) and PE have no such switch, so the option changes nothing there.
pub fn link_no_pie(target_os: &str, pie: Option<&str>) -> Result<bool, String> {
    let pie = match pie {
        None | Some("") => false,
        Some("true") => true,
        Some("false") => false,
        Some(other) => {
            return Err(format!(
                "rustty: `-Dpie` is a bool: RUSTTY_PIE must be `true` or `false`, not {other:?}"
            ))
        }
    };
    Ok(!pie && matches!(target_os, "linux" | "freebsd"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each early return of `checkNixShell`, then the error case.
    #[test]
    fn full_build_rejects_ios_only() {
        // `Config.zig:134`: `.ios` fails the full build; every other target the
        // port builds for does not.
        assert!(full_build_rejects_target("ios"));
        for os in ["macos", "linux", "freebsd", "windows", "android", ""] {
            assert!(!full_build_rejects_target(os), "{os}");
        }
    }

    #[test]
    fn nix_shell_check_matches_the_reference_guards() {
        assert_eq!(nix_shell_error("macos", true, true, false), None);
        assert_eq!(nix_shell_error("linux", false, true, false), None);
        assert_eq!(nix_shell_error("linux", true, false, false), None);
        assert_eq!(nix_shell_error("linux", true, true, true), None);
        let msg = nix_shell_error("linux", true, true, false).unwrap();
        assert!(msg.starts_with("\x1b[33m\x1b[2mDetected building on and for NixOS"));
        assert!(msg.contains("\n  nix develop -c cargo build\n"));
        assert!(msg.ends_with("\x1b[0m"));
    }

    /// `-Dpie`: not passed = the non-system default (false), so Linux and
    /// FreeBSD link `-no-pie`; `true` keeps rustc's PIE; other targets are
    /// untouched; a non-bool value is refused.
    #[test]
    fn pie_option_resolves_like_the_reference() {
        for os in ["linux", "freebsd"] {
            assert_eq!(link_no_pie(os, None), Ok(true), "{os}");
            assert_eq!(link_no_pie(os, Some("")), Ok(true), "{os}");
            assert_eq!(link_no_pie(os, Some("false")), Ok(true), "{os}");
            assert_eq!(link_no_pie(os, Some("true")), Ok(false), "{os}");
        }
        for os in ["macos", "windows", "ios", "android"] {
            for v in [None, Some("false"), Some("true")] {
                assert_eq!(link_no_pie(os, v), Ok(false), "{os} {v:?}");
            }
        }
        for bad in ["yes", "1", "TRUE", "on"] {
            let err = link_no_pie("linux", Some(bad)).unwrap_err();
            assert!(
                err.contains("RUSTTY_PIE must be `true` or `false`"),
                "{err}"
            );
            assert!(err.contains(&format!("{bad:?}")), "{err}");
            assert!(link_no_pie("macos", Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn windows_resource_command_per_toolchain() {
        let rc = Path::new("/w/dist/windows/rustty.rc");
        let out = Path::new("/o");
        let (cmd, obj) = windows_resource_command("gnu", "x86_64", None, rc, out);
        assert_eq!(cmd.get_program(), "x86_64-w64-mingw32-windres");
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/w/dist/windows")));
        assert_eq!(obj, Path::new("/o/rustty-res.o"));
        let (cmd, _) = windows_resource_command("gnu", "x86_64", Some("windres"), rc, out);
        assert_eq!(cmd.get_program(), "windres");
        let (cmd, obj) = windows_resource_command("msvc", "x86_64", None, rc, out);
        assert_eq!(cmd.get_program(), "rc.exe");
        assert_eq!(obj, Path::new("/o/rustty.res"));
    }
}
