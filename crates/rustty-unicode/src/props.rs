//! Property set per codepoint that Ghostty cares about.
//!
//! Adding to this lets you find new properties but also potentially makes
//! our lookup tables less efficient. Any changes to this should run the
//! benchmarks in src/bench to verify that we haven't regressed.

/// Grapheme break property.
///
/// PARITY: mirrors `uucode.types.GraphemeBreakNoControl` (an `enum(u5)`),
/// the grapheme-break property uucode produces for non-control code points.
/// rustty embeds the reference's generated tables (R1) so this enum's
/// discriminants must match uucode's `enum(u5)` ordering exactly.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum GraphemeBreakNoControl {
    #[default]
    Other = 0,
    Prepend = 1,
    RegionalIndicator = 2,
    SpacingMark = 3,
    L = 4,
    V = 5,
    T = 6,
    Lv = 7,
    Lvt = 8,
    Zwj = 9,
    Zwnj = 10,
    ExtendedPictographic = 11,
    EmojiModifierBase = 12,
    EmojiModifier = 13,
    // extend, ==
    //   zwnj +
    //   indic_conjunct_break_extend +
    //   indic_conjunct_break_linker
    IndicConjunctBreakExtend = 14,
    IndicConjunctBreakLinker = 15,
    IndicConjunctBreakConsonant = 16,
}

/// PARITY: the reference `Properties` is a `packed struct(u16)` whose final
/// field is:
///
/// > Explicit padding to give the struct a power-of-two backing integer.
/// > This is a performance workaround: with a non-power-of-two backing
/// > integer (u9), Zig 0.16 lowers loads/stores of this struct through
/// > mismatched exotic-width LLVM integer types (store i9 / load i16),
/// > which blocks LLVM from promoting temporaries to registers and forces
/// > stack round-trips in hot paths like `graphemeBreak`. See
/// > https://github.com/ziglang/zig/issues/21891 for the upstream issue.
/// > The size of the lookup table is unchanged (@sizeOf was already 2).
///
/// PORT NOTE: rustty's `Properties` is a plain Rust struct built from the
/// embedded table, not a bit-packed one, so rustc never lowers it through an
/// exotic-width integer and no `_padding: u7` field is needed. The generated
/// table data is unaffected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Properties {
    /// Codepoint width. We clamp to [0, 2] since Ghostty handles control
    /// characters and we max out at 2 for wide characters (i.e. 3-em dash
    /// becomes a 2-em dash).
    pub width: u8,

    /// Whether the code point does not contribute to the width of a grapheme
    /// cluster (not used for single code point cells).
    pub width_zero_in_grapheme: bool,

    /// Grapheme break property.
    pub grapheme_break: GraphemeBreakNoControl,

    /// Emoji VS compatibility
    pub emoji_vs_base: bool,
}

impl Default for Properties {
    fn default() -> Self {
        Self {
            width: 0,
            width_zero_in_grapheme: false,
            grapheme_break: GraphemeBreakNoControl::Other,
            emoji_vs_base: false,
        }
    }
}

impl Properties {
    // Needed for lut.Generator
    pub fn eql(a: Properties, b: Properties) -> bool {
        a.width == b.width
            && a.width_zero_in_grapheme == b.width_zero_in_grapheme
            && a.grapheme_break == b.grapheme_break
            && a.emoji_vs_base == b.emoji_vs_base
    }

    // Needed for lut.Generator
    //
    // PARITY: the reference `format` writes the struct out as a Zig literal so
    // the lut generator can codegen `stage3`. rustty embeds the already-codegen'd
    // table (R1: comptime artifacts reproduced bit-for-bit), so this is provided
    // for fidelity / debugging and produces the identical text the reference emits.
    pub fn format(self: Properties) -> String {
        format!(
            ".{{\n    .width= {},\n    .width_zero_in_grapheme= {},\n    .grapheme_break= .{},\n    .emoji_vs_base= {},\n}}",
            self.width,
            self.width_zero_in_grapheme,
            self.grapheme_break.tag_name(),
            self.emoji_vs_base,
        )
    }
}

impl crate::lut::WriteZig for Properties {
    fn write_zig_elem(&self) -> String {
        self.format()
    }
}

impl GraphemeBreakNoControl {
    /// `@tagName` equivalent: the snake_case enum field name uucode uses.
    pub fn tag_name(self) -> &'static str {
        match self {
            GraphemeBreakNoControl::Other => "other",
            GraphemeBreakNoControl::Prepend => "prepend",
            GraphemeBreakNoControl::RegionalIndicator => "regional_indicator",
            GraphemeBreakNoControl::SpacingMark => "spacing_mark",
            GraphemeBreakNoControl::L => "l",
            GraphemeBreakNoControl::V => "v",
            GraphemeBreakNoControl::T => "t",
            GraphemeBreakNoControl::Lv => "lv",
            GraphemeBreakNoControl::Lvt => "lvt",
            GraphemeBreakNoControl::Zwj => "zwj",
            GraphemeBreakNoControl::Zwnj => "zwnj",
            GraphemeBreakNoControl::ExtendedPictographic => "extended_pictographic",
            GraphemeBreakNoControl::EmojiModifierBase => "emoji_modifier_base",
            GraphemeBreakNoControl::EmojiModifier => "emoji_modifier",
            GraphemeBreakNoControl::IndicConjunctBreakExtend => "indic_conjunct_break_extend",
            GraphemeBreakNoControl::IndicConjunctBreakLinker => "indic_conjunct_break_linker",
            GraphemeBreakNoControl::IndicConjunctBreakConsonant => "indic_conjunct_break_consonant",
        }
    }
}
