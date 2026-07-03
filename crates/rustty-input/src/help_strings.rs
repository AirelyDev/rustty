//! rustty-input::help_strings
//!
//! PORTS: the `help_strings.KeybindAction` namespace of ghostty's build-generated
//! `help_strings` module (src/build/HelpStrings.zig → src/helpgen.zig).
//!
//! PORT STATUS: done
//
// The table (`keybind_help`) is generated at build time by `build.rs` (using the
// `rustty-helpgen` extraction primitives) from every `Action` variant's `///`
// doc block in `binding.rs`, then `include!`d here. Regenerated from the `///`
// source on every build, so it can never drift from the ported docs (R3).

include!(concat!(env!("OUT_DIR"), "/keybind_help.rs"));
