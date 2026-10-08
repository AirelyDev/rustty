//! stream_terminal tests: Kitty clipboard (OSC 5522), Kitty dnd (OSC 72),
//! paste, clipboard_read and mode-2048.

use super::*;

// ==== Phase-5 dispatch tier: Kitty clipboard (OSC 5522), Kitty dnd
// ==== (OSC 72), paste, clipboard_read, and mode-2048 tests.

/// Shared capture state for the Kitty clipboard (OSC 5522) tests below:
/// records every pty response and the most recent clipboard write.
// const KittyClipboardCapture = struct { ... };
// PORT NOTE: the reference hangs `var`s on a file-scope struct; the
// callbacks are bare `fn` pointers here, so the capture lives in a
// thread-local cell (the module's established idiom). The reference's
// fixed `[8][64]u8`-style capture buffers become owned Vecs.
#[derive(Default)]
struct KittyCap {
    responses: Vec<u8>,

    // Write capture. A None write_result returns without replying.
    write_count: usize,
    write_result: Option<clipboard::WriteResult>,
    write_reply_twice: bool,
    last_location: Option<clipboard::Location>,
    last_contents: Vec<(Vec<u8>, Vec<u8>)>,
    last_write_name: Vec<u8>,
    last_write_granted: bool,
    last_write_can_remember: bool,

    // Read capture. A None read_result returns without replying.
    read_count: usize,
    read_result: Option<KcReadResult>,
    read_reply_twice: bool,
    last_read_location: Option<clipboard::Location>,
    last_read_mimes: Vec<Vec<u8>>,
    last_read_list: bool,
    last_read_name: Vec<u8>,
    last_read_granted: bool,
    last_read_can_remember: bool,
}

/// Owned analog of `clipboard.Read.Result` for the thread-local capture
/// (the real result's memory is only valid during the reply call).
#[derive(Clone)]
enum KcReadResult {
    Denied,
    Unsupported,
    Busy,
    IoError,
    Success {
        contents: Vec<(Vec<u8>, Vec<u8>)>,
        available: Vec<Vec<u8>>,
        remember: bool,
    },
}

impl KcReadResult {
    /// Build the borrowed reply and deliver it.
    fn reply_with(&self, handler: &mut Handler, read: &clipboard::Read) {
        match self {
            KcReadResult::Denied => read.reply(handler, clipboard::ReadResult::Denied),
            KcReadResult::Unsupported => read.reply(handler, clipboard::ReadResult::Unsupported),
            KcReadResult::Busy => read.reply(handler, clipboard::ReadResult::Busy),
            KcReadResult::IoError => read.reply(handler, clipboard::ReadResult::IoError),
            KcReadResult::Success {
                contents,
                available,
                remember,
            } => {
                let contents_b: Vec<clipboard::Content> = contents
                    .iter()
                    .map(|(mime, data)| clipboard::Content { mime, data })
                    .collect();
                let available_b: Vec<&[u8]> = available.iter().map(|a| a.as_slice()).collect();
                read.reply(
                    handler,
                    clipboard::ReadResult::Success(clipboard::ReadResultSuccess {
                        contents: &contents_b,
                        available: &available_b,
                        remember: *remember,
                    }),
                );
            }
        }
    }
}

thread_local! {
    static KC: core::cell::RefCell<KittyCap> =
        core::cell::RefCell::new(KittyCap::default());
}

// fn reset() void { ... }
fn kc_reset() {
    KC.with(|c| {
        *c.borrow_mut() = KittyCap {
            write_result: Some(clipboard::WriteResult::Success(
                clipboard::WriteResultSuccess::default(),
            )),
            ..KittyCap::default()
        };
    });
}

// fn writePty(_: *Handler, data: []const u8) void { ... }
fn kc_write_pty(_: &mut Handler, data: &[u8]) {
    KC.with(|c| c.borrow_mut().responses.extend_from_slice(data));
}

// fn clipboardWrite(_: *Handler, write: clipboard.Write) void { ... }
fn kc_clipboard_write(handler: &mut Handler, write: clipboard::Write) {
    let (result, twice) = KC.with(|c| {
        let mut cap = c.borrow_mut();
        cap.write_count += 1;
        cap.last_location = Some(write.location);
        cap.last_contents = write
            .contents
            .iter()
            .map(|content| (content.mime.to_vec(), content.data.to_vec()))
            .collect();
        cap.last_write_name = write.name.to_vec();
        cap.last_write_granted = write.granted;
        cap.last_write_can_remember = write.can_remember;
        (cap.write_result, cap.write_reply_twice)
    });
    if let Some(r) = result {
        write.reply(handler, r);
    }
    if twice {
        write.reply(handler, clipboard::WriteResult::IoError);
    }
}

// fn clipboardRead(_: *Handler, read: clipboard.Read) void { ... }
fn kc_clipboard_read(handler: &mut Handler, read: clipboard::Read) {
    let (result, twice) = KC.with(|c| {
        let mut cap = c.borrow_mut();
        cap.read_count += 1;
        cap.last_read_location = Some(read.location);
        cap.last_read_mimes = read.mimes.iter().map(|m| m.to_vec()).collect();
        cap.last_read_list = read.list;
        cap.last_read_name = read.name.to_vec();
        cap.last_read_granted = read.granted;
        cap.last_read_can_remember = read.can_remember;
        (cap.read_result.clone(), cap.read_reply_twice)
    });
    if let Some(r) = &result {
        r.reply_with(handler, &read);
    }
    if twice {
        read.reply(handler, clipboard::ReadResult::Denied);
    }
}

fn kc_responses() -> Vec<u8> {
    KC.with(|c| c.borrow().responses.clone())
}

fn kc_responses_len() -> usize {
    KC.with(|c| c.borrow().responses.len())
}

fn kc<T>(f: impl FnOnce(&KittyCap) -> T) -> T {
    KC.with(|c| f(&c.borrow()))
}

fn kc_mut(f: impl FnOnce(&mut KittyCap)) {
    KC.with(|c| f(&mut c.borrow_mut()));
}

// Parity tail S7-4 (routing evidence, no reference counterpart): the
// stream handler's allocations go to the terminal's `gpa()`, which here is
// a counting C allocator. Every OSC 52 / 5522 sequence also takes the OSC
// parser's allocating capture from the stream's allocator, which
// `vt_stream` sets to `gpa()` as the reference does (`CAPTURE` calls of
// `MAX_BUF` bytes each). OSC 52's decode scratch is one exact-sized
// allocation; a Kitty clipboard write transaction (its state, entries,
// MIME copies, spool and the committed contents) and the per-packet
// metadata allocate through it and return every byte; a remembered grant
// is held until RIS frees it; a color response that outgrows its
// 1024-byte stack buffer spills through it, while a small one allocates
// nothing.
#[test]
fn stream_handler_internals_route_through_the_allocator_handle() {
    use crate::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    crate::alloc::testing::with_handle(&c, |alloc| {
        let calls = || counting.alloc_calls.get();
        let frees = || counting.free_calls.get();
        let live = || counting.live_bytes.get();

        let mut t = Terminal::init(alloc, opts(80, 24)).unwrap();
        kc_reset();
        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(kc_write_pty);
            s.handler.effects.clipboard_write = Some(kc_clipboard_write);
            s.handler.effects.clipboard_read = Some(kc_clipboard_read);
            let (a0, f0, live0) = (calls(), frees(), live());

            // OSC 52: `alloc.alloc(u8, simd.base64.maxLen(data))`, freed
            // after the write effect.
            const CAPTURE: usize = 1;
            s.next_slice(b"\x1B]52;c;aGVsbG8=\x1B\\");
            assert_eq!(1, kc(|c| c.write_count));
            assert_eq!(
                a0 + CAPTURE + 1,
                calls(),
                "OSC 52 decode scratch: one allocation"
            );
            assert_eq!(
                Some((rustty_simd::base64::max_len(b"aGVsbG8="), 0)),
                counting.last_alloc.get(),
                "the scratch is maxLen(data) bytes"
            );
            // The capture lives until the next OSC starts (or the stream
            // is freed); the scratch is freed.
            let capture_bytes = crate::osc::MAX_BUF;
            assert_eq!(
                (f0 + 1, live0 + capture_bytes),
                (frees(), live()),
                "the scratch is freed"
            );
            let live0 = live();

            // Kitty clipboard write: `alloc.create(WriteState)` (the
            // empty id/pw/name dupes allocate nothing).
            let a1 = calls();
            s.next_slice(b"\x1B]5522;type=write\x1B\\");
            assert_eq!(a1 + CAPTURE + 1, calls(), "alloc.create(WriteState)");
            assert_eq!(
                Some((core::mem::size_of::<kitty::clipboard::WriteState>(), 3)),
                counting.last_alloc.get()
            );
            assert_eq!(
                live0 + core::mem::size_of::<kitty::clipboard::WriteState>(),
                live()
            );

            // A wdata packet: the decoded mime metadata value (freed with
            // the packet), the entries list, the entry's mime dupe and the
            // spool.
            let a2 = calls();
            s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
            assert_eq!(
                a2 + CAPTURE + 4,
                calls(),
                "metadata mime + entries + mime dupe + spool"
            );

            // Commit: the committed contents, then the whole transaction
            // is freed.
            let a3 = calls();
            s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
            assert_eq!(2, kc(|c| c.write_count));
            assert_eq!(
                a3 + CAPTURE + 1,
                calls(),
                "commit: contents.toOwnedSlice(alloc)"
            );
            assert_eq!(live0, live(), "the transaction returns every byte");
            assert_eq!(
                calls() - a0,
                frees() - f0 + CAPTURE,
                "only the capture is live"
            );

            // A remembered read grant (pw + name) is recorded through
            // gpa() and held until RIS.
            kc_mut(|c| {
                c.read_result = Some(KcReadResult::Success {
                    contents: vec![(b"text/plain".to_vec(), b"x".to_vec())],
                    available: vec![b"text/plain".to_vec()],
                    remember: true,
                })
            });
            s.next_slice(b"\x1b]5522;type=read:pw=cHc=:name=YXBw;dGV4dC9wbGFpbg==\x1b\\");
            assert_eq!(1, s.handler.kitty_clipboard_grants.len());
            // The grant's entries list and its `dupe(pw)` ("pw", 2 bytes).
            let grant_bytes = live() - live0;
            assert!(
                grant_bytes > 2,
                "the grant holds its list and password copy"
            );
            let (held, f_ris) = (live(), frees());
            s.next_slice(b"\x1Bc");
            assert_eq!(0, s.handler.kitty_clipboard_grants.len());
            assert!(
                frees() >= f_ris + 2,
                "RIS frees the grant's list and password"
            );
            assert!(
                live() <= held - grant_bytes,
                "RIS frees the grant through gpa()"
            );

            // Color responses: a small one stays on the stack buffer.
            // (The OSC 4 start frees the last OSC's capture; OSC 4 itself
            // takes none.)
            let a4 = calls();
            s.next_slice(b"\x1B]4;0;?\x1B\\");
            assert_eq!(a4, calls(), "a small response allocates nothing");
            let live0 = live();
            let mut queries = b"\x1B]4".to_vec();
            for i in 0..64 {
                queries.extend_from_slice(format!(";{i};?").as_bytes());
            }
            queries.extend_from_slice(b"\x1B\\");
            let a5 = calls();
            s.next_slice(&queries);
            assert!(
                calls() > a5,
                "a response past 1024 bytes spills through gpa()"
            );
            // (Parity tail S7-7: the 64 requests' list stays with the OSC
            // parser, the reference's `parser.command`, until the next
            // OSC's `reset`; an OSC 104 with one index allocates nothing
            // and frees it.)
            s.next_slice(b"\x1B]104;1\x1B\\");
            assert_eq!(live0, live(), "and frees the spill");

            s.deinit();
        }
        t.deinit(alloc);
        assert_eq!(0, live(), "deinit returns every byte");
    });
}

// Parity tail S7-4b (routing evidence, no reference counterpart): each
// `stackFallback(N, self.terminal.gpa())` response writer keeps a small
// response on its stack buffer and spills a large one through `gpa()`,
// freeing the spill before the handler returns. Each pair runs the same
// code path, so the large response costs exactly the spill on top of the
// small one's allocations: the OSC 52 read reply (256), the Kitty
// clipboard DATA reply (1024), the CSI 21 t title report (256), the Kitty
// color report (1024) and the OSC 72 data serving (512).
#[test]
fn response_writers_spill_through_gpa() {
    use crate::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    crate::alloc::testing::with_handle(&c, |alloc| {
        let calls = || counting.alloc_calls.get();
        let live = || counting.live_bytes.get();
        let mut t = Terminal::init(alloc, opts(80, 24)).unwrap();
        kc_reset();
        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(kc_write_pty);
            s.handler.effects.clipboard_read = Some(kc_clipboard_read);
            // The allocations one input makes, and that the live bytes
            // come back to where they were once its response is written.
            let cost = |s: &mut Stream<'_>, input: &[u8]| -> usize {
                // (An OSC frees the previous OSC's capture when it starts;
                // run an unrecognized OSC first so both inputs start
                // alike.)
                s.next_slice(b"\x1B]99999;x\x1B\\");
                let (a, l) = (calls(), live());
                s.next_slice(input);
                let spent = calls() - a;
                // A capture (if any) stays until the next OSC starts.
                s.next_slice(b"\x1B]99999;x\x1B\\");
                assert!(live() <= l + crate::osc::MAX_BUF, "the spill is freed");
                spent
            };
            let read_reply = |data: Vec<u8>| {
                kc_mut(|c| {
                    c.read_result = Some(KcReadResult::Success {
                        contents: vec![(b"text/plain".to_vec(), data)],
                        available: vec![b"text/plain".to_vec()],
                        remember: false,
                    })
                })
            };

            // OSC 52 read reply: 256 bytes on the stack.
            read_reply(b"x".to_vec());
            let small = cost(&mut s, b"\x1B]52;c;?\x1B\\");
            read_reply(vec![b'x'; 300]);
            let large = cost(&mut s, b"\x1B]52;c;?\x1B\\");
            assert_eq!(small + 1, large, "OSC 52 read reply spills once");

            // Kitty clipboard DATA reply: 1024 bytes on the stack.
            read_reply(b"x".to_vec());
            let small = cost(&mut s, b"\x1B]5522;type=read;dGV4dC9wbGFpbg==\x1B\\");
            read_reply(vec![b'x'; 1000]);
            let large = cost(&mut s, b"\x1B]5522;type=read;dGV4dC9wbGFpbg==\x1B\\");
            assert_eq!(small + 1, large, "Kitty DATA reply spills once");

            // CSI 21 t: 256 bytes on the stack (the title is set first).
            s.handler.title_report = true;
            s.next_slice(b"\x1B]2;t\x1B\\");
            let small = cost(&mut s, b"\x1B[21t");
            let mut title = b"\x1B]2;".to_vec();
            title.extend(std::iter::repeat_n(b'T', 300));
            title.extend_from_slice(b"\x1B\\");
            s.next_slice(&title);
            let large = cost(&mut s, b"\x1B[21t");
            assert_eq!(small + 1, large, "the title report spills once");

            // Kitty color report: 1024 bytes on the stack.
            let small = cost(&mut s, b"\x1B]21;0=?\x1B\\");
            let mut query = b"\x1B]21".to_vec();
            for i in 0..100 {
                query.extend_from_slice(format!(";{i}=?").as_bytes());
            }
            query.extend_from_slice(b"\x1B\\");
            let large = cost(&mut s, &query);
            assert_eq!(small + 1, large, "the Kitty color report spills once");

            // OSC 72 data serving: 512 bytes on the stack.
            s.next_slice(b"\x1B]72;t=a;text/plain\x1B\\");
            let serve = |s: &mut Stream<'_>, data: &[u8]| {
                let mut aw: Vec<u8> = Vec::new();
                // SAFETY: the stream's own terminal pointer (derived from
                // the `&mut t` that `vt_stream` took, so it stays valid
                // while the stream lives); no other borrow of its DnD slot
                // is live. (A pointer taken from `&mut t` before
                // `vt_stream` would be invalidated by that call's retag.)
                unsafe { (*s.handler.terminal).kitty_dnd.as_mut() }
                    .unwrap()
                    .drag_drop(
                        alloc,
                        &mut aw,
                        kitty::dnd::MoveEvent {
                            cell_x: 0,
                            cell_y: 0,
                            pixel_x: 0,
                            pixel_y: 0,
                            operations: kitty::dnd::Operations {
                                copy: true,
                                r#move: false,
                            },
                        },
                        &[kitty::dnd::Item {
                            mime: b"text/plain".to_vec(),
                            data: data.to_vec(),
                        }],
                    )
                    .unwrap();
            };
            serve(&mut s, b"x");
            let small = cost(&mut s, b"\x1B]72;t=r:x=1\x1B\\");
            serve(&mut s, &[b'Z'; 600]);
            let large = cost(&mut s, b"\x1B]72;t=r:x=1\x1B\\");
            assert_eq!(small + 1, large, "OSC 72 data serving spills once");

            s.deinit();
        }
        t.deinit(alloc);
        assert_eq!(0, live(), "deinit returns every byte");
    });
}

// test "kitty clipboard write transaction round trip"
#[test]
fn kitty_clipboard_write_transaction_round_trip() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    // Begin a write, stream two MIME types (one chunked), alias the
    // plain text, and commit. Only the commit produces a response.
    s.next_slice(b"\x1B]5522;type=write:id=42\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\"); // "Ghost"
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;dHk=\x1B\\"); // "ty"
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9odG1s;PGI+aGk8L2I+\x1B\\"); // "<b>hi</b>"
                                                                                // Alias "TEXT UTF8_STRING" -> text/plain.
    s.next_slice(b"\x1B]5522;type=walias:mime=dGV4dC9wbGFpbg==;VEVYVCBVVEY4X1NUUklORw==\x1B\\");
    assert_eq!(0, kc(|c| c.write_count));
    assert_eq!(0, kc_responses_len());

    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(1, kc(|c| c.write_count));
    assert_eq!(Some(clipboard::Location::Standard), kc(|c| c.last_location));
    assert_eq!(4, kc(|c| c.last_contents.len()));
    // PORT NOTE (R2): the aliased text payload decodes to "Ghostty" in the
    // reference; the bytes are protocol data, carried verbatim.
    kc(|c| {
        assert_eq!(b"text/plain".as_slice(), c.last_contents[0].0.as_slice());
        assert_eq!(b"Ghostty".as_slice(), c.last_contents[0].1.as_slice());
        assert_eq!(b"text/html".as_slice(), c.last_contents[1].0.as_slice());
        assert_eq!(b"<b>hi</b>".as_slice(), c.last_contents[1].1.as_slice());
        assert_eq!(b"TEXT".as_slice(), c.last_contents[2].0.as_slice());
        assert_eq!(b"Ghostty".as_slice(), c.last_contents[2].1.as_slice());
        assert_eq!(b"UTF8_STRING".as_slice(), c.last_contents[3].0.as_slice());
        assert_eq!(b"Ghostty".as_slice(), c.last_contents[3].1.as_slice());
    });
    assert_eq!(
        b"\x1B]5522;type=write:status=DONE:id=42\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    // A commit with no transaction in flight is silently ignored.
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(1, kc(|c| c.write_count));

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard write result maps to response status"
#[test]
fn kitty_clipboard_write_result_maps_to_response_status() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    let cases: &[(Option<clipboard::WriteResult>, &[u8])] = &[
        (
            Some(clipboard::WriteResult::Success(
                clipboard::WriteResultSuccess::default(),
            )),
            b"\x1B]5522;type=write:status=DONE\x1B\\",
        ),
        (
            Some(clipboard::WriteResult::Denied),
            b"\x1B]5522;type=write:status=EPERM\x1B\\",
        ),
        (
            Some(clipboard::WriteResult::Unsupported),
            b"\x1B]5522;type=write:status=ENOSYS\x1B\\",
        ),
        (
            Some(clipboard::WriteResult::Busy),
            b"\x1B]5522;type=write:status=EBUSY\x1B\\",
        ),
        (
            Some(clipboard::WriteResult::InvalidData),
            b"\x1B]5522;type=write:status=EINVAL\x1B\\",
        ),
        (
            Some(clipboard::WriteResult::IoError),
            b"\x1B]5522;type=write:status=EIO\x1B\\",
        ),
        // No reply at all is a denial rather than silence.
        (None, b"\x1B]5522;type=write:status=EPERM\x1B\\"),
    ];

    for (result, response) in cases {
        kc_reset();
        kc_mut(|c| c.write_result = *result);

        // An immediately-committed write with no data is a clear.
        s.next_slice(b"\x1B]5522;type=write\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(1, kc(|c| c.write_count));
        assert_eq!(0, kc(|c| c.last_contents.len()));
        assert_eq!(*response, kc_responses().as_slice());
    }

    // A second reply is ignored.
    kc_reset();
    kc_mut(|c| c.write_reply_twice = true);
    s.next_slice(b"\x1B]5522;type=write\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(
        b"\x1B]5522;type=write:status=DONE\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    // The response echoes the request terminator, unlike kitty which
    // always uses ST.
    kc_reset();
    s.next_slice(b"\x1B]5522;type=write:loc=primary\x07");
    s.next_slice(b"\x1B]5522;type=wdata\x07");
    assert_eq!(Some(clipboard::Location::Primary), kc(|c| c.last_location));
    assert_eq!(
        b"\x1B]5522;type=write:status=DONE\x07".as_slice(),
        kc_responses().as_slice(),
    );

    // A system without a primary selection answers a loc=primary write
    // with ENOSYS, echoing the id.
    kc_reset();
    kc_mut(|c| c.write_result = Some(clipboard::WriteResult::Unsupported));
    s.next_slice(b"\x1B]5522;type=write:loc=primary:id=p1\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(Some(clipboard::Location::Primary), kc(|c| c.last_location));
    assert_eq!(
        b"\x1B]5522;type=write:status=ENOSYS:id=p1\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard write without clipboard effect responds ENOSYS"
#[test]
fn kitty_clipboard_write_without_clipboard_effect_responds_enosys() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);

    // The transaction fails as soon as it begins; the rest of it is
    // ignored without further responses.
    s.next_slice(b"\x1B]5522;type=write:id=x\x1B\\");
    assert_eq!(
        b"\x1B]5522;type=write:status=ENOSYS:id=x\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(
        b"\x1B]5522;type=write:status=ENOSYS:id=x\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard read without effect is denied with EPERM"
#[test]
fn kitty_clipboard_read_without_effect_is_denied_with_eperm() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    // The denial never includes loc (only OK responses do) and echoes
    // the sanitized id.
    s.next_slice(b"\x1B]5522;type=read:loc=primary:id=*4 2*;dGV4dC9wbGFpbg==\x1B\\");
    assert_eq!(
        b"\x1B]5522;type=read:status=EPERM:id=42\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    // A missing payload is an empty MIME list, still answered.
    kc_reset();
    s.next_slice(b"\x1B]5522;type=read\x07");
    assert_eq!(
        b"\x1B]5522;type=read:status=EPERM\x07".as_slice(),
        kc_responses().as_slice(),
    );

    // An undecodable payload is dropped without a response.
    kc_reset();
    s.next_slice(b"\x1B]5522;type=read;!!!\x1B\\");
    assert_eq!(0, kc_responses_len());

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard read round trip"
#[test]
fn kitty_clipboard_read_round_trip() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_read = Some(kc_clipboard_read);

    kc_mut(|c| {
        c.read_result = Some(KcReadResult::Success {
            contents: vec![
                // Unrequested representations are never served, and the
                // served ones follow request order, not reply order.
                (b"image/png".to_vec(), b"\x89PNG".to_vec()),
                (b"text/html".to_vec(), b"<b>hi</b>".to_vec()),
                (b"text/plain".to_vec(), b"Ghostty".to_vec()),
            ],
            available: vec![b"text/plain".to_vec(), b"text/html".to_vec()],
            remember: false,
        });
    });

    // Request the targets listing plus two types from the primary
    // selection: ". text/plain text/html".
    s.next_slice(b"\x1B]5522;type=read:loc=primary:id=r1;LiB0ZXh0L3BsYWluIHRleHQvaHRtbA==\x1B\\");
    assert_eq!(1, kc(|c| c.read_count));
    assert_eq!(
        Some(clipboard::Location::Primary),
        kc(|c| c.last_read_location)
    );
    assert_eq!(2, kc(|c| c.last_read_mimes.len()));
    kc(|c| {
        assert_eq!(b"text/plain".as_slice(), c.last_read_mimes[0].as_slice());
        assert_eq!(b"text/html".as_slice(), c.last_read_mimes[1].as_slice());
    });
    assert!(kc(|c| c.last_read_list));
    assert_eq!(0, kc(|c| c.last_read_name.len()));
    assert!(!kc(|c| c.last_read_granted));
    assert!(!kc(|c| c.last_read_can_remember));
    let mut expected: Vec<u8> = Vec::new();
    expected.extend_from_slice(b"\x1B]5522;type=read:status=OK:loc=primary:id=r1\x1B\\");
    expected.extend_from_slice(
        b"\x1B]5522;type=read:status=DATA:id=r1:mime=Lg==;dGV4dC9wbGFpbiB0ZXh0L2h0bWwK\x1B\\",
    );
    expected.extend_from_slice(
        b"\x1B]5522;type=read:status=DATA:id=r1:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==\x1B\\",
    );
    expected.extend_from_slice(
        b"\x1B]5522;type=read:status=DATA:id=r1:mime=dGV4dC9odG1s;PGI+aGk8L2I+\x1B\\",
    );
    expected.extend_from_slice(b"\x1B]5522;type=read:status=DONE:id=r1\x1B\\");
    assert_eq!(expected, kc_responses());

    // Without the listing request `available` is ignored. The response
    // echoes the request terminator.
    kc_mut(|c| c.responses.clear());
    s.next_slice(b"\x1B]5522;type=read:id=r2;dGV4dC9wbGFpbg==\x07");
    assert_eq!(
        Some(clipboard::Location::Standard),
        kc(|c| c.last_read_location)
    );
    assert_eq!(1, kc(|c| c.last_read_mimes.len()));
    assert!(!kc(|c| c.last_read_list));
    let mut expected: Vec<u8> = Vec::new();
    expected.extend_from_slice(b"\x1B]5522;type=read:status=OK:id=r2\x07");
    expected.extend_from_slice(
        b"\x1B]5522;type=read:status=DATA:id=r2:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==\x07",
    );
    expected.extend_from_slice(b"\x1B]5522;type=read:status=DONE:id=r2\x07");
    assert_eq!(expected, kc_responses());

    // A listing-only request carries no types.
    kc_mut(|c| c.responses.clear());
    s.next_slice(b"\x1B]5522;type=read;Lg==\x1B\\");
    assert_eq!(0, kc(|c| c.last_read_mimes.len()));
    assert!(kc(|c| c.last_read_list));
    let mut expected: Vec<u8> = Vec::new();
    expected.extend_from_slice(b"\x1B]5522;type=read:status=OK\x1B\\");
    expected.extend_from_slice(
        b"\x1B]5522;type=read:status=DATA:mime=Lg==;dGV4dC9wbGFpbiB0ZXh0L2h0bWwK\x1B\\",
    );
    expected.extend_from_slice(b"\x1B]5522;type=read:status=DONE\x1B\\");
    assert_eq!(expected, kc_responses());

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard read result maps to response status"
#[test]
fn kitty_clipboard_read_result_maps_to_response_status() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_read = Some(kc_clipboard_read);

    let mut ok_done: Vec<u8> = Vec::new();
    ok_done.extend_from_slice(b"\x1B]5522;type=read:status=OK:id=x\x1B\\");
    ok_done.extend_from_slice(b"\x1B]5522;type=read:status=DONE:id=x\x1B\\");

    let cases: &[(Option<KcReadResult>, &[u8])] = &[
        (
            Some(KcReadResult::Denied),
            b"\x1B]5522;type=read:status=EPERM:id=x\x1B\\",
        ),
        (
            Some(KcReadResult::Unsupported),
            b"\x1B]5522;type=read:status=ENOSYS:id=x\x1B\\",
        ),
        (
            Some(KcReadResult::Busy),
            b"\x1B]5522;type=read:status=EBUSY:id=x\x1B\\",
        ),
        (
            Some(KcReadResult::IoError),
            b"\x1B]5522;type=read:status=EIO:id=x\x1B\\",
        ),
        // No reply at all is a denial rather than silence.
        (None, b"\x1B]5522;type=read:status=EPERM:id=x\x1B\\"),
        // A success with nothing to serve is still OK then DONE.
        (
            Some(KcReadResult::Success {
                contents: Vec::new(),
                available: Vec::new(),
                remember: false,
            }),
            ok_done.as_slice(),
        ),
    ];

    for (result, response) in cases {
        kc_reset();
        kc_mut(|c| c.read_result = result.clone());
        s.next_slice(b"\x1B]5522;type=read:id=x;dGV4dC9wbGFpbg==\x1B\\");
        assert_eq!(1, kc(|c| c.read_count));
        assert_eq!(*response, kc_responses().as_slice());
    }

    // A second reply is ignored.
    kc_reset();
    kc_mut(|c| {
        c.read_result = Some(KcReadResult::Success {
            contents: vec![(b"text/plain".to_vec(), b"hello".to_vec())],
            available: Vec::new(),
            remember: false,
        });
        c.read_reply_twice = true;
    });
    s.next_slice(b"\x1B]5522;type=read;dGV4dC9wbGFpbg==\x1B\\");
    let mut expected: Vec<u8> = Vec::new();
    expected.extend_from_slice(b"\x1B]5522;type=read:status=OK\x1B\\");
    expected
        .extend_from_slice(b"\x1B]5522;type=read:status=DATA:mime=dGV4dC9wbGFpbg==;aGVsbG8=\x1B\\");
    expected.extend_from_slice(b"\x1B]5522;type=read:status=DONE\x1B\\");
    assert_eq!(expected, kc_responses());

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard read caps requested types"
#[test]
fn kitty_clipboard_read_caps_requested_types() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_read = Some(kc_clipboard_read);

    // "a/0 a/1 a/2 a/3 a/4 a/5 .": extras are dropped but the listing
    // request after them still counts.
    kc_mut(|c| {
        c.read_result = Some(KcReadResult::Success {
            contents: Vec::new(),
            available: Vec::new(),
            remember: false,
        });
    });
    s.next_slice(b"\x1B]5522;type=read;YS8wIGEvMSBhLzIgYS8zIGEvNCBhLzUgLg==\x1B\\");
    assert_eq!(1, kc(|c| c.read_count));
    assert_eq!(
        kitty::clipboard::max_read_mimes,
        kc(|c| c.last_read_mimes.len())
    );
    kc(|c| {
        assert_eq!(b"a/0".as_slice(), c.last_read_mimes[0].as_slice());
        assert_eq!(
            b"a/3".as_slice(),
            c.last_read_mimes[kitty::clipboard::max_read_mimes - 1].as_slice()
        );
    });
    assert!(kc(|c| c.last_read_list));

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard read password grants"
#[test]
fn kitty_clipboard_read_password_grants() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_read = Some(kc_clipboard_read);

    let success_remember = KcReadResult::Success {
        contents: Vec::new(),
        available: Vec::new(),
        remember: true,
    };
    let success_plain = KcReadResult::Success {
        contents: Vec::new(),
        available: Vec::new(),
        remember: false,
    };

    // Every read requests a data type ("text/plain"): a request with
    // no data types never consults the grants at all.
    //
    // pw="secret", name="app": the first request isn't granted but the
    // reply may ask to remember it.
    kc_mut(|c| c.read_result = Some(success_remember.clone()));
    s.next_slice(b"\x1B]5522;type=read:pw=c2VjcmV0:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
    assert_eq!(1, kc(|c| c.read_count));
    kc(|c| assert_eq!(b"app".as_slice(), c.last_read_name.as_slice()));
    assert!(!kc(|c| c.last_read_granted));
    assert!(kc(|c| c.last_read_can_remember));

    // The same password is now granted; a different one is not.
    kc_mut(|c| c.read_result = Some(success_plain.clone()));
    s.next_slice(b"\x1B]5522;type=read:pw=c2VjcmV0:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
    assert!(kc(|c| c.last_read_granted));
    s.next_slice(b"\x1B]5522;type=read:pw=b3RoZXI=:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
    assert!(!kc(|c| c.last_read_granted));
    assert!(kc(|c| c.last_read_can_remember));

    // A password without a name doesn't count: it is neither granted
    // nor rememberable, even if the reply asks.
    kc_mut(|c| c.read_result = Some(success_remember.clone()));
    s.next_slice(b"\x1B]5522;type=read:pw=c2VjcmV0;dGV4dC9wbGFpbg==\x1B\\");
    assert_eq!(0, kc(|c| c.last_read_name.len()));
    assert!(!kc(|c| c.last_read_granted));
    assert!(!kc(|c| c.last_read_can_remember));
    s.next_slice(b"\x1B]5522;type=read:pw=b3RoZXI=;dGV4dC9wbGFpbg==\x1B\\");
    assert!(!kc(|c| c.last_read_can_remember));
    kc_mut(|c| c.read_result = Some(success_plain.clone()));
    s.next_slice(b"\x1B]5522;type=read:pw=b3RoZXI=:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
    assert!(!kc(|c| c.last_read_granted));

    // A grant is advisory: the request is still forwarded and the
    // embedder may deny it.
    kc_mut(|c| {
        c.responses.clear();
        c.read_result = Some(KcReadResult::Denied);
    });
    s.next_slice(b"\x1B]5522;type=read:id=d:pw=c2VjcmV0:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
    assert!(kc(|c| c.last_read_granted));
    assert_eq!(
        b"\x1B]5522;type=read:status=EPERM:id=d\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    // Grants are freed with the stream (the testing allocator catches
    // the leak otherwise).
    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard read targets-only never consumes a one-time grant"
#[test]
fn kitty_clipboard_read_targets_only_never_consumes_a_one_time_grant() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_read = Some(kc_clipboard_read);

    // A one-time read grant, as minted for a paste event.
    s.handler
        .kitty_clipboard_grants
        .grant(Global, b"otp", kitty::clipboard::Direction::Read, true)
        .unwrap();

    // A targets-only read (payload ".") is prompt-exempt so it never
    // consults, and must not burn, the one-time password.
    kc_mut(|c| {
        c.read_result = Some(KcReadResult::Success {
            contents: Vec::new(),
            available: Vec::new(),
            remember: false,
        });
    });
    s.next_slice(b"\x1B]5522;type=read:pw=b3Rw:name=YXBw;Lg==\x1B\\");
    assert_eq!(1, kc(|c| c.read_count));
    assert!(kc(|c| c.last_read_list));
    assert_eq!(0, kc(|c| c.last_read_mimes.len()));
    assert!(!kc(|c| c.last_read_granted));

    // The follow-up data read still consumes the grant, exactly once.
    s.next_slice(b"\x1B]5522;type=read:pw=b3Rw:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
    assert!(kc(|c| c.last_read_granted));
    s.next_slice(b"\x1B]5522;type=read:pw=b3Rw:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
    assert!(!kc(|c| c.last_read_granted));

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard write password grants"
#[test]
fn kitty_clipboard_write_password_grants() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);
    s.handler.effects.clipboard_read = Some(kc_clipboard_read);

    // pw="secret", name="app": the first commit isn't granted but the
    // reply may ask to remember it.
    kc_mut(|c| {
        c.write_result = Some(clipboard::WriteResult::Success(
            clipboard::WriteResultSuccess { remember: true },
        ));
    });
    s.next_slice(b"\x1B]5522;type=write:pw=c2VjcmV0:name=YXBw\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(1, kc(|c| c.write_count));
    kc(|c| assert_eq!(b"app".as_slice(), c.last_write_name.as_slice()));
    assert!(!kc(|c| c.last_write_granted));
    assert!(kc(|c| c.last_write_can_remember));

    // The same password is now granted; a different one is not.
    kc_mut(|c| {
        c.write_result = Some(clipboard::WriteResult::Success(
            clipboard::WriteResultSuccess::default(),
        ));
    });
    s.next_slice(b"\x1B]5522;type=write:pw=c2VjcmV0:name=YXBw\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert!(kc(|c| c.last_write_granted));
    s.next_slice(b"\x1B]5522;type=write:pw=b3RoZXI=:name=YXBw\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert!(!kc(|c| c.last_write_granted));
    assert!(kc(|c| c.last_write_can_remember));

    // Directions are independent: a write grant doesn't satisfy reads.
    kc_mut(|c| {
        c.read_result = Some(KcReadResult::Success {
            contents: Vec::new(),
            available: Vec::new(),
            remember: false,
        });
    });
    s.next_slice(b"\x1B]5522;type=read:pw=c2VjcmV0:name=YXBw\x1B\\");
    assert!(!kc(|c| c.last_read_granted));

    // A password without a name doesn't count: it is neither granted
    // nor rememberable, even if the reply asks.
    kc_mut(|c| {
        c.write_result = Some(clipboard::WriteResult::Success(
            clipboard::WriteResultSuccess { remember: true },
        ));
    });
    s.next_slice(b"\x1B]5522;type=write:pw=c2VjcmV0\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(0, kc(|c| c.last_write_name.len()));
    assert!(!kc(|c| c.last_write_granted));
    assert!(!kc(|c| c.last_write_can_remember));

    // A grant is advisory: the request is still forwarded and the
    // embedder may deny it.
    kc_mut(|c| {
        c.responses.clear();
        c.write_result = Some(clipboard::WriteResult::Denied);
    });
    s.next_slice(b"\x1B]5522;type=write:id=d:pw=c2VjcmV0:name=YXBw\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert!(kc(|c| c.last_write_granted));
    assert_eq!(
        b"\x1B]5522;type=write:status=EPERM:id=d\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    // Grants are freed with the stream (the testing allocator catches
    // the leak otherwise).
    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard malformed packets are silently dropped"
#[test]
fn kitty_clipboard_malformed_packets_are_silently_dropped() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        // Missing type, unknown type, bare metadata record, invalid mime
        // base64, and orphaned transaction packets all drop silently.
        s.next_slice(b"\x1B]5522;loc=primary\x1B\\");
        s.next_slice(b"\x1B]5522;type=bobr\x1B\\");
        s.next_slice(b"\x1B]5522;type=read:bare\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=!!!;R2hvc3Q=\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
        s.next_slice(b"\x1B]5522;type=walias:mime=dGV4dC9wbGFpbg==;VEVYVA==\x1B\\");
        assert_eq!(0, kc(|c| c.write_count));
        assert_eq!(0, kc_responses_len());
        assert!(!s.handler.semantic_failure);

        // The terminal is still functional afterwards.
        s.next_slice(b"ok");
        s.deinit();
    }
    let str = t.plain_string(Global).unwrap();
    assert_eq!(b"ok".as_slice(), str.as_slice());

    t.deinit(Global);
}

// test "kitty clipboard new write replaces in-flight transaction"
#[test]
fn kitty_clipboard_new_write_replaces_in_flight_transaction() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    s.next_slice(b"\x1B]5522;type=write:id=old\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;b2xk\x1B\\"); // "old"
    s.next_slice(b"\x1B]5522;type=write:id=new\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;bmV3\x1B\\"); // "new"
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");

    assert_eq!(1, kc(|c| c.write_count));
    assert_eq!(1, kc(|c| c.last_contents.len()));
    kc(|c| assert_eq!(b"new".as_slice(), c.last_contents[0].1.as_slice()));
    assert_eq!(
        b"\x1B]5522;type=write:status=DONE:id=new\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard invalid write packets abort with EINVAL"
#[test]
fn kitty_clipboard_invalid_write_packets_abort_with_einval() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    let invalid_packets: &[&[u8]] = &[
        // Alias payload decodes to a non-UTF-8 byte.
        b"\x1B]5522;type=walias:mime=dGV4dC9wbGFpbg==;/w==\x1B\\",
        // Alias has no target MIME type.
        b"\x1B]5522;type=walias;VEVYVA==\x1B\\",
        // Alias target MIME decodes to non-UTF-8 bytes.
        b"\x1B]5522;type=walias:mime=//4=;VEVYVA==\x1B\\",
        // Write data MIME decodes to non-UTF-8 bytes.
        b"\x1B]5522;type=wdata:mime=//4=;R2hvc3Q=\x1B\\",
    ];

    for packet in invalid_packets {
        kc_mut(|c| c.responses.clear());
        s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
        s.next_slice(packet);
        assert_eq!(
            b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );
        assert!(!s.handler.semantic_failure);

        // The transaction is gone: a commit does nothing further.
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(0, kc(|c| c.write_count));
        assert_eq!(
            b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );
    }

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard invalid read text does not abort a write"
#[test]
fn kitty_clipboard_invalid_read_text_does_not_abort_a_write() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();
    kc_mut(|c| {
        c.read_result = Some(KcReadResult::Success {
            contents: Vec::new(),
            available: Vec::new(),
            remember: false,
        });
    });

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_read = Some(kc_clipboard_read);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");

    // The read payload decodes to a non-UTF-8 byte. It is dropped without
    // invoking the clipboard effect or disturbing the write transaction.
    s.next_slice(b"\x1B]5522;type=read;/w==\x1B\\");
    assert_eq!(0, kc(|c| c.read_count));
    assert_eq!(0, kc_responses_len());

    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(1, kc(|c| c.write_count));
    assert_eq!(
        b"\x1B]5522;type=write:status=DONE:id=w\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard oversized text write aborts with EFBIG"
#[test]
fn kitty_clipboard_oversized_text_write_aborts_with_efbig() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    // Shrink the limit so the test doesn't have to stream the
    // default 64MiB.
    s.handler.kitty_clipboard_write_max_bytes = 4;

    s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;SGVsbA==\x1B\\"); // "Hell"
    assert_eq!(0, kc_responses_len());
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;bw==\x1B\\"); // "o"
    assert_eq!(
        b"\x1B]5522;type=write:status=EFBIG:id=w\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );
    assert!(!s.handler.semantic_failure);

    // The transaction is gone: later data and the commit do nothing.
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;IQ==\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(0, kc(|c| c.write_count));
    assert_eq!(
        b"\x1B]5522;type=write:status=EFBIG:id=w\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard invalid wdata chunk aborts with EINVAL"
#[test]
fn kitty_clipboard_invalid_wdata_chunk_aborts_with_einval() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;SGVsbG8=\x1B\\"); // "Hello"
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;!!!bad!!!\x1B\\");
    assert_eq!(
        b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );
    assert!(!s.handler.semantic_failure);

    // The transaction is gone: later data and the commit do nothing.
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;V29ybGQ=\x1B\\"); // "World"
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(0, kc(|c| c.write_count));
    assert_eq!(
        b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard wdata chunks split one base64 stream"
#[test]
fn kitty_clipboard_wdata_chunks_split_one_base64_stream() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    // "some data" encoded as one stream, split at non-group
    // boundaries across packets.
    s.next_slice(b"\x1B]5522;type=write\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;c29\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;tZSBk\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;YXRh\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");

    assert_eq!(1, kc(|c| c.write_count));
    kc(|c| assert_eq!(b"some data".as_slice(), c.last_contents[0].1.as_slice()));
    assert_eq!(
        b"\x1B]5522;type=write:status=DONE\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard unpadded wdata stream aborts at commit"
#[test]
fn kitty_clipboard_unpadded_wdata_stream_aborts_at_commit() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    // "Hello" without its final padding byte: every packet decodes,
    // but the stream ends mid-group so the commit reports EINVAL.
    s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;SGVsbG8\x1B\\");
    assert_eq!(0, kc_responses_len());
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(0, kc(|c| c.write_count));
    assert_eq!(
        b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
        kc_responses().as_slice(),
    );
    assert!(!s.handler.semantic_failure);

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard in-flight transaction is freed on deinit"
#[test]
fn kitty_clipboard_in_flight_transaction_is_freed_on_deinit() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    // Never committed: stream deinit must free the transaction (the
    // testing allocator catches the leak otherwise; the ported Box
    // frees on drop and `deinit` clears it explicitly).
    s.next_slice(b"\x1B]5522;type=write\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
    assert_eq!(0, kc(|c| c.write_count));
    assert!(s.handler.kitty_clipboard_write.is_some());
    s.deinit();
    assert!(s.handler.kitty_clipboard_write.is_none());

    t.deinit(Global);
}

// test "kitty clipboard allocation failure is ignored"
#[test]
fn kitty_clipboard_allocation_failure_is_ignored() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(kc_write_pty);
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    // Only transaction state uses the terminal allocator here. Swap in
    // an allocator that always fails, then restore it before teardown.
    {
        // const alloc = t.screens.active.alloc;
        // t.screens.active.alloc = testing.failing_allocator;
        // defer t.screens.active.alloc = alloc;
        let alloc = s.handler.term().active_mut().alloc;
        s.handler.term().active_mut().alloc = crate::alloc::failing();
        s.next_slice(b"\x1B]5522;type=write\x1B\\");
        s.handler.term().active_mut().alloc = alloc;
    }

    // Clipboard writes are external effects, best-effort like OSC 52;
    // the failed transaction never started and is not a semantic
    // failure.
    assert!(!s.handler.semantic_failure);
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(0, kc(|c| c.write_count));
    assert_eq!(0, kc_responses_len());

    s.deinit();
    t.deinit(Global);
}

// test "kitty clipboard without write_pty still commits writes"
#[test]
fn kitty_clipboard_without_write_pty_still_commits_writes() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    kc_reset();

    let mut s = t.vt_stream();
    s.handler.effects.clipboard_write = Some(kc_clipboard_write);

    s.next_slice(b"\x1B]5522;type=write\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
    assert_eq!(1, kc(|c| c.write_count));
    kc(|c| assert_eq!(b"Ghost".as_slice(), c.last_contents[0].1.as_slice()));

    // Reads are dropped without a way to respond.
    s.next_slice(b"\x1B]5522;type=read\x1B\\");
    assert_eq!(0, kc_responses_len());

    s.deinit();
    t.deinit(Global);
}

// Capture state for the clipboard_read (OSC 52) effect test. Mirrors the
// reference's per-test `S` struct through the thread-local idiom.
thread_local! {
    static CR_WRITTEN: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static CR_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    static CR_LOCATION: core::cell::Cell<Option<clipboard::Location>> =
        const { core::cell::Cell::new(None) };
    static CR_MIMES: core::cell::RefCell<Vec<Vec<u8>>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static CR_LIST: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    static CR_NAME: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static CR_GRANTED: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    static CR_CAN_REMEMBER: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    static CR_RESULT: core::cell::RefCell<Option<KcReadResult>> =
        const { core::cell::RefCell::new(None) };
    static CR_REPLY_TWICE: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

fn cr_write_pty(_: &mut Handler, data: &[u8]) {
    CR_WRITTEN.with(|c| c.borrow_mut().extend_from_slice(data));
}

fn cr_clipboard_read(handler: &mut Handler, read: clipboard::Read) {
    CR_COUNT.with(|c| c.set(c.get() + 1));
    CR_LOCATION.with(|c| c.set(Some(read.location)));
    CR_MIMES.with(|c| *c.borrow_mut() = read.mimes.iter().map(|m| m.to_vec()).collect());
    CR_LIST.with(|c| c.set(read.list));
    CR_NAME.with(|c| *c.borrow_mut() = read.name.to_vec());
    CR_GRANTED.with(|c| c.set(read.granted));
    CR_CAN_REMEMBER.with(|c| c.set(read.can_remember));
    let result = CR_RESULT.with(|c| c.borrow().clone());
    if let Some(r) = &result {
        r.reply_with(handler, &read);
    }
    if CR_REPLY_TWICE.with(|c| c.get()) {
        read.reply(
            handler,
            clipboard::ReadResult::Success(clipboard::ReadResultSuccess {
                contents: &[clipboard::Content {
                    mime: b"text/plain",
                    data: b"again",
                }],
                ..clipboard::ReadResultSuccess::default()
            }),
        );
    }
}

fn cr_written() -> Vec<u8> {
    CR_WRITTEN.with(|c| c.borrow().clone())
}

fn cr_clipboard_read_then_write_pty(handler: &mut Handler, read: clipboard::Read) {
    read.reply(
        handler,
        clipboard::ReadResult::Success(clipboard::ReadResultSuccess {
            contents: &[clipboard::Content {
                mime: b"text/plain",
                data: b"hello",
            }],
            ..clipboard::ReadResultSuccess::default()
        }),
    );
    handler.write_pty(b"X");
}

// Parity S166-F1 (no reference test): the reference writes a read reply's
// response INSIDE `reply` (`ClipboardReadState.reply` -> `respond` ->
// `handler.writePty`), so pty bytes the callback writes after replying
// come after the response. The port used to deliver the response after
// the callback returned ("X" first).
#[test]
fn clipboard_read_reply_side_effects_happen_inside_reply() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    CR_WRITTEN.with(|c| c.borrow_mut().clear());

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cr_write_pty);
    s.handler.effects.clipboard_read = Some(cr_clipboard_read_then_write_pty);
    s.next_slice(b"\x1B]52;c;?\x1B\\");
    assert_eq!(
        b"\x1B]52;c;aGVsbG8=\x1B\\X".as_slice(),
        cr_written().as_slice()
    );

    s.deinit();
    t.deinit(Global);
}

thread_local! {
    /// `kitty_clipboard_grants.len()` as the callback saw it right after
    /// its `reply` returned.
    static S166_GRANTS_AFTER_REPLY: core::cell::Cell<Option<usize>> =
        const { core::cell::Cell::new(None) };
}

fn s166_kitty_read_remember_then_write_pty(handler: &mut Handler, read: clipboard::Read) {
    read.reply(
        handler,
        clipboard::ReadResult::Success(clipboard::ReadResultSuccess {
            contents: &[clipboard::Content {
                mime: b"text/plain",
                data: b"hi",
            }],
            remember: true,
            ..clipboard::ReadResultSuccess::default()
        }),
    );
    let seen = handler.kitty_clipboard_grants.len();
    S166_GRANTS_AFTER_REPLY.with(|c| c.set(Some(seen)));
    handler.write_pty(b"X");
}

fn s166_kitty_write_remember(handler: &mut Handler, write: clipboard::Write) {
    write.reply(
        handler,
        clipboard::WriteResult::Success(clipboard::WriteResultSuccess { remember: true }),
    );
    let seen = handler.kitty_clipboard_grants.len();
    S166_GRANTS_AFTER_REPLY.with(|c| c.set(Some(seen)));
}

// Parity S166-F1 (no reference test): a Kitty read reply that asks to
// remember records the grant and writes the whole success sequence
// inside `reply` (`KittyClipboardReadState.reply`), so the callback sees
// the grant as soon as `reply` returns and its own pty bytes follow the
// response.
#[test]
fn kitty_clipboard_read_reply_grant_and_response_happen_inside_reply() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    CR_WRITTEN.with(|c| c.borrow_mut().clear());
    S166_GRANTS_AFTER_REPLY.with(|c| c.set(None));

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cr_write_pty);
    s.handler.effects.clipboard_read = Some(s166_kitty_read_remember_then_write_pty);
    assert_eq!(0, s.handler.kitty_clipboard_grants.len());
    s.next_slice(b"\x1B]5522;type=read:pw=c2VjcmV0:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");

    assert_eq!(Some(1), S166_GRANTS_AFTER_REPLY.with(|c| c.get()));
    let written = cr_written();
    assert!(
        written.starts_with(b"\x1B]5522;type=read:status=OK"),
        "{:?}",
        String::from_utf8_lossy(&written)
    );
    assert!(
        written.ends_with(b"\x1B]5522;type=read:status=DONE\x1B\\X"),
        "{:?}",
        String::from_utf8_lossy(&written)
    );

    s.deinit();
    t.deinit(Global);
}

// Parity S166-F1 (no reference test): a Kitty write reply that asks to
// remember records the grant inside `reply`
// (`KittyClipboardWriteReplyState.reply`), so it is visible to the
// callback as soon as `reply` returns.
#[test]
fn kitty_clipboard_write_reply_grant_is_visible_inside_the_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    CR_WRITTEN.with(|c| c.borrow_mut().clear());
    S166_GRANTS_AFTER_REPLY.with(|c| c.set(None));

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cr_write_pty);
    s.handler.effects.clipboard_write = Some(s166_kitty_write_remember);
    s.next_slice(b"\x1B]5522;type=write:pw=c2VjcmV0:name=YXBw\x1B\\");
    s.next_slice(b"\x1B]5522;type=wdata\x1B\\");

    assert_eq!(Some(1), S166_GRANTS_AFTER_REPLY.with(|c| c.get()));
    assert_eq!(1, s.handler.kitty_clipboard_grants.len());
    assert_eq!(
        b"\x1B]5522;type=write:status=DONE\x1B\\".as_slice(),
        cr_written().as_slice()
    );

    s.deinit();
    t.deinit(Global);
}

// test "clipboard_read effect callback"
#[test]
fn clipboard_read_effect_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    CR_WRITTEN.with(|c| c.borrow_mut().clear());
    CR_COUNT.with(|c| c.set(0));
    CR_RESULT.with(|c| {
        *c.borrow_mut() = Some(KcReadResult::Success {
            contents: vec![(b"text/plain".to_vec(), b"hello".to_vec())],
            available: Vec::new(),
            remember: false,
        })
    });
    CR_REPLY_TWICE.with(|c| c.set(false));

    // A null callback (the default readonly effects) silently ignores reads.
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cr_write_pty);
        s.next_slice(b"\x1B]52;c;?\x1B\\");
        assert_eq!(0, cr_written().len());
        s.deinit();
    }

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cr_write_pty);
    s.handler.effects.clipboard_read = Some(cr_clipboard_read);

    // Success echoes the normalized selector and request terminator.
    s.next_slice(b"\x1B]52;c;?\x1B\\");
    assert_eq!(1, CR_COUNT.with(|c| c.get()));
    assert_eq!(
        Some(clipboard::Location::Standard),
        CR_LOCATION.with(|c| c.get())
    );
    assert_eq!(1, CR_MIMES.with(|c| c.borrow().len()));
    CR_MIMES.with(|c| assert_eq!(b"text/plain".as_slice(), c.borrow()[0].as_slice()));
    assert!(!CR_LIST.with(|c| c.get()));
    assert_eq!(0, CR_NAME.with(|c| c.borrow().len()));
    assert!(!CR_GRANTED.with(|c| c.get()));
    assert!(!CR_CAN_REMEMBER.with(|c| c.get()));
    assert_eq!(
        b"\x1B]52;c;aGVsbG8=\x1B\\".as_slice(),
        cr_written().as_slice()
    );

    CR_WRITTEN.with(|c| c.borrow_mut().clear());
    s.next_slice(b"\x1B]52;p;?\x07");
    assert_eq!(
        Some(clipboard::Location::Primary),
        CR_LOCATION.with(|c| c.get())
    );
    assert_eq!(
        b"\x1B]52;p;aGVsbG8=\x07".as_slice(),
        cr_written().as_slice()
    );

    // Only the first text representation is used.
    CR_WRITTEN.with(|c| c.borrow_mut().clear());
    CR_RESULT.with(|c| {
        *c.borrow_mut() = Some(KcReadResult::Success {
            contents: vec![
                (b"image/png".to_vec(), b"\x89PNG".to_vec()),
                (b"UTF8_STRING".to_vec(), b"hi".to_vec()),
            ],
            available: Vec::new(),
            // OSC 52 has no session passwords, so remember is ignored.
            remember: true,
        })
    });
    s.next_slice(b"\x1B]52;s;?\x1B\\");
    assert_eq!(
        Some(clipboard::Location::Selection),
        CR_LOCATION.with(|c| c.get())
    );
    assert_eq!(b"\x1B]52;s;aGk=\x1B\\".as_slice(), cr_written().as_slice());

    // Every failure, no text, and no reply all answer with an empty
    // clipboard.
    let failure_results: &[Option<KcReadResult>] = &[
        Some(KcReadResult::Denied),
        Some(KcReadResult::Unsupported),
        Some(KcReadResult::Busy),
        Some(KcReadResult::IoError),
        Some(KcReadResult::Success {
            contents: Vec::new(),
            available: Vec::new(),
            remember: false,
        }),
        None,
    ];
    for result in failure_results {
        CR_WRITTEN.with(|c| c.borrow_mut().clear());
        CR_RESULT.with(|c| *c.borrow_mut() = result.clone());
        s.next_slice(b"\x1B]52;c;?\x1B\\");
        assert_eq!(b"\x1B]52;c;\x1B\\".as_slice(), cr_written().as_slice());
    }

    // A second reply is ignored.
    CR_WRITTEN.with(|c| c.borrow_mut().clear());
    CR_RESULT.with(|c| {
        *c.borrow_mut() = Some(KcReadResult::Success {
            contents: vec![(b"text/plain".to_vec(), b"hello".to_vec())],
            available: Vec::new(),
            remember: false,
        })
    });
    CR_REPLY_TWICE.with(|c| c.set(true));
    s.next_slice(b"\x1B]52;c;?\x1B\\");
    assert_eq!(
        b"\x1B]52;c;aGVsbG8=\x1B\\".as_slice(),
        cr_written().as_slice()
    );

    CR_REPLY_TWICE.with(|c| c.set(false));
    s.deinit();
    t.deinit(Global);
}

// test "mode 2048 enable reports current geometry and disable is silent"
#[test]
fn mode_2048_enable_reports_current_geometry_and_disable_is_silent() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    fn get_size(_: &mut Handler) -> Option<size_report::Size> {
        Some(size_report::Size {
            rows: 24,
            columns: 80,
            cell_width: 8,
            cell_height: 16,
        })
    }
    calls_reset();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(count_write_pty);
    s.handler.effects.size = Some(get_size);

    s.next_slice(b"\x1b[?2048h");
    s.next_slice(b"\x1b[?2048h");

    assert_eq!(2, calls_get());
    assert_eq!(
        b"\x1b[48;24;80;384;640t".as_slice(),
        cap_take().unwrap().as_slice()
    );

    s.next_slice(b"\x1b[?2048l");
    assert_eq!(2, calls_get());
    assert!(!unsafe { &*s.handler.terminal }
        .modes
        .get(modespkg::Mode::InBandSizeReports));

    s.deinit();
    t.deinit(Global);
}

// test "mode 2048 enable tolerates missing effects"
#[test]
fn mode_2048_enable_tolerates_missing_effects() {
    fn get_size(_: &mut Handler) -> Option<size_report::Size> {
        Some(size_report::Size {
            rows: 24,
            columns: 80,
            cell_width: 8,
            cell_height: 16,
        })
    }
    calls_reset();

    let mut no_size_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut no_size_stream = no_size_terminal.vt_stream();
        no_size_stream.handler.effects.write_pty = Some(count_write_pty);

        no_size_stream.next_slice(b"\x1b[?2048h");
        assert!(unsafe { &*no_size_stream.handler.terminal }
            .modes
            .get(modespkg::Mode::InBandSizeReports));
        assert_eq!(0, calls_get());
        no_size_stream.deinit();
    }
    no_size_terminal.deinit(Global);

    let mut no_write_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut no_write_stream = no_write_terminal.vt_stream();
        no_write_stream.handler.effects.size = Some(get_size);

        no_write_stream.next_slice(b"\x1b[?2048h");
        assert!(unsafe { &*no_write_stream.handler.terminal }
            .modes
            .get(modespkg::Mode::InBandSizeReports));
        assert_eq!(0, calls_get());
        no_write_stream.deinit();
    }
    no_write_terminal.deinit(Global);
}

// Capture for the kitty dnd tests: every pty write.
thread_local! {
    static DND_PTY: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static DND_EVENTS: core::cell::RefCell<Vec<kitty::dnd::Event>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static DND_MIMES: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

fn dnd_write_pty(_: &mut Handler, data: &[u8]) {
    DND_PTY.with(|c| c.borrow_mut().extend_from_slice(data));
}

fn dnd_pty() -> Vec<u8> {
    DND_PTY.with(|c| c.borrow().clone())
}

// test "kitty dnd: query response"
#[test]
fn kitty_dnd_query_response() {
    DND_PTY.with(|c| c.borrow_mut().clear());

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(dnd_write_pty);

    s.next_slice(b"\x1B]72;t=q:i=3\x1B\\");
    assert_eq!(b"\x1b]72;t=q:i=3\x1b\\".as_slice(), dnd_pty().as_slice());

    s.deinit();
    t.deinit(Global);
}

// test "kitty dnd: register, drop, and serve data"
#[test]
fn kitty_dnd_register_drop_and_serve_data() {
    DND_PTY.with(|c| c.borrow_mut().clear());

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;

    let mut s = Stream::init(stream::Options {
        allocator: Some(crate::alloc::Global),
        handler: Handler::init(ptr),
        continuation_max_bytes: None,
        osc_unknown_max_bytes: 0,
    });
    s.handler.effects.write_pty = Some(dnd_write_pty);

    // Client registers to accept drops.
    s.next_slice(b"\x1B]72;t=a;text/plain text/uri-list\x1B\\");
    assert_eq!(0, dnd_pty().len());
    assert!(unsafe { (*ptr).kitty_dnd.is_some() });

    // A native drop arrives; the embedder feeds it to the terminal
    // state and delivers the produced event bytes itself.
    {
        let mut aw: Vec<u8> = Vec::new();
        unsafe { (*ptr).kitty_dnd.as_mut() }
            .unwrap()
            .drag_drop(
                Global,
                &mut aw,
                kitty::dnd::MoveEvent {
                    cell_x: 2,
                    cell_y: 1,
                    pixel_x: 20,
                    pixel_y: 18,
                    operations: kitty::dnd::Operations {
                        copy: true,
                        r#move: false,
                    },
                },
                &[kitty::dnd::Item {
                    mime: b"text/plain".to_vec(),
                    data: b"hello".to_vec(),
                }],
            )
            .unwrap();
        assert_eq!(
            b"\x1b]72;t=M:x=2:y=1:X=20:Y=18:o=1:m=0;text/plain \x1b\\".as_slice(),
            aw.as_slice(),
        );
    }

    // The client requests the data and concludes.
    s.next_slice(b"\x1B]72;t=r:x=1\x1B\\");
    let mut expected: Vec<u8> = Vec::new();
    expected.extend_from_slice(b"\x1b]72;t=r:x=1:m=0;aGVsbG8=\x1b\\");
    expected.extend_from_slice(b"\x1b]72;t=r:x=1\x1b\\");
    assert_eq!(expected, dnd_pty());
    DND_PTY.with(|c| c.borrow_mut().clear());

    s.next_slice(b"\x1B]72;t=r\x1B\\");
    assert_eq!(0, dnd_pty().len());
    assert!(unsafe { (*ptr).kitty_dnd.as_ref() }
        .unwrap()
        .drop
        .items
        .is_none());

    s.deinit();
    t.deinit(Global);
}

// test "kitty dnd: state updates work without write_pty effect"
#[test]
fn kitty_dnd_state_updates_work_without_write_pty_effect() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;

    let mut s = Stream::init(stream::Options {
        allocator: Some(crate::alloc::Global),
        handler: Handler::init(ptr),
        continuation_max_bytes: None,
        osc_unknown_max_bytes: 0,
    });

    // Queries produce no output (nowhere to write) but registration
    // state still updates.
    s.next_slice(b"\x1B]72;t=q\x1B\\");
    s.next_slice(b"\x1B]72;t=a\x1B\\");
    assert!(unsafe { (*ptr).kitty_dnd.is_some() });

    // The terminal remains functional.
    s.next_slice(b"ok");
    let str = unsafe { (*ptr).plain_string(Global) }.unwrap();
    assert_eq!(b"ok".as_slice(), str.as_slice());

    s.deinit();
    t.deinit(Global);
}

// test "kitty dnd: registration survives terminal reset"
#[test]
fn kitty_dnd_registration_survives_terminal_reset() {
    DND_PTY.with(|c| c.borrow_mut().clear());

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;

    let mut s = Stream::init(stream::Options {
        allocator: Some(crate::alloc::Global),
        handler: Handler::init(ptr),
        continuation_max_bytes: None,
        osc_unknown_max_bytes: 0,
    });
    s.handler.effects.write_pty = Some(dnd_write_pty);

    // Start a chunked command, then reset mid-chunk.
    s.next_slice(b"\x1B]72;t=a:i=5\x1B\\");
    s.next_slice(b"\x1B]72;t=m:o=1:m=1;text/pl\x1B\\");
    s.next_slice(b"\x1Bc");

    // Registration survives (matching kitty), chunking was interrupted
    // so a new command is not treated as a continuation.
    assert!(unsafe { (*ptr).kitty_dnd.is_some() });
    assert!(
        !unsafe { (*ptr).kitty_dnd.as_ref() }
            .unwrap()
            .chunking
            .active
    );
    s.next_slice(b"\x1B]72;t=q\x1B\\");
    assert_eq!(b"\x1b]72;t=q\x1b\\".as_slice(), dnd_pty().as_slice());

    s.deinit();
    t.deinit(Global);
}

// test "kitty dnd: effect reports registration, acceptance, and conclusion"
#[test]
fn kitty_dnd_effect_reports_registration_acceptance_and_conclusion() {
    DND_EVENTS.with(|c| c.borrow_mut().clear());
    DND_MIMES.with(|c| c.borrow_mut().clear());

    fn drag_and_drop(handler: &mut Handler, ev: kitty::dnd::Event) {
        DND_EVENTS.with(|c| c.borrow_mut().push(ev));
        // Registration details are read from the terminal state.
        if ev == kitty::dnd::Event::Registration {
            DND_MIMES.with(|c| {
                let mut mimes = c.borrow_mut();
                mimes.clear();
                // const state = handler.terminal.kitty_dnd orelse return;
                // SAFETY: the handler aliases the terminal (see `term()`);
                // the shared read is short-lived inside the callback.
                let Some(state) = (unsafe { (*handler.terminal).kitty_dnd.as_ref() }) else {
                    return;
                };
                let mut it = state.registered_mimes();
                // `while let ... = it.next()`, not `for m in it`: the
                // reference writes `while (it.next()) |m|` and the port
                // mirrors its loop form.
                #[allow(clippy::while_let_on_iterator)]
                while let Some(m) = it.next() {
                    mimes.extend_from_slice(m);
                    mimes.push(b',');
                }
            });
        }
    }

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;

    let mut s = Stream::init(stream::Options {
        allocator: Some(crate::alloc::Global),
        handler: Handler::init(ptr),
        continuation_max_bytes: None,
        osc_unknown_max_bytes: 0,
    });
    s.handler.effects.drag_and_drop = Some(drag_and_drop);

    // Registration with a MIME list, read back from the state.
    s.next_slice(b"\x1B]72;t=a;image/png text/plain\x1B\\");
    assert_eq!(1, DND_EVENTS.with(|c| c.borrow().len()));
    assert_eq!(
        kitty::dnd::Event::Registration,
        DND_EVENTS.with(|c| c.borrow()[0])
    );
    DND_MIMES.with(|c| assert_eq!(b"image/png,text/plain,".as_slice(), c.borrow().as_slice()));

    // A native drag and the client's answer.
    {
        let mut aw: Vec<u8> = Vec::new();
        unsafe { (*ptr).kitty_dnd.as_mut() }
            .unwrap()
            .drag_drop(
                Global,
                &mut aw,
                kitty::dnd::MoveEvent {
                    cell_x: 0,
                    cell_y: 0,
                    pixel_x: 0,
                    pixel_y: 0,
                    operations: kitty::dnd::Operations {
                        copy: true,
                        r#move: false,
                    },
                },
                &[kitty::dnd::Item {
                    mime: b"text/plain".to_vec(),
                    data: b"x".to_vec(),
                }],
            )
            .unwrap();
    }
    s.next_slice(b"\x1B]72;t=m:o=2;text/plain\x1B\\");
    assert_eq!(2, DND_EVENTS.with(|c| c.borrow().len()));
    assert_eq!(
        kitty::dnd::Event::Acceptance,
        DND_EVENTS.with(|c| c.borrow()[1])
    );

    // Conclusion carries the performed operation.
    s.next_slice(b"\x1B]72;t=r:o=2\x1B\\");
    assert_eq!(3, DND_EVENTS.with(|c| c.borrow().len()));
    assert_eq!(
        kitty::dnd::Event::ConcludedMove,
        DND_EVENTS.with(|c| c.borrow()[2])
    );

    // Unregistration reports with the state gone.
    s.next_slice(b"\x1B]72;t=A\x1B\\");
    assert_eq!(4, DND_EVENTS.with(|c| c.borrow().len()));
    assert_eq!(
        kitty::dnd::Event::Registration,
        DND_EVENTS.with(|c| c.borrow()[3])
    );
    assert!(unsafe { (*ptr).kitty_dnd.is_none() });
    DND_MIMES.with(|c| assert_eq!(0, c.borrow().len()));

    s.deinit();
    t.deinit(Global);
}

/// Capture state for the Handler.paste tests below: every pty write and
/// the clipboard reads the program makes afterwards.
// const PasteCapture = struct { ... };
#[derive(Default)]
struct PasteCap {
    written: Vec<u8>,
    write_count: usize,
    read_count: usize,
    read_granted_count: usize,
    last_read_granted: bool,
    last_read_name: Vec<u8>,
}

thread_local! {
    static PASTE_CAP: core::cell::RefCell<PasteCap> =
        core::cell::RefCell::new(PasteCap::default());
}

fn pc_reset() {
    PASTE_CAP.with(|c| *c.borrow_mut() = PasteCap::default());
}

fn pc<T>(f: impl FnOnce(&PasteCap) -> T) -> T {
    PASTE_CAP.with(|c| f(&c.borrow()))
}

fn pc_written() -> Vec<u8> {
    pc(|c| c.written.clone())
}

// fn writePty(_: *Handler, data: []const u8) void { ... }
fn pc_write_pty(_: &mut Handler, data: &[u8]) {
    PASTE_CAP.with(|c| {
        let mut cap = c.borrow_mut();
        cap.written.extend_from_slice(data);
        cap.write_count += 1;
    });
}

// fn clipboardRead(_: *Handler, read: clipboard.Read) void { ... }
fn pc_clipboard_read(handler: &mut Handler, read: clipboard::Read) {
    PASTE_CAP.with(|c| {
        let mut cap = c.borrow_mut();
        cap.read_count += 1;
        cap.last_read_granted = read.granted;
        if read.granted {
            cap.read_granted_count += 1;
        }
        cap.last_read_name = read.name.to_vec();
    });
    read.reply(
        handler,
        clipboard::ReadResult::Success(clipboard::ReadResultSuccess {
            contents: &[clipboard::Content {
                mime: b"text/plain",
                data: b"Ghostty",
            }],
            ..clipboard::ReadResultSuccess::default()
        }),
    );
}

// test "paste: no write_pty effect is an error"
#[test]
fn paste_no_write_pty_effect_is_an_error() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;

    let mut handler = Handler::init(ptr);
    assert_eq!(
        Err(PasteError::NoWritePty),
        handler.paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"hello",
            }]),
            allow_unsafe: false,
        })
    );
    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: plain text converts newlines and strips unsafe bytes"
#[test]
fn paste_plain_text_converts_newlines_and_strips_unsafe_bytes() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);

    // Newlines are unsafe unbracketed; the embedder confirmed.
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"hel\x1blo\nwor\x00ld",
            }]),
            allow_unsafe: true,
        })
        .unwrap());
    assert_eq!(b"hel lo\rwor ld".as_slice(), pc_written().as_slice());
    assert_eq!(1, pc(|c| c.write_count));

    // The first text representation is used; others are ignored.
    pc_reset();
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[
                clipboard::Content {
                    mime: b"image/png",
                    data: b"\x89PNG",
                },
                clipboard::Content {
                    mime: b"UTF8_STRING",
                    data: b"hi",
                },
                clipboard::Content {
                    mime: b"text/plain",
                    data: b"ignored",
                },
            ]),
            allow_unsafe: false,
        })
        .unwrap());
    assert_eq!(b"hi".as_slice(), pc_written().as_slice());
    assert_eq!(1, pc(|c| c.write_count));

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: unsafe text is refused unless allowed"
#[test]
fn paste_unsafe_text_is_refused_unless_allowed() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);

    assert_eq!(
        Err(PasteError::UnsafePaste),
        handler.paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"rm -rf /\n",
            }]),
            allow_unsafe: false,
        })
    );
    assert_eq!(0, pc(|c| c.write_count));

    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"rm -rf /\n",
            }]),
            allow_unsafe: true,
        })
        .unwrap());
    assert_eq!(b"rm -rf /\r".as_slice(), pc_written().as_slice());

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: bracketed paste frames the text"
#[test]
fn paste_bracketed_paste_frames_the_text() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    unsafe { (*ptr).modes.set(modespkg::Mode::BracketedPaste, true) };

    // Newlines are safe inside the frame and are preserved.
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"hello\nworld",
            }]),
            allow_unsafe: false,
        })
        .unwrap());
    assert_eq!(
        b"\x1b[200~hello\nworld\x1b[201~".as_slice(),
        pc_written().as_slice()
    );
    assert_eq!(1, pc(|c| c.write_count));

    // The frame terminator is not.
    pc_reset();
    assert_eq!(
        Err(PasteError::UnsafePaste),
        handler.paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"he\x1b[201~llo",
            }]),
            allow_unsafe: false,
        })
    );
    assert_eq!(0, pc(|c| c.write_count));

    // Allowed, the stripper still defuses it: ESC becomes a space.
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"he\x1b[201~llo",
            }]),
            allow_unsafe: true,
        })
        .unwrap());
    assert_eq!(
        b"\x1b[200~he [201~llo\x1b[201~".as_slice(),
        pc_written().as_slice()
    );

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: no text representation writes nothing"
#[test]
fn paste_no_text_representation_writes_nothing() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);

    assert!(!handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"image/png",
                data: b"\x89PNG",
            }]),
            allow_unsafe: false,
        })
        .unwrap());
    assert!(!handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[]),
            allow_unsafe: false,
        })
        .unwrap());
    assert!(!handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"",
            }]),
            allow_unsafe: false,
        })
        .unwrap());
    assert_eq!(0, pc(|c| c.write_count));

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: large text streams to the pty in chunks"
#[test]
fn paste_large_text_streams_to_the_pty_in_chunks() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    unsafe { (*ptr).modes.set(modespkg::Mode::BracketedPaste, true) };

    // Two full chunks and a partial one with the frame, never the
    // whole thing at once.
    let data = vec![b'x'; 10_000];
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: &data,
            }]),
            allow_unsafe: false,
        })
        .unwrap());
    let total = data.len() + b"\x1b[200~\x1b[201~".len();
    assert_eq!(total.div_ceil(PASTE_CHUNK_SIZE), pc(|c| c.write_count));
    assert_eq!(total, pc(|c| c.written.len()));
    assert!(pc_written().starts_with(b"\x1b[200~xxx"));
    assert!(pc_written().ends_with(b"xxx\x1b[201~"));

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

/// A paste contents reader for the tests: serves fixed data per MIME
/// type in pieces, counting the reads of each representation.
// const PasteReader = struct { ... };
struct PasteReader {
    mimes: &'static [&'static [u8]],
    data: &'static [&'static [u8]],
    piece: usize,
    reads: [usize; 4],
    /// Fail after this many bytes of a read.
    fail_after: Option<usize>,
}

impl PasteReader {
    fn new(mimes: &'static [&'static [u8]], data: &'static [&'static [u8]]) -> PasteReader {
        PasteReader {
            mimes,
            data,
            // piece: usize = 3 (the reference field default)
            piece: 3,
            reads: [0; 4],
            fail_after: None,
        }
    }

    // fn contents(self: *PasteReader) paste_pkg.Contents { ... }
    fn contents(&mut self) -> paste_pkg::Contents<'static> {
        paste_pkg::Contents::Reader(paste_pkg::Reader {
            mimes: self.mimes,
            read: clipboard::MimeReader {
                ctx: self as *mut PasteReader as *mut core::ffi::c_void,
                read_fn: PasteReader::read,
            },
        })
    }

    // fn read(ctx: ?*anyopaque, mime: []const u8, sink: *std.Io.Writer)
    //     clipboard.MimeReader.Error!void { ... }
    fn read(
        ctx: *mut core::ffi::c_void,
        mime: &[u8],
        sink: &mut dyn crate::snapshot::io::Writer,
    ) -> Result<(), clipboard::MimeReaderError> {
        // SAFETY: `ctx` is the `&mut PasteReader` `contents` captured; the
        // reader is only used during the paste call and no other reference
        // to it is live then.
        let this = unsafe { &mut *(ctx as *mut PasteReader) };
        let index = match this.mimes.iter().position(|m| *m == mime) {
            Some(i) => i,
            None => return Err(clipboard::MimeReaderError::ReadFailed),
        };
        this.reads[index] += 1;
        let data = this.data[index];
        let mut offset: usize = 0;
        while offset < data.len() {
            if let Some(limit) = this.fail_after {
                if offset >= limit {
                    return Err(clipboard::MimeReaderError::ReadFailed);
                }
            }
            let n = this.piece.min(data.len() - offset);
            sink.write_all(&data[offset..offset + n])
                .map_err(|_| clipboard::MimeReaderError::WriteFailed)?;
            offset += n;
        }
        Ok(())
    }
}

// Parity tail S7-4b (routing evidence, no reference counterpart): reader
// contents are read into a `std.Io.Writer.Allocating` over `ctx.alloc`,
// which `Handler::paste` sets to the terminal's `gpa()`: the bytes the
// reader writes are allocated through it (at least one allocation, holding
// the 16 bytes read) and freed before `paste` returns.
#[test]
fn paste_reader_contents_route_through_gpa() {
    use crate::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    crate::alloc::testing::with_handle(&c, |alloc| {
        let mut t = Terminal::init(alloc, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();
        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        let mut reader = PasteReader::new(&[b"text/plain"], &[b"echo hello world"]);
        let (a0, l0) = (counting.alloc_calls.get(), counting.live_bytes.get());
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: reader.contents(),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(b"echo hello world".as_slice(), pc_written().as_slice());
        assert!(
            counting.alloc_calls.get() > a0,
            "the read bytes are allocated through gpa()"
        );
        assert!(counting.peak_bytes.get() >= l0 + 16, "they are held there");
        assert_eq!(l0, counting.live_bytes.get(), "and freed");
        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(alloc);
    });
    assert_eq!(0, counting.live_bytes.get());
}

// test "paste: reader contents are read on demand"
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION: sys::io_native OTP entropy reaches the real \
                  OS RNG (`arc4random_buf`), which Miri does not support; the \
                  paste/grant aliasing is Miri-verified by the sibling 5522 \
                  tests that use the sys override / io_failing"
)]
fn paste_reader_contents_are_read_on_demand() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    handler.effects.clipboard_read = Some(pc_clipboard_read);

    // Unsafe text is refused from one read with nothing written; the
    // image is never read.
    let mut reader = PasteReader::new(
        &[b"image/png", b"text/plain"],
        &[b"\x89PNG", b"echo hi\nrm -rf /\n"],
    );
    assert_eq!(
        Err(PasteError::UnsafePaste),
        handler.paste(Paste {
            source: paste_pkg::Source::default(),
            contents: reader.contents(),
            allow_unsafe: false,
        })
    );
    assert_eq!(0, pc(|c| c.write_count));
    assert_eq!(0, reader.reads[0]);
    assert_eq!(1, reader.reads[1]);

    // Allowed, the text is read once, encoded.
    reader.reads = [0; 4];
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: reader.contents(),
            allow_unsafe: true,
        })
        .unwrap());
    assert_eq!(b"echo hi\rrm -rf /\r".as_slice(), pc_written().as_slice());
    assert_eq!(0, reader.reads[0]);
    assert_eq!(1, reader.reads[1]);

    // Safe text is read once too: buffered for the check, then written.
    pc_reset();
    let mut reader = PasteReader::new(&[b"text/plain"], &[b"hello world"]);
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: reader.contents(),
            allow_unsafe: false,
        })
        .unwrap());
    assert_eq!(b"hello world".as_slice(), pc_written().as_slice());
    assert_eq!(1, reader.reads[0]);

    // Empty text is nothing to paste, found on the one read.
    pc_reset();
    let mut reader = PasteReader::new(&[b"text/plain"], &[b""]);
    assert!(!handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: reader.contents(),
            allow_unsafe: false,
        })
        .unwrap());
    assert_eq!(0, pc(|c| c.write_count));
    assert_eq!(1, reader.reads[0]);

    // A paste event lists the types and reads nothing at all.
    pc_reset();
    unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };
    let mut reader = PasteReader::new(&[b"text/plain", b"image/png"], &[b"secret", b"\x89PNG"]);
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: reader.contents(),
            allow_unsafe: false,
        })
        .unwrap());
    let written = pc_written();
    assert!(written
        .windows(b";dGV4dC9wbGFpbiBpbWFnZS9wbmcK\x1b\\".len())
        .any(|w| w == b";dGV4dC9wbGFpbiBpbWFnZS9wbmcK\x1b\\"));
    assert!(!written.windows(b"secret".len()).any(|w| w == b"secret"));
    assert_eq!(0, reader.reads[0]);
    assert_eq!(0, reader.reads[1]);

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: reader failure writes nothing"
#[test]
fn paste_reader_failure_writes_nothing() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    unsafe { (*ptr).modes.set(modespkg::Mode::BracketedPaste, true) };

    // The read is buffered whole before anything is written, so a
    // mid-read failure discards the buffer, checked or not.
    let mut reader = PasteReader::new(&[b"text/plain"], &[b"hello world"]);
    reader.fail_after = Some(6);
    assert_eq!(
        Err(PasteError::ReadFailed),
        handler.paste(Paste {
            source: paste_pkg::Source::default(),
            contents: reader.contents(),
            allow_unsafe: false,
        })
    );
    assert_eq!(0, pc(|c| c.write_count));

    assert_eq!(
        Err(PasteError::ReadFailed),
        handler.paste(Paste {
            source: paste_pkg::Source::default(),
            contents: reader.contents(),
            allow_unsafe: true,
        })
    );
    assert_eq!(0, pc(|c| c.write_count));

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: mode 5522 sends an event the program can read with"
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION: sys::io_native OTP entropy reaches the real \
                  OS RNG (`arc4random_buf`), which Miri does not support; the \
                  paste/grant aliasing is Miri-verified by the sibling 5522 \
                  tests that use the sys override / io_failing"
)]
fn paste_mode_5522_sends_an_event_the_program_can_read_with() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    handler.effects.clipboard_read = Some(pc_clipboard_read);
    let mut s = Stream::init(stream::Options {
        allocator: Some(crate::alloc::Global),
        handler,
        continuation_max_bytes: None,
        osc_unknown_max_bytes: 0,
    });
    unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };
    unsafe { (*ptr).modes.set(modespkg::Mode::BracketedPaste, true) };

    // Every representation is listed, the data is never written, and
    // the one-time password rides on every packet.
    assert!(s
        .handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[
                clipboard::Content {
                    mime: b"text/plain",
                    data: b"secret",
                },
                clipboard::Content {
                    mime: b"image/png",
                    data: b"",
                },
            ]),
            allow_unsafe: false,
        })
        .unwrap());
    assert_eq!(1, pc(|c| c.write_count));
    let written = pc_written();
    assert_eq!(
        3,
        written
            .windows(b"\x1b]5522;".len())
            .filter(|w| *w == b"\x1b]5522;")
            .count()
    );
    assert!(!written.windows(b"secret".len()).any(|w| w == b"secret"));
    assert!(!written
        .windows(b"\x1b[200~".len())
        .any(|w| w == b"\x1b[200~"));

    // OK packet: parse the (base64) password out.
    let ok_prefix = b"\x1b]5522;type=read:status=OK:pw=";
    assert!(written.starts_with(ok_prefix));
    let pw_end = ok_prefix.len()
        + written[ok_prefix.len()..]
            .windows(2)
            .position(|w| w == b"\x1b\\")
            .unwrap();
    // Copied out since the capture buffer is reused below.
    let pw_b64: Vec<u8> = written[ok_prefix.len()..pw_end].to_vec();
    // std.base64.standard.Encoder.calcSize(kitty_clipboard.otp_len)
    assert_eq!(kitty::clipboard::otp_len.div_ceil(3) * 4, pw_b64.len());

    // Listing packet: base64 of "text/plain image/png\n".
    let mut expected: Vec<u8> = Vec::new();
    expected.extend_from_slice(b"\x1b]5522;type=read:status=OK:pw=");
    expected.extend_from_slice(&pw_b64);
    expected.extend_from_slice(b"\x1b\\");
    expected.extend_from_slice(b"\x1b]5522;type=read:status=DATA:mime=Lg==:pw=");
    expected.extend_from_slice(&pw_b64);
    expected.extend_from_slice(b";dGV4dC9wbGFpbiBpbWFnZS9wbmcK\x1b\\");
    expected.extend_from_slice(b"\x1b]5522;type=read:status=DONE:pw=");
    expected.extend_from_slice(&pw_b64);
    expected.extend_from_slice(b"\x1b\\");
    assert_eq!(expected, written);

    // The program reads with the password and the name "Paste event":
    // the read arrives granted, exactly once.
    let mut read: Vec<u8> = Vec::new();
    read.extend_from_slice(b"\x1b]5522;type=read:pw=");
    read.extend_from_slice(&pw_b64);
    read.extend_from_slice(b":name=UGFzdGUgZXZlbnQ=;dGV4dC9wbGFpbg==\x1b\\");
    pc_reset();
    s.next_slice(&read);
    assert_eq!(1, pc(|c| c.read_count));
    assert!(pc(|c| c.last_read_granted));
    pc(|c| assert_eq!(b"Paste event".as_slice(), c.last_read_name.as_slice()));
    let mut expected: Vec<u8> = Vec::new();
    expected.extend_from_slice(b"\x1b]5522;type=read:status=OK\x1b\\");
    expected.extend_from_slice(
        b"\x1b]5522;type=read:status=DATA:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==\x1b\\",
    );
    expected.extend_from_slice(b"\x1b]5522;type=read:status=DONE\x1b\\");
    assert_eq!(expected, pc_written());

    // The password was one-time: a second read is not granted.
    pc_reset();
    s.next_slice(&read);
    assert_eq!(1, pc(|c| c.read_count));
    assert!(!pc(|c| c.last_read_granted));
    assert_eq!(0, pc(|c| c.read_granted_count));

    // Every event mints a fresh password.
    pc_reset();
    assert!(s
        .handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"secret",
            }]),
            allow_unsafe: false,
        })
        .unwrap());
    let written = pc_written();
    assert!(!written
        .windows(pw_b64.len())
        .any(|w| w == pw_b64.as_slice()));

    s.deinit();
    t.deinit(Global);
}

// test "paste: mode 5522 reports the selection as primary"
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION: sys::io_native OTP entropy reaches the real \
                  OS RNG (`arc4random_buf`), which Miri does not support; the \
                  paste/grant aliasing is Miri-verified by the sibling 5522 \
                  tests that use the sys override / io_failing"
)]
fn paste_mode_5522_reports_the_selection_as_primary() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    handler.effects.clipboard_read = Some(pc_clipboard_read);
    unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };

    for location in [clipboard::Location::Primary, clipboard::Location::Selection] {
        pc_reset();
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::Clipboard(location),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"x",
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        let written = pc_written();
        assert!(written.starts_with(b"\x1b]5522;type=read:status=OK:loc=primary:pw="));
        // Only on the OK packet.
        assert_eq!(
            1,
            written
                .windows(b"loc=primary".len())
                .filter(|w| *w == b"loc=primary")
                .count()
        );
    }

    pc_reset();
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::Clipboard(clipboard::Location::Standard),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"x",
            }]),
            allow_unsafe: false,
        })
        .unwrap());
    let written = pc_written();
    assert!(!written.windows(b"loc=".len()).any(|w| w == b"loc="));

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: mode 5522 without clipboard_read pastes text"
#[test]
fn paste_mode_5522_without_clipboard_read_pastes_text() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };

    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"hello",
            }]),
            allow_unsafe: false,
        })
        .unwrap());
    assert_eq!(b"hello".as_slice(), pc_written().as_slice());
    assert_eq!(0, handler.kitty_clipboard_grants.len());

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: text source never becomes an event"
#[test]
fn paste_text_source_never_becomes_an_event() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    handler.effects.clipboard_read = Some(pc_clipboard_read);
    unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };

    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::Text,
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"committed",
            }]),
            allow_unsafe: false,
        })
        .unwrap());
    assert_eq!(b"committed".as_slice(), pc_written().as_slice());
    assert_eq!(0, handler.kitty_clipboard_grants.len());

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: mode 5522 without entropy fails and records no grant"
#[test]
fn paste_mode_5522_without_entropy_fails_and_records_no_grant() {
    // var t: Terminal = try .init(std.Io.failing, testing.allocator, ...);
    // PORT NOTE: rustty's Terminal stores its Io stand-in as a field (see
    // `Terminal.io`); `std.Io.failing` maps to `sys::io_failing`. The
    // `random_secure` override slot is shared process state, so hold the
    // sys test lock like every entropy-dependent test.
    let _lock = crate::sys::random_secure_test_lock.lock().unwrap();

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    t.io = crate::sys::io_failing;
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    handler.effects.clipboard_read = Some(pc_clipboard_read);
    unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };

    assert_eq!(
        Err(PasteError::EntropyUnavailable),
        handler.paste(Paste {
            source: paste_pkg::Source::default(),
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"secret",
            }]),
            allow_unsafe: false,
        })
    );
    assert_eq!(0, pc(|c| c.write_count));
    assert_eq!(0, handler.kitty_clipboard_grants.len());

    // Text pastes need no entropy and still work.
    assert!(handler
        .paste(Paste {
            source: paste_pkg::Source::Text,
            contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                mime: b"text/plain",
                data: b"hello",
            }]),
            allow_unsafe: false,
        })
        .unwrap());
    assert_eq!(b"hello".as_slice(), pc_written().as_slice());

    use crate::stream::Handler as _;
    handler.deinit();
    t.deinit(Global);
}

// test "paste: mode 5522 DECRQM requires clipboard read effect"
#[test]
fn paste_mode_5522_decrqm_requires_clipboard_read_effect() {
    // if (comptime build_options.artifact != .lib) return error.SkipZigTest;
    // PORT NOTE (R8): the reference runs this only in the lib artifact
    // (where the kitty_paste_events mode entry is unconditionally
    // enabled); the same comptime gate is carried here.
    if !matches!(
        crate::options::ARTIFACT,
        crate::build_options::Artifact::Lib
    ) {
        // On non-lib artifacts the mode entry's enablement is
        // platform-dependent; the reference skips.
        return;
    }

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    let mut s = Stream::init(stream::Options {
        allocator: Some(crate::alloc::Global),
        handler,
        continuation_max_bytes: None,
        osc_unknown_max_bytes: 0,
    });

    // A paste event is unusable if its follow-up read can't be served, so
    // the mode isn't advertised without a clipboard read effect.
    s.next_slice(b"\x1B[?5522$p");
    assert_eq!(b"\x1B[?5522;0$y".as_slice(), pc_written().as_slice());

    pc_reset();
    s.next_slice(b"\x1B[?5522h");
    assert!(unsafe { (*ptr).modes.get(modespkg::Mode::KittyPasteEvents) });
    s.next_slice(b"\x1B[?5522$p");
    assert_eq!(b"\x1B[?5522;0$y".as_slice(), pc_written().as_slice());

    // Installing the effect makes the current mode state reportable.
    pc_reset();
    s.handler.effects.clipboard_read = Some(pc_clipboard_read);
    s.next_slice(b"\x1B[?5522$p");
    assert_eq!(b"\x1B[?5522;1$y".as_slice(), pc_written().as_slice());

    pc_reset();
    s.next_slice(b"\x1B[?5522l");
    assert!(!unsafe { (*ptr).modes.get(modespkg::Mode::KittyPasteEvents) });
    s.next_slice(b"\x1B[?5522$p");
    assert_eq!(b"\x1B[?5522;2$y".as_slice(), pc_written().as_slice());

    // Removing the effect stops advertising the capability again.
    pc_reset();
    s.handler.effects.clipboard_read = None;
    s.next_slice(b"\x1B[?5522$p");
    assert_eq!(b"\x1B[?5522;0$y".as_slice(), pc_written().as_slice());

    s.deinit();
    t.deinit(Global);
}

// test "full reset drops kitty clipboard grants"
#[test]
fn full_reset_drops_kitty_clipboard_grants() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let ptr: *mut Terminal = &mut t;
    pc_reset();

    let mut handler = Handler::init(ptr);
    handler.effects.write_pty = Some(pc_write_pty);
    handler.effects.clipboard_read = Some(pc_clipboard_read);
    let mut s = Stream::init(stream::Options {
        allocator: Some(crate::alloc::Global),
        handler,
        continuation_max_bytes: None,
        osc_unknown_max_bytes: 0,
    });

    s.handler
        .kitty_clipboard_grants
        .grant(Global, b"pw", kitty::clipboard::Direction::Read, false)
        .unwrap();
    assert_eq!(1, s.handler.kitty_clipboard_grants.len());

    s.next_slice(b"\x1Bc");
    assert_eq!(0, s.handler.kitty_clipboard_grants.len());

    // A read with the old password is no longer granted, and the
    // handler keeps working (grants can be recorded again).
    pc_reset();
    s.next_slice(b"\x1b]5522;type=read:pw=cHc=:name=YXBw;dGV4dC9wbGFpbg==\x1b\\");
    assert_eq!(1, pc(|c| c.read_count));
    assert!(!pc(|c| c.last_read_granted));
    s.handler
        .kitty_clipboard_grants
        .grant(Global, b"pw", kitty::clipboard::Direction::Read, false)
        .unwrap();
    assert_eq!(1, s.handler.kitty_clipboard_grants.len());

    s.deinit();
    t.deinit(Global);
}
