//! This file contains all the terminal modes that we support
//! and various support types for them: an enum of supported modes,
//! a packed struct to store mode values, a more generalized state
//! struct to store values plus handle save/restore, and much more.
//!
//! There is pretty heavy comptime usage and type generation here.
//! I don't love to have this sort of complexity but its a good way
//! to ensure all our various types and logic remain in sync.
//!
//! PORTS: ghostty src/terminal/modes.zig
//!
//! PORT STATUS: done

// PARITY: the `ModeState` Default mirrors the reference's explicit field
// defaults; the tests use `expectEqual(false/true, ...)` verbatim.
#![allow(clippy::derivable_impls)]
#![allow(clippy::bool_assert_comparison)]

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const testing = std.testing;
// The reference generates `ModePacked` (a packed bool-per-mode struct) and
// `Mode` (an enum(u16)) from the `entries` table at comptime. Rust has no
// comptime type generation, so `ModePacked` is a `u64` bitset with one bit per
// entry (bit index == position in `ENTRIES`), and `Mode` is a `#[repr(u16)]`
// enum whose discriminant is the bitcast `ModeTag` (value | (ansi << 15)),
// matching the reference's `@bitCast(ModeTag{...})` field values.

use core::fmt::Write;

/// A struct that maintains the state of all the settable modes.
#[derive(Clone, Copy, Debug)]
pub struct ModeState {
    /// The values of the current modes.
    pub values: ModePacked,

    /// The saved values. We only allow saving each mode once.
    /// This is in line with other terminals that implement XTSAVE
    /// and XTRESTORE. We can improve this in the future if it becomes
    /// a real-world issue but we need to be aware of a DoS vector.
    pub saved: ModePacked,

    /// The default values for the modes. This is used to reset
    /// the modes to their default values during reset.
    pub default: ModePacked,
}

impl Default for ModeState {
    // PORT NOTE: reference defaults all fields to `ModePacked{}` which itself
    // defaults each mode to its `entry.default`.
    fn default() -> Self {
        ModeState {
            values: ModePacked::default(),
            saved: ModePacked::default(),
            default: ModePacked::default(),
        }
    }
}

impl ModeState {
    /// Reset the modes to their default values. This also clears the
    /// saved state.
    pub fn reset(&mut self) {
        self.values = self.default;
        self.saved = ModePacked::empty();
    }

    /// Set a mode to a value.
    pub fn set(&mut self, mode: Mode, value: bool) {
        // PORT NOTE: the reference `switch (mode) { inline else => ... }` resolves
        // `entryForMode(mode)` at comptime and sets the matching ModePacked
        // field. We set the corresponding bit by the entry index.
        let entry_idx = index_for_mode(mode);
        self.values.set_bit(entry_idx, value);
    }

    /// Get the value of a mode.
    pub fn get(&self, mode: Mode) -> bool {
        let entry_idx = index_for_mode(mode);
        self.values.get_bit(entry_idx)
    }

    /// Save the state of the given mode. This can then be restored
    /// with restore. This will only be accurate if the previous
    /// mode was saved exactly once and not restored. Otherwise this
    /// will just keep restoring the last stored value in memory.
    pub fn save(&mut self, mode: Mode) {
        let entry_idx = index_for_mode(mode);
        self.saved
            .set_bit(entry_idx, self.values.get_bit(entry_idx));
    }

    /// See save. This will return the restored value.
    pub fn restore(&mut self, mode: Mode) -> bool {
        let entry_idx = index_for_mode(mode);
        self.values
            .set_bit(entry_idx, self.saved.get_bit(entry_idx));
        self.values.get_bit(entry_idx)
    }

    /// Return a DECRPM report for the given mode tag. If the tag does
    /// not correspond to a known mode, the report state is .not_recognized.
    pub fn get_report(&self, tag: ModeTag) -> Report {
        let mode = match mode_from_int(tag.value, tag.ansi) {
            Some(m) => m,
            None => {
                return Report {
                    tag,
                    state: State::NotRecognized,
                }
            }
        };
        Report {
            tag,
            state: if self.get(mode) {
                State::Set
            } else {
                State::Reset
            },
        }
    }
}

/// A packed struct of all the settable modes. This shouldn't
/// be used directly but rather through the ModeState struct.
// PORT NOTE: the reference is a comptime-generated `packed struct` with one
// `bool` field per entry (each defaulting to `entry.default`). Bit-packed, it is
// backed by a u64 (asserted `@sizeOf == 8`). We model it as a `u64` where bit i
// corresponds to `ENTRIES[i]`. `Default` sets each entry's default bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModePacked(pub u64);

impl ModePacked {
    /// All bits cleared (no default applied) — equivalent to the saved-state
    /// reset in `ModeState.reset` (`.saved = .{}` uses field defaults, but the
    /// saved state is semantically "nothing saved"; the reference `.{}` here
    /// applies the per-mode defaults). See `empty` vs `default`.
    // PORT NOTE: the reference `.saved = .{}` applies each field's default value.
    // To match exactly, both `default()` and the saved reset use the
    // default-bit pattern; `empty()` is provided for the all-zero pattern.
    pub fn empty() -> ModePacked {
        ModePacked::default()
    }

    /// Get the value of a mode from this packed set. Mirrors the reference
    /// `@field(packed, field.name)` access used by the formatter to read each
    /// mode's default.
    pub fn get_mode(&self, mode: Mode) -> bool {
        self.get_bit(index_for_mode(mode))
    }

    fn set_bit(&mut self, idx: usize, value: bool) {
        if value {
            self.0 |= 1u64 << idx;
        } else {
            self.0 &= !(1u64 << idx);
        }
    }

    fn get_bit(&self, idx: usize) -> bool {
        (self.0 >> idx) & 1 == 1
    }
}

impl Default for ModePacked {
    fn default() -> Self {
        let mut bits: u64 = 0;
        let mut i = 0;
        while i < ENTRIES.len() {
            if ENTRIES[i].default {
                bits |= 1u64 << i;
            }
            i += 1;
        }
        ModePacked(bits)
    }
}

/// An enum(u16) of the available modes. See entries for available values.
// PORT NOTE: the reference generates this enum at comptime with each value set
// to `@bitCast(ModeTag{ .value = entry.value, .ansi = entry.ansi })`. We list
// the variants in the same order as `ENTRIES` with the same `#[repr(u16)]`
// discriminants (value | (ansi << 15)).
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    // ANSI
    DisableKeyboard = 2 | (1 << 15), // KAM
    Insert = 4 | (1 << 15),
    SendReceiveMode = 12 | (1 << 15), // SRM
    Linefeed = 20 | (1 << 15),

    // DEC
    CursorKeys = 1, // DECCKM
    Column132 = 3,
    SlowScroll = 4,
    ReverseColors = 5,
    Origin = 6,
    Wraparound = 7,
    Autorepeat = 8,
    MouseEventX10 = 9,
    CursorBlinking = 12,
    CursorVisible = 25,
    EnableMode3 = 40,
    ReverseWrap = 45,
    AltScreenLegacy = 47,
    KeypadKeys = 66,
    BackarrowKeyMode = 67,
    EnableLeftAndRightMargin = 69,
    MouseEventNormal = 1000,
    MouseEventButton = 1002,
    MouseEventAny = 1003,
    FocusEvent = 1004,
    MouseFormatUtf8 = 1005,
    MouseFormatSgr = 1006,
    MouseAlternateScroll = 1007,
    MouseFormatUrxvt = 1015,
    MouseFormatSgrPixels = 1016,
    IgnoreKeypadWithNumlock = 1035,
    AltEscPrefix = 1036,
    AltSendsEscape = 1039,
    ReverseWrapExtended = 1045,
    AltScreen = 1047,
    SaveCursor = 1048,
    AltScreenSaveCursorClearEnter = 1049,
    BracketedPaste = 2004,
    SynchronizedOutput = 2026,
    GraphemeCluster = 2027,
    ReportColorScheme = 2031,
    InBandSizeReports = 2048,
}

impl Mode {
    /// Iterate every mode in declaration order. Mirrors the reference
    /// `@typeInfo(Mode).@"enum".fields` iteration used by the formatter to emit
    /// all non-default modes.
    pub fn all() -> impl Iterator<Item = Mode> {
        MODE_BY_INDEX.iter().copied()
    }
}

/// The tag type for our enum is a u16 but we use a packed struct
/// in order to pack the ansi bit into the tag.
// PORT NOTE: the reference is `packed struct(u16) { value: u15, ansi: bool }`.
// Zig packs `value` into bits 0..14 and `ansi` into bit 15. We model it with
// explicit fields plus from/to-int helpers since Rust has no bitfield structs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeTag {
    pub value: u16, // u15 in the reference (low 15 bits)
    pub ansi: bool, // default false
}

// pub const Backing = @typeInfo(@This()).@"struct".backing_integer.?;
// PORT NOTE: Rust has no inherent associated type alias on a struct; exposed as
// a module-level alias.
pub type ModeTagBacking = u16;

impl ModeTag {
    /// Construct a ModeTag with the default `ansi = false`.
    pub fn new(value: u16) -> ModeTag {
        ModeTag { value, ansi: false }
    }

    /// `@bitCast` to the backing u16: value in low 15 bits, ansi in bit 15.
    pub fn to_backing(self) -> u16 {
        (self.value & 0x7FFF) | ((self.ansi as u16) << 15)
    }

    /// `@bitCast` from the backing u16.
    pub fn from_backing(int: u16) -> ModeTag {
        ModeTag {
            value: int & 0x7FFF,
            ansi: (int >> 15) & 1 == 1,
        }
    }

    pub fn from_mode(mode: Mode) -> ModeTag {
        // return @bitCast(@intFromEnum(mode));
        ModeTag::from_backing(mode as u16)
    }
}

pub fn mode_from_int(v: u16, ansi: bool) -> Option<Mode> {
    // inline for (entries) |entry| {
    //     if (comptime !entry.disabled) {
    //         if (entry.value == v and entry.ansi == ansi) {
    //             const tag: ModeTag = .{ .ansi = ansi, .value = entry.value };
    //             const int: ModeTag.Backing = @bitCast(tag);
    //             return @enumFromInt(int);
    //         }
    //     }
    // }
    for (i, entry) in ENTRIES.iter().enumerate() {
        if !entry.disabled && entry.value == v && entry.ansi == ansi {
            let tag = ModeTag {
                ansi,
                value: entry.value,
            };
            let int: u16 = tag.to_backing();
            debug_assert_eq!(int, MODE_BY_INDEX[i] as u16);
            return Some(MODE_BY_INDEX[i]);
        }
    }

    None
}

/// A DECRPM mode report response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub tag: ModeTag,
    pub state: State,
}

impl Report {
    // PORT NOTE: the reference `max_size` is a comptime value computed by
    // encoding the largest possible report and counting bytes. The largest
    // report is `tag.value = maxInt(u15) = 32767`, `ansi = false` (so the "?"
    // prefix is present), `state = permanently_reset` (single digit 4):
    // "\x1B[?32767;4$y" == 12 bytes.
    pub const MAX_SIZE: usize = {
        // Construct the largest possible report in terms of values.
        // (Computed verbatim from the encode of the worst-case report.)
        "\x1B[?32767;4$y".len()
    };
}

/// The state of a mode as reported in a DECRPM response.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    NotRecognized = 0,
    Set = 1,
    Reset = 2,
    PermanentlySet = 3,
    PermanentlyReset = 4,
}

impl Report {
    /// Encode the DECRPM report sequence.
    pub fn encode<W: Write>(&self, writer: &mut W) -> core::fmt::Result {
        // try writer.print("\x1B[{s}{};{}$y", .{
        //     if (self.tag.ansi) "" else "?",
        //     self.tag.value,
        //     @intFromEnum(self.state),
        // });
        write!(
            writer,
            "\x1B[{}{};{}$y",
            if self.tag.ansi { "" } else { "?" },
            self.tag.value,
            self.state as u8,
        )
    }
}

// fn entryForMode(comptime mode: Mode) ModeEntry { ... }
// PORT NOTE: the reference resolves the entry by matching `@tagName(mode)`
// against `entry.name`. In the bit-indexed port we instead need the entry's
// INDEX (its bit position); `index_for_mode` returns it. The lookup is by the
// Mode variant rather than by string name.
fn index_for_mode(mode: Mode) -> usize {
    let mut i = 0;
    while i < MODE_BY_INDEX.len() {
        if MODE_BY_INDEX[i] as u16 == mode as u16 {
            return i;
        }
        i += 1;
    }
    unreachable!()
}

/// A single entry of a possible mode we support. This is used to
/// dynamically define the enum and other tables.
// PORT NOTE: the reference uses `comptime_int` for `value` (the mode number);
// we use `u16`.
struct ModeEntry {
    #[allow(dead_code)]
    name: &'static str,
    value: u16,
    default: bool,

    /// True if this is an ANSI mode, false if its a DEC mode (?-prefixed).
    ansi: bool,

    /// If true, this mode is disabled and Ghostty will not allow it to be
    /// set or queried. The mode enum still has it, allowing Ghostty developers
    /// to develop a mode without exposing it to real users.
    disabled: bool,
}

// PORT NOTE: a const constructor for ModeEntry with the reference field defaults
// (`default = false`, `ansi = false`, `disabled = false`), so the ENTRIES table
// reads like the reference struct literals.
const fn e(name: &'static str, value: u16) -> ModeEntry {
    ModeEntry {
        name,
        value,
        default: false,
        ansi: false,
        disabled: false,
    }
}

const fn e_ansi(name: &'static str, value: u16) -> ModeEntry {
    ModeEntry {
        name,
        value,
        default: false,
        ansi: true,
        disabled: false,
    }
}

const fn e_default(mut entry: ModeEntry) -> ModeEntry {
    entry.default = true;
    entry
}

/// The full list of available entries. For documentation see how
/// they're used within Ghostty or google their values. It is not
/// valuable to redocument them all here.
///
/// NOTE: When adding a new mode entry, also add a corresponding
/// GHOSTTY_MODE_* macro in include/ghostty/vt/modes.h.
const ENTRIES: &[ModeEntry] = &[
    // ANSI
    e_ansi("disable_keyboard", 2), // KAM
    e_ansi("insert", 4),
    e_default(e_ansi("send_receive_mode", 12)), // SRM
    e_ansi("linefeed", 20),
    // DEC
    e("cursor_keys", 1), // DECCKM
    e("132_column", 3),
    e("slow_scroll", 4),
    e("reverse_colors", 5),
    e("origin", 6),
    e_default(e("wraparound", 7)),
    e("autorepeat", 8),
    e("mouse_event_x10", 9),
    e("cursor_blinking", 12),
    e_default(e("cursor_visible", 25)),
    e("enable_mode_3", 40),
    e("reverse_wrap", 45),
    e("alt_screen_legacy", 47),
    e("keypad_keys", 66),
    // DEC Backarrow Key Mode (DECBKM)
    // See https://vt100.net/dec/ek-vt3xx-tp-002.pdf page 170
    // If `false` (the default), `backspace` emits 0x7f
    // If `true`, `backspace` emits 0x08
    e("backarrow_key_mode", 67),
    e("enable_left_and_right_margin", 69),
    e("mouse_event_normal", 1000),
    e("mouse_event_button", 1002),
    e("mouse_event_any", 1003),
    e("focus_event", 1004),
    e("mouse_format_utf8", 1005),
    e("mouse_format_sgr", 1006),
    e_default(e("mouse_alternate_scroll", 1007)),
    e("mouse_format_urxvt", 1015),
    e("mouse_format_sgr_pixels", 1016),
    e_default(e("ignore_keypad_with_numlock", 1035)),
    e_default(e("alt_esc_prefix", 1036)),
    e("alt_sends_escape", 1039),
    e("reverse_wrap_extended", 1045),
    e("alt_screen", 1047),
    e("save_cursor", 1048),
    e("alt_screen_save_cursor_clear_enter", 1049),
    e("bracketed_paste", 2004),
    e("synchronized_output", 2026),
    e("grapheme_cluster", 2027),
    e("report_color_scheme", 2031),
    e("in_band_size_reports", 2048),
];

// PORT NOTE: the Mode variant for each `ENTRIES` index, in the same order. This
// pairs the bit-indexed `ENTRIES` table with the generated `Mode` enum (the
// reference keeps them in sync via comptime; we keep this table in sync by
// construction and the `entries_match_mode` test verifies it).
const MODE_BY_INDEX: &[Mode] = &[
    Mode::DisableKeyboard,
    Mode::Insert,
    Mode::SendReceiveMode,
    Mode::Linefeed,
    Mode::CursorKeys,
    Mode::Column132,
    Mode::SlowScroll,
    Mode::ReverseColors,
    Mode::Origin,
    Mode::Wraparound,
    Mode::Autorepeat,
    Mode::MouseEventX10,
    Mode::CursorBlinking,
    Mode::CursorVisible,
    Mode::EnableMode3,
    Mode::ReverseWrap,
    Mode::AltScreenLegacy,
    Mode::KeypadKeys,
    Mode::BackarrowKeyMode,
    Mode::EnableLeftAndRightMargin,
    Mode::MouseEventNormal,
    Mode::MouseEventButton,
    Mode::MouseEventAny,
    Mode::FocusEvent,
    Mode::MouseFormatUtf8,
    Mode::MouseFormatSgr,
    Mode::MouseAlternateScroll,
    Mode::MouseFormatUrxvt,
    Mode::MouseFormatSgrPixels,
    Mode::IgnoreKeypadWithNumlock,
    Mode::AltEscPrefix,
    Mode::AltSendsEscape,
    Mode::ReverseWrapExtended,
    Mode::AltScreen,
    Mode::SaveCursor,
    Mode::AltScreenSaveCursorClearEnter,
    Mode::BracketedPaste,
    Mode::SynchronizedOutput,
    Mode::GraphemeCluster,
    Mode::ReportColorScheme,
    Mode::InBandSizeReports,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_packed_size() {
        // We have this here so that we explicitly fail when we change the
        // size of modes. The size of modes is NOT particularly important,
        // we just want to be mentally aware when it happens.
        // PORT NOTE: the reference `@sizeOf(ModePacked) == 8` — the bit-packed
        // struct is backed by a u64; our `ModePacked(u64)` is 8 bytes exactly.
        assert_eq!(8, core::mem::size_of::<ModePacked>());
    }

    #[test]
    fn order() {
        // test "order" within ModeTag {
        //     const t: ModeTag = .{ .value = 1 };
        //     const int: Backing = @bitCast(t);
        //     try std.testing.expectEqual(@as(Backing, 1), int);
        // }
        let t = ModeTag::new(1);
        let int: u16 = t.to_backing();
        assert_eq!(1_u16, int);
    }

    // PORT NOTE: the reference root `test { _ = Mode; _ = ModePacked; }` is a
    // refAllDecls smoke test; the consistency it implies (entries <-> Mode <->
    // ModePacked bit count) is checked concretely here.
    #[test]
    fn entries_match_mode() {
        assert_eq!(ENTRIES.len(), MODE_BY_INDEX.len());
        for (i, entry) in ENTRIES.iter().enumerate() {
            let tag = ModeTag {
                value: entry.value,
                ansi: entry.ansi,
            };
            assert_eq!(tag.to_backing(), MODE_BY_INDEX[i] as u16);
        }
        // The packed struct must hold one bit per entry.
        assert!(ENTRIES.len() <= 64);
    }

    #[test]
    fn mode_from_int_test() {
        assert!(mode_from_int(4, true).unwrap() == Mode::Insert);
        assert!(mode_from_int(9, true).is_none());
        assert!(mode_from_int(9, false).unwrap() == Mode::MouseEventX10);
        assert!(mode_from_int(14, true).is_none());
    }

    #[test]
    fn mode_state() {
        let mut state: ModeState = ModeState::default();

        // Normal set/get
        assert!(!state.get(Mode::CursorKeys));
        state.set(Mode::CursorKeys, true);
        assert!(state.get(Mode::CursorKeys));

        // Save/restore
        state.save(Mode::CursorKeys);
        state.set(Mode::CursorKeys, false);
        assert!(!state.get(Mode::CursorKeys));
        assert!(state.restore(Mode::CursorKeys));
        assert!(state.get(Mode::CursorKeys));
    }

    #[test]
    fn get_report_known_dec_mode() {
        let mut state: ModeState = ModeState::default();
        let report = state.get_report(ModeTag::new(1));
        assert_eq!(State::Reset, report.state);
        assert_eq!(false, report.tag.ansi);
        assert_eq!(1_u16, report.tag.value);

        state.set(Mode::CursorKeys, true);
        let report2 = state.get_report(ModeTag::new(1));
        assert_eq!(State::Set, report2.state);
    }

    #[test]
    fn get_report_known_ansi_mode() {
        let mut state: ModeState = ModeState::default();
        state.set(Mode::Insert, true);
        let report = state.get_report(ModeTag {
            value: 4,
            ansi: true,
        });
        assert_eq!(State::Set, report.state);
        assert_eq!(true, report.tag.ansi);
    }

    #[test]
    fn get_report_unknown_mode() {
        let state: ModeState = ModeState::default();
        let report = state.get_report(ModeTag::new(9999));
        assert_eq!(State::NotRecognized, report.state);
    }

    #[test]
    fn report_encode_dec_mode_set() {
        let mut buf = String::new();
        let report = Report {
            tag: ModeTag {
                value: 1,
                ansi: false,
            },
            state: State::Set,
        };
        report.encode(&mut buf).unwrap();
        assert_eq!("\x1B[?1;1$y", buf);
    }

    #[test]
    fn report_encode_dec_mode_reset() {
        let mut buf = String::new();
        let report = Report {
            tag: ModeTag {
                value: 1,
                ansi: false,
            },
            state: State::Reset,
        };
        report.encode(&mut buf).unwrap();
        assert_eq!("\x1B[?1;2$y", buf);
    }

    #[test]
    fn report_encode_ansi_mode() {
        let mut buf = String::new();
        let report = Report {
            tag: ModeTag {
                value: 4,
                ansi: true,
            },
            state: State::Set,
        };
        report.encode(&mut buf).unwrap();
        assert_eq!("\x1B[4;1$y", buf);
    }

    #[test]
    fn report_encode_not_recognized() {
        let mut buf = String::new();
        let report = Report {
            tag: ModeTag {
                value: 9999,
                ansi: false,
            },
            state: State::NotRecognized,
        };
        report.encode(&mut buf).unwrap();
        assert_eq!("\x1B[?9999;0$y", buf);
    }
}
