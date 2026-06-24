//! OSC 3008: Hierarchical Context Signalling (UAPI spec)
//! Specification: https://uapi-group.org/specifications/specs/osc_context/
//!
//! OSC 3008 allows programs to signal context changes to the terminal emulator.
//! Each context has an identifier and metadata fields. Contexts are hierarchical
//! and form a stack.
//!
//! PORTS: ghostty src/terminal/osc/parsers/context_signal.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

// PORT NOTE: `const Parser = @import("../../osc.zig").Parser;`
// PORT NOTE: `const OSCCommand = @import("../../osc.zig").Command;` — the OSC
// parent `Command`, aliased to disambiguate from this module's own `Command`.
use crate::osc::{Command as OSCCommand, Parser, State};

// PORT NOTE: `const log = std.log.scoped(.osc_context_signal);` — rustty has no
// logging facade wired into this crate (matching the convention used by the
// `osc.zig` scoped logger), so the scoped logger and its calls are ported as
// commented lines.
// const log = std.log.scoped(.osc_context_signal);

/// Maximum length of a context identifier (per spec).
const MAX_CONTEXT_ID_LEN: usize = 64;

/// A single OSC 3008 context signal command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    // PORT NOTE: `pub const C = void;` — the per-command C-API payload type
    // marker used by the OSC C bindings. There is no C binding surface in this
    // crate, so it is ported as this note.
    pub action: Action,
    /// The context identifier. Must be 1-64 characters in the 32..126 byte range.
    pub id: String,
    /// Raw unparsed metadata fields after the context ID.
    /// Fields are semicolon-separated key=value pairs.
    /// Parsed lazily via `readOption`.
    pub metadata: String,
}

impl Command {
    /// Read a metadata field value from the raw fields string.
    /// Returns null if the field is not present or malformed.
    // PORT NOTE: the reference uses a `comptime option: Field` parameter with a
    // dependent return type `option.Type()`. Rust has no comptime-dependent
    // return types, so `read_option` takes a runtime `Field` and returns a
    // tagged `FieldValue` whose active variant matches `Field::value_type`.
    pub fn read_option(&self, option: Field) -> FieldValue {
        option.read(self.metadata.as_bytes())
    }
}

/// Context types defined by the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextType {
    Boot,
    Container,
    Vm,
    Elevate,
    Chpriv,
    Subcontext,
    Remote,
    Shell,
    Command,
    App,
    Service,
    Session,
}

impl ContextType {
    pub fn parse(value: &[u8]) -> Option<ContextType> {
        // PORT NOTE: `std.meta.stringToEnum(ContextType, value)` — match each
        // tag name verbatim against the field value bytes.
        match value {
            b"boot" => Some(ContextType::Boot),
            b"container" => Some(ContextType::Container),
            b"vm" => Some(ContextType::Vm),
            b"elevate" => Some(ContextType::Elevate),
            b"chpriv" => Some(ContextType::Chpriv),
            b"subcontext" => Some(ContextType::Subcontext),
            b"remote" => Some(ContextType::Remote),
            b"shell" => Some(ContextType::Shell),
            b"command" => Some(ContextType::Command),
            b"app" => Some(ContextType::App),
            b"service" => Some(ContextType::Service),
            b"session" => Some(ContextType::Session),
            _ => None,
        }
    }
}

/// Exit status for the `exit` end-sequence field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    Success,
    Failure,
    Crash,
    Interrupt,
}

impl ExitStatus {
    pub fn parse(value: &[u8]) -> Option<ExitStatus> {
        // PORT NOTE: `std.meta.stringToEnum(ExitStatus, value)`.
        match value {
            b"success" => Some(ExitStatus::Success),
            b"failure" => Some(ExitStatus::Failure),
            b"crash" => Some(ExitStatus::Crash),
            b"interrupt" => Some(ExitStatus::Interrupt),
            _ => None,
        }
    }
}

/// Metadata fields that can appear in OSC 3008 sequences.
/// Fields are read lazily from the raw string using the `read` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    // Start sequence fields
    Type,
    User,
    Hostname,
    Machineid,
    Bootid,
    Pid,
    Pidfdid,
    Comm,
    Cwd,
    Cmdline,
    Vm,
    Container,
    Targetuser,
    Targethost,
    Sessionid,

    // End sequence fields
    Exit,
    Status,
    Signal,
}

/// The dependent value produced by `Field::read`. The active variant matches
/// the `Field::Type()` of the field that produced it.
// PORT NOTE: this enum stands in for the comptime-dependent `option.Type()`
// return type of `Field.read` in the reference:
//   .type            => ?ContextType
//   .exit            => ?ExitStatus
//   .pid/.pidfdid    => ?u64
//   .status          => ?u64
//   // All other fields are string values
//   all other fields => ?[]const u8
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Type(Option<ContextType>),
    Exit(Option<ExitStatus>),
    Int(Option<u64>),
    Str(Option<String>),
}

impl FieldValue {
    /// Helper for `?ContextType` fields.
    pub fn as_type(&self) -> Option<ContextType> {
        match self {
            FieldValue::Type(v) => *v,
            _ => panic!("field value is not a context type"),
        }
    }

    /// Helper for `?ExitStatus` fields.
    pub fn as_exit(&self) -> Option<ExitStatus> {
        match self {
            FieldValue::Exit(v) => *v,
            _ => panic!("field value is not an exit status"),
        }
    }

    /// Helper for `?u64` fields.
    pub fn as_int(&self) -> Option<u64> {
        match self {
            FieldValue::Int(v) => *v,
            _ => panic!("field value is not an integer"),
        }
    }

    /// Helper for `?[]const u8` fields.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            FieldValue::Str(v) => v.as_deref(),
            _ => panic!("field value is not a string"),
        }
    }

    /// Whether the optional field value is absent (`null`).
    pub fn is_null(&self) -> bool {
        match self {
            FieldValue::Type(v) => v.is_none(),
            FieldValue::Exit(v) => v.is_none(),
            FieldValue::Int(v) => v.is_none(),
            FieldValue::Str(v) => v.is_none(),
        }
    }
}

impl Field {
    fn key(self) -> &'static [u8] {
        // PORT NOTE: `@tagName(self)` — the field's spec key spelling.
        match self {
            Field::Type => b"type",
            Field::User => b"user",
            Field::Hostname => b"hostname",
            Field::Machineid => b"machineid",
            Field::Bootid => b"bootid",
            Field::Pid => b"pid",
            Field::Pidfdid => b"pidfdid",
            Field::Comm => b"comm",
            Field::Cwd => b"cwd",
            Field::Cmdline => b"cmdline",
            Field::Vm => b"vm",
            Field::Container => b"container",
            Field::Targetuser => b"targetuser",
            Field::Targethost => b"targethost",
            Field::Sessionid => b"sessionid",
            Field::Exit => b"exit",
            Field::Status => b"status",
            Field::Signal => b"signal",
        }
    }

    /// Read the field value from the raw fields string.
    ///
    /// The raw fields string contains semicolon-separated key=value pairs
    /// e.g. "type=container;user=lennart;hostname=zeta".
    ///
    /// Unknown or malformed fields are ignored per the specification.
    pub fn read(self, raw: &[u8]) -> FieldValue {
        // var it = std.mem.splitScalar(u8, raw, ';');
        // while (it.next()) |full|
        for full in raw.split(|&b| b == b';') {
            // Parse key=value
            // const value = value: {
            let value: &[u8] = {
                if let Some(eql_idx) = full.iter().position(|&b| b == b'=') {
                    if full[0..eql_idx] == *self.key() {
                        full.get(eql_idx + 1..).unwrap_or(&[])
                    } else {
                        // continue;
                        continue;
                    }
                } else {
                    // continue;
                    continue;
                }
            };

            return match self {
                Field::Type => FieldValue::Type(ContextType::parse(value)),
                Field::Exit => FieldValue::Exit(ExitStatus::parse(value)),
                Field::Pid | Field::Pidfdid | Field::Status => {
                    // value: {
                    //     for (value) |c| {
                    //         if (c < '0' or c > '9') break :value null;
                    //     }
                    //     break :value std.fmt.parseInt(u64, value, 10) catch null;
                    // }
                    let mut parsed: Option<u64> = None;
                    'value: {
                        for &c in value {
                            // PORT NOTE: faithful to the reference `c < '0' or
                            // c > '9'`; clippy's range-contains rewrite is
                            // suppressed to preserve the ported comparison.
                            #[allow(clippy::manual_range_contains)]
                            if c < b'0' || c > b'9' {
                                break 'value;
                            }
                        }
                        parsed = core::str::from_utf8(value)
                            .ok()
                            .and_then(|s| s.parse::<u64>().ok());
                    }
                    FieldValue::Int(parsed)
                }
                // String fields
                Field::User
                | Field::Hostname
                | Field::Machineid
                | Field::Bootid
                | Field::Comm
                | Field::Cwd
                | Field::Cmdline
                | Field::Vm
                | Field::Container
                | Field::Targetuser
                | Field::Targethost
                | Field::Sessionid
                | Field::Signal => {
                    if !value.is_empty() {
                        FieldValue::Str(Some(String::from_utf8_lossy(value).into_owned()))
                    } else {
                        FieldValue::Str(None)
                    }
                }
            };
        }

        // Not found
        match self {
            Field::Type => FieldValue::Type(None),
            Field::Exit => FieldValue::Exit(None),
            Field::Pid | Field::Pidfdid | Field::Status => FieldValue::Int(None),
            _ => FieldValue::Str(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// OSC 3008;start=<id> — initiates, updates, or returns to a context.
    Start,
    /// OSC 3008;end=<id> — terminates a context.
    End,
}

/// Parse OSC 3008: hierarchical context signalling.
///
/// Expected data format (after "3008;" prefix has been consumed by the state machine):
///   start=<id>[;<field>=<value>]*
///   end=<id>[;<field>=<value>]*
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut OSCCommand> {
    let data: &[u8] = {
        let cap = if let Some(c) = parser.capture.as_ref() {
            c
        } else {
            parser.state = State::Invalid;
            return None;
        };
        cap.trailing()
    };
    if data.is_empty() {
        parser.state = State::Invalid;
        return None;
    }

    // Determine the action (start= or end=)
    let action: Action = 'action: {
        if data.starts_with(b"start=") {
            break 'action Action::Start;
        }
        if data.starts_with(b"end=") {
            break 'action Action::End;
        }

        // log.warn("OSC 3008: expected 'start=' or 'end=' prefix, got: {s}", .{
        //     data[0..@min(data.len, 10)],
        // });
        parser.state = State::Invalid;
        return None;
    };

    // Skip past the "start=" or "end=" prefix
    let prefix_len: usize = match action {
        Action::Start => "start=".len(),
        Action::End => "end=".len(),
    };
    let rest = &data[prefix_len..];

    if rest.is_empty() {
        // log.warn("OSC 3008: missing context ID", .{});
        parser.state = State::Invalid;
        return None;
    }

    // Extract the context ID (up to the first semicolon or end of data)
    let id_end = rest.iter().position(|&b| b == b';').unwrap_or(rest.len());
    let id = &rest[0..id_end];

    // Validate context ID length (1-64 chars per spec)
    if id.is_empty() || id.len() > MAX_CONTEXT_ID_LEN {
        // log.warn("OSC 3008: context ID length {d} out of range (1-{d})", .{
        //     id.len,
        //     max_context_id_len,
        // });
        parser.state = State::Invalid;
        return None;
    }

    // Validate context ID characters (32..126 byte range per spec)
    for &c in id {
        // PORT NOTE: faithful to the reference `c < 0x20 or c > 0x7e`; clippy's
        // range-contains rewrite is suppressed to preserve the ported comparison.
        #[allow(clippy::manual_range_contains)]
        if c < 0x20 || c > 0x7e {
            // log.warn("OSC 3008: invalid character 0x{x:0>2} in context ID", .{c});
            parser.state = State::Invalid;
            return None;
        }
    }

    // Extract raw metadata fields (everything after the ID)
    let metadata: &[u8] = if id_end < rest.len() {
        &rest[id_end + 1..]
    } else {
        b""
    };

    let id = String::from_utf8_lossy(id).into_owned();
    let metadata = String::from_utf8_lossy(metadata).into_owned();

    parser.command = OSCCommand::ContextSignal(Command {
        action,
        id,
        metadata,
    });

    Some(&mut parser.command)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::{ContextType, ExitStatus, Field};
    use crate::osc::{Command, Parser};

    #[test]
    fn osc_3008_basic_start_command() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;start=abc123";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(cs.action, super::Action::Start);
        assert_eq!(cs.id, "abc123");
        assert_eq!(cs.metadata, "");
    }

    #[test]
    fn osc_3008_basic_end_command() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;end=abc123";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(cs.action, super::Action::End);
        assert_eq!(cs.id, "abc123");
        assert_eq!(cs.metadata, "");
    }

    #[test]
    fn osc_3008_start_with_metadata_fields() {
        let mut p: Parser = Parser::init(None);
        let input =
            "3008;start=bed86fab93af4328bbed0a1224af6d40;type=container;user=lennart;hostname=zeta";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(cs.action, super::Action::Start);
        assert_eq!(cs.id, "bed86fab93af4328bbed0a1224af6d40");

        // Read individual fields
        assert_eq!(
            cs.read_option(Field::Type).as_type().unwrap(),
            ContextType::Container
        );
        assert_eq!(cs.read_option(Field::User).as_str().unwrap(), "lennart");
        assert_eq!(cs.read_option(Field::Hostname).as_str().unwrap(), "zeta");
    }

    #[test]
    fn osc_3008_start_with_all_common_fields() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;start=myctx;type=shell;user=root;hostname=myhost;machineid=3deb5353d3ba43d08201c136a47ead7b;bootid=d4a3d0fdf2e24fdea6d971ce73f4fbf2;pid=1062862;pidfdid=1063162;comm=bash";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(
            cs.read_option(Field::Type).as_type().unwrap(),
            ContextType::Shell
        );
        assert_eq!(cs.read_option(Field::User).as_str().unwrap(), "root");
        assert_eq!(cs.read_option(Field::Hostname).as_str().unwrap(), "myhost");
        assert_eq!(
            cs.read_option(Field::Machineid).as_str().unwrap(),
            "3deb5353d3ba43d08201c136a47ead7b"
        );
        assert_eq!(
            cs.read_option(Field::Bootid).as_str().unwrap(),
            "d4a3d0fdf2e24fdea6d971ce73f4fbf2"
        );
        assert_eq!(cs.read_option(Field::Pid).as_int().unwrap(), 1062862u64);
        assert_eq!(cs.read_option(Field::Pidfdid).as_int().unwrap(), 1063162u64);
        assert_eq!(cs.read_option(Field::Comm).as_str().unwrap(), "bash");
    }

    #[test]
    fn osc_3008_end_with_exit_metadata() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;end=myctx;exit=success;status=0";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(cs.action, super::Action::End);
        assert_eq!(cs.id, "myctx");
        assert_eq!(
            cs.read_option(Field::Exit).as_exit().unwrap(),
            ExitStatus::Success
        );
        assert_eq!(cs.read_option(Field::Status).as_int().unwrap(), 0u64);
    }

    #[test]
    fn osc_3008_end_with_failure_exit() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;end=myctx;exit=failure;status=1;signal=SIGKILL";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(
            cs.read_option(Field::Exit).as_exit().unwrap(),
            ExitStatus::Failure
        );
        assert_eq!(cs.read_option(Field::Status).as_int().unwrap(), 1u64);
        assert_eq!(cs.read_option(Field::Signal).as_str().unwrap(), "SIGKILL");
    }

    #[test]
    fn osc_3008_unknown_fields_are_ignored() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;start=myctx;type=shell;unknownfield=value;user=root";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(
            cs.read_option(Field::Type).as_type().unwrap(),
            ContextType::Shell
        );
        assert_eq!(cs.read_option(Field::User).as_str().unwrap(), "root");
    }

    #[test]
    fn osc_3008_missing_field_returns_null() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;start=myctx;user=lennart";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert!(cs.read_option(Field::Type).is_null());
        assert!(cs.read_option(Field::Hostname).is_null());
        assert!(cs.read_option(Field::Pid).is_null());
    }

    #[test]
    fn osc_3008_invalid_prefix() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;bogus=abc123";
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_3008_empty_data() {
        // Can't really produce empty data after "3008;" because the state machine
        // won't write a writer for that case, but we test the edge case where
        // only "start=" is present with no ID.
        let mut p: Parser = Parser::init(None);
        let input = "3008;start=";
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_3008_max_length_context_id() {
        let mut p: Parser = Parser::init(None);
        let id = "a".repeat(64);
        let input = format!("3008;start={}", id);
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(cs.id, id);
    }

    #[test]
    fn osc_3008_over_length_context_id() {
        let mut p: Parser = Parser::init(None);
        let id = "a".repeat(65);
        let input = format!("3008;start={}", id);
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_3008_context_type_enum_coverage() {
        struct Case {
            str: &'static [u8],
            expected: ContextType,
        }
        let types = [
            Case {
                str: b"boot",
                expected: ContextType::Boot,
            },
            Case {
                str: b"container",
                expected: ContextType::Container,
            },
            Case {
                str: b"vm",
                expected: ContextType::Vm,
            },
            Case {
                str: b"elevate",
                expected: ContextType::Elevate,
            },
            Case {
                str: b"chpriv",
                expected: ContextType::Chpriv,
            },
            Case {
                str: b"subcontext",
                expected: ContextType::Subcontext,
            },
            Case {
                str: b"remote",
                expected: ContextType::Remote,
            },
            Case {
                str: b"shell",
                expected: ContextType::Shell,
            },
            Case {
                str: b"command",
                expected: ContextType::Command,
            },
            Case {
                str: b"app",
                expected: ContextType::App,
            },
            Case {
                str: b"service",
                expected: ContextType::Service,
            },
            Case {
                str: b"session",
                expected: ContextType::Session,
            },
        ];

        for t in types {
            assert_eq!(t.expected, ContextType::parse(t.str).unwrap());
        }

        assert!(ContextType::parse(b"invalid").is_none());
    }

    #[test]
    fn osc_3008_exit_status_enum_coverage() {
        assert_eq!(ExitStatus::parse(b"success").unwrap(), ExitStatus::Success);
        assert_eq!(ExitStatus::parse(b"failure").unwrap(), ExitStatus::Failure);
        assert_eq!(ExitStatus::parse(b"crash").unwrap(), ExitStatus::Crash);
        assert_eq!(
            ExitStatus::parse(b"interrupt").unwrap(),
            ExitStatus::Interrupt
        );
        assert!(ExitStatus::parse(b"invalid").is_none());
    }

    #[test]
    fn osc_3008_spec_example_container_start() {
        // From the spec: a new container "foobar" invoked by user "lennart" on host "zeta"
        let mut p: Parser = Parser::init(None);
        let input = "3008;start=bed86fab93af4328bbed0a1224af6d40;type=container;user=lennart;hostname=zeta;machineid=3deb5353d3ba43d08201c136a47ead7b;bootid=d4a3d0fdf2e24fdea6d971ce73f4fbf2;pid=1062862;pidfdid=1063162;comm=systemd-nspawn;container=foobar";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(cs.action, super::Action::Start);
        assert_eq!(cs.id, "bed86fab93af4328bbed0a1224af6d40");
        assert_eq!(
            cs.read_option(Field::Type).as_type().unwrap(),
            ContextType::Container
        );
        assert_eq!(cs.read_option(Field::User).as_str().unwrap(), "lennart");
        assert_eq!(cs.read_option(Field::Hostname).as_str().unwrap(), "zeta");
        assert_eq!(
            cs.read_option(Field::Comm).as_str().unwrap(),
            "systemd-nspawn"
        );
        assert_eq!(cs.read_option(Field::Container).as_str().unwrap(), "foobar");
        assert_eq!(cs.read_option(Field::Pid).as_int().unwrap(), 1062862u64);
    }

    #[test]
    fn osc_3008_spec_example_context_end() {
        // From the spec: context end
        let mut p: Parser = Parser::init(None);
        let input = "3008;end=bed86fab93af4328bbed0a1224af6d40";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(cs.action, super::Action::End);
        assert_eq!(cs.id, "bed86fab93af4328bbed0a1224af6d40");
    }

    #[test]
    fn osc_3008_cwd_and_cmdline_fields() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;start=myctx;type=command;cwd=/home/user;cmdline=ls -la";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(cs.read_option(Field::Cwd).as_str().unwrap(), "/home/user");
        assert_eq!(cs.read_option(Field::Cmdline).as_str().unwrap(), "ls -la");
    }

    #[test]
    fn osc_3008_start_command_with_no_fields() {
        let mut p: Parser = Parser::init(None);
        let input = "3008;start=simpleid";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, Command::ContextSignal(_)));
        let cs = match cmd {
            Command::ContextSignal(cs) => cs,
            _ => unreachable!(),
        };
        assert_eq!(cs.action, super::Action::Start);
        assert_eq!(cs.id, "simpleid");
        assert!(cs.read_option(Field::Type).is_null());
        assert!(cs.read_option(Field::User).is_null());
        assert!(cs.read_option(Field::Exit).is_null());
    }
}
