//! PORTS: ghostty src/terminal/osc/parsers/osc9.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

// PORT NOTE: `const Parser = @import("../../osc.zig").Parser;` and
// `const Command = @import("../../osc.zig").Command;` — the OSC parent types.
use crate::osc::{
    Command, ConemuChangeTabTitle, Parser, ProgressReport, ProgressReportState, SemanticPrompt,
    State,
};

/// Parse OSC 9, which could be an iTerm2 notification or a ConEmu extension.
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut Command> {
    if parser.capture.is_none() {
        parser.state = State::Invalid;
        return None;
    }

    // Check first to see if this is a ConEmu OSC
    // https://conemu.github.io/en/AnsiEscapeCodes.html#ConEmu_specific_OSC
    'conemu: {
        let data = parser.capture.as_ref().unwrap().trailing();
        if data.is_empty() {
            break 'conemu;
        }
        match data[0] {
            // Check for OSC 9;1 9;10 9;11 9;12
            b'1' => {
                if data.len() < 2 {
                    break 'conemu;
                }
                match data[1] {
                    // OSC 9;1 sleep
                    b';' => {
                        parser.command = Command::ConemuSleep {
                            duration_ms: match std::str::from_utf8(&data[2..])
                                .ok()
                                .and_then(|s| s.parse::<u16>().ok())
                            {
                                Some(num) => num.min(10_000),
                                None => 100,
                            },
                        };
                        return Some(&mut parser.command);
                    }
                    // OSC 9;10 xterm keyboard and output emulation
                    b'0' => {
                        if data.len() == 2 {
                            parser.command = Command::ConemuXtermEmulation {
                                keyboard: Some(true),
                                output: Some(true),
                            };
                            return Some(&mut parser.command);
                        }
                        if data.len() < 4 {
                            break 'conemu;
                        }
                        if data[2] != b';' {
                            break 'conemu;
                        }
                        match data[3] {
                            b'0' => {
                                parser.command = Command::ConemuXtermEmulation {
                                    keyboard: Some(false),
                                    output: Some(false),
                                };
                                return Some(&mut parser.command);
                            }
                            b'1' => {
                                parser.command = Command::ConemuXtermEmulation {
                                    keyboard: Some(true),
                                    output: Some(true),
                                };
                                return Some(&mut parser.command);
                            }
                            b'2' => {
                                parser.command = Command::ConemuXtermEmulation {
                                    keyboard: None,
                                    output: Some(false),
                                };
                                return Some(&mut parser.command);
                            }
                            b'3' => {
                                parser.command = Command::ConemuXtermEmulation {
                                    keyboard: None,
                                    output: Some(true),
                                };
                                return Some(&mut parser.command);
                            }
                            _ => break 'conemu,
                        }
                    }
                    // OSC 9;11 comment
                    b'1' => {
                        if data.len() < 3 {
                            break 'conemu;
                        }
                        if data[2] != b';' {
                            break 'conemu;
                        }
                        if !parser.capture.as_mut().unwrap().write_byte(0) {
                            parser.state = State::Invalid;
                            return None;
                        }
                        let data = parser.capture.as_ref().unwrap().trailing();
                        parser.command = Command::ConemuComment(
                            // data[3 .. data.len - 1 :0]
                            // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
                            data[3..data.len() - 1].to_vec(),
                        );
                        return Some(&mut parser.command);
                    }
                    // OSC 9;12 mark prompt start
                    b'2' => {
                        parser.command = Command::SemanticPrompt(SemanticPrompt::init(
                            crate::osc::parsers::semantic_prompt::Action::FreshLineNewPrompt,
                        ));
                        return Some(&mut parser.command);
                    }
                    _ => break 'conemu,
                }
            }
            // OSC 9;2 show message box
            b'2' => {
                if data.len() < 2 {
                    break 'conemu;
                }
                if data[1] != b';' {
                    break 'conemu;
                }
                if !parser.capture.as_mut().unwrap().write_byte(0) {
                    parser.state = State::Invalid;
                    return None;
                }
                let data = parser.capture.as_ref().unwrap().trailing();
                parser.command = Command::ConemuShowMessageBox(
                    // data[2 .. data.len - 1 :0]
                    // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
                    data[2..data.len() - 1].to_vec(),
                );
                return Some(&mut parser.command);
            }
            // OSC 9;3 change tab title
            b'3' => {
                if data.len() < 2 {
                    break 'conemu;
                }
                if data[1] != b';' {
                    break 'conemu;
                }
                if data.len() == 2 {
                    parser.command = Command::ConemuChangeTabTitle(ConemuChangeTabTitle::Reset);
                    return Some(&mut parser.command);
                }
                if !parser.capture.as_mut().unwrap().write_byte(0) {
                    parser.state = State::Invalid;
                    return None;
                }
                let data = parser.capture.as_ref().unwrap().trailing();
                parser.command = Command::ConemuChangeTabTitle(ConemuChangeTabTitle::Value(
                    // data[2 .. data.len - 1 :0]
                    // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
                    data[2..data.len() - 1].to_vec(),
                ));
                return Some(&mut parser.command);
            }
            // OSC 9;4 progress report
            b'4' => {
                if data.len() < 2 {
                    break 'conemu;
                }
                if data[1] != b';' {
                    break 'conemu;
                }
                if data.len() < 3 {
                    break 'conemu;
                }
                match data[2] {
                    b'0' => {
                        parser.command = Command::ConemuProgressReport(ProgressReport {
                            state: ProgressReportState::Remove,
                            progress: None,
                        });
                    }
                    b'1' => {
                        parser.command = Command::ConemuProgressReport(ProgressReport {
                            state: ProgressReportState::Set,
                            progress: Some(0),
                        });
                    }
                    b'2' => {
                        parser.command = Command::ConemuProgressReport(ProgressReport {
                            state: ProgressReportState::Error,
                            progress: None,
                        });
                    }
                    b'3' => {
                        parser.command = Command::ConemuProgressReport(ProgressReport {
                            state: ProgressReportState::Indeterminate,
                            progress: None,
                        });
                    }
                    b'4' => {
                        parser.command = Command::ConemuProgressReport(ProgressReport {
                            state: ProgressReportState::Pause,
                            progress: None,
                        });
                    }
                    _ => break 'conemu,
                }
                // PORT NOTE: `switch (parser.command.conemu_progress_report.state)`
                // — destructure the report just set above to fill in `progress`.
                let report_state = match &parser.command {
                    Command::ConemuProgressReport(r) => r.state,
                    _ => unreachable!(),
                };
                match report_state {
                    ProgressReportState::Remove | ProgressReportState::Indeterminate => {}
                    ProgressReportState::Set
                    | ProgressReportState::Error
                    | ProgressReportState::Pause => 'progress: {
                        if data.len() < 4 {
                            break 'progress;
                        }
                        if data[3] != b';' {
                            break 'progress;
                        }
                        // parse the progress value
                        let value: Option<u8> = (|| {
                            let parsed = std::str::from_utf8(&data[4..])
                                .ok()
                                .and_then(|s| s.parse::<usize>().ok())?;
                            // @intCast(std.math.clamp(..., 0, 100))
                            Some(parsed.clamp(0, 100) as u8)
                        })();
                        if let Command::ConemuProgressReport(r) = &mut parser.command {
                            r.progress = value;
                        }
                    }
                }
                return Some(&mut parser.command);
            }
            // OSC 9;5 wait for input
            b'5' => {
                parser.command = Command::ConemuWaitInput;
                return Some(&mut parser.command);
            }
            // OSC 9;6 guimacro
            b'6' => {
                if data.len() < 2 {
                    break 'conemu;
                }
                if data[1] != b';' {
                    break 'conemu;
                }
                if !parser.capture.as_mut().unwrap().write_byte(0) {
                    parser.state = State::Invalid;
                    return None;
                }
                let data = parser.capture.as_ref().unwrap().trailing();
                parser.command = Command::ConemuGuimacro(
                    // data[2 .. data.len - 1 :0]
                    // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
                    data[2..data.len() - 1].to_vec(),
                );
                return Some(&mut parser.command);
            }
            // OSC 9;7 run process
            b'7' => {
                if data.len() < 2 {
                    break 'conemu;
                }
                if data[1] != b';' {
                    break 'conemu;
                }
                if !parser.capture.as_mut().unwrap().write_byte(0) {
                    parser.state = State::Invalid;
                    return None;
                }
                let data = parser.capture.as_ref().unwrap().trailing();
                parser.command = Command::ConemuRunProcess(
                    // data[2 .. data.len - 1 :0]
                    // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
                    data[2..data.len() - 1].to_vec(),
                );
                return Some(&mut parser.command);
            }
            // OSC 9;8 output environment variable
            b'8' => {
                if data.len() < 2 {
                    break 'conemu;
                }
                if data[1] != b';' {
                    break 'conemu;
                }
                if !parser.capture.as_mut().unwrap().write_byte(0) {
                    parser.state = State::Invalid;
                    return None;
                }
                let data = parser.capture.as_ref().unwrap().trailing();
                parser.command = Command::ConemuOutputEnvironmentVariable(
                    // data[2 .. data.len - 1 :0]
                    // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
                    data[2..data.len() - 1].to_vec(),
                );
                return Some(&mut parser.command);
            }
            // OSC 9;9 current working directory
            b'9' => {
                if data.len() < 2 {
                    break 'conemu;
                }
                if data[1] != b';' {
                    break 'conemu;
                }
                if !parser.capture.as_mut().unwrap().write_byte(0) {
                    parser.state = State::Invalid;
                    return None;
                }
                let data = parser.capture.as_ref().unwrap().trailing();
                parser.command = Command::ReportPwd {
                    // data[2 .. data.len - 1 :0]
                    // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
                    value: data[2..data.len() - 1].to_vec(),
                };
                return Some(&mut parser.command);
            }
            _ => break 'conemu,
        }
    }

    // If it's not a ConEmu OSC, it's an iTerm2 notification

    if !parser.capture.as_mut().unwrap().write_byte(0) {
        parser.state = State::Invalid;
        return None;
    }
    let data = parser.capture.as_ref().unwrap().trailing();
    parser.command = Command::ShowDesktopNotification {
        title: Vec::new(),
        // data[0 .. data.len - 1 :0]
        // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
        body: data[0..data.len() - 1].to_vec(),
    };
    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use crate::osc::{Command, ConemuChangeTabTitle, Parser, ProgressReportState};

    #[test]
    fn osc_9_show_desktop_notification() {
        let mut p: Parser = Parser::init(None);

        let input = "9;Hello world";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { title, body } => {
                assert_eq!(b"", title.as_slice());
                assert_eq!(b"Hello world", body.as_slice());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_show_single_character_desktop_notification() {
        let mut p: Parser = Parser::init(None);

        let input = "9;H";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { title, body } => {
                assert_eq!(b"", title.as_slice());
                assert_eq!(b"H", body.as_slice());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_1_conemu_sleep() {
        let mut p: Parser = Parser::init(None);

        let input = "9;1;420";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ConemuSleep { .. }));
        match cmd {
            Command::ConemuSleep { duration_ms } => assert_eq!(420, duration_ms),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_1_conemu_sleep_with_no_value_default_to_100ms() {
        let mut p: Parser = Parser::init(None);

        let input = "9;1;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ConemuSleep { .. }));
        match cmd {
            Command::ConemuSleep { duration_ms } => assert_eq!(100, duration_ms),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_1_conemu_sleep_cannot_exceed_10000ms() {
        let mut p: Parser = Parser::init(None);

        let input = "9;1;12345";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ConemuSleep { .. }));
        match cmd {
            Command::ConemuSleep { duration_ms } => assert_eq!(10000, duration_ms),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_1_conemu_sleep_invalid_input() {
        let mut p: Parser = Parser::init(None);

        let input = "9;1;foo";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ConemuSleep { .. }));
        match cmd {
            Command::ConemuSleep { duration_ms } => assert_eq!(100, duration_ms),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_1_conemu_sleep_to_desktop_notification_1() {
        let mut p: Parser = Parser::init(None);

        let input = "9;1";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"1", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_1_conemu_sleep_to_desktop_notification_2() {
        let mut p: Parser = Parser::init(None);

        let input = "9;1a";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"1a", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_2_conemu_message_box() {
        let mut p: Parser = Parser::init(None);

        let input = "9;2;hello world";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuShowMessageBox(_)));
        match cmd {
            Command::ConemuShowMessageBox(s) => assert_eq!(b"hello world", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_2_conemu_message_box_invalid_input() {
        let mut p: Parser = Parser::init(None);

        let input = "9;2";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"2", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_2_conemu_message_box_empty_message() {
        let mut p: Parser = Parser::init(None);

        let input = "9;2;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuShowMessageBox(_)));
        match cmd {
            Command::ConemuShowMessageBox(s) => assert_eq!(b"", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_2_conemu_message_box_spaces_only_message() {
        let mut p: Parser = Parser::init(None);

        let input = "9;2;   ";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuShowMessageBox(_)));
        match cmd {
            Command::ConemuShowMessageBox(s) => assert_eq!(b"   ", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_2_message_box_to_desktop_notification_1() {
        let mut p: Parser = Parser::init(None);

        let input = "9;2";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"2", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_2_message_box_to_desktop_notification_2() {
        let mut p: Parser = Parser::init(None);

        let input = "9;2a";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"2a", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_3_conemu_change_tab_title() {
        let mut p: Parser = Parser::init(None);

        let input = "9;3;foo bar";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuChangeTabTitle(_)));
        match cmd {
            Command::ConemuChangeTabTitle(ConemuChangeTabTitle::Value(v)) => {
                assert_eq!(b"foo bar", v.as_slice())
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_3_conemu_change_tab_title_reset() {
        let mut p: Parser = Parser::init(None);

        let input = "9;3;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        let expected_command: Command = Command::ConemuChangeTabTitle(ConemuChangeTabTitle::Reset);
        assert_eq!(expected_command, cmd);
    }

    #[test]
    fn osc_9_3_conemu_change_tab_title_spaces_only() {
        let mut p: Parser = Parser::init(None);

        let input = "9;3;   ";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ConemuChangeTabTitle(_)));
        match cmd {
            Command::ConemuChangeTabTitle(ConemuChangeTabTitle::Value(v)) => {
                assert_eq!(b"   ", v.as_slice())
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_3_change_tab_title_to_desktop_notification_1() {
        let mut p: Parser = Parser::init(None);

        let input = "9;3";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"3", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_3_message_box_to_desktop_notification_2() {
        let mut p: Parser = Parser::init(None);

        let input = "9;3a";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"3a", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_set() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;1;100";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Set);
                assert!(r.progress == Some(100));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_set_overflow() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;1;900";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Set);
                assert_eq!(Some(100), r.progress);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_set_single_digit() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;1;9";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Set);
                assert!(r.progress == Some(9));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_set_double_digit() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;1;94";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Set);
                assert_eq!(Some(94), r.progress);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_set_extra_semicolon_ignored() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;1;100";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Set);
                assert_eq!(Some(100), r.progress);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_remove_with_no_progress() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;0;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Remove);
                assert!(r.progress.is_none());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_remove_with_double_semicolon() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;0;;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Remove);
                assert!(r.progress.is_none());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_remove_ignores_progress() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;0;100";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Remove);
                assert!(r.progress.is_none());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_remove_extra_semicolon() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;0;100;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Remove);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_error() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;2";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Error);
                assert!(r.progress.is_none());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_error_with_progress() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;2;100";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Error);
                assert!(r.progress == Some(100));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_progress_pause() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;4";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Pause);
                assert!(r.progress.is_none());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_conemu_progress_pause_with_progress() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;4;100";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuProgressReport(_)));
        match cmd {
            Command::ConemuProgressReport(r) => {
                assert!(r.state == ProgressReportState::Pause);
                assert!(r.progress == Some(100));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_progress_to_desktop_notification_1() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"4", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_progress_to_desktop_notification_2() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"4;", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_progress_to_desktop_notification_3() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;5";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"4;5", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_4_progress_to_desktop_notification_4() {
        let mut p: Parser = Parser::init(None);

        let input = "9;4;5a";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"4;5a", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_5_conemu_wait_input() {
        let mut p: Parser = Parser::init(None);

        let input = "9;5";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuWaitInput));
    }

    #[test]
    fn osc_9_5_conemu_wait_ignores_trailing_characters() {
        let mut p: Parser = Parser::init(None);

        let input = "9;5;foo";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuWaitInput));
    }

    #[test]
    fn osc_9_6_conemu_guimacro_1() {
        // PORT NOTE: `var p: Parser = .init(testing.allocator); defer p.deinit();`
        // — these ConEmu string-payload OSCs init the parser with an allocator in
        // the Zig tests. The Rustty `Parser::init(None)` still routes through the
        // fixed-buffer capture (sufficient for these small inputs), matching the
        // allocator-absent fallback in `capture_trailing`.
        let mut p: Parser = Parser::init(None);

        let input = "9;6;a";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuGuimacro(_)));
        match cmd {
            Command::ConemuGuimacro(s) => assert_eq!(b"a", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_6_conemu_guimacro_2() {
        let mut p: Parser = Parser::init(None);

        let input = "9;6;ab";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuGuimacro(_)));
        match cmd {
            Command::ConemuGuimacro(s) => assert_eq!(b"ab", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_6_conemu_guimacro_3_incomplete_to_desktop_notification() {
        let mut p: Parser = Parser::init(None);

        let input = "9;6";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"6", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_7_conemu_run_process_1() {
        let mut p: Parser = Parser::init(None);

        let input = "9;7;ab";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuRunProcess(_)));
        match cmd {
            Command::ConemuRunProcess(s) => assert_eq!(b"ab", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_7_conemu_run_process_2() {
        let mut p: Parser = Parser::init(None);

        let input = "9;7;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuRunProcess(_)));
        match cmd {
            Command::ConemuRunProcess(s) => assert_eq!(b"", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_7_conemu_run_process_incomplete_to_desktop_notification() {
        let mut p: Parser = Parser::init(None);

        let input = "9;7";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"7", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_8_conemu_output_environment_variable_1() {
        let mut p: Parser = Parser::init(None);

        let input = "9;8;ab";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuOutputEnvironmentVariable(_)));
        match cmd {
            Command::ConemuOutputEnvironmentVariable(s) => assert_eq!(b"ab", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_8_conemu_output_environment_variable_2() {
        let mut p: Parser = Parser::init(None);

        let input = "9;8;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuOutputEnvironmentVariable(_)));
        match cmd {
            Command::ConemuOutputEnvironmentVariable(s) => assert_eq!(b"", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_8_conemu_output_environment_variable_incomplete_to_desktop_notification() {
        let mut p: Parser = Parser::init(None);

        let input = "9;8";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"8", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_9_conemu_set_current_working_directory() {
        let mut p: Parser = Parser::init(None);

        let input = "9;9;ab";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ReportPwd { .. }));
        match cmd {
            Command::ReportPwd { value } => assert_eq!(b"ab", value.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_9_conemu_set_current_working_directory_incomplete_to_desktop_notification() {
        let mut p: Parser = Parser::init(None);

        let input = "9;9";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"9", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_10_conemu_xterm_keyboard_and_output_emulation_1() {
        let mut p: Parser = Parser::init(None);

        let input = "9;10";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuXtermEmulation { .. }));
        match cmd {
            Command::ConemuXtermEmulation { keyboard, output } => {
                assert!(keyboard.is_some());
                assert!(keyboard.unwrap());
                assert!(output.is_some());
                assert!(output.unwrap());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_10_conemu_xterm_keyboard_and_output_emulation_2() {
        let mut p: Parser = Parser::init(None);

        let input = "9;10;0";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuXtermEmulation { .. }));
        match cmd {
            Command::ConemuXtermEmulation { keyboard, output } => {
                assert!(keyboard.is_some());
                assert!(!keyboard.unwrap());
                assert!(output.is_some());
                assert!(!output.unwrap());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_10_conemu_xterm_keyboard_and_output_emulation_3() {
        let mut p: Parser = Parser::init(None);

        let input = "9;10;1";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuXtermEmulation { .. }));
        match cmd {
            Command::ConemuXtermEmulation { keyboard, output } => {
                assert!(keyboard.is_some());
                assert!(keyboard.unwrap());
                assert!(output.is_some());
                assert!(output.unwrap());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_10_conemu_xterm_keyboard_and_output_emulation_4() {
        let mut p: Parser = Parser::init(None);

        let input = "9;10;2";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuXtermEmulation { .. }));
        match cmd {
            Command::ConemuXtermEmulation { keyboard, output } => {
                assert!(keyboard.is_none());
                assert!(output.is_some());
                assert!(!output.unwrap());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_10_conemu_xterm_keyboard_and_output_emulation_5() {
        let mut p: Parser = Parser::init(None);

        let input = "9;10;3";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuXtermEmulation { .. }));
        match cmd {
            Command::ConemuXtermEmulation { keyboard, output } => {
                assert!(keyboard.is_none());
                assert!(output.is_some());
                assert!(output.unwrap());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_10_conemu_xterm_keyboard_and_output_emulation_6() {
        let mut p: Parser = Parser::init(None);

        let input = "9;10;4";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"10;4", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_10_conemu_xterm_keyboard_and_output_emulation_7() {
        let mut p: Parser = Parser::init(None);

        let input = "9;10;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"10;", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_10_conemu_xterm_keyboard_and_output_emulation_8() {
        let mut p: Parser = Parser::init(None);

        let input = "9;10;abc";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"10;abc", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_11_conemu_comment() {
        let mut p: Parser = Parser::init(None);

        let input = "9;11;ab";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ConemuComment(_)));
        match cmd {
            Command::ConemuComment(s) => assert_eq!(b"ab", s.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_11_conemu_comment_incomplete_to_desktop_notification() {
        let mut p: Parser = Parser::init(None);

        let input = "9;11";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::ShowDesktopNotification { .. }));
        match cmd {
            Command::ShowDesktopNotification { body, .. } => assert_eq!(b"11", body.as_slice()),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_9_12_conemu_mark_prompt_start_1() {
        let mut p: Parser = Parser::init(None);

        let input = "9;12";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::SemanticPrompt(_)));
    }

    #[test]
    fn osc_9_12_conemu_mark_prompt_start_2() {
        let mut p: Parser = Parser::init(None);

        let input = "9;12;abc";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::SemanticPrompt(_)));
    }
}
