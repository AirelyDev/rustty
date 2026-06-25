//! rustty-args
//!
//! PORTS: ghostty src/cli/args.zig, cli/diagnostics.zig, cli/CommaSplitter.zig — CLI arg parsing
//! base extracted below config and cli (break #7, breaks config<->cli).
//!
//! PORT STATUS: ported (Stage C). args.zig (36t) + CommaSplitter.zig (28t) +
//! diagnostics.zig (0t, required by args.zig) ported assertion-for-assertion.

pub mod args;
pub mod comma_splitter;
pub mod diagnostics;
