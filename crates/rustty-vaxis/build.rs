//! CHUNK 1b table generator — a faithful, Zig-free port of the uucode UCD parsers and
//! the `grapheme_break` derivation, restricted to the four fields vaxis instantiates:
//! `east_asian_width`, `grapheme_break`, `general_category`, `is_emoji_presentation`
//! (VAXIS_C1B_DESIGN.md §3). It parses the vendored UCD corpus under `ucd/` and writes
//! `uucode_tables.rs` into `OUT_DIR`, `include!`d by `src/uucode/tables.rs`.
//!
//! REVISION (parity tail S-10b, NON_PORTS.md §G2; bumped by P7-W10 T2): at the
//! reference pin 12752b2ac, libvaxis 0.6.0 does not build its own uucode; ghostty passes
//! `external_uucode = true` and wires in its singleton uucode module, uucode **9d555245**
//! (ghostty 12542b392 "deps: Update uucode for Unicode 18") built with ghostty's
//! `src/build/uucode_config.zig` (`SharedDeps.zig`; the four fields come from its
//! `libvaxis_only` + `runtime` tables and the config does not alter them). So this
//! generator follows uucode 9d555245's `src/components.zig` (`UnicodeData`,
//! `EastAsianWidth`, `GraphemeBreak`, `DerivedCoreProperties`, `EmojiData`,
//! `GraphemeBreakDerived`) over its UCD **18.0.0** corpus (17.0.0 / uucode 2826a37a
//! before P7-W10 T2; uucode 0.1.0's `src/build/Ucd.zig` + `src/build/tables.zig` over
//! UCD 16.0.0 before S-10b). The per-code-point values are proven equal to the Zig
//! oracle's `uucode.get` over 0..=0x10FFFF (the S-10b differential,
//! `.port/phase-3/w9-vaxis-diff.sh`, NEW-FINDINGS.md N240).
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

// ---- GeneralCategory discriminants (types.zig:1-32, order pinned) ----
const GC_CN: u8 = 29; // other_not_assigned (Cn) — the default

// ---- EastAsianWidth discriminants (types.zig:95-102) ----
const EAW_NEUTRAL: u8 = 0;
const EAW_WIDE: u8 = 3;

// ---- OriginalGraphemeBreak discriminants (types.zig:104-119) ----
const OGB_OTHER: u8 = 0;
const OGB_EXTEND: u8 = 5;

// ---- IndicConjunctBreak discriminants (types.zig:88-93) ----
const INCB_NONE: u8 = 0;
const INCB_LINKER: u8 = 1;
const INCB_CONSONANT: u8 = 2;
const INCB_EXTEND: u8 = 3;

// ---- GraphemeBreak discriminants (types.zig:121-150, 9d555245, order pinned) ----
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
const GB_EMOJI_MODIFIER_BASE: u8 = 15;
const GB_EMOJI_MODIFIER: u8 = 16;
const GB_INDIC_CONJUNCT_BREAK_EXTEND: u8 = 17;
const GB_INDIC_CONJUNCT_BREAK_LINKER_EXTEND: u8 = 18;
const GB_INDIC_CONJUNCT_BREAK_LINKER_OTHER: u8 = 19;
const GB_INDIC_CONJUNCT_BREAK_CONSONANT: u8 = 20;

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
    let emoji = parse_emoji_data(&read("emoji/emoji-data.txt"));

    // Derive grapheme_break per cp, then pack Data into a 14-bit u16.
    let mut packed = vec![0u16; N];
    for cp in 0..N {
        let gb = derive_grapheme_break(cp as u32, &emoji, incb[cp], ogb[cp]);
        packed[cp] = pack(eaw[cp], gb, gc[cp], emoji.is_emoji_presentation[cp]);
    }

    // 256-block dedup (the 2-stage layout of uucode 0.1.0's tables.zig:1001-1014, kept as
    // the port's own representation: 9d555245's generate.zig layout is not reproduced,
    // only the per-cp values are pinned). Offsets assigned in scan order.
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

// ---- shared parse helpers (components.zig:182-225) ----

/// `parseCp` — hex u21 scalar (components.zig:182-184).
fn parse_cp(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap_or_else(|_| panic!("parseCp({s:?})"))
}

/// `parseRange` — `AAAA..BBBB` or a single cp (components.zig:191-200; there `end` is
/// exclusive, `end + 1`, and the loops run `start..end`; here it is inclusive and the loops
/// run `start..=end` — the same code points).
fn parse_range(s: &str) -> (u32, u32) {
    if let Some(idx) = s.find("..") {
        (parse_cp(&s[..idx]), parse_cp(&s[idx + 2..]))
    } else {
        let cp = parse_cp(s);
        (cp, cp)
    }
}

/// `trim` (components.zig:220-225) — strip a `#`-comment (cut at the FIRST `#`) then
/// trim `" \t\r"`. NOTE: applied to `# …` this yields the empty string.
fn trim_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => line[..idx].trim_matches([' ', '\t', '\r']),
        None => line.trim_matches([' ', '\t', '\r']),
    }
}

fn lines(bytes: &[u8]) -> impl Iterator<Item = &str> {
    // UCD files are ASCII/UTF-8; std lines split on '\n' like Zig's splitScalar.
    std::str::from_utf8(bytes)
        .expect("UCD file is valid UTF-8")
        .split('\n')
}

// ---- UnicodeData → general_category (components.zig `UnicodeData`) ----

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

// ---- EastAsianWidth (components.zig `EastAsianWidth`; the @missing whitespace-trim trap, design §3.5.1) ----

fn parse_east_asian_width(bytes: &[u8]) -> Vec<u8> {
    let mut eaw = vec![EAW_NEUTRAL; N];
    for line in lines(bytes) {
        // WHITESPACE-only trim (NOT comment-strip) — load-bearing for @missing.
        let trimmed = line.trim_matches([' ', '\t', '\r']);
        if trimmed.is_empty() {
            continue;
        }

        // Handle @missing directives first, detected on the whitespace-trimmed line.
        if let Some(missing_line) = trimmed.strip_prefix("# @missing:") {
            let mut parts = missing_line.split(';');
            let cp_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
            let width_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
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
        let cp_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
        let width_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
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

// ---- GraphemeBreak → original_grapheme_break (components.zig `GraphemeBreak`) ----

fn parse_grapheme_break(bytes: &[u8]) -> Vec<u8> {
    let mut gb = vec![OGB_OTHER; N];
    for line in lines(bytes) {
        let trimmed = trim_comment(line);
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split(';');
        let cp_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
        let prop_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
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

// ---- DerivedCoreProperties → indic_conjunct_break (InCB only) (components.zig `DerivedCoreProperties`) ----

fn parse_derived_core_properties(bytes: &[u8]) -> Vec<u8> {
    let mut incb = vec![INCB_NONE; N];
    for line in lines(bytes) {
        let trimmed = trim_comment(line);
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split(';');
        let cp_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
        let property_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
        // Only the `InCB` lines matter (design §3.5); ignore all other properties.
        if property_str != "InCB" {
            continue;
        }
        let value_str = parts
            .next()
            .map(|v| v.trim_matches([' ', '\t', '\r']))
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

// ---- EmojiData → the six emoji flags (components.zig `EmojiData`, 9d555245) ----

/// The `EmojiData` component's rows, restricted to the flags the `grapheme_break`
/// derivation and the `is_emoji_presentation` field read (`is_emoji` is parsed and
/// validated like the reference but not stored: no vaxis field reads it).
struct EmojiFlags {
    is_emoji_presentation: Vec<bool>,
    is_emoji_modifier: Vec<bool>,
    is_emoji_modifier_base: Vec<bool>,
    is_emoji_component: Vec<bool>,
    is_extended_pictographic: Vec<bool>,
}

fn parse_emoji_data(bytes: &[u8]) -> EmojiFlags {
    let mut flags = EmojiFlags {
        is_emoji_presentation: vec![false; N],
        is_emoji_modifier: vec![false; N],
        is_emoji_modifier_base: vec![false; N],
        is_emoji_component: vec![false; N],
        is_extended_pictographic: vec![false; N],
    };
    for line in lines(bytes) {
        let trimmed = trim_comment(line);
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split(';');
        let cp_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
        let prop_str = parts.next().unwrap().trim_matches([' ', '\t', '\r']);
        let (start, end) = parse_range(cp_str);
        let field = match prop_str {
            "Emoji" => None,
            "Emoji_Presentation" => Some(&mut flags.is_emoji_presentation),
            "Emoji_Modifier" => Some(&mut flags.is_emoji_modifier),
            "Emoji_Modifier_Base" => Some(&mut flags.is_emoji_modifier_base),
            "Emoji_Component" => Some(&mut flags.is_emoji_component),
            "Extended_Pictographic" => Some(&mut flags.is_extended_pictographic),
            _ => panic!("Unknown EmojiData property: {prop_str}"),
        };
        if let Some(field) = field {
            for cp in start..=end {
                field[cp as usize] = true;
            }
        }
    }
    flags
}

// ---- grapheme_break derivation (components.zig `GraphemeBreakDerived`, 9d555245) ----

fn derive_grapheme_break(cp: u32, emoji: &EmojiFlags, incb: u8, ogb: u8) -> u8 {
    let i = cp as usize;
    let is_emoji_modifier = emoji.is_emoji_modifier[i];
    let is_emoji_modifier_base = emoji.is_emoji_modifier_base[i];
    let is_extended_pictographic = emoji.is_extended_pictographic[i];
    let is_emoji_component = emoji.is_emoji_component[i];

    if is_emoji_modifier {
        // inlineAssert(original_grapheme_break == .extend);
        assert!(
            ogb == OGB_EXTEND,
            "emoji_modifier cp {cp:x} is not original `extend`"
        );
        // inlineAssert(!is_extended_pictographic and is_emoji_component);
        assert!(
            !is_extended_pictographic && is_emoji_component,
            "emoji_modifier cp {cp:x}: extended_pictographic or not an emoji component"
        );
        return GB_EMOJI_MODIFIER;
    }
    if is_emoji_modifier_base {
        // inlineAssert(original_grapheme_break == .other);
        assert!(
            ogb == OGB_OTHER,
            "emoji_modifier_base cp {cp:x} is not original `other`"
        );
        // inlineAssert(is_extended_pictographic);
        assert!(
            is_extended_pictographic,
            "emoji_modifier_base cp {cp:x} is not extended_pictographic"
        );
        return GB_EMOJI_MODIFIER_BASE;
    }
    if is_extended_pictographic {
        // inlineAssert(original_grapheme_break == .other);
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
                // extend/linker.
                if cp == ZERO_WIDTH_NON_JOINER {
                    GB_ZWNJ
                } else {
                    panic!(
                        "Found an `extend` grapheme break that is Indic conjunct break `none` \
                         (and not zwnj): {cp:x}"
                    );
                }
            } else {
                // The `inline else` same-name fan-out, made explicit + exhaustive
                // (design §3.4).
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
                assert!(ogb == 13, "U+200D is not original `zwj`");
                GB_ZWJ
            } else {
                assert!(
                    ogb == OGB_EXTEND,
                    "InCB extend cp {cp:x} is not original `extend`"
                );
                GB_INDIC_CONJUNCT_BREAK_EXTEND
            }
        }
        // Unicode 18 gives some GCB=Other code points InCB=Linker (U+1CF5, U+1CF6,
        // U+11A3A), so the linker splits on the original grapheme break.
        INCB_LINKER => match ogb {
            OGB_EXTEND => GB_INDIC_CONJUNCT_BREAK_LINKER_EXTEND,
            OGB_OTHER => GB_INDIC_CONJUNCT_BREAK_LINKER_OTHER,
            _ => unreachable!("InCB linker cp {cp:x} is original grapheme break {ogb}"),
        },
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
        "// @generated by build.rs from the vendored UCD 18.0.0 corpus. DO NOT EDIT.\n\
         // The per-cp values of uucode 9d555245's four vaxis fields, as 2-stage tables\n\
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

// ---- build-time unit tests (ported from components.zig `parseCp` / `parseRange`) ----

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
