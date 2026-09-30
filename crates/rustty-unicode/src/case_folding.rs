//! uucode's `case_folding_full` field, the one ghostty's `runtime` uucode
//! table declares (`src/build/uucode_config.zig:31-38`) for
//! `input/Binding.zig` `foldedCodepoint` (S65-F1). The data is
//! `generated/case_folding_data.rs`; see its header for provenance.

use crate::generated::case_folding_data::CASE_FOLDING_FULL;

/// `uucode.get(.case_folding_full, cp).with(&buffer, cp)`: the full case
/// folding of `cp` as up to three code points, and how many there are. A code
/// point with no folding folds to itself (`with` returns `[cp]`).
pub fn case_folding_full(cp: u32) -> ([u32; 3], usize) {
    match CASE_FOLDING_FULL.binary_search_by_key(&cp, |&(k, _)| k) {
        Ok(i) => {
            let m = CASE_FOLDING_FULL[i].1;
            let len = m.iter().take_while(|&&c| c != 0).count();
            (m, len)
        }
        Err(_) => ([cp, 0, 0], 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// uucode `src/root.zig` test "case_folding_full": "ß (U+00DF) maps to
    /// "ss" (0x0073, 0x0073)".
    #[test]
    fn case_folding_full_sharp_s() {
        let (mapping, len) = case_folding_full(0x00DF);
        assert_eq!(2, len);
        assert_eq!(0x0073, mapping[0]);
        assert_eq!(0x0073, mapping[1]);
    }

    /// Spot checks against `CaseFolding-18.0.0.txt`: a C entry, a
    /// three-code-point F entry, a code point whose only folding is S (so full
    /// folding leaves it alone), one whose only folding is T, and one with no
    /// entry at all.
    #[test]
    fn case_folding_full_follows_the_ucd_statuses() {
        // 00C9; C; 00E9; # LATIN CAPITAL LETTER E WITH ACUTE
        assert_eq!(case_folding_full(0x00C9), ([0x00E9, 0, 0], 1));
        // 0390; F; 03B9 0308 0301; # GREEK SMALL LETTER IOTA WITH DIALYTIKA AND TONOS
        assert_eq!(case_folding_full(0x0390), ([0x03B9, 0x0308, 0x0301], 3));
        // 1E9E; F; 0073 0073; and 1E9E; S; 00DF; the F mapping wins.
        assert_eq!(case_folding_full(0x1E9E), ([0x0073, 0x0073, 0], 2));
        // 0130; F; 0069 0307; and 0130; T; 0069; T is not full folding.
        assert_eq!(case_folding_full(0x0130), ([0x0069, 0x0307, 0], 2));
        // 0049; T; 0131; only matters to Turkic folding. 'I' still has C.
        assert_eq!(case_folding_full(0x0049), ([0x0069, 0, 0], 1));
        // New in 18.0.0: A7DD; C; 0277; 1DF40; C; 1DF41; and 1DF95; F; 0073 0073; (with S 00DF).
        assert_eq!(case_folding_full(0xA7DD), ([0x0277, 0, 0], 1));
        assert_eq!(case_folding_full(0x1DF40), ([0x1DF41, 0, 0], 1));
        assert_eq!(case_folding_full(0x1DF95), ([0x0073, 0x0073, 0], 2));
        // No entry: folds to itself.
        assert_eq!(case_folding_full(0x00E9), ([0x00E9, 0, 0], 1));
        assert_eq!(case_folding_full(0x10FFFF), ([0x10FFFF, 0, 0], 1));
    }

    /// The table is sorted (the lookup binary-searches it) and has the entry
    /// counts its header records: 1501 C + 105 F = 1606.
    #[test]
    fn case_folding_table_is_sorted_and_complete() {
        assert_eq!(CASE_FOLDING_FULL.len(), 1606);
        assert!(CASE_FOLDING_FULL.windows(2).all(|w| w[0].0 < w[1].0));
        let multi = CASE_FOLDING_FULL.iter().filter(|(_, m)| m[1] != 0).count();
        assert_eq!(multi, 105);
    }
}
