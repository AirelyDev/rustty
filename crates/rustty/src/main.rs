//! rustty — the binary entry point.
//!
//! PORTS: ghostty src/main.zig — the entrypoint SELECTOR. The reference switches
//! `build_config.exe_entrypoint` to pick which module supplies `main`/`std_options`:
//!
//! ```zig
//! const entrypoint = switch (build_config.exe_entrypoint) {
//!     .ghostty => @import("main_ghostty.zig"),
//!     .helpgen => @import("helpgen.zig"),
//!     .mdgen_ghostty_1 => @import("build/mdgen/main_ghostty_1.zig"),
//!     .mdgen_ghostty_5 => @import("build/mdgen/main_ghostty_5.zig"),
//!     .webgen_config => @import("build/webgen/main_config.zig"),
//!     .webgen_actions => @import("build/webgen/main_actions.zig"),
//!     .webgen_commands => @import("build/webgen/main_commands.zig"),
//! };
//! ```
//!
//! For the `rustty` binary this milestone targets, `exe_entrypoint == .ghostty`,
//! so the entrypoint is [`main_rustty`] (ported below). The helpgen/mdgen/webgen
//! entrypoints are the doc/completion codegen tools — separate 0-test build
//! artifacts (DEFERRALS.md "rustty bin C6"), not part of the `rustty` exe.
//!
//! PORT STATUS: C6 (CLI_BLUEPRINT.md §5/§6). Produces the `rustty` binary; the
//! headless CLI actions (`+version`, `+list-actions`, `+validate-config`,
//! `+ssh-cache`, …) run for real. `--version`/XTVERSION/DA report rustty (R2).
//!
//! `main_c.zig` (the libghostty C-API hub, 3t) is re-homed to `rustty-apprt-c`
//! and `lib_vt.zig` (the librustty-vt entry, 1t) to `rustty-vt-c` (ADV-FIX 6);
//! `main_gen.zig`/`main_wasm.zig`/`main_build_data.zig` (0t gen/wasm artifacts,
//! not on the `rustty` exe's switch above) are deferred (DEFERRALS.md).

// /// See build_config.ExeEntrypoint for why we do this.
// const entrypoint = switch (build_config.exe_entrypoint) {
//     .ghostty => @import("main_ghostty.zig"), ...
// };
mod main_rustty;

/// The main entrypoint for the program.
// pub const main = entrypoint.main;
//
// PORT NOTE: the reference re-exports `entrypoint.main` as this file's `main`.
// In Rust the bin's `fn main` calls the selected entrypoint's `main`
// ([`main_rustty::main`]) directly. The `.ghostty` arm is the only one the
// `rustty` exe uses.
fn main() {
    main_rustty::main();
}

// /// Standard options such as logger overrides.
// pub const std_options: std.Options = if (@hasDecl(entrypoint, "std_options"))
//     entrypoint.std_options
// else
//     .{};
//
// PORT NOTE: the reference forwards the entrypoint's `std_options` (log level +
// `logFn`) as the program-wide `std.Options`. `main_rustty` DOES declare
// `std_options`, so the `@hasDecl` picks its arm. Rust has no `std.Options`
// global; the faithful projection lives with the log setup in
// `main_rustty` (`std_options` + `log_fn` — a deferred log-facade seam,
// consistent with the rest of the tree; see there).

#[cfg(test)]
mod tests {
    // test {
    //     _ = entrypoint;
    // }
    //
    // PORT NOTE: the reference's anonymous `test { _ = entrypoint; }` forces the
    // selected entrypoint to be semantically analyzed. The Rust analogue references
    // the `main_rustty` module surface (a wrong path fails to compile). Anonymous
    // `test {}` — skipped by check_test_counts.py's zig counter (zig=0); ported for
    // R1 completeness (rust=1 >= 0).
    #[test]
    fn ref_entrypoint() {
        // Touch the entrypoint's `main` so the module is analyzed.
        let _ = super::main_rustty::main as fn();
    }
}
