use crate::generated::grapheme_data;
use crate::props::GraphemeBreakNoControl;
use crate::props_table::table;

/// Grapheme break state, maintained between calls to [`grapheme_break`].
///
/// PARITY: mirrors `uucode.grapheme.BreakState` (uucode 9d555245, ghostty
/// 12542b392), a `packed struct(u3) { after_linker: bool, base: Base }`.
/// uucode documents it as:
///
/// > GB9c can overlap GB11: after `ExtPict Linker`, `Extend* ZWJ ExtPict` is
/// > still an emoji ZWJ sequence while `Extend* Consonant` is still a conjunct.
/// > So "after a linker" is tracked alongside `base` rather than as a state of
/// > its own.
/// >
/// > Field order matters: packed struct fields are laid out from the low bits
/// > up, so a `@bitCast` to `u3` gives `base * 2 + after_linker`. With
/// > `regional_indicator` last in `Base`, the one combination that can never
/// > occur (`regional_indicator` with `after_linker`, since the flag is only set
/// > when gb2 is InCB Extend or ZWJ while the base only becomes
/// > `regional_indicator` when gb2 is a regional indicator) is the highest value,
/// > so the precomputed tables can leave it out. `buildGraphemeBreakTable`
/// > asserts this at comptime.
///
/// PORT NOTE: stored as the `u3` bit pattern itself (`base * 2 +
/// after_linker`, i.e. the `@bitCast` upstream's packed `Precompute.Key` /
/// `Value` carry) in a `u8`, with [`BreakState::after_linker`] /
/// [`BreakState::base`] as the field reads. rustty-unicode only ever looks the
/// state up in the precomputed table (the rules that write the fields run in
/// the generator), so the hot path in [`grapheme_break`] indexes with the raw
/// value and each `Value` stays two bytes (PERF: no decode on the per-codepoint
/// path, and the 8192-entry table keeps its footprint).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(transparent)]
pub struct BreakState(u8);

/// `BreakState.Base` (`enum(u2)`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum Base {
    #[default]
    Default = 0,
    ExtendedPictographic = 1,
    RegionalIndicator = 2,
}

impl BreakState {
    /// Deprecated: initialize with `BreakState::default()` instead.
    ///
    /// PARITY: uucode keeps `pub const default: BreakState = .{}` as a
    /// deprecated alias, and ghostty still spells its states `.default`.
    #[allow(non_upper_case_globals)]
    pub const Default: BreakState = BreakState(0);

    // pub const table_len = @typeInfo(Base).@"enum".fields.len * 2 - 1;
    pub const TABLE_LEN: usize = 3 * 2 - 1;

    /// `BreakState{ .after_linker = after_linker, .base = base }`.
    #[inline]
    pub const fn new(after_linker: bool, base: Base) -> BreakState {
        BreakState((after_linker as u8) | ((base as u8) << 1))
    }

    /// The `after_linker` field.
    #[inline]
    pub const fn after_linker(self) -> bool {
        (self.0 & 1) != 0
    }

    /// The `base` field.
    #[inline]
    pub const fn base(self) -> Base {
        match self.0 >> 1 {
            0 => Base::Default,
            1 => Base::ExtendedPictographic,
            2 => Base::RegionalIndicator,
            _ => unreachable!(),
        }
    }

    // pub inline fn tableIndex(self: BreakState) usize { return @as(u3, @bitCast(self)); }
    #[inline]
    pub const fn table_index(self) -> usize {
        self.0 as usize
    }

    // pub fn fromTableIndex(i: usize) BreakState { return @bitCast(@as(u3, @intCast(i))); }
    //
    // PORT NOTE: `@intCast` to u3 and a `base` of 3 in the `@bitCast` are
    // safety-checked illegal behaviour upstream; they port as the panics below
    // (R7). Only the precompute decode calls this, at compile time.
    #[inline]
    pub const fn from_table_index(i: usize) -> BreakState {
        assert!(i < 8 && (i >> 1) < 3);
        BreakState(i as u8)
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
    let value = PRECOMPUTE_DATA[Key {
        gb1: table().get(cp1).grapheme_break,
        gb2: table().get(cp2).grapheme_break,
        state: *state,
    }
    .index()];
    *state = value.state;
    value.result
}

/// Width change requested by a codepoint that continues a grapheme cluster.
// pub const GraphemeWidthEffect = enum { ignore, no_change, wide, narrow };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphemeWidthEffect {
    /// Do not append the codepoint to the cluster and leave break state as it
    /// was before seeing it.
    Ignore,

    /// Append the codepoint but leave the current cluster width unchanged.
    NoChange,

    /// Make the cluster occupy two terminal cells.
    Wide,

    /// Make the cluster occupy one terminal cell.
    Narrow,
}

/// Result of measuring the first grapheme cluster in a codepoint slice.
// pub const GraphemeWidth = struct { len: usize, width: u2 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphemeWidth {
    /// Number of codepoints consumed from the input slice.
    pub len: usize,

    /// Display width in terminal cells.
    pub width: u8,
}

/// Returns the width effect of appending cp after prev within a grapheme.
///
/// This is the shared width-decision kernel for the streaming terminal
/// printer and for [`grapheme_width`]. It assumes [`grapheme_break`] has
/// already said there is no break between prev and cp; it does not perform
/// segmentation.
///
/// The `Ignore` result is important for invalid emoji variation selectors.
/// The terminal does not store those selectors in the cell, so callers must
/// also restore their grapheme break state and leave prev unchanged when they
/// see `Ignore`.
// pub inline fn graphemeWidthEffect(prev: u21, cp: u21) GraphemeWidthEffect
#[inline]
pub fn grapheme_width_effect(prev: u32, cp: u32) -> GraphemeWidthEffect {
    // Emoji variation selectors modify the width of a valid base:
    // VS16 makes the grapheme wide and VS15 makes it narrow. Check that
    // prev forms a valid variation sequence in emoji-variation-sequences.txt;
    // if it does not, ignore the selector entirely.
    if cp == 0xFE0F || cp == 0xFE0E {
        let prev_props = table().get(prev);
        if !prev_props.emoji_vs_base {
            return GraphemeWidthEffect::Ignore;
        }
        return match cp {
            0xFE0F => GraphemeWidthEffect::Wide,
            0xFE0E => GraphemeWidthEffect::Narrow,
            _ => unreachable!(),
        };
    }

    // If a code point contributes to the width of a grapheme, the whole
    // grapheme is at least width 2 because the first code point must be at
    // least width 1 to start. Prepend code points could effectively mean
    // the first code point should be width 0, but we don't handle that yet.
    if !table().get(cp).width_zero_in_grapheme {
        return GraphemeWidthEffect::Wide;
    }

    GraphemeWidthEffect::NoChange
}

#[inline]
fn invalid_codepoint(cp: u32) -> bool {
    cp > 0x10FFFF
}

/// Measures the first grapheme cluster in cps using the same segmentation and
/// width rules as `Terminal::print` with mode 2027.
///
/// This is not a streaming API: cps must contain a complete first grapheme
/// cluster or the logical end of the string. If codepoints arrive in chunks,
/// keep buffering when this consumes all available codepoints and more input
/// may still arrive.
///
/// For codepoint types wider than u21, values greater than U+10FFFF are
/// accepted so FFI-facing callers can use u32 input without trapping. An
/// invalid value consumes one codepoint at width 1 when it starts the slice,
/// and terminates the current cluster when it appears later. For u21 callers
/// these checks are comptime-dead.
///
/// PORT NOTE: the reference is generic over the codepoint type `T` with the
/// `check_invalid` guards comptime-dead for u21. Rustty callers are all u32,
/// so the guards are always live.
// pub fn graphemeWidth(comptime T: type, cps: []const T) GraphemeWidth
pub fn grapheme_width(cps: &[u32]) -> GraphemeWidth {
    if cps.is_empty() {
        return GraphemeWidth { len: 0, width: 0 };
    }

    // The C API accepts u32 codepoints, so it can receive values outside
    // Unicode's range. Guard before narrowing to u21; native u21 callers
    // skip this path at comptime.
    if invalid_codepoint(cps[0]) {
        return GraphemeWidth { len: 1, width: 1 };
    }

    let mut len: usize = 1;
    let mut width = table().get(cps[0]).width;
    let mut prev: u32 = cps[0];
    let mut state = BreakState::Default;

    while len < cps.len() {
        // Treat invalid u32 input as a boundary so a valid prefix cluster can
        // still be returned without attempting to narrow the invalid value.
        if invalid_codepoint(cps[len]) {
            break;
        }

        let cp = cps[len];
        let state_before = state;
        if grapheme_break(prev, cp, &mut state) {
            break;
        }

        match grapheme_width_effect(prev, cp) {
            GraphemeWidthEffect::Ignore => state = state_before,
            GraphemeWidthEffect::NoChange => prev = cp,
            GraphemeWidthEffect::Wide => {
                width = 2;
                prev = cp;
            }
            GraphemeWidthEffect::Narrow => {
                width = 1;
                prev = cp;
            }
        }

        len += 1;
    }

    GraphemeWidth { len, width }
}

/// This is all the structures and data for the precomputed lookup table
/// for all possible permutations of state and grapheme break properties.
/// Precomputation requires 2^13 keys of byte-sized values so the whole
/// table is 8KB.
struct Precompute;

/// The one shipped copy of `Precompute::DATA`, which `grapheme_break` reads.
///
/// PORT NOTE (P7-W22, N520): the reference's `Precompute.data` is a
/// container-level `const`, which Zig emits as ONE global object. A Rust
/// `const` array is instead inlined into every use, so the `#[inline]`
/// `grapheme_break` stamped a private 16 KiB copy into each crate that inlined
/// it (`librustty_vt_c.dylib` carried two). A `static` is the faithful
/// spelling of the single object; the read is the same address load.
static PRECOMPUTE_DATA: [Value; 8192] = Precompute::DATA;

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
        let state = self.state.table_index();
        let gb1 = self.gb1 as usize;
        let gb2 = self.gb2 as usize;
        // packed struct(u13): state occupies bits 0..3, gb1 bits 3..8, gb2 bits 8..13
        state | (gb1 << 3) | (gb2 << 8)
    }
}

/// PARITY: the reference `Value` is a `packed struct(u8)` laid out as
/// `{ result: bool, state: u3, _padding: u4 }` — bit 0 is `result`, bits
/// 1..=3 are `state`, bits 4..=7 are zero padding.
///
/// The reference carries that trailing field with this rationale:
///
/// > Explicit padding to give the struct a power-of-two backing
/// > integer, for the same reason as `Properties._padding` (avoids
/// > exotic-width LLVM integer accesses that defeat register
/// > promotion). @sizeOf was already 1, so the table stays 8KB.
///
/// PORT NOTE: rustty decodes each entry from the embedded byte table into a
/// plain (non-bitfield) Rust struct, so there is no exotic-width backing
/// integer to widen and no `_padding` field to carry. The serialized table
/// bytes are identical either way.
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
        // The table never hands back a state it has no row for
        // (`buildGraphemeBreakTable`'s `inlineAssert(state.tableIndex() <
        // BreakState.table_len)`); unreachable keys are zero.
        let mut i = 0;
        while i < 8192 {
            let byte = grapheme_data::DATA[i];
            result[i] = Value {
                result: (byte & 0x1) != 0,
                state: BreakState::from_table_index((byte >> 1) as usize),
            };
            i += 1;
        }
        result
    };
}

/// Harness accessor: re-encode rustty's RUNTIME grapheme precompute
/// (`PRECOMPUTE_DATA`, the table `grapheme_break` actually reads) back into the
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
        let v = PRECOMPUTE_DATA[i];
        out[i] = u8::from(v.result) | ((v.state.table_index() as u8) << 1);
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

    // uucode 9d555245 `src/grapheme.zig` test "BreakState table index layout"
    // (rustty's `BreakState` mirrors uucode's, so its layout test lives here).
    #[test]
    fn break_state_table_index_layout() {
        let s = BreakState::new;
        assert_eq!(0, BreakState::default().table_index());
        assert_eq!(1, s(true, Base::Default).table_index());
        assert_eq!(2, s(false, Base::ExtendedPictographic).table_index());
        assert_eq!(3, s(true, Base::ExtendedPictographic).table_index());
        assert_eq!(4, s(false, Base::RegionalIndicator).table_index());
        assert_eq!(5, BreakState::TABLE_LEN);
        assert_eq!(5, s(true, Base::RegionalIndicator).table_index());
        for i in 0..BreakState::TABLE_LEN {
            assert_eq!(i, BreakState::from_table_index(i).table_index());
        }
        // RUSTTY: the field reads round-trip through the stored bit pattern.
        for i in 0..BreakState::TABLE_LEN {
            let state = BreakState::from_table_index(i);
            assert_eq!(state, s(state.after_linker(), state.base()));
        }
        // The deprecated `.default` alias is the zero state.
        assert_eq!(BreakState::Default, BreakState::default());
    }

    // RUSTTY: uucode 9d555245 `src/grapheme.zig` test "Unicode 18 Indic linker
    // boundaries and overlapping emoji sequences", run through ghostty's
    // precomputed `graphemeBreak` (ghostty 12542b392 builds that table from
    // the same `computeGraphemeBreakNoControl`). These are all non-control
    // code points, so the no-control table must agree with uucode's
    // `isBreakNoControl` column of the upstream test.
    #[test]
    fn unicode_18_indic_linker_boundaries_and_overlapping_emoji_sequences() {
        let cases: [(&[u32], &[bool]); 19] = [
            // GB9c no longer needs a leading consonant: Linker Extend* x Consonant
            (&[0x094D, 0x0915], &[false]),
            (&[0x0061, 0x094D, 0x0915], &[false, false]),
            (&[0x094D, 0x0300, 0x200D, 0x0915], &[false, false, false]),
            (&[0x094D, 0x094D, 0x0915], &[false, false]),
            // Extend without a linker doesn't join a consonant
            (&[0x0915, 0x0300, 0x0915], &[false, true]),
            // A consonant ends the linker sequence
            (&[0x094D, 0x0915, 0x0300, 0x0915], &[false, false, true]),
            // Only InCB=Extend continues the sequence: ZWNJ, SpacingMark and an
            // emoji modifier all end it
            (&[0x094D, 0x200C, 0x0300, 0x0915], &[false, false, true]),
            (&[0x094D, 0x0903, 0x0915], &[false, true]),
            (&[0x094D, 0x1F3FB, 0x0915], &[true, true]),
            // InCB=Linker with Grapheme_Cluster_Break=Other (`indic_conjunct_break_linker_other`):
            // breaks before it, but still joins a following consonant
            (&[0x0061, 0x1CF5, 0x0300, 0x0915], &[true, false, false]),
            (&[0x0061, 0x1CF6, 0x0915], &[true, false]),
            (&[0x0061, 0x11A3A, 0x11A0B], &[true, false]),
            (&[0x11A3A, 0x0061], &[true]),
            // GB9c and GB11 overlap: a linker inside an emoji sequence can still
            // lead to either a ZWJ emoji or a consonant
            (
                &[0x1F600, 0x094D, 0x0300, 0x200D, 0x1F600],
                &[false, false, false, false],
            ),
            (&[0x1F600, 0x094D, 0x0300, 0x0915], &[false, false, false]),
            (
                &[0x1F600, 0x094D, 0x200C, 0x200D, 0x1F600],
                &[false, false, false, false],
            ),
            (&[0x1F600, 0x094D, 0x200C, 0x0915], &[false, false, true]),
            // An emoji modifier sequence can lead into GB9c
            (&[0x1F44D, 0x1F3FB, 0x094D, 0x0915], &[false, false, false]),
            // A Grapheme_Cluster_Break=Other linker ends the emoji sequence
            (&[0x1F600, 0x1CF5, 0x0915], &[true, false]),
        ];
        for (cps, breaks) in cases {
            let mut state = BreakState::default();
            for (i, &expected) in breaks.iter().enumerate() {
                assert_eq!(
                    expected,
                    grapheme_break(cps[i], cps[i + 1], &mut state),
                    "{cps:X?} at {i}"
                );
            }
        }
    }

    // test "grapheme width: variation selectors" (oracle 65e61282a)
    #[test]
    fn grapheme_width_variation_selectors() {
        assert_eq!(
            GraphemeWidthEffect::Wide,
            grapheme_width_effect(0x2764, 0xFE0F)
        );
        assert_eq!(
            GraphemeWidthEffect::Narrow,
            grapheme_width_effect(0x23, 0xFE0E)
        );
        assert_eq!(
            GraphemeWidthEffect::Ignore,
            grapheme_width_effect('x' as u32, 0xFE0F)
        );

        assert_eq!(
            GraphemeWidth { len: 2, width: 2 },
            grapheme_width(&[0x2764, 0xFE0F])
        );
        assert_eq!(
            GraphemeWidth { len: 2, width: 2 },
            grapheme_width(&[0x23, 0xFE0F])
        );
        assert_eq!(
            GraphemeWidth { len: 2, width: 1 },
            grapheme_width(&['x' as u32, 0xFE0F])
        );
        assert_eq!(
            GraphemeWidth { len: 3, width: 1 },
            grapheme_width(&['x' as u32, 0xFE0F, 0xFE0F])
        );
        assert_eq!(
            GraphemeWidth { len: 2, width: 1 },
            grapheme_width(&[0x23, 0xFE0E])
        );
        assert_eq!(
            GraphemeWidth { len: 2, width: 1 },
            grapheme_width(&[0x231A, 0xFE0E])
        );
        assert_eq!(
            GraphemeWidth { len: 3, width: 1 },
            grapheme_width(&[0x231A, 0xFE0E, 0xFE0F])
        );
        assert_eq!(
            GraphemeWidth { len: 4, width: 2 },
            grapheme_width(&[0x1F3F4, 0x200D, 0x2620, 0xFE0F])
        );
    }

    // test "grapheme width: emoji sequences" (oracle 65e61282a)
    #[test]
    fn grapheme_width_emoji_sequences() {
        assert_eq!(
            GraphemeWidth { len: 5, width: 2 },
            grapheme_width(&[0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467])
        );
        assert_eq!(
            GraphemeWidth { len: 3, width: 2 },
            grapheme_width(&[0x23, 0xFE0F, 0x20E3])
        );
        assert_eq!(
            GraphemeWidth { len: 2, width: 1 },
            grapheme_width(&['1' as u32, 0x20E3])
        );
        assert_eq!(
            GraphemeWidth { len: 2, width: 2 },
            grapheme_width(&[0x1F44B, 0x1F3FF])
        );
    }

    // test "grapheme width: spacing marks can widen narrow clusters" (oracle 65e61282a)
    #[test]
    fn grapheme_width_spacing_marks_widen() {
        let mut mark: Option<u32> = None;
        for cp in 0..0x110000u32 {
            let props = table().get(cp);
            if props.width != 1 || props.width_zero_in_grapheme {
                continue;
            }
            let mut state = BreakState::Default;
            if !grapheme_break('a' as u32, cp, &mut state) {
                mark = Some(cp);
                break;
            }
        }
        let cp = mark.expect("a spacing mark should exist");
        assert_eq!(1, table().get(cp).width);
        assert!(!table().get(cp).width_zero_in_grapheme);
        assert_eq!(
            GraphemeWidth { len: 2, width: 2 },
            grapheme_width(&['a' as u32, cp])
        );
    }

    // test "grapheme width: segmentation" (oracle 65e61282a)
    #[test]
    fn grapheme_width_segmentation() {
        assert_eq!(
            GraphemeWidth { len: 1, width: 1 },
            grapheme_width(&['a' as u32])
        );
        assert_eq!(
            GraphemeWidth { len: 1, width: 1 },
            grapheme_width(&['a' as u32, 'b' as u32])
        );
        assert_eq!(
            GraphemeWidth { len: 2, width: 2 },
            grapheme_width(&[0x1F1E6, 0x1F1E7, 0x1F1E8])
        );
        assert_eq!(
            GraphemeWidth { len: 1, width: 2 },
            grapheme_width(&[0x1F1E8])
        );
        assert_eq!(GraphemeWidth { len: 0, width: 0 }, grapheme_width(&[]));
        assert_eq!(
            GraphemeWidth { len: 2, width: 0 },
            grapheme_width(&[0x0301, 0x0302])
        );
    }

    // test "grapheme width: u32 invalid codepoints stand alone" (oracle 65e61282a)
    #[test]
    fn grapheme_width_u32_invalid() {
        assert_eq!(
            GraphemeWidth { len: 1, width: 1 },
            grapheme_width(&[0x110000, 0x0301])
        );
        assert_eq!(
            GraphemeWidth { len: 1, width: 1 },
            grapheme_width(&['a' as u32, 0x110000])
        );
    }
}
