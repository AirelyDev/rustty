//! Nerd Font glyph constraints.
//!
//! `table` is a 3-level codepoint lookup, same shape as
//! `unicode/symbols_table.zig`. Pages with no constrained glyphs share
//! one stage2 block and return null after the stage1 load.
//
// PORT NOTE: ghostty src/font/nerd_font_attributes.zig. Until a3f3331d4 this
// file was the generated `switch (cp)` itself; the table data now lives in
// `generated::nerd_font_tables` (the port of `nerd_font_tables.zig`), and this
// module keeps the lookup, the comptime layout checks and the test.

// const std = @import("std");
// const lut = @import("../unicode/lut.zig");
// const Constraint = @import("Glyph.zig").RenderOptions.Constraint;
// const generated = @import("nerd_font_tables.zig").Tables(?Constraint);
//
// PORT NOTE: `lut.zig` is `rustty_unicode::lut` (rustty-unicode is a leaf, so
// the rustty-font -> rustty-unicode edge keeps the crate graph acyclic).
// `RenderOptions.Constraint` maps to `crate::glyph::Constraint`.
use crate::generated::nerd_font_tables as generated;
use crate::glyph::Constraint;
use rustty_unicode::lut::TablesRef;

/// 3-level codepoint lookup. Same shape as `unicode/symbols_table.zig`.
// pub const table: lut.Tables(?Constraint) = .{
//     .stage1 = &generated.stage1,
//     .stage2 = &generated.stage2,
//     .stage3 = &generated.stage3,
// };
//
// PERF(port): the same idiom as `rustty_unicode::symbols_table::table`: an
// `#[inline] const fn` returning a `TablesRef` whose const-generic sizes carry
// the generated arrays' fixed lengths ([u16; 8192] / [u16; 6400] /
// [?Constraint; 277]) into the type, so the reference's comptime-known table
// needs no runtime indirection and `TablesRef::get` can drop its bounds checks.
#[inline]
pub const fn table() -> TablesRef<
    'static,
    Option<Constraint>,
    { generated::STAGE1.len() },
    { generated::STAGE2.len() },
    { generated::STAGE3.len() },
> {
    TablesRef {
        stage1: &generated::STAGE1,
        stage2: &generated::STAGE2,
        stage3: &generated::STAGE3,
    }
}

// lut.zig pages are 256 codepoints. The generator stores the all-null
// page at stage2 offset 0, and stage3 index 0 is null.
const EMPTY_BLOCK: u16 = 0;

// comptime {
const _: () = {
    // One stage1 entry for every high byte of a u21.
    // if (generated.stage1.len != 0x2000) @compileError("stage1 must cover every u21 page");
    assert!(
        generated::STAGE1.len() == 0x2000,
        "stage1 must cover every u21 page"
    );
    // if (generated.stage3.len == 0 or generated.stage3[0] != null) {
    //     @compileError("stage3[0] must be null");
    // }
    assert!(
        !generated::STAGE3.is_empty() && generated::STAGE3[0].is_none(),
        "stage3[0] must be null"
    );
    // for (generated.stage2[0..256]) |entry| {
    //     if (entry != 0) @compileError("empty nerd-font page must be stage2 offset 0");
    // }
    let mut i = 0;
    while i < 256 {
        assert!(
            generated::STAGE2[i] == 0,
            "empty nerd-font page must be stage2 offset 0"
        );
        i += 1;
    }

    // PORT NOTE: not in the reference. `TablesRef::get` elides its stage2 and
    // stage3 bounds checks under the lut generator invariant (every stage1
    // entry + 255 indexes within stage2; every stage2 entry indexes within
    // stage3). rustty-unicode machine-checks that invariant for its own tables
    // in a unit test; this table is checked here, at compile time, so a
    // mis-transcribed entry can never reach the unchecked reads.
    let mut i = 0;
    while i < generated::STAGE1.len() {
        assert!(
            (generated::STAGE1[i] as usize) + 0xFF < generated::STAGE2.len(),
            "stage1 entry + 255 must index within stage2"
        );
        i += 1;
    }
    let mut i = 0;
    while i < generated::STAGE2.len() {
        assert!(
            (generated::STAGE2[i] as usize) < generated::STAGE3.len(),
            "stage2 entry must index within stage3"
        );
        i += 1;
    }
};
// }

/// Nerd Font constraint for `cp`, or null when the codepoint has none.
/// Pages with no constrained glyphs return after the stage1 load.
// pub inline fn getConstraint(cp: u21) ?Constraint {
//
// PORT NOTE: the reference takes `cp: u21`; Rust has no `u21`, so `cp` is a
// `u32` masked back to the u21 domain (a no-op for every value the reference
// can express), exactly as `TablesRef::get` masks it. Zig's `inline fn` is
// `#[inline]` so the lookup inlines across crates into the renderer.
#[inline]
pub fn get_constraint(cp: u32) -> Option<Constraint> {
    let cp = cp & 0x1F_FFFF;
    // if (table.stage1[cp >> 8] == empty_block) return null;
    if table().stage1[(cp >> 8) as usize] == EMPTY_BLOCK {
        return None;
    }
    // return table.get(cp);
    table().get(cp)
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "Constraints lookup table" {
    #[test]
    fn constraints_lookup_table() {
        // const testing = std.testing;
        assert!(get_constraint(u32::from('A')).is_none());
        assert!(get_constraint(0x30A2).is_none()); // katakana
        assert!(get_constraint(0xFF66).is_none()); // halfwidth katakana
        assert!(get_constraint(0x2630).is_some());
        assert!(get_constraint(0x110000).is_none());
        // try testing.expect(getConstraint(std.math.maxInt(u21)) == null);
        assert!(get_constraint(0x1F_FFFF).is_none());

        let table = table();

        // PORT NOTE (Miri): no iteration cap. The walk skips every empty
        // 256-codepoint page after one stage1 load, so it makes 4,302 page
        // skips plus 12,800 per-codepoint steps (50 non-empty pages), not
        // 1.1M; the full walk
        // takes about 9 s under Miri (measured 2026-09-28, P7-W8 T1), so
        // Miri runs it in full like every other build.
        let mut cp: u32 = 0;
        while cp <= 0x10FFFF {
            // const scalar: u21 = @intCast(cp);
            let scalar: u32 = cp;
            if table.stage1[(scalar >> 8) as usize] == EMPTY_BLOCK {
                assert!(get_constraint(scalar).is_none());
                cp = (cp & !0xFF_u32) + 256;
                continue;
            }
            let full = table.get(scalar);
            let fast = get_constraint(scalar);
            if let Some(value) = full {
                assert_eq!(value, fast.unwrap());
            } else {
                assert!(fast.is_none());
            }
            cp += 1;
        }
    }
}
