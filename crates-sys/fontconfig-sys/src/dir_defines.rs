//! The compiled-in configuration-directory defines of the vendored fontconfig.
//!
//! Compiled into `build.rs` through `#[path]`, where it runs, and into the
//! library's tests, where it is tested (the `rustty-vt-c` `dylib_link_args.rs`
//! pattern). `std` only.

/// `FC_TEMPLATEDIR`, `FONTCONFIG_PATH` and `CONFIGDIR` for a target OS
/// (cargo's `CARGO_CFG_TARGET_OS` spelling), as `(name, quoted value)` pairs.
///
/// PORTS the non-Windows arm of `pkg/fontconfig/build.zig:178-187`: FreeBSD
/// keeps its configuration under `/usr/local/etc/fonts`, every other OS under
/// `/etc/fonts` (S31-F1: the port used the second set on FreeBSD too).
pub fn fc_dir_defines(target_os: &str) -> [(&'static str, &'static str); 3] {
    // if (target.result.os.tag == .freebsd) {
    if target_os == "freebsd" {
        [
            // "-DFC_TEMPLATEDIR=\"/usr/local/etc/fonts/conf.avail\"",
            ("FC_TEMPLATEDIR", "\"/usr/local/etc/fonts/conf.avail\""),
            // "-DFONTCONFIG_PATH=\"/usr/local/etc/fonts\"",
            ("FONTCONFIG_PATH", "\"/usr/local/etc/fonts\""),
            // "-DCONFIGDIR=\"/usr/local/etc/fonts/conf.d\"",
            ("CONFIGDIR", "\"/usr/local/etc/fonts/conf.d\""),
        ]
    } else {
        [
            // "-DFC_TEMPLATEDIR=\"/usr/share/fontconfig/conf.avail\"",
            ("FC_TEMPLATEDIR", "\"/usr/share/fontconfig/conf.avail\""),
            // "-DFONTCONFIG_PATH=\"/etc/fonts\"",
            ("FONTCONFIG_PATH", "\"/etc/fonts\""),
            // "-DCONFIGDIR=\"/usr/local/fontconfig/conf.d\"",
            ("CONFIGDIR", "\"/usr/local/fontconfig/conf.d\""),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::fc_dir_defines;

    /// S31-F1: `pkg/fontconfig/build.zig:178-183`, the FreeBSD arm.
    #[test]
    fn freebsd_uses_usr_local_etc_fonts() {
        assert_eq!(
            fc_dir_defines("freebsd"),
            [
                ("FC_TEMPLATEDIR", "\"/usr/local/etc/fonts/conf.avail\""),
                ("FONTCONFIG_PATH", "\"/usr/local/etc/fonts\""),
                ("CONFIGDIR", "\"/usr/local/etc/fonts/conf.d\""),
            ]
        );
    }

    /// `pkg/fontconfig/build.zig:184-187`, every other non-Windows OS.
    #[test]
    fn other_oses_use_etc_fonts() {
        for os in ["linux", "macos", "android"] {
            assert_eq!(
                fc_dir_defines(os),
                [
                    ("FC_TEMPLATEDIR", "\"/usr/share/fontconfig/conf.avail\""),
                    ("FONTCONFIG_PATH", "\"/etc/fonts\""),
                    ("CONFIGDIR", "\"/usr/local/fontconfig/conf.d\""),
                ],
                "{os}"
            );
        }
    }
}
