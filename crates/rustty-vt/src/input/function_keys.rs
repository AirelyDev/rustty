//! This is the list of "PC style function keys" that xterm supports for
//! the legacy keyboard protocols. These always take priority since at the
//! time of writing this, even the most modern keyboard protocols still
//! are backwards compatible with regards to these sequences.
//!
//! This is based on a variety of sources cross-referenced but mostly
//! based on foot's keymap.h: https://codeberg.org/dnkl/foot/src/branch/master/keymap.h
//!
//! PORTS: ghostty src/input/function_keys.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports
//   const std = @import("std");
//   const key = @import("key.zig");
// `std` is used only for the comptime table builders (`std.EnumArray`,
// `std.fmt.comptimePrint`) and the test. `key` provides `key.Key` / `key.Mods`.

use std::sync::LazyLock;

use super::key::Key;
use super::key_mods::{ModSide, Mods, Side};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorMode {
    Any,
    Normal,
    Application,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeypadMode {
    Any,
    Normal,
    Application,
}

/// A bit confusing so I'll document this one: this is the "modify other keys"
/// setting. We only change behavior for "set_other" which is ESC [ > 4; 2 m.
/// So this can be "any" which means we don't care what's going on. Or it
/// can be "set" which means modify keys must be set EXCEPT FOR "other keys"
/// mode, and "set_other" which means modify keys must be set to "other keys"
/// mode.
///
/// See: https://invisible-island.net/xterm/modified-keys.html
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifyKeys {
    Any,
    Set,
    SetOther,
}

/// A single entry in the table of keys.
///
/// PORT NOTE: the reference `sequence` / `sequence_decbkm` are `[]const u8` /
/// `?[]const u8`. All sequences are either string literals or built by the
/// comptime `pcStyle` formatter; we store them as `&'static str` (literals) and
/// `String` (formatted) uniformly via `Cow`-free owned `String` so a single
/// `Entry` type covers both. The owning `keys()` table holds them for 'static.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The exact set of modifiers that must be active for this entry to match.
    /// If mods_empty_is_any is true then empty mods means any set of mods can
    /// match. Otherwise, empty mods means no mods must be active.
    pub mods: Mods,
    pub mods_empty_is_any: bool,

    /// The state required for cursor/keypad mode.
    pub cursor: CursorMode,
    pub keypad: KeypadMode,

    /// Whether or not this entry should be used
    pub modify_other_keys: ModifyKeys,

    /// The sequence to send to the pty if this entry matches.
    pub sequence: String,

    /// Sequence to send to the PTY if DECBKM is set.
    pub sequence_decbkm: Option<String>,
}

impl Default for Entry {
    fn default() -> Self {
        Entry {
            mods: Mods::default(),
            mods_empty_is_any: true,
            cursor: CursorMode::Any,
            keypad: KeypadMode::Any,
            modify_other_keys: ModifyKeys::Any,
            sequence: String::new(),
            sequence_decbkm: None,
        }
    }
}

impl Entry {
    /// PORT NOTE: convenience constructor mirroring the reference struct literal
    /// `.{ .sequence = "..." }` where only `sequence` is set and everything else
    /// is defaulted.
    fn seq(sequence: &str) -> Entry {
        Entry {
            sequence: sequence.to_string(),
            ..Default::default()
        }
    }
}

/// PORT NOTE: `.{ .shift = true }` etc. — short-hand for a `Mods` with the named
/// fields set and everything else default. Used by the `modifiers` table and
/// the per-key entry literals below.
const fn m(shift: bool, alt: bool, ctrl: bool, super_: bool) -> Mods {
    Mods {
        shift,
        ctrl,
        alt,
        super_,
        caps_lock: false,
        num_lock: false,
        sides: Side {
            shift: ModSide::Left,
            ctrl: ModSide::Left,
            alt: ModSide::Left,
            super_: ModSide::Left,
        },
    }
}

/// The list of modifier combinations for modify other key sequences.
/// The mode value is index + 2.
///
/// PORT NOTE: the reference `modifiers` is a `const` slice (`[]const key.Mods`)
/// materialized at comptime and borrowed by every caller. `Mods` contains no
/// `const fn`-hostile field, so the table is a true `static` here and this
/// accessor hands out the same borrowed slice the reference does — no
/// per-lookup construction.
pub fn modifiers() -> &'static [Mods] {
    &MODIFIERS
}

static MODIFIERS: [Mods; 15] = [
    m(true, false, false, false), // .{ .shift = true },
    m(false, true, false, false), // .{ .alt = true },
    m(true, true, false, false),  // .{ .shift = true, .alt = true },
    m(false, false, true, false), // .{ .ctrl = true },
    m(true, false, true, false),  // .{ .shift = true, .ctrl = true },
    m(false, true, true, false),  // .{ .alt = true, .ctrl = true },
    m(true, true, true, false),   // .{ .shift = true, .alt = true, .ctrl = true },
    m(false, false, false, true), // .{ .super = true },
    m(true, false, false, true),  // .{ .shift = true, .super = true },
    m(false, true, false, true),  // .{ .alt = true, .super = true },
    m(true, true, false, true),   // .{ .shift = true, .alt = true, .super = true },
    m(false, false, true, true),  // .{ .ctrl = true, .super = true },
    m(true, false, true, true),   // .{ .shift = true, .ctrl = true, .super = true },
    m(false, true, true, true),   // .{ .alt = true, .ctrl = true, .super = true },
    m(true, true, true, true),    // .{ .shift = true, .alt = true, .ctrl = true, .super = true },
];

/// This is the array of entries for the PC style function keys as mapped to
/// our set of possible key codes. Not every key has any entries.
///
/// PORT NOTE: the reference type is `std.EnumArray(key.Key, []const Entry)`. We
/// model it as a list of `(Key, Vec<Entry>)` pairs (only the keys that have
/// entries are set; all others map to an empty slice in the reference). Lookups
/// are by `Key`. The `values` iterator in the reference test is reproduced by
/// iterating the entry lists.
pub type KeyEntryArray = Vec<(Key, Vec<Entry>)>;

// The list of keys that we support and their entry values. It is expected
// that you'll just use a for loop through the entry values, there are never
// more than a couple dozen or so.
//
// PORT NOTE: the reference `keys` is a comptime-evaluated `KeyEntryArray`. Rust
// has no comptime table-of-formatted-strings (the `pcStyle` entries hold
// formatted `String`s), so this is built once — lazily, on first use — into the
// `KEYS` static and borrowed thereafter. Callers get the same borrowed table the
// reference's comptime value hands out: no per-lookup (i.e. per-key-event)
// construction or allocation. The produced data is identical to the reference
// comptime value.
static KEYS: LazyLock<KeyEntryArray> = LazyLock::new(build_keys);

/// Returns the (built-once) key entry table. Analogue of the reference's
/// comptime `keys` constant.
pub fn keys() -> &'static KeyEntryArray {
    &KEYS
}

fn build_keys() -> KeyEntryArray {
    let mut result: KeyEntryArray = Vec::new();

    let mut set = |k: Key, entries: Vec<Entry>| result.push((k, entries));

    set(
        Key::ArrowUp,
        concat(pc_style("\x1b[1;{}A"), cursor_key("\x1b[A", "\x1bOA")),
    );
    set(
        Key::ArrowDown,
        concat(pc_style("\x1b[1;{}B"), cursor_key("\x1b[B", "\x1bOB")),
    );
    set(
        Key::ArrowRight,
        concat(pc_style("\x1b[1;{}C"), cursor_key("\x1b[C", "\x1bOC")),
    );
    set(
        Key::ArrowLeft,
        concat(pc_style("\x1b[1;{}D"), cursor_key("\x1b[D", "\x1bOD")),
    );
    set(
        Key::Home,
        concat(pc_style("\x1b[1;{}H"), cursor_key("\x1b[H", "\x1bOH")),
    );
    set(
        Key::End,
        concat(pc_style("\x1b[1;{}F"), cursor_key("\x1b[F", "\x1bOF")),
    );
    set(
        Key::Insert,
        concat(pc_style("\x1b[2;{}~"), vec![Entry::seq("\x1B[2~")]),
    );
    set(
        Key::Delete,
        concat(pc_style("\x1b[3;{}~"), vec![Entry::seq("\x1B[3~")]),
    );
    set(
        Key::PageUp,
        concat(pc_style("\x1b[5;{}~"), vec![Entry::seq("\x1B[5~")]),
    );
    set(
        Key::PageDown,
        concat(pc_style("\x1b[6;{}~"), vec![Entry::seq("\x1B[6~")]),
    );
    set(
        Key::Help,
        concat(pc_style("\x1b[28;{}~"), vec![Entry::seq("\x1B[28~")]),
    );
    set(
        Key::ContextMenu,
        concat(pc_style("\x1b[29;{}~"), vec![Entry::seq("\x1B[29~")]),
    );

    // Function Keys. todo: f26-f35 but we need to add to input.Key
    set(
        Key::F1,
        concat(pc_style("\x1b[1;{}P"), vec![Entry::seq("\x1BOP")]),
    );
    set(
        Key::F2,
        concat(pc_style("\x1b[1;{}Q"), vec![Entry::seq("\x1BOQ")]),
    );
    set(
        Key::F3,
        concat(pc_style("\x1b[13;{}~"), vec![Entry::seq("\x1BOR")]),
    );
    set(
        Key::F4,
        concat(pc_style("\x1b[1;{}S"), vec![Entry::seq("\x1BOS")]),
    );
    set(
        Key::F5,
        concat(pc_style("\x1b[15;{}~"), vec![Entry::seq("\x1B[15~")]),
    );
    set(
        Key::F6,
        concat(pc_style("\x1b[17;{}~"), vec![Entry::seq("\x1B[17~")]),
    );
    set(
        Key::F7,
        concat(pc_style("\x1b[18;{}~"), vec![Entry::seq("\x1B[18~")]),
    );
    set(
        Key::F8,
        concat(pc_style("\x1b[19;{}~"), vec![Entry::seq("\x1B[19~")]),
    );
    set(
        Key::F9,
        concat(pc_style("\x1b[20;{}~"), vec![Entry::seq("\x1B[20~")]),
    );
    set(
        Key::F10,
        concat(pc_style("\x1b[21;{}~"), vec![Entry::seq("\x1B[21~")]),
    );
    set(
        Key::F11,
        concat(pc_style("\x1b[23;{}~"), vec![Entry::seq("\x1B[23~")]),
    );
    set(
        Key::F12,
        concat(pc_style("\x1b[24;{}~"), vec![Entry::seq("\x1B[24~")]),
    );
    set(
        Key::F13,
        concat(pc_style("\x1b[25;{}~"), vec![Entry::seq("\x1B[25~")]),
    );
    set(
        Key::F14,
        concat(pc_style("\x1b[26;{}~"), vec![Entry::seq("\x1B[26~")]),
    );
    set(
        Key::F15,
        concat(pc_style("\x1b[28;{}~"), vec![Entry::seq("\x1B[28~")]),
    );
    set(
        Key::F16,
        concat(pc_style("\x1b[29;{}~"), vec![Entry::seq("\x1B[29~")]),
    );
    set(
        Key::F17,
        concat(pc_style("\x1b[31;{}~"), vec![Entry::seq("\x1B[31~")]),
    );
    set(
        Key::F18,
        concat(pc_style("\x1b[32;{}~"), vec![Entry::seq("\x1B[32~")]),
    );
    set(
        Key::F19,
        concat(pc_style("\x1b[33;{}~"), vec![Entry::seq("\x1B[33~")]),
    );
    set(
        Key::F20,
        concat(pc_style("\x1b[34;{}~"), vec![Entry::seq("\x1B[34~")]),
    );
    set(
        Key::F21,
        concat(pc_style("\x1b[42;{}~"), vec![Entry::seq("\x1B[42~")]),
    );
    set(
        Key::F22,
        concat(pc_style("\x1b[43;{}~"), vec![Entry::seq("\x1B[43~")]),
    );
    set(
        Key::F23,
        concat(pc_style("\x1b[44;{}~"), vec![Entry::seq("\x1B[44~")]),
    );
    set(
        Key::F24,
        concat(pc_style("\x1b[45;{}~"), vec![Entry::seq("\x1B[45~")]),
    );
    set(
        Key::F25,
        concat(pc_style("\x1b[46;{}~"), vec![Entry::seq("\x1B[46~")]),
    );

    // Keypad keys
    set(Key::Numpad0, kp_keys("p", "0"));
    set(Key::Numpad1, kp_keys("q", "1"));
    set(Key::Numpad2, kp_keys("r", "2"));
    set(Key::Numpad3, kp_keys("s", "3"));
    set(Key::Numpad4, kp_keys("t", "4"));
    set(Key::Numpad5, kp_keys("u", "5"));
    set(Key::Numpad6, kp_keys("v", "6"));
    set(Key::Numpad7, kp_keys("w", "7"));
    set(Key::Numpad8, kp_keys("x", "8"));
    set(Key::Numpad9, kp_keys("y", "9"));
    set(Key::NumpadDecimal, kp_keys("n", "."));
    set(Key::NumpadDivide, kp_keys("o", "/"));
    set(Key::NumpadMultiply, kp_keys("j", "*"));
    set(Key::NumpadSubtract, kp_keys("m", "-"));
    set(Key::NumpadAdd, kp_keys("k", "+"));
    set(Key::NumpadEnter, kp_keys("M", "\r"));
    set(
        Key::NumpadUp,
        concat(pc_style("\x1b[1;{}A"), cursor_key("\x1b[A", "\x1bOA")),
    );
    set(
        Key::NumpadDown,
        concat(pc_style("\x1b[1;{}B"), cursor_key("\x1b[B", "\x1bOB")),
    );
    set(
        Key::NumpadRight,
        concat(pc_style("\x1b[1;{}C"), cursor_key("\x1b[C", "\x1bOC")),
    );
    set(
        Key::NumpadLeft,
        concat(pc_style("\x1b[1;{}D"), cursor_key("\x1b[D", "\x1bOD")),
    );
    set(
        Key::NumpadBegin,
        concat(pc_style("\x1b[1;{}E"), cursor_key("\x1b[E", "\x1bOE")),
    );
    set(
        Key::NumpadHome,
        concat(pc_style("\x1b[1;{}H"), cursor_key("\x1b[H", "\x1bOH")),
    );
    set(
        Key::NumpadEnd,
        concat(pc_style("\x1b[1;{}F"), cursor_key("\x1b[F", "\x1bOF")),
    );
    set(
        Key::NumpadInsert,
        concat(pc_style("\x1b[2;{}~"), vec![Entry::seq("\x1B[2~")]),
    );
    set(
        Key::NumpadDelete,
        concat(pc_style("\x1b[3;{}~"), vec![Entry::seq("\x1B[3~")]),
    );
    set(
        Key::NumpadPageUp,
        concat(pc_style("\x1b[5;{}~"), vec![Entry::seq("\x1B[5~")]),
    );
    set(
        Key::NumpadPageDown,
        concat(pc_style("\x1b[6;{}~"), vec![Entry::seq("\x1B[6~")]),
    );

    set(
        Key::Backspace,
        vec![
            // Modify Keys Normal
            Entry {
                mods: m(true, false, false, false),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x7f".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, false),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\x7f".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, false, false),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\x7f".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, false),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x08".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, false),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\x08".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, false, true),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x7f".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, false, true),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x7f".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, true),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\x7f".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, false, true),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\x7f".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, true, true),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x08".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, true),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x08".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, true),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\x08".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, true, true),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\x08".to_string(),
                ..Default::default()
            },
            // Modify Keys Other
            Entry {
                mods: m(true, false, false, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;2;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;3;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, false, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;4;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;6;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;7;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, true, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;8;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, false, true),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;9;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, false, true),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;10;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, true),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;11;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, false, true),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;12;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, true, true),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;13;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, true),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;14;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, true),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;15;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, true, true),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;16;127~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, true, false),
                sequence: "\x08".to_string(),
                sequence_decbkm: Some("\x7f".to_string()),
                ..Default::default()
            },
            Entry {
                sequence: "\x7f".to_string(),
                sequence_decbkm: Some("\x08".to_string()),
                ..Default::default()
            },
        ],
    );

    set(
        Key::Tab,
        vec![
            // Modify Keys Normal
            Entry {
                mods: m(true, false, false, false),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b[Z".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, false),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\t".to_string(),
                ..Default::default()
            },
            // Modify Keys Other
            Entry {
                mods: m(true, false, false, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;2;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;3;9~".to_string(),
                ..Default::default()
            },
            // Everything else
            Entry {
                mods: m(true, true, false, false),
                sequence: "\x1b[27;4;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, true, false),
                sequence: "\x1b[27;5;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, false),
                sequence: "\x1b[27;6;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, false),
                sequence: "\x1b[27;7;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, true, false),
                sequence: "\x1b[27;8;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, false, true),
                sequence: "\x1b[27;9;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, false, true),
                sequence: "\x1b[27;10;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, true),
                sequence: "\x1b[27;11;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, false, true),
                sequence: "\x1b[27;12;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, true, true),
                sequence: "\x1b[27;13;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, true),
                sequence: "\x1b[27;14;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, true),
                sequence: "\x1b[27;15;9~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, true, true),
                sequence: "\x1b[27;16;9~".to_string(),
                ..Default::default()
            },
            Entry {
                sequence: "\t".to_string(),
                ..Default::default()
            },
        ],
    );

    set(
        Key::Enter,
        vec![
            Entry {
                mods: m(true, false, false, false),
                sequence: "\x1b[27;2;13~".to_string(),
                ..Default::default()
            },
            // Modify Keys Normal
            Entry {
                mods: m(false, true, false, false),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\r".to_string(),
                ..Default::default()
            },
            // Modify Keys Other
            Entry {
                mods: m(false, true, false, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;3;13~".to_string(),
                ..Default::default()
            },
            // Everything else
            Entry {
                mods: m(true, true, false, false),
                sequence: "\x1b[27;4;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, true, false),
                sequence: "\x1b[27;5;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, false),
                sequence: "\x1b[27;6;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, false),
                sequence: "\x1b[27;7;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, true, false),
                sequence: "\x1b[27;8;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, false, true),
                sequence: "\x1b[27;9;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, false, true),
                sequence: "\x1b[27;10;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, true),
                sequence: "\x1b[27;11;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, false, true),
                sequence: "\x1b[27;12;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, true, true),
                sequence: "\x1b[27;13;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, true),
                sequence: "\x1b[27;14;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, true),
                sequence: "\x1b[27;15;13~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, true, true),
                sequence: "\x1b[27;16;13~".to_string(),
                ..Default::default()
            },
            Entry {
                sequence: "\r".to_string(),
                ..Default::default()
            },
        ],
    );

    set(
        Key::Escape,
        vec![
            Entry {
                mods: m(true, false, false, false),
                sequence: "\x1b[27;2;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, false),
                modify_other_keys: ModifyKeys::Set,
                sequence: "\x1b\x1b".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, false),
                modify_other_keys: ModifyKeys::SetOther,
                sequence: "\x1b[27;3;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, false, false),
                sequence: "\x1b[27;4;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, true, false),
                sequence: "\x1b[27;5;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, false),
                sequence: "\x1b[27;6;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, false),
                sequence: "\x1b[27;7;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, true, false),
                sequence: "\x1b[27;8;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, false, true),
                sequence: "\x1b[27;9;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, false, true),
                sequence: "\x1b[27;10;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, false, true),
                sequence: "\x1b[27;11;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, false, true),
                sequence: "\x1b[27;12;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, false, true, true),
                sequence: "\x1b[27;13;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, false, true, true),
                sequence: "\x1b[27;14;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(false, true, true, true),
                sequence: "\x1b[27;15;27~".to_string(),
                ..Default::default()
            },
            Entry {
                mods: m(true, true, true, true),
                sequence: "\x1b[27;16;27~".to_string(),
                ..Default::default()
            },
            Entry {
                sequence: "\x1b".to_string(),
                ..Default::default()
            },
        ],
    );

    result
}

/// Looks up the entry slice for a given key, mirroring the reference's
/// `function_keys.keys.get(keyval)` on the `std.EnumArray`.
///
/// PORT NOTE: the reference `keys` is an `EnumArray` whose `.get(keyval)` is an
/// O(1) lookup returning the entry slice (an empty slice for keys with no
/// entries). This helper reproduces that by scanning the built-once table and
/// *borrowing* the entry slice (the reference borrows its comptime slice too —
/// no allocation on the lookup path); keys not present in the table map to an
/// empty slice, exactly as the reference `EnumArray` does.
pub fn keys_get(keyval: Key) -> &'static [Entry] {
    for (k, entries) in keys().iter() {
        if *k == keyval {
            return entries;
        }
    }
    &[]
}

/// PORT NOTE: the reference uses Zig's `++` array concatenation on `[]const
/// Entry`. This helper reproduces it by appending `b` onto `a`.
fn concat(mut a: Vec<Entry>, b: Vec<Entry>) -> Vec<Entry> {
    a.extend(b);
    a
}

/// Returns the default keypad application mode entry.
fn kp_default(suffix: &str) -> Vec<Entry> {
    vec![Entry {
        mods_empty_is_any: false,
        keypad: KeypadMode::Application,
        sequence: format!("\x1bO{}", suffix),
        ..Default::default()
    }]
}

/// Returns the entries for a keypad key. The suffix is the final character
/// of the sent sequence, such as "r" for kp_2.
fn kp_keys(suffix: &str, normal: &str) -> Vec<Entry> {
    let mut pc = pc_style(&format!("\x1bO{{}}{}", suffix));
    for entry in pc.iter_mut() {
        entry.keypad = KeypadMode::Application;
    }
    concat(
        concat(kp_default(suffix), pc),
        vec![Entry {
            keypad: KeypadMode::Normal,
            sequence: normal.to_string(),
            ..Default::default()
        }],
    )
}

/// Returns entries that are dependent on cursor key settings.
fn cursor_key(normal: &str, application: &str) -> Vec<Entry> {
    vec![
        Entry {
            cursor: CursorMode::Normal,
            sequence: normal.to_string(),
            ..Default::default()
        },
        Entry {
            cursor: CursorMode::Application,
            sequence: application.to_string(),
            ..Default::default()
        },
    ]
}

/// Constructs a set of pcStyle function keys using the given format. The
/// format should have exactly one "hole" for the mods code.
/// Example: "\x1b[11;{}~" for F1.
///
/// PORT NOTE: the reference `std.fmt.comptimePrint(fmt, .{code})` substitutes the
/// single `{}` hole with the decimal `code`. We reproduce that with `fmt_hole`
/// (a faithful single-`{}` substitution) over the runtime `fmt`.
fn pc_style(fmt: &str) -> Vec<Entry> {
    // The comptime {} wrapper is superfluous but it prevents us from
    // accidentally running this function at runtime.
    // PORT NOTE: the reference wraps the body in a `comptime { ... }` block (and
    // raises `@setEvalBranchQuota(500_000)`) to force comptime evaluation. The
    // Rust port builds the `keys()` table at runtime, so there is no comptime
    // wrapper / eval-branch-quota to mirror.
    let mods = modifiers();
    // for (modifiers, 2.., 0..) |mods, code, i|
    let mut entries: Vec<Entry> = Vec::with_capacity(mods.len());
    for (i, mods_v) in mods.iter().copied().enumerate() {
        let code = i + 2;
        entries.push(Entry {
            mods: mods_v,
            sequence: fmt_hole(fmt, code),
            ..Default::default()
        });
    }

    entries
}

/// PORT NOTE: faithful equivalent of `std.fmt.comptimePrint(fmt, .{code})` for a
/// format string with exactly one `{}` hole filled by the decimal `code`.
fn fmt_hole(fmt: &str, code: usize) -> String {
    let idx = fmt
        .find("{}")
        .expect("pcStyle format must have one {} hole");
    let mut out = String::with_capacity(fmt.len() + 2);
    out.push_str(&fmt[..idx]);
    out.push_str(&code.to_string());
    out.push_str(&fmt[idx + 2..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_test() {
        // PORT NOTE: the reference switches on `terminal_options.artifact`:
        // `.ghostty => {}` (run) and `.lib => return error.SkipZigTest` (don't
        // want to bring in termio into libghostty-vt). The Rust port's
        // `options::ARTIFACT` is `Ghostty`, so this test runs.
        match crate::options::ARTIFACT {
            crate::build_options::Artifact::Ghostty => {}
            // Don't want to bring in termio into libghostty-vt
            crate::build_options::Artifact::Lib => return,
        }

        // Force resolution for comptime evaluation.
        let keys = keys();

        // All key sequences must fit into a termio array.
        // PORT NOTE: the reference reads `termio.Message.WriteReq.Small.Max`,
        // which is `MessageData(u8, 38).Small.Max == 38`. termio is an apprt/
        // termio-layer dependency NOT part of the VT crate (the reference test
        // explicitly avoids importing it into libghostty-vt), so the constant
        // is inlined here as its reference value 38.
        let max: usize = 38;
        for (_key, entries) in keys.iter() {
            for entry in entries {
                assert!(entry.sequence.len() <= max);
            }
        }
    }

    /// PORT NOTE: the reference `keys` is a comptime constant — every
    /// `function_keys.keys.get(keyval)` borrows the SAME static table and
    /// allocates nothing. This proves the Rust port has the same property: the
    /// table is built exactly once and both `keys()` and `keys_get()` hand out
    /// borrows into that one instance (before the fix each call rebuilt the
    /// whole table, including freshly formatted `String` sequences, so the
    /// pointers differed on every call).
    #[test]
    fn keys_table_built_once() {
        // The table itself is one instance.
        assert!(std::ptr::eq(keys(), keys()));

        // And per-key lookups borrow into it rather than rebuilding.
        let a = keys_get(Key::ArrowUp);
        let b = keys_get(Key::ArrowUp);
        assert!(!a.is_empty());
        assert!(std::ptr::eq(a.as_ptr(), b.as_ptr()));
        assert_eq!(a.len(), b.len());

        // A key with no entries yields the empty slice, matching the
        // reference `EnumArray`'s unset fill value.
        assert!(keys_get(Key::Unidentified).is_empty());

        // `modifiers()` is likewise a single static table.
        assert!(std::ptr::eq(modifiers().as_ptr(), modifiers().as_ptr()));
        assert_eq!(modifiers().len(), 15);
    }
}
