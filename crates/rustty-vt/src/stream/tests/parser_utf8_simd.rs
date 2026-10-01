//! stream tests: the unsupported-once logging, print-slice batching, APC
//! bulk scan, CSI bulk params, fast paths, printing and UTF-8/SIMD decode,
//! and the ground-state C0/C1 handling.

use super::*;

// test Action {
//     // Forces the C type to be reified when the target is C, ensuring
//     // all our types are C ABI compatible.
//     _ = Action.C;
// }
// PORT NOTE: the reference `test Action` reifies the C-ABI union type so the
// compiler verifies C ABI compatibility. The Rust port models `Action` as
// the `Handler` trait; the C-ABI mirror lives in the `rustty-vt-c` crate, so
// there is no `Action.C` type to reify here. The test is intentionally
// retargeted to a no-op assertion that the dispatch surface compiles.
#[test]
fn action() {
    // No C-ABI union in this crate; see PORT NOTE above.
}

// rustty-only test (b5053153f, WS10 T7). The reference ships this commit with
// NO test: its observable is the log stream, and Zig's `log.warn` is not
// capturable in a `test` block. In the port the observable is better —
// `UnsupportedOnceTable` is real state — so the dedup semantics are asserted
// directly. Two arms:
//
//   (1) the MECHANISM, on a table local to this test, so it is deterministic
//       and cannot be perturbed by other tests running in parallel;
//   (2) the WIRING, i.e. that the dispatch sites really call the helper,
//       through the ONE site whose key is a compile-time constant.
#[test]
fn stream_logs_unsupported_once_per_value() {
    // ---- (1) mechanism -------------------------------------------------
    let table = UnsupportedOnceTable::new();

    // First sighting of a key logs; every repeat is suppressed.
    assert!(log_unsupported_once(&table, 34));
    assert!(!log_unsupported_once(&table, 34));
    assert!(!log_unsupported_once(&table, 34));
    assert_eq!(1, table.claimed());

    // A DISTINCT value logs again — "once per distinct value", not "once".
    assert!(log_unsupported_once(&table, 35));
    assert!(!log_unsupported_once(&table, 35));
    assert!(!log_unsupported_once(&table, 34));
    assert_eq!(2, table.claimed());

    // Key 0 is a real key, not the empty sentinel (the sentinel is
    // `u32::MAX`, which no `u16` key can reach — that is why the slots are
    // `u32`). If 0 were the sentinel this pair would both log.
    assert!(log_unsupported_once(&table, 0));
    assert!(!log_unsupported_once(&table, 0));

    // Fill the remaining slots, then assert the reference's table-full rule:
    // "if a table fills, new values are suppressed too".
    for k in 100u16..(100 + 13) {
        assert!(log_unsupported_once(&table, k), "slot {k} should be free");
    }
    assert_eq!(16, table.claimed());
    assert!(
        !log_unsupported_once(&table, 9999),
        "table full: new values must be suppressed too"
    );
    // ...but an ALREADY-CLAIMED key is still found by the scan even when the
    // table is full, so a full table does not resurrect duplicate messages.
    assert!(!log_unsupported_once(&table, 34));

    // ---- (2) wiring ----------------------------------------------------
    // OSC 1 is the only converted site whose key is a literal (`0`), so its
    // global table can never hold more than one claimed slot no matter what
    // any other test does — which makes this arm order- and
    // parallelism-independent, unlike an arm keyed on a mode number.
    struct H;
    impl Handler for H {}

    let mut s: Stream<H> = Stream::init(Options::with_handler(H));
    s.next_slice(b"\x1b]1;icon\x07");
    assert_eq!(
        1,
        SEEN_OSC1_CHANGE_ICON.claimed(),
        "the OSC 1 dispatch arm must claim its dedup slot"
    );

    // Driving the same unsupported input a second time must NOT claim
    // anything further — this is the "once" half of the commit.
    s.next_slice(b"\x1b]1;other-icon\x07");
    assert_eq!(1, SEEN_OSC1_CHANGE_ICON.claimed());
    assert!(
        !log_unsupported_once(&SEEN_OSC1_CHANGE_ICON, 0),
        "key 0 was already claimed by the dispatch arm"
    );
}

// S163-F2: the stream's own inline CSI fast-path dispatch logs
// `log.warn("CSI colon or mixed separators only allowed for 'm' command,
// got: {f}", .{action})` (stream.zig:1352-1356) for a colon-separated CSI
// whose final byte is not 'm' — the same rule (and same message text) as
// `Parser::do_action`'s `warn_csi_sep_mismatch`, ported separately here
// because the fast path builds and dispatches the CSI action without
// going through the state-machine `Parser`.
#[test]
fn csi_fast_path_logs_on_colon_separator_mismatch_for_non_m_command() {
    struct H;
    impl Handler for H {}

    let mut s: Stream<H> = Stream::init(Options::with_handler(H));
    let cap = crate::log_capture::start();
    s.next_slice(b"\x1b[1:2H"); // cursor position, colon sep, NOT 'm'
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "stream"
                && msg.starts_with(
                    "CSI colon or mixed separators only allowed for 'm' command, got: "
                )),
        "expected the CSI-sep-mismatch diagnostic, got: {messages:?}"
    );
    drop(cap);
}

// ------------------------------------------------------------------
// Task 14r-a (S163-F2 / S164-F1): the three shapes a restored `log.*`
// site in this file can independently get wrong. One test per shape;
// each was mutation-proven (emission reverted -> test fails; re-applied
// -> test passes) before being committed.
// ------------------------------------------------------------------

// SHAPE 1 — a plain single-argument site whose Zig comment already carried
// its full argument list: `log.warn("invalid cursor up command: {f}",
// .{input})` (stream.zig:1435), reached when CUU arrives with more than one
// parameter. Asserts level, target AND the exact message text.
#[test]
fn stream_logs_invalid_cursor_up_command() {
    struct H;
    impl Handler for H {}

    let mut s: Stream<H> = Stream::init(Options::with_handler(H));
    let cap = crate::log_capture::start();
    s.next_slice(b"\x1b[1;2;3A"); // CUU with 3 params -> the invalid arm
    let messages = cap.messages();
    drop(cap);
    assert_eq!(
        vec![(
            log::Level::Warn,
            "stream".to_string(),
            "invalid cursor up command: CSI { intermediates: [], params: [1, 2, 3], params_sep: SepList { bits: 0 }, final_: 65 }"
                .to_string(),
        )],
        messages
    );
}

// SHAPE 2 — an ELIDED comment. The port's fidelity comment for this site
// reads `// else => log.warn("ignoring unimplemented CSI A with
// intermediates: {s}", ...),` — the arguments are `...`, so they had to be
// recovered from the oracle (stream.zig:1441-1444: `.{input.intermediates}`,
// NOT `.{input}`). This test fails if the site were restored from the
// comment with `{input:?}`, because the message would then carry the whole
// CSI struct instead of just the intermediates.
#[test]
fn stream_logs_unimplemented_csi_a_with_intermediates() {
    struct H;
    impl Handler for H {}

    let mut s: Stream<H> = Stream::init(Options::with_handler(H));
    let cap = crate::log_capture::start();
    s.next_slice(b"\x1b[?1A"); // CUU with a '?' intermediate
    let messages = cap.messages();
    drop(cap);
    assert_eq!(
        vec![(
            log::Level::Warn,
            "stream".to_string(),
            "ignoring unimplemented CSI A with intermediates: ?".to_string(),
        )],
        messages
    );
}

// SHAPE 3 — a `log_unsupported_once` site. The helper returns the decision
// and the CALL SITE emits (see its doc comment for why the format string
// cannot move back into the helper in Rust), so this asserts the emission
// follows the dedup: first sighting of a key emits, a repeat of the SAME key
// does not, a DIFFERENT key does.
//
// The mode numbers are deliberately absurd (60001/60002) so no other test in
// this binary can have claimed their slots in the process-wide
// `SEEN_UNIMPLEMENTED_MODE` table first.
#[test]
fn stream_unsupported_once_emits_once_per_key() {
    struct H;
    impl Handler for H {}

    let mut s: Stream<H> = Stream::init(Options::with_handler(H));
    let cap = crate::log_capture::start();

    // First sighting of 60001: emits.
    s.next_slice(b"\x1b[?60001h");
    // Same key again: suppressed.
    s.next_slice(b"\x1b[?60001h");
    // A different key: emits.
    s.next_slice(b"\x1b[?60002h");
    let messages = cap.messages();
    drop(cap);

    assert_eq!(
        vec![
            (
                log::Level::Warn,
                "stream".to_string(),
                "unimplemented mode: 60001".to_string(),
            ),
            (
                log::Level::Warn,
                "stream".to_string(),
                "unimplemented mode: 60002".to_string(),
            ),
        ],
        messages
    );
}

// NEW-FINDINGS N3: the two UTF-8-decoded C1 sites are two Zig
// `logUnsupportedOnce` instantiations (`.{cp}` is `u32` at `stream.zig:734`,
// `.{c}` is `u21` at `:1185`), so each has its own `Static.seen` table and
// each emits once. Both key on the literal 0x80, so each static can hold at
// most one claimed slot whatever other tests do: asserting that BOTH are
// claimed after driving both paths is order- and parallelism-independent.
// Were the two sites to share one table, the second path would claim
// nothing and its static would stay empty.
#[test]
fn utf8_decoded_c1_sites_have_separate_once_tables() {
    struct H;
    impl Handler for H {}

    // The slice path (`nextSlice`'s SIMD decode loop, `stream.zig:734`).
    let mut s: Stream<H> = Stream::init(Options::with_handler(H));
    s.next_slice("a\u{85}b".as_bytes());
    assert_eq!(
        1,
        SEEN_UTF8_DECODED_C1_SLICE.claimed(),
        "the slice path's C1 arm must claim its own dedup slot"
    );

    // The per-byte path (`next` -> `handleCodepoint`, `stream.zig:1185`).
    let mut s: Stream<H> = Stream::init(Options::with_handler(H));
    for &b in "a\u{85}b".as_bytes() {
        s.next(b);
    }
    assert_eq!(
        1,
        SEEN_UTF8_DECODED_C1_CODEPOINT.claimed(),
        "the per-codepoint path's C1 arm must claim its own dedup slot"
    );
    assert!(!log_unsupported_once(&SEEN_UTF8_DECODED_C1_SLICE, 0x80));
    assert!(!log_unsupported_once(&SEEN_UTF8_DECODED_C1_CODEPOINT, 0x80));
}

// ---------------------------------------------------------------------
// WS10 T16 (47e26df60 + f2a7652ab): print_slice run batching in the
// ground-state SIMD loop.
// ---------------------------------------------------------------------

/// Drive the input through the GROUND-STATE loop in `next_slice_capped`,
/// which is where `47e26df60`'s printable-run batching and `f2a7652ab`'s
/// vectorized run scan live.
///
/// THIS IS NOT `next_slice`, AND THAT IS THE POINT — same vacuity trap
/// `drive_bulk` documents above: `next_slice` opens with
/// `if DEBUG || !crate::options::SIMD { per-byte; return; }`, and `simd` is
/// OFF by default, so a test written against `next_slice` would compare the
/// per-byte path with itself in a default `cargo test` build and never
/// reach a single `print_slice`. `next_slice_capped` has no such gate, so
/// calling it directly exercises the batching in BOTH feature
/// configurations.
fn drive_ground<H: Handler + Default>(input: &[u8]) -> H {
    let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
    let mut cp_buf: [u32; 4096] = [0; 4096];
    assert!(input.len() <= cp_buf.len());
    unsafe { Stream::next_slice_capped(&mut s, input, &mut cp_buf) };
    s.handler
}

/// Records the `print_slice` runs verbatim (rather than fanning them out
/// to `print` the way the trait default does), so a test can assert on the
/// RUN STRUCTURE and not just the resulting codepoints.
#[derive(Default)]
struct PrintSliceRecorder {
    runs: Vec<Vec<u32>>,
    singles: Vec<u32>,
    bells: usize,
}

impl Handler for PrintSliceRecorder {
    fn print(&mut self, cp: u32) {
        self.singles.push(cp);
    }
    fn print_slice(&mut self, cps: &[u32]) {
        self.runs.push(cps.to_vec());
    }
    fn bell(&mut self) {
        // 0x07 is the control the tests below use as a run separator; the
        // stream's `execute` routes it here.
        self.bells += 1;
    }
}

/// The printable-run batching must be observationally identical to the
/// per-byte `print` stream it replaces: `RecordingHandler` does NOT
/// override `print_slice`, so its trait default fans each run back out to
/// `print` and the two logs must be equal byte for byte.
///
/// This is the falsifiable form of `47e26df60`'s "semantically identical
/// to calling `print` for each codepoint in order".
#[test]
// MIRI: `next_slice_capped` calls `rustty_simd_decode_utf8_until_control_seq`,
// a foreign (C++/highway) function Miri cannot execute ("unsupported
// operation: can't call foreign function"). Structural, not a soundness
// finding — no Rust unsafe code of this port goes unverified, because the
// raw-pointer cell writes these tests reach live in `print_slice_fill` and
// ARE Miri-covered by `terminal_print_slice_*`, which drive
// `Terminal::print_slice` directly with no SIMD decode. Registered in
// .port/phase-2/MIRI_EXCEPTIONS.md (WS10 T16).
#[cfg_attr(miri, ignore)]
fn stream_print_slice_matches_per_byte() {
    const CORPUS: &[&[u8]] = &[
        b"hello world",
        b"",
        b"a",
        b"\x07",
        b"hello\nworld\r\n",
        b"one\x07two\x08three",
        // Multi-byte UTF-8 mixed with ASCII and controls.
        "caf\u{e9} \u{4e00}\u{4e01} \u{1f600}!".as_bytes(),
        // Every C0 in range, interleaved with printables.
        b"a\x00b\x01c\x02d\x03e\x04f\x05g\x06h\x07i\x08j\x09k\x0al\x0bm\x0cn\x0do\x0ep\x0fq",
        // A run longer than any plausible vector width, then a control.
        b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\x07b",
        // Control first.
        b"\x0aabc",
        // Escape sequences bracketing printable runs.
        b"abc\x1b[31mdef\x1b[0mghi",
    ];

    for input in CORPUS {
        let per_byte = drive_per_byte::<RecordingHandler>(input);
        let ground = drive_ground::<RecordingHandler>(input);
        assert_eq!(
            per_byte.log,
            ground.log,
            "print_slice batching diverged from per-byte path on {:?}",
            String::from_utf8_lossy(input)
        );
    }
}

/// The run scan must actually BATCH: a contiguous printable run arrives as
/// ONE `print_slice` carrying every codepoint, and a C0 splits it into two
/// runs with the control executed in between. Without this, the previous
/// test still passes with a `print_slice` that emits one codepoint at a
/// time (which is what an off-by-one in the vectorized `end` scan would
/// produce), so this is the negative control for `f2a7652ab`'s lane loop.
#[test]
// MIRI: `next_slice_capped` calls `rustty_simd_decode_utf8_until_control_seq`,
// a foreign (C++/highway) function Miri cannot execute ("unsupported
// operation: can't call foreign function"). Structural, not a soundness
// finding — no Rust unsafe code of this port goes unverified, because the
// raw-pointer cell writes these tests reach live in `print_slice_fill` and
// ARE Miri-covered by `terminal_print_slice_*`, which drive
// `Terminal::print_slice` directly with no SIMD decode. Registered in
// .port/phase-2/MIRI_EXCEPTIONS.md (WS10 T16).
#[cfg_attr(miri, ignore)]
fn stream_print_slice_batches_printable_runs() {
    // 70 'a's is longer than 2x any native vector width for u32, so the
    // vector loop runs to completion and hands off to the scalar tail.
    let mut input = vec![b'a'; 70];
    input.push(0x07); // BEL, a C0 -> execute
    input.extend_from_slice(b"bcd");

    let h = drive_ground::<PrintSliceRecorder>(&input);
    assert_eq!(2, h.runs.len(), "expected exactly two printable runs");
    assert_eq!(70, h.runs[0].len());
    assert!(h.runs[0].iter().all(|&c| c == 'a' as u32));
    assert_eq!(
        vec!['b' as u32, 'c' as u32, 'd' as u32],
        h.runs[1],
        "the run after the control must be batched too"
    );
    assert_eq!(1, h.bells);
    assert!(
        h.singles.is_empty(),
        "the ground loop must emit print_slice, never bare print"
    );

    // Every lane boundary: a control at offset N for N across a full
    // vector's worth of positions must split exactly there.
    for n in 0..40usize {
        let mut input: Vec<u8> = vec![b'x'; n];
        input.push(0x07);
        input.extend_from_slice(b"yz");
        let h = drive_ground::<PrintSliceRecorder>(&input);
        let expect_runs = if n == 0 { 1 } else { 2 };
        assert_eq!(expect_runs, h.runs.len(), "n={n}");
        if n > 0 {
            assert_eq!(n, h.runs[0].len(), "n={n}");
        }
        assert_eq!(
            vec!['y' as u32, 'z' as u32],
            *h.runs.last().unwrap(),
            "n={n}"
        );
    }
}

// ---------------------------------------------------------------------
// WS10 T17 (f6f79acce + 8c523ed03): bulk APC slice dispatch.
// ---------------------------------------------------------------------

/// A test handler that accumulates APC bytes regardless of whether they
/// arrive per-byte (apc_put) or in bulk (apc_put_slice).
// const ApcTestHandler = struct { ... };
struct ApcTestHandler {
    buf: [u8; 256],
    len: usize,
    slices: usize,
    puts: usize,
    started: usize,
    ended: usize,
}

impl Default for ApcTestHandler {
    fn default() -> ApcTestHandler {
        ApcTestHandler {
            buf: [0; 256],
            len: 0,
            slices: 0,
            puts: 0,
            started: 0,
            ended: 0,
        }
    }
}

impl Handler for ApcTestHandler {
    fn apc_start(&mut self) {
        self.started += 1;
    }
    fn apc_end(&mut self, _v: ApcEnd) {
        self.ended += 1;
    }
    fn apc_put(&mut self, code: u8) {
        self.buf[self.len] = code;
        self.len += 1;
        self.puts += 1;
    }
    fn apc_put_slice(&mut self, bytes: &[u8]) {
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        self.slices += 1;
    }
}

/// Drive input through the bulk path (`consume_until_ground`, where
/// `consume_apc_string` lives), NOT through `next_slice` — see the vacuity
/// note on `drive_bulk`. `next_slice` is per-byte without `--features
/// simd`, so a test written against it would never reach a single
/// `apc_put_slice` in the default build.
fn drive_apc_bulk(input: &[u8]) -> ApcTestHandler {
    drive_bulk::<ApcTestHandler>(input)
}

// test "stream: apc bulk slice"
#[test]
fn stream_apc_bulk_slice() {
    let h = drive_apc_bulk(b"\x1b_Gf=24,s=10,v=20;aGVsbG8=\x1b\\");

    assert_eq!(1, h.started);
    assert_eq!(1, h.ended);
    assert_eq!(b"Gf=24,s=10,v=20;aGVsbG8=".as_slice(), &h.buf[0..h.len]);

    // With SIMD enabled the body must arrive as a single slice.
    //
    // PORT NOTE: the reference guards this with
    // `if (comptime build_options.simd and !debug)`, because its `nextSlice`
    // driver is the one that reaches the bulk path. This port drives
    // `consume_until_ground` directly (see `drive_apc_bulk`), which is
    // always the bulk path, so the assertion is UNCONDITIONAL here — and
    // that is strictly stronger: it holds in both feature configurations.
    assert_eq!(1, h.slices);
    assert_eq!(0, h.puts);
}

// test "stream: apc bulk slice C1 ST"
#[test]
fn stream_apc_bulk_slice_c1_st() {
    // var s: Stream(ApcTestHandler) = .init(.{ .handler = .{} });
    // s.nextSlice("\x1b_Gpayload\x9c");
    //
    // PORT NOTE: driven through `drive_apc_bulk` (the module's bulk-path
    // driver, see its vacuity note) so the C1 ST fast path in
    // `consume_until_ground` is exercised in both feature configurations.
    let h = drive_apc_bulk(b"\x1b_Gpayload\x9c");

    assert_eq!(1, h.started);
    assert_eq!(1, h.ended);
    assert_eq!(b"Gpayload".as_slice(), &h.buf[0..h.len]);
}

// test "stream: apc bulk slice split across inputs"
#[test]
fn stream_apc_bulk_slice_split_across_inputs() {
    let mut s: Stream<ApcTestHandler> =
        Stream::init(Options::with_handler(ApcTestHandler::default()));
    for chunk in [
        b"\x1b_Gf=24,s=10".as_slice(),
        b",v=20;aGVs".as_slice(),
        b"bG8=\x1b\\".as_slice(),
    ] {
        drive_bulk_into(&mut s, chunk);
    }
    let h = s.handler;

    assert_eq!(1, h.started);
    assert_eq!(1, h.ended);
    assert_eq!(b"Gf=24,s=10,v=20;aGVsbG8=".as_slice(), &h.buf[0..h.len]);
}

// test "stream: apc bulk slice keeps C0 bytes as data"
#[test]
fn stream_apc_bulk_slice_keeps_c0_bytes_as_data() {
    // BEL does not terminate an APC string; it is payload data.
    let h = drive_apc_bulk(b"\x1b_Gx\x07y\x1b\\");

    assert_eq!(1, h.ended);
    assert_eq!(b"Gx\x07y".as_slice(), &h.buf[0..h.len]);
}

// test "stream: apc aborted by CAN"  (text widened by 8c523ed03)
#[test]
fn stream_apc_aborted_by_can() {
    // CAN (0x18) aborts the APC string via the anywhere => ground
    // transition. Exiting the sos_pm_apc_string state emits apc_end,
    // and the trailing bytes are printed, not treated as APC data.
    let h = drive_apc_bulk(b"\x1b_Gabcdefghijklmnopqrstuvwxyz0123456789\x18def");

    assert_eq!(1, h.started);
    assert_eq!(1, h.ended);
    assert_eq!(
        b"Gabcdefghijklmnopqrstuvwxyz0123456789".as_slice(),
        &h.buf[0..h.len]
    );
}

// test "stream: apc scalar path matches"
#[test]
fn stream_apc_scalar_path_matches() {
    let h = drive_per_byte::<ApcTestHandler>(b"\x1b_Gf=24;aGVsbG8=\x1b\\");

    assert_eq!(1, h.started);
    assert_eq!(1, h.ended);
    assert_eq!(b"Gf=24;aGVsbG8=".as_slice(), &h.buf[0..h.len]);
}

// test "stream: apc vector boundaries match scalar path"
//
// This is the negative control for `8c523ed03`'s vectorization: it must
// FAIL if the lane boundary handling is off by one.
#[test]
fn stream_apc_vector_boundaries_match_scalar_path() {
    let positions = [15usize, 16, 17, 31, 32, 33, 63, 64, 65];
    let controls = [0x18u8, 0x1A, 0x1B, 0x80, 0xFF];

    for position in positions {
        for control in controls {
            let mut input: Vec<u8> = Vec::with_capacity(96);
            input.extend_from_slice(b"\x1b_G");
            input.extend(core::iter::repeat_n(b'a', position));
            input.push(control);
            input.push(b'\\');
            let bytes = &input[..];

            let bulk = drive_apc_bulk(bytes);
            let scalar = drive_per_byte::<ApcTestHandler>(bytes);

            assert_eq!(
                scalar.started, bulk.started,
                "pos={position} ctl={control:#x}"
            );
            assert_eq!(scalar.ended, bulk.ended, "pos={position} ctl={control:#x}");
            assert_eq!(
                &scalar.buf[0..scalar.len],
                &bulk.buf[0..bulk.len],
                "pos={position} ctl={control:#x}"
            );
        }
    }
}

/// RUSTTY-SIDE (WS10 T17): the oracle's boundary test above NEVER reaches
/// `consume_apc_string`'s vector `invalid` branch.
///
/// Its inputs end two bytes after the control, so with a 16-lane scan the
/// last FULL window always stops just short of the control and the scalar
/// tail finds it — the `if invalid { break }` arm is never taken. Proven by
/// mutation: making that arm `end += 1; break` left the oracle test GREEN.
///
/// This variant pads the input after the control so the control lands
/// mid-window and the vector loop must detect it. Note 0x80..0xFF are
/// "ignored" by the parse table (still APC state), so the trailing bytes
/// after those controls are real payload that both paths must agree on —
/// which is exactly the differential this asserts.
#[test]
fn stream_apc_vector_invalid_branch_matches_scalar_path() {
    let controls = [0x18u8, 0x1A, 0x1B, 0x80, 0xFF];

    // Enough trailing bytes that the control is inside a full vector
    // window for any plausible lane count (16 for NEON/SSE2 u8, 32 for
    // AVX2).
    for position in 0..40usize {
        for control in controls {
            let mut input: Vec<u8> = Vec::with_capacity(160);
            input.extend_from_slice(b"\x1b_G");
            input.extend(core::iter::repeat_n(b'a', position));
            input.push(control);
            input.extend(core::iter::repeat_n(b'z', 80));
            input.extend_from_slice(b"\x1b\\");
            let bytes = &input[..];

            let bulk = drive_apc_bulk(bytes);
            let scalar = drive_per_byte::<ApcTestHandler>(bytes);

            assert_eq!(
                scalar.started, bulk.started,
                "pos={position} ctl={control:#x}"
            );
            assert_eq!(scalar.ended, bulk.ended, "pos={position} ctl={control:#x}");
            assert_eq!(
                &scalar.buf[0..scalar.len],
                &bulk.buf[0..bulk.len],
                "pos={position} ctl={control:#x}"
            );
        }
    }
}

#[test]
fn stream_csi_bulk_params_match_per_byte() {
    for input in CSI_DIFFERENTIAL_CORPUS {
        let per_byte = drive_per_byte::<RecordingHandler>(input);

        let bulk = drive_bulk::<RecordingHandler>(input);
        assert_eq!(
            per_byte.log,
            bulk.log,
            "bulk path diverged from per-byte path on {:?}",
            String::from_utf8_lossy(input)
        );

        // The public entry point too, so whichever of the two shapes the
        // active feature set selects is also covered.
        let sliced = drive_slice::<RecordingHandler>(input);
        assert_eq!(
            per_byte.log,
            sliced.log,
            "next_slice diverged from per-byte path on {:?}",
            String::from_utf8_lossy(input)
        );

        // Split every input at every boundary and feed it as two chunks.
        // `consume_csi_params` carries accumulator state across calls
        // (`param_acc`/`param_acc_idx`/`params_idx` are written back on
        // exit), so a mid-parameter split is exactly where a save/restore
        // bug would hide.
        for split in 1..input.len() {
            let mut s: Stream<RecordingHandler> =
                Stream::init(Options::with_handler(RecordingHandler::default()));
            let mut feed = |chunk: &[u8]| {
                let mut i = 0usize;
                while i < chunk.len() {
                    if s.parser.state != parser::State::Ground {
                        i += unsafe { Stream::consume_until_ground(&mut s, &chunk[i..]) };
                        continue;
                    }
                    let c = chunk[i];
                    if c == 0x1B {
                        s.parser.state = parser::State::Escape;
                        s.parser.clear();
                        i += 1;
                        continue;
                    }
                    unsafe { Stream::next_utf8(&mut s, c) };
                    i += 1;
                }
            };
            feed(&input[..split]);
            feed(&input[split..]);
            assert_eq!(
                per_byte.log,
                s.handler.log,
                "split at {split} diverged on {:?}",
                String::from_utf8_lossy(input)
            );
        }
    }
}

// rustty-only test (WS10 T14 Step 3): the `vtRaw` interlock.
//
// `D-terminal-subsystems.md:506` flags this as load-bearing for the
// inspector: the fast paths dispatch without going through `Parser.next`,
// so a handler with a raw hook must stay on the general path or its VT
// event log silently loses every CSI. Two properties, both required:
//
//   (a) a raw-hooked handler still observes a `CsiDispatch` action for
//       every CSI in the corpus — the property that breaks if `HAS_VT_RAW`
//       is not set on a handler that overrides `vt_raw`;
//   (b) the fast paths did not change WHAT is dispatched — the fast-path
//       handler's callback log equals the general-path handler's, which
//       makes the general path the oracle for the whole fast-path family
//       (T13's test compares two fast-path spellings; this one compares
//       fast path against no fast path at all).
#[test]
fn stream_fast_paths_bypassed_when_vt_raw_hooked() {
    let mut saw_csi = 0usize;
    for input in CSI_DIFFERENTIAL_CORPUS {
        let hooked = drive_bulk::<RawHookedRecorder>(input);
        let fast = drive_bulk::<RecordingHandler>(input);

        // (a)
        let csis = hooked
            .raw
            .iter()
            .filter(|a| a.starts_with("CsiDispatch"))
            .count();
        saw_csi += csis;

        // (b)
        assert_eq!(
            hooked.log,
            fast.log,
            "fast path diverged from the general state machine on {:?}",
            String::from_utf8_lossy(input)
        );
    }

    // Guard against the corpus silently ceasing to contain CSIs, which
    // would make arm (a) vacuous. A FLOOR, not an equality: the count is
    // not one-per-entry, because some entries deliberately produce no
    // dispatch (the MAX_PARAMS-overflow entry) and some produce several
    // (`\x1b[A\x1b[B\x1b[C\x1b[D`). Measured 33 at landing over 34 entries;
    // the floor is set below that so ADDING corpus entries never reddens
    // this, only losing CSIs does.
    assert!(
        saw_csi >= 30,
        "the raw hook must still observe the corpus's CSI dispatches; \
             saw {saw_csi} across {} entries (floor 30)",
        CSI_DIFFERENTIAL_CORPUS.len()
    );
}

// test "stream: print" { ... }
#[test]
fn stream_print() {
    struct H {
        c: Option<u32>,
    }
    impl Handler for H {
        fn print(&mut self, cp: u32) {
            self.c = Some(cp);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { c: Some(0) }));
    s.next(b'x');
    assert_eq!(120u32, s.handler.c.unwrap());
}

// rustty-only test (VALIDATION_REPORT §3d): the REP dispatch delivers the
// reference's `print_repeat: usize` payload (stream.zig:36) — the u16 CSI
// param widens into it; the handler signature must be usize, not u16.
#[test]
fn stream_print_repeat_payload_is_usize() {
    struct H {
        count: Option<usize>,
    }
    impl Handler for H {
        fn print_repeat(&mut self, count: usize) {
            self.count = Some(count);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { count: None }));
    // CSI 500 b => REP with the max-clamped u16 param widened to usize 500.
    for b in b"\x1b[500b" {
        s.next(*b);
    }
    assert_eq!(Some(500usize), s.handler.count);
}

// test "simd: print invalid utf-8" { ... }
#[test]
fn simd_print_invalid_utf8() {
    struct H {
        c: Option<u32>,
    }
    impl Handler for H {
        fn print(&mut self, cp: u32) {
            self.c = Some(cp);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { c: Some(0) }));
    s.next_slice(&[0xFF]);
    assert_eq!(0xFFFDu32, s.handler.c.unwrap());
}

// test "simd: complete incomplete utf-8" { ... }
#[test]
fn simd_complete_incomplete_utf8() {
    struct H {
        c: Option<u32>,
    }
    impl Handler for H {
        fn print(&mut self, cp: u32) {
            self.c = Some(cp);
        }
    }

    let mut s: Stream<H> = Stream::init(Options::with_handler(H { c: None }));
    s.next_slice(&[0xE0]); // 3 byte
    assert!(s.handler.c.is_none());
    s.next_slice(&[0xA0]); // still incomplete
    assert!(s.handler.c.is_none());
    s.next_slice(&[0x80]);
    assert_eq!(0x800u32, s.handler.c.unwrap());
}

// test "stream: ground state C0 controls are executed, not printed" { ... }
#[test]
fn stream_ground_state_c0_controls_are_executed_not_printed() {
    // buf: [128]u21 = undefined,
    // len: usize = 0,
    //
    // PORT NOTE: the reference handler is a bare struct with default field
    // values; `Default` is the Rust spelling of that `.{}` construction.
    struct H {
        buf: [u32; 128],
        len: usize,
    }
    impl Default for H {
        fn default() -> Self {
            Self {
                buf: [0; 128],
                len: 0,
            }
        }
    }
    impl Handler for H {
        // .print => { self.buf[self.len] = value.cp; self.len += 1; },
        fn print(&mut self, cp: u32) {
            self.buf[self.len] = cp;
            self.len += 1;
        }
        // .print_slice => for (value.cps) |cp| {
        //     self.buf[self.len] = @intCast(cp);
        //     self.len += 1;
        // },
        fn print_slice(&mut self, cps: &[u32]) {
            for &cp in cps {
                self.buf[self.len] = cp;
                self.len += 1;
            }
        }
    }

    // Every C0 control except ESC must never produce a print action
    // in the ground state. ESC is excluded because it begins an
    // escape sequence rather than executing.
    for c in 0u32..0x20 {
        if c == 0x1B {
            continue;
        }

        // Scalar path.
        {
            let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
            s.next(b'A');
            s.next(c as u8);
            s.next(b'B');
            assert_eq!(2usize, s.handler.len);
            assert_eq!(b'A' as u32, s.handler.buf[0]);
            assert_eq!(b'B' as u32, s.handler.buf[1]);
        }

        // Batched path.
        {
            let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
            s.next_slice(&[b'A', b'B', c as u8, b'C', b'D']);
            assert_eq!(4usize, s.handler.len);
            for (i, expected) in b"ABCD".iter().enumerate() {
                assert_eq!(*expected as u32, s.handler.buf[i]);
            }
        }

        // Batched path with a run long enough to exercise the
        // vectorized printable-run scan on either side of the control.
        {
            let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
            // var input: [65]u8 = @splat('A');
            let mut input: [u8; 65] = [b'A'; 65];
            input[32] = c as u8;
            s.next_slice(&input);
            assert_eq!(64usize, s.handler.len);
            for &cp in &s.handler.buf[0..s.handler.len] {
                assert_eq!(b'A' as u32, cp);
            }
        }
    }
}

// test "stream: ground state UTF-8-decoded C1 controls are ignored" { ... }
#[test]
fn stream_ground_state_utf8_decoded_c1_controls_are_ignored() {
    // buf: [128]u21 = undefined,
    // len: usize = 0,
    struct H {
        buf: [u32; 128],
        len: usize,
    }
    impl Default for H {
        fn default() -> Self {
            Self {
                buf: [0; 128],
                len: 0,
            }
        }
    }
    impl Handler for H {
        // .print => { self.buf[self.len] = value.cp; self.len += 1; },
        fn print(&mut self, cp: u32) {
            self.buf[self.len] = cp;
            self.len += 1;
        }
        // .print_slice => for (value.cps) |cp| {
        //     self.buf[self.len] = @intCast(cp);
        //     self.len += 1;
        // },
        fn print_slice(&mut self, cps: &[u32]) {
            for &cp in cps {
                self.buf[self.len] = cp;
                self.len += 1;
            }
        }
    }

    // Every C1 control that arrives as well-formed UTF-8 (a two byte
    // 0xC2-lead sequence) must be dropped: not printed and not
    // interpreted as a control (e.g. U+009B must not start a CSI).
    for c in 0x80u32..0xA0 {
        // const enc: [2]u8 = .{ 0xC2, @intCast(c) };
        let enc: [u8; 2] = [0xC2, c as u8];

        // Scalar path.
        {
            let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
            s.next(b'A');
            s.next(enc[0]);
            s.next(enc[1]);
            s.next(b'B');
            assert_eq!(2usize, s.handler.len);
            assert_eq!(b'A' as u32, s.handler.buf[0]);
            assert_eq!(b'B' as u32, s.handler.buf[1]);
        }

        // Batched path.
        {
            let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
            s.next_slice(&[b'A', b'B', enc[0], enc[1], b'C', b'D']);
            assert_eq!(4usize, s.handler.len);
            for (i, expected) in b"ABCD".iter().enumerate() {
                assert_eq!(*expected as u32, s.handler.buf[i]);
            }
        }

        // Batched path with a run long enough to exercise the
        // vectorized printable-run scan on either side of the control.
        {
            let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
            // var input: [66]u8 = @splat('A');
            let mut input: [u8; 66] = [b'A'; 66];
            input[32] = enc[0];
            input[33] = enc[1];
            s.next_slice(&input);
            assert_eq!(64usize, s.handler.len);
            for &cp in &s.handler.buf[0..s.handler.len] {
                assert_eq!(b'A' as u32, cp);
            }
        }
    }

    // U+00A0 (NBSP), just past the C1 range, must still print.
    {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
        s.next_slice(&[0xC2, 0xA0]);
        assert_eq!(1usize, s.handler.len);
        assert_eq!(0xA0u32, s.handler.buf[0]);
    }

    // A codepoint whose continuation byte falls in the C1 range must
    // still print: "Ü" is 0xC3 0x9C.
    {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
        s.next_slice("Ü".as_bytes());
        assert_eq!(1usize, s.handler.len);
        assert_eq!(0xDCu32, s.handler.buf[0]);
    }

    // A raw C1 byte is ill-formed UTF-8, not a decoded C1: it must
    // still produce a U+FFFD replacement.
    {
        let mut s: Stream<H> = Stream::init(Options::with_handler(H::default()));
        s.next_slice(&[0x9B]);
        assert_eq!(1usize, s.handler.len);
        assert_eq!(0xFFFDu32, s.handler.buf[0]);
    }
}
