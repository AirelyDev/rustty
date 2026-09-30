use crate::lut::{Context, Generator};
use crate::props::{GraphemeBreakNoControl, Properties};

/// `uucode.config.max_code_point` — the largest code point uucode has data for.
///
/// PARITY: uucode pins this to `0x10FFFF` (the Unicode code space maximum).
pub const MAX_CODE_POINT: u32 = 0x10FFFF;

// PERF(port): Zig compiles callers whole-program so this per-codepoint lookup
// inlines under ReleaseFast; a non-generic cross-crate fn is un-inlinable
// without LTO — #[inline] restores the idiom.
#[inline]
pub fn get(cp: u32) -> Properties {
    if cp > MAX_CODE_POINT {
        return Properties {
            width: 1,
            width_zero_in_grapheme: true,
            grapheme_break: GraphemeBreakNoControl::Other,
            emoji_vs_base: false,
        };
    }

    // PARITY: the reference reads each field from `uucode` at runtime:
    //
    //     .width = uucode.get(.width, cp),
    //     .width_zero_in_grapheme = uucode.get(.wcwidth_zero_in_grapheme, cp),
    //     .grapheme_break = uucode.get(.grapheme_break_no_control, cp),
    //     .emoji_vs_base = uucode.get(.is_emoji_vs_base, cp),
    //
    // rustty embeds uucode's generated tables (R1; R3: no uucode dependency), so
    // `get` reads the identical uucode-derived values back out of the embedded
    // `props_table`. By construction this returns exactly what `uucode.get`
    // returns for every code point in range.
    crate::props_table::table().get(cp)
}

/// Generator binary to generate the lookup tables and output to stdout.
///
/// PARITY: the reference `main` arena-allocates, runs `lut.Generator` over
/// `props.get`, and writes the table out as Zig via `writeZig`. rustty ships
/// the already-generated table embedded in `generated::props_data` (R1), so
/// this regenerates it from `get` and emits the same Zig source — used to
/// re-derive / verify the embedded artifact.
#[allow(dead_code)]
pub fn main() {
    struct Ctx;
    impl Context<Properties> for Ctx {
        fn get(&self, cp: u32) -> Properties {
            get(cp)
        }

        fn eql(&self, a: &Properties, b: &Properties) -> bool {
            Properties::eql(*a, *b)
        }
    }

    let gen = Generator::new(Ctx);

    let t = gen.generate().unwrap();

    print!("{}", t.write_zig());
    // Use flush instead of end because stdout is a pipe when captured by
    // the build system, and pipes cannot be truncated (Windows returns
    // INVALID_PARAMETER, Linux returns EINVAL).
    use std::io::Write;
    std::io::stdout().flush().unwrap();

    // Uncomment when manually debugging to see our table sizes.
    // std.log.warn("stage1={} stage2={} stage3={}", .{
    //     t.stage1.len,
    //     t.stage2.len,
    //     t.stage3.len,
    // });
}

#[cfg(test)]
mod tests {
    use super::*;

    // PARITY / METHODOLOGY: the reference `test "unicode props: tables match
    // uucode"` compares the generated table against the LIVE `uucode` oracle —
    // an INDEPENDENT source — over the full code-point range. R3 forbids a
    // `uucode` Cargo dep in-crate, so that exhaustive full-range differential is
    // reproduced as a permanent harness gate (ORACLE #2):
    //   .port/phase-0/harness/unicode/  (run_unicode_diff.sh; G4-uni in ci.sh)
    // which dumps rustty's embedded tables AND the live uucode oracle over every
    // cp 0..=0x10FFFF and asserts 0 diffs (confirmed: 0 diffs).
    //
    // This in-crate test is the genuine, NON-tautological complement: a CURATED
    // spot-check of `get` against hand-verified ground truth from the Unicode
    // standard (NOT read from rustty's own table). Together the curated
    // spot-check + the exhaustive harness differential give coverage >= the
    // reference (DEFINITION_OF_PARITY §Methodology). See the harness README.
    //
    // Expected values are sourced independently:
    //  - width: East Asian Width (UAX #11) + Ghostty's clamp to [0,2] and the
    //    uucode_config.zig `computeWidth` rule (width 0 iff zero-in-grapheme AND
    //    not an emoji modifier AND grapheme_break != prepend, else min(2, std)).
    //  - width_zero_in_grapheme: uucode `wcwidth_zero_in_grapheme`.
    //  - grapheme_break: Grapheme_Cluster_Break (UAX #29) folded to uucode's
    //    GraphemeBreakNoControl, incl. Indic conjunct-break + emoji classes.
    //  - emoji_vs_base: emoji-VS base set (digits/#/* take keycap VS16).
    //  - out-of-range (cp > MAX_CODE_POINT): reference rule width=1, wzg=true,
    //    grapheme_break=Other, emoji_vs_base=false.
    #[test]
    fn unicode_props_tables_match_uucode() {
        use GraphemeBreakNoControl as G;

        // (cp, width, width_zero_in_grapheme, grapheme_break, emoji_vs_base, note)
        let cases: &[(u32, u8, bool, GraphemeBreakNoControl, bool, &str)] = &[
            // --- ASCII / Latin: width 1, no special break ---
            (0x0041, 1, false, G::Other, false, "U+0041 'A' ASCII letter"),
            (0x007A, 1, false, G::Other, false, "U+007A 'z' ASCII letter"),
            (0x0020, 1, false, G::Other, false, "U+0020 SPACE"),
            (0x007E, 1, false, G::Other, false, "U+007E '~' tilde"),
            // ASCII digits are emoji-VS bases (keycap 0..9 via VS16).
            (
                0x0039,
                1,
                false,
                G::Other,
                true,
                "U+0039 '9' digit (emoji_vs_base)",
            ),
            (0x0023, 1, false, G::Other, true, "U+0023 '#' (keycap base)"),
            (0x002A, 1, false, G::Other, true, "U+002A '*' (keycap base)"),
            // --- control / tab: width 0 (zero-in-grapheme, Other break) ---
            (0x0000, 0, true, G::Other, false, "U+0000 NUL"),
            (0x0009, 0, true, G::Other, false, "U+0009 TAB"),
            // --- wide CJK / Hiragana: width 2 ---
            (
                0x4E00,
                2,
                false,
                G::Other,
                false,
                "U+4E00 CJK ideograph one",
            ),
            (0x3042, 2, false, G::Other, false, "U+3042 Hiragana A"),
            (
                0x2329,
                2,
                false,
                G::Other,
                false,
                "U+2329 LEFT-POINTING ANGLE BRACKET (wide)",
            ),
            // --- fullwidth forms: width 2 ---
            (
                0xFF01,
                2,
                false,
                G::Other,
                false,
                "U+FF01 FULLWIDTH EXCLAMATION MARK",
            ),
            (
                0xFF21,
                2,
                false,
                G::Other,
                false,
                "U+FF21 FULLWIDTH LATIN A",
            ),
            // --- combining / zero-width: width 0, zero-in-grapheme ---
            (
                0x0301,
                0,
                true,
                G::IndicConjunctBreakExtend,
                false,
                "U+0301 COMBINING ACUTE",
            ),
            (
                0x0300,
                0,
                true,
                G::IndicConjunctBreakExtend,
                false,
                "U+0300 COMBINING GRAVE",
            ),
            (0x200B, 0, true, G::Other, false, "U+200B ZERO WIDTH SPACE"),
            (0x200D, 0, true, G::Zwj, false, "U+200D ZWJ"),
            (
                0xFE0F,
                0,
                true,
                G::IndicConjunctBreakExtend,
                false,
                "U+FE0F VS16",
            ),
            // --- emoji & emoji modifier (modifier keeps width 2 despite wzg) ---
            (
                0x1F600,
                2,
                false,
                G::ExtendedPictographic,
                false,
                "U+1F600 GRINNING FACE",
            ),
            (
                0x1F3FF,
                2,
                true,
                G::EmojiModifier,
                false,
                "U+1F3FF EMOJI MODIFIER FITZPATRICK-6",
            ),
            // --- regional indicator (flags) ---
            (
                0x1F1E6,
                2,
                false,
                G::RegionalIndicator,
                false,
                "U+1F1E6 REGIONAL INDICATOR A",
            ),
            // --- prepend special case: wzg=true but width stays 1 ---
            (
                0x0600,
                1,
                true,
                G::Prepend,
                false,
                "U+0600 ARABIC NUMBER SIGN (prepend)",
            ),
            // --- Hangul jamo / syllable break classes ---
            (
                0x1100,
                2,
                false,
                G::L,
                false,
                "U+1100 HANGUL CHOSEONG KIYEOK (L)",
            ),
            (
                0x1160,
                0,
                true,
                G::V,
                false,
                "U+1160 HANGUL JUNGSEONG FILLER (V)",
            ),
            (
                0x11A8,
                0,
                true,
                G::T,
                false,
                "U+11A8 HANGUL JONGSEONG KIYEOK (T)",
            ),
            (
                0xAC00,
                2,
                false,
                G::Lv,
                false,
                "U+AC00 HANGUL SYLLABLE GA (LV)",
            ),
            (
                0xAC01,
                2,
                false,
                G::Lvt,
                false,
                "U+AC01 HANGUL SYLLABLE GAG (LVT)",
            ),
            // --- Indic conjunct break: consonant + linker (virama) ---
            (
                0x0915,
                1,
                false,
                G::IndicConjunctBreakConsonant,
                false,
                "U+0915 DEVANAGARI KA",
            ),
            (
                0x094D,
                0,
                true,
                G::IndicConjunctBreakLinkerExtend,
                false,
                "U+094D DEVANAGARI VIRAMA",
            ),
            // --- Unicode 18 (uucode 9d555245): InCB=Linker with GCB=Other ---
            (
                0x1CF5,
                1,
                false,
                G::IndicConjunctBreakLinkerOther,
                false,
                "U+1CF5 VEDIC SIGN JIHVAMULIYA",
            ),
            (
                0x11A3A,
                1,
                false,
                G::IndicConjunctBreakLinkerOther,
                false,
                "U+11A3A ZANABAZAR SQUARE CLUSTER-INITIAL LETTER RA",
            ),
            // --- Unicode 18: Extend growth and new wide ranges ---
            (
                0x05C8,
                0,
                true,
                G::IndicConjunctBreakExtend,
                false,
                "U+05C8 HEBREW POINT SHEVA NA MUDGASH (new in 18.0)",
            ),
            (
                0x18E00,
                2,
                false,
                G::Other,
                false,
                "U+18E00 JURCHEN CHARACTER (first)",
            ),
            (
                0x3D000,
                2,
                false,
                G::Other,
                false,
                "U+3D000 SEAL CHARACTER (first)",
            ),
            (
                0x1F6D9,
                2,
                false,
                G::ExtendedPictographic,
                false,
                "U+1F6D9 LIGHTHOUSE (E18.0)",
            ),
        ];

        for &(cp, w, wzg, gb, evb, note) in cases {
            let p = get(cp);
            assert_eq!(p.width, w, "{note}: width");
            assert_eq!(
                p.width_zero_in_grapheme, wzg,
                "{note}: width_zero_in_grapheme"
            );
            assert_eq!(p.grapheme_break, gb, "{note}: grapheme_break");
            assert_eq!(p.emoji_vs_base, evb, "{note}: emoji_vs_base");
        }

        // Out-of-range rule (cp > MAX_CODE_POINT == 0x10FFFF): the reference
        // returns width=1, width_zero_in_grapheme=true, grapheme_break=Other,
        // emoji_vs_base=false. Verify the boundary and beyond.
        for cp in [MAX_CODE_POINT + 1, 0x11_0000, 0x20_0000, u32::MAX] {
            let p = get(cp);
            assert_eq!(p.width, 1, "cp U+{cp:X} > MAX: width");
            assert!(
                p.width_zero_in_grapheme,
                "cp U+{cp:X} > MAX: width_zero_in_grapheme"
            );
            assert_eq!(
                p.grapheme_break,
                GraphemeBreakNoControl::Other,
                "cp U+{cp:X} > MAX: grapheme_break"
            );
            assert!(!p.emoji_vs_base, "cp U+{cp:X} > MAX: emoji_vs_base");
        }
    }
}
