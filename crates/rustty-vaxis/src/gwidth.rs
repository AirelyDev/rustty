//! `vaxis/src/gwidth.zig` — grapheme-width calculation over the vendored `uucode`
//! subset (VAXIS_C1B_DESIGN.md §8). The 11 ported tests pin EXACT widths: any
//! divergence is silently-wrong TUI layout (R1). Strings are `&[u8]` throughout
//! (zig-byteslice-to-rust-rawbytes), never a lossy `String`.

use crate::uucode;
use crate::uucode::types::{EastAsianWidth, GeneralCategory};

/// the method to use when calculating the width of a grapheme
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    Unicode,
    Wcwidth,
    NoZwj,
}

/// `std.unicode.utf8CodepointSequenceLength` — the UTF-8 byte length of a scalar
/// (gwidth uses this with a `catch 1` fallback for out-of-range values).
fn utf8_codepoint_sequence_length(cp: u32) -> usize {
    if cp < 0x80 {
        1
    } else if cp < 0x800 {
        2
    } else if cp < 0x10000 {
        3
    } else if cp <= 0x10FFFF {
        4
    } else {
        1 // Zig returns an error here; gwidth's `catch 1` maps it to 1.
    }
}

/// Calculate width from east asian width property and Unicode properties
fn eaw_to_width(cp: u32, eaw: EastAsianWidth) -> i16 {
    // Based on wcwidth implementation
    // Control characters
    if cp == 0 {
        return 0;
    }
    if cp < 32 || (0x7f..0xa0).contains(&cp) {
        return -1;
    }

    // Use general category for comprehensive zero-width detection
    let gc = uucode::get::general_category(cp);
    match gc {
        GeneralCategory::MarkNonspacing | GeneralCategory::MarkEnclosing => return 0,
        _ => {}
    }

    // Additional zero-width characters not covered by general category
    if cp == 0x00ad {
        return 0; // soft hyphen
    }
    if cp == 0x200b {
        return 0; // zero-width space
    }
    if cp == 0x200c {
        return 0; // zero-width non-joiner
    }
    if cp == 0x200d {
        return 0; // zero-width joiner
    }
    if cp == 0x2060 {
        return 0; // word joiner
    }
    if cp == 0x034f {
        return 0; // combining grapheme joiner
    }
    if cp == 0xfeff {
        return 0; // zero-width no-break space (BOM)
    }
    if (0x180b..=0x180d).contains(&cp) {
        return 0; // Mongolian variation selectors
    }
    if (0xfe00..=0xfe0f).contains(&cp) {
        return 0; // variation selectors
    }
    if (0xe0100..=0xe01ef).contains(&cp) {
        return 0; // Plane-14 variation selectors
    }

    // East Asian Width: fullwidth or wide = 2
    // ambiguous in East Asian context = 2, otherwise 1
    // halfwidth, narrow, or neutral = 1
    match eaw {
        EastAsianWidth::Fullwidth | EastAsianWidth::Wide => 2,
        _ => 1,
    }
}

/// returns the width of the provided string, as measured by the method chosen
pub fn gwidth(str: &[u8], method: Method) -> u16 {
    match method {
        Method::Unicode => {
            let mut total: u16 = 0;
            let mut grapheme_iter =
                uucode::grapheme::Iterator::init(uucode::utf8::Iterator::init(str));

            let mut grapheme_start: usize = 0;
            let mut prev_break: bool = true;

            while let Some(result) = grapheme_iter.next() {
                if prev_break && !result.is_break {
                    // Start of a new grapheme
                    let cp_len: usize = utf8_codepoint_sequence_length(result.cp);
                    grapheme_start = grapheme_iter.i - cp_len;
                }

                if result.is_break {
                    // End of a grapheme - calculate its width
                    let grapheme_end = grapheme_iter.i;
                    let grapheme_bytes = &str[grapheme_start..grapheme_end];

                    // Calculate grapheme width
                    let mut g_iter = uucode::utf8::Iterator::init(grapheme_bytes);
                    let mut width: i16 = 0;
                    let mut has_emoji_vs: bool = false;
                    let mut has_text_vs: bool = false;
                    let mut has_emoji_presentation: bool = false;
                    let mut ri_count: u8 = 0;

                    while let Some(cp) = g_iter.next() {
                        // Check for emoji variation selector (U+FE0F)
                        if cp == 0xfe0f {
                            has_emoji_vs = true;
                            continue;
                        }

                        // Check for text variation selector (U+FE0E)
                        if cp == 0xfe0e {
                            has_text_vs = true;
                            continue;
                        }

                        // Check if this codepoint has emoji presentation
                        if uucode::get::is_emoji_presentation(cp) {
                            has_emoji_presentation = true;
                        }

                        // Count regional indicators (for flag emojis)
                        if (0x1F1E6..=0x1F1FF).contains(&cp) {
                            ri_count += 1;
                        }

                        let eaw = uucode::get::east_asian_width(cp);
                        let w = eaw_to_width(cp, eaw);
                        // Take max of non-zero widths
                        if w > 0 && w > width {
                            width = w;
                        }
                    }

                    // Handle variation selectors and emoji presentation
                    if has_text_vs {
                        // Text presentation explicit - keep width as-is (usually 1)
                        width = width.max(1);
                    } else if has_emoji_vs || has_emoji_presentation || ri_count == 2 {
                        // Emoji presentation or flag pair - force width 2
                        width = width.max(2);
                    }

                    total += width.max(0) as u16;

                    grapheme_start = grapheme_end;
                }
                prev_break = result.is_break;
            }

            total
        }
        Method::Wcwidth => {
            let mut total: u16 = 0;
            let mut iter = uucode::utf8::Iterator::init(str);
            while let Some(cp) = iter.next() {
                let w: i16 = match cp {
                    // undo an override in zg for emoji skintone selectors
                    0x1f3fb..=0x1f3ff => 2,
                    _ => {
                        let eaw = uucode::get::east_asian_width(cp);
                        eaw_to_width(cp, eaw)
                    }
                };
                total += w.max(0) as u16;
            }
            total
        }
        Method::NoZwj => {
            // std.mem.splitSequence over U+200D (the 3-byte UTF-8 sequence).
            const ZWJ: [u8; 3] = [0xE2, 0x80, 0x8D];
            let mut result: u16 = 0;
            let mut start = 0;
            let mut i = 0;
            while i + ZWJ.len() <= str.len() {
                if str[i..i + ZWJ.len()] == ZWJ {
                    result += gwidth(&str[start..i], Method::Unicode);
                    i += ZWJ.len();
                    start = i;
                } else {
                    i += 1;
                }
            }
            result += gwidth(&str[start..], Method::Unicode);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gwidth_a() {
        assert_eq!(gwidth(b"a", Method::Unicode), 1);
        assert_eq!(gwidth(b"a", Method::Wcwidth), 1);
        assert_eq!(gwidth(b"a", Method::NoZwj), 1);
    }

    #[test]
    fn gwidth_emoji_with_zwj() {
        assert_eq!(gwidth("👩‍🚀".as_bytes(), Method::Unicode), 2);
        assert_eq!(gwidth("👩‍🚀".as_bytes(), Method::Wcwidth), 4);
        assert_eq!(gwidth("👩‍🚀".as_bytes(), Method::NoZwj), 4);
    }

    #[test]
    fn gwidth_emoji_with_vs16_selector() {
        assert_eq!(gwidth(b"\xE2\x9D\xA4\xEF\xB8\x8F", Method::Unicode), 2);
        assert_eq!(gwidth(b"\xE2\x9D\xA4\xEF\xB8\x8F", Method::Wcwidth), 1);
        assert_eq!(gwidth(b"\xE2\x9D\xA4\xEF\xB8\x8F", Method::NoZwj), 2);
    }

    #[test]
    fn gwidth_emoji_with_skin_tone_selector() {
        assert_eq!(gwidth("👋🏿".as_bytes(), Method::Unicode), 2);
        assert_eq!(gwidth("👋🏿".as_bytes(), Method::Wcwidth), 4);
        assert_eq!(gwidth("👋🏿".as_bytes(), Method::NoZwj), 2);
    }

    #[test]
    fn gwidth_zero_width_space() {
        assert_eq!(gwidth("\u{200B}".as_bytes(), Method::Unicode), 0);
        assert_eq!(gwidth("\u{200B}".as_bytes(), Method::Wcwidth), 0);
    }

    #[test]
    fn gwidth_zero_width_non_joiner() {
        assert_eq!(gwidth("\u{200C}".as_bytes(), Method::Unicode), 0);
        assert_eq!(gwidth("\u{200C}".as_bytes(), Method::Wcwidth), 0);
    }

    #[test]
    fn gwidth_combining_marks() {
        // Hebrew combining mark
        assert_eq!(gwidth("\u{05B0}".as_bytes(), Method::Unicode), 0);
        // Devanagari combining mark
        assert_eq!(gwidth("\u{093C}".as_bytes(), Method::Unicode), 0);
    }

    #[test]
    fn gwidth_flag_emoji_regional_indicators() {
        // US flag 🇺🇸
        assert_eq!(gwidth("🇺🇸".as_bytes(), Method::Unicode), 2);
        // UK flag 🇬🇧
        assert_eq!(gwidth("🇬🇧".as_bytes(), Method::Unicode), 2);
    }

    #[test]
    fn gwidth_text_variation_selector() {
        // U+2764 (heavy black heart) + U+FE0E (text variation selector)
        // Should be width 1 with text presentation
        assert_eq!(gwidth("❤︎".as_bytes(), Method::Unicode), 1);
    }

    #[test]
    fn gwidth_keycap_sequence() {
        // Digit 1 + U+FE0F + U+20E3 (combining enclosing keycap)
        // Should be width 2
        assert_eq!(gwidth("1️⃣".as_bytes(), Method::Unicode), 2);
    }

    #[test]
    fn gwidth_base_letter_with_combining_mark() {
        // 'a' + combining acute accent (NFD form)
        // Should be width 1 (combining mark is zero-width)
        assert_eq!(gwidth("á".as_bytes(), Method::Unicode), 1);
    }

    /// ADDED (design §8.2) — pins the DerivedEastAsianWidth `@missing: …; Wide`
    /// mechanism the 11 ported tests + the GraphemeBreakTest corpus never exercise.
    /// (a) assigned CJK ideographs are Wide via explicit `W` rows + general_category;
    /// (b) the @missing-ONLY unassigned holes are Wide SOLELY via `@missing` — they
    /// become Neutral (width 1) if the §3.5.1 comment-strip-first trap drops @missing.
    #[test]
    fn gwidth_missing_wide_anchor() {
        // (a) Assigned CJK ideographs — Wide via explicit `W` rows + general_category Lo.
        assert_eq!(gwidth("\u{4E00}".as_bytes(), Method::Unicode), 2); // CJK UNIFIED IDEOGRAPH-4E00
        assert_eq!(gwidth("\u{3400}".as_bytes(), Method::Unicode), 2); // CJK Ext A, First
        assert_eq!(gwidth("\u{20000}".as_bytes(), Method::Unicode), 2); // CJK Ext B, First (astral)

        // (b) @missing-ONLY holes — unassigned (gc Cn), Wide IFF `@missing` is honored.
        assert_eq!(gwidth("\u{FA6E}".as_bytes(), Method::Unicode), 2); // hole in F900..FAFF
        assert_eq!(gwidth("\u{2A6E0}".as_bytes(), Method::Unicode), 2); // hole in 20000..2FFFD
        assert_eq!(gwidth("\u{3134B}".as_bytes(), Method::Unicode), 2); // hole in 30000..3FFFD

        // Dual negative pin — @missing does not leak beyond its ranges: an unassigned
        // non-CJK cp stays Neutral (width 1).
        assert_eq!(gwidth("\u{0378}".as_bytes(), Method::Unicode), 1); // Cn, eaw Neutral
    }
}
