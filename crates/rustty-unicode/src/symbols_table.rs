use crate::generated::symbols_data;
use crate::lut::TablesRef;

/// The lookup tables for Ghostty.
pub const fn table() -> TablesRef<'static, bool> {
    // This is only available after running a generator as part of the Ghostty
    // build.zig process, but due to Zig's lazy analysis we can still reference
    // it here.
    //
    // An example process is the `main` in `symbols_uucode.zig`
    //
    // PARITY: rustty embeds the generator's output (R1: comptime artifacts
    // reproduced bit-for-bit) in `generated::symbols_data` instead of importing
    // a `symbols_tables` module produced by the build system.
    TablesRef {
        stage1: &symbols_data::STAGE1,
        stage2: &symbols_data::STAGE2,
        stage3: &symbols_data::STAGE3,
    }
}
