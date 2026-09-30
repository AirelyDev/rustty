//! The generated stage1/stage2 tables + the `Data` value + the 2-stage lookup
//! (VAXIS_C1B_DESIGN.md §3.2). `STAGE1`/`STAGE2` are emitted by `build.rs` into
//! `OUT_DIR/uucode_tables.rs` from the vendored UCD 18.0.0 corpus; this module wraps
//! them with the typed `Data` decode. Statics only ⇒ Miri-clean (R6).

use super::types::{EastAsianWidth, GeneralCategory, GraphemeBreak};

// STAGE1: [u32; 4352] (block offsets), STAGE2: [u16; _] (packed Data blocks).
include!(concat!(env!("OUT_DIR"), "/uucode_tables.rs"));

/// The runtime per-code-point record — the four fields vaxis's width/grapheme surface
/// reads. In uucode this is the *unpacked* stage2 element; here it is decoded on demand
/// from the packed stage2 `u16` (design §3.3 — representation is free, values are pinned).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Data {
    pub east_asian_width: EastAsianWidth,
    pub grapheme_break: GraphemeBreak,
    pub general_category: GeneralCategory,
    pub is_emoji_presentation: bool,
}

/// The 2-stage lookup (`get.zig::data`, two-stage branch): `stage2[stage1[cp>>8] + (cp&0xFF)]`.
/// `cp` must be a valid scalar (`<= 0x10FFFF`); callers pass decoded UTF-8 scalars.
pub fn data(cp: u32) -> Data {
    let stage1_idx = (cp >> 8) as usize;
    let stage2_idx = (cp & 0xFF) as usize;
    let packed = STAGE2[STAGE1[stage1_idx] as usize + stage2_idx];
    Data {
        east_asian_width: EastAsianWidth::from_u8((packed & 0x7) as u8),
        grapheme_break: GraphemeBreak::from_u8(((packed >> 3) & 0x1F) as u8),
        general_category: GeneralCategory::from_u8(((packed >> 8) & 0x1F) as u8),
        is_emoji_presentation: (packed >> 13) & 1 != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exhaustive self-check (design §10.4): every code point's 2-stage lookup is
    /// in-bounds and decodes to a valid enum discriminant (the packed `u16` fields
    /// never hit `from_u8`'s `unreachable`). Complements the build-time losslessness
    /// assertion in `build.rs`.
    #[test]
    fn exhaustive_decode() {
        assert_eq!(STAGE1.len(), 0x110000 / 256);
        for cp in 0..=0x10FFFFu32 {
            // Panics (via from_u8 `unreachable`) if any stored discriminant is corrupt.
            let _ = data(cp);
        }
    }

    /// Spot-check a few code points' EAW/GC against known Unicode 18.0.0 facts.
    #[test]
    fn spot_check() {
        // 'a' — Ll, narrow.
        let a = data(0x61);
        assert_eq!(a.general_category, GeneralCategory::LetterLowercase);
        assert_eq!(a.east_asian_width, EastAsianWidth::Narrow);
        assert!(!a.is_emoji_presentation);

        // CJK UNIFIED IDEOGRAPH-4E00 — Lo, Wide.
        let cjk = data(0x4E00);
        assert_eq!(cjk.general_category, GeneralCategory::LetterOther);
        assert_eq!(cjk.east_asian_width, EastAsianWidth::Wide);

        // U+200D ZERO WIDTH JOINER — Cf, grapheme_break Zwj.
        let zwj = data(0x200D);
        assert_eq!(zwj.general_category, GeneralCategory::OtherFormat);
        assert_eq!(zwj.grapheme_break, GraphemeBreak::Zwj);

        // U+200C ZERO WIDTH NON-JOINER — grapheme_break Zwnj.
        assert_eq!(data(0x200C).grapheme_break, GraphemeBreak::Zwnj);

        // U+1F469 WOMAN — Emoji_Modifier_Base (so, in uucode 9d555245,
        // `emoji_modifier_base` rather than `extended_pictographic`), emoji presentation, Wide.
        let woman = data(0x1F469);
        assert_eq!(woman.grapheme_break, GraphemeBreak::EmojiModifierBase);
        assert!(woman.is_emoji_presentation);
        assert_eq!(woman.east_asian_width, EastAsianWidth::Wide);

        // U+1F600 GRINNING FACE — extended_pictographic, not a modifier base.
        assert_eq!(
            data(0x1F600).grapheme_break,
            GraphemeBreak::ExtendedPictographic
        );

        // U+1F3FF EMOJI MODIFIER FITZPATRICK TYPE-6 — Emoji_Modifier ⇒ `emoji_modifier`
        // (uucode 0.1.0 derived it from its InCB Extend as `indic_conjunct_break_extend`).
        assert_eq!(data(0x1F3FF).grapheme_break, GraphemeBreak::EmojiModifier);

        // U+1F1FA REGIONAL INDICATOR SYMBOL LETTER U — regional_indicator.
        assert_eq!(
            data(0x1F1FA).grapheme_break,
            GraphemeBreak::RegionalIndicator
        );

        // @missing-only hole U+FA6E (Cn, Wide solely via the DerivedEastAsianWidth
        // `@missing: F900..FAFF; Wide` directive — design §3.5.1/§8.2).
        let hole = data(0xFA6E);
        assert_eq!(hole.general_category, GeneralCategory::OtherNotAssigned);
        assert_eq!(hole.east_asian_width, EastAsianWidth::Wide);

        // Unicode 18.0.0 (uucode 9d555245): U+1CF5 VEDIC SIGN JIHVAMULIYA is InCB=Linker
        // with GCB=Other ⇒ `indic_conjunct_break_linker_other`; U+094D DEVANAGARI VIRAMA
        // stays the extend-side linker; U+18E00 (Jurchen) is Wide; U+1F6D9 LIGHTHOUSE is an
        // E18.0 emoji-presentation extended pictographic.
        assert_eq!(
            data(0x1CF5).grapheme_break,
            GraphemeBreak::IndicConjunctBreakLinkerOther
        );
        assert_eq!(
            data(0x094D).grapheme_break,
            GraphemeBreak::IndicConjunctBreakLinkerExtend
        );
        assert_eq!(data(0x18E00).east_asian_width, EastAsianWidth::Wide);
        let lighthouse = data(0x1F6D9);
        assert_eq!(lighthouse.east_asian_width, EastAsianWidth::Wide);
        assert!(lighthouse.is_emoji_presentation);
        assert_eq!(
            lighthouse.grapheme_break,
            GraphemeBreak::ExtendedPictographic
        );
    }
}
