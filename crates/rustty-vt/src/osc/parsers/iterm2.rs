//! PORTS: ghostty src/terminal/osc/parsers/iterm2.zig
//!
//! PORT STATUS: stub — `parse` is a signature-faithful stub; body and tests
//! land in the fill phase.

use crate::osc::{Command, Parser};

/// Parse OSC 1337
/// https://iterm2.com/documentation-escape-codes.html
#[allow(clippy::todo, unused_variables)]
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    todo!()
}
