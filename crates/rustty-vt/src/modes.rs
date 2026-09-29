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

// const ComptimeIntSet = @import("../datastruct/main.zig").ComptimeIntSet;
use rustty_datastruct::ComptimeIntSet;

/// A struct that maintains the state of all the settable modes.
// PORT NOTE: PartialEq/Eq are derived for the snapshot TERMINAL codec's
// round-trip test (Zig: `std.testing.expectEqualDeep(terminal.modes, ...)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

// PORT NOTE (P3-W4 T2, has no upstream counterpart): an instrumentation counter
// for how many times synchronized output has been CLEARED
// (`set(.synchronized_output, false)`).
//
// Why it exists: `89b103dd5` MOVES the synchronized-output reset (and the pixel
// geometry update) OUT of every resize wrapper — `termio/Termio.zig`,
// `terminal/c/terminal.zig` — and INTO `Terminal.resize`. A port that keeps both
// copies double-clears, which is behaviourally invisible; a port that deletes
// both without adding the terminal-side move loses the reset entirely, and no
// resize KAT would notice either way. Only a COUNT distinguishes 0 / 1 / 2.
//
// Gating: `cfg(debug_assertions)`, NOT `cfg(test)`. `cfg(test)` is only true for
// the crate currently compiled with `--test`, so the counter would read zero
// from `rustty-vt-c`'s and `rustty-termio`'s tests, which are exactly the two
// call sites this guards. `debug_assertions` is the same stand-in the tripwire
// module uses for Zig's `builtin.is_test`; see its PORT NOTE.
#[cfg(debug_assertions)]
thread_local! {
    static SYNC_OUTPUT_CLEARS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

#[cfg(debug_assertions)]
#[inline]
fn sync_output_clear_bump() {
    SYNC_OUTPUT_CLEARS.with(|c| c.set(c.get() + 1));
}

/// Number of `set(.synchronized_output, false)` calls on this thread since the
/// last `sync_output_clear_reset`. Instrumentation only; see the PORT NOTE.
#[cfg(debug_assertions)]
pub fn sync_output_clear_count() -> usize {
    SYNC_OUTPUT_CLEARS.with(|c| c.get())
}

/// Reset the synchronized-output clear counter for this thread.
#[cfg(debug_assertions)]
pub fn sync_output_clear_reset() {
    SYNC_OUTPUT_CLEARS.with(|c| c.set(0));
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
        #[cfg(debug_assertions)]
        if matches!(mode, Mode::SynchronizedOutput) && !value {
            sync_output_clear_bump();
        }
        set_packed(&mut self.values, mode, value);
    }

    /// Set the reset default and current value for a mode.
    pub fn set_default(&mut self, mode: Mode, value: bool) {
        set_packed(&mut self.values, mode, value);
        set_packed(&mut self.default, mode, value);
    }

    /// Get the value of a mode.
    pub fn get(&self, mode: Mode) -> bool {
        get_packed(&self.values, mode)
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
    pub fn get_report(&self, tag: ReportTag) -> Report {
        // DECECM (Erase Color Mode, DEC private mode 117) controls whether erasing
        // and scrolling use the default background or the active background color.
        // Rustty's behavior is fixed equivalent to DECECM reset, and DECRQM has a
        // "permanently reset" response for recognized modes that cannot be changed.
        // Report that instead of "not recognized" so applications can query and adapt
        // to Rustty's erase-color behavior.
        //
        // See VT520/VT525 Programmer Information, "Erase Color" and DECRQM/DECRPM:
        // https://web.mit.edu/dosathena/doc/www/ek-vt520-rm.pdf

        if !tag.ansi && tag.value == 117 {
            return Report {
                tag,
                state: State::PermanentlyReset,
            };
        }
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

// PORT NOTE: the reference `switch (mode) { inline else => ... }` resolves
// `entryForMode(mode)` at comptime and sets/reads the matching ModePacked
// field. We set/read the corresponding bit by the entry index.
fn set_packed(values: &mut ModePacked, mode: Mode, value: bool) {
    let entry_idx = index_for_mode(mode);
    values.set_bit(entry_idx, value);
}

fn get_packed(values: &ModePacked, mode: Mode) -> bool {
    let entry_idx = index_for_mode(mode);
    values.get_bit(entry_idx)
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

    /// Set the value of a mode in this packed set. Mirrors the reference's
    /// direct packed-field writes (`packed.disable_keyboard = true`); the
    /// snapshot codec's mode-registry tests and fixtures construct packed
    /// values this way.
    pub fn set_mode(&mut self, mode: Mode, value: bool) {
        self.set_bit(index_for_mode(mode), value);
    }

    /// `@bitSizeOf(ModePacked)` — the number of packed mode bits (one per
    /// `ENTRIES` row). The snapshot TERMINAL record's mode registry writes
    /// these bits zero-extended into a u64 wire slot and treats the higher
    /// bits as reserved.
    pub const BITS: usize = ENTRIES.len();

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
    ReportVisibility = 2033,
    InBandSizeReports = 2048,
    KittyPasteEvents = 5522,
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

/// Returns the mode for a mode number as it appears in a control
/// sequence, or null if the mode is unknown or disabled.
///
/// ANSI and DEC private modes are numbered separately, so the same
/// number can mean two different modes. `ansi` selects which one. For
/// example, `CSI ? 4 h` is `modeFromInt(4, false)` (slow scroll) and
/// `CSI 4 h` is `modeFromInt(4, true)` (insert).
#[inline]
pub fn mode_from_int(v: u16, ansi: bool) -> Option<Mode> {
    // ModeTag stores the number in 15 bits. A larger number can't be a
    // known mode, and packing it below would spill into the ansi bit.
    if v > 0x7FFF {
        return None;
    }

    // This runs for every mode that is set, reset, or queried, so we use
    // a sorted lookup instead of comparing against each entry in turn.
    let tag = ModeTag { ansi, value: v };
    let int: ModeTagBacking = tag.to_backing();
    let idx = ENTRY_SET.index_of(int)?;
    if ENTRIES_ANY_DISABLED && ENTRIES_DISABLED[idx] {
        return None;
    }

    // PORT NOTE (R8 deviation, parity S6-F2): the artifact-dependent
    // `.disabled` of the `kitty_paste_events` entry is not part of the
    // comptime `entries_disabled` fold above; it is checked at run time,
    // only once the index lookup has matched that entry. See
    // `ModeEntry::disabled_by_artifact`.
    if ENTRIES_DISABLED_BY_ARTIFACT[idx] && kitty_paste_events_disabled() {
        return None;
    }

    // return @enumFromInt(int);
    // PORT NOTE: `Mode` is a closed Rust enum, so the index selects the
    // variant; its discriminant is `int` by construction
    // (`entries_match_mode`).
    let mode = MODE_BY_INDEX[idx];
    debug_assert_eq!(int, mode as u16);
    Some(mode)
}

// PORT NOTE: `entries.len` as a const, so the per-entry const tables below
// can be arrays.
const ENTRIES_LEN: usize = ENTRIES.len();

/// The tag of every entry, in the same order as `entries`. Looking up
/// a tag here returns the index of its entry.
const ENTRY_SET: ComptimeIntSet<ModeTagBacking, ENTRIES_LEN> =
    ComptimeIntSet::<ModeTagBacking, ENTRIES_LEN>::new({
        let mut keys = [0 as ModeTagBacking; ENTRIES_LEN];
        let mut i = 0;
        while i < ENTRIES_LEN {
            // @bitCast(ModeTag{ .value = entry.value, .ansi = entry.ansi })
            keys[i] = (ENTRIES[i].value & 0x7FFF) | ((ENTRIES[i].ansi as u16) << 15);
            i += 1;
        }
        keys
    });

/// Whether each entry is disabled, in the same order as `entries`.
const ENTRIES_DISABLED: [bool; ENTRIES_LEN] = {
    let mut result = [false; ENTRIES_LEN];
    let mut i = 0;
    while i < ENTRIES_LEN {
        result[i] = ENTRIES[i].disabled;
        i += 1;
    }
    result
};

/// True if any entry is disabled. When false, modeFromInt skips the
/// disabled check entirely.
const ENTRIES_ANY_DISABLED: bool = {
    let mut any = false;
    let mut i = 0;
    while i < ENTRIES_LEN {
        if ENTRIES[i].disabled {
            any = true;
        }
        i += 1;
    }
    any
};

// PORT NOTE (parity S6-F2): whether each entry's `.disabled` is the
// artifact-dependent predicate, in the same order as `entries`. The run-time
// half of the reference's `entries_disabled`.
const ENTRIES_DISABLED_BY_ARTIFACT: [bool; ENTRIES_LEN] = {
    let mut result = [false; ENTRIES_LEN];
    let mut i = 0;
    while i < ENTRIES_LEN {
        result[i] = ENTRIES[i].disabled_by_artifact;
        i += 1;
    }
    result
};

/// A DECRPM mode report response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub tag: ReportTag,
    pub state: State,
}

/// A query identifier can use the full parser parameter range. Keep it
/// separate from ModeTag, which packs supported modes and the ANSI flag
/// into a u16 and is also used by the public C API.
// PORT NOTE: the reference nests this as `Report.Tag`; Rust has no nested
// type declarations, so it is the module-level `ReportTag`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportTag {
    pub value: u16,
    pub ansi: bool, // default false
}

impl ReportTag {
    /// Construct a ReportTag with the default `ansi = false`.
    pub fn new(value: u16) -> ReportTag {
        ReportTag { value, ansi: false }
    }

    pub fn from_mode(mode: Mode) -> ReportTag {
        let tag = ModeTag::from_mode(mode);
        ReportTag {
            value: tag.value,
            ansi: tag.ansi,
        }
    }
}

impl Report {
    // PORT NOTE: the reference `max_size` is a comptime value computed by
    // encoding the largest possible report and counting bytes. The largest
    // report is `tag.value = maxInt(u16) = 65535`, `ansi = false` (so the "?"
    // prefix is present), `state = permanently_reset` (single digit 4):
    // "\x1B[?65535;4$y" == 12 bytes.
    pub const MAX_SIZE: usize = {
        // Construct the largest possible report in terms of values.
        // (Computed verbatim from the encode of the worst-case report.)
        "\x1B[?65535;4$y".len()
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

    /// Whether an embedder may safely configure this bit as reset policy.
    /// Modes that perform a transition or mirror additional terminal state
    /// must use their semantic configuration API instead.
    default_configurable: bool,

    // PORT NOTE (R8 deviation, parity S6-F2): the one `.disabled` predicate
    // that reads `build_options.artifact` (`kitty_paste_events`,
    // `modes.zig:341`) is evaluated at run time against
    // `options::runtime_artifact()`, and only when an entry's value and
    // `ansi` already match, so the cost is one relaxed load on that single
    // path. True only for that entry; see `kitty_paste_events_disabled`.
    disabled_by_artifact: bool,
}

/// Return whether a mode can safely be configured as a reset default by an
/// embedder. This excludes modes whose set/reset operations have side effects
/// beyond changing the mode bit.
pub fn default_configurable(mode: Mode) -> bool {
    // PORT NOTE: the reference resolves `entryForMode(mode).default_configurable`
    // at comptime; we index the ENTRIES table by the mode's bit index.
    ENTRIES[index_for_mode(mode)].default_configurable
}

// PORT NOTE: a const constructor for ModeEntry with the reference field defaults
// (`default = false`, `ansi = false`, `disabled = false`,
// `default_configurable = true`), so the ENTRIES table reads like the reference
// struct literals.
const fn e(name: &'static str, value: u16) -> ModeEntry {
    ModeEntry {
        name,
        value,
        default: false,
        ansi: false,
        disabled: false,
        default_configurable: true,
        disabled_by_artifact: false,
    }
}

const fn e_ansi(name: &'static str, value: u16) -> ModeEntry {
    ModeEntry {
        name,
        value,
        default: false,
        ansi: true,
        disabled: false,
        default_configurable: true,
        disabled_by_artifact: false,
    }
}

const fn e_default(mut entry: ModeEntry) -> ModeEntry {
    entry.default = true;
    entry
}

// PORT NOTE: mirrors the reference's `.default_configurable = false` overrides
// in the entries table.
const fn e_not_default_configurable(mut entry: ModeEntry) -> ModeEntry {
    entry.default_configurable = false;
    entry
}

// PORT NOTE: mirrors the reference's per-entry `.disabled = <predicate>`
// override (the `kitty_paste_events` entry is the only user). Its predicate
// reads the artifact, which the port knows only at run time (parity S6-F2),
// so the entry is marked and `mode_from_int` evaluates
// `kitty_paste_events_disabled()` when it matches.
const fn e_disabled_by_artifact(mut entry: ModeEntry) -> ModeEntry {
    entry.disabled_by_artifact = true;
    entry
}

/// The full list of available entries. For documentation see how
/// they're used within Ghostty or google their values. It is not
/// valuable to redocument them all here.
///
/// The reference's note here reads:
///
///   NOTE: When adding a new mode entry, also add a corresponding
///   GHOSTTY_MODE_* macro in include/ghostty/vt/modes.h.
///
/// NOTE: the reference (ghostty) keeps a parallel `GHOSTTY_MODE_*` macro per
/// entry in its own `include/ghostty/vt/modes.h` embedding header. Rustty's
/// vt-c C API (`crates/rustty-vt-c/src/modes.rs`) does not yet surface a
/// matching named-macro C enum for individual mode entries (`ReportState` is
/// a bare `c_int` newtype) — when adding a new mode entry here, cross-check
/// the reference header for the oracle's numbering, and if/when Rustty grows
/// its own named-macro surface, add the `RUSTTY_MODE_*` entry there too.
const ENTRIES: &[ModeEntry] = &[
    // ANSI
    e_ansi("disable_keyboard", 2), // KAM
    e_ansi("insert", 4),
    e_default(e_ansi("send_receive_mode", 12)), // SRM
    e_ansi("linefeed", 20),
    // DEC
    e("cursor_keys", 1), // DECCKM
    e_not_default_configurable(e("132_column", 3)),
    e("slow_scroll", 4),
    e("reverse_colors", 5),
    e_not_default_configurable(e("origin", 6)),
    e_default(e("wraparound", 7)),
    e("autorepeat", 8),
    e_not_default_configurable(e("mouse_event_x10", 9)),
    e_not_default_configurable(e("cursor_blinking", 12)),
    e_default(e("cursor_visible", 25)),
    e("enable_mode_3", 40),
    e("reverse_wrap", 45),
    e_not_default_configurable(e("alt_screen_legacy", 47)),
    e("keypad_keys", 66),
    // DEC Backarrow Key Mode (DECBKM)
    // See https://vt100.net/dec/ek-vt3xx-tp-002.pdf page 170
    // If `false` (the default), `backspace` emits 0x7f
    // If `true`, `backspace` emits 0x08
    e("backarrow_key_mode", 67),
    e_not_default_configurable(e("enable_left_and_right_margin", 69)),
    e_not_default_configurable(e("mouse_event_normal", 1000)),
    e_not_default_configurable(e("mouse_event_button", 1002)),
    e_not_default_configurable(e("mouse_event_any", 1003)),
    e("focus_event", 1004),
    e_not_default_configurable(e("mouse_format_utf8", 1005)),
    e_not_default_configurable(e("mouse_format_sgr", 1006)),
    e_default(e("mouse_alternate_scroll", 1007)),
    e_not_default_configurable(e("mouse_format_urxvt", 1015)),
    e_not_default_configurable(e("mouse_format_sgr_pixels", 1016)),
    e_default(e("ignore_keypad_with_numlock", 1035)),
    e_default(e("alt_esc_prefix", 1036)),
    e("alt_sends_escape", 1039),
    e("reverse_wrap_extended", 1045),
    e_not_default_configurable(e("alt_screen", 1047)),
    e_not_default_configurable(e("save_cursor", 1048)),
    e_not_default_configurable(e("alt_screen_save_cursor_clear_enter", 1049)),
    e("bracketed_paste", 2004),
    e_not_default_configurable(e("synchronized_output", 2026)),
    e("grapheme_cluster", 2027),
    e("report_color_scheme", 2031),
    e_not_default_configurable(e("report_visibility", 2033)),
    e("in_band_size_reports", 2048),
    // Kitty clipboard protocol paste events. When set, a user-initiated
    // paste sends an unsolicited OSC 5522 targets listing with a
    // one-time password instead of pasting the text.
    // See https://sw.kovidgoyal.net/kitty/clipboard/
    // .{
    //     .name = "kitty_paste_events",
    //     .value = 5522,
    //     // The macOS app and libghostty-vt can both serve the follow-up
    //     // Kitty clipboard read that a paste event grants.
    //     .disabled = build_options.artifact != .lib and builtin.os.tag != .macos,
    // },
    e_disabled_by_artifact(e("kitty_paste_events", 5522)),
];

// The macOS app and libghostty-vt can both serve the follow-up
// Kitty clipboard read that a paste event grants.
// PORT NOTE (R8, cfg not folded): the reference predicate is
// `build_options.artifact != .lib and builtin.os.tag != .macos`.
// `builtin.os.tag` maps to `cfg!(target_os)`. `artifact` maps to
// `options::runtime_artifact()`, NOT the comptime `options::ARTIFACT`
// (R8 deviation, parity S6-F2, Decision A: a comptime switch cannot differ
// between the app and `rustty-vt-c` in one cargo build; the PORT NOTE on
// `options::runtime_artifact` argues why no other artifact can observe the
// run-time value). Both halves are carried, never collapsed to a constant.
// The OS half is tested first, so macOS pays nothing.
#[inline]
fn kitty_paste_events_disabled() -> bool {
    !cfg!(target_os = "macos")
        && kitty_paste_events_disabled_for(crate::options::runtime_artifact(), false)
}

/// The predicate itself, over explicit inputs (testable on every host).
const fn kitty_paste_events_disabled_for(
    artifact: crate::build_options::Artifact,
    os_is_macos: bool,
) -> bool {
    !matches!(artifact, crate::build_options::Artifact::Lib) && !os_is_macos
}

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
    Mode::ReportVisibility,
    Mode::InBandSizeReports,
    Mode::KittyPasteEvents,
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

        // Numbers too large for the tag must not be mistaken for an ANSI mode.
        assert!(mode_from_int(4 | 0x8000, false).is_none());
        assert!(mode_from_int(u16::MAX, true).is_none());
    }

    // test "modeFromInt matches entries exhaustively" (oracle 079502e23)
    //
    // PORT NOTE (R14, DECISIONS R12): the reference runs all 2 x 65,536
    // inputs. Under Miri that is too slow, so a Miri run is capped: it
    // checks every `STRIDE`-th input plus every entry's number and its
    // neighbours and the u15/u16 boundaries, and prints the cap. Outside
    // Miri the loop is the full, uncapped run.
    #[test]
    fn mode_from_int_matches_entries_exhaustively() {
        #[cfg(miri)]
        const STRIDE: usize = 257;
        #[cfg(not(miri))]
        const STRIDE: usize = 1;

        let mut inputs: Vec<usize> = (0..u16::MAX as usize + 1).step_by(STRIDE).collect();
        if STRIDE != 1 {
            for entry in ENTRIES {
                let v = entry.value as usize;
                inputs.extend([v.saturating_sub(1), v, v + 1, v | 0x8000]);
            }
            inputs.extend([0x7FFE, 0x7FFF, 0x8000, 0x8001, 0xFFFE, 0xFFFF]);
            inputs.retain(|&v| v <= u16::MAX as usize);
            eprintln!(
                "R14 MIRI CAP (DECISIONS R12): modeFromInt matches entries exhaustively \
                 checks {} of 65536 inputs per namespace (stride {STRIDE} plus every \
                 entry +/-1 and the u15/u16 boundaries); the full run is outside Miri",
                inputs.len()
            );
        }

        // Compare every possible input against a plain scan of the entries.
        for ansi in [false, true] {
            for &v in &inputs {
                let expected: Option<Mode> = 'expected: {
                    for (i, entry) in ENTRIES.iter().enumerate() {
                        if !entry.disabled && entry.value as usize == v && entry.ansi == ansi {
                            // The run-time half of `.disabled` (parity S6-F2).
                            if entry.disabled_by_artifact && kitty_paste_events_disabled() {
                                break 'expected None;
                            }
                            break 'expected Some(MODE_BY_INDEX[i]);
                        }
                    }
                    None
                };
                assert_eq!(expected, mode_from_int(v as u16, ansi), "v={v} ansi={ansi}");
            }
        }
    }

    /// Parity S6-F2: mode 5522's `.disabled` is
    /// `artifact != .lib and os != .macos`, over both inputs.
    #[test]
    fn kitty_paste_events_disabled_predicate_reads_artifact_and_os() {
        use crate::build_options::Artifact;
        assert!(kitty_paste_events_disabled_for(Artifact::Ghostty, false));
        assert!(!kitty_paste_events_disabled_for(Artifact::Ghostty, true));
        assert!(!kitty_paste_events_disabled_for(Artifact::Lib, false));
        assert!(!kitty_paste_events_disabled_for(Artifact::Lib, true));
        // Only the 5522 entry depends on the artifact.
        for entry in ENTRIES {
            assert_eq!(
                entry.disabled_by_artifact,
                entry.value == 5522,
                "{}",
                entry.name
            );
        }
    }

    /// Parity S6-F2: in this crate's tests the process is the application
    /// (`options::runtime_artifact()` is `Ghostty`), so mode 5522 is not
    /// recognized off macOS and recognized on macOS, as in the reference's
    /// `.ghostty` build. `rustty-vt-c`'s `terminal` tests cover the lib side.
    #[test]
    fn mode_5522_follows_the_application_artifact() {
        assert_eq!(
            crate::options::runtime_artifact(),
            crate::build_options::Artifact::Ghostty
        );
        let expected = if cfg!(target_os = "macos") {
            Some(Mode::KittyPasteEvents)
        } else {
            None
        };
        assert_eq!(mode_from_int(5522, false), expected);
        let state = ModeState::default();
        let report = state.get_report(ReportTag {
            value: 5522,
            ansi: false,
        });
        assert_eq!(
            report.state == State::NotRecognized,
            expected.is_none(),
            "DECRQM must follow mode_from_int"
        );
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
    fn mode_state_set_default_updates_current_and_reset_value() {
        let mut state: ModeState = ModeState::default();

        state.set_default(Mode::GraphemeCluster, true);
        assert!(state.get(Mode::GraphemeCluster));
        assert!(state.default.get_mode(Mode::GraphemeCluster));

        state.set(Mode::GraphemeCluster, false);
        assert!(!state.get(Mode::GraphemeCluster));
        assert!(state.default.get_mode(Mode::GraphemeCluster));

        state.set_default(Mode::GraphemeCluster, true);
        assert!(state.get(Mode::GraphemeCluster));

        state.set(Mode::GraphemeCluster, false);
        state.reset();
        assert!(state.get(Mode::GraphemeCluster));
    }

    #[test]
    fn default_configurable_modes() {
        assert!(default_configurable(Mode::GraphemeCluster));
        assert!(default_configurable(Mode::Wraparound));
        assert!(!default_configurable(Mode::AltScreen));
        assert!(!default_configurable(Mode::CursorBlinking));
    }

    #[test]
    fn get_report_known_dec_mode() {
        let mut state: ModeState = ModeState::default();
        let report = state.get_report(ReportTag::new(1));
        assert_eq!(State::Reset, report.state);
        assert_eq!(false, report.tag.ansi);
        assert_eq!(1_u16, report.tag.value);

        state.set(Mode::CursorKeys, true);
        let report2 = state.get_report(ReportTag::new(1));
        assert_eq!(State::Set, report2.state);
    }

    #[test]
    fn get_report_known_ansi_mode() {
        let mut state: ModeState = ModeState::default();
        state.set(Mode::Insert, true);
        let report = state.get_report(ReportTag {
            value: 4,
            ansi: true,
        });
        assert_eq!(State::Set, report.state);
        assert_eq!(true, report.tag.ansi);
    }

    #[test]
    fn get_report_dececm_permanently_reset() {
        let state: ModeState = ModeState::default();
        let report = state.get_report(ReportTag {
            value: 117,
            ansi: false,
        });
        assert_eq!(State::PermanentlyReset, report.state);
        assert_eq!(false, report.tag.ansi);
    }

    #[test]
    fn get_report_unknown_mode() {
        let state: ModeState = ModeState::default();
        let report = state.get_report(ReportTag::new(9999));
        assert_eq!(State::NotRecognized, report.state);
    }

    #[test]
    fn report_encode_dec_mode_set() {
        let mut buf = String::new();
        let report = Report {
            tag: ReportTag {
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
            tag: ReportTag {
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
            tag: ReportTag {
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
            tag: ReportTag {
                value: 9999,
                ansi: false,
            },
            state: State::NotRecognized,
        };
        report.encode(&mut buf).unwrap();
        assert_eq!("\x1B[?9999;0$y", buf);
    }

    // test "getReport large unknown modes" (oracle 3beb6d717)
    #[test]
    fn get_report_large_unknown_modes() {
        let state: ModeState = ModeState::default();
        // These would alias ANSI insert mode and fixed-status DEC mode 117.
        for tag in [
            ReportTag {
                value: 32772,
                ansi: true,
            },
            ReportTag::new(32885),
        ] {
            let report = state.get_report(tag);
            assert_eq!(State::NotRecognized, report.state);
            assert_eq!(tag, report.tag);
        }
    }

    // test "Report.encode maximum size" (oracle 3beb6d717)
    //
    // PORT NOTE: the reference encodes into a `.fixed` writer over a
    // `[Report.max_size]u8`, which fails on overflow; the port encodes into
    // a fixed-capacity sink that errors past `MAX_SIZE` bytes.
    #[test]
    fn report_encode_maximum_size() {
        struct Fixed {
            buf: [u8; Report::MAX_SIZE],
            len: usize,
        }
        impl Write for Fixed {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                let end = self.len + s.len();
                if end > self.buf.len() {
                    return Err(core::fmt::Error);
                }
                self.buf[self.len..end].copy_from_slice(s.as_bytes());
                self.len = end;
                Ok(())
            }
        }
        let mut writer = Fixed {
            buf: [0; Report::MAX_SIZE],
            len: 0,
        };
        let report = Report {
            tag: ReportTag::new(65535),
            state: State::PermanentlyReset,
        };
        report.encode(&mut writer).unwrap();
        assert_eq!(b"\x1b[?65535;4$y", &writer.buf[..writer.len]);
        assert_eq!(writer.buf.len(), writer.len);
    }
}
