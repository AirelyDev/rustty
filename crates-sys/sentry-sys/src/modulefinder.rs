//! The per-OS module-finder translation unit of the vendored sentry-native.
//!
//! Compiled into `build.rs` through `#[path]`, where it runs, and into the
//! library's tests, where it is tested (the `rustty-vt-c` `dylib_link_args.rs`
//! pattern). `std` only.

/// The module finder `build.rs` compiles for a target OS (cargo's
/// `CARGO_CFG_TARGET_OS` spelling): `Ok(Some(file))`, `Ok(None)` for
/// freestanding (`none`), and `Err(warning)` for any other OS.
///
/// PORTS `pkg/sentry/build.zig:94-124`. The reference fails the build on an
/// unsupported OS (`std.log.warn("target={} not supported", ...)` then
/// `return error.UnsupportedTarget`); the port used to skip the module finder
/// there and build on (S38-F4). The `Err` string is the reference's warning,
/// with the OS tag spelled as Zig's `{}` prints an enum (`.freebsd`).
pub fn modulefinder_source(target_os: &str) -> Result<Option<&'static str>, String> {
    match target_os {
        // .windows => "src/modulefinder/sentry_modulefinder_windows.c",
        "windows" => Ok(Some("src/modulefinder/sentry_modulefinder_windows.c")),
        // .macos, .ios => "src/modulefinder/sentry_modulefinder_apple.c",
        "macos" | "ios" => Ok(Some("src/modulefinder/sentry_modulefinder_apple.c")),
        // .linux => "src/modulefinder/sentry_modulefinder_linux.c",
        "linux" => Ok(Some("src/modulefinder/sentry_modulefinder_linux.c")),
        // .freestanding => {},
        "none" => Ok(None),
        // else => {
        //     std.log.warn("target={} not supported", .{target.result.os.tag});
        //     return error.UnsupportedTarget;
        // },
        other => Err(format!("target=.{other} not supported")),
    }
}

#[cfg(test)]
mod tests {
    use super::modulefinder_source;

    #[test]
    fn supported_oses_select_their_module_finder() {
        assert_eq!(
            modulefinder_source("windows"),
            Ok(Some("src/modulefinder/sentry_modulefinder_windows.c"))
        );
        assert_eq!(
            modulefinder_source("macos"),
            Ok(Some("src/modulefinder/sentry_modulefinder_apple.c"))
        );
        assert_eq!(
            modulefinder_source("ios"),
            Ok(Some("src/modulefinder/sentry_modulefinder_apple.c"))
        );
        assert_eq!(
            modulefinder_source("linux"),
            Ok(Some("src/modulefinder/sentry_modulefinder_linux.c"))
        );
        // Freestanding builds without one.
        assert_eq!(modulefinder_source("none"), Ok(None));
    }

    /// S38-F4: any other OS fails the build, as `error.UnsupportedTarget` does.
    #[test]
    fn an_unsupported_os_is_an_error() {
        assert_eq!(
            modulefinder_source("freebsd"),
            Err("target=.freebsd not supported".to_string())
        );
        assert_eq!(
            modulefinder_source("android"),
            Err("target=.android not supported".to_string())
        );
    }
}
