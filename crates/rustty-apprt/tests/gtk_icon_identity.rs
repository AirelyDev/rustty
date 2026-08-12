//! Identity guard for the vendored GTK application-icon PNGs (W14-01).
//!
//! The gresource pipeline (`build.rs`) embeds `assets/images/gnome/{size}.png`
//! under aliases named by `rustty_identity::BUNDLE_ID` — Rustty's application
//! identity. The R2 identity fork therefore requires those bytes to be RUSTTY
//! artwork (the committed art at `<repo>/images/gnome`), not the Ghostty
//! oracle's icons: Ghostty serves Ghostty art under `com.mitchellh.ghostty`,
//! Rustty serves Rustty art under `io.airely.rustty` — structural parity with
//! the oracle's identity-consistent routing, not byte parity (the one asset
//! class where verbatim vendoring is WRONG; see the provenance note in
//! `src/gtk/gresource.rs`).
//!
//! Deliberately UNGATED (unlike `tests/gtk_assets.rs`): the comparison is pure
//! file I/O, so it runs on every host — including macOS, where the linux+gtk
//! gresource pipeline itself never compiles — and prevents a regression to the
//! 8 pinned upstream Ghostty icons.

use std::path::PathBuf;

/// The crate-local vendored asset root: `<crate>/assets/images/gnome`.
fn crate_icons_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/images/gnome")
}

/// The committed Rustty artwork: `<repo>/images/gnome` (two levels up from the
/// crate — the workspace root).
fn repo_icons_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../images/gnome")
}

/// Every embedded icon source (the 6 `ICON_SIZES` at 1x and 2x = 8 distinct
/// files: 16..2048) must be byte-identical to the committed Rustty artwork.
/// Byte equality subsumes the SHA-256 comparison the vet fix plan asks for,
/// with no hash dependency.
#[test]
fn gnome_icons_are_rustty_artwork() {
    // Mirrors gresource::ICON_SIZES = [16, 32, 128, 256, 512, 1024] expanded to
    // the 8 distinct 1x/2x source files. Spelled literally because the
    // `gresource` module only exists under `all(linux, feature = "gtk")`.
    for size in [16u32, 32, 64, 128, 256, 512, 1024, 2048] {
        let crate_png = crate_icons_dir().join(format!("{size}.png"));
        let repo_png = repo_icons_dir().join(format!("{size}.png"));
        let crate_bytes = std::fs::read(&crate_png)
            .unwrap_or_else(|e| panic!("read {}: {e}", crate_png.display()));
        let repo_bytes =
            std::fs::read(&repo_png).unwrap_or_else(|e| panic!("read {}: {e}", repo_png.display()));
        assert!(
            crate_bytes == repo_bytes,
            "{size}.png: crate-local gresource icon ({} bytes) differs from the \
             committed Rustty artwork at images/gnome ({} bytes) — the embedded \
             GTK fallback icon must carry Rustty art under Rustty's app id (W14-01)",
            crate_bytes.len(),
            repo_bytes.len(),
        );
    }
}
