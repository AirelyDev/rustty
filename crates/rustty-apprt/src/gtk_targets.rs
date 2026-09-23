//! PORTS `src/build/gtk.zig` `targets()` and the `x11` / `wayland` option
//! defaults it feeds (`Config.zig:167, 251-261`), S65-F6.
//!
//! Compiled into `build.rs` through `#[path]`, where it decides the
//! `rustty_gtk_x11` / `rustty_gtk_wayland` cfgs the winproto arms are gated
//! on, and into the library's tests. `std` only.
//!
//! The cargo features `x11` / `wayland` (default-on) say which arms MAY be
//! built: they pull in the protocol crates. Which arms ARE built follows the
//! reference: an explicit option wins (`RUSTTY_GTK_X11` / `RUSTTY_GTK_WAYLAND`
//! = `true` | `false`, the port's `-Dx11=` / `-Dwayland=`), and otherwise the
//! default is whether the system GTK4 was built with that target, from
//! `pkg-config --variable=targets gtk4`.

/// Which targets GTK4 was compiled with.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Targets {
    pub x11: bool,
    pub wayland: bool,
}

/// `targets()` over `pkg-config`'s output: `None` when pkg-config could not
/// be run or failed ("We allow it to fail so that zig build --help works
/// without all dependencies. The build will fail later when GTK isn't found
/// anyways."), which yields neither target.
pub fn parse_targets(output: Option<&str>) -> Targets {
    let Some(output) = output else {
        return Targets::default();
    };
    Targets {
        x11: output.contains("x11"),
        wayland: output.contains("wayland"),
    }
}

/// Run `pkg-config --variable=targets gtk4`.
// Called by `build.rs`; the library's test build includes this file only for
// the pure functions' tests.
#[allow(dead_code)]
pub fn probe() -> Option<String> {
    let out = std::process::Command::new("pkg-config")
        .args(["--variable=targets", "gtk4"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Whether one winproto arm is built: its cargo feature must be on (the arm's
/// crates are only there then), and then the explicit option if given, else
/// the probed GTK target.
pub fn arm_enabled(feature: bool, explicit: Option<&str>, probed: bool) -> bool {
    if !feature {
        return false;
    }
    match explicit {
        Some("true") => true,
        Some("false") => false,
        _ => probed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `gtk.zig:14-26`.
    #[test]
    fn targets_follow_pkg_config_output() {
        assert_eq!(parse_targets(None), Targets::default());
        assert_eq!(
            parse_targets(Some("broadway wayland x11\n")),
            Targets {
                x11: true,
                wayland: true
            }
        );
        assert_eq!(
            parse_targets(Some("wayland\n")),
            Targets {
                x11: false,
                wayland: true
            }
        );
        assert_eq!(
            parse_targets(Some("x11\n")),
            Targets {
                x11: true,
                wayland: false
            }
        );
    }

    /// `Config.zig:251-261`: `b.option(...) orelse gtk_targets.<arm>`.
    #[test]
    fn explicit_option_wins_else_the_probe() {
        assert!(arm_enabled(true, None, true));
        assert!(!arm_enabled(true, None, false));
        assert!(arm_enabled(true, Some("true"), false));
        assert!(!arm_enabled(true, Some("false"), true));
        // Without the cargo feature the arm's crates are absent.
        assert!(!arm_enabled(false, Some("true"), true));
    }
}
