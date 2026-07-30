//! rustty-apprt::gtk::flatpak
//!
//! PORTS: ghostty `src/apprt/gtk/flatpak.zig` (the whole file — a single
//! `resourcesDir` override).
//!
//! This is DISTINCT from `rustty_os::flatpak` (the port of `src/os/flatpak.zig`,
//! which carries `isFlatpak` and `FlatpakHostCommand`). This file is the GTK
//! apprt's resources-dir override: inside a Flatpak sandbox the app's own view of
//! its resources (`/app/share/<app>`) differs from the HOST's view of them, and
//! the host path has to be read out of `/.flatpak-info`.

use rustty_os::ResourcesDir;

/// PORTS: `flatpak.zig:7-29` `pub fn resourcesDir(alloc)`.
///
/// # Errors
/// Propagates `rustty_os::resourcesDir`'s error on the non-Flatpak fallthrough
/// (`flatpak.zig:28`). The Flatpak arm itself is infallible apart from the
/// allocations the reference's `!` covers.
pub fn resources_dir() -> Result<ResourcesDir, rustty_os::resourcesdir::ResourcesDirError> {
    // `if (comptime build_config.flatpak) { ... }` (`flatpak.zig:8`). The
    // reference's `comptime` fold is `rustty_build_config::flatpak()`, a
    // `const fn`, so this branch folds identically.
    if rustty_build_config::flatpak() {
        // "Only consult Flatpak runtime data for host case."
        // `if (internal_os.isFlatpak())` (`flatpak.zig:10`).
        if rustty_os::flatpak::isFlatpak() {
            // `var result: internal_os.ResourcesDir = .{ .app_path = try
            //  alloc.dupe(u8, "/app/share/ghostty") };` (`flatpak.zig:11-13`),
            // retargeted to the rustty product identity (R2) from the single
            // source of truth.
            let app_path = format!("/app/share/{}", rustty_identity::APP_NAME);

            // `const keyfile = glib.KeyFile.new(); defer keyfile.unref();`
            // (`flatpak.zig:16-17`) — the gtk-rs `KeyFile`'s `Drop` is the
            // `defer unref`.
            let keyfile = glib::KeyFile::new();

            // `if (keyfile.loadFromFile("/.flatpak-info", .{}, null) == 0)
            //  return result;` (`flatpak.zig:19`) — a zero return is failure.
            if keyfile
                .load_from_file("/.flatpak-info", glib::KeyFileFlags::NONE)
                .is_err()
            {
                return Ok(ResourcesDir::from_paths(Some(app_path), None));
            }

            // `const app_dir = std.mem.span(keyfile.getString("Instance",
            //  "app-path", null)) orelse return result;` (`flatpak.zig:20`).
            // `defer glib.free(app_dir.ptr)` (`:21`) is the `GString`'s drop.
            let Ok(app_dir) = keyfile.string("Instance", "app-path") else {
                return Ok(ResourcesDir::from_paths(Some(app_path), None));
            };

            // `result.host_path = try std.fs.path.join(alloc, &[_][]const u8{
            //  app_dir, "share", "ghostty" });` (`flatpak.zig:23`), R2-retargeted.
            let host_path = std::path::Path::new(app_dir.as_str())
                .join("share")
                .join(rustty_identity::APP_NAME)
                .to_string_lossy()
                .into_owned();

            // `return result;` (`flatpak.zig:24`).
            return Ok(ResourcesDir::from_paths(Some(app_path), Some(host_path)));
        }
    }

    // `return try internal_os.resourcesDir(alloc);` (`flatpak.zig:28`).
    rustty_os::resourcesDir()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With `build_config.flatpak` off — the default, and the shape this build
    /// compiles — `resourcesDir` is a pure passthrough to
    /// `internal_os.resourcesDir` (`flatpak.zig:8,28`).
    #[test]
    fn passthrough_when_flatpak_disabled() {
        if rustty_build_config::flatpak() {
            // The Flatpak arm only diverges INSIDE a sandbox; outside one it is
            // the same passthrough.
            assert!(
                !rustty_os::flatpak::isFlatpak() || std::path::Path::new("/.flatpak-info").exists()
            );
            return;
        }
        let ours = resources_dir()
            .ok()
            .and_then(|d| d.app().map(str::to_owned));
        let theirs = rustty_os::resourcesDir()
            .ok()
            .and_then(|d| d.app().map(str::to_owned));
        assert_eq!(ours, theirs);
    }
}
