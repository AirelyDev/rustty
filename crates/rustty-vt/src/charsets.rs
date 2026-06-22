//! rustty-vt charsets
//!
//! PORTS: ghostty src/terminal/charsets.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const build_options = @import("terminal_options");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const LibEnum = @import("../lib/enum.zig").Enum;
// `LibEnum(if (build_options.c_abi) .c else .zig, &.{...})` for the `.zig`
// target is a plain enum with the keys in order; ported as Rust enums. `assert`
// is the `inlineAssert` quirk in rustty-util.

// PORT NOTE: the reference `assert` is `quirks.inlineAssert`, but that is a
// non-`const` fn and `init_table` is a `const fn` (the reference `initTable`
// runs at comptime). Inside the const context we use the builtin `assert!`
// macro, which is const-callable and carries identical semantics.

/// The available charset slots for a terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slots {
    G0,
    G1,
    G2,
    G3,
}

/// The name of the active slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveSlot {
    GL,
    GR,
}

/// The list of supported character sets and their associated tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charset {
    Utf8,
    Ascii,
    British,
    DecSpecial,
}

/// The table for the given charset. This returns a pointer to a
/// slice that is guaranteed to be 255 chars that can be used to map
/// ASCII to the given charset.
// PORT NOTE: the reference returns `[]const u16` (a slice of the comptime
// tables); ported as `&'static [u16]`. The `.utf8 => unreachable` arm is kept
// verbatim as a `panic!`-equivalent (`unreachable!`).
#[inline]
pub fn table(set: Charset) -> &'static [u16] {
    match set {
        Charset::British => &BRITISH,
        Charset::DecSpecial => &DEC_SPECIAL,

        // utf8 is not a table, callers should double-check if the
        // charset is utf8 and NOT use tables.
        Charset::Utf8 => unreachable!(),

        // recommended that callers just map ascii directly but we can
        // support a table
        Charset::Ascii => &ASCII,
    }
}

/// Just a basic c => c ascii table
const ASCII: [u16; TABLE_LEN] = init_table();

/// https://vt100.net/docs/vt220-rm/chapter2.html
const BRITISH: [u16; TABLE_LEN] = {
    let mut tbl = init_table();
    tbl[0x23] = 0x00a3;
    tbl
};

/// https://en.wikipedia.org/wiki/DEC_Special_Graphics
const DEC_SPECIAL: [u16; TABLE_LEN] = {
    let mut tbl = init_table();
    tbl[0x60] = 0x25C6;
    tbl[0x61] = 0x2592;
    tbl[0x62] = 0x2409;
    tbl[0x63] = 0x240C;
    tbl[0x64] = 0x240D;
    tbl[0x65] = 0x240A;
    tbl[0x66] = 0x00B0;
    tbl[0x67] = 0x00B1;
    tbl[0x68] = 0x2424;
    tbl[0x69] = 0x240B;
    tbl[0x6a] = 0x2518;
    tbl[0x6b] = 0x2510;
    tbl[0x6c] = 0x250C;
    tbl[0x6d] = 0x2514;
    tbl[0x6e] = 0x253C;
    tbl[0x6f] = 0x23BA;
    tbl[0x70] = 0x23BB;
    tbl[0x71] = 0x2500;
    tbl[0x72] = 0x23BC;
    tbl[0x73] = 0x23BD;
    tbl[0x74] = 0x251C;
    tbl[0x75] = 0x2524;
    tbl[0x76] = 0x2534;
    tbl[0x77] = 0x252C;
    tbl[0x78] = 0x2502;
    tbl[0x79] = 0x2264;
    tbl[0x7a] = 0x2265;
    tbl[0x7b] = 0x03C0;
    tbl[0x7c] = 0x2260;
    tbl[0x7d] = 0x00A3;
    tbl[0x7e] = 0x00B7;
    tbl
};

/// Our table length is 256 so we can contain all ASCII chars.
// PORT NOTE: the reference is `std.math.maxInt(u8) + 1` == 256.
const TABLE_LEN: usize = u8::MAX as usize + 1;

/// Creates a table that maps ASCII to ASCII as a getting started point.
// PORT NOTE: `const fn` so the tables above can be built at compile time, like
// the reference comptime blocks. The `assert(i == table_len)` is kept.
const fn init_table() -> [u16; TABLE_LEN] {
    let mut result: [u16; TABLE_LEN] = [0; TABLE_LEN];
    let mut i: usize = 0;
    while i < TABLE_LEN {
        result[i] = i as u16;
        i += 1;
    }
    assert!(i == TABLE_LEN);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charset_tables() {
        // PORT NOTE: the reference uses `inline for (info.fields)` over the
        // enum's fields, skipping utf8. Rust has no comptime field reflection,
        // so we enumerate the variants explicitly. (utf8 is excluded here as
        // the reference `continue`s past it.)
        let fields = [Charset::Ascii, Charset::British, Charset::DecSpecial];
        for field in fields {
            // utf8 has no table
            if field == Charset::Utf8 {
                continue;
            }

            let tbl = table(field);

            // Yes, I could use `table_len` here, but I want to explicitly use a
            // hardcoded constant so that if there are miscompilations or a comptime
            // issue, we catch it.
            assert_eq!(256_usize, tbl.len());
        }
    }
}
