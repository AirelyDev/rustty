//! rustty-bench binary entry — faithful port of ghostty `src/main_bench.zig`.
//!
//! The reference file is exactly two decls (a re-export hub for the benchmark
//! CLI's `main`):
//!
//! ```zig
//! const benchmark = @import("benchmark/main.zig");
//!
//! pub const main = benchmark.cli.main;
//! ```
//!
//! In Rust, `benchmark/main.zig` is this crate's library root (`rustty_bench`),
//! and `benchmark.cli.main` is `rustty_bench::cli::main`. A Zig `pub const main
//! = ...` aliases the fn as the program entry point; Rust has no fn-alias entry
//! point, so this is a thin `fn main` wrapper that calls it.
//!
//! `cli::Error` is a large enum, so returning it from `fn main` would trip
//! `clippy::result_large_err` (workspace lints deny warnings). Instead we match
//! and map to an `ExitCode`, printing the error to stderr — the process-exit
//! behaviour Zig's `!void` main achieves via its error return.

use std::process::ExitCode;

fn main() -> ExitCode {
    match rustty_bench::cli::main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:?}");
            ExitCode::FAILURE
        }
    }
}
