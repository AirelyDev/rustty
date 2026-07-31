//! D2 — capacity-saturation tests: drive every fixed `[T; N]` + monotonic
//! write-then-increment counter in rustty-vt hot code to its ceiling.
//!
//! Detector D2 (`.port/phase-2/BUN_PORT_LEARNINGS.md` §4, kills Bun Class 2a /
//! oven-sh/bun#31503). The Bun escape: `ptrs[allocated]` write-then-increment
//! was a silent out-of-bounds write in ReleaseFast Zig for years, but the naive
//! Rust twin HARD PANICS on the identical index. Zig's own
//! `IntFittingRange(0, max + 1)` already encoded the true capacity. The rule
//! (per the inventory `.port/audits/capacity-saturation.tsv`): derive `N` from
//! the same expression the Zig counter range implies, confirm the port matches,
//! and drive every bounded store to (and one past) its ceiling so any
//! off-by-one is a RED test rather than a shipped OOB.
//!
//! Each fixed-array/counter pair in the inventory names its `*_capacity_saturation`
//! test HERE. The load-bearing assertion for every store is that driving it to
//! and beyond its ceiling **does not panic** (a Rust bounds panic is exactly the
//! #31503-class failure) — plus, wherever a public observable exists, that the
//! guard fired at precisely the derived `N`.
//!
//! This file is OWNED by detector D2 and is disjoint from every other detector's
//! files (naming convention: `d2_*`). It edits no shared source and reaches only
//! the crate's already-`pub` API.
//!
//! Triage rule (process, not code): a Rust panic here where the ghostty oracle
//! "worked" ⇒ re-run the oracle under ReleaseSafe/Debug FIRST (R3); ReleaseFast
//! Zig silently tolerates the OOB the port correctly refuses.

use allocator_api2::alloc::Global;

use rustty_vt::apc;
use rustty_vt::color::{DynamicPalette, DynamicRGB};
use rustty_vt::dcs::{self, Decrqss, DCS};
use rustty_vt::kitty::graphics_command::Parser as KittyCmdParser;
use rustty_vt::kitty::graphics_image::Limits;
use rustty_vt::modes::ModePacked;
use rustty_vt::osc;
use rustty_vt::pagelist::access::DiagramError;
use rustty_vt::pagelist::PageList;
use rustty_vt::parser::{Parser, MAX_INTERMEDIATE, MAX_PARAMS};
use rustty_vt::point::{Coordinate, Point};
use rustty_vt::sgr::{Attribute, Underline};
use rustty_vt::terminal::{Colors, Options, Terminal};

// ---------------------------------------------------------------------------
// #1 — CSI parameter buffer  (THE #31503 analog)
//
//   parser.rs:  params: [u16; MAX_PARAMS]   +   params_idx: u8
//   guard:      `if self.params_idx as usize >= MAX_PARAMS { return }`  (pre-write)
//   N derivation: MAX_PARAMS == 24 (parser.rs) === ghostty Parser.zig:203
//                 `pub const MAX_PARAMS = 24`. The counter `params_idx` is bounded
//                 by the pre-store guard, so the store index is in 0..MAX_PARAMS.
// ---------------------------------------------------------------------------
#[test]
fn parser_params_capacity_saturation() {
    // Feed a CSI with FAR more parameters than the buffer can hold: an ESC '['
    // then `MAX_PARAMS + 8` empty params (semicolons). Every ';' past the ceiling
    // must be dropped by the guard, never written OOB into `params`.
    let mut p = Parser::init();
    let _ = p.next(0x1B);
    let _ = p.next(b'[');

    let overshoot = MAX_PARAMS + 8;
    for _ in 0..overshoot {
        let _ = p.next(b'1');
        let _ = p.next(b';');
    }

    // The counter saturates at exactly the derived N and never exceeds it — this
    // is the direct proof the fixed store's index stays in-bounds.
    assert_eq!(
        p.params_idx as usize, MAX_PARAMS,
        "params_idx must saturate at MAX_PARAMS, not walk past the [u16; MAX_PARAMS] buffer"
    );

    // Closing the sequence must dispatch without panicking (the OOB would fire here
    // in the reference's ReleaseFast build; the port stays bounded).
    let _ = p.next(b'm');
    assert!(
        p.params_idx as usize <= MAX_PARAMS,
        "post-dispatch counter still bounded by N"
    );
}

// ---------------------------------------------------------------------------
// #2 — CSI intermediate buffer
//
//   parser.rs:  intermediates: [u8; MAX_INTERMEDIATE]  +  intermediates_idx: u8
//   guard:      `if self.intermediates_idx as usize >= MAX_INTERMEDIATE { return }`
//   N derivation: MAX_INTERMEDIATE == 4 === ghostty Parser.zig:190
//                 `pub const MAX_INTERMEDIATE = 4`.
// ---------------------------------------------------------------------------
#[test]
fn parser_intermediates_capacity_saturation() {
    // ESC '[' then MANY intermediate bytes (0x20 ' ' is a "collect" byte in the
    // csi_entry/csi_intermediate states). The guard must cap the counter at N.
    let mut p = Parser::init();
    let _ = p.next(0x1B);
    let _ = p.next(b'[');

    for _ in 0..(MAX_INTERMEDIATE + 6) {
        let _ = p.next(b' ');
    }

    assert_eq!(
        p.intermediates_idx as usize, MAX_INTERMEDIATE,
        "intermediates_idx must saturate at MAX_INTERMEDIATE, not overrun [u8; MAX_INTERMEDIATE]"
    );

    // Finish the sequence — must not panic.
    let _ = p.next(b'm');
}

// ---------------------------------------------------------------------------
// #3 — DECRQSS payload buffer
//
//   dcs.rs:  DecrqssState { data: [u8; 2], len: u8 }   (reference `len: u2`)
//   guard:   `if (buffer.len as usize) >= buffer.data.len() { return Err(..) }` (pre-write)
//   N derivation: `data: [2]u8` === ghostty DCS.zig anonymous state
//                 `struct { data: [2]u8 = undefined, len: u2 = 0 }`. The counter
//                 type `u2` == IntFittingRange(0, 2+1)? — it is the exact width
//                 that indexes a length-2 array (0..=2), the D2 shape.
// ---------------------------------------------------------------------------
#[test]
fn dcs_decrqss_capacity_saturation() {
    // (a) Ceiling-exact: DECRQSS `DCS $ q` then EXACTLY 2 payload bytes " q" fills
    //     data[0], data[1] to the brim and unhook must decode Decscusr.
    {
        let mut h: dcs::Handler<Global> = dcs::Handler::default();
        assert!(h
            .hook(
                Global,
                DCS {
                    intermediates: b"$",
                    final_: b'q',
                    ..Default::default()
                }
            )
            .is_none());
        assert!(h.put(b' ').is_none()); // data[0] = ' '  (len 0 -> 1)
        assert!(h.put(b'q').is_none()); // data[1] = 'q'  (len 1 -> 2, at ceiling)
        let cmd = h.unhook().expect("2-byte DECRQSS must produce a command");
        assert!(
            matches!(cmd, dcs::Command::Decrqss(Decrqss::Decscusr)),
            "\" q\" fills [u8; 2] exactly and decodes DECSCUSR"
        );
        h.deinit();
    }

    // (b) Overflow: a 3rd payload byte trips the pre-write guard (Err -> discard ->
    //     Ignore) rather than writing data[2] out of bounds. Must not panic.
    {
        let mut h: dcs::Handler<Global> = dcs::Handler::default();
        assert!(h
            .hook(
                Global,
                DCS {
                    intermediates: b"$",
                    final_: b'q',
                    ..Default::default()
                }
            )
            .is_none());
        let _ = h.put(b' ');
        let _ = h.put(b'q');
        let _ = h.put(b'x'); // 3rd byte: guard fires, state -> Ignore, no OOB
        assert!(
            h.unhook().is_none(),
            "overflowed DECRQSS is discarded (Ignore), not written past [u8; 2]"
        );
        h.deinit();
    }
}

// ---------------------------------------------------------------------------
// #4 — Kitty graphics control-key buffer
//
//   kitty/graphics_command.rs:  kv_temp: [u8; 11]  +  kv_temp_len: u8  (ref `u4`)
//   guard (increment-then-check-then-write):
//       idx = kv_temp_len; kv_temp_len += 1;
//       if kv_temp_len > kv_temp.len() { overflow; kv_temp_len = 0; return }
//       kv_temp[idx] = c;
//   N derivation: `kv_temp: [11]u8` === ghostty graphics_command.zig:30. The
//                 counter is `u4` in the reference (0..=15); the `> 11` post-inc
//                 guard forces reset at 12, so the write index never exceeds 10.
// ---------------------------------------------------------------------------
#[test]
fn kitty_kv_temp_capacity_saturation() {
    // Parser starts in ControlKey; every non-'='/';' byte accumulates into kv_temp.
    // Feed 11 bytes (fills indices 0..=10 exactly) then 4 more to trip the overflow
    // guard. No feed may panic (a Rust OOB write is the #31503-class defect).
    let mut p = KittyCmdParser::init(1024);
    for _ in 0..(11 + 4) {
        assert!(
            p.feed(b'a').is_ok(),
            "kv_temp accumulation must stay bounded, never OOB the [u8; 11] buffer"
        );
    }
    // After overflow the parser is in ControlKeyIgnore, so complete() reports the
    // invalid-format error rather than crashing — the observable that the guard,
    // not a panic, absorbed the overflow.
    assert!(
        p.complete().is_err(),
        "overflowed control key ends in Ignore -> InvalidFormat, proving the guard fired"
    );
}

// ---------------------------------------------------------------------------
// #5 — APC identify prefix buffer
//
//   apc/mod.rs:  Identify { len: u8, buf: [u8; 4] }   (reference `len: u3`)
//   guard:       `if id.len as usize >= id.buf.len() { state = Ignore; return }` (pre-write)
//   N derivation: `buf: [4]u8` === ghostty apc.zig:169; the counter is `len: u3`
//                 (apc.zig:168) == IntFittingRange(0, 4+1): the canonical D2 shape,
//                 a u3 that indexes a length-4 array (values 0..=4).
// ---------------------------------------------------------------------------
#[test]
fn apc_identify_buf_capacity_saturation() {
    // (a) Ceiling-exact: the recognized glyph prefix "25a1" is EXACTLY 4 bytes —
    //     it fills buf[0..4] to the brim, then ';' reads buf[0..4] == "25a1".
    //     Must not panic on the 4th (index-3) write.
    {
        let mut h = apc::Handler::default();
        h.start();
        for &b in b"25a1;" {
            h.feed(Global, b); // fills [u8; 4] exactly, then ';' consumes it
        }
        // end() must not panic (glyph completion may yield None on empty payload).
        let _ = h.end(Global);
        h.deinit();
    }

    // (b) Overflow: a 5th identify byte trips the pre-write guard (state -> Ignore)
    //     instead of writing buf[4] out of bounds. Must not panic.
    {
        let mut h = apc::Handler::default();
        h.start();
        for &b in b"abcde" {
            h.feed(Global, b); // 5th byte: guard fires, no OOB into [u8; 4]
        }
        assert!(
            h.end(Global).is_none(),
            "overflowed APC identify is dropped (Ignore), not written past [u8; 4]"
        );
        h.deinit();
    }
}

// ---------------------------------------------------------------------------
// #6 — DECRPSS SGR attribute scratch buffer (`printAttributes`)
//
//   terminal.rs:  attrs: [u8; 8]  +  i: usize   (write-then-increment per flag)
//   N derivation: `[8]u8` === ghostty Terminal.zig:2793 `var attrs: [8]u8`. There
//                 are EXACTLY 8 boolean style flags that can each push one byte
//                 (bold, faint, italic, underline, blink, inverse, invisible,
//                 strikethrough), so a pen with all 8 set drives i to 8 — the
//                 array's precise ceiling. A 9th flag without growing the array is
//                 the off-by-one this test pins.
// ---------------------------------------------------------------------------
#[test]
fn terminal_print_attributes_capacity_saturation() {
    let mut t = terminal(5, 5);

    // Set ALL EIGHT style flags so `i` marches to the array ceiling of 8.
    t.set_attribute(Attribute::Bold).unwrap();
    t.set_attribute(Attribute::Faint).unwrap();
    t.set_attribute(Attribute::Italic).unwrap();
    t.set_attribute(Attribute::Underline(Underline::Single))
        .unwrap();
    t.set_attribute(Attribute::Blink).unwrap();
    t.set_attribute(Attribute::Inverse).unwrap();
    t.set_attribute(Attribute::Invisible).unwrap();
    t.set_attribute(Attribute::Strikethrough).unwrap();

    // With every flag set, printAttributes writes attrs[0..8] — filling the buffer
    // to its exact ceiling. This must not panic and must emit all eight codes.
    let mut storage: [u8; 64] = [0; 64];
    let out = t
        .print_attributes(&mut storage)
        .expect("print_attributes to the [u8; 8] ceiling must not overflow");
    let s = std::str::from_utf8(out).unwrap();
    for code in ['1', '2', '3', '4', '5', '7', '8', '9'] {
        assert!(
            s.contains(&format!(";{code}")),
            "all 8 SGR attribute codes must appear ({code} missing) — the [u8; 8] buffer held them all"
        );
    }

    t.deinit(Global);
}

// ---------------------------------------------------------------------------
// #7 — OSC capture buffer
//
//   osc/mod.rs:  buffer: [u8; MAX_BUF]  (fixed backing)  +  writer len
//   guard:       `if w.len() >= MAX_BUF { state = Invalid } else { w.push(c) }`
//   N derivation: MAX_BUF == 2048 (osc/mod.rs:484) === ghostty osc.zig:292
//                 `pub const MAX_BUF = 2048`.
// ---------------------------------------------------------------------------
#[test]
fn osc_buffer_capacity_saturation() {
    // OSC "0;" (set title) opens a Fixed capture with NO allocator, so overflow
    // beyond MAX_BUF must set State::Invalid — never write past the fixed buffer.
    let mut p = osc::Parser::init(None);
    p.next(b'0');
    p.next(b';');

    // Fill to EXACTLY the ceiling: MAX_BUF bytes still fit (state stays valid).
    for _ in 0..osc::MAX_BUF {
        p.next(b'x');
    }
    assert_ne!(
        p.state,
        osc::State::Invalid,
        "exactly MAX_BUF captured bytes must still be a valid (unsaturated) capture"
    );

    // One more byte overflows: the guard flips the parser to Invalid, discarding
    // further input instead of overrunning [u8; MAX_BUF]. No panic.
    p.next(b'x');
    assert_eq!(
        p.state,
        osc::State::Invalid,
        "the (MAX_BUF+1)th byte must trip the guard to Invalid, not overrun the buffer"
    );
    p.deinit();
}

// ---------------------------------------------------------------------------
// #8 — PageList diagram tracked-pin scratch buffer
//
//   pagelist/access.rs:  pin_buf: [*mut Pin; 16]  +  pin_count: usize
//   guard (write-then-increment-then-check):
//       pin_buf[pin_count] = p; pin_count += 1;
//       if pin_count >= pin_buf.len() { return Err(TooManyTrackedPinsInRow) }
//   N derivation: `pin_buf: [16]*Pin` === ghostty PageList.zig:4260. The counter
//                 is checked `>= 16` after the store, so 16 pins in one row drive
//                 the buffer to its ceiling and the guard errors out.
// ---------------------------------------------------------------------------
#[test]
fn pagelist_pin_buf_capacity_saturation() {
    // (a) Sanity: a few tracked pins in a row render without tripping the guard.
    {
        let mut pl = PageList::init(rustty_vt::pagelist::Options::new(20, 5)).unwrap();
        for x in 0..3u16 {
            let pin = pl
                .pin(Point::Active(Coordinate { x, y: 0 }))
                .expect("pin in row 0");
            let _ = pl.track_pin(pin).unwrap();
        }
        let mut out = String::new();
        assert!(
            pl.diagram(&mut out).is_ok(),
            "a handful of tracked pins must render (well under the 16-pin ceiling)"
        );
        pl.deinit();
    }

    // (b) Ceiling: 16 tracked pins in a SINGLE row drive pin_buf to its [_; 16]
    //     ceiling; the guard must return TooManyTrackedPinsInRow, never index
    //     pin_buf[16] out of bounds.
    {
        let mut pl = PageList::init(rustty_vt::pagelist::Options::new(20, 5)).unwrap();
        for x in 0..16u16 {
            let pin = pl
                .pin(Point::Active(Coordinate { x, y: 0 }))
                .expect("pin in row 0");
            let _ = pl.track_pin(pin).unwrap();
        }
        let mut out = String::new();
        assert!(
            matches!(
                pl.diagram(&mut out),
                Err(DiagramError::TooManyTrackedPinsInRow)
            ),
            "16 pins in one row must trip the guard, not overrun [*mut Pin; 16]"
        );
        pl.deinit();
    }
}

// ---------------------------------------------------------------------------
// Shared helper — build a minimal `Terminal` (spelled out because the in-crate
// `opts()` test helper is not exported and `Colors::default()` is still `todo!()`).
// Mirrors the d5 integration-test helper.
// ---------------------------------------------------------------------------
fn terminal(cols: u16, rows: u16) -> Terminal {
    let opts = Options {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: Colors {
            background: DynamicRGB::UNSET,
            foreground: DynamicRGB::UNSET,
            cursor: DynamicRGB::UNSET,
            palette: DynamicPalette::default_palette(),
        },
        default_modes: ModePacked::default(),
        default_cursor_style: rustty_vt::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: Limits::DIRECT,
    };
    Terminal::init(Global, opts).expect("Terminal::init")
}
