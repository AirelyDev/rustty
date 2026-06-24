//! https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md
//!
//! PORTS: ghostty src/terminal/osc/parsers/semantic_prompt.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

// PORT NOTE: `const Parser = @import("../../osc.zig").Parser;` and
// `const OSCCommand = @import("../../osc.zig").Command;` — the OSC parent
// `Command`, aliased to disambiguate from this module's own `Command`.
use crate::osc::{Command as OSCCommand, Parser, State};
// PORT NOTE: `const string_encoding = @import("../../../os/string_encoding.zig");`
// — retargeted to the ported `rustty-os` crate (R2).
use rustty_os::string_encoding;

// PORT NOTE: `const log = std.log.scoped(.osc_semantic_prompt);` — rustty has no
// logging facade wired into this crate, so the scoped logger is carried as a
// comment (no call sites reference it in this module).

/// A single semantic prompt command.
///
/// Technically according to the spec, not all commands have options
/// but it is easier to be "liberal in what we accept" here since
/// all except one do and the spec does also say to ignore unknown
/// options. So, I think this is a fair interpretation.
// PORT NOTE: `pub const C = void;` — the C ABI tag for this command has no
// payload; the Rust port carries it as a comment as there is no C surface here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub action: Action,
    // PORT NOTE (fidelity): `options_unvalidated: []const u8` is a borrowed slice
    // into the parser's capture buffer in Zig. The ported `Command` owns the
    // bytes as a `Vec<u8>` (RAW bytes, not a lossy `String`); option reading and
    // command-line decoding operate over the byte slice directly.
    pub options_unvalidated: Vec<u8>,
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
    pub fn init(action: Action) -> Command {
        Command {
            action,
            options_unvalidated: Vec::new(),
        }
    }

    /// Read an option for this command. Returns null if unset or invalid.
    // PORT NOTE: `readOption(self, comptime option) ?option.Type()` is generic
    // over the option's value type. Rust dispatches via the `ReadableOption`
    // trait (implemented for both `Option_<T>` and `ByteOption`) so both the
    // `Copy`-valued options and the borrow-returning `[]const u8` options share
    // one `read_option` entry point.
    pub fn read_option<'a, O: ReadableOption<'a>>(&'a self, option: O) -> Option<O::Output> {
        option.read(&self.options_unvalidated)
    }

    /// Write the decoded command line (if any) to the writer. If an error
    /// occurs garbage may have been written to the writer.
    pub fn write_command_line<W: string_encoding::Writer>(
        &self,
        writer: &mut W,
    ) -> Result<(), string_encoding::WriteOrDecodeError<W::Error>> {
        if let Some(command_line) = self.read_option(option::CMDLINE) {
            string_encoding::printfQDecode(writer, command_line)?;
            return Ok(());
        }
        if let Some(command_line) = self.read_option(option::CMDLINE_URL) {
            string_encoding::urlPercentDecode(writer, command_line)?;
            return Ok(());
        }
        Ok(())
    }
}

// ClickEvents can either be a click_events=1 or click_events=2.
// The click_events=1 sends a click event with the absolute coordinates
// of the click.
// The click_events=2 sends a click event with the coordinates of the click
// relative to the prompt area.
// See https://github.com/ghostty-org/ghostty/issues/10865 and
// https://github.com/kovidgoyal/kitty/issues/9500
// for further details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickEvents {
    Absolute,
    Relative,
}

// PORT NOTE: `pub const Option = enum {...}` with a comptime-dependent
// `Type()` per tag, plus a `key()` and a `read()` returning `self.Type()`.
// Rust cannot express a value-dependent return type on a plain enum, so each
// option is modeled as a typed descriptor whose Rust type encodes the option's
// value type (the comptime `Type()`):
//
//   * the `[]const u8`-typed options (`aid`, `err`, `cmdline`, `cmdline_url`)
//     return a borrow into the caller's `raw`, so they are `ByteOption`
//     descriptors whose `read<'a>(&self, raw: &'a [u8]) -> Option<&'a [u8]>`
//     ties the result lifetime to the input;
//   * every other option's value type is `Copy` and lifetime-free, so they are
//     `Option_<T>` descriptors carrying a `decode: fn(&[u8]) -> Option<T>`
//     function pointer (the per-tag `switch (self)` arm).
//
// `exit_code` is the special case parsed from the first value rather than a
// `key=value` pair (`is_exit_code`). Both descriptor kinds share the
// `scan_value` helper which implements the `read` scanning loop and returns the
// matched raw value sub-slice. The concrete options live in the `option`
// module below (e.g. `option::AID`, `option::CL`); the enum's per-variant doc
// comments are carried on those `option::` constants.

/// Descriptor for an option whose value type `T` is a `Copy`, lifetime-free
/// value (everything except the `[]const u8`-typed options).
#[derive(Clone, Copy)]
pub struct Option_<T> {
    /// The option key, e.g. `b"cl"`. Empty for `exit_code` which has no key
    /// (the special case handled before ever calling key).
    key: &'static [u8],
    /// `true` only for `exit_code`, which is parsed from the first value rather
    /// than a `key=value` pair.
    is_exit_code: bool,
    /// Maps the matched raw value bytes to the option's parsed value.
    decode: fn(&[u8]) -> Option<T>,
}

/// Descriptor for a `[]const u8`-typed option (`aid`, `err`, `cmdline`,
/// `cmdline_url`) whose `read` returns a borrow into the caller's `raw`.
#[derive(Clone, Copy)]
pub struct ByteOption {
    /// The option key, e.g. `b"aid"`.
    key: &'static [u8],
}

/// PORT NOTE: the matched-value scanning loop shared by both descriptor kinds.
/// Mirrors `Option.read`'s `while` loop, returning the matched raw value
/// sub-slice (the `value:` block's result) or `None` ("Not found" / fallthrough
/// `break`). The `exit_code` special case (`is_exit_code`) returns the first
/// `full` value rather than a `key=value` match.
fn scan_value<'a>(raw: &'a [u8], key: &[u8], is_exit_code: bool) -> Option<&'a [u8]> {
    let mut remaining = raw;
    'scan: while !remaining.is_empty() {
        // Length of the next value is up to the `;` or the
        // end of the string.
        let len = remaining
            .iter()
            .position(|&b| b == b';')
            .unwrap_or(remaining.len());

        // Grab our full value and move our cursor past the `;`
        let full = &remaining[0..len];

        // If we're looking for exit_code we special case it.
        // as the first value.
        if is_exit_code {
            return Some(full);
        }

        // Parse our key=value and verify our key matches our
        // expectation.
        let value = 'value: {
            if let Some(eql_idx) = full.iter().position(|&b| b == b'=') {
                if &full[0..eql_idx] == key {
                    break 'value &full[eql_idx + 1..];
                }
            }

            // No match!
            if len < remaining.len() {
                remaining = &remaining[len + 1..];
                continue 'scan;
            }

            // PORT NOTE: bare `break` in Zig exits the `while` loop, falling
            // through to the "Not found" return below.
            break 'scan;
        };

        return Some(value);
    }

    // Not found
    None
}

impl<T> Option_<T> {
    /// Read the option value from the raw options string.
    ///
    /// The raw options string is the raw unparsed data after the
    /// OSC 133 command. e.g. for `133;A;aid=14;cl=line`, the
    /// raw options string would be `aid=14;cl=line`.
    ///
    /// Any errors in the raw string will return null since the OSC133
    /// specification says to ignore unknown or malformed options.
    pub fn read(&self, raw: &[u8]) -> Option<T> {
        let value = scan_value(raw, self.key, self.is_exit_code)?;
        (self.decode)(value)
    }
}

impl ByteOption {
    /// Read the option value from the raw options string. See `Option_::read`.
    pub fn read<'a>(&self, raw: &'a [u8]) -> Option<&'a [u8]> {
        // The `[]const u8` options' `switch (self)` arm is the identity
        // `=> value`, so the matched sub-slice is the result directly.
        scan_value(raw, self.key, false)
    }
}

/// PORT NOTE: shared entry point for `Command::read_option`, abstracting over
/// the two descriptor kinds and their `Output` value type (the comptime
/// `option.Type()`).
pub trait ReadableOption<'a> {
    type Output;
    fn read(self, raw: &'a [u8]) -> Option<Self::Output>;
}

impl<'a, T: 'a> ReadableOption<'a> for Option_<T> {
    type Output = T;
    fn read(self, raw: &'a [u8]) -> Option<T> {
        Option_::read(&self, raw)
    }
}

impl<'a> ReadableOption<'a> for ByteOption {
    type Output = &'a [u8];
    fn read(self, raw: &'a [u8]) -> Option<&'a [u8]> {
        ByteOption::read(&self, raw)
    }
}

/// PORT NOTE: the concrete `Option` values. Each constant pairs the option's
/// key + decoder with its value type, mirroring the Zig enum's tag + comptime
/// `Type()` + `key()` + `read()` arm.
pub mod option {
    use super::{ByteOption, Click, ClickEvents, Option_, PromptKind, Redraw};

    pub const AID: ByteOption = ByteOption { key: b"aid" };
    pub const CL: Option_<Click> = Option_ {
        key: b"cl",
        is_exit_code: false,
        decode: Click::init,
    };
    pub const PROMPT_KIND: Option_<PromptKind> = Option_ {
        key: b"k",
        is_exit_code: false,
        decode: |value| {
            if value.len() == 1 {
                PromptKind::init(value[0])
            } else {
                None
            }
        },
    };
    pub const ERR: ByteOption = ByteOption { key: b"err" };
    pub const CMDLINE: ByteOption = ByteOption { key: b"cmdline" };
    pub const CMDLINE_URL: ByteOption = ByteOption {
        key: b"cmdline_url",
    };

    // https://sw.kovidgoyal.net/kitty/shell-integration/#notes-for-shell-developers
    // Kitty supports a "redraw" option for prompt_start. This is extended
    // by Ghostty with the "last" option. See Redraw the type for more details.
    pub const REDRAW: Option_<Redraw> = Option_ {
        key: b"redraw",
        is_exit_code: false,
        decode: |value| {
            if value == b"0" {
                Some(Redraw::False)
            } else if value == b"1" {
                Some(Redraw::True)
            } else if value == b"last" {
                Some(Redraw::Last)
            } else {
                None
            }
        },
    };

    // Use a special key instead of arrow keys to move the cursor on
    // mouse click. Useful if arrow keys have side-effets like triggering
    // auto-complete. The shell integration script should bind the special
    // key as needed.
    // See: https://sw.kovidgoyal.net/kitty/shell-integration/#notes-for-shell-developers
    pub const SPECIAL_KEY: Option_<bool> = Option_ {
        key: b"special_key",
        is_exit_code: false,
        decode: |value| {
            if value.len() == 1 {
                match value[0] {
                    b'0' => Some(false),
                    b'1' => Some(true),
                    _ => None,
                }
            } else {
                None
            }
        },
    };

    // If true, the shell is capable of handling mouse click events.
    // Ghostty will then send a click event to the shell when the user
    // clicks somewhere in the prompt. The shell can then move the cursor
    // to that position or perform some other appropriate action. If false,
    // Ghostty may generate a number of fake key events to move the cursor
    // which is not very robust.
    // See: https://sw.kovidgoyal.net/kitty/shell-integration/#notes-for-shell-developers
    pub const CLICK_EVENTS: Option_<ClickEvents> = Option_ {
        key: b"click_events",
        is_exit_code: false,
        decode: |value| {
            if value.len() == 1 {
                match value[0] {
                    b'1' => Some(ClickEvents::Absolute),
                    b'2' => Some(ClickEvents::Relative),
                    _ => None,
                }
            } else {
                None
            }
        },
    };

    // Not technically an option that can be set with k=v and only
    // present currently with command 'D' but its easier to just
    // parse it into our options.
    pub const EXIT_CODE: Option_<i32> = Option_ {
        // special case, handled before ever calling key
        key: b"",
        is_exit_code: true,
        decode: |full| {
            // std.fmt.parseInt(i32, full, 10) catch null
            core::str::from_utf8(full).ok()?.parse::<i32>().ok()
        },
    };
}

/// The `cl` option specifies what kind of cursor key sequences are handled
/// by the application for click-to-move-cursor functionality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// Value: "line". Allows motion within a single input line using standard
    /// left/right arrow escape sequences. Only a single left/right sequence
    /// should be emitted for double-width characters.
    Line,

    /// Value: "m". Allows movement between different lines in the same group,
    /// but only using left/right arrow escape sequences.
    Multiple,

    /// Value: "v". Like `multiple` but cursor up/down should be used. The
    /// terminal should be conservative when moving between lines: move the
    /// cursor left to the start of line, emit the needed up/down sequences,
    /// then move the cursor right to the clicked destination.
    ConservativeVertical,

    /// Value: "w". Like `conservative_vertical` but specifies that there are
    /// no spurious spaces at the end of the line, and the application editor
    /// handles "smart vertical movement" (moving 2 lines up from position 20,
    /// where the intermediate line is 15 chars wide and the destination is
    /// 18 chars wide, ends at position 18).
    SmartVertical,
}

impl Click {
    pub fn init(value: &[u8]) -> Option<Click> {
        if value.len() == 1 {
            match value[0] {
                b'm' => Some(Click::Multiple),
                b'v' => Some(Click::ConservativeVertical),
                b'w' => Some(Click::SmartVertical),
                _ => None,
            }
        } else if value == b"line" {
            Some(Click::Line)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Initial,
    Right,
    Continuation,
    Secondary,
}

impl PromptKind {
    pub fn init(c: u8) -> Option<PromptKind> {
        match c {
            b'i' => Some(PromptKind::Initial),
            b'r' => Some(PromptKind::Right),
            b'c' => Some(PromptKind::Continuation),
            b's' => Some(PromptKind::Secondary),
            _ => None,
        }
    }
}

/// The values for the `redraw` extension to OSC133. This was
/// started by Kitty[1] and extended by Ghostty (the "last" option).
///
/// [1]: https://sw.kovidgoyal.net/kitty/shell-integration/#notes-for-shell-developers
// PORT NOTE: `enum(u2)` — the explicit `u2` backing tag pins the discriminant
// width in Zig; Rust's enum carries the same ordered variants (true, false,
// last) and the width invariant is not asserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redraw {
    /// The shell supports redrawing the full prompt and all continuations.
    /// This is the default value, it does not need to be explicitly set
    /// unless it is to reset a prior other value.
    True,

    /// The shell does NOT support redrawing. In this case, Ghostty will NOT
    /// clear any prompt lines on resize.
    False,

    /// The shell supports redrawing only the LAST line of the prompt.
    /// Ghostty will only clear the last line of the prompt on resize.
    ///
    /// This is specifically introduced because Bash only redraws the last
    /// line. It is literally the only shell that does this and it does this
    /// because its bad and they should feel bad. Don't be like Bash.
    Last,
}

/// Parse OSC 133, semantic prompts
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut OSCCommand> {
    let cap = if let Some(c) = parser.capture.as_ref() {
        c
    } else {
        parser.state = State::Invalid;
        return None;
    };
    let data = cap.trailing();
    if data.is_empty() {
        parser.state = State::Invalid;
        return None;
    }

    // PORT NOTE: Zig uses labeled blocks with `break :valid` for the "invalid
    // fallthrough" pattern and per-arm `break :<label>` for the early
    // "no options" success. Rust models the outer `valid:` block as a `'valid`
    // loop executed once; each command arm assigns the command then either
    // `break` (no options) or sets `options_unvalidated`, with `break 'valid`
    // for the invalid fallthrough. Reaching the end of the block returns the
    // command; `break 'valid` falls through to the invalid path below.
    //
    // All valid cases terminate within this block. Any fallthroughs
    // are invalid. This makes some of our parse logic a little less
    // repetitive.
    let valid = 'valid: {
        match data[0] {
            b'A' => 'fresh_line: {
                parser.command =
                    OSCCommand::SemanticPrompt(Command::init(Action::FreshLineNewPrompt));
                if data.len() == 1 {
                    break 'fresh_line;
                }
                if data[1] != b';' {
                    break 'valid false;
                }
                set_options(&mut parser.command, &data[2..]);
            }

            b'B' => 'end_prompt: {
                parser.command =
                    OSCCommand::SemanticPrompt(Command::init(Action::EndPromptStartInput));
                if data.len() == 1 {
                    break 'end_prompt;
                }
                if data[1] != b';' {
                    break 'valid false;
                }
                set_options(&mut parser.command, &data[2..]);
            }

            b'I' => 'end_prompt_line: {
                parser.command = OSCCommand::SemanticPrompt(Command::init(
                    Action::EndPromptStartInputTerminateEol,
                ));
                if data.len() == 1 {
                    break 'end_prompt_line;
                }
                if data[1] != b';' {
                    break 'valid false;
                }
                set_options(&mut parser.command, &data[2..]);
            }

            b'C' => 'end_input: {
                parser.command =
                    OSCCommand::SemanticPrompt(Command::init(Action::EndInputStartOutput));
                if data.len() == 1 {
                    break 'end_input;
                }
                if data[1] != b';' {
                    break 'valid false;
                }
                set_options(&mut parser.command, &data[2..]);
            }

            b'D' => 'end_command: {
                parser.command = OSCCommand::SemanticPrompt(Command::init(Action::EndCommand));
                if data.len() == 1 {
                    break 'end_command;
                }
                if data[1] != b';' {
                    break 'valid false;
                }
                set_options(&mut parser.command, &data[2..]);
            }

            b'L' => {
                if data.len() > 1 {
                    break 'valid false;
                }
                parser.command = OSCCommand::SemanticPrompt(Command::init(Action::FreshLine));
            }

            b'N' => 'new_command: {
                parser.command = OSCCommand::SemanticPrompt(Command::init(Action::NewCommand));
                if data.len() == 1 {
                    break 'new_command;
                }
                if data[1] != b';' {
                    break 'valid false;
                }
                set_options(&mut parser.command, &data[2..]);
            }

            b'P' => 'prompt_start: {
                parser.command = OSCCommand::SemanticPrompt(Command::init(Action::PromptStart));
                if data.len() == 1 {
                    break 'prompt_start;
                }
                if data[1] != b';' {
                    break 'valid false;
                }
                set_options(&mut parser.command, &data[2..]);
            }

            _ => break 'valid false,
        }

        true
    };

    if valid {
        return Some(&mut parser.command);
    }

    // Any fallthroughs are invalid
    parser.state = State::Invalid;
    None
}

// PORT NOTE: `parser.command.semantic_prompt.options_unvalidated = data[2..];`
// — in Zig this stores a borrowed slice into the capture buffer. The ported
// `Command` owns its options as a `Vec<u8>`, so we copy the RAW bytes in —
// NEVER a lossy String decode.
fn set_options(command: &mut OSCCommand, data: &[u8]) {
    if let OSCCommand::SemanticPrompt(sp) = command {
        sp.options_unvalidated = data.to_vec();
    }
}

#[cfg(test)]
mod tests {
    use super::option;
    use super::{Click, Command, PromptKind, Redraw};
    use crate::osc::{Command as OSCCommand, Parser};
    use rustty_os::string_encoding::{AllocatingWriter, WriteOrDecodeError};

    // PORT NOTE: `cmd.semantic_prompt` is `cmd` unwrapped to the
    // `Command::SemanticPrompt` payload. This helper mirrors the Zig
    // `cmd.semantic_prompt` projection after asserting the tag.
    fn semantic_prompt(cmd: &OSCCommand) -> &Command {
        match cmd {
            OSCCommand::SemanticPrompt(sp) => sp,
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_133_end_input_start_output() {
        let mut p: Parser = Parser::init(None);

        let input = "133;C";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);
        assert!(semantic_prompt(&cmd).read_option(option::AID).is_none());
        assert!(semantic_prompt(&cmd).read_option(option::CL).is_none());
    }

    #[test]
    fn osc_133_end_input_start_output_extra_contents() {
        let mut p: Parser = Parser::init(None);
        let input = "133;Cextra";
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_133_end_input_start_output_with_options() {
        let mut p: Parser = Parser::init(None);
        let input = "133;C;aid=foo";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);
        assert_eq!(
            b"foo".as_slice(),
            semantic_prompt(&cmd).read_option(option::AID).unwrap()
        );
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline=echo bobr kurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);

        semantic_prompt(&cmd).write_command_line(&mut w).unwrap();
        assert_eq!(b"echo bobr kurwa".as_slice(), w.written());
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_3() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline=echo bobr\\nkurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));

        semantic_prompt(&cmd).write_command_line(&mut w).unwrap();
        assert_eq!(b"echo bobr\nkurwa".as_slice(), w.written());
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_4() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline=$'echo bobr kurwa'";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);

        semantic_prompt(&cmd).write_command_line(&mut w).unwrap();
        assert_eq!(b"echo bobr kurwa".as_slice(), w.written());
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_5() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline='echo bobr kurwa'";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);

        semantic_prompt(&cmd).write_command_line(&mut w).unwrap();
        assert_eq!(b"echo bobr kurwa".as_slice(), w.written());
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_6() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline='echo bobr kurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);
        assert!(matches!(
            semantic_prompt(&cmd).write_command_line(&mut w),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_7() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline=$'echo bobr kurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);

        assert!(matches!(
            semantic_prompt(&cmd).write_command_line(&mut w),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_8() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline=$'";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);
        assert!(matches!(
            semantic_prompt(&cmd).write_command_line(&mut w),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_9() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline=";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);

        semantic_prompt(&cmd).write_command_line(&mut w).unwrap();
        assert_eq!(b"".as_slice(), w.written());
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_url_1() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline_url=echo bobr kurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);

        semantic_prompt(&cmd).write_command_line(&mut w).unwrap();
        assert_eq!(b"echo bobr kurwa".as_slice(), w.written());
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_url_2() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline_url=echo bobr%20kurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);

        semantic_prompt(&cmd).write_command_line(&mut w).unwrap();
        assert_eq!(b"echo bobr kurwa".as_slice(), w.written());
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_url_3() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline_url=echo bobr%3bkurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);

        semantic_prompt(&cmd).write_command_line(&mut w).unwrap();
        assert_eq!(b"echo bobr;kurwa".as_slice(), w.written());
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_url_4() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline_url=echo bobr%3kurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);
        assert!(matches!(
            semantic_prompt(&cmd).write_command_line(&mut w),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_url_5() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline_url=echo bobr%kurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);
        assert!(matches!(
            semantic_prompt(&cmd).write_command_line(&mut w),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_url_6() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline_url=echo bobr kurwa%20";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);

        semantic_prompt(&cmd).write_command_line(&mut w).unwrap();
        assert_eq!(b"echo bobr kurwa ".as_slice(), w.written());
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_url_7() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline_url=echo bobr kurwa%2";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);
        assert!(matches!(
            semantic_prompt(&cmd).write_command_line(&mut w),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn osc_133_end_input_start_output_with_cmdline_url_8() {
        let mut w = AllocatingWriter::init();

        let mut p: Parser = Parser::init(None);
        let input = "133;C;cmdline_url=echo bobr kurwa%";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndInputStartOutput);
        assert!(matches!(
            semantic_prompt(&cmd).write_command_line(&mut w),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn osc_133_fresh_line() {
        let mut p: Parser = Parser::init(None);

        let input = "133;L";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLine);
    }

    #[test]
    fn osc_133_fresh_line_extra_contents() {
        // Random
        {
            let mut p: Parser = Parser::init(None);
            let input = "133;Lol";
            for ch in input.bytes() {
                p.next(ch);
            }
            assert!(p.end(None).is_none());
        }

        // Options
        {
            let mut p: Parser = Parser::init(None);
            let input = "133;L;aid=foo";
            for ch in input.bytes() {
                p.next(ch);
            }
            assert!(p.end(None).is_none());
        }
    }

    #[test]
    fn osc_133_fresh_line_new_prompt() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert!(semantic_prompt(&cmd).read_option(option::AID).is_none());
        assert!(semantic_prompt(&cmd).read_option(option::CL).is_none());
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_aid() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;aid=14";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert_eq!(
            b"14".as_slice(),
            semantic_prompt(&cmd).read_option(option::AID).unwrap()
        );
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_eq_in_aid() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;aid=a=b";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert_eq!(
            b"a=b".as_slice(),
            semantic_prompt(&cmd).read_option(option::AID).unwrap()
        );
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_cl_line() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;cl=line";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert!(semantic_prompt(&cmd).read_option(option::CL) == Some(Click::Line));
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_cl_m() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;cl=m";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert!(semantic_prompt(&cmd).read_option(option::CL) == Some(Click::Multiple));
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_invalid_cl() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;cl=invalid";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert!(semantic_prompt(&cmd).read_option(option::CL).is_none());
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_trailing_semicolon() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_bare_key() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;barekey";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert!(semantic_prompt(&cmd).read_option(option::AID).is_none());
        assert!(semantic_prompt(&cmd).read_option(option::CL).is_none());
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_multiple_options() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;aid=foo;cl=line";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert_eq!(
            b"foo".as_slice(),
            semantic_prompt(&cmd).read_option(option::AID).unwrap()
        );
        assert!(semantic_prompt(&cmd).read_option(option::CL) == Some(Click::Line));
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_default_redraw() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert!(semantic_prompt(&cmd).read_option(option::REDRAW).is_none());
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_redraw_0() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;redraw=0";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert!(semantic_prompt(&cmd).read_option(option::REDRAW).unwrap() == Redraw::False);
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_redraw_1() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;redraw=1";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert!(semantic_prompt(&cmd).read_option(option::REDRAW).unwrap() == Redraw::True);
    }

    #[test]
    fn osc_133_fresh_line_new_prompt_with_invalid_redraw() {
        let mut p: Parser = Parser::init(None);

        let input = "133;A;redraw=x";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::FreshLineNewPrompt);
        assert!(semantic_prompt(&cmd).read_option(option::REDRAW).is_none());
    }

    #[test]
    fn osc_133_prompt_start() {
        let mut p: Parser = Parser::init(None);

        let input = "133;P";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::PromptStart);
        assert!(semantic_prompt(&cmd)
            .read_option(option::PROMPT_KIND)
            .is_none());
    }

    #[test]
    fn osc_133_prompt_start_with_k_i() {
        let mut p: Parser = Parser::init(None);

        let input = "133;P;k=i";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::PromptStart);
        assert!(
            semantic_prompt(&cmd).read_option(option::PROMPT_KIND) == Some(PromptKind::Initial)
        );
    }

    #[test]
    fn osc_133_prompt_start_with_k_r() {
        let mut p: Parser = Parser::init(None);

        let input = "133;P;k=r";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::PromptStart);
        assert!(semantic_prompt(&cmd).read_option(option::PROMPT_KIND) == Some(PromptKind::Right));
    }

    #[test]
    fn osc_133_prompt_start_with_k_c() {
        let mut p: Parser = Parser::init(None);

        let input = "133;P;k=c";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::PromptStart);
        assert!(
            semantic_prompt(&cmd).read_option(option::PROMPT_KIND)
                == Some(PromptKind::Continuation)
        );
    }

    #[test]
    fn osc_133_prompt_start_with_k_s() {
        let mut p: Parser = Parser::init(None);

        let input = "133;P;k=s";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::PromptStart);
        assert!(
            semantic_prompt(&cmd).read_option(option::PROMPT_KIND) == Some(PromptKind::Secondary)
        );
    }

    #[test]
    fn osc_133_prompt_start_with_invalid_k() {
        let mut p: Parser = Parser::init(None);

        let input = "133;P;k=x";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::PromptStart);
        assert!(semantic_prompt(&cmd)
            .read_option(option::PROMPT_KIND)
            .is_none());
    }

    #[test]
    fn osc_133_prompt_start_extra_contents() {
        let mut p: Parser = Parser::init(None);
        let input = "133;Pextra";
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_133_new_command() {
        let mut p: Parser = Parser::init(None);

        let input = "133;N";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::NewCommand);
        assert!(semantic_prompt(&cmd).read_option(option::AID).is_none());
        assert!(semantic_prompt(&cmd).read_option(option::CL).is_none());
    }

    #[test]
    fn osc_133_new_command_with_aid() {
        let mut p: Parser = Parser::init(None);

        let input = "133;N;aid=foo";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::NewCommand);
        assert_eq!(
            b"foo".as_slice(),
            semantic_prompt(&cmd).read_option(option::AID).unwrap()
        );
    }

    #[test]
    fn osc_133_new_command_with_cl_line() {
        let mut p: Parser = Parser::init(None);

        let input = "133;N;cl=line";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::NewCommand);
        assert!(semantic_prompt(&cmd).read_option(option::CL) == Some(Click::Line));
    }

    #[test]
    fn osc_133_new_command_with_multiple_options() {
        let mut p: Parser = Parser::init(None);

        let input = "133;N;aid=foo;cl=line";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::NewCommand);
        assert_eq!(
            b"foo".as_slice(),
            semantic_prompt(&cmd).read_option(option::AID).unwrap()
        );
        assert!(semantic_prompt(&cmd).read_option(option::CL) == Some(Click::Line));
    }

    #[test]
    fn osc_133_new_command_extra_contents() {
        let mut p: Parser = Parser::init(None);
        let input = "133;Nextra";
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_133_end_prompt_start_input() {
        let mut p: Parser = Parser::init(None);

        let input = "133;B";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndPromptStartInput);
    }

    #[test]
    fn osc_133_end_prompt_start_input_extra_contents() {
        let mut p: Parser = Parser::init(None);
        let input = "133;Bextra";
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_133_end_prompt_start_input_with_options() {
        let mut p: Parser = Parser::init(None);
        let input = "133;B;aid=foo";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndPromptStartInput);
        assert_eq!(
            b"foo".as_slice(),
            semantic_prompt(&cmd).read_option(option::AID).unwrap()
        );
    }

    #[test]
    fn osc_133_end_prompt_start_input_terminate_eol() {
        let mut p: Parser = Parser::init(None);

        let input = "133;I";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndPromptStartInputTerminateEol);
    }

    #[test]
    fn osc_133_end_prompt_start_input_terminate_eol_extra_contents() {
        let mut p: Parser = Parser::init(None);
        let input = "133;Iextra";
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_133_end_prompt_start_input_terminate_eol_with_options() {
        let mut p: Parser = Parser::init(None);
        let input = "133;I;aid=foo";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndPromptStartInputTerminateEol);
        assert_eq!(
            b"foo".as_slice(),
            semantic_prompt(&cmd).read_option(option::AID).unwrap()
        );
    }

    #[test]
    fn osc_133_end_command() {
        let mut p: Parser = Parser::init(None);

        let input = "133;D";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndCommand);
        assert!(semantic_prompt(&cmd)
            .read_option(option::EXIT_CODE)
            .is_none());
        assert!(semantic_prompt(&cmd).read_option(option::AID).is_none());
        assert!(semantic_prompt(&cmd).read_option(option::ERR).is_none());
    }

    #[test]
    fn osc_133_end_command_extra_contents() {
        let mut p: Parser = Parser::init(None);
        let input = "133;Dextra";
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_133_end_command_with_exit_code_0() {
        let mut p: Parser = Parser::init(None);

        let input = "133;D;0";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndCommand);
        assert!(semantic_prompt(&cmd).read_option(option::EXIT_CODE) == Some(0));
    }

    #[test]
    fn osc_133_end_command_with_exit_code_and_aid() {
        let mut p: Parser = Parser::init(None);

        let input = "133;D;12;aid=foo";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, OSCCommand::SemanticPrompt(_)));
        assert!(semantic_prompt(&cmd).action == super::Action::EndCommand);
        assert_eq!(
            b"foo".as_slice(),
            semantic_prompt(&cmd).read_option(option::AID).unwrap()
        );
        assert!(semantic_prompt(&cmd).read_option(option::EXIT_CODE) == Some(12));
    }

    #[test]
    fn option_read_aid() {
        assert_eq!(
            b"test123".as_slice(),
            option::AID.read(b"aid=test123").unwrap()
        );
        assert_eq!(
            b"myaid".as_slice(),
            option::AID.read(b"cl=line;aid=myaid;k=i").unwrap()
        );
        assert!(option::AID.read(b"cl=line;k=i").is_none());
        assert_eq!(b"".as_slice(), option::AID.read(b"aid=").unwrap());
        assert_eq!(
            b"last".as_slice(),
            option::AID.read(b"k=i;aid=last").unwrap()
        );
        assert_eq!(
            b"first".as_slice(),
            option::AID.read(b"aid=first;k=i").unwrap()
        );
        assert!(option::AID.read(b"").is_none());
        assert!(option::AID.read(b"aid").is_none());
        assert_eq!(
            b"value".as_slice(),
            option::AID.read(b";;aid=value;;").unwrap()
        );
    }

    #[test]
    fn option_read_cl() {
        assert!(option::CL.read(b"cl=line").unwrap() == Click::Line);
        assert!(option::CL.read(b"cl=m").unwrap() == Click::Multiple);
        assert!(option::CL.read(b"cl=v").unwrap() == Click::ConservativeVertical);
        assert!(option::CL.read(b"cl=w").unwrap() == Click::SmartVertical);
        assert!(option::CL.read(b"cl=invalid").is_none());
        assert!(option::CL.read(b"aid=foo").is_none());
    }

    #[test]
    fn option_read_prompt_kind() {
        assert!(option::PROMPT_KIND.read(b"k=i").unwrap() == PromptKind::Initial);
        assert!(option::PROMPT_KIND.read(b"k=r").unwrap() == PromptKind::Right);
        assert!(option::PROMPT_KIND.read(b"k=c").unwrap() == PromptKind::Continuation);
        assert!(option::PROMPT_KIND.read(b"k=s").unwrap() == PromptKind::Secondary);
        assert!(option::PROMPT_KIND.read(b"k=x").is_none());
        assert!(option::PROMPT_KIND.read(b"k=ii").is_none());
        assert!(option::PROMPT_KIND.read(b"k=").is_none());
    }

    #[test]
    fn option_read_err() {
        assert_eq!(
            b"some_error".as_slice(),
            option::ERR.read(b"err=some_error").unwrap()
        );
        assert!(option::ERR.read(b"aid=foo").is_none());
    }

    #[test]
    fn option_read_redraw() {
        assert!(option::REDRAW.read(b"redraw=1").unwrap() == Redraw::True);
        assert!(option::REDRAW.read(b"redraw=0").unwrap() == Redraw::False);
        assert!(option::REDRAW.read(b"redraw=last").unwrap() == Redraw::Last);
        assert!(option::REDRAW.read(b"redraw=2").is_none());
        assert!(option::REDRAW.read(b"redraw=10").is_none());
        assert!(option::REDRAW.read(b"redraw=").is_none());
    }

    #[test]
    fn option_read_special_key() {
        assert!(option::SPECIAL_KEY.read(b"special_key=1").unwrap());
        assert!(!option::SPECIAL_KEY.read(b"special_key=0").unwrap());
        assert!(option::SPECIAL_KEY.read(b"special_key=x").is_none());
    }

    #[test]
    fn option_read_click_events() {
        use super::ClickEvents;
        assert!(option::CLICK_EVENTS.read(b"click_events=yes").is_none());
        assert!(option::CLICK_EVENTS.read(b"click_events=0").is_none());
        assert!(option::CLICK_EVENTS.read(b"click_events=1").unwrap() == ClickEvents::Absolute);
        assert!(option::CLICK_EVENTS.read(b"click_events=2").unwrap() == ClickEvents::Relative);
    }

    #[test]
    fn option_read_exit_code() {
        assert!(option::EXIT_CODE.read(b"42").unwrap() == 42);
        assert!(option::EXIT_CODE.read(b"0").unwrap() == 0);
        assert!(option::EXIT_CODE.read(b"-1").unwrap() == -1);
        assert!(option::EXIT_CODE.read(b"abc").is_none());
        assert!(option::EXIT_CODE.read(b"127;aid=foo").unwrap() == 127);
    }
}
