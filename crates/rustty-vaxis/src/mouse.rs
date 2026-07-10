//! Port of `libvaxis` `Mouse.zig` (50 LOC) — the `Mouse` value struct + its nested
//! `Shape`/`Button`/`Modifiers`/`Type`.
//!
//! PORT-NOTE: Zig field `type: Type` → Rust `r#type` (verbatim fidelity — `type` is a
//! Rust keyword, kept as a raw identifier). Zig kebab enum variants `@"ew-resize"`/
//! `@"ns-resize"` → `EwResize`/`NsResize`. `Mouse.Modifiers` is a `packed struct(u3)`
//! (shift/alt/ctrl), DISTINCT from `Key.Modifiers`'s u8.

/// A mouse event
///
/// PORT-NOTE: Zig `Mouse` has no default (`col`/`row`/`button`/`mods`/`type` have no
/// default values), so this struct intentionally does NOT derive `Default`.
///
/// PORT-NOTE (Chunk 3a): `PartialEq` added so the Parser tests can `assert_eq!` a whole
/// `Mouse` (Zig `testing.expectEqual`); all fields already compare by value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mouse {
    pub col: u16,
    pub row: u16,
    pub xoffset: u16,
    pub yoffset: u16,
    pub button: Button,
    pub mods: Modifiers,
    pub r#type: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Default,
    Text,
    Pointer,
    Help,
    Progress,
    Wait,
    EwResize,
    NsResize,
    Cell,
}

// pub const Button = enum(u8) { left, middle, right, none, wheel_up = 64, ... };
// Explicit discriminants — the gaps matter (SGR mouse decode maps raw bytes to these).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Button {
    Left = 0,
    Middle = 1,
    Right = 2,
    None = 3,
    WheelUp = 64,
    WheelDown = 65,
    WheelRight = 66,
    WheelLeft = 67,
    Button8 = 128,
    Button9 = 129,
    Button10 = 130,
    Button11 = 131,
}

impl Button {
    /// `@enumFromInt(v)` — the SGR/xterm mouse `button_mask & buttons` decode
    /// (Parser). PORT-NOTE (Chunk 3a): faithful to Zig's checked `@enumFromInt`
    /// illegal-behavior — a masked value with both wheel bits (6+7) set is an
    /// invalid button (192..195); Zig safe-mode panics there, so we `unreachable!()`.
    /// Provably not hit by the SGR corpus (button masks never set both wheel bits).
    pub fn from_u8(v: u8) -> Button {
        match v {
            0 => Button::Left,
            1 => Button::Middle,
            2 => Button::Right,
            3 => Button::None,
            64 => Button::WheelUp,
            65 => Button::WheelDown,
            66 => Button::WheelRight,
            67 => Button::WheelLeft,
            128 => Button::Button8,
            129 => Button::Button9,
            130 => Button::Button10,
            131 => Button::Button11,
            _ => unreachable!(),
        }
    }
}

// pub const Modifiers = packed struct(u3) { shift, alt, ctrl : bool = false };
// LSB-first: shift0 alt1 ctrl2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Press,
    Release,
    Motion,
    Drag,
}
