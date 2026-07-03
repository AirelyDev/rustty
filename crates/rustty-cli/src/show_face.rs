//! rustty-cli::show_face
//!
//! PORTS: ghostty src/cli/show_face.zig.
//!
//! PORT STATUS: sequencing stub (C1). The full port lands in its
//! CLI_BLUEPRINT.md §7 chunk; this faithful sequencing stub carries the
//! reference `run` signature so the C1 `ghostty::Action::run_main` dispatcher
//! compiles. 0 tests; NOT part of the C1 test contract (its comment coverage +
//! MAP entry arrive with the real port).

use crate::ghostty::Error;

// pub fn run(alloc: std.mem.Allocator) !u8
#[allow(dead_code)]
pub fn run() -> Result<u8, Error> {
    unimplemented!("cli/show_face.zig — ported in a later CLI_BLUEPRINT.md §7 chunk")
}
