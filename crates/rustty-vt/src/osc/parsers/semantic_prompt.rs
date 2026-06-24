//! https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md
//!
//! PORTS: ghostty src/terminal/osc/parsers/semantic_prompt.zig
//!
//! PORT STATUS: stub — the `Command` payload (referenced by the OSC parent
//! `Command::SemanticPrompt`) plus `Action` are declared so the parent
//! compiles; `parse` is a signature-faithful stub. The body, `init`,
//! `readOption`, `writeCommandLine`, the `Option`/`Click`/`PromptKind`/
//! `Redraw` machinery, and the module tests land in the fill phase.

// PORT NOTE: `const OSCCommand = @import("../../osc.zig").Command;` — the OSC
// parent `Command`, aliased to disambiguate from this module's own `Command`.
use crate::osc::{Command as OSCCommand, Parser};

/// A single semantic prompt command.
///
/// Technically according to the spec, not all commands have options
/// but it is easier to be "liberal in what we accept" here since
/// all except one do and the spec does also say to ignore unknown
/// options. So, I think this is a fair interpretation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub action: Action,
    pub options_unvalidated: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    FreshLine,                       // 'L'
    FreshLineNewPrompt,              // 'A'
    NewCommand,                      // 'N'
    PromptStart,                     // 'P'
    EndPromptStartInput,             // 'B'
    EndPromptStartInputTerminateEol, // 'I'
    EndInputStartOutput,             // 'C'
    EndCommand,                      // 'D'
}

impl Command {
    // PORT NOTE: `pub fn init(action: Action) Command` — the only `Command`
    // method needed by callers in the current fill phase (used by osc9.zig's
    // OSC 9;12 mapping). The rest of the `Command` machinery (`readOption`,
    // `writeCommandLine`, `Option`/`Click`/`PromptKind`/`Redraw`) lands when the
    // semantic_prompt parser body itself is filled.
    pub fn init(action: Action) -> Command {
        Command {
            action,
            options_unvalidated: String::new(),
        }
    }
}

/// Parse OSC 133
#[allow(clippy::todo, unused_variables)]
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut OSCCommand> {
    todo!()
}
