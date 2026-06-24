//! Kitty's text sizing protocol (OSC 66)
//! Specification: https://sw.kovidgoyal.net/kitty/text-sizing-protocol/
//!
//! PORTS: ghostty src/terminal/osc/parsers/kitty_text_sizing.zig
//!
//! PORT STATUS: stub — the `OSC` payload (referenced by the OSC parent
//! `Command::KittyTextSizing`) plus `VAlign`/`HAlign` are declared so the
//! parent compiles; `parse` is a signature-faithful stub. The body, `update`,
//! `cval`, and the module tests land in the fill phase.

use crate::osc::{Command, Parser};

pub const MAX_PAYLOAD_LENGTH: usize = 4096;

// PORT NOTE: `pub const VAlign = lib.Enum(lib.target, &.{ "top", "bottom",
// "center" })` — a plain enum with the keys in order for the `.zig` target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Bottom,
    Center,
}

// PORT NOTE: `pub const HAlign = lib.Enum(lib.target, &.{ "left", "right",
// "center" })` — a plain enum with the keys in order for the `.zig` target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OSC {
    pub scale: u8,       // 1 - 7 (u3)
    pub width: u8,       // 0 - 7 (0 means default) (u3)
    pub numerator: u8,   // u4
    pub denominator: u8, // u4
    pub valign: VAlign,
    pub halign: HAlign,
    pub text: String,
}

/// Parse OSC 66
#[allow(clippy::todo, unused_variables)]
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    todo!()
}
