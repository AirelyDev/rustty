use crate::lut::{Context, Generator};
use crate::props_uucode::MAX_CODE_POINT;

/// Generator binary to generate the lookup tables and output to stdout.
///
/// PARITY: the reference `main` arena-allocates, runs `lut.Generator` over
/// `uucode.get(.is_symbol, cp)`, and writes the table out as Zig via
/// `writeZig`. rustty ships the already-generated table embedded in
/// `generated::symbols_data` (R1; R3: no uucode dependency), so this
/// regenerates it from the embedded symbols table and emits the same Zig
/// source — used to re-derive / verify the embedded artifact.
#[allow(dead_code)]
pub fn main() {
    struct Ctx;
    impl Context<bool> for Ctx {
        fn get(&self, cp: u32) -> bool {
            if cp > MAX_CODE_POINT {
                false
            } else {
                // PARITY: oracle value is `uucode.get(.is_symbol, cp)`; rustty
                // reads the identical embedded uucode-derived value (R3).
                crate::symbols_table::table().get(cp)
            }
        }

        fn eql(&self, a: &bool, b: &bool) -> bool {
            a == b
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
    use crate::symbols_table::table;

    /// `is_symbol(cp)` — the reference symbol rule with the out-of-range guard.
    ///
    /// PARITY: mirrors the reference `main`'s `get` closure: `false` for
    /// `cp > uucode.config.max_code_point`, otherwise `uucode.get(.is_symbol,
    /// cp)` — which rustty reads from the embedded table. Used here as the
    /// function-under-test (NOT a self-oracle: it is compared against curated
    /// independent ground truth below).
    fn is_symbol(cp: u32) -> bool {
        if cp > MAX_CODE_POINT {
            false
        } else {
            table().get(cp)
        }
    }

    // PARITY / METHODOLOGY: the reference `test "unicode symbols: tables match
    // uucode"` compares the generated table against the LIVE `uucode` oracle —
    // an INDEPENDENT source — over the full code-point range. R3 forbids a
    // `uucode` Cargo dep in-crate, so that exhaustive full-range differential is
    // reproduced as a permanent harness gate (ORACLE #2):
    //   .port/phase-0/harness/unicode/  (run_unicode_diff.sh; G4-uni in ci.sh)
    // which dumps rustty's embedded symbols table AND the live uucode oracle
    // over every cp 0..=0x10FFFF and asserts 0 diffs (confirmed: 0 diffs).
    //
    // This in-crate test is the genuine, NON-tautological complement: a CURATED
    // spot-check of `is_symbol` against hand-verified ground truth (NOT read from
    // rustty's own table). Together the curated spot-check + the exhaustive
    // harness differential give coverage >= the reference
    // (DEFINITION_OF_PARITY §Methodology). See the harness README.
    //
    // Ground truth is the reference `computeIsSymbol` rule
    // (ghostty src/build/uucode_config.zig): `is_symbol` iff
    //   general_category == other_private_use  OR  block in {
    //     arrows, dingbats, emoticons, miscellaneous_symbols,
    //     enclosed_alphanumerics, enclosed_alphanumeric_supplement,
    //     miscellaneous_symbols_and_pictographs, transport_and_map_symbols }.
    // Each codepoint below is in (or out of) those blocks per the Unicode
    // standard's block assignments (UAX/Blocks.txt) and PropList general
    // categories.
    #[test]
    fn unicode_symbols_tables_match_uucode() {
        // (cp, expected_is_symbol, note)
        let cases: &[(u32, bool, &str)] = &[
            // --- arrows (U+2190..U+21FF) ---
            (0x2190, true, "U+2190 LEFTWARDS ARROW (Arrows)"),
            (0x2192, true, "U+2192 RIGHTWARDS ARROW (Arrows)"),
            (0x21FF, true, "U+21FF Arrows block end"),
            // --- dingbats (U+2700..U+27BF) ---
            (0x2702, true, "U+2702 BLACK SCISSORS (Dingbats)"),
            (0x2764, true, "U+2764 HEAVY BLACK HEART (Dingbats)"),
            // --- miscellaneous symbols (U+2600..U+26FF) ---
            (0x2600, true, "U+2600 BLACK SUN WITH RAYS (Misc Symbols)"),
            (0x2622, true, "U+2622 RADIOACTIVE SIGN (Misc Symbols)"),
            // --- enclosed alphanumerics (U+2460..U+24FF) ---
            (
                0x2460,
                true,
                "U+2460 CIRCLED DIGIT ONE (Enclosed Alphanumerics)",
            ),
            // --- enclosed alphanumeric supplement (U+1F100..U+1F1FF) ---
            (
                0x1F150,
                true,
                "U+1F150 NEGATIVE CIRCLED A (Encl. Alphanumeric Suppl.)",
            ),
            // --- emoticons (U+1F600..U+1F64F) ---
            (0x1F600, true, "U+1F600 GRINNING FACE (Emoticons)"),
            (0x1F62D, true, "U+1F62D LOUDLY CRYING FACE (Emoticons)"),
            // --- misc symbols and pictographs (U+1F300..U+1F5FF) ---
            (
                0x1F300,
                true,
                "U+1F300 CYCLONE (Misc Symbols and Pictographs)",
            ),
            (
                0x1F4A9,
                true,
                "U+1F4A9 PILE OF POO (Misc Symbols and Pictographs)",
            ),
            // --- transport and map symbols (U+1F680..U+1F6FF) ---
            (0x1F680, true, "U+1F680 ROCKET (Transport and Map Symbols)"),
            (0x1F6A2, true, "U+1F6A2 SHIP (Transport and Map Symbols)"),
            // --- other_private_use (BMP PUA + supplementary PUA-A/B) ---
            (
                0xE000,
                true,
                "U+E000 PUA start (general_category other_private_use)",
            ),
            (0xF8FF, true, "U+F8FF PUA end (other_private_use)"),
            (0xF0000, true, "U+F0000 PUA-A start (other_private_use)"),
            // --- NON-symbols: letters / digits / CJK / kana / Indic / space ---
            (
                0x0041,
                false,
                "U+0041 'A' (Latin letter, not a symbol block)",
            ),
            (0x0039, false, "U+0039 '9' (digit, not a symbol block)"),
            (0x0020, false, "U+0020 SPACE"),
            (0x4E00, false, "U+4E00 CJK ideograph"),
            (0x3042, false, "U+3042 Hiragana A"),
            (0x0915, false, "U+0915 DEVANAGARI KA"),
            // U+2200 is a *math* symbol but in the Mathematical Operators block,
            // which is NOT in the is_symbol block set -> NOT a symbol.
            (
                0x2200,
                false,
                "U+2200 FOR ALL (Math Operators block; NOT a symbol)",
            ),
        ];

        for &(cp, expect, note) in cases {
            assert_eq!(is_symbol(cp), expect, "{note}");
        }

        // Out-of-range rule (cp > MAX_CODE_POINT == 0x10FFFF): the reference
        // returns `false`. Verify the boundary and beyond.
        for cp in [MAX_CODE_POINT + 1, 0x11_0000, 0x20_0000, u32::MAX] {
            assert!(!is_symbol(cp), "cp U+{cp:X} > MAX must not be a symbol");
        }
    }
}
