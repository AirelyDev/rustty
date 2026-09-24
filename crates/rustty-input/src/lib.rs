//! rustty-input
//!
//! PORTS: ghostty src/input/ minus the VT slice owned by rustty-vt. (Ownership
//! doc corrected 2026-07-10, VALIDATION_REPORT §3b — the old "5-file VT slice"
//! claim under-counted it by half.) The rustty-vt slice
//! (`crates/rustty-vt/src/input/`, 10 Rust modules incl. the aggregator `mod.rs`)
//! owns 9 reference files: key.zig, key_encode.zig, mouse.zig, mouse_encode.zig,
//! paste.zig, function_keys.zig, kitty.zig, plus the VT halves of config.zig
//! (`OptionAsAlt`) and key_mods.zig (the VT-Mods half). THIS crate owns the
//! rest: input.zig (partially carved — see below), Binding, KeymapDarwin,
//! KeymapNoop, Link, command, keyboard, keycodes, helpgen_actions, and the
//! config-dependent halves of config.zig / key_mods.zig.
//!
//! PORT STATUS: stub
//!
//! Carved incrementally: `Binding.zig` is FULLY ported in `binding.rs` (PORT
//! STATUS: done — every line/comment/test{}, all 85 tests, R1). The input
//! leaves `KeymapDarwin`/`command`/`keycodes`/`Link`/`KeymapNoop`/`keyboard`/
//! `helpgen_actions`/`config` are FULLY ported in the modules below. The
//! config-dependent slice of `key_mods.zig` (the `RemapSet` + `Mods.translation`
//! that the `rustty-vt` partial port deferred behind `config.OptionAsAlt` + the
//! config `formatter`) is ported in `key_mods.rs` now that both seams exist
//! (`config::OptionAsAlt` + `rustty-cfg-fmt`); the VT-Mods half stays in
//! `rustty-vt`. The crate-level status stays `stub` because the crate's
//! remaining mapped reference file (input.zig) is not yet carved (R1: every
//! line/comment/test{} ported as the crate is carved, leaves-first, under the
//! acyclicity gate per R4).

pub mod binding;
pub mod command;
pub mod config;
pub mod help_strings;
pub mod helpgen_actions;
pub mod key_mods;
pub mod keyboard;
pub mod keycodes;
pub mod keymap_darwin;
pub mod keymap_noop;
pub mod link;

// PORT NOTE: `input.zig:39-44` —
//   // Keymap is only available on macOS right now. We could implement it
//   // in theory for XKB too on Linux but we don't need it right now.
//   pub const Keymap = switch (builtin.os.tag) {
//       .macos => @import("input/KeymapDarwin.zig"),
//       else => @import("input/KeymapNoop.zig"),
//   };
// (CORRECTION, parity S43-F7: the quote's second line used to read "but we
// don't as of writing this.", which is not the pinned reference text.)
// The platform-selected keymap type (`input.Keymap`). macOS uses the
// Carbon-backed [`keymap_darwin::Keymap`]; every other target uses the
// [`keymap_noop::KeymapNoop`]. This single `input.zig` public decl is ported here
// at the input-package surface ahead of the full `input.zig` carve because the
// first `input.Keymap` consumer — the `apprt/embedded` `App.keymap` field
// (embedded.zig:122) — has landed (same rationale as the `ctrlOrSuper` re-export
// above).
#[cfg(target_os = "macos")]
pub use keymap_darwin::Keymap;
#[cfg(not(target_os = "macos"))]
pub use keymap_noop::KeymapNoop as Keymap;

// PORT NOTE: `input.zig:18` — `pub const ctrlOrSuper = key.ctrlOrSuper;`. The
// `input.zig`/`key.zig` files are not yet carved into this crate; this single
// re-exported public decl (the `Mods`-valued `ctrlOrSuper`, key.zig:862) is the
// only piece the `config.Keybinds` value type needs, so it is ported here at the
// input-package public surface ahead of the full `key.zig` carve. `Mods` is the
// `rustty-vt` `key_mods.Mods` (the same type `input.Mods` aliases).
use rustty_vt::input::key_mods::Mods;

/// This sets either "ctrl" or "super" to true (but not both)
/// on mods depending on if the build target is Mac or not. On
/// Mac, we default to super (i.e. super+c for copy) and on
/// non-Mac we default to ctrl (i.e. ctrl+c for copy).
// pub fn ctrlOrSuper(mods: Mods) Mods {
pub fn ctrl_or_super(mods: Mods) -> Mods {
    // var copy = mods;
    let mut copy = mods;
    // if (comptime builtin.target.os.tag.isDarwin()) {
    if cfg!(target_vendor = "apple") {
        // copy.super = true;
        copy.super_ = true;
    } else {
        // copy.ctrl = true;
        copy.ctrl = true;
    }

    // return copy;
    copy
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "ctrlOrSuper"
    #[test]
    fn ctrl_or_super_test() {
        // var m: Mods = ctrlOrSuper(.{});
        let m: Mods = ctrl_or_super(Mods::default());

        // try testing.expect(m.ctrlOrSuper());
        assert!(m.ctrl_or_super());
    }
}
