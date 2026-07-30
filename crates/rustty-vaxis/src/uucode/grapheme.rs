//! `uucode/src/grapheme.zig` — the UAX #29 grapheme-break `Iterator` + the full
//! GB3..GB999 rule ladder (`computeGraphemeBreak`) + the 1620-entry precompute
//! (VAXIS_C1B_DESIGN.md §7). Ported line-for-line; every rule comment preserved (R1).
//!
//! The Zig `CustomIterator` is generic over the code-point iterator via a comptime
//! type param; here [`Iterator`] is generic over the [`CodePointIterator`] trait
//! (`.next()`/`.i`), monomorphized by vaxis to `utf8::Iterator`.

use super::get;
use super::types::GraphemeBreak;

/// The code-point source the grapheme `Iterator` walks (the `.next()`/`.i` surface the
/// Zig `CustomIterator` reads off its iterator field).
pub trait CodePointIterator {
    fn next_cp(&mut self) -> Option<u32>;
    fn index(&self) -> usize;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IteratorResult {
    pub cp: u32,
    pub is_break: bool,
}

#[derive(Clone)]
pub struct Iterator<I: CodePointIterator + Clone> {
    pub state: BreakState,
    pub i: usize,
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
            next_gb: if let Some(cp) = next_cp {
                get::grapheme_break(cp)
            } else {
                GraphemeBreak::Other
            },
            next_cp_it,
            next_cp,
        }
    }

    // Faithful port of Zig `CustomIterator.next`; not the std `Iterator` trait
    // (this type also exposes `peek`/`next_break` + is generic over its own trait).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<IteratorResult> {
        self.next_cp?;

        let cp1 = self.next_cp.unwrap();
        let gb1 = self.next_gb;
        self.i = self.next_cp_it.index();
        self.next_cp = self.next_cp_it.next_cp();

        if let Some(cp2) = self.next_cp {
            self.next_gb = get::grapheme_break(cp2);
            let is_break = precomputed_grapheme_break(gb1, self.next_gb, &mut self.state);
            Some(IteratorResult { cp: cp1, is_break })
        } else {
            Some(IteratorResult {
                cp: cp1,
                is_break: true,
            })
        }
    }

    pub fn peek(&self) -> Option<IteratorResult> {
        let mut it = self.clone();
        it.next()
    }

    pub fn next_break(&mut self) -> Option<usize> {
        while let Some(result) = self.next() {
            if result.is_break {
                return Some(self.i);
            }
        }
        None
    }
}

/// `BreakState` `enum(u3)` — 5 variants, discriminants pinned to grapheme.zig:157-163.
/// Indexes the precompute's `state` dimension (`state*324 + …`).
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

const fn is_indic_conjunct_break_extend(gb: GraphemeBreak) -> bool {
    matches!(
        gb,
        GraphemeBreak::IndicConjunctBreakExtend | GraphemeBreak::Zwj
    )
}

const fn is_extend(gb: GraphemeBreak) -> bool {
    matches!(
        gb,
        GraphemeBreak::Zwnj
            | GraphemeBreak::IndicConjunctBreakExtend
            | GraphemeBreak::IndicConjunctBreakLinker
    )
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
                | GraphemeBreak::ExtendedPictographic => {}

                _ => *state = BreakState::Default,
            }

            match gb2 {
                // Keep state if in possibly valid sequence
                GraphemeBreak::IndicConjunctBreakExtend // extend
                | GraphemeBreak::IndicConjunctBreakLinker // extend
                | GraphemeBreak::Zwnj // extend
                | GraphemeBreak::Zwj
                | GraphemeBreak::ExtendedPictographic => {}

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
        //std.debug.assert(state.* == .default);

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

    // GB11: Emoji ZWJ sequence
    if matches!(gb1, GraphemeBreak::ExtendedPictographic) {
        // start of sequence:

        // In normal operation, we'll be in this state, but
        // buildGraphemeBreakTable iterates all states.
        // std.debug.assert(state.* == .default);

        if is_extend(gb2) || matches!(gb2, GraphemeBreak::Zwj) {
            *state = BreakState::ExtendedPictographic;
            return false;
        }
        // else, not an Emoji ZWJ sequence
    } else if matches!(*state, BreakState::ExtendedPictographic) {
        // continue or end sequence:

        if is_extend(gb1) && (is_extend(gb2) || matches!(gb2, GraphemeBreak::Zwj)) {
            // continue extend* ZWJ sequence
            return false;
        } else if matches!(gb1, GraphemeBreak::Zwj)
            && matches!(gb2, GraphemeBreak::ExtendedPictographic)
        {
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

// The precompute: 5 BreakState x 18 GraphemeBreak x 18 GraphemeBreak = 1620 entries.
const N_GB: usize = GraphemeBreak::COUNT; // 18
const N_GB_2: usize = N_GB * N_GB; // 324
const N_STATE: usize = BreakState::COUNT; // 5
const N: usize = N_STATE * N_GB_2; // 1620

// grapheme.zig:514 asserts `@sizeOf(table) == 1620` — there the packed `Result`
// (bool + u3 state) is 1 byte, so bytes == entries. We pin the entry COUNT instead.
const _: () = assert!(N == 1620);

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
    use crate::uucode::utf8;

    #[test]
    fn iterator_next_peek() {
        let str = "👩‍🍼😀";
        let mut it = Iterator::init(utf8::Iterator::init(str.as_bytes()));
        assert_eq!(it.i, 0);

        let mut result = it.peek();
        assert_eq!(it.i, 0);
        assert!(result.is_some());
        assert_eq!(result.unwrap().cp, 0x1F469); // 👩
        assert!(!result.unwrap().is_break);

        result = it.next();
        assert_eq!(it.i, 4);
        assert!(result.is_some());
        assert_eq!(result.unwrap().cp, 0x1F469); // 👩
        assert!(!result.unwrap().is_break);

        result = it.next();
        assert!(result.is_some());
        assert_eq!(result.unwrap().cp, 0x200D); // Zero width joiner
        assert!(!result.unwrap().is_break);

        result = it.next();
        assert!(result.is_some());
        assert_eq!(result.unwrap().cp, 0x1F37C); // 🍼
        assert!(result.unwrap().is_break);

        result = it.peek();
        assert!(result.is_some());
        assert_eq!(result.unwrap().cp, 0x1F600); // 😀
        assert!(result.unwrap().is_break);

        result = it.next();
        assert_eq!(it.i, str.len());
        assert!(result.is_some());
        assert_eq!(result.unwrap().cp, 0x1F600); // 😀
        assert!(result.unwrap().is_break);

        assert!(it.peek().is_none());
        assert!(it.next().is_none());
        assert!(it.next().is_none());
    }

    #[test]
    fn iterator_next_break() {
        let str = "👩‍🍼😀";
        let mut it = Iterator::init(utf8::Iterator::init(str.as_bytes()));

        assert_eq!(it.next_break(), Some(11));
        assert_eq!(it.i, 11);

        let result = it.peek();
        assert!(result.is_some());
        assert_eq!(result.unwrap().cp, 0x1F600); // 😀
        assert!(result.unwrap().is_break);

        let start_i = it.i;
        assert_eq!(it.next_break(), Some(str.len()));
        assert_eq!(it.i, str.len());
        assert_eq!(&str.as_bytes()[start_i..it.i], "😀".as_bytes());
        assert_eq!(it.next_break(), None);
        assert_eq!(it.next_break(), None);
    }

    // ---- GraphemeBreakTest.txt conformance (design §7/§10.1) ----

    fn test_parse_cp(s: &str) -> u32 {
        u32::from_str_radix(s, 16).unwrap()
    }

    /// `Ucd.trim` — strip a `#`-comment then whitespace-trim.
    fn test_trim(line: &str) -> &str {
        match line.find('#') {
            Some(idx) => line[..idx].trim_matches([' ', '\t']),
            None => line.trim_matches([' ', '\t']),
        }
    }

    fn test_grapheme_break(get_actual: fn(u32, u32, &mut BreakState) -> bool) {
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
        for (i, line) in content.split('\n').enumerate() {
            let line_num = i + 1;
            let trimmed = test_trim(line);
            if trimmed.is_empty() {
                continue;
            }

            let mut parts = trimmed.split(' ');
            let start = parts.next().unwrap();
            assert_eq!(start, "÷");

            cases += 1;
            let mut state = BreakState::Default;
            let mut cp1 = test_parse_cp(parts.next().unwrap());
            let mut expected_str = parts.next().unwrap();
            let mut cp2 = test_parse_cp(parts.next().unwrap());
            let mut next_expected_str = parts.next().unwrap();

            loop {
                let gb1 = get::grapheme_break(cp1);
                let gb2 = get::grapheme_break(cp2);
                let expected = expected_str == "÷";
                let actual = get_actual(cp1, cp2, &mut state);
                assert!(expected || expected_str == "×");
                if actual != expected {
                    eprintln!(
                        "line={line_num} cp1={cp1:x}, cp2={cp2:x}: gb1={gb1:?}, gb2={gb2:?}, \
                         state={state:?}, expected={expected}, actual={actual}"
                    );
                    success = false;
                }

                if parts.clone().next().is_none() {
                    break;
                }

                cp1 = cp2;
                expected_str = next_expected_str;
                cp2 = test_parse_cp(parts.next().unwrap());
                next_expected_str = parts.next().unwrap();
            }

            assert_eq!(next_expected_str, "÷");
        }

        // UAX #29 `GraphemeBreakTest.txt` for Unicode 16.0.0 carries 1093 non-comment
        // cases. Pin a floor (not the exact number, so a legitimate Unicode bump does not
        // spuriously fail) that is high enough that a truncated or wrong corpus is caught.
        assert!(
            cases >= 1000,
            "UAX #29 conformance corpus drove only {cases} cases — expected >= 1000. \
             The GraphemeBreakTest.txt embed is empty, truncated or misparsed, which \
             would make this test vacuously green."
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
}
