//! OSC 3008: Hierarchical Context Signalling (UAPI spec)
//! Specification: https://uapi-group.org/specifications/specs/osc_context/
//!
//! OSC 3008 allows programs to signal context changes to the terminal emulator.
//! Each context has an identifier and metadata fields. Contexts are hierarchical
//! and form a stack.
//!
//! PORTS: ghostty src/terminal/osc/parsers/context_signal.zig
//!
//! PORT STATUS: stub — the `Command` payload (referenced by the OSC parent
//! `Command::ContextSignal`) plus `Action` are declared so the parent compiles;
//! `parse` is a signature-faithful stub. The body, the `ContextType`/
//! `ExitStatus`/`Field` machinery, and the module tests land in the fill phase.

// PORT NOTE: `const OSCCommand = @import("../../osc.zig").Command;` — the OSC
// parent `Command`, aliased to disambiguate from this module's own `Command`.
use crate::osc::{Command as OSCCommand, Parser};

/// A single OSC 3008 context signal command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub action: Action,
    /// The context identifier. Must be 1-64 characters in the 32..126 byte range.
    pub id: String,
    /// Raw unparsed metadata fields after the context ID.
    /// Fields are semicolon-separated key=value pairs.
    /// Parsed lazily via `readOption`.
    pub metadata: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// OSC 3008;start=<id> — initiates, updates, or returns to a context.
    Start,
    /// OSC 3008;end=<id> — terminates a context.
    End,
}

/// Parse OSC 3008
#[allow(clippy::todo, unused_variables)]
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut OSCCommand> {
    todo!()
}
