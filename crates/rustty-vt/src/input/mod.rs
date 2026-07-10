//! rustty-vt input
//!
//! PORTS: ghostty src/input/{key,key_encode,mouse,mouse_encode,paste,
//! function_keys,kitty}.zig in full, plus the VT halves of config.zig
//! (`OptionAsAlt`) and key_mods.zig (the VT-Mods half) — the 9-reference-file /
//! 10-Rust-module VT input slice. (Ownership doc corrected 2026-07-10,
//! VALIDATION_REPORT §3b: the old one-line list omitted key_mods.zig.) The
//! config-dependent halves of config.zig/key_mods.zig (`Mods.translation`,
//! `RemapSet`) + the rest of src/input/ live in `rustty-input` (see its lib.rs).
//!
//! PORT STATUS: ported (with the crate — vt is done; the module's one live seam
//! is `key.rs` `imgui_key`, the dcimgui apprt-GUI binding leaf behind the
//! off-by-default `imgui` feature — DEFERRALS.md "Deferred from vt-14")

pub mod config;
pub mod function_keys;
pub mod key;
pub mod key_encode;
pub mod key_mods;
pub mod kitty;
pub mod mouse;
pub mod mouse_encode;
pub mod paste;
