//! rustty-cli::help_strings
//!
//! PORTS: the `help_strings.Action` namespace of ghostty's build-generated
//! `help_strings` module (src/build/HelpStrings.zig → src/helpgen.zig).
//!
//! PORT STATUS: done
//
// The table (`action_help`) is generated at build time by `build.rs` (using the
// `rustty-helpgen` extraction primitives) from each cli action module's
// `pub fn run` `///` doc block, then `include!`d here. Regenerated from the `///`
// source on every build, so it can never drift from the ported docs (R3).

include!(concat!(env!("OUT_DIR"), "/action_help.rs"));
