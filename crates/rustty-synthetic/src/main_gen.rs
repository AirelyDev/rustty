//! PORT NOTE: `src/main_gen.zig` — `pub const main = synthetic.cli.main;`.
//! The binary entry for the synthetic generator CLI.

// const synthetic = @import("synthetic/main.zig");
// pub const main = synthetic.cli.main;
//
// Returning `Result<(), Error>` (with `Error: Debug`) lets a non-`Ok` outcome
// (`NoAction`, `InvalidAction`, a parse error) exit non-zero with a diagnostic —
// matching Zig `main() !void` propagating the error to the runtime.
fn main() -> Result<(), rustty_synthetic::cli::Error> {
    rustty_synthetic::cli::main()
}
