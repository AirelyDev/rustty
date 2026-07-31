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
//! Upstream (`20c3eae04`) later prefixed it with an artifact-root wiring block:
//!
//! ```zig
//! comptime {
//!     // Force-reference our memset override so its export is emitted.
//!     // See quirks_memset.zig for details on why this exists.
//!     _ = @import("quirks_memset.zig");
//! }
//! ```
//!
//! NON-PORT (`.port/phase-3/NON_PORTS.md` §E34): `quirks_memset.zig` is a
//! Zig-toolchain workaround with no Rust counterpart — Zig 0.16 globally
//! disabled LLVM loop auto-vectorization, which made compiler_rt's byte-loop
//! `memset` ~24× slower, so upstream ships its own. `rustc` lowers
//! `@llvm.memset` to the **platform libc** `memset` (Apple's `libSystem` /
//! glibc's IFUNC dispatch), so there is no override to force-reference and
//! exporting one would only pessimize rustty. The wiring block is therefore
//! deliberately absent, not dropped.
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
