//! stream_terminal tests: the response/query effects (DECRQM, desktop
//! notification, progress report, clipboard write, window title, XTVERSION,
//! size/device status/device attributes reports) and Kitty graphics.

use super::*;

// test "request mode DECRQM with write_pty callback"
#[test]
fn request_mode_decrqm_with_write_pty_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // Without callback, DECRQM should not crash
    {
        let mut s = t.vt_stream();
        // DECRQM for mode 7 (wraparound) — should be silently ignored
        s.next_slice(b"\x1B[?7$p");
        s.next_slice(b"\x1B[4$p");
    }

    t.full_reset();

    // With callback, DECRQM should produce a response
    {
        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        // Wraparound mode (7) is set by default
        s.next_slice(b"\x1B[?7$p");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?7;1$y");

        // Disable wraparound and query again
        s.next_slice(b"\x1B[?7l");
        s.next_slice(b"\x1B[?7$p");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?7;2$y");

        // A large unknown mode must not alias wraparound mode 7.
        let before = s.handler.term().modes;
        s.next_slice(b"\x1B[?32775$p");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?32775;0$y");
        assert_eq!(before, s.handler.term().modes);

        // Query an unknown mode
        s.next_slice(b"\x1B[?9999$p");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?9999;0$y");

        // Query DECECM, which Rustty recognizes but does not allow changing
        s.next_slice(b"\x1B[?117$p");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?117;4$y");
    }

    t.deinit(Global);
}

// Capture cells for "request mode DECRQM ANSI responses" (oracle
// 9dc0d974e): the reference's `S { var response: [32]u8; var len; var
// calls }`, as thread-locals so the parallel runner never aliases them.
thread_local! {
    static ANSI_RESPONSE: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static ANSI_CALLS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

fn ansi_write_pty(_: &mut Handler, data: &[u8]) {
    ANSI_RESPONSE.with(|c| *c.borrow_mut() = data.to_vec());
    ANSI_CALLS.with(|c| c.set(c.get() + 1));
}

fn ansi_reset() {
    ANSI_CALLS.with(|c| c.set(0));
    ANSI_RESPONSE.with(|c| c.borrow_mut().clear());
}

fn ansi_calls() -> usize {
    ANSI_CALLS.with(|c| c.get())
}

fn ansi_response() -> Vec<u8> {
    ANSI_RESPONSE.with(|c| c.borrow().clone())
}

// test "request mode DECRQM ANSI responses" (oracle 9dc0d974e)
#[test]
fn request_mode_decrqm_ansi_responses() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(ansi_write_pty);

        // PORT NOTE: the reference's two nested `inline for`s over
        // comptime tuples become plain loops; the query and the expected
        // response are formatted at run time instead of `++`-concatenated.
        for (num, mode) in [
            ("2", modes::Mode::DisableKeyboard),
            ("4", modes::Mode::Insert),
            ("12", modes::Mode::SendReceiveMode),
            ("20", modes::Mode::Linefeed),
        ] {
            for enabled in [false, true, false] {
                s.next_slice(format!("\x1b[{num}{}", if enabled { "h" } else { "l" }).as_bytes());
                assert_eq!(enabled, s.handler.term().modes.get(mode));
                let query = format!("\x1b[{num}$p");
                let query = query.as_bytes();
                for split in 0..query.len() + 1 {
                    ansi_reset();
                    s.next_slice(&query[..split]);
                    if split < query.len() {
                        assert_eq!(0, ansi_calls());
                    }
                    s.next_slice(&query[split..]);
                    assert_eq!(1, ansi_calls());
                    assert_eq!(
                        format!("\x1b[{num}{}", if enabled { ";1$y" } else { ";2$y" }).as_bytes(),
                        ansi_response().as_slice()
                    );
                    assert_eq!(enabled, s.handler.term().modes.get(mode));
                }
            }
        }

        // The two namespaces must report independent states for mode 4.
        s.next_slice(b"\x1b[4h\x1b[?4l");
        let cases: [(&[u8], &[u8]); 3] = [
            (b"\x1b[4$p", b"\x1b[4;1$y"),
            (b"\x1b[?4$p", b"\x1b[?4;2$y"),
            (b"\x1b[9999$p", b"\x1b[9999;0$y"),
        ];
        for (query, expected) in cases {
            ansi_reset();
            s.next_slice(query);
            assert_eq!(1, ansi_calls());
            assert_eq!(expected, ansi_response().as_slice());
        }
    }
    t.deinit(Global);
}

// Capture cells for the desktop_notification effect test (oracle c3655ba25).
// Same `S`-struct-via-thread-local shape the clipboard test below uses.
//
// PER THE BYTE-SLICE RULE the captured title/body are `Vec<u8>`, never
// `String`: OSC 9 / OSC 777 payloads are raw bytes from the pty and a
// lossy UTF-8 conversion here would make the test unable to see a
// non-UTF-8 title at all.
thread_local! {
    static DN_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    static DN_TITLE: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static DN_BODY: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

fn cap_desktop_notification(_: &mut Handler, n: stream::ShowDesktopNotification) {
    DN_COUNT.with(|c| c.set(c.get() + 1));
    DN_TITLE.with(|c| *c.borrow_mut() = n.title.to_vec());
    DN_BODY.with(|c| *c.borrow_mut() = n.body.to_vec());
}

// test "desktop_notification effect callback" (oracle c3655ba25)
#[test]
fn desktop_notification_effect_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // A null callback (the default readonly effects) silently ignores
    // notifications and leaves the terminal usable.
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1B]9;Ignored\x1B\\AfterNotification");
    }
    let str_ = t.plain_string(Global).unwrap();
    assert_eq!(b"AfterNotification".as_slice(), str_.as_slice());

    t.full_reset();

    DN_COUNT.with(|c| c.set(0));
    DN_TITLE.with(|c| c.borrow_mut().clear());
    DN_BODY.with(|c| c.borrow_mut().clear());

    let mut s = t.vt_stream();
    s.handler.effects.desktop_notification = Some(cap_desktop_notification);

    // OSC 9 is split across writes and carries only a body.
    s.next_slice(b"\x1B]9;Build ");
    assert_eq!(0usize, DN_COUNT.with(|c| c.get()));
    s.next_slice(b"complete\x1B\\");
    assert_eq!(1usize, DN_COUNT.with(|c| c.get()));
    assert_eq!(
        b"".as_slice(),
        DN_TITLE.with(|c| c.borrow().clone()).as_slice()
    );
    assert_eq!(
        b"Build complete".as_slice(),
        DN_BODY.with(|c| c.borrow().clone()).as_slice()
    );

    // OSC 777 preserves its separate title and body fields.
    s.next_slice(b"\x1B]777;notify;Codex;Needs attention\x07");
    assert_eq!(2usize, DN_COUNT.with(|c| c.get()));
    assert_eq!(
        b"Codex".as_slice(),
        DN_TITLE.with(|c| c.borrow().clone()).as_slice()
    );
    assert_eq!(
        b"Needs attention".as_slice(),
        DN_BODY.with(|c| c.borrow().clone()).as_slice()
    );

    t.deinit(Global);
}

// Capture cells for the progress_report effect test (oracle 47d602c42).
thread_local! {
    static PR_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    static PR_STATE: core::cell::Cell<osc::ProgressReportState> =
        const { core::cell::Cell::new(osc::ProgressReportState::Remove) };
    static PR_PROGRESS: core::cell::Cell<Option<u8>> = const { core::cell::Cell::new(None) };
}

fn cap_progress_report(_: &mut Handler, r: osc::ProgressReport) {
    PR_COUNT.with(|c| c.set(c.get() + 1));
    PR_STATE.with(|c| c.set(r.state));
    PR_PROGRESS.with(|c| c.set(r.progress));
}

// test "progress_report effect callback" (oracle 47d602c42)
#[test]
fn progress_report_effect_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // A null callback (the default readonly effects) silently ignores reports.
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1B]9;4;1;25\x1B\\");
    }

    PR_COUNT.with(|c| c.set(0));
    PR_STATE.with(|c| c.set(osc::ProgressReportState::Remove));
    PR_PROGRESS.with(|c| c.set(None));

    let mut s = t.vt_stream();
    s.handler.effects.progress_report = Some(cap_progress_report);

    let cases: &[(&[u8], osc::ProgressReportState, Option<u8>)] = &[
        (b"\x1B]9;4;0;\x1B\\", osc::ProgressReportState::Remove, None),
        (
            b"\x1B]9;4;1;42\x07",
            osc::ProgressReportState::Set,
            Some(42),
        ),
        (
            b"\x1B]9;4;2;7\x1B\\",
            osc::ProgressReportState::Error,
            Some(7),
        ),
        (
            b"\x1B]9;4;3\x1B\\",
            osc::ProgressReportState::Indeterminate,
            None,
        ),
        (
            b"\x1B]9;4;4;75\x1B\\",
            osc::ProgressReportState::Pause,
            Some(75),
        ),
    ];

    for (i, (sequence, state, progress)) in cases.iter().enumerate() {
        let expected_count = i + 1;
        // Split each sequence to verify parsing survives PTY read boundaries.
        let midpoint = sequence.len() / 2;
        s.next_slice(&sequence[0..midpoint]);
        assert_eq!(expected_count - 1, PR_COUNT.with(|c| c.get()));
        s.next_slice(&sequence[midpoint..]);
        assert_eq!(expected_count, PR_COUNT.with(|c| c.get()));
        assert_eq!(*state, PR_STATE.with(|c| c.get()));
        assert_eq!(*progress, PR_PROGRESS.with(|c| c.get()));
    }

    // A full reset (RIS) removes any active progress bar.
    // s.nextSlice("\x1B]9;4;1;50\x1B\\");
    s.next_slice(b"\x1B]9;4;1;50\x1B\\");
    // try testing.expectEqual(@as(usize, cases.len + 1), S.count);
    assert_eq!(cases.len() + 1, PR_COUNT.with(|c| c.get()));
    // try testing.expectEqual(osc.Command.ProgressReport.State.set, S.last_state);
    assert_eq!(osc::ProgressReportState::Set, PR_STATE.with(|c| c.get()));
    // s.nextSlice("\x1Bc");
    s.next_slice(b"\x1Bc");
    // try testing.expectEqual(@as(usize, cases.len + 2), S.count);
    assert_eq!(cases.len() + 2, PR_COUNT.with(|c| c.get()));
    // try testing.expectEqual(osc.Command.ProgressReport.State.remove, S.last_state);
    assert_eq!(osc::ProgressReportState::Remove, PR_STATE.with(|c| c.get()));
    // try testing.expectEqual(@as(?u8, null), S.last_progress);
    assert_eq!(None::<u8>, PR_PROGRESS.with(|c| c.get()));

    t.deinit(Global);
}

// Capture cells for the program_status effect test (the reference's `S`).
thread_local! {
    static PS_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    static PS_LAST_STATE: core::cell::Cell<Option<osc::parsers::program_status::State>> =
        const { core::cell::Cell::new(None) };
    static PS_LAST_ID: core::cell::RefCell<Option<Vec<u8>>> =
        const { core::cell::RefCell::new(None) };
    static PS_LAST_MESSAGE: core::cell::RefCell<Option<Vec<u8>>> =
        const { core::cell::RefCell::new(None) };
    static PS_WRITTEN: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

// fn reset() void { ... }
fn ps_reset() {
    PS_COUNT.with(|c| c.set(0));
    PS_LAST_STATE.with(|c| c.set(None));
    PS_LAST_ID.with(|c| *c.borrow_mut() = None);
    PS_LAST_MESSAGE.with(|c| *c.borrow_mut() = None);
    PS_WRITTEN.with(|c| c.borrow_mut().clear());
}

// fn programStatus(_: *Handler, report: osc.Command.ProgramStatus.Report) void { ... }
fn ps_program_status(_: &mut Handler, report: osc::parsers::program_status::Report) {
    use osc::parsers::program_status::{OptionKey, OptionValue};
    PS_COUNT.with(|c| c.set(c.get() + 1));
    PS_LAST_STATE.with(|c| c.set(Some(report.state)));
    // last_id_len = if (report.readOption(.id)) |v| copy(&last_id, v) else null;
    let OptionValue::Str(id) = report.read_option(OptionKey::Id) else {
        unreachable!("id reads as a string")
    };
    PS_LAST_ID.with(|c| *c.borrow_mut() = id.map(<[u8]>::to_vec));
    // var message: std.Io.Writer = .fixed(&last_message);
    // report.writeText(.msg, &message) catch unreachable;
    // last_message_len = message.buffered().len;
    let mut message = Vec::new();
    report
        .write_text(OptionKey::Msg, &mut message)
        .expect("unreachable");
    PS_LAST_MESSAGE.with(|c| *c.borrow_mut() = Some(message));
}

// fn writePty(_: *Handler, data: []const u8) void { ... }
fn ps_write_pty(_: &mut Handler, data: &[u8]) {
    PS_WRITTEN.with(|c| c.borrow_mut().extend_from_slice(data));
}

fn ps_written() -> Vec<u8> {
    PS_WRITTEN.with(|c| c.borrow().clone())
}

fn ps_count() -> usize {
    PS_COUNT.with(|c| c.get())
}

// test "program_status effect callback"
#[test]
fn program_status_effect_callback() {
    use osc::parsers::program_status::State;
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // Without a callback, reports are ignored and the query isn't
    // answered so the program sees the protocol as unsupported.
    {
        ps_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(ps_write_pty);
        s.next_slice(b"\x1B]7501;?\x1B\\\x1B]7501;state=idle\x1B\\");
        assert_eq!(0, ps_written().len());
        s.deinit();
    }

    ps_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(ps_write_pty);
    s.handler.effects.program_status = Some(ps_program_status);

    // The query is answered with the same body and terminator.
    s.next_slice(b"\x1B]7501;?\x1B\\");
    assert_eq!(b"\x1B]7501;?\x1B\\".as_slice(), ps_written().as_slice());
    PS_WRITTEN.with(|c| c.borrow_mut().clear());
    s.next_slice(b"\x1B]7501;?\x07");
    assert_eq!(b"\x1B]7501;?\x07".as_slice(), ps_written().as_slice());
    assert_eq!(0, ps_count());
    PS_WRITTEN.with(|c| c.borrow_mut().clear());

    // A report split across writes. "Syncing photos"
    s.next_slice(b"\x1B]7501;state=working:id=sync:msg=U3lu");
    assert_eq!(0, ps_count());
    s.next_slice(b"Y2luZyBwaG90b3M=\x1B\\");
    assert_eq!(1, ps_count());
    assert_eq!(Some(State::Working), PS_LAST_STATE.with(|c| c.get()));
    assert_eq!(
        Some(b"sync".to_vec()),
        PS_LAST_ID.with(|c| c.borrow().clone())
    );
    assert_eq!(
        Some(b"Syncing photos".to_vec()),
        PS_LAST_MESSAGE.with(|c| c.borrow().clone())
    );

    // A full reset clears every record.
    s.next_slice(b"\x1Bc");
    assert_eq!(2, ps_count());
    assert_eq!(Some(State::Clear), PS_LAST_STATE.with(|c| c.get()));
    assert_eq!(None, PS_LAST_ID.with(|c| c.borrow().clone()));
    // try testing.expectEqual(@as(?usize, 0), S.last_message_len);
    assert_eq!(
        Some(0),
        PS_LAST_MESSAGE.with(|c| c.borrow().as_ref().map(Vec::len))
    );

    // Nothing from the reports was echoed back.
    assert_eq!(0, ps_written().len());

    s.deinit();
    drop(s);
    t.deinit(Global);
}

// Capture cells for the semantic_prompt effect test (the reference's `S`).
// `last` keeps the event's scalars; the borrowed strings are copied into
// `SP_COMMAND` / `SP_ERR` ("The strings are borrowed, so copy them.").
thread_local! {
    static SP_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    static SP_LAST: core::cell::Cell<(SemanticPromptKind, SemanticPromptPromptKind, Option<i32>)> =
        const {
            core::cell::Cell::new((
                SemanticPromptKind::Invalid,
                SemanticPromptPromptKind::Primary,
                None,
            ))
        };
    static SP_LAST_CURSOR_X: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    static SP_COMMAND: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static SP_ERR: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

// fn semanticPrompt(handler: *Handler, event: Handler.SemanticPrompt) void { ... }
fn sp_semantic_prompt(handler: &mut Handler, event: SemanticPrompt) {
    SP_COUNT.with(|c| c.set(c.get() + 1));
    SP_LAST.with(|c| c.set((event.kind, event.prompt_kind, event.exit_code)));
    // last_cursor_x = handler.terminal.screens.active.cursor.x;
    // SAFETY: the callback reaches the terminal only through the handler's
    // raw alias; nothing else borrows it during the callback.
    let x = unsafe { (*(*handler.terminal).screens.active).cursor.x };
    SP_LAST_CURSOR_X.with(|c| c.set(usize::from(x)));

    // The strings are borrowed, so copy them.
    SP_COMMAND.with(|c| *c.borrow_mut() = event.command.to_vec());
    SP_ERR.with(|c| *c.borrow_mut() = event.err.to_vec());
}

fn sp_count() -> usize {
    SP_COUNT.with(|c| c.get())
}
fn sp_kind() -> SemanticPromptKind {
    SP_LAST.with(|c| c.get().0)
}
fn sp_prompt_kind() -> SemanticPromptPromptKind {
    SP_LAST.with(|c| c.get().1)
}
fn sp_exit_code() -> Option<i32> {
    SP_LAST.with(|c| c.get().2)
}
fn sp_command() -> Vec<u8> {
    SP_COMMAND.with(|c| c.borrow().clone())
}
fn sp_err() -> Vec<u8> {
    SP_ERR.with(|c| c.borrow().clone())
}

// test "semantic_prompt effect callback"
#[test]
fn semantic_prompt_effect_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // A null callback (the default readonly effects) silently ignores events.
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1B]133;C;cmdline_url=ls\x1B\\");
        s.deinit();
    }

    // S.count = 0;
    SP_COUNT.with(|c| c.set(0));

    let mut s = t.vt_stream();
    s.handler.effects.semantic_prompt = Some(sp_semantic_prompt);

    // The effect fires after the terminal applied the fresh line.
    s.next_slice(b"abc");
    s.next_slice(b"\x1B]133;A\x07");
    assert_eq!(1, sp_count());
    assert_eq!(SemanticPromptKind::PromptStart, sp_kind());
    assert_eq!(SemanticPromptPromptKind::Primary, sp_prompt_kind());
    assert_eq!(0, SP_LAST_CURSOR_X.with(|c| c.get()));

    s.next_slice(b"\x1B]133;P;k=r\x07");
    assert_eq!(2, sp_count());
    assert_eq!(SemanticPromptKind::PromptStart, sp_kind());
    assert_eq!(SemanticPromptPromptKind::Right, sp_prompt_kind());

    s.next_slice(b"\x1B]133;P;k=c\x07");
    assert_eq!(3, sp_count());
    assert_eq!(SemanticPromptPromptKind::Continuation, sp_prompt_kind());

    s.next_slice(b"\x1B]133;P;k=s\x07");
    assert_eq!(4, sp_count());
    assert_eq!(SemanticPromptPromptKind::Secondary, sp_prompt_kind());

    // An unknown prompt kind is primary.
    s.next_slice(b"\x1B]133;P;k=x\x07");
    assert_eq!(5, sp_count());
    assert_eq!(SemanticPromptPromptKind::Primary, sp_prompt_kind());

    s.next_slice(b"\x1B]133;N\x07");
    assert_eq!(6, sp_count());
    assert_eq!(SemanticPromptKind::PromptStart, sp_kind());
    assert_eq!(SemanticPromptPromptKind::Primary, sp_prompt_kind());

    s.next_slice(b"\x1B]133;B\x07");
    assert_eq!(7, sp_count());
    assert_eq!(SemanticPromptKind::InputStart, sp_kind());

    s.next_slice(b"\x1B]133;I\x07");
    assert_eq!(8, sp_count());
    assert_eq!(SemanticPromptKind::InputStart, sp_kind());

    s.next_slice(b"\x1B]133;C;cmdline_url=ls%20-la\x07");
    assert_eq!(9, sp_count());
    assert_eq!(SemanticPromptKind::OutputStart, sp_kind());
    assert_eq!(b"ls -la".as_slice(), sp_command().as_slice());

    s.next_slice(b"\x1B]133;C;cmdline='echo hi'\x07");
    assert_eq!(10, sp_count());
    assert_eq!(b"echo hi".as_slice(), sp_command().as_slice());

    // No command line, and an undecodable one, are both empty.
    s.next_slice(b"\x1B]133;C\x07");
    assert_eq!(11, sp_count());
    assert_eq!(SemanticPromptKind::OutputStart, sp_kind());
    assert_eq!(b"".as_slice(), sp_command().as_slice());

    s.next_slice(b"\x1B]133;C;cmdline='bad\x07");
    assert_eq!(12, sp_count());
    assert_eq!(b"".as_slice(), sp_command().as_slice());

    s.next_slice(b"\x1B]133;D;1\x07");
    assert_eq!(13, sp_count());
    assert_eq!(SemanticPromptKind::CommandEnd, sp_kind());
    assert_eq!(Some(1), sp_exit_code());
    assert_eq!(b"".as_slice(), sp_err().as_slice());

    s.next_slice(b"\x1B]133;D\x07");
    assert_eq!(14, sp_count());
    assert_eq!(SemanticPromptKind::CommandEnd, sp_kind());
    assert_eq!(None, sp_exit_code());

    s.next_slice(b"\x1B]133;D;-2;err=boom\x07");
    assert_eq!(15, sp_count());
    assert_eq!(Some(-2), sp_exit_code());
    assert_eq!(b"boom".as_slice(), sp_err().as_slice());

    // Fresh line alone is layout, not lifecycle.
    s.next_slice(b"\x1B]133;L\x07");
    assert_eq!(15, sp_count());

    // Sequences the parser rejects report nothing.
    s.next_slice(b"\x1B]133;Lx\x07");
    s.next_slice(b"\x1B]133;Z\x07");
    assert_eq!(15, sp_count());

    s.deinit();
    drop(s);
    t.deinit(Global);
}

// Capture cells for the reset effect test (the reference's `S`).
thread_local! {
    static RESET_EVENTS: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static RESET_CURSOR_X: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

// fn progressReport(_: *Handler, _: osc.Command.ProgressReport) void { ... }
fn reset_progress_report(_: &mut Handler, _: osc::ProgressReport) {
    RESET_EVENTS.with(|c| c.borrow_mut().push(b'p'));
}

// fn reset(handler: *Handler) void { ... }
fn reset_reset(handler: &mut Handler) {
    // reset_cursor_x = handler.terminal.screens.active.cursor.x;
    // SAFETY: the callback reaches the terminal only through the handler's
    // raw alias; nothing else borrows it during the callback.
    let x = unsafe { (*(*handler.terminal).screens.active).cursor.x };
    RESET_CURSOR_X.with(|c| c.set(usize::from(x)));
    RESET_EVENTS.with(|c| c.borrow_mut().push(b'r'));
}

// test "reset effect callback"
#[test]
fn reset_effect_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // S.len = 0;
    // S.reset_cursor_x = 0;
    RESET_EVENTS.with(|c| c.borrow_mut().clear());
    RESET_CURSOR_X.with(|c| c.set(0));

    let mut s = t.vt_stream();
    s.handler.effects.progress_report = Some(reset_progress_report);
    s.handler.effects.reset = Some(reset_reset);

    // A soft reset (DECSTR) doesn't report a reset.
    s.next_slice(b"\x1B[!p");
    assert_eq!(
        b"".as_slice(),
        RESET_EVENTS.with(|c| c.borrow().clone()).as_slice()
    );

    // A full reset reports the progress removal first, and the terminal
    // already reset itself when the reset is reported.
    s.next_slice(b"abc\x1Bc");
    assert_eq!(
        b"pr".as_slice(),
        RESET_EVENTS.with(|c| c.borrow().clone()).as_slice()
    );
    assert_eq!(0, RESET_CURSOR_X.with(|c| c.get()));

    s.deinit();
    drop(s);
    t.deinit(Global);
}

// Capture cells for the clipboard_write effect test (the analog of the
// reference's per-test `S` struct; bare `fn` callbacks route through
// thread-locals since Rust fn pointers cannot capture).
thread_local! {
    static CLIP_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    static CLIP_RESULT: core::cell::Cell<clipboard::WriteResult> =
        const { core::cell::Cell::new(clipboard::WriteResult::Success(
            clipboard::WriteResultSuccess { remember: false },
        )) };
    static CLIP_LOCATION: core::cell::Cell<clipboard::Location> =
        const { core::cell::Cell::new(clipboard::Location::Standard) };
    static CLIP_CONTENTS_LEN: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    static CLIP_MIME: core::cell::RefCell<Option<Vec<u8>>> = const { core::cell::RefCell::new(None) };
    static CLIP_DATA: core::cell::RefCell<Option<Vec<u8>>> = const { core::cell::RefCell::new(None) };
}

fn clip_clear_capture() {
    CLIP_MIME.with(|c| *c.borrow_mut() = None);
    CLIP_DATA.with(|c| *c.borrow_mut() = None);
    CLIP_CONTENTS_LEN.with(|c| c.set(0));
}

// fn clipboardWrite(_: *Handler, write: clipboard.Write) void { ...
//     write.reply(result); }
fn cap_clipboard_write(handler: &mut Handler, write: clipboard::Write) {
    clip_clear_capture();
    CLIP_COUNT.with(|c| c.set(c.get() + 1));
    CLIP_LOCATION.with(|c| c.set(write.location));
    CLIP_CONTENTS_LEN.with(|c| c.set(write.contents.len()));
    if !write.contents.is_empty() {
        CLIP_MIME.with(|c| *c.borrow_mut() = Some(write.contents[0].mime.to_vec()));
        CLIP_DATA.with(|c| *c.borrow_mut() = Some(write.contents[0].data.to_vec()));
    }
    write.reply(handler, CLIP_RESULT.with(|c| c.get()));
}

// test "clipboard_write effect callback" (oracle 634ef7198)
#[test]
fn clipboard_write_effect_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // A null callback (the default readonly effects) silently ignores writes.
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1B]52;c;aGVsbG8=\x1B\\");

        // Terminal should still be functional after the ignored sequence
        s.next_slice(b"AfterClipboard");
    }
    let str = t.plain_string(Global).unwrap();
    assert_eq!(b"AfterClipboard".as_slice(), str.as_slice());

    t.full_reset();

    CLIP_COUNT.with(|c| c.set(0));
    CLIP_RESULT.with(|c| c.set(clipboard::WriteResult::Denied));
    clip_clear_capture();

    let mut s = t.vt_stream();
    s.handler.effects.clipboard_write = Some(cap_clipboard_write);

    // Selectors are normalized and payloads are decoded before the callback.
    let cases: &[(&[u8], clipboard::Location, &[u8])] = &[
        (
            b"\x1B]52;c;aGVsbG8=\x1B\\",
            clipboard::Location::Standard,
            b"hello",
        ),
        (
            b"\x1B]52;s;d29ybGQ=\x07",
            clipboard::Location::Selection,
            b"world",
        ),
        (
            b"\x1B]52;p;cHJpbWFyeQ==\x1B\\",
            clipboard::Location::Primary,
            b"primary",
        ),
        (
            b"\x1B]52;0;Y3V0\x1B\\",
            clipboard::Location::Standard,
            b"cut",
        ),
        (
            b"\x1B]52;x;ZmFsbGJhY2s=\x1B\\",
            clipboard::Location::Standard,
            b"fallback",
        ),
        (
            b"\x1B]52;c;YQBi\x1B\\",
            clipboard::Location::Standard,
            b"a\x00b",
        ),
        // Missing padding is tolerated for OSC 52 since it has no way
        // to report errors to the client, matching kitty.
        (
            b"\x1B]52;c;dW5wYWRkZWQ\x1B\\",
            clipboard::Location::Standard,
            b"unpadded",
        ),
    ];

    for (i, (sequence, location, data)) in cases.iter().enumerate() {
        s.next_slice(sequence);
        assert_eq!(i + 1, CLIP_COUNT.with(|c| c.get()));
        assert_eq!(*location, CLIP_LOCATION.with(|c| c.get()));
        assert_eq!(1, CLIP_CONTENTS_LEN.with(|c| c.get()));
        assert_eq!(
            b"text/plain".as_slice(),
            CLIP_MIME.with(|c| c.borrow().clone()).unwrap().as_slice()
        );
        assert_eq!(
            *data,
            CLIP_DATA.with(|c| c.borrow().clone()).unwrap().as_slice()
        );
    }

    // Empty data is a clear, represented by an empty contents slice.
    s.next_slice(b"\x1B]52;s;\x1B\\");
    assert_eq!(cases.len() + 1, CLIP_COUNT.with(|c| c.get()));
    assert_eq!(
        clipboard::Location::Selection,
        CLIP_LOCATION.with(|c| c.get())
    );
    assert_eq!(0, CLIP_CONTENTS_LEN.with(|c| c.get()));
    assert!(CLIP_MIME.with(|c| c.borrow().is_none()));
    assert!(CLIP_DATA.with(|c| c.borrow().is_none()));

    // Reads and malformed base64 are ignored. The whole request is
    // discarded on invalid characters (including whitespace) rather
    // than decoding around them, per the Kitty clipboard spec that
    // governs OSC 52 base64 handling.
    s.next_slice(b"\x1B]52;c;?\x1B\\");
    s.next_slice(b"\x1B]52;c;***\x1B\\");
    s.next_slice(b"\x1B]52;c;SGVs!!!bG8=\x1B\\");
    s.next_slice(b"\x1B]52;c;aGVs bG8=\x1B\\");
    assert_eq!(cases.len() + 1, CLIP_COUNT.with(|c| c.get()));

    // OSC 1337 Copy shares the normalized clipboard write path.
    s.next_slice(b"\x1B]1337;Copy=:aVRlcm0y\x1B\\");
    assert_eq!(cases.len() + 2, CLIP_COUNT.with(|c| c.get()));
    assert_eq!(
        clipboard::Location::Standard,
        CLIP_LOCATION.with(|c| c.get())
    );
    assert_eq!(
        b"text/plain".as_slice(),
        CLIP_MIME.with(|c| c.borrow().clone()).unwrap().as_slice()
    );
    assert_eq!(
        b"iTerm2".as_slice(),
        CLIP_DATA.with(|c| c.borrow().clone()).unwrap().as_slice()
    );

    // Parsing across write boundaries still invokes exactly one atomic write.
    s.next_slice(b"\x1B]52;p;ZnJh");
    s.next_slice(b"Z21lbnRlZA==\x1B");
    s.next_slice(b"\\");
    assert_eq!(cases.len() + 3, CLIP_COUNT.with(|c| c.get()));
    assert_eq!(
        clipboard::Location::Primary,
        CLIP_LOCATION.with(|c| c.get())
    );
    assert_eq!(
        b"text/plain".as_slice(),
        CLIP_MIME.with(|c| c.borrow().clone()).unwrap().as_slice()
    );
    assert_eq!(
        b"fragmented".as_slice(),
        CLIP_DATA.with(|c| c.borrow().clone()).unwrap().as_slice()
    );

    // Reply results are intentionally ignored for protocols without a
    // write acknowledgement. The denied reply above did not stop later writes.
    assert_eq!(
        clipboard::WriteResult::Denied,
        CLIP_RESULT.with(|c| c.get())
    );

    clip_clear_capture();
    t.deinit(Global);
}

// test "clipboard_write allocation failure is ignored"
#[test]
fn clipboard_write_allocation_failure_is_ignored() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    CLIP_COUNT.with(|c| c.set(0));
    CLIP_RESULT.with(|c| {
        c.set(clipboard::WriteResult::Success(
            clipboard::WriteResultSuccess::default(),
        ))
    });
    clip_clear_capture();

    {
        let mut s = t.vt_stream();
        s.handler.effects.clipboard_write = Some(cap_clipboard_write);

        // Only the decoded scratch data uses the terminal allocator here. Swap in
        // an allocator that always fails, then restore it before terminal teardown.
        {
            // const alloc = t.screens.active.alloc;
            // t.screens.active.alloc = testing.failing_allocator;
            // defer t.screens.active.alloc = alloc;
            let alloc = s.handler.term().active_mut().alloc;
            s.handler.term().active_mut().alloc = crate::alloc::failing();
            s.next_slice(b"\x1B]52;c;aGVsbG8=\x1B\\");
            s.handler.term().active_mut().alloc = alloc;
        }

        assert_eq!(0, CLIP_COUNT.with(|c| c.get()));
        assert!(!s.handler.semantic_failure);

        // The failure is swallowed, not fatal: the very same sequence
        // succeeds once the allocation does.
        s.next_slice(b"\x1B]52;c;aGVsbG8=\x1B\\");
        assert_eq!(1, CLIP_COUNT.with(|c| c.get()));
        assert_eq!(
            b"hello".as_slice(),
            CLIP_DATA.with(|c| c.borrow().clone()).unwrap().as_slice()
        );
    }

    clip_clear_capture();
    t.deinit(Global);
}

// test "window_title effect is called"
#[test]
fn window_title_effect_is_called() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // fn titleChanged(_: *Handler) void { title_changed_count += 1; }
    fn title_changed(_: &mut Handler) {
        COUNTER.with(|c| c.set(c.get() + 1));
    }

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.title_changed = Some(title_changed);

    {
        // Set window title via OSC 2
        s.next_slice(b"\x1b]2;Hello World\x1b\\");
    }
    assert_eq!(t.get_title().unwrap(), b"Hello World");
    assert_eq!(1_usize, COUNTER.with(|c| c.get()));

    t.deinit(Global);
}

// test "window_title effect not called without callback"
#[test]
fn window_title_effect_not_called_without_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        // Should not crash when no callback is set
        s.next_slice(b"\x1b]2;Hello World\x1b\\");
    }

    // Title should still be set on terminal state
    assert_eq!(t.get_title().unwrap(), b"Hello World");

    {
        let mut s = t.vt_stream();
        // Terminal should still be functional
        s.next_slice(b"Test");
    }
    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"Test");

    t.deinit(Global);
}

// test "window_title effect with empty title"
#[test]
fn window_title_effect_with_empty_title() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    fn title_changed(_: &mut Handler) {
        COUNTER.with(|c| c.set(c.get() + 1));
    }

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.title_changed = Some(title_changed);

    {
        // Set empty window title
        s.next_slice(b"\x1b]2;\x1b\\");
    }
    assert!(t.get_title().is_none());
    assert_eq!(1_usize, COUNTER.with(|c| c.get()));

    t.deinit(Global);
}

// test "window_title not changed by cancelled OSC"
#[test]
fn window_title_not_changed_by_cancelled_osc() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    fn title_changed(_: &mut Handler) {
        COUNTER.with(|c| c.set(c.get() + 1));
    }

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.title_changed = Some(title_changed);

    s.next_slice(b"\x1b]2;before\x07");
    assert_eq!(s.handler.term().get_title().unwrap(), b"before");
    assert_eq!(1_usize, COUNTER.with(|c| c.get()));

    // Cancelled with CAN and SUB, fed in one slice.
    s.next_slice(b"\x1b]2;can\x18");
    assert_eq!(s.handler.term().get_title().unwrap(), b"before");
    assert_eq!(1_usize, COUNTER.with(|c| c.get()));
    s.next_slice(b"\x1b]2;sub\x1a");
    assert_eq!(s.handler.term().get_title().unwrap(), b"before");
    assert_eq!(1_usize, COUNTER.with(|c| c.get()));

    // Cancelled with CAN and SUB, fed one byte at a time.
    for c in b"\x1b]2;can\x18" {
        s.next(*c);
    }
    assert_eq!(s.handler.term().get_title().unwrap(), b"before");
    assert_eq!(1_usize, COUNTER.with(|c| c.get()));
    for c in b"\x1b]2;sub\x1a" {
        s.next(*c);
    }
    assert_eq!(s.handler.term().get_title().unwrap(), b"before");
    assert_eq!(1_usize, COUNTER.with(|c| c.get()));

    // An OSC that ends normally after a cancel still takes effect.
    s.next_slice(b"\x1b]2;after\x1b\\");
    assert_eq!(s.handler.term().get_title().unwrap(), b"after");
    assert_eq!(2_usize, COUNTER.with(|c| c.get()));

    drop(s);
    t.deinit(Global);
}

// S166-F3: `do_window_title` logs `log.warn("title length {d} exceeds max
// length {d}, truncating", .{...})` (stream_terminal.zig:1471-1474) when
// the OSC 2 title payload exceeds 1024 bytes.
#[test]
fn window_title_over_max_length_logs_and_truncates() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    let mut osc = Vec::new();
    osc.extend_from_slice(b"\x1b]2;");
    osc.extend(std::iter::repeat(b'A').take(2000));
    osc.extend_from_slice(b"\x1b\\");

    let cap = crate::log_capture::start();
    let mut s = t.vt_stream();
    s.next_slice(&osc);
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "stream_terminal"
                && msg.starts_with("title length 2000 exceeds max length 1024, truncating")),
        "expected the title-length-exceeded diagnostic, got: {messages:?}"
    );
    drop(cap);

    assert_eq!(1024, t.get_title().unwrap().len());

    t.deinit(Global);
}

// test "kitty_keyboard_query"
#[test]
fn kitty_keyboard_query() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    // Default kitty keyboard flags should be 0
    s.next_slice(b"\x1b[?u");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?0u");

    // Push kitty keyboard mode with flags and query again
    cap_reset();
    s.next_slice(b"\x1b[>1u"); // push with disambiguate flag
    s.next_slice(b"\x1b[?u");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?1u");

    drop(s);
    t.deinit(Global);
}

// test "xtversion default"
//
// PORT NOTE (R2 ghostty->Rustty): the reference asserts the fallback version
// "libghostty"; the retargeted fallback emits "librustty".
#[test]
fn xtversion_default() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    {
        // Without xtversion effect set, should report "librustty"
        s.next_slice(b"\x1b[>0q");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP>|librustty\x1b\\");

    t.deinit(Global);
}

// test "xtversion with effect"
#[test]
fn xtversion_with_effect() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // fn xtversion(_: *Handler) []const u8 { return "ghostty 1.2.3"; }
    fn xtversion(_: &mut Handler) -> &'static [u8] {
        b"ghostty 1.2.3"
    }

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.xtversion = Some(xtversion);

    {
        s.next_slice(b"\x1b[>0q");
    }
    assert_eq!(
        cap_take().unwrap().as_slice(),
        b"\x1bP>|ghostty 1.2.3\x1b\\"
    );

    t.deinit(Global);
}

// test "xtversion with empty string effect"
//
// PORT NOTE (R2 ghostty->Rustty): the empty-string effect falls back to the
// retargeted "librustty".
#[test]
fn xtversion_with_empty_string_effect() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // fn xtversion(_: *Handler) []const u8 { return ""; }
    fn xtversion(_: &mut Handler) -> &'static [u8] {
        b""
    }

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.xtversion = Some(xtversion);

    {
        // Empty string from effect should fall back to "librustty"
        s.next_slice(b"\x1b[>0q");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP>|librustty\x1b\\");

    t.deinit(Global);
}

// NEW TESTS (no oracle counterpart): the reference formats the response
// with `std.fmt.bufPrintZ` into a fixed `[288]u8` and `catch return`s, so
// the response (framing + version + NUL) must fit in 288 bytes. That caps
// the effective version string at 288 - 1 - 4 - 2 = 281 bytes; a 282-byte
// version produces NO pty write at all.
#[test]
fn xtversion_281_byte_version_is_written() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    static V281: [u8; 281] = [b'v'; 281];
    fn xtversion(_: &mut Handler) -> &'static [u8] {
        &V281
    }

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.xtversion = Some(xtversion);
    s.next_slice(b"\x1b[>0q");

    let mut expect: Vec<u8> = Vec::new();
    expect.extend_from_slice(b"\x1bP>|");
    expect.extend_from_slice(&V281);
    expect.extend_from_slice(b"\x1b\\");
    assert_eq!(287, expect.len());
    assert_eq!(expect.as_slice(), cap_take().unwrap().as_slice());

    t.deinit(Global);
}

#[test]
fn xtversion_282_byte_version_is_suppressed() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    static V282: [u8; 282] = [b'v'; 282];
    fn xtversion(_: &mut Handler) -> &'static [u8] {
        &V282
    }

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.xtversion = Some(xtversion);
    s.next_slice(b"\x1b[>0q");

    assert_eq!(None, cap_take());

    t.deinit(Global);
}

// fn getSize(_: *Handler) ?size_report.Size {
//     return .{ .rows = 24, .columns = 80, .cell_width = 9, .cell_height = 18 };
// }
fn get_size(_: &mut Handler) -> Option<size_report::Size> {
    Some(size_report::Size {
        rows: 24,
        columns: 80,
        cell_width: 9,
        cell_height: 18,
    })
}

// test "size report csi_14_t with effect"
#[test]
fn size_report_csi_14_t_with_effect() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.size = Some(get_size);

    {
        // CSI 14 t - report text area size in pixels
        s.next_slice(b"\x1b[14t");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[4;432;720t");

    t.deinit(Global);
}

// test "size report csi_16_t with effect"
#[test]
fn size_report_csi_16_t_with_effect() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.size = Some(get_size);

    {
        // CSI 16 t - report cell size in pixels
        s.next_slice(b"\x1b[16t");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[6;18;9t");

    t.deinit(Global);
}

// test "size report csi_18_t with effect"
#[test]
fn size_report_csi_18_t_with_effect() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.size = Some(get_size);

    {
        // CSI 18 t - report text area size in characters
        s.next_slice(b"\x1b[18t");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[8;24;80t");

    t.deinit(Global);
}

// test "size report no effect callback"
#[test]
fn size_report_no_effect_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    {
        // Without size effect, size reports should be silently ignored
        s.next_slice(b"\x1b[14t");
    }
    assert!(cap_take().is_none());

    t.deinit(Global);
}

// test "size report csi_21_t title disabled by default"
#[test]
fn size_report_csi_21_t_title_disabled_by_default() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    {
        // Set a title first
        s.next_slice(b"\x1b]2;My Title\x1b\\");

        // CSI 21 t - report title (no size effect needed)
        s.next_slice(b"\x1b[21t");
    }
    assert!(cap_take().is_none());

    t.deinit(Global);
}

// test "size report csi_21_t title enabled"
#[test]
fn size_report_csi_21_t_title_enabled() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.title_report = true;

    {
        // Set a title first
        s.next_slice(b"\x1b]2;My Title\x1b\\");

        // CSI 21 t - report title (no size effect needed)
        s.next_slice(b"\x1b[21t");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b]lMy Title\x1b\\");

    t.deinit(Global);
}

// test "DECRQCRA disabled" (oracle 9272a2f7c)
#[test]
fn decrqcra_disabled() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    s.next_slice(b"hello\x1b[1;1;1;1;1;5*y");
    assert!(cap_take().is_none());

    t.deinit(Global);
}

// test "DECRQCRA enabled" (oracle 9272a2f7c)
#[test]
fn decrqcra_enabled() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.xt_checksum_report = true;

    // "hello" sums to 0x214, which the DEC checksum negates.
    s.next_slice(b"hello\x1b[3;1;1;1;1;5*y");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP3!~FDEC\x1b\\");

    // XTCHECKSUM 1 turns off the negation.
    s.next_slice(b"\x1b[1#y\x1b[4;1;1;1;1;5*y");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP4!~0214\x1b\\");

    // A full reset restores the DEC checksum.
    s.next_slice(b"\x1bchello\x1b[5;1;1;1;1;5*y");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP5!~FDEC\x1b\\");

    t.deinit(Global);
}

// test "XTCHECKSUM ignored without xt checksum report" (oracle 9272a2f7c)
#[test]
fn xtchecksum_ignored_without_xt_checksum_report() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[1#y");
    }
    assert_eq!(crate::xt_checksum::Flags::default(), t.flags.xt_checksum);

    t.deinit(Global);
}

// test "enquiry no effect"
#[test]
fn enquiry_no_effect() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    {
        // ENQ without enquiry effect should not write anything
        s.next_slice(b"\x05");
    }
    assert!(cap_take().is_none());

    t.deinit(Global);
}

// test "enquiry with effect"
#[test]
fn enquiry_with_effect() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // fn enquiry(_: *Handler) []const u8 { return "ghostty"; }
    fn enquiry(_: &mut Handler) -> &'static [u8] {
        b"ghostty"
    }

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.enquiry = Some(enquiry);

    {
        s.next_slice(b"\x05");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"ghostty");

    t.deinit(Global);
}

// test "enquiry with empty response"
#[test]
fn enquiry_with_empty_response() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // fn enquiry(_: *Handler) []const u8 { return ""; }
    fn enquiry(_: &mut Handler) -> &'static [u8] {
        b""
    }

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.enquiry = Some(enquiry);

    {
        // Empty enquiry response should not write anything
        s.next_slice(b"\x05");
    }
    assert!(cap_take().is_none());

    t.deinit(Global);
}

// test "device status: operating status"
#[test]
fn device_status_operating_status() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    {
        // CSI 5 n — operating status report
        s.next_slice(b"\x1B[5n");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[0n");

    t.deinit(Global);
}

// test "device status: cursor position"
#[test]
fn device_status_cursor_position() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    // Default position is 0,0 — reported as 1,1
    s.next_slice(b"\x1B[6n");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[1;1R");

    // Move cursor to row 5, col 10
    s.next_slice(b"\x1B[5;10H");
    s.next_slice(b"\x1B[6n");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[5;10R");

    drop(s);
    t.deinit(Global);
}

// test "device status: cursor position with origin mode"
#[test]
fn device_status_cursor_position_with_origin_mode() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    {
        // Set scroll region rows 5-20
        s.next_slice(b"\x1B[5;20r");
        // Enable origin mode
        s.next_slice(b"\x1B[?6h");
        // Move to row 3, col 5 within the region
        s.next_slice(b"\x1B[3;5H");
        // Query cursor position
        s.next_slice(b"\x1B[6n");
    }
    // Should report position relative to the scroll region
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[3;5R");

    t.deinit(Global);
}

// fn colorScheme(_: *Handler) ?device_status.ColorScheme { return .dark; }
fn color_scheme_dark(_: &mut Handler) -> Option<device_status::ColorScheme> {
    Some(device_status::ColorScheme::Dark)
}

// fn colorScheme(_: *Handler) ?device_status.ColorScheme { return .light; }
fn color_scheme_light(_: &mut Handler) -> Option<device_status::ColorScheme> {
    Some(device_status::ColorScheme::Light)
}

// test "device status: color scheme dark"
#[test]
fn device_status_color_scheme_dark() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.color_scheme = Some(color_scheme_dark);

    {
        // CSI ? 996 n — color scheme query
        s.next_slice(b"\x1B[?996n");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?997;1n");

    t.deinit(Global);
}

// test "device status: color scheme light"
#[test]
fn device_status_color_scheme_light() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.color_scheme = Some(color_scheme_light);

    {
        // CSI ? 996 n — color scheme query
        s.next_slice(b"\x1B[?996n");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?997;2n");

    t.deinit(Global);
}

// test "device status: color scheme without callback"
#[test]
fn device_status_color_scheme_without_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    {
        // Without color_scheme effect, query should be silently ignored
        s.next_slice(b"\x1B[?996n");
    }
    assert!(cap_take().is_none());

    t.deinit(Global);
}

// test "visibility reports"
//
// NEGATIVE CONTROL (P3-W4 T7 Step 5), vt half. Three independently
// droppable behaviours, three assertions:
//   (1) mode 2033 exists and an explicit CSI ? 998 n query answers without
//       enabling it,
//   (2) ENABLING always sends an immediate report, even when already
//       enabled (and DISABLING sends nothing) — a count assertion, because
//       a value assertion cannot tell "sent again" from "still the old
//       value",
//   (3) RIS PRESERVES `flags.visible` (the exemption). Removing the
//       exemption makes the final assertion fail.
#[test]
fn visibility_reports() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    calls_reset();

    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(count_write_pty);

        // Mode 2033 is supported and initially disabled.
        s.next_slice(b"\x1B[?2033$p");
        assert_eq!(b"\x1B[?2033;2$y".as_slice(), cap_take().unwrap().as_slice());

        // A one-shot query reports the current state without enabling the
        // mode.
        s.next_slice(b"\x1B[?998n");
        assert_eq!(b"\x1B[?999;1n".as_slice(), cap_take().unwrap().as_slice());
        assert!(!s.handler.term().modes.get(modespkg::Mode::ReportVisibility));

        // Enabling always sends an immediate report, even when already
        // enabled.
        s.handler.term().flags.visible = false;
        s.next_slice(b"\x1B[?2033h");
        assert_eq!(b"\x1B[?999;2n".as_slice(), cap_take().unwrap().as_slice());
        let count = calls_get();
        s.next_slice(b"\x1B[?2033h");
        assert_eq!(count + 1, calls_get());

        // Disabling sends no report.
        s.next_slice(b"\x1B[?2033l");
        assert_eq!(count + 1, calls_get());

        // A terminal reset preserves the view's externally owned
        // visibility.
        s.next_slice(b"\x1Bc");
        s.next_slice(b"\x1B[?998n");
        assert_eq!(b"\x1B[?999;2n".as_slice(), cap_take().unwrap().as_slice());
    }

    t.deinit(Global);
}

// test "device status: readonly ignores all"
#[test]
fn device_status_readonly_ignores_all() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        // All device status queries should be silently ignored without effects
        s.next_slice(b"\x1B[5n");
        s.next_slice(b"\x1B[6n");
        s.next_slice(b"\x1B[?996n");
        s.next_slice(b"\x1B[?998n");

        // Terminal should still be functional
        s.next_slice(b"Test");
    }
    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"Test");

    t.deinit(Global);
}

// fn da(_: *Handler) device_attributes.Attributes { return .{}; }
fn da_default(_: &mut Handler) -> device_attributes::Attributes {
    device_attributes::Attributes::default()
}

// test "device attributes: primary DA"
#[test]
fn device_attributes_primary_da() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.device_attributes = Some(da_default);

    {
        s.next_slice(b"\x1B[c");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?62;22c");

    t.deinit(Global);
}

// test "device attributes: secondary DA"
#[test]
fn device_attributes_secondary_da() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.device_attributes = Some(da_default);

    {
        s.next_slice(b"\x1B[>c");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[>1;0;0c");

    t.deinit(Global);
}

// test "device attributes: tertiary DA"
#[test]
fn device_attributes_tertiary_da() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.device_attributes = Some(da_default);

    {
        s.next_slice(b"\x1B[=c");
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP!|00000000\x1b\\");

    t.deinit(Global);
}

// test "device attributes: readonly ignores"
#[test]
fn device_attributes_readonly_ignores() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        // All DA queries should be silently ignored without effects
        s.next_slice(b"\x1B[c");
        s.next_slice(b"\x1B[>c");
        s.next_slice(b"\x1B[=c");

        // Terminal should still be functional
        s.next_slice(b"Test");
    }
    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"Test");

    t.deinit(Global);
}

// fn da(_: *Handler) device_attributes.Attributes {
//     return .{
//         .primary = .{
//             .conformance_level = .vt420,
//             .features = &.{ .ansi_color, .clipboard },
//         },
//         .secondary = .{
//             .device_type = .vt420,
//             .firmware_version = 100,
//         },
//     };
// }
fn da_custom(_: &mut Handler) -> device_attributes::Attributes {
    device_attributes::Attributes {
        primary: device_attributes::Primary {
            conformance_level: device_attributes::ConformanceLevel::VT420,
            features: &[
                device_attributes::Feature::AnsiColor,
                device_attributes::Feature::Clipboard,
            ],
        },
        secondary: device_attributes::Secondary {
            device_type: device_attributes::DeviceType::VT420,
            firmware_version: 100,
            ..device_attributes::Secondary::default()
        },
        ..device_attributes::Attributes::default()
    }
}

// test "device attributes: custom response"
#[test]
fn device_attributes_custom_response() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);
    s.handler.effects.device_attributes = Some(da_custom);

    s.next_slice(b"\x1B[c");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?64;22;52c");

    s.next_slice(b"\x1B[>c");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[>41;100;0c");

    drop(s);
    t.deinit(Global);
}

// test "kitty graphics APC response"
#[test]
#[cfg_attr(
    not(all(feature = "kitty-graphics", not(target_os = "none"))),
    ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
)]
fn kitty_graphics_apc_response() {
    // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    // handler.effects.write_pty = &S.writePty;
    s.handler.effects.write_pty = Some(cap_write_pty);

    // Send a kitty graphics transmit command with image id 1
    s.next_slice(b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2,c=10,r=1;////////\x1b\\");

    // Should have written a response back
    // try testing.expectEqualStrings("\x1b_Gi=1;OK\x1b\\", S.written.?);
    drop(s);
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b_Gi=1;OK\x1b\\");

    t.deinit(Global);
}

// test "kitty graphics via APC"
#[test]
#[cfg_attr(
    not(all(feature = "kitty-graphics", not(target_os = "none"))),
    ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
)]
fn kitty_graphics_via_apc() {
    // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Send a kitty graphics transmit command via APC:
        // ESC _ G <payload> ESC \
        // a=t,t=d,f=24,i=1,s=1,v=2,c=10,r=1;//////// (1x2 RGB direct)
        s.next_slice(b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2,c=10,r=1;////////\x1b\\");
    }

    // const storage = &t.screens.active.kitty_images;
    // const img = storage.imageById(1).?;
    // try testing.expectEqual(.rgb, img.format);
    let storage = unsafe { &(*t.screens.active).kitty_images };
    let img = storage.image_by_id(1).unwrap();
    assert_eq!(kitty::graphics_command::Format::Rgb, img.format);

    t.deinit(Global);
}

// ---- rustty-only (R13, UPSTREAM-DEFECTS.md UD61, and the port's P6) ----
//
// Kitty `t=s` (POSIX shared memory) transmissions through the VT stream. The
// test plays the peer: it creates and fills the object, keeps its own
// read-write descriptor, and can shrink the object while the terminal loads it.

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct PeerShm {
    fd: libc::c_int,
    name: std::ffi::CString,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl PeerShm {
    fn create(tag: &str, len: usize, fill: impl Fn(usize) -> u8) -> PeerShm {
        // macOS limits shared-memory names to 31 bytes.
        let name = std::ffi::CString::new(format!("/r13b-{}-{tag}", std::process::id())).unwrap();
        // SAFETY: `name` is a valid NUL-terminated string.
        unsafe { libc::shm_unlink(name.as_ptr()) };
        // SAFETY: as above.
        let fd = unsafe {
            libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o600,
            )
        };
        assert!(fd >= 0, "shm_open: {}", std::io::Error::last_os_error());
        // SAFETY: `fd` is open read-write.
        assert_eq!(0, unsafe { libc::ftruncate(fd, len as libc::off_t) });
        // SAFETY: `fd` is open read-write and `len` bytes long.
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        assert_ne!(
            libc::MAP_FAILED,
            map,
            "mmap: {}",
            std::io::Error::last_os_error()
        );
        for i in 0..len {
            // SAFETY: `i < len`, inside the writable mapping.
            unsafe { *(map as *mut u8).add(i) = fill(i) };
        }
        // SAFETY: `map`/`len` come from the mmap above.
        unsafe { libc::munmap(map, len) };
        PeerShm { fd, name }
    }

    /// `ESC _ G a=t,t=s,f=24,s=2,v=2,i=<id>,O=<offset>,S=<size>;<name> ESC \`
    fn transmit(&self, id: u32, offset: usize, size: usize) -> Vec<u8> {
        use crate::snapshot::io::Writer as _;
        let mut aw = crate::snapshot::io::AllocatingWriter::new();
        aw.write_all(format!("\x1b_Ga=t,t=s,f=24,s=2,v=2,i={id},O={offset},S={size};").as_bytes())
            .unwrap();
        crate::kitty::clipboard_response::b64_encode_writer(&mut aw, self.name.as_bytes()).unwrap();
        aw.write_all(b"\x1b\\").unwrap();
        aw.written().to_vec()
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for PeerShm {
    fn drop(&mut self) {
        // SAFETY: `fd` and `name` come from `create`.
        unsafe {
            libc::close(self.fd);
            libc::shm_unlink(self.name.as_ptr());
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn shm_opts() -> Options {
    let mut o = opts(10, 10);
    o.kitty_image_loading_limits = kitty::graphics_image::Limits {
        file: false,
        temporary_file: kitty::graphics_image::TemporaryFileLimit::Disabled,
        shared_memory: true,
    };
    o
}

/// The load reads exactly `O`..`O+S` of the object (`pread` on Linux, the
/// raw-pointer copy out of the mapping on macOS, where `pread` answers
/// ESPIPE).
#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[cfg_attr(
    not(all(feature = "kitty-graphics", not(target_os = "none"))),
    ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
)]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-vt, parity tail C-1): calls `shm_open`/`shm_unlink`, foreign functions Miri cannot execute; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
)]
fn kitty_graphics_shared_memory_copies_the_requested_range() {
    let shm = PeerShm::create("range", 4096, |i| (i % 251) as u8);
    let mut t = Terminal::init(Global, shm_opts()).unwrap();

    cap_reset();
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.next_slice(&shm.transmit(7, 5, 12));
    }
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b_Gi=7;OK\x1b\\");

    let storage = unsafe { &(*t.screens.active).kitty_images };
    let img = storage.image_by_id(7).expect("image 7 must be stored");
    let expected: Vec<u8> = (5..17).map(|i| (i % 251) as u8).collect();
    match &img.data {
        kitty::graphics_image::Data::Complete(d) => {
            assert_eq!(expected.as_slice(), d.as_slice())
        }
        other => panic!("unexpected image data: {other:?}"),
    }

    t.deinit(Global);
}

/// The reference's SIGBUS: the peer shrinks the object between the terminal's
/// `fstat` and its copy. The load now fails with `EINVAL: invalid data` and
/// the terminal survives. Linux only: macOS refuses to shrink a POSIX
/// shared-memory object (`ftruncate` returns EINVAL, measured).
#[test]
#[cfg(target_os = "linux")]
#[cfg_attr(
    not(all(feature = "kitty-graphics", not(target_os = "none"))),
    ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
)]
fn kitty_graphics_shared_memory_truncated_after_fstat_is_invalid_data() {
    let shm = PeerShm::create("trunc", 4096, |i| (i % 251) as u8);
    let peer_fd = shm.fd;
    let truncated = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = truncated.clone();
    kitty::graphics_image::LoadingImage::test_after_fstat_hook(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            // SAFETY: `peer_fd` is the peer's open read-write descriptor.
            assert_eq!(0, unsafe { libc::ftruncate(peer_fd, 0) });
            flag.set(true);
        }));
    });

    let mut t = Terminal::init(Global, shm_opts()).unwrap();
    cap_reset();
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.next_slice(&shm.transmit(7, 5, 12));
    }
    kitty::graphics_image::LoadingImage::test_after_fstat_hook(|hook| {
        *hook.borrow_mut() = None;
    });

    // Positive reach: the truncation ran inside the load, after the fstat.
    assert!(
        truncated.get(),
        "the peer truncation must run between fstat and the read"
    );
    assert_eq!(
        cap_take().unwrap().as_slice(),
        b"\x1b_Gi=7;EINVAL: invalid data\x1b\\"
    );
    let storage = unsafe { &(*t.screens.active).kitty_images };
    assert!(storage.image_by_id(7).is_none());

    t.deinit(Global);
}

// test "continuation reconstructs standard stream without duplicate effects"
#[test]
fn continuation_reconstructs_standard_stream_without_duplicate_effects() {
    use crate::clipboard;
    use crate::stream_continuation::FixedIoWriter;

    // `testing.allocator` for `Options.allocator` (the Rust heap handle,
    // as `Terminal::vt_stream` passes `gpa()`).
    fn cont_alloc() -> crate::alloc::Allocator {
        crate::alloc::Global
    }

    // const S = struct { var bell_count ... fn reset() ... };
    // PORT NOTE: the reference hangs `var`s on a local `S` struct; the
    // callbacks are bare `fn` pointers here, so the counters live in
    // thread-locals (the module's established idiom).
    thread_local! {
        static BELL_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static TITLE_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static WRITE_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static NOTIFICATION_COUNT: core::cell::Cell<usize> =
            const { core::cell::Cell::new(0) };
        static CLIPBOARD_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    fn bell(_: &mut Handler) {
        BELL_COUNT.with(|c| c.set(c.get() + 1));
    }
    fn title_changed(_: &mut Handler) {
        TITLE_COUNT.with(|c| c.set(c.get() + 1));
    }
    fn write_pty(_: &mut Handler, _: &[u8]) {
        WRITE_COUNT.with(|c| c.set(c.get() + 1));
    }
    fn desktop_notification(_: &mut Handler, _: stream::ShowDesktopNotification) {
        NOTIFICATION_COUNT.with(|c| c.set(c.get() + 1));
    }
    fn clipboard_write(handler: &mut Handler, write: clipboard::Write) {
        CLIPBOARD_COUNT.with(|c| c.set(c.get() + 1));
        // write.reply(.{ .success = .{} });
        write.reply(
            handler,
            clipboard::WriteResult::Success(clipboard::WriteResultSuccess::default()),
        );
    }
    fn reset() {
        BELL_COUNT.with(|c| c.set(0));
        TITLE_COUNT.with(|c| c.set(0));
        WRITE_COUNT.with(|c| c.set(0));
        NOTIFICATION_COUNT.with(|c| c.set(0));
        CLIPBOARD_COUNT.with(|c| c.set(0));
    }
    reset();

    // const committed = "A\n\x07" ++ "\x1b]2;title\x1b\\" ++ "\x1b[5n" ++
    //     "\x1b]9;body\x1b\\" ++ "\x1b]52;c;aA==\x1b\\";
    const COMMITTED: &[u8] =
        b"A\n\x07\x1b]2;title\x1b\\\x1b[5n\x1b]9;body\x1b\\\x1b]52;c;aA==\x1b\\";

    let mut source_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
    // PORT NOTE (SB discipline): the handler aliases the terminal through a
    // raw pointer while the test also reads terminal state directly. Both
    // go through ONE `*mut Terminal` per terminal so no fresh `&mut`
    // invalidates the stream's stored pointer mid-test (the module's
    // raw-pointer seam; memory rust-mut-args-strongly-protected).
    let source_ptr: *mut Terminal = &mut source_terminal;

    // var source_handler: Handler = .init(&source_terminal);
    let mut source_handler = Handler::init(source_ptr);
    source_handler.effects.bell = Some(bell);
    source_handler.effects.title_changed = Some(title_changed);
    source_handler.effects.write_pty = Some(write_pty);
    source_handler.effects.desktop_notification = Some(desktop_notification);
    source_handler.effects.clipboard_write = Some(clipboard_write);
    // var source = Stream.init(.{
    //     .allocator = testing.allocator,
    //     .handler = source_handler,
    //     .continuation_max_bytes = 1024,
    // });
    // defer source.deinit();
    let mut source = Stream::init(stream::Options {
        allocator: Some(cont_alloc()),
        handler: source_handler,
        continuation_max_bytes: Some(1024),
        osc_unknown_max_bytes: 0,
    });

    // Terminal mutation and all callbacks have already committed. The
    // unfinished CSI is the only input needed to recreate the stream state.
    let mut committed_and_pending = COMMITTED.to_vec();
    committed_and_pending.extend_from_slice(b"\x1b[31");
    source.next_slice(&committed_and_pending);
    assert_eq!(1, BELL_COUNT.with(|c| c.get()));
    assert_eq!(1, TITLE_COUNT.with(|c| c.get()));
    assert_eq!(1, WRITE_COUNT.with(|c| c.get()));
    assert_eq!(1, NOTIFICATION_COUNT.with(|c| c.get()));
    assert_eq!(1, CLIPBOARD_COUNT.with(|c| c.get()));

    let mut continuation_buf = [0u8; 1024];
    let mut continuation_writer = FixedIoWriter::new(&mut continuation_buf);
    source.write_continuation(&mut continuation_writer).unwrap();
    assert_eq!(b"\x1b[31".as_slice(), continuation_writer.buffered());

    let mut restored_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
    let restored_ptr: *mut Terminal = &mut restored_terminal;

    // Stand in for restoring the already-committed terminal snapshot.
    {
        // var snapshot_stream: Stream = .init(.{
        //     .allocator = testing.allocator,
        //     .handler = .init(&restored_terminal),
        // });
        // defer snapshot_stream.deinit();
        let mut snapshot_stream = Stream::init(stream::Options {
            allocator: Some(cont_alloc()),
            handler: Handler::init(restored_ptr),
            continuation_max_bytes: None,
            osc_unknown_max_bytes: 0,
        });
        snapshot_stream.next_slice(COMMITTED);
    }

    // const before = try restored_terminal.plainString(testing.allocator);
    let before = unsafe { (*restored_ptr).plain_string(Global) }.unwrap();
    let before_x = unsafe { (*(*restored_ptr).screens.active).cursor.x };
    let before_y = unsafe { (*(*restored_ptr).screens.active).cursor.y };
    let before_style = unsafe { (*(*restored_ptr).screens.active).cursor.style_id };
    let before_title = unsafe { (*restored_ptr).get_title() }.unwrap().to_vec();

    let mut restored_handler = Handler::init(restored_ptr);
    restored_handler.effects.bell = Some(bell);
    restored_handler.effects.title_changed = Some(title_changed);
    restored_handler.effects.write_pty = Some(write_pty);
    restored_handler.effects.desktop_notification = Some(desktop_notification);
    restored_handler.effects.clipboard_write = Some(clipboard_write);
    // var restored = Stream.init(.{ ... .continuation_max_bytes = 1024 });
    // defer restored.deinit();
    let mut restored = Stream::init(stream::Options {
        allocator: Some(cont_alloc()),
        handler: restored_handler,
        continuation_max_bytes: Some(1024),
        osc_unknown_max_bytes: 0,
    });

    // S.reset();
    reset();
    restored.next_slice(continuation_writer.buffered());
    assert_eq!(0, BELL_COUNT.with(|c| c.get()));
    assert_eq!(0, TITLE_COUNT.with(|c| c.get()));
    assert_eq!(0, WRITE_COUNT.with(|c| c.get()));
    assert_eq!(0, NOTIFICATION_COUNT.with(|c| c.get()));
    assert_eq!(0, CLIPBOARD_COUNT.with(|c| c.get()));
    let after = unsafe { (*restored_ptr).plain_string(Global) }.unwrap();
    assert_eq!(before, after);
    assert_eq!(before_x, unsafe {
        (*(*restored_ptr).screens.active).cursor.x
    });
    assert_eq!(before_y, unsafe {
        (*(*restored_ptr).screens.active).cursor.y
    });
    assert_eq!(before_style, unsafe {
        (*(*restored_ptr).screens.active).cursor.style_id
    });
    assert_eq!(
        before_title.as_slice(),
        unsafe { (*restored_ptr).get_title() }.unwrap()
    );

    source.next_slice(b"mB");
    restored.next_slice(b"mB");
    let source_text = unsafe { (*source_ptr).plain_string(Global) }.unwrap();
    let restored_text = unsafe { (*restored_ptr).plain_string(Global) }.unwrap();
    assert_eq!(source_text, restored_text);
    assert_eq!(
        unsafe { (*(*source_ptr).screens.active).cursor.style_id },
        unsafe { (*(*restored_ptr).screens.active).cursor.style_id },
    );

    source.deinit();
    restored.deinit();
    restored_terminal.deinit(Global);
    source_terminal.deinit(Global);
}
