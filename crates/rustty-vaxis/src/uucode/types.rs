//! `uucode/src/types.zig` — the enum subset vaxis's width/grapheme surface needs
//! (VAXIS_C1B_DESIGN.md §5). Discriminants are PINNED (`#[repr(u8)]` + explicit
//! values) because the 1620-entry grapheme-break precompute indexes by
//! `state*324 + gb1*18 + gb2` — any drift silently corrupts the table.
//!
//! `OriginalGraphemeBreak` / `IndicConjunctBreak` are build-time-only inputs to the
//! `grapheme_break` derivation and live entirely in `build.rs`; only the three runtime
//! table enums + the derived `GraphemeBreak` (+ `BreakState`, in `grapheme.rs`) appear here.

/// `EastAsianWidth` `enum(u3)` (types.zig:101-108).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EastAsianWidth {
    Neutral = 0,
    Fullwidth = 1,
    Halfwidth = 2,
    Wide = 3,
    Narrow = 4,
    Ambiguous = 5,
}

impl EastAsianWidth {
    /// Decode a stored discriminant (from the packed stage2 `u16`).
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Neutral,
            1 => Self::Fullwidth,
            2 => Self::Halfwidth,
            3 => Self::Wide,
            4 => Self::Narrow,
            5 => Self::Ambiguous,
            _ => unreachable!(),
        }
    }
}

/// `GeneralCategory` `enum(u5)` — 30 variants Lu..Cn (types.zig:6-37).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GeneralCategory {
    LetterUppercase = 0,          // Lu
    LetterLowercase = 1,          // Ll
    LetterTitlecase = 2,          // Lt
    LetterModifier = 3,           // Lm
    LetterOther = 4,              // Lo
    MarkNonspacing = 5,           // Mn
    MarkSpacingCombining = 6,     // Mc
    MarkEnclosing = 7,            // Me
    NumberDecimalDigit = 8,       // Nd
    NumberLetter = 9,             // Nl
    NumberOther = 10,             // No
    PunctuationConnector = 11,    // Pc
    PunctuationDash = 12,         // Pd
    PunctuationOpen = 13,         // Ps
    PunctuationClose = 14,        // Pe
    PunctuationInitialQuote = 15, // Pi
    PunctuationFinalQuote = 16,   // Pf
    PunctuationOther = 17,        // Po
    SymbolMath = 18,              // Sm
    SymbolCurrency = 19,          // Sc
    SymbolModifier = 20,          // Sk
    SymbolOther = 21,             // So
    SeparatorSpace = 22,          // Zs
    SeparatorLine = 23,           // Zl
    SeparatorParagraph = 24,      // Zp
    OtherControl = 25,            // Cc
    OtherFormat = 26,             // Cf
    OtherSurrogate = 27,          // Cs
    OtherPrivateUse = 28,         // Co
    OtherNotAssigned = 29,        // Cn
}

impl GeneralCategory {
    /// Decode a stored discriminant (from the packed stage2 `u16`).
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::LetterUppercase,
            1 => Self::LetterLowercase,
            2 => Self::LetterTitlecase,
            3 => Self::LetterModifier,
            4 => Self::LetterOther,
            5 => Self::MarkNonspacing,
            6 => Self::MarkSpacingCombining,
            7 => Self::MarkEnclosing,
            8 => Self::NumberDecimalDigit,
            9 => Self::NumberLetter,
            10 => Self::NumberOther,
            11 => Self::PunctuationConnector,
            12 => Self::PunctuationDash,
            13 => Self::PunctuationOpen,
            14 => Self::PunctuationClose,
            15 => Self::PunctuationInitialQuote,
            16 => Self::PunctuationFinalQuote,
            17 => Self::PunctuationOther,
            18 => Self::SymbolMath,
            19 => Self::SymbolCurrency,
            20 => Self::SymbolModifier,
            21 => Self::SymbolOther,
            22 => Self::SeparatorSpace,
            23 => Self::SeparatorLine,
            24 => Self::SeparatorParagraph,
            25 => Self::OtherControl,
            26 => Self::OtherFormat,
            27 => Self::OtherSurrogate,
            28 => Self::OtherPrivateUse,
            29 => Self::OtherNotAssigned,
            _ => unreachable!(),
        }
    }
}

/// `GraphemeBreak` `enum(u5)` — 18 variants, discriminants pinned to types.zig:127-150.
/// Note `extend` is decomposed into `Zwnj` + `IndicConjunctBreak{Extend,Linker}` (the
/// derivation in `build.rs`), so this order differs from `OriginalGraphemeBreak`'s.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GraphemeBreak {
    Other = 0,
    Control = 1,
    Prepend = 2,
    Cr = 3,
    Lf = 4,
    RegionalIndicator = 5,
    SpacingMark = 6,
    L = 7,
    V = 8,
    T = 9,
    Lv = 10,
    Lvt = 11,
    Zwj = 12,
    Zwnj = 13,
    ExtendedPictographic = 14,
    // extend ==
    //   zwnj +
    //   indic_conjunct_break_extend +
    //   indic_conjunct_break_linker
    IndicConjunctBreakExtend = 15,
    IndicConjunctBreakLinker = 16,
    IndicConjunctBreakConsonant = 17,
}

impl GraphemeBreak {
    /// Number of variants — the grapheme-break precompute's `gb` dimension.
    pub const COUNT: usize = 18;

    /// Decode a stored discriminant (from the packed stage2 `u16`).
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Other,
            1 => Self::Control,
            2 => Self::Prepend,
            3 => Self::Cr,
            4 => Self::Lf,
            5 => Self::RegionalIndicator,
            6 => Self::SpacingMark,
            7 => Self::L,
            8 => Self::V,
            9 => Self::T,
            10 => Self::Lv,
            11 => Self::Lvt,
            12 => Self::Zwj,
            13 => Self::Zwnj,
            14 => Self::ExtendedPictographic,
            15 => Self::IndicConjunctBreakExtend,
            16 => Self::IndicConjunctBreakLinker,
            17 => Self::IndicConjunctBreakConsonant,
            _ => unreachable!(),
        }
    }
}
