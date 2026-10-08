//! PORTS: ghostty src/terminal/osc/parsers/unknown.zig
//!
//! PORT STATUS: ported — `parse` + the 13 `OSC unknown:` tests (7b11f3dca,
//! end state after 520d8f55a), plus a Rust-only huge-sequence test (P8 Review
//! Focus 3).

use crate::osc::{Command, Parser, State, Terminator, Unknown};

/// Build `Command.unknown` from the bytes collected for a sequence whose
/// number the parser does not implement. Only reached when
/// `Parser.unknown_max_bytes` is nonzero.
// PORT NOTE (N263): the command borrows the capture's bytes (the fixed
// `Parser.buffer` or the allocating capture), as every `Command<'p>` does.
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<Command<'a>> {
    // assert(parser.state == .unknown or parser.state == .unknown_truncated);
    assert!(parser.state == State::Unknown || parser.state == State::UnknownTruncated);

    let truncated = parser.state == State::UnknownTruncated;
    // const cap = &parser.capture.?;
    let cap = parser
        .capture
        .as_ref()
        .expect("unknown states always have an active capture");
    Some(Command::Unknown(Unknown {
        content: cap.trailing(&parser.buffer),
        truncated,
        terminator: Terminator::init(terminator_ch),
    }))
}

#[cfg(test)]
mod tests {
    use crate::alloc::testing::{with_handle, Config, CountingAllocator, FailingAllocator};
    use crate::osc::{CaptureBacking, Command, Parser, State, Terminator, Unknown, MAX_BUF};

    /// Every `Parser.State`, in declaration order (the reference iterates
    /// `@typeInfo(Parser.State).@"enum".fields`).
    // PORT NOTE: Rust has no enum reflection. The list is checked against the
    // state machine below: every state reachable by feeding digits must be in
    // it, so a new prefix state cannot be left out silently.
    const ALL_STATES: &[State] = &[
        State::Start,
        State::Invalid,
        State::Unknown,
        State::UnknownTruncated,
        State::S0,
        State::S1,
        State::S2,
        State::S3,
        State::S4,
        State::S5,
        State::S6,
        State::S7,
        State::S8,
        State::S9,
        State::S30,
        State::S300,
        State::S3008,
        State::S10,
        State::S11,
        State::S12,
        State::S13,
        State::S14,
        State::S15,
        State::S16,
        State::S17,
        State::S18,
        State::S19,
        State::S21,
        State::S22,
        State::S52,
        State::S55,
        State::S66,
        State::S72,
        State::S75,
        State::S750,
        State::S7501,
        State::S77,
        State::S99,
        State::S104,
        State::S105,
        State::S110,
        State::S111,
        State::S112,
        State::S113,
        State::S114,
        State::S115,
        State::S116,
        State::S117,
        State::S118,
        State::S119,
        State::S133,
        State::S552,
        State::S777,
        State::S1337,
        State::S5522,
    ];

    fn unknown(cmd: Command<'_>) -> Unknown<'_> {
        match cmd {
            Command::Unknown(v) => v,
            other => panic!("expected unknown, got {other:?}"),
        }
    }

    fn is_fixed(p: &Parser) -> bool {
        matches!(
            p.capture.as_ref().unwrap().backing,
            CaptureBacking::Fixed(_)
        )
    }

    fn is_allocating(p: &Parser) -> bool {
        matches!(
            p.capture.as_ref().unwrap().backing,
            CaptureBacking::Allocating(_)
        )
    }

    /// `p.capture.?.writer.buffer.len`: the allocating writer's backing
    /// buffer length, which is the Vec's capacity here.
    fn allocating_capacity(p: &Parser) -> usize {
        match &p.capture.as_ref().unwrap().backing {
            CaptureBacking::Allocating(w) => w.capacity(),
            CaptureBacking::Fixed(_) => panic!("expected an allocating capture"),
        }
    }

    fn trailing_len(p: &Parser) -> usize {
        p.capture.as_ref().unwrap().trailing(&p.buffer).len()
    }

    // test "OSC unknown: prefix state names are the bytes consumed"
    #[test]
    fn osc_unknown_prefix_state_names_are_the_bytes_consumed() {
        // beginUnknown recovers the consumed identifier from the prefix state's
        // name, so every prefix state must be reachable by feeding its name.
        for &state in ALL_STATES {
            match state {
                State::Start | State::Invalid | State::Unknown | State::UnknownTruncated => {}
                _ => {
                    let mut p = Parser::init(None);
                    for &ch in state.prefix() {
                        p.next(ch);
                    }
                    assert_eq!(state, p.state, "prefix {:?}", state.prefix());
                    p.deinit();
                }
            }
        }

        // PORT NOTE: the converse, which the reference gets from reflection:
        // every prefix state the machine reaches on digits is in the list, and
        // its table entry is exactly the digits that reached it.
        let mut frontier: Vec<Vec<u8>> = vec![Vec::new()];
        let mut reached = 0usize;
        while let Some(path) = frontier.pop() {
            for d in b'0'..=b'9' {
                let mut next = path.clone();
                next.push(d);
                let mut p = Parser::init(None);
                for &ch in &next {
                    p.next(ch);
                }
                if p.state != State::Invalid {
                    assert!(p.capture.is_none());
                    assert!(ALL_STATES.contains(&p.state), "{:?} is not listed", p.state);
                    assert_eq!(next.as_slice(), p.state.prefix());
                    reached += 1;
                    frontier.push(next);
                }
                p.deinit();
            }
        }
        // start, invalid, unknown and unknown_truncated are not prefixes.
        assert_eq!(ALL_STATES.len() - 4, reached);
    }

    // test "OSC unknown: disabled by default"
    #[test]
    fn osc_unknown_disabled_by_default() {
        let mut p = Parser::init(Some(crate::alloc::Global));

        assert_eq!(0usize, p.unknown_max_bytes);
        p.next_slice(b"7400;status=busy");
        assert_eq!(State::Invalid, p.state);
        assert!(p.capture.is_none());
        assert!(p.end(Some(0x07)).is_none());

        // A sequence ending on a partial number is dropped too.
        p.reset();
        p.next_slice(b"77");
        assert!(p.end(Some(0x07)).is_none());
        p.deinit();
    }

    // test "OSC unknown: reported"
    #[test]
    fn osc_unknown_reported() {
        struct Case {
            input: &'static [u8],
            terminator_ch: Option<u8>,
            terminator: Terminator,
        }
        // terminator_ch: ?u8 = 0x1B, terminator: osc.Terminator = .st,
        const fn c(input: &'static [u8]) -> Case {
            Case {
                input,
                terminator_ch: Some(0x1B),
                terminator: Terminator::St,
            }
        }
        let cases = [
            // Unsupported number with a body.
            c(b"7400;status=busy"),
            // Unsupported number with no body, BEL terminated.
            Case {
                input: b"7400",
                terminator_ch: Some(0x07),
                terminator: Terminator::Bel,
            },
            // Bridge states that only lead to longer identifiers.
            c(b"3;x"),
            c(b"30;x"),
            c(b"300;x"),
            c(b"55;x"),
            c(b"552;x"),
            c(b"6;x"),
            c(b"77;x"),
            // Bridge states with no body end on the prefix itself.
            c(b"3"),
            c(b"300"),
            Case {
                input: b"77",
                terminator_ch: Some(0x07),
                terminator: Terminator::Bel,
            },
            // Known number extended by a digit.
            c(b"1338;x"),
            c(b"1339"),
            // Leading zero is not a known number.
            c(b"0133;x"),
            // Non-numeric identifier.
            c(b"I;name"),
            // Known number followed by junk before ';'.
            c(b"133 x"),
            // Empty identifier.
            c(b";x"),
            // A null terminator is treated as ST.
            Case {
                input: b"7400;x",
                terminator_ch: None,
                terminator: Terminator::St,
            },
        ];

        for case in &cases {
            let mut p = Parser::init(Some(crate::alloc::Global));
            p.unknown_max_bytes = 64;

            p.next_slice(case.input);
            let cmd = unknown(p.end(case.terminator_ch).unwrap());
            assert_eq!(case.input, cmd.content);
            assert!(!cmd.truncated);
            assert_eq!(case.terminator, cmd.terminator);
            p.deinit();
        }
    }

    // test "OSC unknown: supported commands are unchanged"
    #[test]
    fn osc_unknown_supported_commands_are_unchanged() {
        let mut p = Parser::init(Some(crate::alloc::Global));
        p.unknown_max_bytes = 64;

        // A supported command still parses as itself.
        p.next_slice(b"0;title");
        match p.end(Some(0x07)).unwrap() {
            Command::ChangeWindowTitle(title) => assert_eq!(b"title", title.as_bytes()),
            other => panic!("expected change_window_title, got {other:?}"),
        }

        // A known number with a malformed body is not reported as unknown.
        p.reset();
        p.next_slice(b"9;4;9");
        if let Some(c) = p.end(Some(0x07)) {
            assert!(!matches!(c, Command::Unknown(_)));
        }

        // A known number with an unsupported sub-command is not reported.
        p.reset();
        p.next_slice(b"1337;SetUserVar=foo=YmFy");
        if let Some(c) = p.end(Some(0x07)) {
            assert!(!matches!(c, Command::Unknown(_)));
        }

        // A known number without a body is not reported.
        p.reset();
        p.next_slice(b"7");
        if let Some(c) = p.end(Some(0x07)) {
            assert!(!matches!(c, Command::Unknown(_)));
        }

        // An empty OSC has no identifier and is not reported.
        p.reset();
        assert!(p.end(Some(0x07)).is_none());
        p.deinit();
    }

    // test "OSC unknown: aborted sequences are dropped"
    #[test]
    fn osc_unknown_aborted_sequences_are_dropped() {
        // std.ascii.control_code.can / .sub
        const CAN: u8 = 0x18;
        const SUB: u8 = 0x1A;

        let mut p = Parser::init(Some(crate::alloc::Global));
        p.unknown_max_bytes = 64;

        p.next_slice(b"7400;x");
        assert!(p.end(Some(CAN)).is_none());

        p.reset();
        p.next_slice(b"7400;x");
        assert!(p.end(Some(SUB)).is_none());

        // Bridge prefixes are dropped on abort too.
        p.reset();
        p.next_slice(b"77");
        assert!(p.end(Some(CAN)).is_none());
        p.deinit();
    }

    // test "OSC unknown: truncated at the limit"
    #[test]
    fn osc_unknown_truncated_at_the_limit() {
        let mut p = Parser::init(Some(crate::alloc::Global));
        p.unknown_max_bytes = 8;

        // Exactly at the limit is not truncated.
        p.next_slice(b"7400;abc");
        let cmd = unknown(p.end(Some(0x07)).unwrap());
        assert_eq!(b"7400;abc", cmd.content);
        assert!(!cmd.truncated);

        // One byte over is truncated but still reported.
        p.reset();
        p.next_slice(b"7400;abcd");
        let cmd = unknown(p.end(Some(0x07)).unwrap());
        assert_eq!(b"7400;abc", cmd.content);
        assert!(cmd.truncated);

        // Later input is discarded, both by slice and by byte.
        p.reset();
        p.next_slice(b"7400;abcdef");
        p.next_slice(b"ghi");
        p.next(b'j');
        let cmd = unknown(p.end(Some(0x07)).unwrap());
        assert_eq!(b"7400;abc", cmd.content);
        assert!(cmd.truncated);
        p.deinit();
    }

    // test "OSC unknown: limit smaller than the identifier"
    #[test]
    fn osc_unknown_limit_smaller_than_the_identifier() {
        let mut p = Parser::init(Some(crate::alloc::Global));
        p.unknown_max_bytes = 2;

        // The prefix itself overflows while beginning the capture.
        p.next_slice(b"7400;x");
        let cmd = unknown(p.end(Some(0x07)).unwrap());
        assert_eq!(b"74", cmd.content);
        assert!(cmd.truncated);

        // The prefix fits but the unrecognized byte does not.
        p.reset();
        p.next_slice(b"77x");
        let cmd = unknown(p.end(Some(0x07)).unwrap());
        assert_eq!(b"77", cmd.content);
        assert!(cmd.truncated);

        // A bridge prefix longer than the limit at end.
        p.reset();
        p.unknown_max_bytes = 1;
        p.next_slice(b"77");
        let cmd = unknown(p.end(Some(0x07)).unwrap());
        assert_eq!(b"7", cmd.content);
        assert!(cmd.truncated);
        p.deinit();
    }

    // test "OSC unknown: fixed tier does not allocate"
    #[test]
    fn osc_unknown_fixed_tier_does_not_allocate() {
        // testing.failing_allocator
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 0,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            p.unknown_max_bytes = MAX_BUF;

            p.next_slice(b"7400;");
            assert!(is_fixed(&p));
            assert_eq!(MAX_BUF, p.capture.as_ref().unwrap().max_bytes);
            p.next_slice(b"x");
            let cmd = unknown(p.end(Some(0x07)).unwrap());
            assert_eq!(b"7400;x", cmd.content);
            p.deinit();
        });
        // PORT NOTE: the reference's failing allocator would fail the test
        // with an error if anything tried to allocate; here the attempt is
        // counted.
        assert!(!failing.has_induced_failure.get());
    }

    // test "OSC unknown: allocating tier"
    #[test]
    fn osc_unknown_allocating_tier() {
        let limit = MAX_BUF * 2;

        // PORT NOTE: upstream's `testing.allocator` fails the test on a leak;
        // the counting allocator's live bytes are checked after `deinit`.
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            p.unknown_max_bytes = limit;

            p.next_slice(b"7400;");
            assert!(is_allocating(&p));

            let body = vec![b'a'; limit];

            // Fill to exactly the limit, including the 5 identifier bytes.
            p.next_slice(&body[..limit - 5]);
            assert_eq!(State::Unknown, p.state);
            assert_eq!(limit, allocating_capacity(&p));

            // One more byte truncates without growing the allocation.
            p.next_slice(b"a");
            assert_eq!(State::UnknownTruncated, p.state);
            assert_eq!(limit, allocating_capacity(&p));

            let cmd = unknown(p.end(Some(0x07)).unwrap());
            assert!(cmd.truncated);
            assert_eq!(limit, cmd.content.len());
            assert!(cmd.content.starts_with(b"7400;aaa"));
            p.deinit();
        });
        assert!(counting.alloc_calls.get() > 0);
        assert_eq!(0, counting.live_bytes.get());
    }

    // test "OSC unknown: large limit without allocator uses fixed buffer"
    #[test]
    fn osc_unknown_large_limit_without_allocator_uses_fixed_buffer() {
        let mut p = Parser::init(None);
        p.unknown_max_bytes = MAX_BUF * 2;

        p.next_slice(b"7400;");
        assert!(is_fixed(&p));

        let body = [b'a'; MAX_BUF];
        p.next_slice(&body);

        let cmd = unknown(p.end(Some(0x07)).unwrap());
        assert!(cmd.truncated);
        assert_eq!(MAX_BUF, cmd.content.len());
        p.deinit();
    }

    // test "OSC unknown: allocation failure falls back to fixed buffer"
    #[test]
    fn osc_unknown_allocation_failure_falls_back_to_fixed_buffer() {
        // testing.failing_allocator
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 0,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            p.unknown_max_bytes = MAX_BUF * 2;

            p.next_slice(b"7400;x");
            assert!(is_fixed(&p));
            let cmd = unknown(p.end(Some(0x07)).unwrap());
            assert_eq!(b"7400;x", cmd.content);
            p.deinit();
        });
    }

    // test "OSC unknown: allocation failure while growing truncates"
    #[test]
    fn osc_unknown_allocation_failure_while_growing_truncates() {
        // Allow the initial capacity allocation and fail the first growth,
        // whether it is attempted in place or by a new allocation.
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 1,
            resize_fail_index: 0,
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            p.unknown_max_bytes = MAX_BUF * 2;

            p.next_slice(b"7400;");
            assert!(is_allocating(&p));

            let body = [b'a'; MAX_BUF];
            p.next_slice(&body);
            assert_eq!(State::UnknownTruncated, p.state);
            let retained = trailing_len(&p);
            assert!(retained <= MAX_BUF);

            // Later bytes must not be appended after the gap, even if an
            // allocation would now succeed.
            failing.fail_index.set(usize::MAX);
            failing.resize_fail_index.set(usize::MAX);
            p.next_slice(b"zz");
            p.next(b'z');

            let cmd = unknown(p.end(Some(0x07)).unwrap());
            assert!(cmd.truncated);
            assert_eq!(retained, cmd.content.len());
            assert!(!cmd.content.contains(&b'z'));
            p.deinit();
        });
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
    }

    // test "OSC unknown: nextSlice matches per-byte parsing"
    #[test]
    fn osc_unknown_next_slice_matches_per_byte_parsing() {
        let inputs: [&[u8]; 5] = [
            b"7400;status=busy:jobs=4",
            b"I;name",
            b"1338;x",
            b"77",
            b"7400;abcdefghijklmnop",
        ];

        for input in inputs {
            // Per-byte reference.
            let mut r = Parser::init(Some(crate::alloc::Global));
            r.unknown_max_bytes = 12;
            for &ch in input {
                r.next(ch);
            }
            let expected = unknown(r.end(Some(0x07)).unwrap());
            let (expected_content, expected_truncated) =
                (expected.content.to_vec(), expected.truncated);

            // Every two-way split of the input must parse identically.
            for split in 0..input.len() + 1 {
                let mut p = Parser::init(Some(crate::alloc::Global));
                p.unknown_max_bytes = 12;
                p.next_slice(&input[..split]);
                p.next_slice(&input[split..]);

                let cmd = p.end(Some(0x07)).unwrap();
                assert!(matches!(cmd, Command::Unknown(_)));
                let cmd = unknown(cmd);
                assert_eq!(expected_content.as_slice(), cmd.content);
                assert_eq!(expected_truncated, cmd.truncated);
                p.deinit();
            }
            r.deinit();
        }
    }

    // Rust-only (P8 Review Focus 3): a huge unknown OSC. A 1 MiB payload fed
    // in 4 KiB slices with capture at 4 KiB (the allocating tier) truncates
    // at the limit, never grows the capture past it, and leaves the parser
    // ready for the next OSC. With capture disabled it costs nothing.
    //
    // PORT NOTE: the review names "OSC 1337 ; File=" as the payload, but OSC
    // 1337 is a supported number (iTerm2), so its body is never captured as
    // unknown (upstream's `OSC unknown: supported commands are unchanged`
    // asserts a supported number is never reported as unknown, even with an
    // unsupported body). Its 1 MiB body overflows the fixed buffer and the
    // sequence is dropped (`.invalid`), in Ghostty and in Rustty alike. That
    // was an error in the review's example, not a defect, so no finding is
    // recorded. The unknown leg uses an unimplemented number with the same
    // body; the 1337 leg asserts the supported path stays bounded and
    // allocation free.
    #[test]
    fn osc_unknown_huge_sequence_is_bounded_at_the_limit() {
        const LIMIT: usize = 4096;
        const SLICE: usize = 4096;
        const TOTAL: usize = 1024 * 1024;
        let slice = [b'A'; SLICE];

        // Feed `prefix` + 1 MiB of 'A' + BEL, then `0;title` + BEL. Returns
        // the unknown command's (content length, truncated, all 'A' after
        // the prefix), or None if the sequence was not reported.
        fn run(
            p: &mut Parser,
            prefix: &[u8],
            slice: &[u8],
            check: &dyn Fn(&Parser),
        ) -> Option<(usize, bool, bool)> {
            p.next_slice(prefix);
            for _ in 0..TOTAL / SLICE {
                p.next_slice(slice);
                check(p);
            }
            let got = match p.end(Some(0x07)) {
                Some(Command::Unknown(u)) => Some((
                    u.content.len(),
                    u.truncated,
                    u.content[prefix.len()..].iter().all(|&b| b == b'A'),
                )),
                Some(other) => panic!("expected unknown or nothing, got {other:?}"),
                None => None,
            };

            // The next supported OSC parses normally.
            p.reset();
            p.next_slice(b"0;title");
            match p.end(Some(0x07)) {
                Some(Command::ChangeWindowTitle(t)) => assert_eq!(b"title", t.as_bytes()),
                other => panic!("expected change_window_title, got {other:?}"),
            }
            p.reset();
            got
        }

        let prefix: &[u8] = b"7400;File=";
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            p.set_unknown_max_bytes(LIMIT);
            let got = run(&mut p, prefix, &slice, &|p: &Parser| {
                if let Some(cap) = p.capture.as_ref() {
                    if let CaptureBacking::Allocating(w) = &cap.backing {
                        assert!(w.capacity() <= LIMIT, "capacity {}", w.capacity());
                    }
                }
            });
            // content holds the prefix and then `LIMIT - prefix.len()` bytes
            // of the body.
            assert_eq!(Some((LIMIT, true, true)), got);
            p.deinit();
        });
        assert!(
            counting.peak_bytes.get() <= LIMIT,
            "{}",
            counting.peak_bytes.get()
        );
        assert_eq!(0, counting.live_bytes.get());

        // Capture disabled: the same input allocates nothing and reports
        // nothing.
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            assert_eq!(
                None,
                run(&mut p, prefix, &slice, &|p: &Parser| {
                    assert_eq!(State::Invalid, p.state);
                })
            );
            p.deinit();
        });
        assert_eq!(0, counting.alloc_calls.get());

        // OSC 1337 is supported: with capture enabled its 1 MiB body still
        // overflows the fixed buffer, is dropped, and allocates nothing.
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            p.set_unknown_max_bytes(LIMIT);
            assert_eq!(None, run(&mut p, b"1337;File=", &slice, &|_: &Parser| {}));
            p.deinit();
        });
        assert_eq!(0, counting.alloc_calls.get());
    }
}
