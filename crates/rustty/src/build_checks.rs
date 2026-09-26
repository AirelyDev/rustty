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

#[cfg(test)]
mod tests {
    use super::*;

    /// Each early return of `checkNixShell`, then the error case.
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
