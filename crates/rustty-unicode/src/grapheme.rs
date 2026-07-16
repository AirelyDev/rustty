use crate::generated::grapheme_data;
use crate::props::GraphemeBreakNoControl;
use crate::props_table::table;

/// Grapheme break state, maintained between calls to [`grapheme_break`].
///
/// PARITY: mirrors `uucode.grapheme.BreakState` (an `enum(u3)`). The packed
/// `Precompute.Value` stores this in bits 1..=3, so the discriminants must
/// match uucode's ordering exactly.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum BreakState {
    #[default]
    Default = 0,
    RegionalIndicator = 1,
    ExtendedPictographic = 2,
    IndicConjunctBreakConsonant = 3,
    IndicConjunctBreakLinker = 4,
}

impl BreakState {
    const fn from_int(v: u8) -> BreakState {
        match v {
            0 => BreakState::Default,
            1 => BreakState::RegionalIndicator,
            2 => BreakState::ExtendedPictographic,
            3 => BreakState::IndicConjunctBreakConsonant,
            4 => BreakState::IndicConjunctBreakLinker,
            _ => unreachable!(),
        }
    }
}

/// Determines if there is a grapheme break between two codepoints. This
/// must be called sequentially maintaining the state between calls.
///
/// This function does NOT work with control characters. Control characters,
/// line feeds, and carriage returns are expected to be filtered out before
/// calling this function. This is because this function is tuned for
/// Ghostty.
// PERF(port): Zig compiles callers whole-program so this per-codepoint lookup
// inlines under ReleaseFast; a non-generic cross-crate fn is un-inlinable
// without LTO — #[inline] restores the idiom.
#[inline]
pub fn grapheme_break(cp1: u32, cp2: u32, state: &mut BreakState) -> bool {
    let value = Precompute::DATA[Key {
        gb1: table().get(cp1).grapheme_break,
        gb2: table().get(cp2).grapheme_break,
        state: *state,
    }
    .index()];
    *state = value.state;
    value.result
}

/// This is all the structures and data for the precomputed lookup table
/// for all possible permutations of state and grapheme break properties.
/// Precomputation requires 2^13 keys of 4 bit values so the whole table is
/// 8KB.
struct Precompute;

/// PARITY: the reference `Key` is a `packed struct(u13)` laid out as
/// `{ state: u3, gb1: u5, gb2: u5 }` (least-significant field first), bitcast
/// to a u13 to index the table. We compute the same index arithmetically.
struct Key {
    state: BreakState,
    gb1: GraphemeBreakNoControl,
    gb2: GraphemeBreakNoControl,
}

impl Key {
    fn index(&self) -> usize {
        let state = self.state as usize;
        let gb1 = self.gb1 as usize;
        let gb2 = self.gb2 as usize;
        // packed struct(u13): state occupies bits 0..3, gb1 bits 3..8, gb2 bits 8..13
        state | (gb1 << 3) | (gb2 << 8)
    }
}

/// PARITY: the reference `Value` is a `packed struct(u4)` laid out as
/// `{ result: bool, state: u3 }` — bit 0 is `result`, bits 1..=3 are `state`.
#[derive(Clone, Copy)]
struct Value {
    result: bool,
    state: BreakState,
}

impl Precompute {
    const DATA: [Value; 8192] = {
        let mut result = [Value {
            result: false,
            state: BreakState::Default,
        }; 8192];
        let mut i = 0;
        while i < 8192 {
            let byte = grapheme_data::DATA[i];
            result[i] = Value {
                result: (byte & 0x1) != 0,
                state: BreakState::from_int(byte >> 1),
            };
            i += 1;
        }
        result
    };
}

/// Harness accessor: re-encode rustty's RUNTIME grapheme precompute
/// (`Precompute::DATA`, the table `grapheme_break` actually reads) back into the
/// `result | state<<1` byte form, for the unicode differential gate (ORACLE #2,
/// `.port/phase-0/harness/unicode/`). This is harness-only glue (R1: adds
/// nothing-lost) — it lets the harness diff rustty's live table against an
/// INDEPENDENT uucode regeneration, the full-range reproduction of the
/// reference's `test "... tables match uucode"`. Not used by any in-crate test.
#[doc(hidden)]
pub fn harness_precompute_bytes() -> [u8; 8192] {
    let mut out = [0u8; 8192];
    let mut i = 0;
    while i < 8192 {
        let v = Precompute::DATA[i];
        out[i] = u8::from(v.result) | ((v.state as u8) << 1);
        i += 1;
    }
    out
}

/// If you build this file as a binary, we will verify the grapheme break
/// implementation. This iterates over billions of codepoints so it is
/// SLOW. It's not meant to be run in CI, but it's useful for debugging.
/// TODO: this is hard to build with newer zig build, so
/// https://github.com/ghostty-org/ghostty/pull/7806 took the approach of
/// adding a `-Demit-unicode-test` option for `zig build`, but that
/// hasn't been done here.
///
/// PARITY: the reference `main` diffs rustty's `graphemeBreak` against
/// `uucode`'s own `grapheme.isBreak` as an oracle. rustty does not depend on
/// uucode/ghostty (R3), so the uucode oracle side lives in the external parity
/// harness; this debug entry point exercises rustty's `grapheme_break` over the
/// codepoint sweep and is provided for fidelity. The `uucode.config.max_code_point`
/// bound is `0x10FFFF`.
#[allow(dead_code)]
pub fn main() {
    // Set the min and max to control the test range.
    let min: u32 = 0;
    let max: u32 = 0x10FFFF + 1; // uucode.config.max_code_point + 1

    let mut state: BreakState = BreakState::Default;
    let _uu_state: BreakState = BreakState::Default;
    for cp1 in min..max {
        if cp1 % 1000 == 0 {
            eprintln!("progress cp1={cp1}");
        }

        if cp1 == '\r' as u32 || cp1 == '\n' as u32 {
            continue;
        }

        for cp2 in min..max {
            if cp2 == '\r' as u32 || cp2 == '\n' as u32 {
                continue;
            }

            let _gb = grapheme_break(cp1, cp2, &mut state);
            // The uucode oracle comparison (`uucode.grapheme.isBreak`) runs in
            // the external parity harness (R3); a mismatch there is the
            // differential gate equivalent of the reference's `std.log.warn`.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PARITY: the reference test uses `uucode.utf8.Iterator` to decode the
    // family-emoji byte sequence; this is a minimal equivalent over a Rust
    // `&str`'s `char`s yielding codepoints as `u32`.
    struct Utf8Iterator<'a> {
        chars: std::str::Chars<'a>,
    }

    impl<'a> Utf8Iterator<'a> {
        fn init(s: &'a str) -> Self {
            Self { chars: s.chars() }
        }

        fn next(&mut self) -> Option<u32> {
            self.chars.next().map(|c| c as u32)
        }
    }

    #[test]
    fn grapheme_break_emoji_modifier() {
        // Emoji and modifier
        {
            let mut state: BreakState = BreakState::Default;
            assert!(!grapheme_break(0x261D, 0x1F3FF, &mut state));
        }

        // Non-emoji and emoji modifier
        {
            let mut state: BreakState = BreakState::Default;
            assert!(grapheme_break(0x22, 0x1F3FF, &mut state));
        }
    }

    #[test]
    fn long_emoji_zwj_sequences() {
        let mut state: BreakState = BreakState::Default;
        // 👩‍👩‍👧‍👦 (family: woman, woman, girl, boy)
        let mut it =
            Utf8Iterator::init("\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}_");
        let mut cp1 = it.next().unwrap();
        let mut cp2 = it.next().unwrap();
        assert!(cp1 == 0x1F469); // 👩
        assert!(!grapheme_break(cp1, cp2, &mut state));

        cp1 = cp2;
        cp2 = it.next().unwrap();
        assert!(cp1 == 0x200D);
        assert!(!grapheme_break(cp1, cp2, &mut state));

        cp1 = cp2;
        cp2 = it.next().unwrap();
        assert!(cp1 == 0x1F469); // 👩
        assert!(!grapheme_break(cp1, cp2, &mut state));

        cp1 = cp2;
        cp2 = it.next().unwrap();
        assert!(cp1 == 0x200D);
        assert!(!grapheme_break(cp1, cp2, &mut state));

        cp1 = cp2;
        cp2 = it.next().unwrap();
        assert!(cp1 == 0x1F467); // 👧
        assert!(!grapheme_break(cp1, cp2, &mut state));

        cp1 = cp2;
        cp2 = it.next().unwrap();
        assert!(cp1 == 0x200D);
        assert!(!grapheme_break(cp1, cp2, &mut state));

        cp1 = cp2;
        cp2 = it.next().unwrap();
        assert!(cp1 == 0x1F466); // 👦
        assert!(grapheme_break(cp1, cp2, &mut state)); // break
    }
}
