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
