//! rustty-config::help_strings
//!
//! PORTS: the `help_strings.Config` namespace of ghostty's build-generated
//! `help_strings` module (src/build/HelpStrings.zig → src/helpgen.zig).
//!
//! PORT STATUS: done
//
// The table itself (`config_help`) is generated at build time by `build.rs`
// (using the `rustty-helpgen` extraction primitives) from every `Config` field's
// `///` doc block in `config_struct.rs`, then `include!`d here — the faithful
// analog of `HelpStrings.zig` running `helpgen` and importing its output. It is
// regenerated from the `///` source on every build, so the table can never drift
// from the ported doc comments (R3).

include!(concat!(env!("OUT_DIR"), "/config_help.rs"));
