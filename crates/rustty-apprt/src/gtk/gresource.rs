//! rustty-apprt::gtk::gresource
//!
//! PORTS: ghostty `src/apprt/gtk/build/gresource.zig:15-123` — the gresource
//! *manifest* (blueprint / css / icon inventory) and the compile-time blueprint
//! resource-path resolver. The `main()` gresource-XML generator
//! (`gresource.zig:125-277`) and the `file_inputs` build-cache list
//! (`gresource.zig:73-96`) belong to the build system, not this library module,
//! and are ported by the crate's `build.rs` in a later task.
//!
//! PORT STATUS: done (L5.0 T4). Manifest + resolver ported; the runtime
//! `cwd.access(...)` existence assertions (`gresource.zig:182,195,224`) are
//! hoisted to `tests/gtk_assets.rs`.
//!
//! # AD-3: the manifest is 19, not 21
//!
//! The reference lists 21 blueprints (`gresource.zig:33-55`). The baseline pin
//! (gtk 4.14+ / adw 1.5+) drops the two legacy-version-only entries — the
//! `.Debug`/older-adw arms that select a compile-time artifact (the resource
//! manifest itself) rather than a live predicate, per the module-root RULE:
//!
//! * `{1, 0, "clipboard-confirmation-dialog"}` (the `1.4` variant is kept)
//! * `{1, 2, "debug-warning"}` (the `1.3` variant is kept)
//!
//! # The resolver is a compile-time lookup (`gresource.zig:103-123`)
//!
//! The reference [`blueprint`] is a `comptime` fn whose `@compileError("invalid
//! blueprint")` makes an unknown blueprint a *compile* error. rustty keeps that
//! property with a `const fn`: each manifest entry stores its precomputed
//! `{resource_path}/ui/{major}.{minor}/{name}.ui` path (const-folded from
//! [`build_info::RESOURCE_PATH`], which Rust cannot spell as a literal because
//! R2 sources it from the bundle id), and [`blueprint`] does a `const`-evaluable
//! linear scan that `panic!`s on a miss — a `panic!` inside a `const` context is
//! a compile error, exactly like `@compileError`. (Deviation from the reference
//! `[:0]const u8` return: a Rust `&'static str` is not NUL-terminated; gtk-rs
//! takes `&str` for resource paths and appends the terminator itself, so the NUL
//! is not observable.) The macro escape hatch the brief sanctions was not
//! needed — the const-fn scan compiles cleanly.
//!
//! # Vendored-asset provenance (byte-for-byte, `sha256`)
//!
//! The `ui`/`css` trees are copied verbatim from the pinned oracle
//! (`ghostty @ 4749c4e9`, `src/apprt/gtk/{ui,css}`). Mirrors the
//! `.port/tools/oracle-ghostty-h.sha256` precedent. `cmp` reported 0 diffs.
//!
//! `images/gnome` is the ONE deliberate byte deviation (W14-01): the icons are
//! byte-copies of the committed RUSTTY artwork at `<repo>/images/gnome`, NOT
//! the oracle's PNGs. The gresource aliases (`build.rs`) serve these bytes
//! under `rustty_identity::BUNDLE_ID` — Rustty's application identity — so the
//! R2 identity fork requires Rustty art here, exactly as the oracle serves
//! Ghostty art under `com.mitchellh.ghostty` (identity-consistent routing, not
//! byte parity). `tests/gtk_icon_identity.rs` enforces the byte identity
//! against the top-level art on every host.
//!
//! ```text
//! ui/1.2/close-confirmation-dialog.blp   a1bec4d6632151bb45edefc9708da853c20e6cc9b8bfda6ff21fde8374c60d1a
//! ui/1.2/config-errors-dialog.blp        edd7329520bbafc55dd397f03bc24805111d8de88d91226d1f09d4f3e09b0caa
//! ui/1.2/key-state-overlay.blp           4a5200f739ea5ef7d0c529d9df336c1d807cbc28f6ca934be7755bb3aff660fc
//! ui/1.2/resize-overlay.blp              268d2544b232446368b3097ef3c3f4e705e5fcc2e9f429fe2250c9b57dd0ab88
//! ui/1.2/search-overlay.blp              cf1ffc36bfc7b79d95136ab5d0a64393fe000b7ab25a6da130fff7ac3456dead
//! ui/1.2/surface.blp                     403829eeb9e799ce7a9a6c5c70cfe34b27056f66ebe2ed4d7bd04d326e1312fe
//! ui/1.3/debug-warning.blp               9baa15e624dd0b4b6696040e7245b4d7c098f81013d3c429f0b204748e5815d1
//! ui/1.3/surface-child-exited.blp        1f2940eb47a6281c5060a7fc8b11f594b088da195db28234e2b302a8be674e09
//! ui/1.4/clipboard-confirmation-dialog.blp dd706567d45df90da574b4bd1efe6ed9c05bf9d168dcf584f1dd12a86824547c
//! ui/1.5/command-palette.blp             a64daa211fa8802c8197dcb54960638edc071b192e8eae0371f94cd650a64b9a
//! ui/1.5/imgui-widget.blp                4e61ad7a8df2da71aa5d9c68aafc3edebbc80eaec8dad4eef1245194cbf65ec2
//! ui/1.5/inspector-widget.blp            d518cbbff0946028dca7a30355210b513b53cba33cd8b6d5b53d126a3d1e1f32
//! ui/1.5/inspector-window.blp            81f5188fd7b5fc337e7bd750b5339d3afc1ec3db5ac47eebf43ed1800e891545
//! ui/1.5/split-tree.blp                  4376cd5751f152497f40b1eb22c199422aa8513c93897af17cc85725a2c8b563
//! ui/1.5/split-tree-split.blp            039ed00ccfd0885762ca4c6228c914a398a4618f1c9764397738e9282949cdc9
//! ui/1.5/surface-scrolled-window.blp     194aa30420706049d9b4dd584d9ed1942458ae89405ab70930be6b98e800da75
//! ui/1.5/tab.blp                         5e5e0b192e41b866d3c0783afef6cae976a3536300f6e5df822aeb79acecf57e
//! ui/1.5/title-dialog.blp                661f522ed4a0f0acec2dd779deacb8fdf5b2949ad17debdbc59c599ebb35f21a
//! ui/1.5/window.blp                      275ff90ad89cb38dd57cfe1ecdc1da23e63a3be6e57ddb22c2137f74df56a60f
//! css/style.css                          41e16b541ab8806e7780ed7fc614e13d6e431f8687d32ba2428e0507615fb2cd
//! css/style-dark.css                     2a31164b2b601c8b875c4c0cb25c5a34fdae64b5d696c932f744acd05b2eed7e
//! css/style-hc.css                       45c9c179c042b2f0ef7147b3a250ef159a50916abf107d33dfa4eaf3512504bc
//! css/style-hc-dark.css                  45c9c179c042b2f0ef7147b3a250ef159a50916abf107d33dfa4eaf3512504bc
//! images/gnome/16.png                    9dcea9e0c499930871c9278d1628fe45e52eefd3f20ab597d3c6c0cec0f425ac  (rustty art)
//! images/gnome/32.png                    0a6a4e0d148b19c8489d30f304c2a5a69a58755d5e7502e70662d3cdca5a7f64  (rustty art)
//! images/gnome/64.png                    3207a731ecbd0e9e47b3f8ca93340eee9ddc9b112d4694642670d4a3cbe1e514  (rustty art)
//! images/gnome/128.png                   cdddfc86ca696ad65a39da85d856172bb326279e8bcce6561d1fff19e991418e  (rustty art)
//! images/gnome/256.png                   755aa39a43df0bfae9feb0e4a57b5e20694b2eeaeb6b9ef75b53154e9c01ccaf  (rustty art)
//! images/gnome/512.png                   55970c4441f0196b57ce4bd4d0d0acbd83d463a8fff5fdd9ea01a35d21a30c40  (rustty art)
//! images/gnome/1024.png                  a4597f2fe758e643533b3f55cc144bc872ba1a48cc37f4171370b1a33b6ee0ee  (rustty art)
//! images/gnome/2048.png                  2e753310f80e7b0ce75f260411c29d28445740c7622a5962020cfd34703e3fc1  (rustty art)
//! ```

#![allow(clippy::doc_markdown)]

use super::build_info;

/// The path (relative to the crate) to the Blueprint files, laid out as
/// `{version}/{name}.blp`. Reference: `gresource.zig:15` (`ui_path`); rustty
/// vendors the tree under `assets/ui` rather than `src/apprt/gtk/ui`.
pub const UI_PATH: &str = "assets/ui";

/// The path (relative to the crate) to the CSS files. Reference:
/// `gresource.zig:18` (`css_path`).
pub const CSS_PATH: &str = "assets/css";

/// The icon sizes embedded into the gresource file (each yields a 1x `{size}`
/// and a 2x `{size*2}` source). Reference: `gresource.zig:24` (`icon_sizes`).
pub const ICON_SIZES: &[u32] = &[16, 32, 128, 256, 512, 1024];

/// CSS files in [`CSS_PATH`]. Reference: `gresource.zig:58-63` (`css`).
pub const CSS: &[&str; 4] = &[
    "style.css",
    "style-dark.css",
    "style-hc.css",
    "style-hc-dark.css",
];

/// A blueprint definition: the minimum adwaita `{major}.{minor}` and the file
/// stem. Reference: `gresource.zig:65-69` (`Blueprint`). The `path` field is a
/// rustty addition — the const-folded resource path the reference computes on
/// demand in [`blueprint`] via `comptimePrint` (`gresource.zig:112-117`); storing
/// it lets [`blueprint`] stay a `const fn` returning `&'static str`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blueprint {
    /// Minimum adwaita major version.
    pub major: u16,
    /// Minimum adwaita minor version.
    pub minor: u16,
    /// File stem (no extension).
    pub name: &'static str,
    /// Precomputed `{resource_path}/ui/{major}.{minor}/{name}.ui`.
    path: &'static str,
}

/// Compile-time concatenation of [`build_info::RESOURCE_PATH`] and a suffix.
/// `N` must equal `RESOURCE_PATH.len() + suffix.len()`.
const fn resource_join<const N: usize>(suffix: &str) -> [u8; N] {
    let rp = build_info::RESOURCE_PATH.as_bytes();
    let sb = suffix.as_bytes();
    let mut out = [0u8; N];
    let mut i = 0;
    while i < rp.len() {
        out[i] = rp[i];
        i += 1;
    }
    let mut j = 0;
    while j < sb.len() {
        out[i + j] = sb[j];
        j += 1;
    }
    out
}

/// Define the manifest, const-folding each entry's resource path from
/// [`build_info::RESOURCE_PATH`] + a `concat!`-built literal suffix (so the
/// suffix cannot drift from the `major`/`minor`/`name` it is built from).
macro_rules! blueprints {
    ( $( ($maj:literal, $min:literal, $name:literal) ),+ $(,)? ) => {
        /// The blueprint files embedded into the gresource file. Reference:
        /// `gresource.zig:33-55`, minus the two AD-3-dropped legacy entries —
        /// **19**, not 21. Asserted to exist on disk by `tests/gtk_assets.rs`.
        pub const BLUEPRINTS: &[Blueprint; 19] = &[
            $(
                Blueprint {
                    major: $maj,
                    minor: $min,
                    name: $name,
                    path: {
                        const SUFFIX: &str =
                            concat!("/ui/", stringify!($maj), ".", stringify!($min), "/", $name, ".ui");
                        const BUF: [u8; build_info::RESOURCE_PATH.len() + SUFFIX.len()] =
                            resource_join(SUFFIX);
                        match core::str::from_utf8(&BUF) {
                            Ok(s) => s,
                            Err(_) => panic!("blueprint path is not valid utf-8"),
                        }
                    },
                },
            )+
        ];
    };
}

blueprints! {
    // AD-3: `(1, 0, "clipboard-confirmation-dialog")` dropped — `1.4` kept below.
    (1, 4, "clipboard-confirmation-dialog"),
    (1, 2, "close-confirmation-dialog"),
    (1, 2, "config-errors-dialog"),
    // AD-3: `(1, 2, "debug-warning")` dropped — `1.3` kept below.
    (1, 3, "debug-warning"),
    (1, 5, "imgui-widget"),
    (1, 5, "inspector-widget"),
    (1, 5, "inspector-window"),
    (1, 2, "resize-overlay"),
    (1, 2, "search-overlay"),
    (1, 2, "key-state-overlay"),
    (1, 5, "split-tree"),
    (1, 5, "split-tree-split"),
    (1, 2, "surface"),
    (1, 5, "surface-scrolled-window"),
    (1, 3, "surface-child-exited"),
    (1, 5, "tab"),
    (1, 5, "title-dialog"),
    (1, 5, "window"),
    (1, 5, "command-palette"),
}

/// Const `&str` equality (std's `PartialEq` for `str` is not `const`).
const fn str_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Returns the resource path (`{resource_path}/ui/{major}.{minor}/{name}.ui`) for
/// the given blueprint. Reference: `gresource.zig:103-123`.
///
/// This is `const`: calling it with a blueprint not in [`BLUEPRINTS`] from a
/// `const` context is a *compile* error (the `panic!` fires at const-eval),
/// faithful to the reference's `@compileError("invalid blueprint")`.
///
/// # Panics
/// Panics if `(major, minor, name)` is not a manifest entry.
#[must_use]
pub const fn blueprint(major: u16, minor: u16, name: &str) -> &'static str {
    let mut i = 0;
    while i < BLUEPRINTS.len() {
        let bp = &BLUEPRINTS[i];
        if bp.major == major && bp.minor == minor && str_eq(bp.name, name) {
            return bp.path;
        }
        i += 1;
    }
    panic!("invalid blueprint")
}

// Compile-time proof that the resolver folds at const-eval (mirrors the
// reference's comptime call sites) — a bad name here would fail to compile.
const _: &str = blueprint(1, 5, "window");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_nineteen() {
        assert_eq!(BLUEPRINTS.len(), 19);
    }

    #[test]
    fn dropped_legacy_entries_absent() {
        // AD-3 drops exactly these two; their kept siblings remain.
        for bp in BLUEPRINTS {
            assert!(
                !(bp.major == 1 && bp.minor == 0 && bp.name == "clipboard-confirmation-dialog"),
                "1.0 clipboard dialog must be dropped"
            );
            assert!(
                !(bp.major == 1 && bp.minor == 2 && bp.name == "debug-warning"),
                "1.2 debug-warning must be dropped"
            );
        }
        assert!(!blueprint(1, 4, "clipboard-confirmation-dialog").is_empty());
        assert!(!blueprint(1, 3, "debug-warning").is_empty());
    }

    #[test]
    fn resolver_matches_resource_path() {
        for bp in BLUEPRINTS {
            let expected = format!(
                "{}/ui/{}.{}/{}.ui",
                build_info::RESOURCE_PATH,
                bp.major,
                bp.minor,
                bp.name
            );
            assert_eq!(blueprint(bp.major, bp.minor, bp.name), expected);
        }
    }

    #[test]
    #[should_panic(expected = "invalid blueprint")]
    fn resolver_rejects_unknown() {
        // Runtime call of the const fn: an unknown name panics (the same
        // condition that is a compile error at const-eval).
        let _ = blueprint(9, 9, "does-not-exist");
    }

    #[test]
    fn css_and_icon_inventory() {
        assert_eq!(CSS.len(), 4);
        assert_eq!(ICON_SIZES, &[16u32, 32, 128, 256, 512, 1024]);
    }
}
