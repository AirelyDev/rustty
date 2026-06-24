//! PORTS: ghostty src/terminal/osc/parsers/change_window_icon.zig
//!
//! PORT STATUS: stub — `parse` is a signature-faithful stub; body and tests
//! land in the fill phase.

use crate::osc::{Command, Parser};

/// Parse OSC 1
#[allow(clippy::todo, unused_variables)]
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    todo!()
}
