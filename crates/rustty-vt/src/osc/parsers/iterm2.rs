//! PORTS: ghostty src/terminal/osc/parsers/iterm2.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser, State};

// PORT NOTE: `const log = std.log.scoped(.osc_iterm2);` — the reference scopes a
// logger for this module. rustty-vt does not thread a logging dependency; the
// single `log.debug` call below is preserved as a verbatim comment at its use
// site.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    AddAnnotation,
    AddHiddenAnnotation,
    Block,
    Button,
    ClearCapturedOutput,
    ClearScrollback,
    Copy,
    CopyToClipboard,
    CurrentDir,
    CursorShape,
    Custom,
    Disinter,
    EndCopy,
    File,
    FileEnd,
    FilePart,
    HighlightCursorLine,
    MultipartFile,
    OpenURL,
    PopKeyLabels,
    PushKeyLabels,
    RemoteHost,
    ReportCellSize,
    ReportVariable,
    RequestAttention,
    RequestUpload,
    SetBackgroundImageFile,
    SetBadgeFormat,
    SetColors,
    SetKeyLabel,
    SetMark,
    SetProfile,
    SetUserVar,
    ShellIntegrationVersion,
    StealFocus,
    UnicodeVersion,
}

// Instead of using `std.meta.stringToEnum` we set up a StaticStringMap so
// that we can get ASCII case-insensitive lookups.
//
// PORT NOTE: `std.StaticStringMapWithEql(Key, std.ascii.eqlIgnoreCase)` built
// comptime from `@typeInfo(Key).@"enum".fields`. The field names are the Zig
// enum tag names; we list them here as `(name, Key)` pairs and perform the
// case-insensitive lookup explicitly in `map_get`.
const MAP: &[(&str, Key)] = &[
    ("AddAnnotation", Key::AddAnnotation),
    ("AddHiddenAnnotation", Key::AddHiddenAnnotation),
    ("Block", Key::Block),
    ("Button", Key::Button),
    ("ClearCapturedOutput", Key::ClearCapturedOutput),
    ("ClearScrollback", Key::ClearScrollback),
    ("Copy", Key::Copy),
    ("CopyToClipboard", Key::CopyToClipboard),
    ("CurrentDir", Key::CurrentDir),
    ("CursorShape", Key::CursorShape),
    ("Custom", Key::Custom),
    ("Disinter", Key::Disinter),
    ("EndCopy", Key::EndCopy),
    ("File", Key::File),
    ("FileEnd", Key::FileEnd),
    ("FilePart", Key::FilePart),
    ("HighlightCursorLine", Key::HighlightCursorLine),
    ("MultipartFile", Key::MultipartFile),
    ("OpenURL", Key::OpenURL),
    ("PopKeyLabels", Key::PopKeyLabels),
    ("PushKeyLabels", Key::PushKeyLabels),
    ("RemoteHost", Key::RemoteHost),
    ("ReportCellSize", Key::ReportCellSize),
    ("ReportVariable", Key::ReportVariable),
    ("RequestAttention", Key::RequestAttention),
    ("RequestUpload", Key::RequestUpload),
    ("SetBackgroundImageFile", Key::SetBackgroundImageFile),
    ("SetBadgeFormat", Key::SetBadgeFormat),
    ("SetColors", Key::SetColors),
    ("SetKeyLabel", Key::SetKeyLabel),
    ("SetMark", Key::SetMark),
    ("SetProfile", Key::SetProfile),
    ("SetUserVar", Key::SetUserVar),
    ("ShellIntegrationVersion", Key::ShellIntegrationVersion),
    ("StealFocus", Key::StealFocus),
    ("UnicodeVersion", Key::UnicodeVersion),
];

/// PORT NOTE: `map.get(key_str)` with `std.ascii.eqlIgnoreCase` — ASCII
/// case-insensitive lookup over the comptime `MAP`.
fn map_get(key_str: &[u8]) -> Option<Key> {
    for (name, key) in MAP {
        if name.as_bytes().eq_ignore_ascii_case(key_str) {
            return Some(*key);
        }
    }
    None
}

/// Parse OSC 1337
/// https://iterm2.com/documentation-escape-codes.html
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut Command> {
    // assert(parser.state == .@"1337");
    debug_assert!(parser.state == State::S1337);

    let cap = if let Some(c) = parser.capture.as_mut() {
        c
    } else {
        parser.state = State::Invalid;
        return None;
    };
    if !cap.write_byte(&mut parser.buffer, 0) {
        parser.state = State::Invalid;
        return None;
    }

    // PORT NOTE: the reference splits `data` in place by overwriting the `=`
    // separator with a sentinel `0` (`data[index] = 0`) and produces two
    // `[:0]u8` sentinel-terminated slices into the capture buffer. We operate
    // over the mutable capture bytes and compute the key/value byte ranges,
    // mirroring the trailing-`0` (and middle-`0`) sentinel handling exactly.
    let data = cap.trailing_mut(&mut parser.buffer);

    let (key_str, value_): (&[u8], Option<Vec<u8>>) = {
        match data.iter().position(|&b| b == b'=') {
            None => {
                // .{ data[0 .. data.len - 1 :0], null }
                (&data[0..data.len() - 1], None)
            }
            Some(index) => {
                data[index] = 0;
                // .{ data[0..index :0], data[index + 1 .. data.len - 1 :0] }
                let value = data[index + 1..data.len() - 1].to_vec();
                (&data[0..index], Some(value))
            }
        }
    };

    let key = match map_get(key_str) {
        Some(k) => k,
        None => {
            parser.command = Command::Invalid;
            return None;
        }
    };

    match key {
        Key::Copy => {
            let mut value = match value_ {
                Some(v) => v,
                None => {
                    parser.command = Command::Invalid;
                    return None;
                }
            };

            // Sending a blank entry to clear the clipboard is an OSC 52-ism,
            // make sure that is invalid here.
            if value.is_empty() {
                parser.command = Command::Invalid;
                return None;
            }

            // base64 value must be prefixed by a colon
            if value[0] != b':' {
                parser.command = Command::Invalid;
                return None;
            }

            // value = value[1..value.len :0];
            value = value[1..value.len()].to_vec();

            // Sending a blank entry to clear the clipboard is an OSC 52-ism,
            // make sure that is invalid here.
            if value.is_empty() {
                parser.command = Command::Invalid;
                return None;
            }

            // Sending a '?' to query the clipboard is an OSC 52-ism, make sure
            // that is invalid here.
            if value.len() == 1 && value[0] == b'?' {
                parser.command = Command::Invalid;
                return None;
            }

            // It would be better to check for valid base64 data here, but that
            // would mean parsing the base64 data twice in the "normal" case.

            parser.command = Command::ClipboardContents {
                kind: b'c',
                // PORT NOTE (fidelity): reference assigns `value` ([:0]u8) to
                // `data` ([:0]const u8) directly — RAW bytes, not lossy.
                data: value,
            };
            Some(&mut parser.command)
        }

        Key::CurrentDir => {
            let value = match value_ {
                Some(v) => v,
                None => {
                    parser.command = Command::Invalid;
                    return None;
                }
            };
            if value.is_empty() {
                parser.command = Command::Invalid;
                return None;
            }
            parser.command = Command::ReportPwd {
                // PORT NOTE (fidelity): reference assigns `value` ([:0]u8) to
                // `value` ([:0]const u8) directly — RAW bytes, not lossy.
                value,
            };
            Some(&mut parser.command)
        }

        Key::AddAnnotation
        | Key::AddHiddenAnnotation
        | Key::Block
        | Key::Button
        | Key::ClearCapturedOutput
        | Key::ClearScrollback
        | Key::CopyToClipboard
        | Key::CursorShape
        | Key::Custom
        | Key::Disinter
        | Key::EndCopy
        | Key::File
        | Key::FileEnd
        | Key::FilePart
        | Key::HighlightCursorLine
        | Key::MultipartFile
        | Key::OpenURL
        | Key::PopKeyLabels
        | Key::PushKeyLabels
        | Key::RemoteHost
        | Key::ReportCellSize
        | Key::ReportVariable
        | Key::RequestAttention
        | Key::RequestUpload
        | Key::SetBackgroundImageFile
        | Key::SetBadgeFormat
        | Key::SetColors
        | Key::SetKeyLabel
        | Key::SetMark
        | Key::SetProfile
        | Key::SetUserVar
        | Key::ShellIntegrationVersion
        | Key::StealFocus
        | Key::UnicodeVersion => {
            // log.debug("unimplemented OSC 1337: {t}", .{key});
            parser.command = Command::Invalid;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::osc::{Command, Parser};

    #[test]
    fn osc_1337_test_valid_unimplemented_key_with_no_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;SetBadgeFormat";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_valid_unimplemented_key_with_empty_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;SetBadgeFormat=";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_valid_unimplemented_key_with_non_empty_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;SetBadgeFormat=abc123";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_valid_key_with_lower_case_and_with_no_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;setbadgeformat";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_valid_key_with_lower_case_and_with_empty_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;setbadgeformat=";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_valid_key_with_lower_case_and_with_non_empty_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;setbadgeformat=abc123";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_invalid_key_with_no_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;BobrKurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_invalid_key_with_empty_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;BobrKurwa=";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_invalid_key_with_non_empty_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;BobrKurwa=abc123";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_copy_with_no_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;Copy";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_copy_with_empty_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;Copy=";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_copy_with_only_prefix_colon() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;Copy=:";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_copy_with_question_mark() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;Copy=:?";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    #[ignore = "return error.SkipZigTest"]
    fn osc_1337_test_copy_with_non_empty_value_that_is_invalid_base64() {
        // For performance reasons, we don't check for valid base64 data
        // right now.

        // const testing = std.testing;

        // var p: Parser = .init(testing.allocator);
        // defer p.deinit();

        // const input = "1337;Copy=:abc123";
        // for (input) |ch| p.next(ch);

        // try testing.expect(p.end('\x1b') == null);
    }

    #[test]
    fn osc_1337_test_copy_with_non_empty_value_that_is_valid_base64_but_not_prefixed_with_a_colon()
    {
        let mut p: Parser = Parser::init(None);

        let input = "1337;Copy=YWJjMTIz";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_copy_with_non_empty_value_that_is_valid_base64() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;Copy=:YWJjMTIz";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(b'\x1b')).unwrap().clone();
        assert!(matches!(cmd, Command::ClipboardContents { .. }));
        match cmd {
            Command::ClipboardContents { kind, data } => {
                assert_eq!(b'c', kind);
                assert_eq!(b"YWJjMTIz", data.as_slice());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_1337_test_current_dir_with_no_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;CurrentDir";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_current_dir_with_empty_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;CurrentDir=";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(b'\x1b')).is_none());
    }

    #[test]
    fn osc_1337_test_current_dir_with_non_empty_value() {
        let mut p: Parser = Parser::init(None);

        let input = "1337;CurrentDir=abc123";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(b'\x1b')).unwrap().clone();
        assert!(matches!(cmd, Command::ReportPwd { .. }));
        match cmd {
            Command::ReportPwd { value } => assert_eq!(b"abc123", value.as_slice()),
            _ => unreachable!(),
        }
    }
}
