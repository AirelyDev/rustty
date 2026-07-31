//! rustty-unicode
//!
//! PORTS: ghostty src/unicode/ (grapheme, lut, main, props, props_table, props_uucode,
//! symbols_table, symbols_uucode) + generated tables (reproduced bit-for-bit per R1).
//!
//! PORT STATUS: done
//!
//! This crate is a leaf. It has NO dependency on the ghostty tree (R3); the
//! Unicode tables are reproduced bit-for-bit (R1) from the reference's own
//! `props-unigen`/`symbols-unigen` generators (built against uucode 0.2.0 with
//! ghostty's src/build/uucode_config.zig), and the grapheme-break precompute
//! table from uucode 0.2.0's `computeGraphemeBreakNoControl`. It is fully
//! Miri-able (R6).

mod generated;

pub mod grapheme;
pub mod lut;
pub mod props;
pub mod props_table;
pub mod props_uucode;
pub mod symbols_table;
pub mod symbols_uucode;

pub use grapheme::grapheme_break as graphemeBreak;
pub use props::Properties;
pub use props_table::table;

/// Returns the terminal display width of a codepoint in terminal
/// grid cells: 0, 1, or 2.
///
/// This is the same width table the terminal uses when laying out
/// printed text: 0 for zero-width codepoints (controls, combining
/// marks, default-ignorables, surrogates), 2 for wide codepoints
/// (East Asian Wide/Fullwidth, regional indicators, clamped at 2),
/// and 1 otherwise.
///
/// This operates on a single codepoint and cannot account for
/// grapheme-cluster-level width rules (VS16, combining sequences);
/// callers needing cluster-accurate widths should use [`grapheme::grapheme_width`].
/// Summing per-codepoint widths is only correct when mode 2027 is
/// disabled.
///
/// Callers must pass `cp <= 0x10FFFF` (the C wrapper guards larger values).
// pub fn codepointWidth(cp: u21) u2 { return table.get(cp).width; }
pub fn codepoint_width(cp: u32) -> u8 {
    table().get(cp).width
}

#[cfg(test)]
mod width_tests {
    use super::codepoint_width;

    // test "codepointWidth" (oracle fc5a72772)
    #[test]
    fn codepoint_width_table() {
        // Narrow (width 1)
        assert_eq!(1, codepoint_width('a' as u32));
        assert_eq!(1, codepoint_width(' ' as u32));
        assert_eq!(1, codepoint_width(0x10FFFF)); // max codepoint

        // C0/C1 control characters (width 0)
        assert_eq!(0, codepoint_width(0x00)); // NUL
        assert_eq!(0, codepoint_width(0x07)); // BEL
        assert_eq!(0, codepoint_width(0x1B)); // ESC
        assert_eq!(0, codepoint_width(0x7F)); // DEL
        assert_eq!(0, codepoint_width(0x80)); // C1 PAD

        // Zero-width codepoints
        assert_eq!(0, codepoint_width(0x0301)); // combining acute
        assert_eq!(0, codepoint_width(0x200B)); // zero width space
        assert_eq!(0, codepoint_width(0x200D)); // ZWJ
        assert_eq!(0, codepoint_width(0xFE0F)); // VS16
        assert_eq!(0, codepoint_width(0xD800)); // surrogate

        // Wide (width 2)
        assert_eq!(2, codepoint_width(0x4E00)); // CJK ideograph
        assert_eq!(2, codepoint_width(0xFF21)); // fullwidth A
        assert_eq!(2, codepoint_width(0xAC00)); // Hangul syllable
        assert_eq!(2, codepoint_width(0x1F600)); // emoji
        assert_eq!(2, codepoint_width(0x1F1E6)); // regional indicator
        assert_eq!(2, codepoint_width(0x2E3B)); // three-em dash (clamped)
    }
}

#[cfg(test)]
mod tests {
    // PARITY: the reference `main.zig` has
    // `test { @import("std").testing.refAllDecls(@This()); }` which forces
    // analysis of every public declaration. Rust analyzes/compiles all
    // referenced items as part of `cargo test`; this test touches the public
    // surface so it is exercised.
    #[test]
    fn ref_all_decls() {
        use crate::grapheme::BreakState;
        let mut state = BreakState::Default;
        let _ = crate::graphemeBreak(0x261D, 0x1F3FF, &mut state);
        let _ = crate::table().get(0x41);
        let _ = crate::Properties::default();
    }
}
