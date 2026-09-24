//! rustty-input
//!
//! PORTS: ghostty src/input/ minus the VT slice owned by rustty-vt. (Ownership
//! doc corrected 2026-07-10, VALIDATION_REPORT §3b — the old "5-file VT slice"
//! claim under-counted it by half.) The rustty-vt slice
//! (`crates/rustty-vt/src/input/`, 10 Rust modules incl. the aggregator `mod.rs`)
//! owns 9 reference files: key.zig, key_encode.zig, mouse.zig, mouse_encode.zig,
//! paste.zig, function_keys.zig, kitty.zig, plus the VT halves of config.zig
//! (`OptionAsAlt`) and key_mods.zig (the VT-Mods half). THIS crate owns the
//! rest: input.zig (the `input` namespace, carved below), Binding,
//! KeymapDarwin, KeymapNoop, Link, command, keyboard, keycodes,
//! helpgen_actions, and the config-dependent halves of config.zig /
//! key_mods.zig.
//!
//! PORT STATUS: done
//!
//! All 13 mapped reference files are ported (R1). `Binding.zig` is in
//! `binding.rs` (every line/comment/test{}, all 85 tests). The input leaves
//! `KeymapDarwin`/`command`/`keycodes`/`Link`/`KeymapNoop`/`keyboard`/
//! `helpgen_actions`/`config` are in the modules below. The config-dependent
//! slice of `key_mods.zig` (the `RemapSet` + `Mods.translation` that the
//! `rustty-vt` partial port deferred behind `config.OptionAsAlt` + the config
//! `formatter`) is in `key_mods.rs`; the VT-Mods half stays in `rustty-vt`.
//! `input.zig` (the namespace's re-export decls and its `refAllDecls` test) is
//! carved at the end of this file.
//!
//! Every gate examines the rustty-vt half too: `_gatelib.RELOCATIONS` names the
//! four rustty-vt files that port this crate's mapped files
//! (`function_keys.rs`, `kitty.rs`, `key_mods.rs`, `config.rs`), so
//! G1/G2/G3/G3b/D3/G9 scan them as part of this crate's port. (Status word
//! corrected from `stub` 2026-09-24, parity tail S-8a; `in progress` -> `done`
//! the same day, parity tail S-8c, once input.zig was carved and every gate's
//! default run was green for this crate.)

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

// ===========================================================================
// PORTS: src/input.zig — the `input` package namespace (its re-export decls and
// its `refAllDecls` test). The package spans two crates (R4): the VT slice of
// src/input/ is ported in `rustty_vt::input`, the rest in this crate, so each
// decl below names the crate its reference file was ported into.
// ===========================================================================

// const std = @import("std");
// const builtin = @import("builtin");
//
// const config = @import("input/config.zig");
// const mouse = @import("input/mouse.zig");
// const key = @import("input/key.zig");
// const key_mods = @import("input/key_mods.zig");
// const keyboard = @import("input/keyboard.zig");
//
// PORT NOTE: the five private imports are Rust paths below. `config` and
// `keyboard` are this crate's modules; `mouse` and `key` are
// `rustty_vt::input::{mouse, key}`; `key_mods` is split across both crates
// (`rustty_vt::input::key_mods` holds `Mods`, this crate's `key_mods` holds
// `RemapSet`).
use rustty_vt::input::{key, mouse};

// pub const command = @import("input/command.zig");
// (`pub mod command` above.)
// pub const function_keys = @import("input/function_keys.zig");
pub use rustty_vt::input::function_keys;
// pub const keycodes = @import("input/keycodes.zig");
// (`pub mod keycodes` above.)
// pub const key_encode = @import("input/key_encode.zig");
pub use rustty_vt::input::key_encode;
// pub const kitty = @import("input/kitty.zig");
pub use rustty_vt::input::kitty;
// pub const mouse_encode = @import("input/mouse_encode.zig");
pub use rustty_vt::input::mouse_encode;
// pub const paste = @import("input/paste.zig");
pub use rustty_vt::input::paste;

// pub const ctrlOrSuper = key.ctrlOrSuper;
//
// PORT NOTE (S-8c): this crate used to carry a second, line-for-line copy of
// key.zig's `ctrlOrSuper` (and of its `test "ctrlOrSuper"`) "ahead of the
// key.zig carve". key.zig is ported in rustty-vt (`key.rs` holds both the fn and
// the test), so the reference's re-export is a re-export here too.
pub use key::ctrl_or_super;
// pub const Action = key.Action;
pub use key::Action;
// pub const Binding = @import("input/Binding.zig");
pub use binding::Binding;
// pub const Command = command.Command;
pub use command::Command;
// pub const Link = @import("input/Link.zig");
pub use link::Link;
// pub const Key = key.Key;
pub use key::Key;
// pub const KeyboardLayout = keyboard.Layout;
pub use keyboard::Layout as KeyboardLayout;
// pub const KeyEvent = key.KeyEvent;
pub use key::KeyEvent;
// pub const KeyRemapSet = key_mods.RemapSet;
pub use key_mods::RemapSet as KeyRemapSet;
// pub const InspectorMode = Binding.Action.InspectorMode;
pub use binding::InspectorMode;
// pub const Mods = key_mods.Mods;
pub use rustty_vt::input::key_mods::Mods;
// pub const MouseAction = mouse.Action;
pub use mouse::Action as MouseAction;
// pub const MouseButton = mouse.Button;
pub use mouse::Button as MouseButton;
// pub const MouseButtonState = mouse.ButtonState;
pub use mouse::ButtonState as MouseButtonState;
// pub const MousePressureStage = mouse.PressureStage;
pub use mouse::PressureStage as MousePressureStage;
// pub const OptionAsAlt = config.OptionAsAlt;
//
// PORT NOTE: `config.OptionAsAlt` is this crate's `config::OptionAsAlt` (the
// type the `macos-option-as-alt` config field holds). rustty-vt keeps its own
// twin for the VT-Mods half of key_mods.zig.
pub use config::OptionAsAlt;
// pub const ScrollMods = mouse.ScrollMods;
pub use mouse::ScrollMods;
// pub const SplitFocusDirection = Binding.Action.SplitFocusDirection;
pub use binding::SplitFocusDirection;
// pub const SplitResizeDirection = Binding.Action.SplitResizeDirection;
pub use binding::SplitResizeDirection;
// pub const Trigger = Binding.Trigger;
pub use binding::Trigger;

// Keymap is only available on macOS right now. We could implement it
// in theory for XKB too on Linux but we don't need it right now.
// pub const Keymap = switch (builtin.os.tag) {
//     .macos => @import("input/KeymapDarwin.zig"),
//     else => @import("input/KeymapNoop.zig"),
// };
// (CORRECTION, parity S43-F7: the quote's second line used to read "but we
// don't as of writing this.", which is not the pinned reference text.)
// The platform-selected keymap type (`input.Keymap`). macOS uses the
// Carbon-backed [`keymap_darwin::Keymap`]; every other target uses the
// [`keymap_noop::KeymapNoop`].
#[cfg(target_os = "macos")]
pub use keymap_darwin::Keymap;
#[cfg(not(target_os = "macos"))]
pub use keymap_noop::KeymapNoop as Keymap;

// test {
//     std.testing.refAllDecls(@This());
// }
#[cfg(test)]
mod tests {
    // PORT NOTE: `refAllDecls` forces semantic analysis of every public decl of
    // the namespace. Rust type-checks every item at build time, so the analogue
    // (terminfo/inspector precedent) names each re-exported decl above, which
    // fails to compile if any of them stops resolving.
    #[test]
    fn ref_all_decls() {
        use std::any::type_name;
        let names = [
            type_name::<crate::Action>(),
            type_name::<crate::Binding>(),
            type_name::<crate::Command>(),
            type_name::<crate::Link>(),
            type_name::<crate::Key>(),
            type_name::<crate::KeyboardLayout>(),
            type_name::<crate::KeyEvent>(),
            type_name::<crate::KeyRemapSet>(),
            type_name::<crate::InspectorMode>(),
            type_name::<crate::Mods>(),
            type_name::<crate::MouseAction>(),
            type_name::<crate::MouseButton>(),
            type_name::<crate::MouseButtonState>(),
            type_name::<crate::MousePressureStage>(),
            type_name::<crate::OptionAsAlt>(),
            type_name::<crate::ScrollMods>(),
            type_name::<crate::SplitFocusDirection>(),
            type_name::<crate::SplitResizeDirection>(),
            type_name::<crate::Trigger>(),
            type_name::<crate::Keymap>(),
        ];
        let _: fn(crate::Mods) -> crate::Mods = crate::ctrl_or_super;
        // The namespace's module decls (`command`, `function_keys`, `keycodes`,
        // `key_encode`, `kitty`, `mouse_encode`, `paste`) resolve as paths.
        let _ = type_name::<crate::command::Command>();
        let _ = type_name::<crate::function_keys::Entry>();
        let _ = type_name::<crate::keycodes::Entry>();
        let _ = type_name::<crate::key_encode::Options>();
        let _ = type_name::<crate::kitty::Entry>();
        let _ = type_name::<crate::mouse_encode::Event>();
        let _ = type_name::<crate::paste::Options>();
        assert!(names.iter().all(|n| !n.is_empty()));
    }
}
