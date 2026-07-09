//! CHUNK 1b table generator — a faithful, Zig-free port of uucode 0.1.0's
//! `src/build/Ucd.zig` (the UCD parsers) + `src/build/tables.zig` (the 256-block
//! stage1/stage2 packer), restricted to the four fields vaxis instantiates:
//! `east_asian_width`, `grapheme_break`, `general_category`, `is_emoji_presentation`
//! (VAXIS_C1B_DESIGN.md §3). It parses the vendored UCD 16.0.0 corpus under `ucd/`
//! and writes `uucode_tables.rs` into `OUT_DIR`, `include!`d by `src/uucode/tables.rs`.
//!
//! Representation freedom (design §3.3): the uucode table is *unpacked*, so there is
//! no canonical stage2 bit layout to reproduce — only the per-cp *values* must be
//! identical. We therefore store each `Data` as a 14-bit packed `u16`
//! (`eaw:3 | grapheme_break:5 | general_category:5 | is_emoji_presentation:1`) and
//! keep stage1 offsets as `u32` (avoids the Zig `u16` block-offset ceiling). The
//! block-dedup is a pure function of scan order, so the output is deterministic.
//!
//! std-only; NO build-dependencies (R4 leaf).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

const MAX_CODE_POINT: u32 = 0x10FFFF;
const N: usize = (MAX_CODE_POINT + 1) as usize;
// config.zig:5-6
const ZERO_WIDTH_NON_JOINER: u32 = 0x200C;
const ZERO_WIDTH_JOINER: u32 = 0x200D;

// ---- GeneralCategory discriminants (types.zig:6-37, order pinned) ----
const GC_CN: u8 = 29; // other_not_assigned (Cn) — the default

// ---- EastAsianWidth discriminants (types.zig:101-108) ----
const EAW_NEUTRAL: u8 = 0;
const EAW_WIDE: u8 = 3;

// ---- OriginalGraphemeBreak discriminants (types.zig:110-125) ----
const OGB_OTHER: u8 = 0;
const OGB_EXTEND: u8 = 5;

// ---- IndicConjunctBreak discriminants (types.zig:94-99) ----
const INCB_NONE: u8 = 0;
const INCB_LINKER: u8 = 1;
const INCB_CONSONANT: u8 = 2;
const INCB_EXTEND: u8 = 3;

// ---- GraphemeBreak discriminants (types.zig:127-150, order pinned) ----
const GB_OTHER: u8 = 0;
const GB_CONTROL: u8 = 1;
const GB_PREPEND: u8 = 2;
const GB_CR: u8 = 3;
const GB_LF: u8 = 4;
const GB_REGIONAL_INDICATOR: u8 = 5;
const GB_SPACING_MARK: u8 = 6;
const GB_L: u8 = 7;
const GB_V: u8 = 8;
const GB_T: u8 = 9;
const GB_LV: u8 = 10;
const GB_LVT: u8 = 11;
const GB_ZWJ: u8 = 12;
const GB_ZWNJ: u8 = 13;
const GB_EXTENDED_PICTOGRAPHIC: u8 = 14;
const GB_INDIC_CONJUNCT_BREAK_EXTEND: u8 = 15;
const GB_INDIC_CONJUNCT_BREAK_LINKER: u8 = 16;
const GB_INDIC_CONJUNCT_BREAK_CONSONANT: u8 = 17;

fn main() {
    let ucd = Path::new(env!("CARGO_MANIFEST_DIR")).join("ucd");
    // cargo:rerun-if-changed on each vendored UCD file (design §3.3).
    for rel in [
        "UnicodeData.txt",
        "extracted/DerivedEastAsianWidth.txt",
        "emoji/emoji-data.txt",
        "auxiliary/GraphemeBreakProperty.txt",
        "DerivedCoreProperties.txt",
    ] {
        println!("cargo:rerun-if-changed=ucd/{rel}");
    }
    println!("cargo:rerun-if-changed=build.rs");

    let read = |rel: &str| -> Vec<u8> {
        std::fs::read(ucd.join(rel)).unwrap_or_else(|e| panic!("read ucd/{rel}: {e}"))
    };

    let gc = parse_unicode_data(&read("UnicodeData.txt"));
    let eaw = parse_east_asian_width(&read("extracted/DerivedEastAsianWidth.txt"));
    let ogb = parse_grapheme_break(&read("auxiliary/GraphemeBreakProperty.txt"));
    let incb = parse_derived_core_properties(&read("DerivedCoreProperties.txt"));
    let (emoji_pres, ext_picto) = parse_emoji_data(&read("emoji/emoji-data.txt"));

    // Derive grapheme_break per cp, then pack Data into a 14-bit u16.
    let mut packed = vec![0u16; N];
    for cp in 0..N {
        let gb = derive_grapheme_break(cp as u32, ext_picto[cp], incb[cp], ogb[cp]);
        packed[cp] = pack(eaw[cp], gb, gc[cp], emoji_pres[cp]);
    }

    // 256-block dedup (tables.zig:1001-1014). Offsets assigned in scan order.
    let mut stage1: Vec<u32> = Vec::with_capacity(N / 256);
    let mut stage2: Vec<u16> = Vec::new();
    let mut block_map: HashMap<[u16; 256], u32> = HashMap::new();
    let mut block = [0u16; 256];
    let mut block_len = 0usize;
    for &p in &packed {
        block[block_len] = p;
        block_len += 1;
        if block_len == 256 {
            let off = *block_map.entry(block).or_insert_with(|| {
                let o = stage2.len() as u32;
                stage2.extend_from_slice(&block);
                o
            });
            stage1.push(off);
            block_len = 0;
        }
    }
    assert_eq!(block_len, 0);
    assert_eq!(stage1.len(), N / 256);

    // Exhaustive dedup self-check (design §10.4): the 2-stage lookup must be lossless
    // vs the flat `packed` array for every code point. Verified here where the flat
    // data exists (a runtime test re-verifies every cp decodes to a valid enum).
    for cp in 0..N {
        let off = stage1[cp >> 8] as usize + (cp & 0xFF);
        assert_eq!(
            stage2[off], packed[cp],
            "stage lookup mismatch at cp {cp:x}"
        );
    }

    emit(&stage1, &stage2);
}

// ---- shared parse helpers (Ucd.zig:106-142) ----

/// `parseCp` — hex u21 scalar (Ucd.zig:106-108).
fn parse_cp(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap_or_else(|_| panic!("parseCp({s:?})"))
}

/// `parseRange` — `AAAA..BBBB` or a single cp (Ucd.zig:110-119).
fn parse_range(s: &str) -> (u32, u32) {
    if let Some(idx) = s.find("..") {
        (parse_cp(&s[..idx]), parse_cp(&s[idx + 2..]))
    } else {
        let cp = parse_cp(s);
        (cp, cp)
    }
}

/// `trim` (Ucd.zig:137-142) — strip a `#`-comment (cut at the FIRST `#`) then
/// whitespace-trim. NOTE: applied to `# …` this yields the empty string.
fn trim_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => line[..idx].trim_matches([' ', '\t']),
        None => line.trim_matches([' ', '\t']),
    }
}

fn lines(bytes: &[u8]) -> impl Iterator<Item = &str> {
    // UCD files are ASCII/UTF-8; std lines split on '\n' like Zig's splitScalar.
    std::str::from_utf8(bytes)
        .expect("UCD file is valid UTF-8")
        .split('\n')
}

// ---- parseUnicodeData → general_category (Ucd.zig:144-311) ----

fn parse_unicode_data(bytes: &[u8]) -> Vec<u8> {
    let mut gc = vec![GC_CN; N];
    let mut next_cp: u32 = 0;
    // Only the general_category is retained from the range's First entry.
    let mut range_gc: Option<u8> = None;

    for line in lines(bytes) {
        let trimmed = trim_comment(line);
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split(';');
        let cp_str = parts.next().unwrap();
        let cp = parse_cp(cp_str);

        // Fill ranges or gaps.
        while next_cp < cp {
            gc[next_cp as usize] = range_gc.unwrap_or(GC_CN);
            next_cp += 1;
        }

        if let Some(rgc) = range_gc {
            // We're in a range; this entry marks the Last, with the same info.
            let name_field = parts.next().unwrap();
            assert!(name_field.ends_with("Last>"));
            gc[cp as usize] = rgc;
            range_gc = None;
            next_cp = cp + 1;
            continue;
        }

        let name_str = parts.next().unwrap(); // Field 1
        let general_category_str = parts.next().unwrap(); // Field 2
        let general_category = general_category_map(general_category_str);

        // Handle range entries with "First>" and "Last>".
        if name_str.ends_with("First>") {
            range_gc = Some(general_category);
        }

        gc[cp as usize] = general_category;
        next_cp = cp + 1;
    }

    // Fill any remaining gaps at the end with default values.
    for cp in next_cp..=MAX_CODE_POINT {
        gc[cp as usize] = GC_CN;
    }
    gc
}

fn general_category_map(s: &str) -> u8 {
    match s {
        "Lu" => 0,
        "Ll" => 1,
        "Lt" => 2,
        "Lm" => 3,
        "Lo" => 4,
        "Mn" => 5,
        "Mc" => 6,
        "Me" => 7,
        "Nd" => 8,
        "Nl" => 9,
        "No" => 10,
        "Pc" => 11,
        "Pd" => 12,
        "Ps" => 13,
        "Pe" => 14,
        "Pi" => 15,
        "Pf" => 16,
        "Po" => 17,
        "Sm" => 18,
        "Sc" => 19,
        "Sk" => 20,
        "So" => 21,
        "Zs" => 22,
        "Zl" => 23,
        "Zp" => 24,
        "Cc" => 25,
        "Cf" => 26,
        "Cs" => 27,
        "Co" => 28,
        "Cn" => 29,
        _ => panic!("Unknown general category: {s}"),
    }
}

// ---- parseEastAsianWidth (Ucd.zig:658-723; the @missing whitespace-trim trap, design §3.5.1) ----

fn parse_east_asian_width(bytes: &[u8]) -> Vec<u8> {
    let mut eaw = vec![EAW_NEUTRAL; N];
    for line in lines(bytes) {
        // WHITESPACE-only trim (NOT comment-strip) — load-bearing for @missing.
        let trimmed = line.trim_matches([' ', '\t']);
        if trimmed.is_empty() {
            continue;
        }

        // Handle @missing directives first, detected on the whitespace-trimmed line.
        if let Some(missing_line) = trimmed.strip_prefix("# @missing:") {
            let mut parts = missing_line.split(';');
            let cp_str = parts.next().unwrap().trim_matches([' ', '\t']);
            let width_str = parts.next().unwrap().trim_matches([' ', '\t']);
            let (start, end) = parse_range(cp_str);

            // Skip `Neutral` as it's the default.
            if width_str == "Neutral" {
                continue;
            }
            if width_str != "Wide" {
                panic!("Unknown @missing EastAsianWidth value: {width_str}");
            }
            for cp in start..=end {
                eaw[cp as usize] = EAW_WIDE;
            }
            continue;
        }

        // Handle regular entries (now comment-stripped).
        let data_line = trim_comment(trimmed);
        if data_line.is_empty() {
            continue;
        }
        let mut parts = data_line.split(';');
        let cp_str = parts.next().unwrap().trim_matches([' ', '\t']);
        let width_str = parts.next().unwrap().trim_matches([' ', '\t']);
        let (start, end) = parse_range(cp_str);
        let width = east_asian_width_map(width_str);
        for cp in start..=end {
            eaw[cp as usize] = width;
        }
    }
    eaw
}

fn east_asian_width_map(s: &str) -> u8 {
    match s {
        "F" => 1,  // fullwidth
        "H" => 2,  // halfwidth
        "W" => 3,  // wide
        "Na" => 4, // narrow
        "A" => 5,  // ambiguous
        "N" => 0,  // neutral
        _ => panic!("Unknown EastAsianWidth value: {s}"),
    }
}

// ---- parseGraphemeBreak → original_grapheme_break (Ucd.zig:734-766) ----

fn parse_grapheme_break(bytes: &[u8]) -> Vec<u8> {
    let mut gb = vec![OGB_OTHER; N];
    for line in lines(bytes) {
        let trimmed = trim_comment(line);
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split(';');
        let cp_str = parts.next().unwrap().trim_matches([' ', '\t']);
        let prop_str = parts.next().unwrap().trim_matches([' ', '\t']);
        let (start, end) = parse_range(cp_str);
        // unknown ⇒ .other
        let prop = grapheme_break_property_map(prop_str).unwrap_or(OGB_OTHER);
        for cp in start..=end {
            gb[cp as usize] = prop;
        }
    }
    gb
}

fn grapheme_break_property_map(s: &str) -> Option<u8> {
    Some(match s {
        "Prepend" => 1,
        "CR" => 2,
        "LF" => 3,
        "Control" => 4,
        "Extend" => 5,
        "Regional_Indicator" => 6,
        "SpacingMark" => 7,
        "L" => 8,
        "V" => 9,
        "T" => 10,
        "LV" => 11,
        "LVT" => 12,
        "ZWJ" => 13,
        _ => return None,
    })
}

// ---- parseDerivedCoreProperties → indic_conjunct_break (InCB only) (Ucd.zig:543-590) ----

fn parse_derived_core_properties(bytes: &[u8]) -> Vec<u8> {
    let mut incb = vec![INCB_NONE; N];
    for line in lines(bytes) {
        let trimmed = trim_comment(line);
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split(';');
        let cp_str = parts.next().unwrap().trim_matches([' ', '\t']);
        let property_str = parts.next().unwrap().trim_matches([' ', '\t']);
        // Only the `InCB` lines matter (design §3.5); ignore all other properties.
        if property_str != "InCB" {
            continue;
        }
        let value_str = parts
            .next()
            .map(|v| v.trim_matches([' ', '\t']))
            .unwrap_or("");
        let (start, end) = parse_range(cp_str);
        let value = match value_str {
            "Linker" => INCB_LINKER,
            "Consonant" => INCB_CONSONANT,
            "Extend" => INCB_EXTEND,
            _ => panic!("Unknown InCB value: {value_str}"),
        };
        for cp in start..=end {
            incb[cp as usize] = value;
        }
    }
    incb
}

// ---- parseEmojiData → is_emoji_presentation + is_extended_pictographic (Ucd.zig:784-823) ----

fn parse_emoji_data(bytes: &[u8]) -> (Vec<bool>, Vec<bool>) {
    let mut emoji_pres = vec![false; N];
    let mut ext_picto = vec![false; N];
    for line in lines(bytes) {
        let trimmed = trim_comment(line);
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split(';');
        let cp_str = parts.next().unwrap().trim_matches([' ', '\t']);
        let prop_str = parts.next().unwrap().trim_matches([' ', '\t']);
        let (start, end) = parse_range(cp_str);
        match prop_str {
            "Emoji_Presentation" => {
                for cp in start..=end {
                    emoji_pres[cp as usize] = true;
                }
            }
            "Extended_Pictographic" => {
                for cp in start..=end {
                    ext_picto[cp as usize] = true;
                }
            }
            // Known but unused emoji properties (we only store the two above).
            "Emoji" | "Emoji_Modifier" | "Emoji_Modifier_Base" | "Emoji_Component" => {}
            _ => panic!("Unknown EmojiData property: {prop_str}"),
        }
    }
    (emoji_pres, ext_picto)
}

// ---- grapheme_break derivation (tables.zig:927-971; design §3.4) ----

fn derive_grapheme_break(cp: u32, ext_picto: bool, incb: u8, ogb: u8) -> u8 {
    if ext_picto {
        // is_extended_pictographic ⇒ extended_pictographic (tables.zig:929-931).
        assert!(
            ogb == OGB_OTHER,
            "extended_pictographic cp {cp:x} has non-`other` original grapheme break"
        );
        return GB_EXTENDED_PICTOGRAPHIC;
    }
    match incb {
        INCB_NONE => {
            if ogb == OGB_EXTEND {
                // GraphemeBreak has no `extend`; it decomposes into zwnj + InCB
                // extend/linker (tables.zig:936-946).
                if cp == ZERO_WIDTH_NON_JOINER {
                    GB_ZWNJ
                } else {
                    panic!(
                        "Found an `extend` grapheme break that is Indic conjunct break `none` \
                         (and not zwnj): {cp:x}"
                    );
                }
            } else {
                // The `inline else` same-name fan-out (tables.zig:947-950),
                // made explicit + exhaustive (design §3.4).
                match ogb {
                    OGB_OTHER => GB_OTHER,
                    1 => GB_PREPEND,            // prepend
                    2 => GB_CR,                 // cr
                    3 => GB_LF,                 // lf
                    4 => GB_CONTROL,            // control
                    6 => GB_REGIONAL_INDICATOR, // regional_indicator
                    7 => GB_SPACING_MARK,       // spacing_mark
                    8 => GB_L,                  // l
                    9 => GB_V,                  // v
                    10 => GB_T,                 // t
                    11 => GB_LV,                // lv
                    12 => GB_LVT,               // lvt
                    13 => GB_ZWJ,               // zwj
                    _ => unreachable!("OriginalGraphemeBreak discriminant {ogb}"),
                }
            }
        }
        INCB_EXTEND => {
            if cp == ZERO_WIDTH_JOINER {
                GB_ZWJ
            } else {
                assert!(
                    ogb == OGB_EXTEND,
                    "InCB extend cp {cp:x} is not original `extend`"
                );
                GB_INDIC_CONJUNCT_BREAK_EXTEND
            }
        }
        INCB_LINKER => {
            assert!(
                ogb == OGB_EXTEND,
                "InCB linker cp {cp:x} is not original `extend`"
            );
            GB_INDIC_CONJUNCT_BREAK_LINKER
        }
        INCB_CONSONANT => {
            assert!(
                ogb == OGB_OTHER,
                "InCB consonant cp {cp:x} is not original `other`"
            );
            GB_INDIC_CONJUNCT_BREAK_CONSONANT
        }
        _ => unreachable!("IndicConjunctBreak discriminant {incb}"),
    }
}

// ---- packing (design §3.3 representation choice) ----

/// 14-bit pack: `eaw:3 | grapheme_break:5 | general_category:5 | is_emoji_presentation:1`.
fn pack(eaw: u8, gb: u8, gc: u8, emoji_pres: bool) -> u16 {
    (eaw as u16) | ((gb as u16) << 3) | ((gc as u16) << 8) | ((emoji_pres as u16) << 13)
}

// ---- emit uucode_tables.rs into OUT_DIR ----

fn emit(stage1: &[u32], stage2: &[u16]) {
    let mut out = String::with_capacity(stage2.len() * 6 + stage1.len() * 6 + 4096);
    out.push_str(
        "// @generated by build.rs from the vendored UCD 16.0.0 corpus. DO NOT EDIT.\n\
         // Faithful reproduction of uucode 0.1.0's 2-stage/unpacked stage1/stage2 tables\n\
         // (VAXIS_C1B_DESIGN.md §3): stage2 holds each `Data` packed as a 14-bit u16\n\
         // (eaw:3 | grapheme_break:5 | general_category:5 | is_emoji_presentation:1).\n\n",
    );

    write!(out, "pub static STAGE1: [u32; {}] = [", stage1.len()).unwrap();
    for (i, v) in stage1.iter().enumerate() {
        if i % 16 == 0 {
            out.push_str("\n    ");
        }
        write!(out, "{v},").unwrap();
    }
    out.push_str("\n];\n\n");

    write!(out, "pub static STAGE2: [u16; {}] = [", stage2.len()).unwrap();
    for (i, v) in stage2.iter().enumerate() {
        if i % 16 == 0 {
            out.push_str("\n    ");
        }
        write!(out, "{v},").unwrap();
    }
    out.push_str("\n];\n");

    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("uucode_tables.rs");
    std::fs::write(&dest, out).unwrap_or_else(|e| panic!("write {}: {e}", dest.display()));
}

// ---- build-time unit tests (ported from Ucd.zig `parseCp` / `parseRange`) ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cp() {
        assert_eq!(super::parse_cp("0000"), 0x0000);
        assert_eq!(super::parse_cp("1F600"), 0x1F600);
    }

    #[test]
    fn parse_range() {
        assert_eq!(super::parse_range("0030..0039"), (0x0030, 0x0039));
        assert_eq!(super::parse_range("1F600"), (0x1F600, 0x1F600));
    }
}
