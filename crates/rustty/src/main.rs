//! rustty — the binary entry point.
//!
//! PORTS: ghostty src/main.zig, main_ghostty.zig, main_c.zig, main_gen.zig,
//! main_wasm.zig, main_build_data.zig, lib_vt.zig (the entry points fan out only).
//! Produces the `rustty` binary; --version/XTVERSION/DA report rustty (R2).
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1).

fn main() {
    // Stub entry point. Real dispatch (cli/apprt/core) lands as the upper
    // tiers are ported, leaves-first, under the acyclicity gate (R4).
}
