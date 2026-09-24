//! `uucode/src/grapheme.zig` (uucode 2826a37a, the revision ghostty@20abdb50a wires into
//! libvaxis 0.6.0) — the UAX #29 grapheme-break `Iterator` + the full GB3..GB999 rule
//! ladder (`computeGraphemeBreak`) + the 2000-entry precompute. Ported line-for-line;
//! every rule comment preserved (R1).
//!
//! SUBSET: vaxis reaches only `Iterator(utf8.Iterator)` (`nextCodePoint`, `.code_point`,
//! `.is_break`, `.i`), so the `NoControl` family, the `wcwidth*` functions and their tests
//! (grapheme.zig:750-1512) are not ported here; Ghostty's own copy of those lives in
//! `rustty-unicode`.
//!
//! The Zig `CustomIterator` is generic over the code-point iterator, the break enum, the
//! state enum, the field and the break function (comptime params); vaxis only ever
//! instantiates `Iterator(utf8.Iterator)` = `CustomIterator(.., types.GraphemeBreak,
//! BreakState, .grapheme_break, precomputedGraphemeBreak)`, so [`Iterator`] is generic
//! over the [`CodePointIterator`] trait only (`.next()`/`.i`).

use super::get;
use super::types::GraphemeBreak;
use super::utf8;

/// The code-point source the grapheme `Iterator` walks (the `.next()`/`.i` surface the
/// Zig `CustomIterator` reads off its iterator field).
pub trait CodePointIterator {
    fn next_cp(&mut self) -> Option<u32>;
    fn index(&self) -> usize;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IteratorResult {
    pub code_point: u32,
    pub is_break: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Grapheme {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone)]
pub struct Iterator<I: CodePointIterator + Clone> {
    // This "i" is part of the documented API of this iterator, pointing to
    // the current location of the iterator in the underlying bytes (the
    // `i` of the CodePointIterator).
    pub i: usize,

    pub state: BreakState,
    next_cp_it: I,
    next_cp: Option<u32>,
    next_gb: GraphemeBreak,
}

impl<I: CodePointIterator + Clone> Iterator<I> {
    pub fn init(cp_it: I) -> Self {
        let mut next_cp_it = cp_it;
        let i = next_cp_it.index();
        let next_cp = next_cp_it.next_cp();

        Self {
            state: BreakState::Default,
            i,
            next_gb: if let Some(code_point) = next_cp {
                get::grapheme_break(code_point)
            } else {
                GraphemeBreak::Other
            },
            next_cp_it,
            next_cp,
        }
    }

    pub fn next_code_point(&mut self) -> Option<IteratorResult> {
        self.next_cp?;

        let cp1 = self.next_cp.unwrap();
        let gb1 = self.next_gb;
        self.i = self.next_cp_it.index();
        self.next_cp = self.next_cp_it.next_cp();

        if let Some(cp2) = self.next_cp {
            self.next_gb = get::grapheme_break(cp2);
            let is_break = precomputed_grapheme_break(gb1, self.next_gb, &mut self.state);
            Some(IteratorResult {
                code_point: cp1,
                is_break,
            })
        } else {
            Some(IteratorResult {
                code_point: cp1,
                is_break: true,
            })
        }
    }

    pub fn peek_code_point(&self) -> Option<IteratorResult> {
        let mut it = self.clone();
        it.next_code_point()
    }

    pub fn next_grapheme(&mut self) -> Option<Grapheme> {
        let start = self.i;
        while let Some(result) = self.next_code_point() {
            if result.is_break {
                return Some(Grapheme { start, end: self.i });
            }
        }
        None
    }

    pub fn peek_grapheme(&self) -> Option<Grapheme> {
        let mut it = self.clone();
        it.next_grapheme()
    }
}

pub fn utf8_iterator(bytes: &[u8]) -> Iterator<utf8::Iterator<'_>> {
    Iterator::init(utf8::Iterator::init(bytes))
}

/// `BreakState` `enum(u3)` — 5 variants, discriminants pinned to grapheme.zig:197-203.
/// Indexes the precompute's `state` dimension (`state*400 + …`).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BreakState {
    Default = 0,
    RegionalIndicator = 1,
    ExtendedPictographic = 2,
    IndicConjunctBreakConsonant = 3,
    IndicConjunctBreakLinker = 4,
}

impl BreakState {
    const COUNT: usize = 5;

    const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Default,
            1 => Self::RegionalIndicator,
            2 => Self::ExtendedPictographic,
            3 => Self::IndicConjunctBreakConsonant,
            4 => Self::IndicConjunctBreakLinker,
            _ => unreachable!(),
        }
    }
}

pub const fn compute_grapheme_break(
    gb1: GraphemeBreak,
    gb2: GraphemeBreak,
    state: &mut BreakState,
) -> bool {
    // Set state back to default when `gb1` or `gb2` is not expected in sequence.
    match *state {
        BreakState::RegionalIndicator => {
            if !matches!(gb1, GraphemeBreak::RegionalIndicator)
                || !matches!(gb2, GraphemeBreak::RegionalIndicator)
            {
                *state = BreakState::Default;
            }
        }
        BreakState::ExtendedPictographic => {
            match gb1 {
                // Keep state if in possibly valid sequence
                GraphemeBreak::IndicConjunctBreakExtend // extend
                | GraphemeBreak::IndicConjunctBreakLinker // extend
                | GraphemeBreak::Zwnj // extend
                | GraphemeBreak::Zwj
                | GraphemeBreak::ExtendedPictographic
                | GraphemeBreak::EmojiModifierBase
                | GraphemeBreak::EmojiModifier => {}

                _ => *state = BreakState::Default,
            }

            match gb2 {
                // Keep state if in possibly valid sequence
                GraphemeBreak::IndicConjunctBreakExtend // extend
                | GraphemeBreak::IndicConjunctBreakLinker // extend
                | GraphemeBreak::Zwnj // extend
                | GraphemeBreak::Zwj
                | GraphemeBreak::ExtendedPictographic
                | GraphemeBreak::EmojiModifierBase
                | GraphemeBreak::EmojiModifier => {}

                _ => *state = BreakState::Default,
            }
        }
        BreakState::IndicConjunctBreakConsonant | BreakState::IndicConjunctBreakLinker => {
            match gb1 {
                // Keep state if in possibly valid sequence
                GraphemeBreak::IndicConjunctBreakConsonant
                | GraphemeBreak::IndicConjunctBreakLinker
                | GraphemeBreak::IndicConjunctBreakExtend
                | GraphemeBreak::Zwj => {} // indic_conjunct_break_extend

                _ => *state = BreakState::Default,
            }

            match gb2 {
                // Keep state if in possibly valid sequence
                GraphemeBreak::IndicConjunctBreakConsonant
                | GraphemeBreak::IndicConjunctBreakLinker
                | GraphemeBreak::IndicConjunctBreakExtend
                | GraphemeBreak::Zwj => {} // indic_conjunct_break_extend

                _ => *state = BreakState::Default,
            }
        }
        BreakState::Default => {}
    }

    // GB3: CR x LF
    if matches!(gb1, GraphemeBreak::Cr) && matches!(gb2, GraphemeBreak::Lf) {
        return false;
    }

    // GB4: Control
    if matches!(
        gb1,
        GraphemeBreak::Control | GraphemeBreak::Cr | GraphemeBreak::Lf
    ) {
        return true;
    }

    // GB5: Control
    if matches!(
        gb2,
        GraphemeBreak::Control | GraphemeBreak::Cr | GraphemeBreak::Lf
    ) {
        return true;
    }

    // GB6: L x (L | V | LV | VT)
    if matches!(gb1, GraphemeBreak::L)
        && matches!(
            gb2,
            GraphemeBreak::L | GraphemeBreak::V | GraphemeBreak::Lv | GraphemeBreak::Lvt
        )
    {
        return false;
    }

    // GB7: (LV | V) x (V | T)
    if matches!(gb1, GraphemeBreak::Lv | GraphemeBreak::V)
        && matches!(gb2, GraphemeBreak::V | GraphemeBreak::T)
    {
        return false;
    }

    // GB8: (LVT | T) x T
    if matches!(gb1, GraphemeBreak::Lvt | GraphemeBreak::T) && matches!(gb2, GraphemeBreak::T) {
        return false;
    }

    // Handle GB9 (Extend | ZWJ) later, since it can also match the start of
    // GB9c (Indic) and GB11 (Emoji ZWJ)

    // GB9a: SpacingMark
    if matches!(gb2, GraphemeBreak::SpacingMark) {
        return false;
    }

    // GB9b: Prepend
    if matches!(gb1, GraphemeBreak::Prepend) {
        return false;
    }

    // GB9c: Indic
    if matches!(gb1, GraphemeBreak::IndicConjunctBreakConsonant) {
        // start of sequence:

        // In normal operation, we'll be in this state, but
        // buildGraphemeBreakTable iterates all states.
        //inlineAssert(state.* == .default);

        if is_indic_conjunct_break_extend(gb2) {
            *state = BreakState::IndicConjunctBreakConsonant;
            return false;
        } else if matches!(gb2, GraphemeBreak::IndicConjunctBreakLinker) {
            // jump straight to linker state
            *state = BreakState::IndicConjunctBreakLinker;
            return false;
        }
        // else, not an Indic sequence
    } else if matches!(*state, BreakState::IndicConjunctBreakConsonant) {
        // consonant state:

        if matches!(gb2, GraphemeBreak::IndicConjunctBreakLinker) {
            // consonant -> linker transition
            *state = BreakState::IndicConjunctBreakLinker;
            return false;
        } else if is_indic_conjunct_break_extend(gb2) {
            // continue [extend]* sequence
            return false;
        } else {
            // Not a valid Indic sequence
            *state = BreakState::Default;
        }
    } else if matches!(*state, BreakState::IndicConjunctBreakLinker) {
        // linker state:

        if matches!(gb2, GraphemeBreak::IndicConjunctBreakLinker)
            || is_indic_conjunct_break_extend(gb2)
        {
            // continue [extend linker]* sequence
            return false;
        } else if matches!(gb2, GraphemeBreak::IndicConjunctBreakConsonant) {
            // linker -> end of sequence
            *state = BreakState::Default;
            return false;
        } else {
            // Not a valid Indic sequence
            *state = BreakState::Default;
        }
    }

    // GB11: Emoji ZWJ sequence and Emoji modifier sequence
    if is_extended_pictographic(gb1) {
        // start of sequence:

        // In normal operation, we'll be in this state, but
        // buildGraphemeBreakTable iterates all states.
        // inlineAssert(state.* == .default);

        if is_extend(gb2) || matches!(gb2, GraphemeBreak::Zwj) {
            *state = BreakState::ExtendedPictographic;
            return false;
        }

        // The `emoji_modifier_sequence` case is described in the comment for
        // `isExtend` above, from UTS #51.
        if matches!(gb1, GraphemeBreak::EmojiModifierBase)
            && matches!(gb2, GraphemeBreak::EmojiModifier)
        {
            *state = BreakState::ExtendedPictographic;
            return false;
        }

        // else, not an Emoji ZWJ sequence
    } else if matches!(*state, BreakState::ExtendedPictographic) {
        // continue or end sequence:

        if (is_extend(gb1) || matches!(gb1, GraphemeBreak::EmojiModifier))
            && (is_extend(gb2) || matches!(gb2, GraphemeBreak::Zwj))
        {
            // continue extend* ZWJ sequence
            return false;
        } else if matches!(gb1, GraphemeBreak::Zwj) && is_extended_pictographic(gb2) {
            // ZWJ -> end of sequence
            *state = BreakState::Default;
            return false;
        } else {
            // Not a valid Emoji ZWJ sequence
            *state = BreakState::Default;
        }
    }

    // GB12 and GB13: Regional Indicator
    if matches!(gb1, GraphemeBreak::RegionalIndicator)
        && matches!(gb2, GraphemeBreak::RegionalIndicator)
    {
        if matches!(*state, BreakState::Default) {
            *state = BreakState::RegionalIndicator;
            return false;
        } else {
            *state = BreakState::Default;
            return true;
        }
    }

    // GB9: x (Extend | ZWJ)
    if is_extend(gb2) || matches!(gb2, GraphemeBreak::Zwj) {
        return false;
    }

    // GB999: Otherwise, break everywhere
    true
}

const fn is_indic_conjunct_break_extend(gb: GraphemeBreak) -> bool {
    matches!(
        gb,
        GraphemeBreak::IndicConjunctBreakExtend | GraphemeBreak::Zwj
    )
}

// Despite `emoji_modifier` being `extend` according to
// GraphemeBreakProperty.txt and UAX #29 (in addition to tests in
// GraphemeBreakTest.txt), UTS #51 states: `emoji_modifier_sequence :=
// emoji_modifier_base emoji_modifier` in ED-13 (emoji modifier sequence) under
// 1.4.4 (Emoji Modifiers), and: "When used alone, the default representation
// of these modifier characters is a color swatch... To have an effect on an
// emoji, an emoji modifier must immediately follow that base emoji
// character." in 2.4 (Diversity). Additionally it states "Skin tone
// modifiers and hair components should be
// displayed even in isolation" in ED-20 (basic emoji set) under 1.4.6 (Emoji
// Sets). See this revision of UAX #29 when the grapheme cluster break
// properties were simplified to remove `E_Base` and `E_Modifier`:
// http://www.unicode.org/reports/tr29/tr29-32.html
// Here we decide to diverge from the grapheme break spec, which is allowed
// under "tailored" grapheme clusters.
const fn is_extend(gb: GraphemeBreak) -> bool {
    matches!(
        gb,
        GraphemeBreak::Zwnj
            | GraphemeBreak::IndicConjunctBreakExtend
            | GraphemeBreak::IndicConjunctBreakLinker
    )
}

const fn is_extended_pictographic(gb: GraphemeBreak) -> bool {
    matches!(
        gb,
        GraphemeBreak::ExtendedPictographic | GraphemeBreak::EmojiModifierBase
    )
}

// The precompute: 5 BreakState x 20 GraphemeBreak x 20 GraphemeBreak = 2000 entries.
const N_GB: usize = GraphemeBreak::COUNT; // 20
const N_GB_2: usize = N_GB * N_GB; // 400
const N_STATE: usize = BreakState::COUNT; // 5
const N: usize = N_STATE * N_GB_2; // 2000

// grapheme.zig:607 asserts `@sizeOf(@TypeOf(table)) == 2000` ("5 BreakState fields x
// (20 GraphemeBreak fields)^2 = 2000") — there the packed `Result` (bool + u3 state) is 1
// byte, so bytes == entries. We pin the entry COUNT instead.
const _: () = assert!(N == 2000);

#[derive(Clone, Copy)]
struct TableResult {
    result: bool,
    state: BreakState,
}

const fn table_index(gb1: GraphemeBreak, gb2: GraphemeBreak, state: BreakState) -> usize {
    state as usize * N_GB_2 + gb1 as usize * N_GB + gb2 as usize
}

const fn build_grapheme_break_table() -> [TableResult; N] {
    let mut table = [TableResult {
        result: false,
        state: BreakState::Default,
    }; N];

    let mut si = 0;
    while si < N_STATE {
        let mut g1 = 0;
        while g1 < N_GB {
            let mut g2 = 0;
            while g2 < N_GB {
                let original_state = BreakState::from_u8(si as u8);
                let gb1 = GraphemeBreak::from_u8(g1 as u8);
                let gb2 = GraphemeBreak::from_u8(g2 as u8);
                let mut state = original_state;
                let result = compute_grapheme_break(gb1, gb2, &mut state);
                table[table_index(gb1, gb2, original_state)] = TableResult { result, state };
                g2 += 1;
            }
            g1 += 1;
        }
        si += 1;
    }

    table
}

static GRAPHEME_BREAK_TABLE: [TableResult; N] = build_grapheme_break_table();

/// `precomputedGraphemeBreak` — the table lookup. Equivalent to [`compute_grapheme_break`]
/// for all inputs (a pure optimization); both are ported to preserve R1.
pub fn precomputed_grapheme_break(
    gb1: GraphemeBreak,
    gb2: GraphemeBreak,
    state: &mut BreakState,
) -> bool {
    let result = GRAPHEME_BREAK_TABLE[table_index(gb1, gb2, *state)];
    *state = result.state;
    result.result
}

pub fn is_break(cp1: u32, cp2: u32, state: &mut BreakState) -> bool {
    let gb1 = get::grapheme_break(cp1);
    let gb2 = get::grapheme_break(cp2);
    precomputed_grapheme_break(gb1, gb2, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One `nextCodePoint` step and its two `expect`s (`result.?.code_point == cp`,
    /// `result.?.is_break == is_break`).
    #[track_caller]
    fn step(it: &mut Iterator<utf8::Iterator<'_>>, cp: u32, is_break: bool) {
        let result = it.next_code_point().expect("result.?");
        assert_eq!(result.code_point, cp);
        assert_eq!(result.is_break, is_break);
    }

    #[test]
    fn iterator_next_code_point_peek_code_point() {
        let str = "👩🏽‍🚀🇨🇭";
        let mut it = Iterator::init(utf8::Iterator::init(str.as_bytes()));
        assert_eq!(it.i, 0);

        let mut result = it.peek_code_point();
        assert_eq!(it.i, 0);
        assert_eq!(result.unwrap().code_point, 0x1F469); // 👩
        assert!(!result.unwrap().is_break);

        result = it.next_code_point();
        assert_eq!(it.i, 4);
        assert_eq!(result.unwrap().code_point, 0x1F469); // 👩
        assert!(!result.unwrap().is_break);

        result = it.next_code_point();
        assert_eq!(result.unwrap().code_point, 0x1F3FD); // 🏽
        assert!(!result.unwrap().is_break);

        result = it.next_code_point();
        assert_eq!(result.unwrap().code_point, 0x200D); // Zero width joiner
        assert!(!result.unwrap().is_break);

        result = it.peek_code_point();
        assert_eq!(result.unwrap().code_point, 0x1F680); // 🚀
        assert!(result.unwrap().is_break);

        result = it.next_code_point();
        assert_eq!(it.i, 15);
        assert_eq!(result.unwrap().code_point, 0x1F680); // 🚀
        assert!(result.unwrap().is_break);
        assert_eq!(&str.as_bytes()[..it.i], "👩🏽‍🚀".as_bytes());

        result = it.next_code_point();
        assert_eq!(result.unwrap().code_point, 0x1F1E8); // Regional Indicator "C"
        assert!(!result.unwrap().is_break);

        result = it.next_code_point();
        assert_eq!(it.i, str.len());
        assert_eq!(result.unwrap().code_point, 0x1F1ED); // Regional Indicator "H"
        assert!(result.unwrap().is_break);

        assert!(it.peek_code_point().is_none());
        assert!(it.next_code_point().is_none());
        assert!(it.next_code_point().is_none());
    }

    #[test]
    fn utf8_iterator_next_grapheme_peek_grapheme() {
        let str = "👩🏽‍🚀🇨🇭👨🏻‍🍼";
        let s = str.as_bytes();
        let mut it = utf8_iterator(s);
        assert_eq!(it.i, 0);

        let mut result = it.peek_grapheme();
        assert_eq!(result.unwrap().start, 0);
        assert_eq!(result.unwrap().end, 15);
        assert_eq!(it.i, 0);

        result = it.next_grapheme();
        assert_eq!(result.unwrap().start, 0);
        assert_eq!(result.unwrap().end, 15);
        assert_eq!(it.i, 15);
        assert_eq!(
            &s[result.unwrap().start..result.unwrap().end],
            "👩🏽‍🚀".as_bytes()
        );

        result = it.next_grapheme();
        assert_eq!(result.unwrap().start, 15);
        assert_eq!(result.unwrap().end, 23);
        assert_eq!(it.i, 23);
        assert_eq!(
            &s[result.unwrap().start..result.unwrap().end],
            "🇨🇭".as_bytes()
        );

        result = it.peek_grapheme();
        assert_eq!(result.unwrap().start, 23);
        assert_eq!(result.unwrap().end, s.len());
        assert_eq!(
            &s[result.unwrap().start..result.unwrap().end],
            "👨🏻‍🍼".as_bytes()
        );

        result = it.next_grapheme();
        assert_eq!(result.unwrap().start, 23);
        assert_eq!(result.unwrap().end, s.len());
        assert_eq!(it.i, s.len());

        assert!(it.peek_grapheme().is_none());
        assert!(it.next_grapheme().is_none());
        assert!(it.next_grapheme().is_none());
    }

    // ---- GraphemeBreakTest.txt conformance ----

    fn test_parse_cp(s: &str) -> u32 {
        u32::from_str_radix(s, 16).unwrap()
    }

    /// `components.trim` — strip a `#`-comment then trim `" \t\r"`.
    fn test_trim(line: &str) -> &str {
        match line.find('#') {
            Some(idx) => line[..idx].trim_matches([' ', '\t', '\r']),
            None => line.trim_matches([' ', '\t', '\r']),
        }
    }

    fn test_grapheme_break(get_actual_is_break: fn(u32, u32, &mut BreakState) -> bool) {
        // Embedded at compile time (not read at runtime) so the UAX #29
        // conformance corpus also runs under Miri, which cannot do file I/O
        // under isolation (the canonical G6 gate runs `cargo +nightly miri
        // test` with isolation on).
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/ucd/auxiliary/GraphemeBreakTest.txt"
        ));

        let mut success = true;
        // WS12 T4 vacuity guard: a corpus-driven test that silently iterates ZERO lines
        // is the classic vacuous check — it would stay green if the `include_str!` path
        // ever resolved to an empty or comment-only file. Count the cases actually
        // driven and assert a non-zero floor below.
        let mut cases = 0usize;
        // S-10b: the emoji-modifier divergence below must actually be exercised by the
        // corpus, or the tailoring it documents is untested.
        let mut modifier_overrides = 0usize;
        for (i, line) in content.split('\n').enumerate() {
            let line_num = i + 1;
            let trimmed = test_trim(line);
            if trimmed.is_empty() {
                continue;
            }

            let mut parts = trimmed.split(' ').peekable();
            let start = parts.next().unwrap();
            assert_eq!(start, "÷");

            cases += 1;
            let mut state = BreakState::Default;
            let mut cp1 = test_parse_cp(parts.next().unwrap());
            let mut gb1 = get::grapheme_break(cp1);
            let mut expected_str = parts.next().unwrap();
            let mut cp2 = test_parse_cp(parts.next().unwrap());
            let mut gb2 = get::grapheme_break(cp2);
            let mut next_expected_str = parts.next().unwrap();

            loop {
                let mut expected_is_break = expected_str == "÷";
                let actual_is_break = get_actual_is_break(cp1, cp2, &mut state);
                assert!(expected_is_break || expected_str == "×");
                // GraphemeBreakTest.txt has tests for UAX #29 treating emoji
                // modifier as extend, always, but we diverge from that (see
                // comment above `isExtend`).
                if gb2 == GraphemeBreak::EmojiModifier && gb1 != GraphemeBreak::EmojiModifierBase {
                    assert!(!expected_is_break);
                    expected_is_break = true;
                    modifier_overrides += 1;
                }
                if actual_is_break != expected_is_break {
                    eprintln!(
                        "line={line_num} cp1={cp1:x}, cp2={cp2:x}: gb1={gb1:?}, gb2={gb2:?}, \
                         state={state:?}, expected={expected_is_break}, actual={actual_is_break}"
                    );
                    success = false;
                }

                if parts.peek().is_none() {
                    break;
                }

                cp1 = cp2;
                gb1 = gb2;
                expected_str = next_expected_str;
                cp2 = test_parse_cp(parts.next().unwrap());
                gb2 = get::grapheme_break(cp2);
                next_expected_str = parts.next().unwrap();
            }

            assert_eq!(next_expected_str, "÷");
        }

        // UAX #29 `GraphemeBreakTest.txt` for Unicode 17.0.0 carries 766 non-comment
        // cases. Pin a floor (not the exact number, so a legitimate Unicode bump does not
        // spuriously fail) that is high enough that a truncated or wrong corpus is caught.
        assert!(
            cases >= 700,
            "UAX #29 conformance corpus drove only {cases} cases — expected >= 700. \
             The GraphemeBreakTest.txt embed is empty, truncated or misparsed, which \
             would make this test vacuously green."
        );
        assert!(
            modifier_overrides > 0,
            "the emoji-modifier tailoring was never exercised by the corpus"
        );

        assert!(success);
    }

    fn test_get_actual_computed_grapheme_break(cp1: u32, cp2: u32, state: &mut BreakState) -> bool {
        let gb1 = get::grapheme_break(cp1);
        let gb2 = get::grapheme_break(cp2);
        compute_grapheme_break(gb1, gb2, state)
    }

    #[test]
    fn grapheme_break_test_compute_grapheme_break() {
        test_grapheme_break(test_get_actual_computed_grapheme_break);
    }

    #[test]
    fn grapheme_break_test_is_break() {
        test_grapheme_break(is_break);
    }

    #[test]
    fn long_emoji_zwj_sequences() {
        // 👩‍👩‍👧‍👦 (family: woman, woman, girl, boy)
        let mut it = utf8_iterator(
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}_".as_bytes(),
        );
        step(&mut it, 0x1F469, false); // 👩
        step(&mut it, 0x200D, false);
        step(&mut it, 0x1F469, false); // 👩
        step(&mut it, 0x200D, false);
        step(&mut it, 0x1F467, false); // 👧
        step(&mut it, 0x200D, false);
        step(&mut it, 0x1F466, true); // 👦 // break
        step(&mut it, b'_' as u32, true); // break
    }

    #[test]
    fn long_emoji_zwj_sequences_with_emoji_modifiers() {
        // 👨🏻‍❤️‍👨🏿 Kiss: man, man, light skin tone, dark skin tone
        let mut it = utf8_iterator(
            "\u{1F468}\u{1F3FB}\u{200D}\u{2764}\u{FE0F}\u{200D}\u{1F48B}\u{200D}\u{1F468}\u{1F3FF}_"
                .as_bytes(),
        );
        step(&mut it, 0x1F468, false); // Man
        step(&mut it, 0x1F3FB, false); // Light Skin Tone
        step(&mut it, 0x200D, false); // ZWJ
        step(&mut it, 0x2764, false); // Heart
        step(&mut it, 0xFE0F, false); // VS16
        step(&mut it, 0x200D, false); // ZWJ
        step(&mut it, 0x1F48B, false); // Kiss Mark
        step(&mut it, 0x200D, false); // ZWJ
        step(&mut it, 0x1F468, false); // Man
        step(&mut it, 0x1F3FF, true); // Dark Skin Tone // break
        step(&mut it, b'_' as u32, true); // break (emoji_mod)
    }

    #[test]
    fn sequence_of_regional_indicators() {
        // 🇺🇸🇦🇹🇼_🇳_
        let mut it =
            utf8_iterator("\u{1F1FA}\u{1F1F8}\u{1F1E6}\u{1F1F9}\u{1F1FC}_\u{1F1F3}_".as_bytes());

        let result = it.next_code_point();
        assert_eq!(result.unwrap().code_point, 0x1F1FA); // U
        assert_eq!(it.state, BreakState::RegionalIndicator);
        assert!(!result.unwrap().is_break);

        let result = it.next_code_point();
        assert_eq!(result.unwrap().code_point, 0x1F1F8); // S
        assert_eq!(it.state, BreakState::Default);
        assert!(result.unwrap().is_break); // break

        step(&mut it, 0x1F1E6, false); // A
        step(&mut it, 0x1F1F9, true); // T // break
        step(&mut it, 0x1F1FC, true); // W // break
        step(&mut it, b'_' as u32, true); // break
        step(&mut it, 0x1F1F3, true); // N // break
        step(&mut it, b'_' as u32, true); // break
    }
}
