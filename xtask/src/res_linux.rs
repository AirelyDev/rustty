//! Linux desktop-application resources (`xtask resources --target-os linux`).
//!
//! PORTS: `addLinuxAppResources` of ghostty's `src/build/GhosttyResources.zig`
//! (gated there on `cfg.target.result.os.tag == .linux`) — the resource files
//! that make the GTK app a proper Linux desktop application:
//!
//!   * the templated `.desktop` launcher, D-Bus activation service, `systemd`
//!     user service, and AppStream metainfo (`dist/linux/*.in`, cmake `@VAR@`
//!     templates);
//!   * the Dolphin (KDE) and Nautilus (GNOME) right-click actions;
//!   * the hicolor icon set from `images/gnome/` (including the `@2` scaled
//!     aliases and the flatpak 512px cap).
//!
//! F-018: these committed, rebranded assets previously had NO build/install
//! consumer — `xtask resources` never staged them, so a PACKAGING.md-following
//! Linux package shipped no launcher/icons. This module is that consumer.
//!
//! Template mechanics: ghostty runs each `.in` file through
//! `std.Build.Step.ConfigHeader` (cmake style, substituting `@NAME@`/`@APPID@`
//! and the exe-path token) and then strips the FIRST LINE with `tail -n +2` —
//! that stripped line is the `ConfigHeader`-GENERATED comment header, not
//! template content (a `//` comment would corrupt a `.desktop` file). Plain
//! substitution here produces no such header, so the net output is identical
//! and no strip step exists. rustty's rebranded templates spell the exe token
//! `@RUSTTY@` (was `@GHOSTTY@` — rename-casing fidelity).
//!
//! Identity: the `dist/linux/` templates are REBRANDED assets hardcoding the
//! published `io.airely.rustty` identity (`Icon=`, homepage URLs, …), so the
//! app id and desktop name here are data-identity — pinned to the `rustty`
//! product and NOT flipped by the `identity-oracle` profile (see the
//! [`crate::APP`] doc's "NOT used for…" list). Only share-tree FILENAMES that
//! follow [`crate::APP`] elsewhere (the Nautilus extension basename) follow it
//! here.

use std::path::{Path, PathBuf};

/// The published Linux application id (ghostty: `com.mitchellh.ghostty`).
/// Mirrors `rustty_identity::macos::BUNDLE_ID` — a literal here because the
/// `dist/linux` templates hardcode the same rebranded identity (data-identity;
/// see the module doc).
const BASE_APP_ID: &str = "io.airely.rustty";

/// The desktop-facing product name (ghostty: `Ghostty`). Data-identity, like
/// [`BASE_APP_ID`].
const BASE_NAME: &str = "Rustty";

/// Knobs of ghostty's `addLinuxAppResources` that come off its build config.
pub struct Options {
    /// Absolute path of the INSTALLED binary, substituted for the templates'
    /// `@RUSTTY@` (ghostty: `"{install_prefix}/bin/ghostty"`). The `.desktop`
    /// `TryExec`/`Exec` and the D-Bus/systemd `Exec*` lines point here, so it
    /// must be the final `$PREFIX/bin/rustty` path, not a staging path.
    pub exe_abs_path: String,
    /// Flatpak build (ghostty `cfg.flatpak`): swaps the D-Bus service template
    /// for the flatpak variant, drops the systemd user service, and caps icons
    /// at 512x512 (flatpaks support nothing larger).
    pub flatpak: bool,
    /// Debug build (ghostty `cfg.optimize == .Debug/.ReleaseSafe`): suffixes
    /// the name with ` (Debug)` and the app id with `-debug` for the templated
    /// outputs. The Dolphin action and icons keep the BASE app id — the oracle
    /// hardcodes those paths.
    pub debug: bool,
    /// System-package install (ghostty `b.graph.system_package_mode`): systemd
    /// looks for user services under `lib/systemd/user` for system packages
    /// but `share/systemd/user` otherwise, so this routes the unit to `lib/`
    /// (a SIBLING of `share/`).
    pub system_package_mode: bool,
}

impl Options {
    /// The effective desktop name: `Rustty`, or `Rustty (Debug)` for debug
    /// builds (ghostty's `name` fmt).
    fn name(&self) -> String {
        format!("{}{}", BASE_NAME, if self.debug { " (Debug)" } else { "" })
    }

    /// The effective app id: `io.airely.rustty`, or `io.airely.rustty-debug`
    /// for debug builds (ghostty's `app_id` fmt).
    fn app_id(&self) -> String {
        format!("{}{}", BASE_APP_ID, if self.debug { "-debug" } else { "" })
    }
}

/// Substitute the cmake-style `@NAME@` / `@APPID@` / `@RUSTTY@` tokens of a
/// `dist/linux/*.in` template (ghostty's `addConfigHeader(.cmake, .{ .NAME,
/// .APPID, .GHOSTTY })`; the generated-header strip is a no-op here — see the
/// module doc). Errors if any `@TOKEN@`-shaped placeholder survives, so a new
/// template variable can never ship unsubstituted (the metainfo's literal
/// `gael@airely.io` contact address is not token-shaped and passes through).
fn substitute(template: &str, opts: &Options) -> Result<String, String> {
    let out = template
        .replace("@NAME@", &opts.name())
        .replace("@APPID@", &opts.app_id())
        .replace("@RUSTTY@", &opts.exe_abs_path);

    // Reject surviving @UPPERCASE_TOKEN@ placeholders.
    let bytes = out.as_bytes();
    let mut i = 0;
    while let Some(start) = out[i..].find('@').map(|p| i + p) {
        let rest = &bytes[start + 1..];
        if let Some(end) = rest.iter().position(|&b| b == b'@') {
            let token = &rest[..end];
            if !token.is_empty() && token.iter().all(|&b| b.is_ascii_uppercase() || b == b'_') {
                return Err(format!(
                    "unsubstituted template token @{}@",
                    String::from_utf8_lossy(token)
                ));
            }
        }
        i = start + 1;
    }
    Ok(out)
}

/// Write `contents` at `dst`, creating parent directories.
fn write_out(dst: &Path, contents: &[u8]) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(dst, contents).map_err(|e| format!("write {}: {e}", dst.display()))
}

/// Copy `src` to `dst`, creating parent directories.
fn copy_out(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("copy {} -> {}: {e}", src.display(), dst.display()))
}

/// Stage the Linux desktop-application resources into the install tree
/// (ghostty `addLinuxAppResources`). `share_dir` is the `share/` directory
/// (e.g. `zig-out/share`), matching the sibling `gen_*` modules; the
/// system-package systemd unit is the one output routed to its `lib/` sibling.
pub fn gen_linux_app_resources(share_dir: &Path, opts: &Options) -> Result<(), String> {
    let root = crate::workspace_root();
    let dist = root.join("dist/linux");
    let images = root.join("images/gnome");
    let app_id = opts.app_id();

    // -- The cmake templates: (source, destination). Mirrors the oracle's
    //    `templates` list one-for-one, including the flatpak D-Bus swap and
    //    the flatpak systemd skip.
    let mut templates: Vec<(PathBuf, PathBuf)> = vec![
        // Desktop file so that we have an icon and other metadata.
        (
            dist.join("app.desktop.in"),
            share_dir.join(format!("applications/{app_id}.desktop")),
        ),
        // Service for DBus activation.
        (
            dist.join(if opts.flatpak {
                "dbus.service.flatpak.in"
            } else {
                "dbus.service.in"
            }),
            share_dir.join(format!("dbus-1/services/{app_id}.service")),
        ),
    ];
    // `systemd` user service. systemd looks for user services under `lib` for
    // system packages and `share` otherwise (ghostty's system_package_mode
    // fork). The XDG Desktop Portal requires the `app-<appid>.service` unit
    // name to recover the application id — see the oracle's comment trail
    // (xdg-desktop-portal xdp-utils.c) for the undocumented requirement.
    if !opts.flatpak {
        let unit_root = if opts.system_package_mode {
            share_dir
                .parent()
                .ok_or_else(|| format!("share dir {} has no parent for lib/", share_dir.display()))?
                .join("lib")
        } else {
            share_dir.to_path_buf()
        };
        templates.push((
            dist.join("systemd.service.in"),
            unit_root.join(format!("systemd/user/app-{app_id}.service")),
        ));
    }
    // AppStream metainfo so that the application has rich metadata within app
    // stores.
    templates.push((
        dist.join("io.airely.rustty.metainfo.xml.in"),
        share_dir.join(format!("metainfo/{app_id}.metainfo.xml")),
    ));

    for (src, dst) in &templates {
        let template = std::fs::read_to_string(src)
            .map_err(|e| format!("read template {}: {e}", src.display()))?;
        let rendered =
            substitute(&template, opts).map_err(|e| format!("render {}: {e}", src.display()))?;
        write_out(dst, rendered.as_bytes())?;
    }

    // Right click menu action for Plasma desktop. BASE app id — the oracle
    // hardcodes `com.mitchellh.ghostty.desktop` here regardless of profile.
    copy_out(
        &dist.join("rustty_dolphin.desktop"),
        &share_dir.join(format!("kio/servicemenus/{BASE_APP_ID}.desktop")),
    )?;

    // Right click menu action for Nautilus. Note that this _must_ keep the
    // short name (`rustty.py`); using the full app id causes problems
    // (ghostty #5468). The basename follows `crate::APP` like the rest of the
    // share tree.
    copy_out(
        &dist.join("rustty_nautilus.py"),
        &share_dir.join(format!("nautilus-python/extensions/{}.py", crate::APP)),
    )?;

    // Various icons that our application can use, including the icon that will
    // be used for the desktop. BASE app id (oracle-hardcoded), sizes and @2
    // aliases per the oracle's map; flatpaks only support icons up to 512x512.
    let mut icons: Vec<(&str, &str)> = vec![
        ("16.png", "16x16"),
        ("32.png", "32x32"),
        ("128.png", "128x128"),
        ("256.png", "256x256"),
        ("512.png", "512x512"),
    ];
    if !opts.flatpak {
        icons.push(("1024.png", "1024x1024"));
    }
    icons.extend([
        ("32.png", "16x16@2"),
        ("64.png", "32x32@2"),
        ("256.png", "128x128@2"),
        ("512.png", "256x256@2"),
    ]);
    for (src_name, size_dir) in icons {
        copy_out(
            &images.join(src_name),
            &share_dir.join(format!("icons/hicolor/{size_dir}/apps/{BASE_APP_ID}.png")),
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_share(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "rustty-xtask-linuxres-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let share = base.join("prefix/share");
        std::fs::create_dir_all(&share).unwrap();
        share
    }

    fn read(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
    }

    /// F-018 KAT: the default (release, non-flatpak, non-system-package) run
    /// stages every oracle-mapped path with fully substituted templates.
    #[test]
    fn default_layout_matches_oracle_map() {
        let share = temp_share("default");
        gen_linux_app_resources(
            &share,
            &Options {
                exe_abs_path: "/usr/bin/rustty".into(),
                flatpak: false,
                debug: false,
                system_package_mode: false,
            },
        )
        .expect("gen_linux_app_resources");

        // Templated outputs — every oracle-mapped relative path exists.
        let desktop = read(&share.join("applications/io.airely.rustty.desktop"));
        let dbus = read(&share.join("dbus-1/services/io.airely.rustty.service"));
        let systemd = read(&share.join("systemd/user/app-io.airely.rustty.service"));
        let metainfo = read(&share.join("metainfo/io.airely.rustty.metainfo.xml"));

        // Substitution happened (and left no token behind).
        for (name, text) in [
            ("desktop", &desktop),
            ("dbus", &dbus),
            ("systemd", &systemd),
            ("metainfo", &metainfo),
        ] {
            for token in ["@NAME@", "@APPID@", "@RUSTTY@"] {
                assert!(!text.contains(token), "{name}: unsubstituted {token}");
            }
        }
        assert!(desktop.contains("Name=Rustty\n"));
        assert!(desktop.contains("Exec=/usr/bin/rustty --gtk-single-instance=true"));
        assert!(desktop.contains("StartupWMClass=io.airely.rustty"));
        // The template's own first line is PRESERVED (the oracle's `tail -n +2`
        // strips only the ConfigHeader-generated comment, which plain
        // substitution never emits).
        assert!(desktop.starts_with("[Desktop Entry]\n"));
        // Non-flatpak D-Bus service points at the systemd unit.
        assert!(dbus.contains("SystemdService=app-io.airely.rustty.service"));
        assert!(systemd.contains("BusName=io.airely.rustty"));
        assert!(metainfo.contains("<id>io.airely.rustty</id>"));
        // The metainfo contact address's '@' must pass the token check.
        assert!(metainfo.contains("gael@airely.io"));

        // Dolphin + Nautilus actions.
        assert!(share
            .join("kio/servicemenus/io.airely.rustty.desktop")
            .is_file());
        assert!(share
            .join(format!("nautilus-python/extensions/{}.py", crate::APP))
            .is_file());

        // Icons: the five base sizes, the non-flatpak 1024, and the four @2
        // aliases; @2 aliases hold the DOUBLED source image's bytes.
        let root = crate::workspace_root();
        for size_dir in [
            "16x16",
            "32x32",
            "128x128",
            "256x256",
            "512x512",
            "1024x1024",
            "16x16@2",
            "32x32@2",
            "128x128@2",
            "256x256@2",
        ] {
            assert!(
                share
                    .join(format!(
                        "icons/hicolor/{size_dir}/apps/io.airely.rustty.png"
                    ))
                    .is_file(),
                "missing {size_dir} icon"
            );
        }
        assert_eq!(
            std::fs::read(share.join("icons/hicolor/16x16@2/apps/io.airely.rustty.png")).unwrap(),
            std::fs::read(root.join("images/gnome/32.png")).unwrap(),
            "16x16@2 must be the 32px image (oracle @2 map)"
        );

        let _ = std::fs::remove_dir_all(share.parent().unwrap().parent().unwrap());
    }

    /// F-018 KAT: the flatpak arm swaps the D-Bus template (no SystemdService
    /// line), drops the systemd user unit, and caps icons at 512x512.
    #[test]
    fn flatpak_arm_swaps_dbus_and_caps_icons() {
        let share = temp_share("flatpak");
        gen_linux_app_resources(
            &share,
            &Options {
                exe_abs_path: "/app/bin/rustty".into(),
                flatpak: true,
                debug: false,
                system_package_mode: false,
            },
        )
        .expect("gen_linux_app_resources");

        let dbus = read(&share.join("dbus-1/services/io.airely.rustty.service"));
        assert!(
            !dbus.contains("SystemdService="),
            "flatpak dbus service must not reference systemd"
        );
        assert!(dbus.contains("Exec=/app/bin/rustty"));
        assert!(
            !share
                .join("systemd/user/app-io.airely.rustty.service")
                .exists(),
            "flatpak must not install a systemd user unit"
        );
        assert!(
            !share
                .join("icons/hicolor/1024x1024/apps/io.airely.rustty.png")
                .exists(),
            "flatpak icons cap at 512x512"
        );
        assert!(share
            .join("icons/hicolor/512x512/apps/io.airely.rustty.png")
            .is_file());

        let _ = std::fs::remove_dir_all(share.parent().unwrap().parent().unwrap());
    }

    /// F-018 KAT: debug builds suffix the templated name/app id (` (Debug)` /
    /// `-debug`) while the Dolphin action and icons keep the BASE app id
    /// (oracle-hardcoded paths); system-package mode routes the systemd unit
    /// to the `lib/` sibling of `share/`.
    #[test]
    fn debug_and_system_package_arms() {
        let share = temp_share("dbg-spm");
        gen_linux_app_resources(
            &share,
            &Options {
                exe_abs_path: "/usr/bin/rustty".into(),
                flatpak: false,
                debug: true,
                system_package_mode: true,
            },
        )
        .expect("gen_linux_app_resources");

        let desktop = read(&share.join("applications/io.airely.rustty-debug.desktop"));
        assert!(desktop.contains("Name=Rustty (Debug)\n"));
        assert!(desktop.contains("StartupWMClass=io.airely.rustty-debug"));
        assert!(share
            .join("metainfo/io.airely.rustty-debug.metainfo.xml")
            .is_file());
        // system-package mode: unit under lib/, not share/.
        let lib = share.parent().unwrap().join("lib");
        assert!(lib
            .join("systemd/user/app-io.airely.rustty-debug.service")
            .is_file());
        assert!(!share
            .join("systemd/user/app-io.airely.rustty-debug.service")
            .exists());
        // Base-id paths stay base-id in debug.
        assert!(share
            .join("kio/servicemenus/io.airely.rustty.desktop")
            .is_file());
        assert!(share
            .join("icons/hicolor/16x16/apps/io.airely.rustty.png")
            .is_file());

        let _ = std::fs::remove_dir_all(share.parent().unwrap().parent().unwrap());
    }

    /// The template renderer rejects any surviving `@TOKEN@` placeholder, so a
    /// template growing a new variable fails loudly instead of shipping it.
    #[test]
    fn substitute_rejects_unknown_tokens() {
        let opts = Options {
            exe_abs_path: "/usr/bin/rustty".into(),
            flatpak: false,
            debug: false,
            system_package_mode: false,
        };
        assert!(substitute("Exec=@RUSTTY@\n", &opts).is_ok());
        let err = substitute("Name=@NEW_TOKEN@\n", &opts).unwrap_err();
        assert!(err.contains("@NEW_TOKEN@"), "{err}");
        // A bare non-token '@' (email address) passes.
        assert!(substitute("contact gael@airely.io\n", &opts).is_ok());
    }
}
