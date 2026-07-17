//! Integration test: the vendored GTK asset manifest must match what is on disk.
//!
//! PORTS the reference's *runtime* `cwd.access(...)` existence assertions in
//! `src/apprt/gtk/build/gresource.zig:182,195,224` (the gresource generator
//! errors if a manifest entry has no file) — hoisted here to a build-time test
//! so a manifest/disk drift is caught in the normal gate rather than at
//! `glib-compile-resources` time.
//!
//! R5: the whole file is gated `all(target_os = "linux", feature = "gtk")` — the
//! `gresource`/`build_info` modules it references only exist under that cfg
//! (`lib.rs`), so off the gate this test crate compiles to nothing. Run it with:
//!   cargo test -p rustty-apprt --features gtk --test gtk_assets
#![cfg(all(target_os = "linux", feature = "gtk"))]

use std::path::PathBuf;

use rustty_apprt::gtk::{build_info, gresource};

/// The vendored asset root: `<crate>/assets`.
fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

/// Every blueprint in the manifest must have a `.blp` on disk at
/// `assets/ui/{major}.{minor}/{name}.blp`. The manifest is **19** (AD-3):
/// `ui/1.0/clipboard-confirmation-dialog.blp` and `ui/1.2/debug-warning.blp`
/// are dropped by the `>= 4.14 / >= 1.5` baseline pin.
#[test]
fn blueprint_manifest_matches_disk() {
    assert_eq!(
        gresource::BLUEPRINTS.len(),
        19,
        "AD-3: manifest is 19, not 21"
    );
    for bp in gresource::BLUEPRINTS {
        let path = assets_dir().join(format!("ui/{}.{}/{}.blp", bp.major, bp.minor, bp.name));
        assert!(
            path.is_file(),
            "manifest blueprint {}.{}/{} has no file at {}",
            bp.major,
            bp.minor,
            bp.name,
            path.display()
        );
    }
}

/// The 4 CSS files plus the icon set must exist on disk. `ICON_SIZES` is the
/// 6-entry `{16,32,128,256,512,1024}` set (`gresource.zig:24`); each contributes
/// a 1x (`{size}.png`) and a 2x (`{size*2}.png`) source, so 12 png *references*
/// resolve onto 8 distinct files.
#[test]
fn css_and_icon_manifest_matches_disk() {
    assert_eq!(gresource::CSS.len(), 4);
    for name in gresource::CSS {
        let path = assets_dir().join("css").join(name);
        assert!(
            path.is_file(),
            "manifest css {name} has no file at {}",
            path.display()
        );
    }

    assert_eq!(gresource::ICON_SIZES, &[16u32, 32, 128, 256, 512, 1024]);
    let mut png_refs = 0;
    for &size in gresource::ICON_SIZES {
        for src in [size, size * 2] {
            let path = assets_dir().join(format!("images/gnome/{src}.png"));
            assert!(
                path.is_file(),
                "manifest icon {src}.png has no file at {}",
                path.display()
            );
            png_refs += 1;
        }
    }
    assert_eq!(png_refs, 12, "6 icon sizes * (1x + 2x) = 12 png references");
}

/// The build-mode identity split (`build/info.zig:7-10,15-17`): under
/// `debug_assertions` (the `.Debug`/`.ReleaseSafe` arm) the application id / object
/// path carry the `-debug` / `_debug` suffix; otherwise they equal the base.
/// The resource path is the bundle id in reverse-DNS *path* form, and it is the
/// prefix of every blueprint resource path.
#[test]
fn build_info_ids() {
    // Identity is sourced from rustty-identity (R2), never hardcoded.
    assert_eq!(build_info::BASE_APPLICATION_ID, rustty_identity::BUNDLE_ID);

    if cfg!(debug_assertions) {
        assert_eq!(
            build_info::APPLICATION_ID,
            format!("{}-debug", build_info::BASE_APPLICATION_ID)
        );
        assert_eq!(
            build_info::OBJECT_PATH,
            format!("{}_debug", build_info::RESOURCE_PATH)
        );
    } else {
        assert_eq!(build_info::APPLICATION_ID, build_info::BASE_APPLICATION_ID);
        assert_eq!(build_info::OBJECT_PATH, build_info::RESOURCE_PATH);
    }

    // resource_path is `/` + bundle_id with `.` -> `/` (reference literal
    // `/com/mitchellh/ghostty` == `com.mitchellh.ghostty` in path form).
    let expected_rp = format!("/{}", rustty_identity::BUNDLE_ID.replace('.', "/"));
    assert_eq!(build_info::RESOURCE_PATH, expected_rp);

    // The resolver builds `{resource_path}/ui/{maj}.{min}/{name}.ui` and every
    // blueprint path is rooted at resource_path.
    for bp in gresource::BLUEPRINTS {
        let path = gresource::blueprint(bp.major, bp.minor, bp.name);
        assert_eq!(
            path,
            format!(
                "{}/ui/{}.{}/{}.ui",
                build_info::RESOURCE_PATH,
                bp.major,
                bp.minor,
                bp.name
            )
        );
    }
}
