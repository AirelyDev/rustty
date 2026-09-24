//! rustty-args
//!
//! PORTS: ghostty src/cli/args.zig, cli/diagnostics.zig, cli/CommaSplitter.zig — CLI arg parsing
//! (plus Zig std's `std.process.Args.IteratorGeneral(.{})`, in `arg_iterator_general`)
//! base extracted below config and cli (break #7, breaks config<->cli).
//!
//! PORT STATUS: ported (Stage C). args.zig (36t) + CommaSplitter.zig (28t) +
//! diagnostics.zig (0t, required by args.zig) ported assertion-for-assertion.

pub mod arg_iterator_general;
pub mod args;
pub mod comma_splitter;
pub mod diagnostics;

// TEST-ONLY `log::Log` capture backend for the restored `log.*` sites
// (task-14e-2, S67-F2/S67-F9). See the module header.
#[cfg(test)]
pub(crate) mod log_capture;
