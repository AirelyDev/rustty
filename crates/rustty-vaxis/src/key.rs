//! Port of `libvaxis` `Key.zig` (458 LOC, 5 test) — the `Key` value struct, the
//! `Modifiers`/`KittyFlags` packed structs, the 113 key-constants, the `name_map`,
//! and the loose-matching algorithm (`matches`/`matchesAny`/`matchShortcut`/
//! `matchShiftedCodepoint`/`matchText`/`matchExact`/`isModifier`).
//!
//! PORT-NOTE (codepoint width): Zig `codepoint: u21` → Rust `u32`. `u32` (not `char`)
//! is REQUIRED — `multicodepoint = 1_114_113` exceeds `char::MAX` (0x10FFFF), so
//! `char` would be wrong. `shifted_codepoint`/`base_layout_codepoint` (`?u21`) → `Option<u32>`.
//!
//! PORT-NOTE (byte-slice): `Key.text` (`?[]const u8`) is grapheme BYTES backed by a
//! caller-managed ring buffer → `Option<&'a [u8]>`, NEVER a lossy `String`.
//!
//! PORT-NOTE (packed-struct fidelity, §7): `Modifiers` is a `packed struct(u8)`,
//! LSB-first; `@bitCast` to `u8` is the equality `std.meta.eql` uses, and the matchers
//! rely on it. We reproduce it via an explicit `to_u8()` bit-pack + `Modifiers::eql`.
//!
//! PORT-NOTE (const casing): the Zig key constants are lowercase (`tab`, `f1`, `kp_0`).
//! We keep the exact Zig identifiers (R1/R2), so the module allows `non_upper_case_globals`.
//! The two Zig quoted idents `@"f16"`/`@"f32"` → plain `f16_`/`f32_` (Rust reserves the
//! bare `f16`/`f32` primitive-type names).
#![allow(non_upper_case_globals)]

/// Modifier Keys for a Key Match Event.
///
/// PORT-NOTE: Zig `super` field → `super_` (`super` is a Rust keyword that cannot be a
/// raw identifier). LSB bit order (§7): shift0 alt1 ctrl2 super3 hyper4 meta5 caps6 num7.
///
/// PORT-NOTE (Chunk 3a): `PartialEq`/`Eq` added — the field-by-field bool compare is
/// identical to the existing bitcast `eql`, and is needed to derive `PartialEq` on `Key`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub super_: bool,
    pub hyper: bool,
    pub meta: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl Modifiers {
    /// `@bitCast(Modifiers) -> u8` — the LSB-first packed bit layout (§7).
    fn to_u8(self) -> u8 {
        (self.shift as u8)
            | ((self.alt as u8) << 1)
            | ((self.ctrl as u8) << 2)
            | ((self.super_ as u8) << 3)
            | ((self.hyper as u8) << 4)
            | ((self.meta as u8) << 5)
            | ((self.caps_lock as u8) << 6)
            | ((self.num_lock as u8) << 7)
    }

    pub fn eql(self, other: Modifiers) -> bool {
        let a: u8 = self.to_u8();
        let b: u8 = other.to_u8();
        a == b
    }

    /// `@bitCast(u8) -> Modifiers` — the inverse of [`to_u8`]; unpacks the LSB-first
    /// packed bit layout (§7). PORT-NOTE (Chunk 3a): used at the CSI decode sites
    /// (`key.mods = @bitCast(modifier_mask -| 1)`).
    pub fn from_u8(bits: u8) -> Modifiers {
        Modifiers {
            shift: bits & 0b0000_0001 != 0,
            alt: bits & 0b0000_0010 != 0,
            ctrl: bits & 0b0000_0100 != 0,
            super_: bits & 0b0000_1000 != 0,
            hyper: bits & 0b0001_0000 != 0,
            meta: bits & 0b0010_0000 != 0,
            caps_lock: bits & 0b0100_0000 != 0,
            num_lock: bits & 0b1000_0000 != 0,
        }
    }
}

/// Flags for the Kitty Protocol.
///
/// `packed struct(u5)`, LSB-first: disamb0 events1 alt_keys2 all_ctl3 text4. Non-trivial
/// defaults → default bitcast `0b11101 = 29`. Shape-only in Chunk 1a (consumed by
/// tty/Parser later); no methods.
#[derive(Clone, Copy, Debug)]
pub struct KittyFlags {
    pub disambiguate: bool,
    pub report_events: bool,
    pub report_alternate_keys: bool,
    pub report_all_as_ctl_seqs: bool,
    pub report_text: bool,
}

impl Default for KittyFlags {
    fn default() -> Self {
        KittyFlags {
            disambiguate: true,
            report_events: false,
            report_alternate_keys: true,
            report_all_as_ctl_seqs: true,
            report_text: true,
        }
    }
}

/// The `Key` value struct. Ported from the top-level `Key.zig` fields.
///
/// PORT-NOTE: Zig `Key` has no default (`codepoint` has no default value), so this
/// struct intentionally does NOT derive `Default`.
///
/// PORT-NOTE (Chunk 3a): `PartialEq` added so the Parser tests can `assert_eq!` a whole
/// `Key` (Zig `expectEqual`/`expectEqualDeep`). `text: Option<&[u8]>` compares by
/// byte-slice equality — the robust faithful semantic (Zig's `expectEqual` on a slice is
/// pointer-identity that happens to pass via literal interning; `expectEqualDeep` is
/// exactly byte-equality).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key<'a> {
    /// the unicode codepoint of the key event.
    pub codepoint: u32,

    /// the text generated from the key event. The underlying slice has a limited
    /// lifetime. Vaxis maintains an internal ring buffer to temporarily store text.
    /// If the application needs these values longer than the lifetime of the event
    /// it must copy the data.
    pub text: Option<&'a [u8]>,

    /// the shifted codepoint of this key event. This will only be present if the
    /// Shift modifier was used to generate the event
    pub shifted_codepoint: Option<u32>,

    /// the key that would have been pressed on a standard keyboard layout. This is
    /// useful for shortcut matching
    pub base_layout_codepoint: Option<u32>,

    pub mods: Modifiers,
}

impl Key<'_> {
    // matches follows a loose matching algorithm for key matches.
    // 1. If the codepoint and modifiers are exact matches, after removing caps_lock
    //    and num_lock
    // 2. If the utf8 encoding of the codepoint matches the text, after removing
    //    num_lock
    // 3. If there is a shifted codepoint and it matches after removing the shift
    //    modifier from self, after removing caps_lock and num_lock
    pub fn matches(&self, cp: u32, mods: Modifiers) -> bool {
        // rule 1
        if self.match_exact(cp, mods) {
            return true;
        }

        // rule 2
        if self.match_text(cp, mods) {
            return true;
        }

        // rule 3
        if self.match_shifted_codepoint(cp, mods) {
            return true;
        }

        false
    }

    /// matches against any of the provided codepoints.
    pub fn matches_any(&self, cps: &[u32], mods: Modifiers) -> bool {
        for &cp in cps {
            if self.matches(cp, mods) {
                return true;
            }
        }
        false
    }

    /// matches base layout codes, useful for shortcut matching when an alternate key
    /// layout is used
    pub fn match_shortcut(&self, cp: u32, mods: Modifiers) -> bool {
        if self.base_layout_codepoint.is_none() {
            return false;
        }
        cp == self.base_layout_codepoint.unwrap() && Modifiers::eql(self.mods, mods)
    }

    /// matches keys that aren't upper case versions when shifted. For example, shift
    /// + semicolon produces a colon. The key can be matched against shift +
    /// semicolon or just colon...or shift + ctrl + ; or just ctrl + :
    // (verbatim Zig doc wording; the leading `+` reads as a Markdown list to clippy)
    #[allow(clippy::doc_lazy_continuation)]
    // R1 fidelity: `tgt_mods` is computed but never read — Zig does the same (it
    // compares `self_mods` against the ORIGINAL `mods`, not `tgt_mods`). Preserving
    // the dead store faithfully requires silencing the two unused lints here only.
    #[allow(unused_variables, unused_assignments)]
    pub fn match_shifted_codepoint(&self, cp: u32, mods: Modifiers) -> bool {
        if self.shifted_codepoint.is_none() {
            return false;
        }
        if !self.mods.shift {
            return false;
        }
        let mut self_mods = self.mods;
        self_mods.shift = false;
        self_mods.caps_lock = false;
        self_mods.num_lock = false;
        // PORT-NOTE (faithful quirk): Zig clears caps_lock/num_lock on `tgt_mods` here
        // but then compares `self_mods` against the ORIGINAL `mods` (not `tgt_mods`).
        // Reproduced exactly — this is a real asymmetry, not a bug to "fix".
        let mut tgt_mods = mods;
        tgt_mods.caps_lock = false;
        tgt_mods.num_lock = false;
        cp == self.shifted_codepoint.unwrap() && Modifiers::eql(self_mods, mods)
    }

    /// matches when the utf8 encoding of the codepoint and relevant mods matches the
    /// text of the key. This function will consume Shift and Caps Lock when matching
    pub fn match_text(&self, cp: u32, mods: Modifiers) -> bool {
        // return early if we have no text
        if self.text.is_none() {
            return false;
        }

        let mut self_mods = self.mods;
        self_mods.num_lock = false;
        self_mods.shift = false;
        self_mods.caps_lock = false;
        let mut arg_mods = mods;

        // TODO: Use zg case_data for full unicode support. We'll need to allocate the case data
        // somewhere
        let _cp: u32 = if cp < 128 && (mods.shift || mods.caps_lock) {
            // Uppercase our codepoint
            (cp as u8).to_ascii_uppercase() as u32
        } else {
            cp
        };

        arg_mods.num_lock = false;
        arg_mods.shift = false;
        arg_mods.caps_lock = false;

        let mut buf: [u8; 4] = [0; 4];
        let n = match char::from_u32(_cp) {
            Some(c) => c.encode_utf8(&mut buf).len(),
            None => return false,
        };
        self.text.unwrap() == &buf[0..n] && Modifiers::eql(self_mods, arg_mods)
    }

    // The key must exactly match the codepoint and modifiers. caps_lock and
    // num_lock are removed before matching
    pub fn match_exact(&self, cp: u32, mods: Modifiers) -> bool {
        let mut self_mods = self.mods;
        self_mods.caps_lock = false;
        self_mods.num_lock = false;
        let mut tgt_mods = mods;
        tgt_mods.caps_lock = false;
        tgt_mods.num_lock = false;
        self.codepoint == cp && Modifiers::eql(self_mods, tgt_mods)
    }

    /// True if the key is a single modifier (ie: left_shift)
    pub fn is_modifier(&self) -> bool {
        self.codepoint == left_shift
            || self.codepoint == left_alt
            || self.codepoint == left_super
            || self.codepoint == left_hyper
            || self.codepoint == left_control
            || self.codepoint == left_meta
            || self.codepoint == right_shift
            || self.codepoint == right_alt
            || self.codepoint == right_super
            || self.codepoint == right_hyper
            || self.codepoint == right_control
            || self.codepoint == right_meta
    }
}

// a few special keys that we encode as their actual ascii value
pub const tab: u32 = 0x09;
pub const enter: u32 = 0x0D;
pub const escape: u32 = 0x1B;
pub const space: u32 = 0x20;
pub const backspace: u32 = 0x7F;

/// multicodepoint is a key which generated text but cannot be expressed as a
/// single codepoint. The value is the maximum unicode codepoint + 1
pub const multicodepoint: u32 = 1_114_112 + 1;

// kitty encodes these keys directly in the private use area. We reuse those
// mappings
pub const insert: u32 = 57348;
pub const delete: u32 = 57349;
pub const left: u32 = 57350;
pub const right: u32 = 57351;
pub const up: u32 = 57352;
pub const down: u32 = 57353;
pub const page_up: u32 = 57354;
pub const page_down: u32 = 57355;
pub const home: u32 = 57356;
pub const end: u32 = 57357;
pub const caps_lock: u32 = 57358;
pub const scroll_lock: u32 = 57359;
pub const num_lock: u32 = 57360;
pub const print_screen: u32 = 57361;
pub const pause: u32 = 57362;
pub const menu: u32 = 57363;
pub const f1: u32 = 57364;
pub const f2: u32 = 57365;
pub const f3: u32 = 57366;
pub const f4: u32 = 57367;
pub const f5: u32 = 57368;
pub const f6: u32 = 57369;
pub const f7: u32 = 57370;
pub const f8: u32 = 57371;
pub const f9: u32 = 57372;
pub const f10: u32 = 57373;
pub const f11: u32 = 57374;
pub const f12: u32 = 57375;
pub const f13: u32 = 57376;
pub const f14: u32 = 57377;
pub const f15: u32 = 57378;
pub const f16_: u32 = 57379;
pub const f17: u32 = 57380;
pub const f18: u32 = 57381;
pub const f19: u32 = 57382;
pub const f20: u32 = 57383;
pub const f21: u32 = 57384;
pub const f22: u32 = 57385;
pub const f23: u32 = 57386;
pub const f24: u32 = 57387;
pub const f25: u32 = 57388;
pub const f26: u32 = 57389;
pub const f27: u32 = 57390;
pub const f28: u32 = 57391;
pub const f29: u32 = 57392;
pub const f30: u32 = 57393;
pub const f31: u32 = 57394;
pub const f32_: u32 = 57395;
pub const f33: u32 = 57396;
pub const f34: u32 = 57397;
pub const f35: u32 = 57398;
pub const kp_0: u32 = 57399;
pub const kp_1: u32 = 57400;
pub const kp_2: u32 = 57401;
pub const kp_3: u32 = 57402;
pub const kp_4: u32 = 57403;
pub const kp_5: u32 = 57404;
pub const kp_6: u32 = 57405;
pub const kp_7: u32 = 57406;
pub const kp_8: u32 = 57407;
pub const kp_9: u32 = 57408;
pub const kp_decimal: u32 = 57409;
pub const kp_divide: u32 = 57410;
pub const kp_multiply: u32 = 57411;
pub const kp_subtract: u32 = 57412;
pub const kp_add: u32 = 57413;
pub const kp_enter: u32 = 57414;
pub const kp_equal: u32 = 57415;
pub const kp_separator: u32 = 57416;
pub const kp_left: u32 = 57417;
pub const kp_right: u32 = 57418;
pub const kp_up: u32 = 57419;
pub const kp_down: u32 = 57420;
pub const kp_page_up: u32 = 57421;
pub const kp_page_down: u32 = 57422;
pub const kp_home: u32 = 57423;
pub const kp_end: u32 = 57424;
pub const kp_insert: u32 = 57425;
pub const kp_delete: u32 = 57426;
pub const kp_begin: u32 = 57427;
pub const media_play: u32 = 57428;
pub const media_pause: u32 = 57429;
pub const media_play_pause: u32 = 57430;
pub const media_reverse: u32 = 57431;
pub const media_stop: u32 = 57432;
pub const media_fast_forward: u32 = 57433;
pub const media_rewind: u32 = 57434;
pub const media_track_next: u32 = 57435;
pub const media_track_previous: u32 = 57436;
pub const media_record: u32 = 57437;
pub const lower_volume: u32 = 57438;
pub const raise_volume: u32 = 57439;
pub const mute_volume: u32 = 57440;
pub const left_shift: u32 = 57441;
pub const left_control: u32 = 57442;
pub const left_alt: u32 = 57443;
pub const left_super: u32 = 57444;
pub const left_hyper: u32 = 57445;
pub const left_meta: u32 = 57446;
pub const right_shift: u32 = 57447;
pub const right_control: u32 = 57448;
pub const right_alt: u32 = 57449;
pub const right_super: u32 = 57450;
pub const right_hyper: u32 = 57451;
pub const right_meta: u32 = 57452;
pub const iso_level_3_shift: u32 = 57453;
pub const iso_level_5_shift: u32 = 57454;

/// Port of `name_map` — Zig `StaticStringMap(u21)`. A plain `&[(&[u8], u32)]` slice
/// consulted by a linear [`name_map_get`] (no external `phf` dep — R4 acyclic).
pub const name_map: &[(&[u8], u32)] = &[
    // common names
    (b"plus", b'+' as u32),
    (b"minus", b'-' as u32),
    (b"colon", b':' as u32),
    (b"semicolon", b';' as u32),
    (b"comma", b',' as u32),
    // special keys
    (b"tab", tab),
    (b"enter", enter),
    (b"escape", escape),
    (b"space", space),
    (b"backspace", backspace),
    (b"insert", insert),
    (b"delete", delete),
    (b"left", left),
    (b"right", right),
    (b"up", up),
    (b"down", down),
    (b"page_up", page_up),
    (b"page_down", page_down),
    (b"home", home),
    (b"end", end),
    (b"caps_lock", caps_lock),
    (b"scroll_lock", scroll_lock),
    (b"num_lock", num_lock),
    (b"print_screen", print_screen),
    (b"pause", pause),
    (b"menu", menu),
    (b"f1", f1),
    (b"f2", f2),
    (b"f3", f3),
    (b"f4", f4),
    (b"f5", f5),
    (b"f6", f6),
    (b"f7", f7),
    (b"f8", f8),
    (b"f9", f9),
    (b"f10", f10),
    (b"f11", f11),
    (b"f12", f12),
    (b"f13", f13),
    (b"f14", f14),
    (b"f15", f15),
    (b"f16", f16_),
    (b"f17", f17),
    (b"f18", f18),
    (b"f19", f19),
    (b"f20", f20),
    (b"f21", f21),
    (b"f22", f22),
    (b"f23", f23),
    (b"f24", f24),
    (b"f25", f25),
    (b"f26", f26),
    (b"f27", f27),
    (b"f28", f28),
    (b"f29", f29),
    (b"f30", f30),
    (b"f31", f31),
    (b"f32", f32_),
    (b"f33", f33),
    (b"f34", f34),
    (b"f35", f35),
    (b"kp_0", kp_0),
    (b"kp_1", kp_1),
    (b"kp_2", kp_2),
    (b"kp_3", kp_3),
    (b"kp_4", kp_4),
    (b"kp_5", kp_5),
    (b"kp_6", kp_6),
    (b"kp_7", kp_7),
    (b"kp_8", kp_8),
    (b"kp_9", kp_9),
    (b"kp_decimal", kp_decimal),
    (b"kp_divide", kp_divide),
    (b"kp_multiply", kp_multiply),
    (b"kp_subtract", kp_subtract),
    (b"kp_add", kp_add),
    (b"kp_enter", kp_enter),
    (b"kp_equal", kp_equal),
    (b"kp_separator", kp_separator),
    (b"kp_left", kp_left),
    (b"kp_right", kp_right),
    (b"kp_up", kp_up),
    (b"kp_down", kp_down),
    (b"kp_page_up", kp_page_up),
    (b"kp_page_down", kp_page_down),
    (b"kp_home", kp_home),
    (b"kp_end", kp_end),
    (b"kp_insert", kp_insert),
    (b"kp_delete", kp_delete),
    (b"kp_begin", kp_begin),
    (b"media_play", media_play),
    (b"media_pause", media_pause),
    (b"media_play_pause", media_play_pause),
    (b"media_reverse", media_reverse),
    (b"media_stop", media_stop),
    (b"media_fast_forward", media_fast_forward),
    (b"media_rewind", media_rewind),
    (b"media_track_next", media_track_next),
    (b"media_track_previous", media_track_previous),
    (b"media_record", media_record),
    (b"lower_volume", lower_volume),
    (b"raise_volume", raise_volume),
    (b"mute_volume", mute_volume),
    (b"left_shift", left_shift),
    (b"left_control", left_control),
    (b"left_alt", left_alt),
    (b"left_super", left_super),
    (b"left_hyper", left_hyper),
    (b"left_meta", left_meta),
    (b"right_shift", right_shift),
    (b"right_control", right_control),
    (b"right_alt", right_alt),
    (b"right_super", right_super),
    (b"right_hyper", right_hyper),
    (b"right_meta", right_meta),
    (b"iso_level_3_shift", iso_level_3_shift),
    (b"iso_level_5_shift", iso_level_5_shift),
];

/// `name_map.get(name)` — the linear lookup over [`name_map`]. Returns `?u21` → `Option<u32>`.
pub fn name_map_get(name: &[u8]) -> Option<u32> {
    for &(k, v) in name_map {
        if k == name {
            return Some(v);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_a() {
        let key = Key {
            codepoint: b'a' as u32,
            mods: Modifiers {
                num_lock: true,
                ..Default::default()
            },
            text: Some(b"a"),
            shifted_codepoint: None,
            base_layout_codepoint: None,
        };
        assert!(key.matches(b'a' as u32, Modifiers::default()));
        assert!(!key.matches(
            b'a' as u32,
            Modifiers {
                shift: true,
                ..Default::default()
            }
        ));
    }

    #[test]
    fn matches_shift_a() {
        let key = Key {
            codepoint: b'a' as u32,
            shifted_codepoint: Some(b'A' as u32),
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
            text: Some(b"A"),
            base_layout_codepoint: None,
        };
        assert!(key.matches(
            b'a' as u32,
            Modifiers {
                shift: true,
                ..Default::default()
            }
        ));
        assert!(!key.matches(b'a' as u32, Modifiers::default()));
        assert!(key.matches(b'A' as u32, Modifiers::default()));
        assert!(!key.matches(
            b'A' as u32,
            Modifiers {
                ctrl: true,
                ..Default::default()
            }
        ));
    }

    #[test]
    fn matches_shift_tab() {
        let key = Key {
            codepoint: tab,
            mods: Modifiers {
                shift: true,
                num_lock: true,
                ..Default::default()
            },
            text: None,
            shifted_codepoint: None,
            base_layout_codepoint: None,
        };
        assert!(key.matches(
            tab,
            Modifiers {
                shift: true,
                ..Default::default()
            }
        ));
        assert!(!key.matches(tab, Modifiers::default()));
    }

    #[test]
    fn matches_shift_semicolon() {
        let key = Key {
            codepoint: b';' as u32,
            shifted_codepoint: Some(b':' as u32),
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
            text: Some(b":"),
            base_layout_codepoint: None,
        };
        assert!(key.matches(
            b';' as u32,
            Modifiers {
                shift: true,
                ..Default::default()
            }
        ));
        assert!(key.matches(b':' as u32, Modifiers::default()));

        let colon = Key {
            codepoint: b':' as u32,
            mods: Modifiers::default(),
            text: None,
            shifted_codepoint: None,
            base_layout_codepoint: None,
        };
        assert!(colon.matches(b':' as u32, Modifiers::default()));
    }

    #[test]
    fn name_map_lookup() {
        assert_eq!(Some(insert), name_map_get(b"insert"));
    }
}
