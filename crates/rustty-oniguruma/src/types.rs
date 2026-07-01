//! rustty-oniguruma types
//!
//! PORTS: ghostty pkg/oniguruma/types.zig
//!
//! PORT STATUS: done. `Encoding`/`Syntax` opaque handles (the reference's
//! `opaque {}` types whose associated values point at Oniguruma's global
//! encoding/syntax singletons) and the `Option` packed bitflags (re-exported as
//! the frozen `RegexOptions`/`SearchOptions`).

use oniguruma_sys as c;
use std::os::raw::c_uint;

/// An Oniguruma character encoding singleton.
// PORT NOTE: the reference `Encoding = opaque {}` exposes each encoding as a
// `*Encoding` pointing at the C global (e.g. `ONIG_ENCODING_UTF8` == the address
// of `OnigEncodingUTF8`). The faithful Rust mirror is an uninhabited opaque type
// (`enum {}`) used only behind `*mut Encoding`; each associated constructor
// returns the address of the matching `oniguruma-sys` global. These are `fn`s
// (not consts) because a raw pointer to a `static mut` is not const-evaluable.
pub enum Encoding {}

// PORT NOTE: `&raw mut STATIC` is the raw-reference form (no intermediate
// reference to the mutable static). Each cast reproduces the reference's
// `@ptrCast(c.ONIG_ENCODING_*())`.
macro_rules! encoding {
    ($name:ident, $sym:ident) => {
        pub fn $name() -> *mut Encoding {
            (&raw mut c::$sym) as *mut Encoding
        }
    };
}

impl Encoding {
    encoding!(ascii, OnigEncodingASCII);
    encoding!(iso_8859_1, OnigEncodingISO_8859_1);
    encoding!(iso_8859_2, OnigEncodingISO_8859_2);
    encoding!(iso_8859_3, OnigEncodingISO_8859_3);
    encoding!(iso_8859_4, OnigEncodingISO_8859_4);
    encoding!(iso_8859_5, OnigEncodingISO_8859_5);
    encoding!(iso_8859_6, OnigEncodingISO_8859_6);
    encoding!(iso_8859_7, OnigEncodingISO_8859_7);
    encoding!(iso_8859_8, OnigEncodingISO_8859_8);
    encoding!(iso_8859_9, OnigEncodingISO_8859_9);
    encoding!(iso_8859_10, OnigEncodingISO_8859_10);
    encoding!(iso_8859_11, OnigEncodingISO_8859_11);
    encoding!(iso_8859_13, OnigEncodingISO_8859_13);
    encoding!(iso_8859_14, OnigEncodingISO_8859_14);
    encoding!(iso_8859_15, OnigEncodingISO_8859_15);
    encoding!(iso_8859_16, OnigEncodingISO_8859_16);
    encoding!(utf8, OnigEncodingUTF8);
    encoding!(utf16_be, OnigEncodingUTF16_BE);
    encoding!(utf16_le, OnigEncodingUTF16_LE);
    encoding!(utf32_be, OnigEncodingUTF32_BE);
    encoding!(utf32_le, OnigEncodingUTF32_LE);
    encoding!(euc_jp, OnigEncodingEUC_JP);
    encoding!(euc_tw, OnigEncodingEUC_TW);
    encoding!(euc_kr, OnigEncodingEUC_KR);
    encoding!(euc_cn, OnigEncodingEUC_CN);
    encoding!(sjis, OnigEncodingSJIS);
    encoding!(koi8, OnigEncodingKOI8);
    encoding!(koi8_r, OnigEncodingKOI8_R);
    encoding!(cp1251, OnigEncodingCP1251);
    encoding!(big5, OnigEncodingBIG5);
    encoding!(gb18030, OnigEncodingGB18030);
}

/// An Oniguruma regex syntax singleton.
// PORT NOTE: `Syntax = opaque {}` mirror. `default`/`oniguruma` both point at
// `OnigSyntaxOniguruma`, faithful to the reference (`ONIG_SYNTAX_ONIGURUMA`).
pub enum Syntax {}

macro_rules! syntax {
    ($name:ident, $sym:ident) => {
        pub fn $name() -> *mut Syntax {
            (&raw mut c::$sym) as *mut Syntax
        }
    };
}

// PORT NOTE: `Syntax::default()` mirrors the reference `Syntax.default` singleton
// accessor; it is NOT `std::default::Default` (there is no default `Syntax`
// value — it returns a pointer to the global), so the advisory is suppressed.
#[allow(clippy::should_implement_trait)]
impl Syntax {
    syntax!(default, OnigSyntaxOniguruma);
    syntax!(asis, OnigSyntaxASIS);
    syntax!(posix_basic, OnigSyntaxPosixBasic);
    syntax!(posix_extended, OnigSyntaxPosixExtended);
    syntax!(emacs, OnigSyntaxEmacs);
    syntax!(grep, OnigSyntaxGrep);
    syntax!(gnu_regex, OnigSyntaxGnuRegex);
    syntax!(java, OnigSyntaxJava);
    syntax!(perl, OnigSyntaxPerl);
    syntax!(perl_ng, OnigSyntaxPerl_NG);
    syntax!(ruby, OnigSyntaxRuby);
    syntax!(oniguruma, OnigSyntaxOniguruma);
}

/// Oniguruma compile/search option flags.
// PORT NOTE: the reference `Option = packed struct(c_uint)` lays its fields out
// LSB-first in declaration order (bit i == the i-th field), matching the C
// `ONIG_OPTION_*` bit values. The faithful Rust mirror is a plain-bool struct
// whose `int()` re-derives that packed `c_uint` by OR-ing `1 << i` per set flag.
// Re-exported as the frozen `RegexOptions` (compile time) and `SearchOptions`
// (search time) — a single flag type used for both, exactly like the reference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub ignorecase: bool,
    pub extend: bool,
    pub multiline: bool,
    pub singleline: bool,
    pub find_longest: bool,
    pub find_not_empty: bool,
    pub negate_singleline: bool,
    pub dont_capture_group: bool,
    pub capture_group: bool,
    // search time
    pub notbol: bool,
    pub noteol: bool,
    pub posix_region: bool,
    pub check_validity_of_string: bool,
    // compile time
    pub ignorecase_is_ascii: bool,
    pub word_is_ascii: bool,
    pub digit_is_ascii: bool,
    pub space_is_ascii: bool,
    pub posix_is_ascii: bool,
    pub text_segment_extended_grapheme_cluster: bool,
    pub text_segment_word: bool,
    // search time
    pub not_begin_string: bool,
    pub not_end_string: bool,
    pub not_begin_position: bool,
    pub callback_each_match: bool,
    pub match_whole_string: bool,
}

impl Options {
    // PORT NOTE: `@bitCast(self)` — pack the bools back into the `c_uint`.
    pub fn int(self) -> c_uint {
        let mut v: c_uint = 0;
        let flags = [
            self.ignorecase,
            self.extend,
            self.multiline,
            self.singleline,
            self.find_longest,
            self.find_not_empty,
            self.negate_singleline,
            self.dont_capture_group,
            self.capture_group,
            self.notbol,
            self.noteol,
            self.posix_region,
            self.check_validity_of_string,
            self.ignorecase_is_ascii,
            self.word_is_ascii,
            self.digit_is_ascii,
            self.space_is_ascii,
            self.posix_is_ascii,
            self.text_segment_extended_grapheme_cluster,
            self.text_segment_word,
            self.not_begin_string,
            self.not_end_string,
            self.not_begin_position,
            self.callback_each_match,
            self.match_whole_string,
        ];
        for (i, &set) in flags.iter().enumerate() {
            if set {
                v |= 1 << i;
            }
        }
        v
    }
}

/// Compile-time regex options (frozen `oni::RegexOptions`).
pub type RegexOptions = Options;

/// Search-time regex options (frozen `oni::SearchOptions`).
pub type SearchOptions = Options;

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: reference `test "order"` — `Option{ .extend = true }.int()` must
    // equal `ONIG_OPTION_EXTEND` (proves the packed bit layout matches C).
    #[test]
    fn order() {
        let opt = Options {
            extend: true,
            ..Default::default()
        };
        assert_eq!(c::ONIG_OPTION_EXTEND, opt.int());
    }
}
