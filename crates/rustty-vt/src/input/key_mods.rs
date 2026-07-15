//! rustty-vt input/key_mods
//!
//! PORTS: ghostty src/input/key_mods.zig (VT-relevant slice)
//!
//! PORT STATUS: partial
//!
//! PORT NOTE: only the `Mods` packed struct and the VT-relevant methods used by
//! `key.zig` / `function_keys.zig` are ported here, PLUS `Mod`/`ALIAS`/`ModSide`
//! (consumed by `function_keys`/`key_encode` and `rustty-input`'s `binding`).
//! `RemapSet` and `Mods.translation` depend on the apprt-layer
//! `config.OptionAsAlt` and the config `formatter` (an apprt/config seam, not VT)
//! and are therefore ported in their config-dependent peer crate
//! `rustty-input` (`crates/rustty-input/src/key_mods.rs`, with the full
//! `RemapSet`/`translation` test suite) — see DEFERRALS.md (RESOLVED).

// PORT NOTE: the reference imports
//   const std = @import("std");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const Allocator = std.mem.Allocator;
//   const builtin = @import("builtin");
//   const OptionAsAlt = @import("config.zig").OptionAsAlt;
// Only `builtin` (target OS, for `ctrlOrSuper`) is needed by the ported slice.

/// Aliases for modifier names.
//
// PORT NOTE: the reference `alias: []const struct { []const u8, Mod }`. Used by
// the deferred `RemapSet` parsing AND by `Binding.zig` trigger parsing (modifier
// aliases like `cmd`/`command`/`opt`/`option`/`control`). Each entry pairs a
// name with the `Mod` it aliases.
pub const ALIAS: &[(&[u8], Mod)] = &[
    (b"cmd", Mod::Super),
    (b"command", Mod::Super),
    (b"opt", Mod::Alt),
    (b"option", Mod::Alt),
    (b"control", Mod::Ctrl),
];

/// Single modifier
// PORT NOTE: `Mod` and its `alias` table are used by the deferred `RemapSet`
// parsing AND by `Binding.zig`; the side enum is mirrored below as `ModSide`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mod {
    Shift,
    Ctrl,
    Alt,
    Super,
}

impl Mod {
    /// PORT NOTE: the reference uses `@tagName(pair[1])` to name the `Mods`
    /// field to set. Returns the matching `Mods` bool field name.
    pub fn tag_name(self) -> &'static str {
        match self {
            Mod::Shift => "shift",
            Mod::Ctrl => "ctrl",
            Mod::Alt => "alt",
            Mod::Super => "super",
        }
    }
}

/// PORT NOTE: `Mod.Side` — `enum(u1) { left, right }`. Mirrored below as
/// `ModSide`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ModSide {
    #[default]
    Left = 0,
    Right = 1,
}

/// A bitmask for all key modifiers.
///
/// IMPORTANT: Any changes here update include/rustty-embed.h (the embedding
/// C ABI; renamed from the reference's include/ghostty.h)
///
/// PORT NOTE: `packed struct(u16)` with backing `u16`. The field order maps to
/// bit positions: shift=0, ctrl=1, alt=2, super=3, caps_lock=4, num_lock=5, then
/// the `sides` (a packed `u4`: shift/ctrl/alt/super at bits 6..=9), then a `u6`
/// padding. `int()` reproduces that layout bit-for-bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
    pub sides: Side,
    // _padding: u6 = 0,
}

/// The standard modifier keys only. Does not include the lock keys,
/// only standard bindable keys.
///
/// PORT NOTE: `packed struct(u4)`. shift=bit0, ctrl=bit1, alt=bit2, super=bit3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Keys {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
}

/// PORT NOTE: `Keys.Backing` — the documented `u4` backing integer of `Keys`.
#[allow(non_camel_case_types)]
pub type KeysBacking = u4_repr;

impl Keys {
    pub fn int(self) -> u8 {
        (self.shift as u8)
            | ((self.ctrl as u8) << 1)
            | ((self.alt as u8) << 2)
            | ((self.super_ as u8) << 3)
    }

    /// PORT NOTE: `@bitCast` from the `u4` backing integer back to the packed
    /// `Keys` struct. Reproduces the bit layout from `int()` (shift=bit0,
    /// ctrl=bit1, alt=bit2, super=bit3). Used by `RemapSet.Mask` (rustty-input).
    pub fn from_int(v: u8) -> Keys {
        Keys {
            shift: (v & 0b0001) != 0,
            ctrl: (v & 0b0010) != 0,
            alt: (v & 0b0100) != 0,
            super_: (v & 0b1000) != 0,
        }
    }
}

/// PORT NOTE: alias for the documented `u4` backing integer of `Keys`/`Side`.
/// Rust has no native 3- or 4-bit integer; the value range `0..=15` is modeled
/// with `u8` and masked operations.
#[allow(non_camel_case_types)]
pub type u4_repr = u8;

/// Tracks the side that is active for any given modifier. Note
/// that this doesn't confirm a modifier is pressed; you must check
/// the bool for that in addition to this.
///
/// Not all platforms support this, check apprt for more info.
///
/// PORT NOTE: `packed struct(u4)` of four `Mod.Side` (each a `u1`). shift=bit0,
/// ctrl=bit1, alt=bit2, super=bit3, with `right = 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Side {
    pub shift: ModSide,
    pub ctrl: ModSide,
    pub alt: ModSide,
    pub super_: ModSide,
}

impl Side {
    pub fn int(self) -> u8 {
        (self.shift as u8)
            | ((self.ctrl as u8) << 1)
            | ((self.alt as u8) << 2)
            | ((self.super_ as u8) << 3)
    }

    /// PORT NOTE: `@bitCast` from the `u4` backing integer back to the packed
    /// `Side` struct. Reproduces the bit layout from `int()` (shift=bit0,
    /// ctrl=bit1, alt=bit2, super=bit3, with `right = 1`). Used by
    /// `RemapSet.Mask` (rustty-input).
    pub fn from_int(v: u8) -> Side {
        Side {
            shift: if (v & 0b0001) != 0 {
                ModSide::Right
            } else {
                ModSide::Left
            },
            ctrl: if (v & 0b0010) != 0 {
                ModSide::Right
            } else {
                ModSide::Left
            },
            alt: if (v & 0b0100) != 0 {
                ModSide::Right
            } else {
                ModSide::Left
            },
            super_: if (v & 0b1000) != 0 {
                ModSide::Right
            } else {
                ModSide::Left
            },
        }
    }
}

/// PORT NOTE: `Mods.Backing` — the documented `u16` backing integer of `Mods`.
pub type ModsBacking = u16;

impl Mods {
    /// The mask that has all the side bits set.
    pub const SIDE_MASK: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
        super_: false,
        caps_lock: false,
        num_lock: false,
        sides: Side {
            shift: ModSide::Right,
            ctrl: ModSide::Right,
            alt: ModSide::Right,
            super_: ModSide::Right,
        },
    };

    /// Integer value of this struct.
    pub fn int(self) -> u16 {
        (self.shift as u16)
            | ((self.ctrl as u16) << 1)
            | ((self.alt as u16) << 2)
            | ((self.super_ as u16) << 3)
            | ((self.caps_lock as u16) << 4)
            | ((self.num_lock as u16) << 5)
            | ((self.sides.int() as u16) << 6)
    }

    /// Returns true if no modifiers are set.
    pub fn empty(self) -> bool {
        self.int() == 0
    }

    /// Returns true if two mods are equal.
    pub fn equal(self, other: Mods) -> bool {
        self.int() == other.int()
    }

    /// Returns only the keys.
    ///
    /// In the future I want to remove `binding` for this. I didn't want
    /// to do that all in one PR where I added this because its a bigger
    /// change.
    pub fn keys(self) -> Keys {
        // PORT NOTE: `@truncate(self.int())` then `@bitCast` to `Keys`. The low
        // four bits of the `u16` int are exactly shift/ctrl/alt/super.
        let backing: u8 = (self.int() & 0b1111) as u8;
        Keys {
            shift: (backing & 0b0001) != 0,
            ctrl: (backing & 0b0010) != 0,
            alt: (backing & 0b0100) != 0,
            super_: (backing & 0b1000) != 0,
        }
    }

    /// Return mods that are only relevant for bindings.
    pub fn binding(self) -> Mods {
        Mods {
            shift: self.shift,
            ctrl: self.ctrl,
            alt: self.alt,
            super_: self.super_,
            ..Default::default()
        }
    }

    /// Perform `self &~ other` to remove the other mods from self.
    pub fn unset(self, other: Mods) -> Mods {
        Mods::from_int(self.int() & !other.int())
    }

    /// Returns the mods without locks set.
    pub fn without_locks(self) -> Mods {
        let mut copy = self;
        copy.caps_lock = false;
        copy.num_lock = false;
        copy
    }

    /// Checks to see if super is on (MacOS) or ctrl.
    pub fn ctrl_or_super(self) -> bool {
        if cfg!(target_vendor = "apple") {
            return self.super_;
        }
        self.ctrl
    }

    /// PORT NOTE: the bool field names of the `Mods` packed struct, in
    /// declaration order. The reference iterates
    /// `inline for (@typeInfo(key.Mods).@"struct".fields) |field|` and acts on
    /// each `field.type == bool` field by name; this exposes exactly those
    /// names so a downstream port (`Binding`) can reproduce that loop.
    pub const BOOL_FIELD_NAMES: &'static [&'static str] =
        &["shift", "ctrl", "alt", "super", "caps_lock", "num_lock"];

    /// PORT NOTE: `@field(self, name)` read for the bool fields — supports the
    /// reference's "repeat not allowed" check (`if (@field(result.mods, ...))`).
    /// Returns `None` if `name` is not a bool field.
    pub fn get_bool_field(self, name: &str) -> Option<bool> {
        match name {
            "shift" => Some(self.shift),
            "ctrl" => Some(self.ctrl),
            "alt" => Some(self.alt),
            "super" => Some(self.super_),
            "caps_lock" => Some(self.caps_lock),
            "num_lock" => Some(self.num_lock),
            _ => None,
        }
    }

    /// PORT NOTE: `@field(self, name) = value` write for the bool fields. Used
    /// to reproduce the reference's `@field(result.mods, field.name) = true`.
    pub fn set_bool_field(&mut self, name: &str, value: bool) {
        match name {
            "shift" => self.shift = value,
            "ctrl" => self.ctrl = value,
            "alt" => self.alt = value,
            "super" => self.super_ = value,
            "caps_lock" => self.caps_lock = value,
            "num_lock" => self.num_lock = value,
            _ => {}
        }
    }

    /// PORT NOTE: `@bitCast` from the `u16` backing integer back to the packed
    /// struct. Reproduces the bit layout from `int()`.
    pub fn from_int(v: u16) -> Mods {
        Mods {
            shift: (v & 0b1) != 0,
            ctrl: (v & 0b10) != 0,
            alt: (v & 0b100) != 0,
            super_: (v & 0b1000) != 0,
            caps_lock: (v & 0b1_0000) != 0,
            num_lock: (v & 0b10_0000) != 0,
            sides: Side {
                shift: if (v & 0b100_0000) != 0 {
                    ModSide::Right
                } else {
                    ModSide::Left
                },
                ctrl: if (v & 0b1000_0000) != 0 {
                    ModSide::Right
                } else {
                    ModSide::Left
                },
                alt: if (v & 0b1_0000_0000) != 0 {
                    ModSide::Right
                } else {
                    ModSide::Left
                },
                super_: if (v & 0b10_0000_0000) != 0 {
                    ModSide::Right
                } else {
                    ModSide::Left
                },
            },
        }
    }
}

// For our own understanding
#[cfg(test)]
#[test]
fn mods_int_layout() {
    assert_eq!(Mods::default().int(), 0b0u16);
    assert_eq!(
        Mods {
            shift: true,
            ..Default::default()
        }
        .int(),
        0b0000_0001u16
    );
}
