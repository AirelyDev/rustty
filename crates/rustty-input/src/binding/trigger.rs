//! PORTS: ghostty src/input/Binding.zig — `Trigger` (and its C ABI form `TriggerC`), its parser,
//! folding and hashing, and `BACKWARDS_COMPATIBLE_KEYS`.

use super::*;

// ===========================================================================
// Trigger
// ===========================================================================

/// Trigger is the associated key state that can trigger an action.
/// This is an extern struct because this is also used in the C API.
///
/// This must be kept in sync with include/rustty.h ghostty_input_trigger_s
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trigger {
    /// The key that has to be pressed for a binding to take action.
    pub key: TriggerKey,

    /// The key modifiers that must be active for this to match.
    pub mods: Mods,
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger {
            // key: Trigger.Key = .{ .physical = .unidentified }
            key: TriggerKey::Physical(KeyKey::Unidentified),
            // mods: key.Mods = .{}
            mods: Mods::default(),
        }
    }
}

/// PORT NOTE: `Trigger.Key = union(C.Tag)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerKey {
    /// key is the "physical" version. This is the same as mapped for
    /// standard US keyboard layouts. For non-US keyboard layouts, this
    /// is used to bind to a physical key location rather than a translated
    /// key.
    Physical(KeyKey),

    /// This is used for binding to keys that produce a certain unicode
    /// codepoint. This is useful for binding to keys that don't have a
    /// registered keycode with Rustty.
    Unicode(u21),

    /// A catch-all key that matches any key press that is otherwise
    /// unbound.
    CatchAll,
}

/// The extern struct used for triggers in the C API.
//
// PORT NOTE: `Trigger.C = extern struct { tag, key: C.Key (extern union), mods }`
// where the reference's `mods` is `key.Mods = packed struct(u16)`. The embedding
// C ABI (rustty-embed.h) projects `mods` as `rustty_input_mods_e`, an `int`-wide
// C enum (4 bytes on the Apple LP64 ABI). We therefore carry `mods` as the packed
// integer widened to that 4-byte slot rather than embedding the ergonomic `Mods`
// struct (10 bytes, NOT `#[repr(C)]`): embedding it made `size_of::<TriggerC>()`
// 20, so `rustty_config_trigger` returned a >16-byte struct via the AAPCS64 x8
// sret path while every C/Swift caller (12-byte header struct) expected a
// register return — a hard ABI mismatch that crashed on the first call. Using the
// 4-byte packed slot restores byte-exact parity with the header (and hence the
// reference's proven 12-byte, register-returned ABI). The `int()` bit layout is
// identical to the reference's `key.Mods` backing integer. See the
// `size_of::<TriggerC>() == 12` freeze below and
// `.port/phase-2/MACOS_UI_PARITY_GAPS.md`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TriggerC {
    pub tag: TriggerCTag,
    pub key: TriggerCKey,
    /// Packed `Mods` (`Mods::int()`) widened to the header's `int`-wide
    /// `rustty_input_mods_e` slot. Reconstruct with `Mods::from_int(mods as u16)`.
    pub mods: u32,
}

// ABI freeze: the header (`rustty_input_trigger_s`) is `{ tag(4), key(4),
// mods(4) } = 12` bytes, register-returned. Any drift back above 16 bytes
// re-introduces the sret crash. (Companion runtime check: rustty-input
// `abi_trigger_c_layout`; embedding-header gate: check_embed_h_abi.sh.)
// @embed-abi rustty_input_trigger_s 12
const _: () = assert!(core::mem::size_of::<TriggerC>() == 12);
const _: () = assert!(core::mem::align_of::<TriggerC>() == 4);
// @embed-abi-field rustty_input_trigger_s tag 0
const _: () = assert!(core::mem::offset_of!(TriggerC, tag) == 0);
// @embed-abi-field rustty_input_trigger_s key 4
const _: () = assert!(core::mem::offset_of!(TriggerC, key) == 4);
// @embed-abi-field rustty_input_trigger_s mods 8
const _: () = assert!(core::mem::offset_of!(TriggerC, mods) == 8);

/// PORT NOTE: `Trigger.C.Tag = enum(c_int) { physical, unicode, catch_all }`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerCTag {
    Physical,
    Unicode,
    CatchAll,
}

/// PORT NOTE: `Trigger.C.Key = extern union { physical: key.Key, unicode: u32 }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub union TriggerCKey {
    pub physical: KeyKey,
    pub unicode: u32,
}

impl Trigger {
    /// Parse a single trigger. The input is expected to be ONLY the trigger
    /// (i.e. in the sequence `a=ignore` input is only `a`). The trigger may
    /// not be part of a sequence (i.e. `a>b`). This parses exactly a single
    /// trigger.
    pub fn parse(input: &[u8]) -> Result<Trigger, Error> {
        if input.is_empty() {
            return Err(Error::InvalidFormat);
        }
        let mut result: Trigger = Trigger::default();
        let mut rem: &[u8] = input;
        'loop_: while !rem.is_empty() {
            let idx = rem.iter().position(|&c| c == b'+').unwrap_or(rem.len());
            let part = &rem[0..idx];
            rem = if idx >= rem.len() {
                b""
            } else {
                &rem[idx + 1..]
            };

            // Check if its a modifier
            // const modsInfo = @typeInfo(key.Mods).@"struct";
            // inline for (modsInfo.fields) |field| { if (field.type == bool) {...} }
            for field in Mods::BOOL_FIELD_NAMES {
                if part == field.as_bytes() {
                    // Repeat not allowed
                    if result.mods.get_bool_field(field).unwrap() {
                        return Err(Error::InvalidFormat);
                    }
                    result.mods.set_bool_field(field, true);
                    continue 'loop_;
                }
            }

            // Alias modifiers
            // inline for (key_mods.alias) |pair| { ... }
            for pair in key_mods::ALIAS {
                if part == pair.0 {
                    // Repeat not allowed
                    let field = pair.1.tag_name();
                    if result.mods.get_bool_field(field).unwrap() {
                        return Err(Error::InvalidFormat);
                    }
                    result.mods.set_bool_field(field, true);
                    continue 'loop_;
                }
            }

            // Anything after this point is a key and we only support
            // single keys.
            if !result.is_key_unset() {
                return Err(Error::InvalidFormat);
            }

            // If the part is empty it means that it is actually
            // a literal `+`, which we treat as a Unicode character.
            if part.is_empty() {
                result.key = TriggerKey::Unicode(b'+' as u21);
                continue 'loop_;
            }

            // Check if its a key
            // const keysInfo = @typeInfo(key.Key).@"enum";
            // inline for (keysInfo.fields) |field| { if name != "unidentified" ... }
            for (k, name) in key::Key::tag_names() {
                if *name != "unidentified" && part == name.as_bytes() {
                    result.key = TriggerKey::Physical(*k);
                    continue 'loop_;
                }
            }

            // If we're still unset and we have exactly one unicode
            // character then we can use that as a key.
            if result.is_key_unset() {
                'unicode: {
                    // Invalid UTF8 drops to invalid format
                    let s = match std::str::from_utf8(part) {
                        Ok(s) => s,
                        Err(_) => break 'unicode,
                    };
                    let mut it = s.chars();
                    // No codepoints or multiple codepoints drops to invalid format
                    let cp = match it.next() {
                        Some(c) => c,
                        None => break 'unicode,
                    };
                    if it.next().is_some() {
                        break 'unicode;
                    }
                    result.key = TriggerKey::Unicode(cp as u21);
                    continue 'loop_;
                }
            }

            // Look for a matching w3c name next.
            if let Some(w3c_key) = key::Key::from_w3c(part) {
                result.key = TriggerKey::Physical(w3c_key);
                continue 'loop_;
            }

            // Check for catch_all. We do this near the end since its unlikely
            // in most cases that we're setting a catch-all key.
            if part == b"catch_all" {
                result.key = TriggerKey::CatchAll;
                continue 'loop_;
            }

            // If we're still unset then we look for backwards compatible
            // keys with Rustty 1.1.x. We do this last so its least likely
            // to impact performance for modern users.
            if let Some(old_key) = backwards_compatible_keys_get(part) {
                result.key = old_key;
                continue 'loop_;
            }

            // We didn't recognize this value
            return Err(Error::InvalidFormat);
        }

        Ok(result)
    }

    /// Returns true if this trigger has no key set.
    pub fn is_key_unset(self) -> bool {
        match self.key {
            TriggerKey::Physical(v) => v == KeyKey::Unidentified,
            TriggerKey::Unicode(_) | TriggerKey::CatchAll => false,
        }
    }

    /// Returns a hash code that can be used to uniquely identify this trigger.
    pub fn hash(self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash_incremental(&mut hasher);
        hasher.finish()
    }

    /// Hash the trigger into the given hasher.
    pub(super) fn hash_incremental(self, hasher: &mut DefaultHasher) {
        // std.hash.autoHash(hasher, std.meta.activeTag(self.key));
        match self.key {
            TriggerKey::Physical(v) => {
                hasher.write_u8(0);
                hasher.write_i32(v as i32);
            }
            TriggerKey::Unicode(cp) => {
                hasher.write_u8(1);
                // std.hash.autoHash(hasher, foldedCodepoint(cp));
                for c in folded_codepoint(cp) {
                    hasher.write_u32(c);
                }
            }
            TriggerKey::CatchAll => {
                hasher.write_u8(2);
            }
        }
        // std.hash.autoHash(hasher, self.mods.binding());
        hasher.write_u16(self.mods.binding().int());
    }

    /// Returns true if two triggers are equal.
    pub fn equal(self, other: Trigger) -> bool {
        if self.mods != other.mods {
            return false;
        }
        // const self_tag = std.meta.activeTag(self.key);
        // const other_tag = std.meta.activeTag(other.key);
        // if (self_tag != other_tag) return false;
        match (self.key, other.key) {
            (TriggerKey::Physical(v), TriggerKey::Physical(o)) => v == o,
            (TriggerKey::Unicode(v), TriggerKey::Unicode(o)) => v == o,
            (TriggerKey::CatchAll, TriggerKey::CatchAll) => true,
            _ => false,
        }
    }

    /// Returns true if two triggers are equal using folded codepoints.
    pub fn folded_equal(self, other: Trigger) -> bool {
        if self.mods != other.mods {
            return false;
        }
        match (self.key, other.key) {
            (TriggerKey::Physical(v), TriggerKey::Physical(o)) => v == o,
            (TriggerKey::Unicode(v), TriggerKey::Unicode(o)) => {
                // deepEqual([3]u21, foldedCodepoint(v), foldedCodepoint(other.key.unicode))
                deep_equal::<[u21; 3]>(&folded_codepoint(v), &folded_codepoint(o))
            }
            (TriggerKey::CatchAll, TriggerKey::CatchAll) => true,
            _ => false,
        }
    }

    // PORT NOTE: `const bindingSetEqual = foldedEqual;` — the `Set` hash-map
    // `Context(Trigger)` uses folded equality. Exposed via `folded_equal`.

    /// Convert the trigger to a C API compatible trigger.
    pub fn cval(self) -> TriggerC {
        TriggerC {
            tag: match self.key {
                TriggerKey::Physical(_) => TriggerCTag::Physical,
                TriggerKey::Unicode(_) => TriggerCTag::Unicode,
                TriggerKey::CatchAll => TriggerCTag::CatchAll,
            },
            key: match self.key {
                TriggerKey::Physical(v) => TriggerCKey { physical: v },
                TriggerKey::Unicode(v) => TriggerCKey { unicode: v },
                // catch_all has no associated value so its an error
                // for a C consumer to look at it.
                //
                // PORT NOTE: the reference leaves this `undefined`; we use the
                // zero `physical` payload as the (never-read) placeholder.
                TriggerKey::CatchAll => TriggerCKey {
                    physical: KeyKey::Unidentified,
                },
            },
            // Project the ergonomic `Mods` struct to its packed integer, widened
            // to the header's `int`-wide `rustty_input_mods_e` slot (see TriggerC).
            mods: u32::from(self.mods.int()),
        }
    }

    /// Format implementation for fmt package.
    pub fn format<W: Writer>(&self, writer: &mut W) -> Result<(), W::Error> {
        // Modifiers first
        if self.mods.super_ {
            writer.write_all(b"super+")?;
        }
        if self.mods.ctrl {
            writer.write_all(b"ctrl+")?;
        }
        if self.mods.alt {
            writer.write_all(b"alt+")?;
        }
        if self.mods.shift {
            writer.write_all(b"shift+")?;
        }

        // Key
        match self.key {
            TriggerKey::Physical(k) => writer.write_all(k.tag_name().as_bytes()),
            TriggerKey::Unicode(c) => {
                // try writer.print("{u}", .{c}); — the codepoint as UTF-8.
                let ch = char::from_u32(c).unwrap_or('\u{fffd}');
                let mut buf = [0u8; 4];
                writer.write_all(ch.encode_utf8(&mut buf).as_bytes())
            }
            TriggerKey::CatchAll => writer.write_all(b"catch_all"),
        }
    }
}

/// The codepoint we use for comparisons. Case folding can result
/// in more codepoints so we need to use a 3 element array.
//
// PORT NOTE: the reference's `foldedCodepoint` ASCII fast path uses
// `uucode.ascii.isAlphabetic`/`toLower`; the Unicode slow path uses
// `uucode.get(.case_folding_full, cp)`, ported as
// `rustty_unicode::case_folding::case_folding_full`. CORRECTION (S65-F1,
// 2026-09-23): the slow path used to return `[cp, 0, 0]` for every non-ASCII
// code point, under a note that the table was "NOT yet ported". As in the
// reference, a folding of up to three code points fills the array; the
// reference's comment about returning the code point as-is does not describe
// its own code, which copies the whole slice.
pub(super) fn folded_codepoint(cp: u21) -> [u21; 3] {
    // ASCII fast path
    if ascii_is_alphabetic(cp) {
        return [ascii_to_lower(cp), 0, 0];
    }

    // Unicode slow path. Case folding can result in more codepoints.
    // If more codepoints are produced then we return the codepoint
    // as-is which isn't correct but until we have a failing test
    // then I don't want to handle this.
    let (slice, len) = rustty_unicode::case_folding::case_folding_full(cp);
    let mut array: [u21; 3] = [0; 3];
    array[..len].copy_from_slice(&slice[..len]);
    array
}

// PORT NOTE: `uucode.ascii.isAlphabetic` (uucode ascii.zig): `'A'...'Z',
// 'a'...'z' => true`.
fn ascii_is_alphabetic(c: u21) -> bool {
    matches!(c, 0x41..=0x5A | 0x61..=0x7A)
}

// PORT NOTE: `uucode.ascii.toLower` (uucode ascii.zig):
// `c | (@intFromBool(isUpper(c)) << 5)`.
fn ascii_to_lower(c: u21) -> u21 {
    let is_upper = (0x41..=0x5A).contains(&c);
    c | ((is_upper as u21) << 5)
}

/// The values that are backwards compatible with Rustty 1.1.x.
/// Rustty 1.2+ doesn't support these anymore since we moved to
/// W3C key codes.
//
// PORT NOTE: the reference `backwards_compatible_keys` is a
// `std.StaticStringMap(Key)`. Modeled as a static name->TriggerKey table; the
// lookup `.get(part)` is `backwards_compatible_keys_get`. The `keys()`/`values()`
// iteration used by the test is exposed via `BACKWARDS_COMPATIBLE_KEYS`.
pub const BACKWARDS_COMPATIBLE_KEYS: &[(&[u8], TriggerKey)] = &[
    (b"zero", TriggerKey::Unicode(b'0' as u21)),
    (b"one", TriggerKey::Unicode(b'1' as u21)),
    (b"two", TriggerKey::Unicode(b'2' as u21)),
    (b"three", TriggerKey::Unicode(b'3' as u21)),
    (b"four", TriggerKey::Unicode(b'4' as u21)),
    (b"five", TriggerKey::Unicode(b'5' as u21)),
    (b"six", TriggerKey::Unicode(b'6' as u21)),
    (b"seven", TriggerKey::Unicode(b'7' as u21)),
    (b"eight", TriggerKey::Unicode(b'8' as u21)),
    (b"nine", TriggerKey::Unicode(b'9' as u21)),
    (b"plus", TriggerKey::Unicode(b'+' as u21)),
    (b"apostrophe", TriggerKey::Unicode(b'\'' as u21)),
    (b"grave_accent", TriggerKey::Physical(KeyKey::Backquote)),
    (b"left_bracket", TriggerKey::Physical(KeyKey::BracketLeft)),
    (b"right_bracket", TriggerKey::Physical(KeyKey::BracketRight)),
    (b"up", TriggerKey::Physical(KeyKey::ArrowUp)),
    (b"down", TriggerKey::Physical(KeyKey::ArrowDown)),
    (b"left", TriggerKey::Physical(KeyKey::ArrowLeft)),
    (b"right", TriggerKey::Physical(KeyKey::ArrowRight)),
    (b"kp_0", TriggerKey::Physical(KeyKey::Numpad0)),
    (b"kp_1", TriggerKey::Physical(KeyKey::Numpad1)),
    (b"kp_2", TriggerKey::Physical(KeyKey::Numpad2)),
    (b"kp_3", TriggerKey::Physical(KeyKey::Numpad3)),
    (b"kp_4", TriggerKey::Physical(KeyKey::Numpad4)),
    (b"kp_5", TriggerKey::Physical(KeyKey::Numpad5)),
    (b"kp_6", TriggerKey::Physical(KeyKey::Numpad6)),
    (b"kp_7", TriggerKey::Physical(KeyKey::Numpad7)),
    (b"kp_8", TriggerKey::Physical(KeyKey::Numpad8)),
    (b"kp_9", TriggerKey::Physical(KeyKey::Numpad9)),
    (b"kp_add", TriggerKey::Physical(KeyKey::NumpadAdd)),
    (b"kp_subtract", TriggerKey::Physical(KeyKey::NumpadSubtract)),
    (b"kp_multiply", TriggerKey::Physical(KeyKey::NumpadMultiply)),
    (b"kp_divide", TriggerKey::Physical(KeyKey::NumpadDivide)),
    (b"kp_decimal", TriggerKey::Physical(KeyKey::NumpadDecimal)),
    (b"kp_enter", TriggerKey::Physical(KeyKey::NumpadEnter)),
    (b"kp_equal", TriggerKey::Physical(KeyKey::NumpadEqual)),
    (
        b"kp_separator",
        TriggerKey::Physical(KeyKey::NumpadSeparator),
    ),
    (b"kp_left", TriggerKey::Physical(KeyKey::NumpadLeft)),
    (b"kp_right", TriggerKey::Physical(KeyKey::NumpadRight)),
    (b"kp_up", TriggerKey::Physical(KeyKey::NumpadUp)),
    (b"kp_down", TriggerKey::Physical(KeyKey::NumpadDown)),
    (b"kp_page_up", TriggerKey::Physical(KeyKey::NumpadPageUp)),
    (
        b"kp_page_down",
        TriggerKey::Physical(KeyKey::NumpadPageDown),
    ),
    (b"kp_home", TriggerKey::Physical(KeyKey::NumpadHome)),
    (b"kp_end", TriggerKey::Physical(KeyKey::NumpadEnd)),
    (b"kp_insert", TriggerKey::Physical(KeyKey::NumpadInsert)),
    (b"kp_delete", TriggerKey::Physical(KeyKey::NumpadDelete)),
    (b"kp_begin", TriggerKey::Physical(KeyKey::NumpadBegin)),
    (b"left_shift", TriggerKey::Physical(KeyKey::ShiftLeft)),
    (b"right_shift", TriggerKey::Physical(KeyKey::ShiftRight)),
    (b"left_control", TriggerKey::Physical(KeyKey::ControlLeft)),
    (b"right_control", TriggerKey::Physical(KeyKey::ControlRight)),
    (b"left_alt", TriggerKey::Physical(KeyKey::AltLeft)),
    (b"right_alt", TriggerKey::Physical(KeyKey::AltRight)),
    (b"left_super", TriggerKey::Physical(KeyKey::MetaLeft)),
    (b"right_super", TriggerKey::Physical(KeyKey::MetaRight)),
    // Physical variants. This is a blunt approach to this but its
    // glue for backwards compatibility so I'm not too worried about
    // making this super nice.
    (b"physical:zero", TriggerKey::Physical(KeyKey::Digit0)),
    (b"physical:one", TriggerKey::Physical(KeyKey::Digit1)),
    (b"physical:two", TriggerKey::Physical(KeyKey::Digit2)),
    (b"physical:three", TriggerKey::Physical(KeyKey::Digit3)),
    (b"physical:four", TriggerKey::Physical(KeyKey::Digit4)),
    (b"physical:five", TriggerKey::Physical(KeyKey::Digit5)),
    (b"physical:six", TriggerKey::Physical(KeyKey::Digit6)),
    (b"physical:seven", TriggerKey::Physical(KeyKey::Digit7)),
    (b"physical:eight", TriggerKey::Physical(KeyKey::Digit8)),
    (b"physical:nine", TriggerKey::Physical(KeyKey::Digit9)),
    (b"physical:apostrophe", TriggerKey::Physical(KeyKey::Quote)),
    (
        b"physical:grave_accent",
        TriggerKey::Physical(KeyKey::Backquote),
    ),
    (
        b"physical:left_bracket",
        TriggerKey::Physical(KeyKey::BracketLeft),
    ),
    (
        b"physical:right_bracket",
        TriggerKey::Physical(KeyKey::BracketRight),
    ),
    (b"physical:up", TriggerKey::Physical(KeyKey::ArrowUp)),
    (b"physical:down", TriggerKey::Physical(KeyKey::ArrowDown)),
    (b"physical:left", TriggerKey::Physical(KeyKey::ArrowLeft)),
    (b"physical:right", TriggerKey::Physical(KeyKey::ArrowRight)),
    (b"physical:kp_0", TriggerKey::Physical(KeyKey::Numpad0)),
    (b"physical:kp_1", TriggerKey::Physical(KeyKey::Numpad1)),
    (b"physical:kp_2", TriggerKey::Physical(KeyKey::Numpad2)),
    (b"physical:kp_3", TriggerKey::Physical(KeyKey::Numpad3)),
    (b"physical:kp_4", TriggerKey::Physical(KeyKey::Numpad4)),
    (b"physical:kp_5", TriggerKey::Physical(KeyKey::Numpad5)),
    (b"physical:kp_6", TriggerKey::Physical(KeyKey::Numpad6)),
    (b"physical:kp_7", TriggerKey::Physical(KeyKey::Numpad7)),
    (b"physical:kp_8", TriggerKey::Physical(KeyKey::Numpad8)),
    (b"physical:kp_9", TriggerKey::Physical(KeyKey::Numpad9)),
    (b"physical:kp_add", TriggerKey::Physical(KeyKey::NumpadAdd)),
    (
        b"physical:kp_subtract",
        TriggerKey::Physical(KeyKey::NumpadSubtract),
    ),
    (
        b"physical:kp_multiply",
        TriggerKey::Physical(KeyKey::NumpadMultiply),
    ),
    (
        b"physical:kp_divide",
        TriggerKey::Physical(KeyKey::NumpadDivide),
    ),
    (
        b"physical:kp_decimal",
        TriggerKey::Physical(KeyKey::NumpadDecimal),
    ),
    (
        b"physical:kp_enter",
        TriggerKey::Physical(KeyKey::NumpadEnter),
    ),
    (
        b"physical:kp_equal",
        TriggerKey::Physical(KeyKey::NumpadEqual),
    ),
    (
        b"physical:kp_separator",
        TriggerKey::Physical(KeyKey::NumpadSeparator),
    ),
    (
        b"physical:kp_left",
        TriggerKey::Physical(KeyKey::NumpadLeft),
    ),
    (
        b"physical:kp_right",
        TriggerKey::Physical(KeyKey::NumpadRight),
    ),
    (b"physical:kp_up", TriggerKey::Physical(KeyKey::NumpadUp)),
    (
        b"physical:kp_down",
        TriggerKey::Physical(KeyKey::NumpadDown),
    ),
    (
        b"physical:kp_page_up",
        TriggerKey::Physical(KeyKey::NumpadPageUp),
    ),
    (
        b"physical:kp_page_down",
        TriggerKey::Physical(KeyKey::NumpadPageDown),
    ),
    (
        b"physical:kp_home",
        TriggerKey::Physical(KeyKey::NumpadHome),
    ),
    (b"physical:kp_end", TriggerKey::Physical(KeyKey::NumpadEnd)),
    (
        b"physical:kp_insert",
        TriggerKey::Physical(KeyKey::NumpadInsert),
    ),
    (
        b"physical:kp_delete",
        TriggerKey::Physical(KeyKey::NumpadDelete),
    ),
    (
        b"physical:kp_begin",
        TriggerKey::Physical(KeyKey::NumpadBegin),
    ),
    (
        b"physical:left_shift",
        TriggerKey::Physical(KeyKey::ShiftLeft),
    ),
    (
        b"physical:right_shift",
        TriggerKey::Physical(KeyKey::ShiftRight),
    ),
    (
        b"physical:left_control",
        TriggerKey::Physical(KeyKey::ControlLeft),
    ),
    (
        b"physical:right_control",
        TriggerKey::Physical(KeyKey::ControlRight),
    ),
    (b"physical:left_alt", TriggerKey::Physical(KeyKey::AltLeft)),
    (
        b"physical:right_alt",
        TriggerKey::Physical(KeyKey::AltRight),
    ),
    (
        b"physical:left_super",
        TriggerKey::Physical(KeyKey::MetaLeft),
    ),
    (
        b"physical:right_super",
        TriggerKey::Physical(KeyKey::MetaRight),
    ),
];

fn backwards_compatible_keys_get(part: &[u8]) -> Option<TriggerKey> {
    for (k, v) in BACKWARDS_COMPATIBLE_KEYS {
        if *k == part {
            return Some(*v);
        }
    }
    None
}
