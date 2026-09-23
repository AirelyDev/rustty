//! The extra link arguments of the `rustty-vt-c` shared library.
//!
//! Compiled into `build.rs` through `#[path]`, where it runs, and into the
//! library's tests, where it is tested. `std` only.

/// The `cargo::rustc-cdylib-link-arg` values for a target OS (cargo's
/// `CARGO_CFG_TARGET_OS` spelling), the crate version `X.Y.Z`, and the
/// optional `RUSTTY_VT_SONAME` request.
///
/// - Apple (S64-F5): `-current_version <X.Y.Z>` ("Record the exact package
///   version for runtime inspection"), `-compatibility_version 1.0.0`
///   ("Preserve compatibility with the initial public ABI") and
///   `-headerpad_max_install_names` ("Leave room for packaging tools to
///   rewrite install names"), `GhosttyLibVt.zig:420-437`. The install name is
///   left as cargo sets it (`@rpath/librustty_vt_c.dylib`) so the build tree
///   keeps working; the CMake install renames the file and rewrites the
///   install name to `@rpath/librustty-vt.dylib`.
/// - ELF (S64-F5): the soname `librustty-vt.so.<X>` (Zig's
///   `libghostty-vt.so.<X>`), only when `RUSTTY_VT_SONAME` asks for it. The
///   CMake build sets it and links the build-tree name; a plain `cargo build`
///   leaves the cdylib without a soname, so the examples that link
///   `-lrustty_vt_c` straight from `target/` keep resolving.
/// - Android (S64-F9): "Support 16kb page sizes, required for Android 15+."
///   (`GhosttyLibVt.zig:273-276`, `lib.link_z_max_page_size = 16384`).
pub fn dylib_link_args(os: &str, version: &str, soname: Option<&str>) -> Vec<String> {
    let mut args = Vec::new();
    let apple = os == "macos" || os == "ios";
    if apple {
        args.push(format!("-Wl,-current_version,{version}"));
        args.push("-Wl,-compatibility_version,1.0.0".to_string());
        args.push("-Wl,-headerpad_max_install_names".to_string());
    }
    if os == "android" {
        args.push("-Wl,-z,max-page-size=16384".to_string());
    }
    if let Some(soname) = soname.filter(|s| !s.is_empty()) {
        if !apple && os != "windows" {
            args.push(format!("-Wl,-soname,{soname}"));
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_dylib_carries_the_reference_versions() {
        assert_eq!(
            dylib_link_args("macos", "0.1.0", None),
            [
                "-Wl,-current_version,0.1.0",
                "-Wl,-compatibility_version,1.0.0",
                "-Wl,-headerpad_max_install_names"
            ]
        );
        // The soname request does not apply to Mach-O.
        assert_eq!(dylib_link_args("ios", "0.1.0", Some("x")).len(), 3);
    }

    #[test]
    fn elf_soname_only_when_requested() {
        assert!(dylib_link_args("linux", "0.1.0", None).is_empty());
        assert!(dylib_link_args("linux", "0.1.0", Some("")).is_empty());
        assert_eq!(
            dylib_link_args("linux", "0.1.0", Some("librustty-vt.so.0")),
            ["-Wl,-soname,librustty-vt.so.0"]
        );
        assert!(dylib_link_args("windows", "0.1.0", Some("x")).is_empty());
    }

    /// S64-F9: `GhosttyLibVt.zig:273-276` sets `link_z_max_page_size = 16384`
    /// for every Android ABI.
    #[test]
    fn android_links_with_16k_max_page_size() {
        assert_eq!(
            dylib_link_args("android", "0.1.0", None),
            ["-Wl,-z,max-page-size=16384"]
        );
        assert!(!dylib_link_args("linux", "0.1.0", None)
            .iter()
            .any(|a| a.contains("max-page-size")));
    }
}
