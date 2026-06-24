//! Kitty's clipboard protocol (OSC 5522)
//! Specification: https://sw.kovidgoyal.net/kitty/clipboard/
//! https://rockorager.dev/misc/bracketed-paste-mime/
//!
//! PORTS: ghostty src/terminal/osc/parsers/kitty_clipboard_protocol.zig
//!
//! PORT STATUS: stub — the `OSC` payload (referenced by the OSC parent
//! `Command::KittyClipboardProtocol`) is declared so the parent compiles;
//! `parse` is a signature-faithful stub. The body, `readOption`, the
//! `Location`/`Operation`/`Status`/`Option` machinery, and the module tests
//! land in the fill phase.

use crate::osc::{Command, Parser, Terminator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OSC {
    /// The raw metadata that was received. It can be parsed by using the `readOption` method.
    pub metadata: String,
    /// The raw payload. It may be Base64 encoded, check the `e` option.
    pub payload: Option<String>,
    /// The terminator that was used in case we need to send a response.
    pub terminator: Terminator,
}

/// Parse OSC 5522
#[allow(clippy::todo, unused_variables)]
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    todo!()
}
