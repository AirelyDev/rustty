//! `P3-W9 T2` step 4 — NEGATIVE CONTROL, kept as a permanent gate.
//!
//! The `background-blur` (oracle `2da02f4d2`) and `keybind` `global:` (oracle
//! `d71695550`) doc blocks are USER-FACING text. They reach users through three
//! generated consumers: the `.5` man page (`mdgen`), the website config data
//! (`webgen`), and `+show-config --docs`. A doc edit that lands in
//! `config_struct.rs` but does not reach the generated artifacts is invisible —
//! and nothing previously asserted that path for any doc block.
//!
//! This asserts BOTH directions for BOTH rewrites:
//!   * the NEW oracle sentences are present in man(5) AND in the web MDX, and
//!   * the DELETED pin sentences are absent from both.
//!
//! The deletion half is what makes it non-vacuous: an "is the new text there"
//! check alone would pass on a file that still carried the old text as well.

/// Sentences oracle `2da02f4d2` / `d71695550` ADDED. R2: `Ghostty` -> `Rustty`.
const ADDED: &[&str] = &[
    // 2da02f4d2 — background-blur
    "ext-background-effect-v1",
    "On X11, blur can only be enabled when using the KWin compositor",
    // d71695550 — keybind global:
    "vicinae-hotkey-v1",
    "Since Rustty 1.4.0, global shortcuts on Linux are primarily supported",
    "Hyprland since version 0.56.0",
    "with `xdg-desktop-portal-kde` installed",
    "and COSMIC.",
];

/// Sentences those two commits DELETED. None may survive anywhere.
const DELETED: &[&str] = &[
    // 2da02f4d2 — background-blur
    "KDE Plasma (Wayland and X11)",
    "To configure KWin's global blur setting, open System Settings",
    "All other Linux desktop environments are as of now unsupported",
    // d71695550 — keybind global:
    "`global:` is only supported on macOS and certain Linux platforms",
    "should be able",
    "wiki.hyprland.org/Configuring/Binds",
];

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("generated artifact is UTF-8")
}

#[test]
// MIRI EXCEPTION (rustty-extra webgen/mdgen family): this drives BOTH
// `mdgen::man_5_markdown()` and `webgen::config_mdx()`, the two generators
// already registered as Miri-intractable in .port/phase-2/MIRI_EXCEPTIONS.md
// ("Family 4"): O(config fields x document length) substring scanning that is
// milliseconds natively and does not finish under the interpreter (measured
// again here: no completion after 30+ minutes in the ci.sh G6 lane, where it
// was the reason rustty-extra's Miri run never terminated). The module has no
// `unsafe`; the native run in ci.sh's `cargo test --workspace` is the control.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-extra webgen/mdgen): intractable \
     doc-generation scan — see .port/phase-2/MIRI_EXCEPTIONS.md Family 4"
)]
fn background_blur_and_keybind_doc_rewrites_reach_man5_and_webdata() {
    let man5 = text(rustty_extra::mdgen::man_5_markdown());
    let web = text(rustty_extra::webgen::config_mdx());

    // Non-vacuity: both artifacts must actually contain the two option blocks,
    // or "the deleted text is absent" would pass on an empty string.
    for (name, out) in [("man5", &man5), ("webdata", &web)] {
        assert!(
            out.contains("background-blur"),
            "{name} does not document background-blur at all"
        );
        assert!(
            out.contains("keybind"),
            "{name} does not document keybind at all"
        );
    }

    for needle in ADDED {
        for (name, out) in [("man5", &man5), ("webdata", &web)] {
            assert!(
                out.contains(needle),
                "{name} is missing the ported oracle sentence: {needle:?}"
            );
        }
    }

    for needle in DELETED {
        for (name, out) in [("man5", &man5), ("webdata", &web)] {
            assert!(
                !out.contains(needle),
                "{name} still carries a sentence the oracle DELETED: {needle:?}"
            );
        }
    }
}
