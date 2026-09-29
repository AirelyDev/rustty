use crate::generated::props_data;
use crate::lut::TablesRef;
use crate::props::Properties;

/// The lookup tables for Ghostty.
// PERF(port): Zig resolves this table reference at comptime; a non-generic
// cross-crate fn is un-inlinable without LTO — #[inline] restores the
// zero-cost access idiom.
// PERF(port): the const-generic sizes carry the generated arrays' fixed
// lengths ([u16; 8192] / [u16; 32000] / [Properties; 31]) into the type so
// `TablesRef::get` can elide its bounds checks, matching the reference's
// unchecked ReleaseFast indexing.
#[inline]
pub const fn table() -> TablesRef<
    'static,
    Properties,
    { props_data::STAGE1.len() },
    { props_data::STAGE2.len() },
    { props_data::STAGE3.len() },
> {
    // This is only available after running a generator as part of the Ghostty
    // build.zig process, but due to Zig's lazy analysis we can still reference
    // it here.
    //
    // An example process is the `main` in `props_uucode.zig`
    //
    // PARITY: rustty embeds the generator's output (R1: comptime artifacts
    // reproduced bit-for-bit) in `generated::props_data` instead of importing
    // an `unicode_tables` module produced by the build system.
    TablesRef {
        stage1: &props_data::STAGE1,
        stage2: &props_data::STAGE2,
        stage3: &props_data::STAGE3,
    }
}
