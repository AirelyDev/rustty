//! stream tests: CSI dispatch (modes, DECSCA/DECSED/DECSEL, DECSCUSR,
//! XTSHIFTESCAPE, CSI t/W, SGR, tab clear) and the OSC title/52 paths.

use super::*;

// test "stream: cursor right (CUF)" { ... }
#[test]
fn stream_cursor_right_cuf() {
    struct H {
        amount: u16,
    }
    impl Handler for H {
        fn cursor_right(&mut self, v: CursorMovement) {
            self.amount = v.value;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { amount: 0 }));
    s.next_slice(b"\x1B[C");
    assert_eq!(1u16, s.handler.amount);

    s.next_slice(b"\x1B[5C");
    assert_eq!(5u16, s.handler.amount);

    s.handler.amount = 0;
    s.next_slice(b"\x1B[5;4C");
    assert_eq!(0u16, s.handler.amount);

    s.handler.amount = 0;
    s.next_slice(b"\x1b[?3C");
    assert_eq!(0u16, s.handler.amount);
}

// test "stream: dec set mode (SM) and reset mode (RM)" { ... }
#[test]
fn stream_dec_set_mode_sm_and_reset_mode_rm() {
    // PORT NOTE: the reference sentinel `@as(modes.Mode, @enumFromInt(1))`
    // is the mode whose tag is 1 (DECCKM / `cursor_keys`); used here as a
    // "not origin" marker. We use `modes::Mode::CursorKeys` (= 1) directly.
    struct H {
        mode: modes::Mode,
    }
    impl Handler for H {
        fn set_mode(&mut self, v: Mode) {
            self.mode = v.mode;
        }
        fn reset_mode(&mut self, _v: Mode) {
            self.mode = modes::Mode::CursorKeys;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H {
        mode: modes::Mode::CursorKeys,
    }));
    s.next_slice(b"\x1B[?6h");
    assert_eq!(modes::Mode::Origin, s.handler.mode);

    s.next_slice(b"\x1B[?6l");
    assert_eq!(modes::Mode::CursorKeys, s.handler.mode);

    s.handler.mode = modes::Mode::CursorKeys;
    s.next_slice(b"\x1B[6 h");
    assert_eq!(modes::Mode::CursorKeys, s.handler.mode);
}

// test "stream: ansi set mode (SM) and reset mode (RM)" { ... }
#[test]
fn stream_ansi_set_mode_sm_and_reset_mode_rm() {
    struct H {
        mode: Option<modes::Mode>,
    }
    impl Handler for H {
        fn set_mode(&mut self, v: Mode) {
            self.mode = Some(v.mode);
        }
        fn reset_mode(&mut self, _v: Mode) {
            self.mode = None;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { mode: None }));
    s.next_slice(b"\x1B[4h");
    assert_eq!(modes::Mode::Insert, s.handler.mode.unwrap());

    s.next_slice(b"\x1B[4l");
    assert!(s.handler.mode.is_none());

    s.handler.mode = None;
    s.next_slice(b"\x1B[>5h");
    assert!(s.handler.mode.is_none());
}

// test "stream: DECRQCRA dispatch" (oracle 9272a2f7c)
#[test]
fn stream_decrqcra_dispatch() {
    #[derive(Default)]
    struct H {
        calls: usize,
        req: Option<crate::xt_checksum::Request>,
    }
    impl Handler for H {
        fn request_xt_checksum(&mut self, v: crate::xt_checksum::Request) {
            self.calls += 1;
            self.req = Some(v);
        }
    }

    struct Case {
        input: &'static [u8],
        req: Option<crate::xt_checksum::Request>,
    }
    use crate::xt_checksum::Request;
    let cases = [
        Case {
            input: b"\x1b[7;1;2;3;4;5*y",
            req: Some(Request {
                id: 7,
                top: 2,
                left: 3,
                bottom: 4,
                right: 5,
            }),
        },
        Case {
            input: b"\x1b[7*y",
            req: Some(Request {
                id: 7,
                ..Request::default()
            }),
        },
        Case {
            input: b"\x1b[*y",
            req: Some(Request::default()),
        },
        Case {
            input: b"\x1b[1;1;2;3*y",
            req: Some(Request {
                id: 1,
                top: 2,
                left: 3,
                ..Request::default()
            }),
        },
        Case {
            input: b"\x1b[1;1;1;1;1;1;1*y",
            req: None,
        },
        Case {
            input: b"\x1b[1;1;1;1;1;1y",
            req: None,
        },
        Case {
            input: b"\x1b[1;1;1;1;1;1\"y",
            req: None,
        },
    ];

    for case in cases {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
        s.next_slice(case.input);
        if let Some(req) = case.req {
            assert_eq!(1, s.handler.calls);
            assert_eq!(req, s.handler.req.unwrap());
        } else {
            assert_eq!(0, s.handler.calls);
        }
    }
}

// test "stream: XTCHECKSUM dispatch" (oracle 9272a2f7c)
#[test]
fn stream_xtchecksum_dispatch() {
    use crate::xt_checksum::Flags;

    #[derive(Default)]
    struct H {
        calls: usize,
        flags: Option<Flags>,
    }
    impl Handler for H {
        fn xt_checksum_extension(&mut self, v: XtChecksumExtension) {
            self.calls += 1;
            self.flags = Some(v.flags);
        }
    }

    struct Case {
        input: &'static [u8],
        flags: Option<Flags>,
    }
    let cases = [
        Case {
            input: b"\x1b[#y",
            flags: Some(Flags::default()),
        },
        Case {
            input: b"\x1b[0#y",
            flags: Some(Flags::default()),
        },
        Case {
            input: b"\x1b[5#y",
            flags: Some(Flags::default().with_positive(true).with_no_trim(true)),
        },
        Case {
            input: b"\x1b[48#y",
            flags: Some(Flags::default().with_full(true)),
        },
        Case {
            input: b"\x1b[1;2#y",
            flags: None,
        },
    ];

    for case in cases {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
        s.next_slice(case.input);
        if let Some(flags) = case.flags {
            assert_eq!(1, s.handler.calls);
            assert_eq!(flags, s.handler.flags.unwrap());
        } else {
            assert_eq!(0, s.handler.calls);
        }
    }
}

// test "stream: DECRQM dispatch" (oracle 9dc0d974e)
#[test]
fn stream_decrqm_dispatch() {
    #[derive(Default)]
    struct H {
        calls: usize,
        mode: Option<modes::Mode>,
        raw: Option<RawMode>,
    }
    impl Handler for H {
        fn request_mode(&mut self, v: Mode) {
            self.calls += 1;
            self.mode = Some(v.mode);
        }
        fn request_mode_unknown(&mut self, v: RawMode) {
            self.calls += 1;
            self.raw = Some(v);
        }
    }

    struct Case {
        input: &'static [u8],
        mode: Option<modes::Mode>,
        raw: Option<RawMode>,
    }
    const fn c(input: &'static [u8]) -> Case {
        Case {
            input,
            mode: None,
            raw: None,
        }
    }
    let cases = [
        Case {
            mode: Some(modes::Mode::Insert),
            ..c(b"\x1b[4$p")
        },
        Case {
            mode: Some(modes::Mode::SlowScroll),
            ..c(b"\x1b[?4$p")
        },
        Case {
            raw: Some(RawMode {
                mode: 9999,
                ansi: true,
            }),
            ..c(b"\x1b[9999$p")
        },
        Case {
            raw: Some(RawMode {
                mode: 9999,
                ansi: false,
            }),
            ..c(b"\x1b[?9999$p")
        },
        c(b"\x1b[4p"),
        c(b"\x1b[?4p"),
        c(b"\x1b[4!p"),
        c(b"\x1b[4 p"),
        c(b"\x1b[>4$p"),
        c(b"\x1b[?4!p"),
        c(b"\x1b[$p"),
        c(b"\x1b[?$p"),
        c(b"\x1b[4;20$p"),
        c(b"\x1b[?4;7$p"),
        c(b"\x1b[4:20$p"),
    ];
    for case in &cases {
        for split in 0..case.input.len() + 1 {
            let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
            s.next_slice(&case.input[..split]);
            if split < case.input.len() {
                assert_eq!(0, s.handler.calls);
            }
            s.next_slice(&case.input[split..]);
            assert_eq!(
                if case.mode.is_some() || case.raw.is_some() {
                    1usize
                } else {
                    0
                },
                s.handler.calls
            );
            assert_eq!(case.mode, s.handler.mode);
            assert_eq!(case.raw, s.handler.raw);
        }
    }
}

// test "stream: ansi set mode (SM) and reset mode (RM) with unknown value" { ... }
#[test]
fn stream_ansi_set_mode_sm_and_reset_mode_rm_with_unknown_value() {
    // PORT NOTE: the reference handler defines a `setMode` method that is
    // never invoked (the modern dispatch uses `vt`), and a no-op `vt`. The
    // mode `6` (DECCKM-adjacent value) used here is `origin`, which IS a
    // known mode, but the no-op handler ignores it, so `mode` stays null.
    struct H {
        mode: Option<modes::Mode>,
    }
    impl Handler for H {}

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { mode: None }));
    s.next_slice(b"\x1B[6h");
    assert!(s.handler.mode.is_none());

    s.next_slice(b"\x1B[6l");
    assert!(s.handler.mode.is_none());
}

// test "stream: restore mode" { ... }
#[test]
fn stream_restore_mode() {
    struct H {
        called: bool,
    }
    impl Handler for H {
        fn top_and_bottom_margin(&mut self, _v: Margin) {
            self.called = true;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
    for c in b"\x1B[?42r" {
        s.next(*c);
    }
    assert!(!s.handler.called);
}

// test "stream: pop kitty keyboard with no params defaults to 1" { ... }
#[test]
fn stream_pop_kitty_keyboard_with_no_params_defaults_to_1() {
    struct H {
        n: u16,
    }
    impl Handler for H {
        fn kitty_keyboard_pop(&mut self, v: u16) {
            self.n = v;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { n: 0 }));
    for c in b"\x1B[<u" {
        s.next(*c);
    }
    assert_eq!(1u16, s.handler.n);
}

// test "stream: DECSCA" { ... }
#[test]
fn stream_decsca() {
    struct H {
        v: Option<ansi::ProtectedMode>,
    }
    impl Handler for H {
        fn protected_mode_off(&mut self) {
            self.v = Some(ansi::ProtectedMode::Off);
        }
        fn protected_mode_iso(&mut self) {
            self.v = Some(ansi::ProtectedMode::Iso);
        }
        fn protected_mode_dec(&mut self) {
            self.v = Some(ansi::ProtectedMode::Dec);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { v: None }));
    {
        for c in b"\x1B[\"q" {
            s.next(*c);
        }
        assert_eq!(ansi::ProtectedMode::Off, s.handler.v.unwrap());
    }
    {
        for c in b"\x1B[0\"q" {
            s.next(*c);
        }
        assert_eq!(ansi::ProtectedMode::Off, s.handler.v.unwrap());
    }
    {
        for c in b"\x1B[2\"q" {
            s.next(*c);
        }
        assert_eq!(ansi::ProtectedMode::Off, s.handler.v.unwrap());
    }
    {
        for c in b"\x1B[1\"q" {
            s.next(*c);
        }
        assert_eq!(ansi::ProtectedMode::Dec, s.handler.v.unwrap());
    }
}

// test "stream: DECED, DECSED" { ... }
#[test]
fn stream_deced_decsed() {
    struct H {
        mode: Option<csi::EraseDisplay>,
        protected: Option<bool>,
    }
    impl Handler for H {
        fn erase_display_below(&mut self, protected: bool) {
            self.mode = Some(csi::EraseDisplay::Below);
            self.protected = Some(protected);
        }
        fn erase_display_above(&mut self, protected: bool) {
            self.mode = Some(csi::EraseDisplay::Above);
            self.protected = Some(protected);
        }
        fn erase_display_complete(&mut self, protected: bool) {
            self.mode = Some(csi::EraseDisplay::Complete);
            self.protected = Some(protected);
        }
        fn erase_display_scrollback(&mut self, protected: bool) {
            self.mode = Some(csi::EraseDisplay::Scrollback);
            self.protected = Some(protected);
        }
        fn erase_display_scroll_complete(&mut self, protected: bool) {
            self.mode = Some(csi::EraseDisplay::ScrollComplete);
            self.protected = Some(protected);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H {
        mode: None,
        protected: None,
    }));
    {
        for c in b"\x1B[?J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Below, s.handler.mode.unwrap());
        assert!(s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[?0J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Below, s.handler.mode.unwrap());
        assert!(s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[?1J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Above, s.handler.mode.unwrap());
        assert!(s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[?2J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Complete, s.handler.mode.unwrap());
        assert!(s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[?3J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Scrollback, s.handler.mode.unwrap());
        assert!(s.handler.protected.unwrap());
    }

    {
        for c in b"\x1B[J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Below, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[0J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Below, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[1J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Above, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[2J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Complete, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[3J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Scrollback, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
    {
        // Invalid and ignored by the handler
        for c in b"\x1B[>0J" {
            s.next(*c);
        }
        assert_eq!(csi::EraseDisplay::Scrollback, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
}

// test "stream: DECEL, DECSEL" { ... }
#[test]
fn stream_decel_decsel() {
    struct H {
        mode: Option<csi::EraseLine>,
        protected: Option<bool>,
    }
    impl Handler for H {
        fn erase_line_right(&mut self, protected: bool) {
            self.mode = Some(csi::EraseLine::Right);
            self.protected = Some(protected);
        }
        fn erase_line_left(&mut self, protected: bool) {
            self.mode = Some(csi::EraseLine::Left);
            self.protected = Some(protected);
        }
        fn erase_line_complete(&mut self, protected: bool) {
            self.mode = Some(csi::EraseLine::Complete);
            self.protected = Some(protected);
        }
        fn erase_line_right_unless_pending_wrap(&mut self, protected: bool) {
            self.mode = Some(csi::EraseLine::RightUnlessPendingWrap);
            self.protected = Some(protected);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H {
        mode: None,
        protected: None,
    }));
    {
        for c in b"\x1B[?K" {
            s.next(*c);
        }
        assert_eq!(csi::EraseLine::Right, s.handler.mode.unwrap());
        assert!(s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[?0K" {
            s.next(*c);
        }
        assert_eq!(csi::EraseLine::Right, s.handler.mode.unwrap());
        assert!(s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[?1K" {
            s.next(*c);
        }
        assert_eq!(csi::EraseLine::Left, s.handler.mode.unwrap());
        assert!(s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[?2K" {
            s.next(*c);
        }
        assert_eq!(csi::EraseLine::Complete, s.handler.mode.unwrap());
        assert!(s.handler.protected.unwrap());
    }

    {
        for c in b"\x1B[K" {
            s.next(*c);
        }
        assert_eq!(csi::EraseLine::Right, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[0K" {
            s.next(*c);
        }
        assert_eq!(csi::EraseLine::Right, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[1K" {
            s.next(*c);
        }
        assert_eq!(csi::EraseLine::Left, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
    {
        for c in b"\x1B[2K" {
            s.next(*c);
        }
        assert_eq!(csi::EraseLine::Complete, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
    {
        // Invalid and ignored by the handler
        for c in b"\x1B[<1K" {
            s.next(*c);
        }
        assert_eq!(csi::EraseLine::Complete, s.handler.mode.unwrap());
        assert!(!s.handler.protected.unwrap());
    }
}

// test "stream: DECSCUSR" { ... }
#[test]
fn stream_decscusr() {
    struct H {
        style: Option<ansi::CursorStyle>,
    }
    impl Handler for H {
        fn cursor_style(&mut self, v: ansi::CursorStyle) {
            self.style = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { style: None }));
    s.next_slice(b"\x1B[ q");
    assert!(s.handler.style.unwrap() == ansi::CursorStyle::Default);

    s.next_slice(b"\x1B[1 q");
    assert!(s.handler.style.unwrap() == ansi::CursorStyle::BlinkingBlock);

    // Invalid and ignored by the handler
    s.next_slice(b"\x1B[?0 q");
    assert!(s.handler.style.unwrap() == ansi::CursorStyle::BlinkingBlock);
}

// test "stream: DECSCUSR without space" { ... }
#[test]
fn stream_decscusr_without_space() {
    struct H {
        style: Option<ansi::CursorStyle>,
    }
    impl Handler for H {
        fn cursor_style(&mut self, v: ansi::CursorStyle) {
            self.style = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { style: None }));
    s.next_slice(b"\x1B[q");
    assert!(s.handler.style.is_none());

    s.next_slice(b"\x1B[1q");
    assert!(s.handler.style.is_none());
}

// test "stream: XTSHIFTESCAPE" { ... }
#[test]
fn stream_xtshiftescape() {
    struct H {
        escape: Option<bool>,
    }
    impl Handler for H {
        fn mouse_shift_capture(&mut self, capture: bool) {
            self.escape = Some(capture);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { escape: None }));
    s.next_slice(b"\x1B[>2s");
    assert!(s.handler.escape.is_none());

    s.next_slice(b"\x1B[>s");
    assert!(!s.handler.escape.unwrap());

    s.next_slice(b"\x1B[>0s");
    assert!(!s.handler.escape.unwrap());

    s.next_slice(b"\x1B[>1s");
    assert!(s.handler.escape.unwrap());

    // Invalid and ignored by the handler
    s.next_slice(b"\x1B[1 s");
    assert!(s.handler.escape.unwrap());
}

// test "stream: change window title with invalid utf-8" { ... }
#[test]
fn stream_change_window_title_with_invalid_utf8() {
    struct H {
        seen: bool,
    }
    impl Handler for H {
        fn window_title(&mut self, _v: WindowTitle) {
            self.seen = true;
        }
    }

    {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H { seen: false }));
        s.next_slice(b"\x1b]2;abc\x1b\\");
        assert!(s.handler.seen);
    }

    {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H { seen: false }));
        s.next_slice(b"\x1b]2;abc\xc0\x1b\\");
        assert!(!s.handler.seen);
    }
}

// test "stream: osc 52 large payload in chunks" { ... }
#[test]
fn stream_osc_52_large_payload_in_chunks() {
    struct H {
        data: Vec<u8>,
        kind: u8,
        // terminator: ?osc.Terminator = null,
        terminator: Option<osc::Terminator>,
        count: usize,
    }
    impl Handler for H {
        // .clipboard_contents => { ... }
        fn clipboard_contents(&mut self, v: ClipboardContents) {
            self.count += 1;
            self.kind = v.kind;
            // self.terminator = value.terminator;
            self.terminator = Some(v.terminator);
            // self.data.appendSlice(testing.allocator, value.data) catch @panic("OOM");
            self.data.extend_from_slice(v.data);
        }
    }

    // A payload that far exceeds the fixed buffer so the allocating
    // capture is used, fed in chunk sizes that don't align with the
    // sequence so the bulk path sees every kind of boundary.
    let prefix = b"\x1b]52;c;";
    let payload_len: usize = 150_000;
    let mut input: Vec<u8> = Vec::new();
    input.extend_from_slice(prefix);
    // for (0..payload_len) |i| try input.append(alloc, 'A' + @as(u8, @intCast(i % 26)));
    for i in 0..payload_len {
        input.push(b'A' + u8::try_from(i % 26).unwrap());
    }
    input.extend_from_slice(b"\x1b\\");

    // var s: Stream(H) = .init(.{ .handler = .{}, .allocator = alloc });
    let mut s: Stream<H> = Stream::init(Options {
        handler: H {
            data: Vec::new(),
            kind: 0,
            terminator: None,
            count: 0,
        },
        allocator: Some(testing_allocator()),
        continuation_max_bytes: None,
        osc_unknown_max_bytes: 0,
    });

    let mut i: usize = 0;
    while i < input.len() {
        // const end = @min(i + 4093, input.items.len);
        let end = (i + 4093).min(input.len());
        s.next_slice(&input[i..end]);
        i = end;
    }

    assert_eq!(1usize, s.handler.count);
    assert_eq!(b'c', s.handler.kind);
    // try testing.expectEqual(osc.Terminator.st, s.handler.terminator.?);
    assert_eq!(osc::Terminator::St, s.handler.terminator.unwrap());
    assert_eq!(
        &input[prefix.len()..prefix.len() + payload_len],
        s.handler.data.as_slice()
    );

    // defer s.parser.deinit();
    s.parser.deinit();
}

// test "stream: osc bulk path matches per-byte path" { ... }
#[test]
fn stream_osc_bulk_path_matches_per_byte_path() {
    // Records every dispatch relevant to OSC processing in a
    // normalized text form so streams fed different ways can be
    // compared byte-for-byte.
    //
    // PORT NOTE: the reference handler carries `alloc: Allocator` for
    // `std.fmt.allocPrint`; the ported journal is an owned `Vec<u8>`
    // built by direct byte appends (`{s}` prints RAW bytes — kept
    // byte-exact rather than routed through a lossy UTF-8 format).
    #[derive(Default)]
    struct H {
        journal: Vec<u8>,
    }
    impl H {
        // fn record(self: *Self, comptime fmt: []const u8, args: anytype) void { ... }
        fn record(&mut self, s: &[u8]) {
            self.journal.extend_from_slice(s);
        }
    }
    impl Handler for H {
        // .clipboard_contents => self.record(
        //     "clip kind={c} term={s} data={s}\n",
        //     .{ value.kind, @tagName(value.terminator), value.data },
        // ),
        fn clipboard_contents(&mut self, v: ClipboardContents) {
            let mut msg: Vec<u8> = Vec::new();
            msg.extend_from_slice(b"clip kind=");
            msg.push(v.kind);
            msg.extend_from_slice(b" term=");
            // @tagName(value.terminator)
            msg.extend_from_slice(match v.terminator {
                osc::Terminator::St => b"st".as_slice(),
                osc::Terminator::Bel => b"bel".as_slice(),
            });
            msg.extend_from_slice(b" data=");
            msg.extend_from_slice(v.data);
            msg.push(b'\n');
            self.record(&msg);
        }
        // .kitty_clipboard => self.record(
        //     "kitty meta={s} payload={?s}\n",
        //     .{ value.metadata, value.payload },
        // ),
        // (`{?s}` prints the literal "null" for a null payload.)
        fn kitty_clipboard(&mut self, v: osc::KittyClipboardProtocol) {
            let mut msg: Vec<u8> = Vec::new();
            msg.extend_from_slice(b"kitty meta=");
            msg.extend_from_slice(v.metadata);
            msg.extend_from_slice(b" payload=");
            match &v.payload {
                Some(p) => msg.extend_from_slice(p),
                None => msg.extend_from_slice(b"null"),
            }
            msg.push(b'\n');
            self.record(&msg);
        }
        //
        // .window_title => self.record("title {s}\n", .{value.title}),
        fn window_title(&mut self, v: WindowTitle) {
            let mut msg: Vec<u8> = Vec::new();
            msg.extend_from_slice(b"title ");
            msg.extend_from_slice(v.title);
            msg.push(b'\n');
            self.record(&msg);
        }
        // Normalize prints to per-codepoint so the per-byte
        // and slice paths journal identically.
        // .print => self.record("print {u}\n", .{value.cp}),
        fn print(&mut self, cp: u32) {
            let mut msg: Vec<u8> = Vec::new();
            msg.extend_from_slice(b"print ");
            let mut buf = [0u8; 4];
            msg.extend_from_slice(char::from_u32(cp).unwrap().encode_utf8(&mut buf).as_bytes());
            msg.push(b'\n');
            self.record(&msg);
        }
        // .print_slice => for (value.cps) |cp| self.record(
        //     "print {u}\n",
        //     .{@as(u21, @intCast(cp))},
        // ),
        fn print_slice(&mut self, cps: &[u32]) {
            for &cp in cps {
                self.print(cp);
            }
        }
    }

    // const cases = [_][]const u8{ ... };
    let mut cases: Vec<Vec<u8>> = vec![
        b"\x1b]52;c;aGVsbG8=\x1b\\".to_vec(),
        b"\x1b]52;c;aGVsbG8=\x07".to_vec(),
        b"\x1b]52;;aGVsbG8=\x07".to_vec(),
        // Ignored C0 byte embedded in the payload.
        b"\x1b]52;c;aGVs\x01bG8=\x07".to_vec(),
        // CAN and SUB aborts.
        b"\x1b]52;c;aGVsbG8=\x18".to_vec(),
        b"\x1b]52;c;aGVsbG8=\x1a".to_vec(),
        // C1 byte embedded in the payload is data.
        b"\x1b]0;ab\x9ccd\x07".to_vec(),
        // Terminated with trailing printable text.
        b"\x1b]0;a title\x07x".to_vec(),
        // Back-to-back sequences.
        b"\x1b]2;another title\x1b\\\x1b]0;t2\x07".to_vec(),
        b"\x1b]5522;type=write;aGVsbG8=\x1b\\".to_vec(),
        // Invalid OSC number.
        b"\x1b]999;junk\x07".to_vec(),
    ];
    // Exceeds the fixed buffer: allocating capture.
    // "\x1b]52;c;" ++ "y" ** 3000 ++ "\x1b\\",
    {
        let mut c = b"\x1b]52;c;".to_vec();
        c.extend(std::iter::repeat(b'y').take(3000));
        c.extend_from_slice(b"\x1b\\");
        cases.push(c);
    }
    // Exceeds the fixed buffer: overflow, no dispatch.
    // "\x1b]0;" ++ "x" ** 3000 ++ "\x07",
    {
        let mut c = b"\x1b]0;".to_vec();
        c.extend(std::iter::repeat(b'x').take(3000));
        c.extend_from_slice(b"\x07");
        cases.push(c);
    }

    for case in &cases {
        // Reference: byte-at-a-time.
        let mut r: Stream<H> = Stream::init(Options {
            handler: H::default(),
            allocator: Some(testing_allocator()),
            continuation_max_bytes: None,
            osc_unknown_max_bytes: 0,
        });
        for &c in case.iter() {
            r.next(c);
        }

        // The whole slice at once.
        {
            let mut s: Stream<H> = Stream::init(Options {
                handler: H::default(),
                allocator: Some(testing_allocator()),
                continuation_max_bytes: None,
                osc_unknown_max_bytes: 0,
            });
            s.next_slice(case);
            // try testing.expectEqualSlices(u8, ref.handler.journal.items, s.handler.journal.items);
            assert_eq!(r.handler.journal, s.handler.journal);
            // defer s.parser.deinit();
            s.parser.deinit();
        }

        // Split into two slices at every possible boundary.
        for split in 0..=case.len() {
            let mut s: Stream<H> = Stream::init(Options {
                handler: H::default(),
                allocator: Some(testing_allocator()),
                continuation_max_bytes: None,
                osc_unknown_max_bytes: 0,
            });
            s.next_slice(&case[..split]);
            s.next_slice(&case[split..]);
            assert_eq!(r.handler.journal, s.handler.journal);
            // defer s.parser.deinit();
            s.parser.deinit();
        }

        // defer ref.parser.deinit();
        r.parser.deinit();
    }
}

// test "stream: insert characters" { ... }
#[test]
fn stream_insert_characters() {
    struct H {
        called: bool,
    }
    impl Handler for H {
        fn insert_blanks(&mut self, _count: usize) {
            self.called = true;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
    for c in b"\x1B[42@" {
        s.next(*c);
    }
    assert!(s.handler.called);

    s.handler.called = false;
    for c in b"\x1B[?42@" {
        s.next(*c);
    }
    assert!(!s.handler.called);
}

// test "stream: insert characters explicit zero clamps to 1" { ... }
#[test]
fn stream_insert_characters_explicit_zero_clamps_to_1() {
    struct H {
        value: Option<usize>,
    }
    impl Handler for H {
        fn insert_blanks(&mut self, count: usize) {
            self.value = Some(count);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { value: None }));
    for c in b"\x1B[0@" {
        s.next(*c);
    }
    assert_eq!(1usize, s.handler.value.unwrap());
}

// test "stream: SCOSC" { ... }
#[test]
fn stream_scosc() {
    struct H {
        called: bool,
    }
    impl Handler for H {
        fn left_and_right_margin(&mut self, _v: Margin) {
            panic!("bad");
        }
        fn left_and_right_margin_ambiguous(&mut self) {
            self.called = true;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
    for c in b"\x1B[s" {
        s.next(*c);
    }
    assert!(s.handler.called);
}

// test "stream: SCORC" { ... }
#[test]
fn stream_scorc() {
    struct H {
        called: bool,
    }
    impl Handler for H {
        fn restore_cursor(&mut self) {
            self.called = true;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
    for c in b"\x1B[u" {
        s.next(*c);
    }
    assert!(s.handler.called);
}

// test "stream: too many csi params" { ... }
#[test]
fn stream_too_many_csi_params() {
    struct H {}
    impl Handler for H {
        fn cursor_right(&mut self, _v: CursorMovement) {
            unreachable!();
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H {}));
    s.next_slice(b"\x1B[1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1C");
}

// test "stream: csi param too long" { ... }
#[test]
fn stream_csi_param_too_long() {
    struct H {}
    impl Handler for H {}

    let mut s: Stream<H> = Stream::init(Options::with_handler(H {}));
    s.next_slice(b"\x1B[1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111C");
}

// test "stream: send report with CSI t" { ... }
#[test]
fn stream_send_report_with_csi_t() {
    struct H {
        style: Option<csi::SizeReportStyle>,
    }
    impl Handler for H {
        fn size_report(&mut self, v: csi::SizeReportStyle) {
            self.style = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { style: None }));

    s.next_slice(b"\x1b[14t");
    assert_eq!(Some(csi::SizeReportStyle::Csi14T), s.handler.style);

    s.next_slice(b"\x1b[16t");
    assert_eq!(Some(csi::SizeReportStyle::Csi16T), s.handler.style);

    s.next_slice(b"\x1b[18t");
    assert_eq!(Some(csi::SizeReportStyle::Csi18T), s.handler.style);

    s.next_slice(b"\x1b[21t");
    assert_eq!(Some(csi::SizeReportStyle::Csi21T), s.handler.style);
}

// test "stream: CSI 8 t resize window" { ... }
#[test]
fn stream_csi_8_t_resize_window() {
    struct H {
        size: Option<ResizeWindow>,
    }
    impl Handler for H {
        fn resize_window(&mut self, v: ResizeWindow) {
            self.size = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { size: None }));

    s.next_slice(b"\x1b[8;40;120t");
    assert_eq!(40, s.handler.size.unwrap().rows);
    assert_eq!(120, s.handler.size.unwrap().columns);

    // Omitted parameters keep the current size
    s.next_slice(b"\x1b[8;;100t");
    assert_eq!(0, s.handler.size.unwrap().rows);
    assert_eq!(100, s.handler.size.unwrap().columns);

    s.next_slice(b"\x1b[8;30t");
    assert_eq!(30, s.handler.size.unwrap().rows);
    assert_eq!(0, s.handler.size.unwrap().columns);

    s.next_slice(b"\x1b[8t");
    assert_eq!(0, s.handler.size.unwrap().rows);
    assert_eq!(0, s.handler.size.unwrap().columns);

    // Extra parameters are invalid
    s.handler.size = None;
    s.next_slice(b"\x1b[8;30;100;1t");
    assert!(s.handler.size.is_none());
}

// test "stream: invalid CSI t" { ... }
#[test]
fn stream_invalid_csi_t() {
    // PORT NOTE: the reference handler defines an (unused) `sendSizeReport`
    // method and a no-op `vt`; CSI 19 t is unimplemented so nothing fires.
    struct H {
        style: Option<csi::SizeReportStyle>,
    }
    impl Handler for H {}

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { style: None }));

    s.next_slice(b"\x1b[19t");
    assert_eq!(None, s.handler.style);
}

// test "stream: CSI t push title" { ... }
#[test]
fn stream_csi_t_push_title() {
    struct H {
        index: Option<u16>,
    }
    impl Handler for H {
        fn title_push(&mut self, v: u16) {
            self.index = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

    s.next_slice(b"\x1b[22;0t");
    assert_eq!(0u16, s.handler.index.unwrap());
}

// test "stream: CSI t push title with explicit window" { ... }
#[test]
fn stream_csi_t_push_title_with_explicit_window() {
    struct H {
        index: Option<u16>,
    }
    impl Handler for H {
        fn title_push(&mut self, v: u16) {
            self.index = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

    s.next_slice(b"\x1b[22;2t");
    assert_eq!(0u16, s.handler.index.unwrap());
}

// test "stream: CSI t push title with explicit icon" { ... }
#[test]
fn stream_csi_t_push_title_with_explicit_icon() {
    struct H {
        index: Option<u16>,
    }
    impl Handler for H {
        fn title_push(&mut self, v: u16) {
            self.index = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

    s.next_slice(b"\x1b[22;1t");
    assert_eq!(None, s.handler.index);
}

// test "stream: CSI t push title with index" { ... }
#[test]
fn stream_csi_t_push_title_with_index() {
    struct H {
        index: Option<u16>,
    }
    impl Handler for H {
        fn title_push(&mut self, v: u16) {
            self.index = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

    s.next_slice(b"\x1b[22;0;5t");
    assert_eq!(5u16, s.handler.index.unwrap());
}

// test "stream: CSI t pop title" { ... }
#[test]
fn stream_csi_t_pop_title() {
    struct H {
        index: Option<u16>,
    }
    impl Handler for H {
        fn title_pop(&mut self, v: u16) {
            self.index = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

    s.next_slice(b"\x1b[23;0t");
    assert_eq!(0u16, s.handler.index.unwrap());
}

// test "stream: CSI t pop title with explicit window" { ... }
#[test]
fn stream_csi_t_pop_title_with_explicit_window() {
    struct H {
        index: Option<u16>,
    }
    impl Handler for H {
        fn title_pop(&mut self, v: u16) {
            self.index = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

    s.next_slice(b"\x1b[23;2t");
    assert_eq!(0u16, s.handler.index.unwrap());
}

// test "stream: CSI t pop title with explicit icon" { ... }
#[test]
fn stream_csi_t_pop_title_with_explicit_icon() {
    struct H {
        index: Option<u16>,
    }
    impl Handler for H {
        fn title_pop(&mut self, v: u16) {
            self.index = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

    s.next_slice(b"\x1b[23;1t");
    assert_eq!(None, s.handler.index);
}

// test "stream: CSI t pop title with index" { ... }
#[test]
fn stream_csi_t_pop_title_with_index() {
    struct H {
        index: Option<u16>,
    }
    impl Handler for H {
        fn title_pop(&mut self, v: u16) {
            self.index = Some(v);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { index: None }));

    s.next_slice(b"\x1b[23;0;5t");
    assert_eq!(5u16, s.handler.index.unwrap());
}

// PORT NOTE: the next three tests record `?Action.Key` — *which* action
// fired. The Rust port has no `Action.Key` tag enum (Action == Handler
// trait), so each test declares a local `Observed` enum mirroring exactly
// the `Action.Key` variants those tests can witness, and the handler sets
// the field from whichever overridden method runs.

// test "stream CSI W clear tab stops" { ... }
#[test]
fn stream_csi_w_clear_tab_stops() {
    #[derive(PartialEq, Eq, Debug)]
    #[allow(clippy::enum_variant_names)]
    enum Observed {
        TabClearCurrent,
        TabClearAll,
        TabSet,
        TabReset,
    }
    struct H {
        action: Option<Observed>,
    }
    impl Handler for H {
        fn tab_clear_current(&mut self) {
            self.action = Some(Observed::TabClearCurrent);
        }
        fn tab_clear_all(&mut self) {
            self.action = Some(Observed::TabClearAll);
        }
        fn tab_set(&mut self) {
            self.action = Some(Observed::TabSet);
        }
        fn tab_reset(&mut self) {
            self.action = Some(Observed::TabReset);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { action: None }));

    s.next_slice(b"\x1b[2W");
    assert_eq!(
        Observed::TabClearCurrent,
        *s.handler.action.as_ref().unwrap()
    );

    s.next_slice(b"\x1b[5W");
    assert_eq!(Observed::TabClearAll, *s.handler.action.as_ref().unwrap());
}

// test "stream CSI W tab set" { ... }
#[test]
fn stream_csi_w_tab_set() {
    #[derive(PartialEq, Eq, Debug)]
    #[allow(clippy::enum_variant_names)]
    enum Observed {
        TabClearCurrent,
        TabClearAll,
        TabSet,
        TabReset,
    }
    struct H {
        action: Option<Observed>,
    }
    impl Handler for H {
        fn tab_clear_current(&mut self) {
            self.action = Some(Observed::TabClearCurrent);
        }
        fn tab_clear_all(&mut self) {
            self.action = Some(Observed::TabClearAll);
        }
        fn tab_set(&mut self) {
            self.action = Some(Observed::TabSet);
        }
        fn tab_reset(&mut self) {
            self.action = Some(Observed::TabReset);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { action: None }));

    s.next_slice(b"\x1b[W");
    assert_eq!(Observed::TabSet, *s.handler.action.as_ref().unwrap());

    s.handler.action = None;
    s.next_slice(b"\x1b[0W");
    assert_eq!(Observed::TabSet, *s.handler.action.as_ref().unwrap());

    s.handler.action = None;
    s.next_slice(b"\x1b[>W");
    assert!(s.handler.action.is_none());

    s.handler.action = None;
    s.next_slice(b"\x1b[99W");
    assert!(s.handler.action.is_none());
}

// test "stream CSI ? W reset tab stops" { ... }
#[test]
fn stream_csi_question_w_reset_tab_stops() {
    #[derive(PartialEq, Eq, Debug)]
    #[allow(clippy::enum_variant_names)]
    enum Observed {
        TabClearCurrent,
        TabClearAll,
        TabSet,
        TabReset,
    }
    struct H {
        action: Option<Observed>,
    }
    impl Handler for H {
        fn tab_clear_current(&mut self) {
            self.action = Some(Observed::TabClearCurrent);
        }
        fn tab_clear_all(&mut self) {
            self.action = Some(Observed::TabClearAll);
        }
        fn tab_set(&mut self) {
            self.action = Some(Observed::TabSet);
        }
        fn tab_reset(&mut self) {
            self.action = Some(Observed::TabReset);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { action: None }));

    s.next_slice(b"\x1b[?2W");
    assert!(s.handler.action.is_none());

    s.next_slice(b"\x1b[?5W");
    assert_eq!(Observed::TabReset, *s.handler.action.as_ref().unwrap());

    // Invalid and ignored by the handler
    s.handler.action = None;
    s.next_slice(b"\x1b[?1;2;3W");
    assert!(s.handler.action.is_none());
}

// test "stream: SGR with 17+ parameters for underline color" { ... }
#[test]
fn stream_sgr_with_17_plus_parameters_for_underline_color() {
    // PORT NOTE: `sgr::Attribute<'a>` borrows; the reference stores `attrs`
    // but only asserts `called`. We record `called` only (the stored attr
    // is observationally unused in the assertion).
    struct H {
        called: bool,
    }
    impl Handler for H {
        fn set_attribute(&mut self, _attr: sgr::Attribute) {
            self.called = true;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));

    // Kakoune-style SGR with underline color as 17th parameter
    // This tests the fix where param 17 was being dropped
    s.next_slice(b"\x1b[4:3;38;2;51;51;51;48;2;170;170;170;58;2;255;97;136;0m");
    assert!(s.handler.called);
}

// test "stream: tab clear with overflowing param" { ... }
#[test]
fn stream_tab_clear_with_overflowing_param() {
    // Regression test for a fuzz crash: CSI with a parameter value that
    // saturates to 65535 (u16 max) causes @enumFromInt to panic when
    // converting to TabClear (enum(u8)).
    struct H {
        called: bool,
    }
    impl Handler for H {
        fn tab_clear_current(&mut self) {
            self.called = true;
        }
        fn tab_clear_all(&mut self) {
            self.called = true;
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { called: false }));
    // This is the exact input from the fuzz crash (minus the mode byte):
    // CSI with a huge numeric param that saturates to 65535, followed by 'g'.
    s.next_slice(b"\x1b[388888888888888888888888888888888888g\x1b[0m");
}
