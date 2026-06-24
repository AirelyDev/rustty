//! PORTS: ghostty src/terminal/osc/parsers/kitty_color.zig
//!
//! PORT STATUS: stub — `parse` is a signature-faithful stub; body and tests
//! land in the fill phase. The OSC 21 payload type is `crate::kitty::color::OSC`
//! (already ported), referenced by the parent `Command::KittyColorProtocol`.

use crate::osc::{Command, Parser};

/// Parse OSC 21, the Kitty Color Protocol.
#[allow(clippy::todo, unused_variables)]
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    todo!()
}
