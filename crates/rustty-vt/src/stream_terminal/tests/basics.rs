//! stream_terminal tests: `Handler.resize`, the render_hold effect,
//! basic printing and the `semantic_failure` routing.

use super::*;

// -- render_hold effect tests (oracle e50779498) ---------------------------
//
// PORT NOTE: the reference hangs `var events: [8]bool` / `var len` on a
// local `S` struct; the callback is a bare `fn` pointer here, so the
// captures live in thread-locals (the module's `CAPTURE` idiom). A `Vec`
// replaces the fixed `[8]bool` + `len` pair; `S.events[0..S.len]` is the
// whole vector.
thread_local! {
    static HOLD_EVENTS: core::cell::RefCell<Vec<bool>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

fn hold_events() -> Vec<bool> {
    HOLD_EVENTS.with(|e| e.borrow().clone())
}

fn hold_events_reset() {
    HOLD_EVENTS.with(|e| e.borrow_mut().clear());
}

// test "render hold effect fires on synchronized output transitions only"
#[test]
fn render_hold_effect_fires_on_synchronized_output_transitions_only() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // fn hold(_: *Handler, held: bool) void {
    //     events[len] = held;
    //     len += 1;
    // }
    fn hold(_: &mut Handler, held: bool) {
        HOLD_EVENTS.with(|e| e.borrow_mut().push(held));
    }
    // S.len = 0;
    hold_events_reset();

    // var handler: Handler = .init(&t);
    // handler.effects.render_hold = &S.hold;
    // var s: Stream = .init(.{ .allocator = testing.allocator, .handler = handler });
    // defer s.deinit();
    let mut s = t.vt_stream();
    s.handler.effects.render_hold = Some(hold);

    // Reset without a hold is ignored
    s.next_slice(b"\x1b[?2026l");
    assert_eq!(0, hold_events().len());

    // A set during a hold is ignored since the screen is half-drawn
    s.next_slice(b"\x1b[?2026hA\x1b[?2026hB\x1b[?2026l\x1b[?2026l");
    assert_eq!(vec![true, false], hold_events());

    // Save and restore report the same way as set and reset
    hold_events_reset();
    s.next_slice(b"\x1b[?2026s\x1b[?2026h\x1b[?2026r\x1b[?2026r");
    assert_eq!(vec![true, false], hold_events());
    // try testing.expect(!t.modes.get(.synchronized_output));
    // SAFETY: the stream's handler aliases `t`; read through its raw
    // pointer while the stream (which borrows `t`) is live.
    assert!(!unsafe {
        (*s.handler.terminal)
            .modes
            .get(modespkg::Mode::SynchronizedOutput)
    });

    // Full reset
    hold_events_reset();
    s.next_slice(b"\x1b[?2026h\x1bc\x1bc");
    assert_eq!(vec![true, false], hold_events());

    // Resize
    hold_events_reset();
    s.next_slice(b"\x1b[?2026h");
    s.handler
        .resize(crate::terminal::Resize {
            cols: 80,
            rows: 24,
            cell_size_px: None,
        })
        .unwrap();
    s.handler
        .resize(crate::terminal::Resize {
            cols: 80,
            rows: 24,
            cell_size_px: None,
        })
        .unwrap();
    assert_eq!(vec![true, false], hold_events());

    s.deinit();
    t.deinit(Global);
}

// test "render hold effect can snapshot the frame when the hold begins"
#[test]
fn render_hold_effect_can_snapshot_the_frame_when_the_hold_begins() {
    use crate::render::RenderState;
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

    // This is how a renderer is expected to use the effect: capture the
    // frame when the hold begins and then leave the render state alone
    // until the hold ends.
    //
    // PORT NOTE: the reference's `S.state`/`S.snapshots` statics live in
    // thread-locals here (`RenderState` is not const-constructible, so the
    // state slot is an `Option` filled before the stream runs).
    thread_local! {
        static STATE: core::cell::RefCell<Option<RenderState>> =
            const { core::cell::RefCell::new(None) };
        static SNAPSHOTS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    // fn hold(h: *Handler, held: bool) void {
    //     if (!held) return;
    //     state.update(testing.allocator, h.terminal) catch unreachable;
    //     snapshots += 1;
    // }
    fn hold(h: &mut Handler, held: bool) {
        if !held {
            return;
        }
        // SAFETY: `h.terminal` is the live terminal the handler aliases;
        // no other `&mut Terminal` is live across the effect callback
        // (the render-hold call sites drop every `term()` borrow first).
        let term = unsafe { &mut *h.terminal };
        STATE.with(|s| {
            s.borrow_mut()
                .as_mut()
                .unwrap()
                .update(Global, term)
                .expect("unreachable")
        });
        SNAPSHOTS.with(|c| c.set(c.get() + 1));
    }

    // fn char(y: usize, x: usize) u21 {
    //     return state.row_data.items(.cells)[y].get(x).raw.codepoint();
    // }
    fn char(y: usize, x: usize) -> u32 {
        STATE.with(|s| {
            s.borrow().as_ref().unwrap().row_data[y]
                .cells
                .get(x)
                .raw
                .codepoint()
        })
    }

    // S.state = .empty;
    // S.snapshots = 0;
    // defer S.state.deinit(alloc);
    STATE.with(|s| *s.borrow_mut() = Some(RenderState::empty()));
    SNAPSHOTS.with(|c| c.set(0));

    // var handler: Handler = .init(&t);
    // handler.effects.render_hold = &S.hold;
    // var s: Stream = .init(.{ .allocator = alloc, .handler = handler });
    // defer s.deinit();
    let mut s = t.vt_stream();
    s.handler.effects.render_hold = Some(hold);

    // Normal output, the start of a hold, and part of the next frame all
    // arrive in one write with no draw in between. The captured frame
    // must have all of the output before the hold and none after.
    s.next_slice(b"AB\x1b[?2026h\x1b[HXY");
    assert_eq!(1, SNAPSHOTS.with(|c| c.get()));
    assert_eq!('A' as u32, char(0, 0));
    assert_eq!('B' as u32, char(0, 1));

    // The frame is finished and the next hold begins before we ever
    // draw. The finished frame must be captured, not lost.
    s.next_slice(b"Z\x1b[?2026l\x1b[?2026h\x1b[H123");
    assert_eq!(2, SNAPSHOTS.with(|c| c.get()));
    assert_eq!('X' as u32, char(0, 0));
    assert_eq!('Y' as u32, char(0, 1));
    assert_eq!('Z' as u32, char(0, 2));

    s.deinit();
    if let Some(mut state) = STATE.with(|s| s.borrow_mut().take()) {
        state.deinit(alloc);
    }
    t.deinit(alloc);
}

// test "resize clears synchronized output on unchanged cell dimensions"
#[test]
fn resize_clears_synchronized_output_on_unchanged_cell_dimensions() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    t.modes.set(modespkg::Mode::SynchronizedOutput, true);
    {
        let mut s = t.vt_stream();
        s.handler
            .resize(crate::terminal::Resize {
                cols: 80,
                rows: 24,
                cell_size_px: Some(cell_9x18()),
            })
            .unwrap();
    }

    assert!(!t.modes.get(modespkg::Mode::SynchronizedOutput));
    assert_eq!(720_u32, t.width_px);
    assert_eq!(432_u32, t.height_px);

    t.deinit(Global);
}

// test "unknown APC effect callback"
#[test]
fn unknown_apc_effect_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // const S = struct {
    //     var count: usize = 0;
    //     var content: [16]u8 = undefined;
    //     var content_len: usize = undefined;
    //     var truncated: bool = undefined;
    //     fn unknownSequence(_: *Handler, value: Handler.UnknownSequence) void { ... }
    // };
    // PORT NOTE: the reference hangs `var`s on a local `S` struct; the
    // callback is a bare `fn` pointer here, so the captures live in
    // thread-locals (the module's established `CAPTURE` idiom).
    thread_local! {
        static UNKNOWN_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static UNKNOWN_CONTENT: core::cell::RefCell<Vec<u8>> =
            const { core::cell::RefCell::new(Vec::new()) };
        static UNKNOWN_TRUNCATED: core::cell::Cell<bool> =
            const { core::cell::Cell::new(false) };
    }

    fn unknown_sequence(_: &mut Handler, value: UnknownSequence) {
        match value {
            UnknownSequence::Apc(apc_value) => {
                UNKNOWN_CONTENT.with(|c| *c.borrow_mut() = apc_value.content.to_vec());
                UNKNOWN_TRUNCATED.with(|c| c.set(apc_value.truncated));
            }
        }
        UNKNOWN_COUNT.with(|c| c.set(c.get() + 1));
    }
    // S.count = 0;
    UNKNOWN_COUNT.with(|c| c.set(0));

    // var handler: Handler = .init(&t);
    // handler.unknown_sequence = &S.unknownSequence;
    // handler.apc_handler.unknown_max_bytes = 8;
    // var s: Stream = .init(.{ .allocator = testing.allocator, .handler = handler });
    // defer s.deinit();
    let mut s = t.vt_stream();
    s.handler.unknown_sequence = Some(unknown_sequence);
    s.handler.apc_handler.unknown_max_bytes = 8;

    // Unknown OSC commands retain their legacy behavior and are ignored.
    s.next_slice(b"\x1B]999;abcdef\x07");
    s.next_slice(b"\x1B_abcd;payload\x1B\\");

    assert_eq!(1, UNKNOWN_COUNT.with(|c| c.get()));
    assert_eq!(
        b"abcd;pay".as_slice(),
        UNKNOWN_CONTENT.with(|c| c.borrow().clone()).as_slice()
    );
    assert!(UNKNOWN_TRUNCATED.with(|c| c.get()));

    // Aborted unknown APCs are suppressed.
    s.next_slice(b"\x1B_Xpayload\x18");
    assert_eq!(1, UNKNOWN_COUNT.with(|c| c.get()));

    drop(s);
    t.deinit(Global);
}

// test "resize reports mode 2048 geometry"
#[test]
fn resize_reports_mode_2048_geometry() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    calls_reset();

    t.modes.set(modespkg::Mode::InBandSizeReports, true);
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(count_write_pty);
        s.handler
            .resize(crate::terminal::Resize {
                cols: 100,
                rows: 40,
                cell_size_px: Some(cell_9x18()),
            })
            .unwrap();
    }

    assert_eq!(
        b"\x1B[48;40;100;720;900t".as_slice(),
        cap_take().unwrap().as_slice()
    );

    t.deinit(Global);
}

// test "resize suppresses mode 2048 reports"
#[test]
fn resize_suppresses_mode_2048_reports() {
    calls_reset();

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // Disabled mode suppresses a report even with pixels and a callback.
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(count_write_pty);
        s.handler
            .resize(crate::terminal::Resize {
                cols: 80,
                rows: 24,
                cell_size_px: Some(cell_9x18()),
            })
            .unwrap();
    }
    assert_eq!(0_usize, calls_get());

    // Missing pixel geometry suppresses a report even with the mode enabled.
    t.modes.set(modespkg::Mode::InBandSizeReports, true);
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(count_write_pty);
        s.handler
            .resize(crate::terminal::Resize {
                cols: 80,
                rows: 24,
                cell_size_px: None,
            })
            .unwrap();
    }
    assert_eq!(0_usize, calls_get());

    // A read-only stream has no write effect and remains successful.
    let mut readonly_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
    readonly_terminal
        .modes
        .set(modespkg::Mode::InBandSizeReports, true);
    {
        let mut readonly_stream = readonly_terminal.vt_stream();
        readonly_stream
            .handler
            .resize(crate::terminal::Resize {
                cols: 80,
                rows: 24,
                cell_size_px: Some(cell_9x18()),
            })
            .unwrap();
    }
    assert_eq!(0_usize, calls_get());

    readonly_terminal.deinit(Global);
    t.deinit(Global);
}

// test "resize failure preserves terminal state and does not write"
//
// WS18 release lane (CORRECTED, parity S166-F2): this test injects a failure
// through a `rustty_util::tripwire` fail point, and `Module::enabled()` is
// `cfg!(any(test, debug_assertions, feature = "tripwire-test"))` — NOT
// `cfg!(debug_assertions)`, which is what the note here used to claim. The
// seam it arms (`tabstops::RESIZE_TW`) is `cfg(test)`, so both the seam and
// the tripwire are live under `cargo test -p rustty-vt --release`, and the
// `#[cfg_attr(not(debug_assertions), ignore)]` that used to sit here only
// ever suppressed a test that would have passed. The reference runs its
// `FailingAllocator` equivalent at every optimize mode.
#[test]
fn resize_failure_preserves_terminal_state_and_does_not_write() {
    calls_reset();

    let mut t = Terminal::init(Global, opts(10, 1)).unwrap();

    t.modes.set(modespkg::Mode::SynchronizedOutput, true);
    t.modes.set(modespkg::Mode::InBandSizeReports, true);

    // PORT NOTE: the reference swaps in a `testing.FailingAllocator`. This
    // port's seam is the `Tabstops::resize` tripwire, whose only fail point
    // (`DynamicAlloc`) sits at the dynamic-stops allocation that
    // `Tabstops::init` reaches from `Terminal::resize`. 513 columns is past
    // `PREALLOC_COLUMNS`, so the allocation IS attempted.
    crate::tabstops::resize_tw_error_always(crate::tabstops::ResizeFp::DynamicAlloc);
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(count_write_pty);
        assert_eq!(
            Err(crate::terminal::ResizeError::OutOfMemory),
            s.handler.resize(crate::terminal::Resize {
                cols: 513,
                rows: 1,
                cell_size_px: Some(cell_9x18()),
            })
        );
    }
    crate::tabstops::resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();

    // oracle a3c1caba5: the failed resize now ROLLS BACK, so
    // synchronized output stays as it was and the pixel geometry is
    // untouched (it was never set on this terminal).
    assert!(t.modes.get(modespkg::Mode::SynchronizedOutput));
    assert_eq!(0_usize, calls_get());
    assert_eq!(10 as size::CellCountInt, t.cols);
    assert_eq!(0_u32, t.width_px);
    assert_eq!(0_u32, t.height_px);

    t.deinit(Global);
}

// test "resize effects do not change canonical terminal state"
#[test]
fn resize_effects_do_not_change_canonical_terminal_state() {
    let mut authoritative = Terminal::init(Global, opts(10, 5)).unwrap();
    let mut readonly = Terminal::init(Global, opts(10, 5)).unwrap();

    authoritative
        .modes
        .set(modespkg::Mode::InBandSizeReports, true);
    readonly.modes.set(modespkg::Mode::InBandSizeReports, true);

    let value = crate::terminal::Resize {
        cols: 20,
        rows: 10,
        cell_size_px: Some(cell_9x18()),
    };
    {
        let mut authoritative_stream = authoritative.vt_stream();
        authoritative_stream.handler.effects.write_pty = Some(noop_write_pty);
        authoritative_stream.handler.resize(value).unwrap();
    }
    {
        let mut readonly_stream = readonly.vt_stream();
        readonly_stream.handler.resize(value).unwrap();
    }

    assert_eq!(authoritative.cols, readonly.cols);
    assert_eq!(authoritative.rows, readonly.rows);
    assert_eq!(authoritative.width_px, readonly.width_px);
    assert_eq!(authoritative.height_px, readonly.height_px);
    // std.meta.eql(authoritative.modes, readonly.modes)
    assert_eq!(authoritative.modes.values, readonly.modes.values);
    assert_eq!(authoritative.modes.saved, readonly.modes.saved);
    assert_eq!(authoritative.modes.default, readonly.modes.default);
    assert_eq!(
        authoritative.scrolling_region.top,
        readonly.scrolling_region.top
    );
    assert_eq!(
        authoritative.scrolling_region.bottom,
        readonly.scrolling_region.bottom
    );
    assert_eq!(
        authoritative.scrolling_region.left,
        readonly.scrolling_region.left
    );
    assert_eq!(
        authoritative.scrolling_region.right,
        readonly.scrolling_region.right
    );

    readonly.deinit(Global);
    authoritative.deinit(Global);
}

fn noop_write_pty(_: &mut Handler, _: &[u8]) {}

// test "basic print"
#[test]
fn basic_print() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(b"Hello");
    }

    assert_eq!(5_usize, t.active_mut().cursor.x as usize);
    assert_eq!(0_usize, t.active_mut().cursor.y as usize);

    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"Hello");

    t.deinit(Global);
}

// test "semantic failure is sticky while processing continues"
//
// Ported as written (parity tail S7-3): the terminal runs over a
// `FailingAllocator`, as in the reference. (A `SetStringFp` tripwire stood
// in for it while `title` lived on the Rust heap.)
#[test]
fn semantic_failure_is_sticky_while_processing_continues() {
    // var failing = testing.FailingAllocator.init(testing.allocator, .{});
    // const alloc = failing.allocator();
    let failing = crate::alloc::testing::FailingAllocator::over_heap(
        crate::alloc::testing::Config::default(),
    );
    let failing_c = failing.allocator();
    // SAFETY: `failing_c` and `failing` outlive `t`, deinited below.
    let alloc = unsafe { crate::alloc::VtAllocator::from_c(core::ptr::NonNull::from(&failing_c)) };
    let mut t = Terminal::init(alloc, opts(10, 2)).unwrap();

    {
        let mut s = t.vt_stream();
        assert!(!s.handler.semantic_failure);

        // Setting the title is a terminal-owned semantic update. Force its
        // allocation to fail at the central vtFallible boundary.
        failing.fail_index.set(failing.alloc_index.get());
        s.next_slice(b"\x1B]2;unavailable\x1B\\");
        assert!(failing.has_induced_failure.get());
        assert!(s.handler.semantic_failure);

        // failing.fail_index = std.math.maxInt(usize);
        failing.fail_index.set(usize::MAX);

        // Later input and RIS remain best-effort and never clear the
        // diagnostic.
        s.next_slice(b"ignored");
        s.next_slice(b"\x1Bc");
        s.next_slice(b"OK");
        assert!(s.handler.semantic_failure);
    }

    let str = t.plain_string(Global).unwrap();
    assert_eq!(b"OK".as_slice(), str.as_slice());

    // A new execution root starts without inheriting the diagnostic.
    let fresh = Handler::init(&raw mut t);
    assert!(!fresh.semantic_failure);

    t.deinit(alloc);
}

// NEW TEST (no oracle counterpart; the reference gets this for free from
// `vtFallible`'s single `try` chain): the `set_mode` / `reset_mode` /
// `restore_mode` actions are `try self.setMode(...)` in the reference, so a
// failure inside `setMode` reaches the `vt()` catch and sets
// `semantic_failure`. `setMode`'s fallible arms are the alt-screen switches
// and `132_column` -> `deccolm` -> `resize`; `resize` is the one with an
// established failure-injection seam (`terminal::RESIZE_TW`), and it is the
// only one reachable from all THREE actions (the alt-screen seam can only
// fail on the lazy first creation of the alternate screen, so it cannot
// exercise `reset_mode`/`restore_mode`).
//
// Each arm gets its own terminal because `semantic_failure` is sticky.
#[test]
fn semantic_failure_is_set_by_a_failing_mode_action() {
    use crate::terminal::{resize_tw_end, resize_tw_error_always, ResizeFp};
    use rustty_util::tripwire::ResetMode;

    // .set_mode
    {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // DECSET 40 is required before mode 3 does anything; it is an
            // infallible arm and must leave the flag clear.
            s.next_slice(b"\x1b[?40h");
            assert!(!s.handler.semantic_failure);

            resize_tw_error_always(ResizeFp::Tabstops);
            s.next_slice(b"\x1b[?3h");
            resize_tw_end(ResetMode::Reset).unwrap();
            assert!(s.handler.semantic_failure);
        }
        t.deinit(Global);
    }

    // .reset_mode (with the unarmed success path as the negative control)
    {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[?40h");
            s.next_slice(b"\x1b[?3h");
            assert!(!s.handler.semantic_failure);

            resize_tw_error_always(ResizeFp::Tabstops);
            s.next_slice(b"\x1b[?3l");
            resize_tw_end(ResetMode::Reset).unwrap();
            assert!(s.handler.semantic_failure);
        }
        // The successful DECSET 3 above really did resize (side effects on
        // the success path are unchanged), and the failed DECRST 3 rolled
        // the geometry back rather than half-applying it.
        assert_eq!(132, t.cols);
        t.deinit(Global);
    }

    // .restore_mode
    {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[?40h");
            s.next_slice(b"\x1b[?3h");
            s.next_slice(b"\x1b[?3s");
            s.next_slice(b"\x1b[?3l");
            assert!(!s.handler.semantic_failure);
            assert_eq!(80, s.handler.term().cols);

            resize_tw_error_always(ResizeFp::Tabstops);
            s.next_slice(b"\x1b[?3r");
            resize_tw_end(ResetMode::Reset).unwrap();
            assert!(s.handler.semantic_failure);
        }
        t.deinit(Global);
    }
}

// NEW TEST (no oracle counterpart): `.start_hyperlink => try
// self.terminal.screens.active.startHyperlink(...)` is inside the
// reference's `vtFallible`, so an OSC 8 failure sets `semantic_failure`.
// The injection is a `FailingAllocator` swapped in as the active screen's
// allocator (the same seam `Screen: implicit hyperlink ID wraps` uses; parity
// tail S7-3 replaced a tripwire stand-in).
#[test]
fn semantic_failure_is_set_by_a_failing_start_hyperlink() {
    let failing = crate::alloc::testing::FailingAllocator::over_heap(
        crate::alloc::testing::Config::default(),
    );
    let failing_c = failing.allocator();
    // SAFETY: `failing_c` and `failing` outlive every use below.
    let failing_alloc =
        unsafe { crate::alloc::VtAllocator::from_c(core::ptr::NonNull::from(&failing_c)) };

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();

        // NEGATIVE CONTROL: the arm is reached and succeeds, flag stays clear.
        s.next_slice(b"\x1b]8;;http://example.com/a\x1b\\");
        assert!(!s.handler.semantic_failure);
        assert!(s.handler.term().active_mut().cursor.hyperlink_id != 0);

        // s.alloc = failing.allocator(); failing.fail_index = failing.alloc_index;
        let original_alloc = s.handler.term().active_mut().alloc;
        s.handler.term().active_mut().alloc = failing_alloc;
        failing.fail_index.set(failing.alloc_index.get());
        // S166-F3: `vt_catch` logs `log.warn("error handling VT action
        // action={} err={}", .{ action, err })` (stream_terminal.zig:344-347)
        // on this exact shared-catch path.
        let cap = crate::log_capture::start();
        s.next_slice(b"\x1b]8;;http://example.com/b\x1b\\");
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "stream_terminal"
                    && msg.starts_with("error handling VT action action=start_hyperlink err=")),
            "expected the vt_catch diagnostic, got: {messages:?}"
        );
        drop(cap);
        s.handler.term().active_mut().alloc = original_alloc;
        assert!(failing.has_induced_failure.get());
        assert!(s.handler.semantic_failure);

        // Processing continues after the failure (best-effort, like every
        // other arm that funnels through the shared catch).
        s.next_slice(b"OK");
    }
    let str = t.plain_string(Global).unwrap();
    assert_eq!(b"OK".as_slice(), str.as_slice());

    t.deinit(Global);
}

// NEGATIVE CONTROL (P3-W4 T5 Step 3): a `color_operation` failure must
// leave the flag FALSE. This is the assertion that proves the error
// RE-PARTITION was ported, rather than a blanket "any handler error sets
// the flag" — which is what a mechanical port of the `vt()` catch alone
// would produce, and which every other assertion in this file would still
// pass with.
//
// The failure is real (parity tail S7-4): the response of 64 palette
// queries outgrows the `stackFallback(1024, gpa())` buffer, and the
// active screen's allocator (`gpa()`) is swapped for
// `std.mem.Allocator.failing`, so the spill fails.
#[test]
fn semantic_failure_is_not_set_by_a_color_operation_failure() {
    // One OSC 4 with 64 queries: one color operation, one response.
    let mut queries = b"\x1B]4".to_vec();
    for i in 0..64 {
        queries.extend_from_slice(format!(";{i};?").as_bytes());
    }
    queries.extend_from_slice(b"\x1B\\");

    // (a) Baseline: the arm IS reached and DOES produce a response, so the
    //     failure injected in (b) is landing on a live code path. (Without
    //     this the whole control could be passing because the input never
    //     reaches `colorOperation` at all.)
    let mut t = Terminal::init(Global, opts(10, 2)).unwrap();
    cap_reset();
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.next_slice(&queries);
        let resp =
            cap_take().expect("colorOperation must be reached for this control to mean anything");
        assert!(
            resp.len() > 1024,
            "the response must outgrow the stack buffer"
        );
        assert!(!s.handler.semantic_failure);
    }
    t.deinit(Global);

    // (b) The same input with `do_color_operation` FORCED to fail. The
    //     response must not be produced (proving the failure was injected
    //     on the live path) and `semantic_failure` must stay FALSE — that
    //     is the re-partition assertion. Routing this arm through
    //     `vt_catch` (the mechanical "any error sets the flag" port) makes
    //     this fail.
    let mut t = Terminal::init(Global, opts(10, 2)).unwrap();
    cap_reset();
    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        let original_alloc = s.handler.term().active_mut().alloc;
        s.handler.term().active_mut().alloc = crate::alloc::failing();
        let cap = crate::log_capture::start();
        s.next_slice(&queries);
        let messages = cap.messages();
        drop(cap);
        s.handler.term().active_mut().alloc = original_alloc;
        assert!(
            messages
                .iter()
                .any(|(level, _, msg)| *level == log::Level::Warn
                    && msg.starts_with("error reporting OSC color err=")),
            "the dispatch arm logs the failure: {messages:?}"
        );
        assert!(
            cap_take().is_none(),
            "the injected failure must actually abort the response"
        );
        assert!(
            !s.handler.semantic_failure,
            "a color-operation failure is GRACEFUL and must not set semantic_failure"
        );
    }
    t.deinit(Global);
}
