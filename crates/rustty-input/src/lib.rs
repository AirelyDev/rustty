//! rustty-input
//!
//! PORTS: ghostty src/input/ minus the 5-file VT slice owned by rustty-vt (input.zig, Binding,
//! KeymapDarwin, KeymapNoop, Link, command, config, function_keys, helpgen_actions,
//! key_mods, keyboard, keycodes, kitty).
//!
//! PORT STATUS: stub
//!
//! Carved incrementally: `Binding.zig` is FULLY ported in `binding.rs` (PORT
//! STATUS: done — every line/comment/test{}, all 85 tests, R1). The input
//! leaves `KeymapDarwin`/`command`/`keycodes`/`Link`/`KeymapNoop`/`keyboard`/
//! `helpgen_actions`/`config` are FULLY ported in the modules below. The
//! crate-level status stays `stub` because the crate's remaining mapped
//! reference file (input.zig) is not yet carved (R1: every line/comment/test{}
//! ported as the crate is carved, leaves-first, under the acyclicity gate per R4).

pub mod binding;
pub mod command;
pub mod config;
pub mod helpgen_actions;
pub mod keyboard;
pub mod keycodes;
pub mod keymap_darwin;
pub mod keymap_noop;
pub mod link;
