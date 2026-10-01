//! PORTS: ghostty src/terminal/c/terminal.zig — tests: the kitty
//! temp-file medium and storage limit options, and the allocator-routing
//! and allocation-failure sweeps through the C API.

use super::*;

// -----------------------------------------------------------------------
// P3-W11 T1 (P3-W6 T4 Steps 3/5) — the kitty temp-file medium is a
// DIRECTORY across the C ABI (oracle e8525c0fd, b988efcfe, 960c2cca5).
// -----------------------------------------------------------------------

/// Read `RUSTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE` back as bytes.
fn temp_dir_data(t: Terminal) -> Vec<u8> {
    let mut out = LibString {
        ptr: core::ptr::null(),
        len: 0,
    };
    assert_eq!(Result::Success, unsafe {
        get(
            t,
            TerminalData::KittyImageMediumTempFile,
            core::ptr::addr_of_mut!(out) as *mut c_void,
        )
    });
    if out.len == 0 {
        return Vec::new();
    }
    unsafe { core::slice::from_raw_parts(out.ptr, out.len) }.to_vec()
}

/// Parity S6-F2 (Decision A): `terminal_new` marks the process as the lib
/// artifact, so mode 5522 is recognized on every OS
/// (`.disabled = artifact != .lib and os != .macos`), and its DECRQM
/// report is set/reset, not "not recognized". On macOS the OS half alone
/// enables the mode, so the artifact assertion is what tells the two
/// artifacts apart there; on Linux all three assertions do.
#[test]
fn new_terminal_marks_the_process_as_the_lib_artifact_for_mode_5522() {
    with_test_allocator(|alloc| {
        // Through the C export, whose first step is
        // `sys::install_log_backend` (`new_t` calls the inner `new`).
        let mut t: Terminal = std::ptr::null_mut();
        let r = unsafe { crate::rustty_terminal_new(alloc as *const CAllocator, &mut t, 80, 24) };
        assert_eq!(Result::Success.to_c_int(), r);
        assert_eq!(
            rustty_vt::options::runtime_artifact(),
            rustty_vt::build_options::Artifact::Lib
        );
        assert_eq!(
            rustty_vt::modes::mode_from_int(5522, false),
            Some(rustty_vt::modes::Mode::KittyPasteEvents)
        );
        let report = unsafe { &(*zig_terminal(t)).modes }.get_report(rustty_vt::modes::ReportTag {
            value: 5522,
            ansi: false,
        });
        assert_eq!(report.state, rustty_vt::modes::State::Reset);
        unsafe { free(t) };
    });
}

// Parity S141-F1 / S179-F1: no reference test. `new_` relies on
// `Terminal.Options`'s field default, and this crate is the `.lib`
// artifact, whose default is 10MB (`Terminal.zig`'s
// `.lib => 10 * 1000 * 1000`), not the application's 320MB.
#[test]
#[cfg_attr(
    not(feature = "kitty-graphics"),
    ignore = "needs the kitty-graphics vt feature: the storage limit query is gated on it (R14)"
)]
fn kitty_image_storage_limit_defaults_to_lib_artifact_value() {
    with_test_allocator(|alloc| {
        let (r, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, r);

        let mut out: u64 = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::KittyImageStorageLimit,
                &mut out as *mut u64 as *mut c_void,
            )
        });
        assert_eq!(10 * 1000 * 1000, out);

        unsafe { free(t) };
    });
}

// W33-B07-32BIT-01: the reference narrows the `uint64_t` storage limit with
// `@intCast` — a CHECKED conversion that traps in safety-checked builds when
// the value exceeds `usize`. The port must use `usize::try_from(...)`, not a
// bare `as`, which would silently truncate. On 64-bit targets the conversion
// is infallible, so a limit above 2^32 must round-trip byte-exactly (a
// 32-bit-wide truncation would be visible here); see the companion
// `#[cfg(target_pointer_width = "32")]` test below for the trapping arm.
#[test]
#[cfg(target_pointer_width = "64")]
#[cfg_attr(
    not(feature = "kitty-graphics"),
    ignore = "SkipZigTest: needs the kitty-graphics vt feature (R14)"
)]
fn kitty_image_storage_limit_round_trips_above_32_bits() {
    with_test_allocator(|alloc| {
        let (r, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, r);

        for limit in [0u64, 1 << 20, (1u64 << 32) + 7, u32::MAX as u64 + 1] {
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::KittyImageStorageLimit,
                    core::ptr::addr_of!(limit) as *const c_void,
                )
            });

            let mut out: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::KittyImageStorageLimit,
                    &mut out as *mut u64 as *mut c_void,
                )
            });
            assert_eq!(limit, out);
        }

        unsafe { free(t) };
    });
}

// Task R13-P (`UPSTREAM-DEFECTS.md` UD63) REVERSED this test, which pinned
// the trapping arm of `@intCast`. The value is a C-ABI caller's `uint64_t`
// inside an `extern "C"` setter, not an invariant, so under R13 the port
// saturates instead of aborting the host. Only reachable where `usize` is
// narrower than `u64` (e.g. wasm32, a Kitty-graphics-ENABLED target), which
// cannot be RUN here — `saturating_from_u64_takes_the_maximum` below drives
// the same helper at u32 width on this host.
#[test]
#[cfg(target_pointer_width = "32")]
fn kitty_image_storage_limit_over_usize_saturates() {
    with_test_allocator(|alloc| {
        let (r, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, r);

        let limit: u64 = u64::from(u32::MAX) + 1;
        let got = unsafe {
            set(
                t,
                Option_::KittyImageStorageLimit,
                core::ptr::addr_of!(limit) as *const c_void,
            )
        };
        assert_eq!(Result::Success, got);

        unsafe { free(t) };
    });
}

/// The saturating conversion itself, at a width this host can exercise: on
/// a 32-bit target `usize` is exactly this narrow, and the limit a C caller
/// passes above it becomes "no practical limit" instead of a panic.
#[test]
fn saturating_from_u64_takes_the_maximum() {
    assert_eq!(0u32, saturating_from_u64(0, u32::MAX));
    assert_eq!(320_000_000u32, saturating_from_u64(320_000_000, u32::MAX));
    assert_eq!(u32::MAX, saturating_from_u64(u64::from(u32::MAX), u32::MAX));
    // The value the reversed test above passes: 2^32 has no u32.
    assert_eq!(
        u32::MAX,
        saturating_from_u64(u64::from(u32::MAX) + 1, u32::MAX)
    );
    assert_eq!(u32::MAX, saturating_from_u64(u64::MAX, u32::MAX));
    // On this host the production width is infallible, and unchanged.
    assert_eq!(usize::MAX, saturating_from_u64(u64::MAX, usize::MAX));
}

#[test]
fn kitty_image_medium_temp_file_is_a_directory_round_trip() {
    with_test_allocator(|alloc| {
        let (r, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, r);

        // Default construction leaves the medium DISABLED, which reads
        // back as the empty string (upstream's `.disabled => ""`).
        assert_eq!(Vec::<u8>::new(), temp_dir_data(t));

        let dir = b"/some/where/tmp";
        let v = LibString::init(dir);
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::KittyImageMediumTempFile,
                core::ptr::addr_of!(v) as *const c_void,
            )
        });
        assert_eq!(dir.to_vec(), temp_dir_data(t));

        // The string is COPIED into the terminal: mutating the caller's
        // buffer afterwards must not be observable.
        let mut owned = dir.to_vec();
        let v2 = LibString::init(&owned);
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::KittyImageMediumTempFile,
                core::ptr::addr_of!(v2) as *const c_void,
            )
        });
        owned[1] = b'X';
        assert_eq!(dir.to_vec(), temp_dir_data(t));

        // NULL disables the medium (the `else` arm), and the read-back
        // string becomes empty again.
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::KittyImageMediumTempFile,
                core::ptr::null::<c_void>(),
            )
        });
        assert_eq!(Vec::<u8>::new(), temp_dir_data(t));

        unsafe { free(t) };
    });
}

/// NEGATIVE CONTROL for `960c2cca5` (the copy-length bound).
///
/// Upstream copies into a fixed `[std.fs.max_path_bytes]u8`; the guard
/// `if (v.len > wrapper.tmp_dir_path.len) return .out_of_memory;` is what
/// stops an over-long directory, and `960c2cca5` fixed the memcpy it
/// guards. rustty owns the bytes, so what must be pinned is the guard's
/// observable contract: `MAX_PATH_BYTES` is accepted, `MAX_PATH_BYTES + 1`
/// is rejected with `OUT_OF_MEMORY` **and leaves the previous value
/// untouched** (i.e. no partial copy happened).
#[test]
fn kitty_image_medium_temp_file_rejects_an_over_long_directory() {
    with_test_allocator(|alloc| {
        let (r, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, r);

        let ok = vec![b'a'; MAX_PATH_BYTES - 1];
        let v = LibString::init(&ok);
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::KittyImageMediumTempFile,
                core::ptr::addr_of!(v) as *const c_void,
            )
        });
        assert_eq!(ok, temp_dir_data(t));

        // Exactly at the bound: still accepted.
        let exact = vec![b'b'; MAX_PATH_BYTES];
        let v = LibString::init(&exact);
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::KittyImageMediumTempFile,
                core::ptr::addr_of!(v) as *const c_void,
            )
        });
        assert_eq!(exact, temp_dir_data(t));

        // One over: rejected, and NOTHING was copied.
        let too_long = vec![b'c'; MAX_PATH_BYTES + 1];
        let v = LibString::init(&too_long);
        assert_eq!(Result::OutOfMemory, unsafe {
            set(
                t,
                Option_::KittyImageMediumTempFile,
                core::ptr::addr_of!(v) as *const c_void,
            )
        });
        assert_eq!(
            exact,
            temp_dir_data(t),
            "a rejected over-long directory must not partially overwrite \
             the previous value"
        );

        unsafe { free(t) };
    });
}

/// DECISIONS B6 (P7-W2 T3, oracle `105d0a545`): the wrapper allocates
/// exactly ONE copy of the kitty temp-dir path and every screen's
/// `Limits` BORROWS it — never an independent per-screen copy, and the
/// one copy is actually freed (through the caller's allocator) when the
/// path is cleared.
#[test]
fn kitty_image_medium_temp_file_screens_share_wrapper_copy() {
    let stats = crate::test_util::CountingCAllocator::new();
    let calloc = stats.c_allocator();
    let (r, t) = new_t(&calloc, 80, 24);
    assert_eq!(Result::Success, r);

    // SAFETY: `t` is a live terminal from `new_t`.
    let zt: *mut ZigTerminal = unsafe { (*t).terminal };
    // The alternate screen is allocated lazily; enter it so both slots
    // are populated — otherwise "every screen" below would be vacuously
    // true for just the primary screen.
    vtw(t, b"\x1B[?1049h");
    let screen_count = unsafe { (*zt).screens.all.iter() }
        .filter(|slot| slot.is_some())
        .count();
    assert!(
        screen_count >= 2,
        "expected at least primary + alternate screens, got {screen_count}"
    );

    let dir = b"/shared/wrapper/copy";
    let v = LibString::init(dir);
    let alloc_calls_before = stats.alloc_calls.get();
    assert_eq!(Result::Success, unsafe {
        set(
            t,
            Option_::KittyImageMediumTempFile,
            core::ptr::addr_of!(v) as *const c_void,
        )
    });
    // The wrapper allocates its ONE copy through the caller allocator —
    // not one allocation per screen (there are `screen_count` screens,
    // all sharing it).
    assert_eq!(
        alloc_calls_before + 1,
        stats.alloc_calls.get(),
        "setting the path must make exactly one allocation, shared by every screen"
    );

    // Every screen's `directory` pointer must be the SAME address as the
    // wrapper's own copy — a shared borrow, not N independent clones.
    // SAFETY: `t` is a live terminal; `tmp_dir_path` was just populated.
    let wrapper_ptr = unsafe { (*t).tmp_dir_path.as_ref() }
        .expect("tmp_dir_path must be populated after set")
        .ptr;
    let mut checked = 0;
    for slot in unsafe { (*zt).screens.all.iter() } {
        if let Some(screen) = *slot {
            match unsafe { &(*screen).kitty_images.image_limits.temporary_file } {
                TemporaryFileLimit::Enabled { directory } => {
                    assert_eq!(
                        wrapper_ptr as *const u8,
                        directory.as_ptr(),
                        "every screen must borrow the wrapper's single copy"
                    );
                    assert_eq!(dir.as_slice(), directory.as_slice());
                    checked += 1;
                }
                TemporaryFileLimit::Disabled => panic!("temp file medium was not enabled"),
            }
        }
    }
    assert_eq!(screen_count, checked);

    // Clearing the path must free the wrapper's one copy through the
    // caller allocator (not leak it, and not free it more than once —
    // Miri/the counting allocator would trip on a double free).
    let free_calls_before = stats.free_calls.get();
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::KittyImageMediumTempFile, core::ptr::null())
    });
    assert!(
        stats.free_calls.get() > free_calls_before,
        "clearing the path must free the wrapper's copy"
    );
    // SAFETY: `t` is still live.
    assert!(
        unsafe { (*t).tmp_dir_path.is_none() },
        "the wrapper must drop its own reference once cleared"
    );
    for slot in unsafe { (*zt).screens.all.iter() } {
        if let Some(screen) = *slot {
            assert_eq!(
                TemporaryFileLimit::Disabled,
                unsafe { &(*screen).kitty_images.image_limits.temporary_file }.clone(),
                "every screen must stop borrowing the freed copy"
            );
        }
    }

    unsafe { free(t) };
}

/// The corpus the allocation-failure sweep drives through the C API: text
/// that scrolls into history (node allocation), titles and pwds (the
/// `gpa()`-grown lists), OSC 8 hyperlinks (the cursor's heap hyperlink),
/// the alternate screen (a second `Screen` from `ScreenSet.getInit`), kitty
/// drag and drop state, wide tab stops after a resize past 512 columns, a
/// tracked grid ref (a pin and the tracked-pin set), a search (the
/// terminal's `searches` registry), and a full reset.
///
/// Parity tail S7-4 adds: Kitty graphics (a transmit-and-display, a chunked
/// transmit, an animation frame and a composition, a display and an
/// uppercase delete), a glyph protocol registration (the request bytes and
/// the decoded outline), an OSC 52 write and a Kitty clipboard write
/// transaction (both through the installed clipboard effect), a
/// five-representation Kitty clipboard write (the C trampoline's array past
/// its four stack slots) and a Kitty clipboard read whose reply spills the
/// reply trampoline's stack buffer, continuation tracking (the tracker's
/// reservation and growth), a color response larger than its stack buffer,
/// and XTGETTCAP / DECRQSS.
fn sweep_corpus(t: Terminal, before_reset: &dyn Fn(Terminal)) -> Vec<Result> {
    // A smaller corpus under Miri (the design's "small-corpus instance");
    // it still reaches every family below.
    let (lines, more_lines) = if cfg!(miri) { (4, 2) } else { (60, 40) };
    let mut results = Vec::new();
    for i in 0..lines {
        vtw(
            t,
            format!("line {i:03} the quick brown fox jumps over the lazy dog\r\n").as_bytes(),
        );
    }
    vtw(t, b"\x1B]0;a window title\x1B\\");
    // A longer title grows a non-empty list: `remap`, then the fallback.
    vtw(
        t,
        b"\x1B]2;a considerably longer window title, past the first capacity of the list: 0123456789012345678901234567890123456789012345678901234567890123456789\x1B\\",
    );
    vtw(t, b"\x1B]7;file:///tmp/sweep\x1B\\");
    // An OSC longer than the parser's fixed buffer: the stream's capture
    // grows through the caller's allocator by `remap`.
    let mut osc = b"\x1B]52;c;".to_vec();
    osc.extend(std::iter::repeat_n(b'A', rustty_vt::osc::MAX_BUF * 3));
    osc.extend_from_slice(b"\x1B\\");
    vtw(t, &osc);
    vtw(
        t,
        b"\x1B]8;id=one;https://example.com/one\x1B\\link\x1B]8;;\x1B\\",
    );
    vtw(t, b"\x1B]8;;https://example.com/implicit\x1B\\open");
    vtw(t, b"\x1B[?1049h");
    vtw(t, b"alternate screen text\r\n\x1B]2;alt title\x1B\\");
    vtw(t, b"\x1B[?1049l");
    // Kitty drag and drop: registration (the state and its MIME list) and
    // an acceptance status; the registration outlives RIS and is
    // destroyed by `free`.
    vtw(t, b"\x1B]72;t=a;text/plain text/uri-list\x1B\\");
    vtw(t, b"\x1B]72;t=m:o=1;text/plain\x1B\\");
    // Kitty graphics.
    vtw(t, b"\x1B_Ga=T,f=24,s=1,v=1,i=1;/wAA\x1B\\");
    vtw(t, b"\x1B_Ga=t,f=24,s=2,v=1,i=2,m=1;/wAA\x1B\\");
    vtw(t, b"\x1B_Gm=0;AAD/\x1B\\");
    vtw(t, b"\x1B_Ga=f,i=1,f=24,s=1,v=1;AAD/\x1B\\");
    vtw(t, b"\x1B_Ga=c,i=1,r=1,c=2,w=1,h=1\x1B\\");
    vtw(t, b"\x1B_Ga=p,i=2\x1B\\");
    vtw(t, b"\x1B_Ga=d,d=I,i=2\x1B\\");
    // A glyph protocol registration: a one-contour, three-point outline.
    vtw(
        t,
        b"\x1B_25a1;r;cp=e0a0;AAEAAAAAAGQAZAACAAABAQEAAABk/84AAAAAAGQ=\x1B\\",
    );
    // Clipboard writes (the sweep installs a clipboard effect).
    vtw(t, b"\x1B]52;c;aGVsbG8=\x1B\\");
    vtw(t, b"\x1B]5522;type=write\x1B\\");
    vtw(
        t,
        b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\",
    );
    vtw(t, b"\x1B]5522;type=wdata\x1B\\");
    vtw(t, b"\x1B]5522;type=write\x1B\\");
    for mime in ["dGV4dC9h", "dGV4dC9i", "dGV4dC9j", "dGV4dC9k", "dGV4dC9l"] {
        vtw(
            t,
            format!("\x1B]5522;type=wdata:mime={mime};eA==\x1B\\").as_bytes(),
        );
    }
    vtw(t, b"\x1B]5522;type=wdata\x1B\\");
    vtw(t, b"\x1B]5522;type=read;dGV4dC9wbGFpbg==\x1B\\");
    // Continuation tracking: past `growCapacity(4096)` (6272 bytes on
    // 64-bit targets) the suffix grows beyond the tracker's initial
    // reservation (under Miri only the reservation).
    let (max, unfinished) = if cfg!(miri) {
        (64usize, 40)
    } else {
        (16384, 7000)
    };
    results.push(unsafe {
        set(
            t,
            Option_::ContinuationMaxBytes,
            &max as *const usize as *const c_void,
        )
    });
    let mut csi = b"\x1B[".to_vec();
    csi.extend(std::iter::repeat_n(b'1', unfinished));
    vtw(t, &csi);
    vtw(t, b"m");
    // A color response past its 1024-byte stack buffer.
    let mut queries = b"\x1B]4".to_vec();
    for i in 0..64 {
        queries.extend_from_slice(format!(";{i};?").as_bytes());
    }
    queries.extend_from_slice(b"\x1B\\");
    vtw(t, &queries);
    // DCS: XTGETTCAP ("TN") and DECRQSS (SGR).
    vtw(t, b"\x1BP+q544e\x1B\\");
    vtw(t, b"\x1BP$qm\x1B\\");
    results.push(unsafe { resize(t, 600, 30, 8, 16) });
    vtw(t, b"\x1B[3g\x1B[1;590H\x1BH\r\t\tafter tabs\r\n");
    let mut ref_: crate::grid_ref_tracked::CTrackedGridRef = std::ptr::null_mut();
    results.push(unsafe {
        grid_ref_track(
            t,
            crate::point::PointC::cval(rustty_vt::point::Point::Active(
                rustty_vt::point::Coordinate { x: 0, y: 0 },
            )),
            &mut ref_,
        )
    });
    // A search over the caller's allocator registers itself in the
    // terminal's `searches` map (grown through the terminal's allocator).
    // SAFETY: `t` is the live wrapper; its allocator copy is read out.
    let search_alloc: Option<CAllocator> = unsafe { (*t).c_alloc };
    let mut search: crate::search::Search = std::ptr::null_mut();
    results.push(unsafe { crate::search::new(search_alloc.as_ref(), &mut search, t) });
    for i in 0..more_lines {
        vtw(t, format!("more {i:03}\r\n").as_bytes());
    }
    results.push(unsafe { resize(t, 80, 24, 8, 16) });
    before_reset(t);
    vtw(t, b"\x1Bc");
    vtw(t, b"\x1B]2;after reset\x1B\\done");
    if !ref_.is_null() {
        unsafe { crate::grid_ref_tracked::tracked_grid_ref_free(ref_) };
    }
    if !search.is_null() {
        unsafe { crate::search::free(search) };
    }
    results
}

thread_local! {
    /// Clipboard writes the sweep's effect accepted (this thread's).
    static SWEEP_CLIPBOARD_WRITES: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    /// Clipboard reads the sweep's effect served (this thread's).
    static SWEEP_CLIPBOARD_READS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

// Parity tail S7-4 (routing evidence, no reference counterpart): the
// clipboard trampolines allocate their C arrays through the terminal's
// `gpa()`. A write of five representations spills past the four stack
// slots into one 160-byte array on the caller's allocator; four stay on
// the stack. A read reply of nine contents spills past
// `stackFallback(256, gpa)` (8 x 32 bytes fit) into exactly one extra
// allocation. Every spilled byte is returned.
#[test]
fn clipboard_trampolines_route_through_the_terminals_allocator() {
    use rustty_vt::alloc::testing::CountingAllocator;
    thread_local! {
        static COUNTING: core::cell::Cell<*const CountingAllocator> =
            const { core::cell::Cell::new(std::ptr::null()) };
        static WRITE_CONTENTS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static WRITE_CALLS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static REPLY_CONTENTS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static REPLY_ALLOCS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }
    fn counting() -> &'static CountingAllocator {
        // SAFETY: set to the test's live allocator for the test's body.
        unsafe { &*COUNTING.with(|c| c.get()) }
    }
    extern "C" fn write_pty(_: Terminal, _: *mut c_void, _: *const u8, _: usize) {}
    extern "C" fn clipboard_write(_: Terminal, _: *mut c_void, request: *const ClipboardWrite) {
        unsafe {
            WRITE_CONTENTS.with(|c| c.set((*request).contents_len));
            WRITE_CALLS.with(|c| c.set(counting().alloc_calls.get()));
            ((*request).reply)(
                request,
                &ClipboardWriteReply {
                    size: std::mem::size_of::<ClipboardWriteReply>(),
                    result: clipboard::WriteStatus::Success,
                    remember: false,
                },
            );
        }
    }
    extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
        let n = REPLY_CONTENTS.with(|c| c.get());
        let contents = [ClipboardContent {
            mime: LibString::init(b"text/plain"),
            data: LibString::init(b"x"),
        }; 9];
        let before = counting().alloc_calls.get();
        unsafe {
            ((*request).reply)(
                request,
                &ClipboardReadReply {
                    size: std::mem::size_of::<ClipboardReadReply>(),
                    result: clipboard::ReadStatus::Success,
                    contents: contents.as_ptr(),
                    contents_len: n,
                    available: std::ptr::null(),
                    available_len: 0,
                    remember: false,
                },
            );
        }
        REPLY_ALLOCS.with(|c| c.set(counting().alloc_calls.get() - before));
    }

    let counting_alloc = CountingAllocator::over_heap();
    COUNTING.with(|c| c.set(&counting_alloc));
    let calloc = counting_alloc.allocator();
    let mut t: Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        new(&calloc as *const CAllocator, &mut t, 80, 24)
    });
    unsafe {
        assert_eq!(
            Result::Success,
            set(t, Option_::WritePty, write_pty as *const c_void)
        );
        assert_eq!(
            Result::Success,
            set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
        );
        assert_eq!(
            Result::Success,
            set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
        );
    }
    assert_eq!(32, std::mem::size_of::<ClipboardContent>());

    // Writes: four representations stay on the stack, five spill one
    // allocation more before the callback runs.
    let mut spills = [0usize; 2];
    for (i, (n, mimes)) in [0usize, 1].into_iter().zip([
        (
            4usize,
            &["dGV4dC9h", "dGV4dC9i", "dGV4dC9j", "dGV4dC9k"][..],
        ),
        (
            5,
            &["dGV4dC9h", "dGV4dC9i", "dGV4dC9j", "dGV4dC9k", "dGV4dC9l"][..],
        ),
    ]) {
        vtw(t, b"\x1B]5522;type=write:id=w\x1B\\");
        for m in mimes {
            vtw(
                t,
                format!("\x1B]5522;type=wdata:mime={m};eA==\x1B\\").as_bytes(),
            );
        }
        let live = counting_alloc.live_bytes.get();
        let before = counting_alloc.alloc_calls.get();
        vtw(t, b"\x1B]5522;type=wdata\x1B\\");
        spills[i] = WRITE_CALLS.with(|c| c.get()) - before;
        assert_eq!(n, WRITE_CONTENTS.with(|c| c.get()));
        assert!(
            counting_alloc.live_bytes.get() <= live,
            "the array is freed"
        );
    }
    assert_eq!(spills[0] + 1, spills[1], "five representations spill once");

    // Read replies: eight contents fit the 256-byte buffer, nine spill one
    // allocation more.
    let mut allocs = [0usize; 2];
    for (i, n) in [8usize, 9].into_iter().enumerate() {
        REPLY_CONTENTS.with(|c| c.set(n));
        vtw(t, b"\x1B]5522;type=read:id=r;dGV4dC9wbGFpbg==\x1B\\");
        allocs[i] = REPLY_ALLOCS.with(|c| c.get());
    }
    assert_eq!(allocs[0] + 1, allocs[1], "nine contents spill once");

    unsafe { free(t) };
    assert_eq!(0, counting_alloc.live_bytes.get(), "every byte is returned");
    COUNTING.with(|c| c.set(std::ptr::null()));
}

// Parity tail S7-4 (routing evidence, no reference counterpart): enabling
// continuation tracking builds the tracker over the terminal's allocator
// (`.init(wrapper.terminal.gpa(), max_bytes)`): its best-effort
// `ensureTotalCapacity(4096)` reservation (`growCapacity(4096)` bytes) and
// its growth reach the caller's allocator, and disabling or `free` returns
// every byte.
#[test]
fn continuation_tracker_routes_through_the_terminals_allocator() {
    use rustty_vt::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let calloc = counting.allocator();
    let mut t: Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        new(&calloc as *const CAllocator, &mut t, 80, 24)
    });
    let live = counting.live_bytes.get();
    let before = counting.alloc_calls.get();
    let max: usize = 16384;
    assert_eq!(Result::Success, unsafe {
        set(
            t,
            Option_::ContinuationMaxBytes,
            &max as *const usize as *const c_void,
        )
    });
    assert_eq!(before + 1, counting.alloc_calls.get(), "the reservation");
    let first = rustty_vt::alloc::ArrayListUnmanaged::<u8>::grow_capacity(4096);
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    assert_eq!(6272, first);
    assert_eq!(Some((first, 0)), counting.last_alloc.get());

    let mut input = vec![b'1'; 7000];
    input[0..2].copy_from_slice(b"\x1b[");
    vtw(t, &input);
    assert!(
        counting.live_bytes.get() >= live + 7000,
        "the retained suffix is on the caller's allocator"
    );

    let zero: usize = 0;
    assert_eq!(Result::Success, unsafe {
        set(
            t,
            Option_::ContinuationMaxBytes,
            &zero as *const usize as *const c_void,
        )
    });
    assert_eq!(
        live,
        counting.live_bytes.get(),
        "disabling frees the suffix"
    );
    unsafe { free(t) };
    assert_eq!(0, counting.live_bytes.get());
}

// Parity tail S7-4: the sweep corpus is not vacuous. On a clean run over a
// counting C allocator it stores a Kitty image with an animation frame
// (and deletes the chunked one), registers the glyph, and hands both
// clipboard writes to the effect; every byte it took is returned by `free`.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): intractable under Miri, one pass of the sweep corpus costs about 30 minutes of Miri time (its 600-column resize alone about 27); measured past a 20-minute bound; runs natively and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn sweep_corpus_reaches_the_s7_4_families() {
    use rustty_vt::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let calloc = counting.allocator();
    let mut t: Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        new(&calloc as *const CAllocator, &mut t, 80, 24)
    });
    sweep_install_effects(t);
    SWEEP_CLIPBOARD_WRITES.with(|c| c.set(0));
    SWEEP_CLIPBOARD_READS.with(|c| c.set(0));
    let reached = |t: Terminal| {
        // SAFETY: `t` is the live wrapper; raw reads of its terminal and
        // active screen.
        unsafe {
            let term = (*t).terminal;
            let storage = &(*(*term).screens.active).kitty_images;
            let img = storage.images.get(&1).expect("image 1 is stored");
            assert_eq!(
                1,
                img.animation
                    .as_ref()
                    .expect("image 1 animates")
                    .frames
                    .len()
            );
            assert!(storage.images.get(&2).is_none(), "image 2 was deleted");
            assert!(
                (*term).glyph_glossary.contains(0xE0A0),
                "the glyph is registered"
            );
            let tracker = (*t)
                .stream
                .continuation
                .as_ref()
                .expect("continuation tracking is on");
            assert!(!tracker.broken);
            assert_eq!(1, (*t).searches.count(), "the search is registered");
            assert!(
                tracker.bytes.capacity()
                    > rustty_vt::alloc::ArrayListUnmanaged::<u8>::grow_capacity(4096),
                "the suffix grew"
            );
        }
    };
    for r in sweep_corpus(t, &reached) {
        assert_eq!(Result::Success, r);
    }
    // OSC 52 twice (the long capture and the short write) and the two
    // Kitty clipboard commits; the Kitty read.
    assert_eq!(4, SWEEP_CLIPBOARD_WRITES.with(|c| c.get()));
    assert_eq!(1, SWEEP_CLIPBOARD_READS.with(|c| c.get()));
    let calls_before_free = counting.alloc_calls.get();
    unsafe { free(t) };
    // (S7-3's corpus made 33 allocations; the families above add theirs.)
    assert!(
        calls_before_free > 33,
        "the corpus allocates: {calls_before_free}"
    );
    assert_eq!(0, counting.live_bytes.get(), "free returns every byte");
}

/// The effects the sweep corpus exercises: a pty sink, a clipboard writer
/// that accepts every write, and a clipboard reader that answers with nine
/// contents. Setting them allocates nothing.
fn sweep_install_effects(t: Terminal) {
    extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
        SWEEP_CLIPBOARD_READS.with(|c| c.set(c.get() + 1));
        let contents = [ClipboardContent {
            mime: LibString::init(b"text/plain"),
            data: LibString::init(b"x"),
        }; 9];
        // SAFETY: `request` is valid for the callback.
        unsafe {
            ((*request).reply)(
                request,
                &ClipboardReadReply {
                    size: std::mem::size_of::<ClipboardReadReply>(),
                    result: clipboard::ReadStatus::Success,
                    contents: contents.as_ptr(),
                    contents_len: contents.len(),
                    available: std::ptr::null(),
                    available_len: 0,
                    remember: false,
                },
            )
        };
    }
    extern "C" fn write_pty(_: Terminal, _: *mut c_void, _: *const u8, _: usize) {}
    extern "C" fn clipboard_write(_: Terminal, _: *mut c_void, request: *const ClipboardWrite) {
        SWEEP_CLIPBOARD_WRITES.with(|c| c.set(c.get() + 1));
        // SAFETY: `request` is valid for the callback.
        unsafe {
            ((*request).reply)(
                request,
                &ClipboardWriteReply {
                    size: std::mem::size_of::<ClipboardWriteReply>(),
                    result: clipboard::WriteStatus::Success,
                    remember: false,
                },
            )
        };
    }
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::WritePty, write_pty as *const c_void)
    });
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
    });
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
    });
}

/// One sweep scenario over `failing`: `new`, the corpus, `free`. Asserts
/// the result set and that every byte taken from the caller's allocator is
/// returned. Returns (whether `new` failed, API calls that reported OOM).
fn sweep_once(failing: &rustty_vt::alloc::testing::FailingAllocator, what: &str) -> (bool, usize) {
    let calloc = failing.allocator();
    let mut t: Terminal = std::ptr::null_mut();
    let r = unsafe { new(&calloc as *const CAllocator, &mut t, 80, 24) };
    let mut new_failed = false;
    let mut call_failures = 0usize;
    match r {
        Result::Success => {
            assert!(!t.is_null());
            sweep_install_effects(t);
            for r in sweep_corpus(t, &|_| {}) {
                assert!(
                    matches!(r, Result::Success | Result::OutOfMemory),
                    "{what}: unexpected result {r:?}"
                );
                if r == Result::OutOfMemory {
                    call_failures += 1;
                }
            }
            unsafe { free(t) };
        }
        Result::OutOfMemory => {
            assert!(
                t.is_null(),
                "{what}: a failed new must not return a terminal"
            );
            new_failed = true;
        }
        other => panic!("{what}: new returned {other:?}"),
    }
    assert_eq!(
        failing.allocated_bytes.get(),
        failing.freed_bytes.get(),
        "{what}: every byte taken from the caller's allocator is returned"
    );
    assert_eq!(
        failing.allocations.get(),
        failing.deallocations.get(),
        "{what}"
    );
    (new_failed, call_failures)
}

// Parity tail S7-3 (T2, no reference counterpart — the Zig
// `checkAllAllocationFailures` idea applied to the C API): for every n, a
// `FailingAllocator` that refuses its n-th allocation drives `new`, a write
// corpus, `resize`, grid-ref tracking and `free`. Nothing aborts, every
// result is `SUCCESS` or `OUT_OF_MEMORY` (the stream's failures are
// log-and-drop), and every byte the terminal took is returned by `free`.
// The sweep ends at the first n that induces no failure: every allocation
// the scenario makes has then been failed once.
//
// A second sweep (S7-3b, review F3) refuses `remap`/`resize` from their
// n-th call on (`resize_fail_index`; sticky as in Zig, whose index only
// advances on success), so every later growth takes the alloc+copy+free
// fallback, with the same checks. Under Miri the corpus is smaller.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): intractable under Miri, one pass of the sweep corpus costs about 30 minutes of Miri time (its 600-column resize alone about 27) and the sweep makes one pass per allocation; measured past a 20-minute bound; runs natively and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn allocation_failure_sweep_through_the_c_api() {
    use rustty_vt::alloc::testing::{Config, FailingAllocator};

    let mut n = 0usize;
    let mut new_failures = 0usize;
    let mut call_failures = 0usize;
    loop {
        let failing = FailingAllocator::over_heap(Config {
            fail_index: n,
            ..Config::default()
        });
        let (new_failed, calls) = sweep_once(&failing, &format!("alloc n={n}"));
        new_failures += usize::from(new_failed);
        call_failures += calls;
        if !failing.has_induced_failure.get() {
            break;
        }
        n += 1;
        assert!(n < 10_000, "the sweep must terminate");
    }
    // The sweep reached into the terminal's internals: more allocations
    // than `new`'s own storage, some of them failing `new` itself and some
    // failing later API calls.
    let min_swept = if cfg!(miri) { 10 } else { 20 };
    assert!(n > min_swept, "only {n} allocations were swept");
    assert!(new_failures > 2, "new failed at {new_failures} indices");
    assert!(call_failures > 0, "no API call reported OUT_OF_MEMORY");

    // The remap/resize sweep. A clean run counts the scenario's successful
    // remap/resize calls (R); refusing from each n in 0..=R reaches every
    // one of them (and n = R refuses none).
    let clean = FailingAllocator::over_heap(Config::default());
    // The clean run fails nothing.
    assert_eq!((false, 0), sweep_once(&clean, "remap clean run"));
    let remaps = clean.resize_index.get();
    assert!(remaps > 0, "the corpus makes no remap/resize call");
    for n in 0..=remaps {
        let failing = FailingAllocator::over_heap(Config {
            resize_fail_index: n,
            ..Config::default()
        });
        let (new_failed, call_failures) = sweep_once(&failing, &format!("remap n={n}"));
        // A refused remap never fails the scenario: the fallback allocates.
        assert!(
            !new_failed,
            "remap n={n}: new must not fail on a refused remap"
        );
        assert_eq!(
            0, call_failures,
            "remap n={n}: no call reports OUT_OF_MEMORY on a refused remap"
        );
        assert!(
            failing.resize_index.get() <= n,
            "remap n={n}: no remap/resize succeeded past the refusal"
        );
    }
}

// Parity tail S7-7 (Rust-only routing evidence): the terminal's stream
// builds its OSC command lists through the caller's allocator
// (`Stream.init(.{ .allocator = alloc, .. })` hands `alloc` to the OSC
// parser): an OSC 104 with three indices grows the color list's shelf
// array and shelf 0, an OSC 21 grows the kitty color list once. The OSC
// parser keeps each list (the reference's `parser.command` owns it) until
// the next OSC's `reset` or the terminal's `free`. An OSC 104 with two
// indices allocates nothing (the preallocated slots).
#[test]
fn terminal_stream_osc_lists_route_through_the_callers_allocator() {
    use rustty_vt::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let calloc = counting.allocator();
    let mut t: Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        new(&calloc as *const CAllocator, &mut t, 80, 24)
    });
    let live = counting.live_bytes.get();

    let (a, f) = (counting.alloc_calls.get(), counting.free_calls.get());
    vtw(t, b"\x1B]104;1;2\x1B\\");
    assert_eq!(
        (a, f),
        (counting.alloc_calls.get(), counting.free_calls.get())
    );

    vtw(t, b"\x1B]104;1;2;3\x1B\\");
    assert_eq!(
        a + 2,
        counting.alloc_calls.get(),
        "the shelf array and shelf 0"
    );
    assert_eq!(
        f,
        counting.free_calls.get(),
        "the OSC parser keeps the list"
    );

    vtw(t, b"\x1B]21;foreground=#010203;background=?\x1B\\");
    assert_eq!(a + 3, counting.alloc_calls.get(), "the kitty color list");
    assert_eq!(
        f + 2,
        counting.free_calls.get(),
        "OSC 21's reset freed the color list"
    );
    assert_eq!(
        rustty_vt::alloc::ArrayListUnmanaged::<rustty_vt::kitty::color::Request>::grow_capacity(1)
            * core::mem::size_of::<rustty_vt::kitty::color::Request>(),
        counting.live_bytes.get() - live,
        "only the kitty list is live"
    );

    unsafe { free(t) };
    assert_eq!(0, counting.live_bytes.get());
}

// Parity tail S7-8 (N331): `continuation_alloc` builds the continuation in
// a `Writer.Allocating` over the caller's allocator (`continuationAllocIo(
// .., lib.alloc.default(alloc_), ..)`) and hands its buffer over
// (`toOwnedSlice`); `restoreContinuation`'s verification writer is on the
// terminal's allocator and is freed before returning. The continuation is
// written a byte at a time (the tracker's `writeByte` loop, which
// `Writer.Allocating` grows by `growCapacity(1)`).
#[test]
fn continuation_writers_route_through_their_allocators() {
    use rustty_vt::alloc::testing::{Config, CountingAllocator, FailingAllocator};
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(t);
        vtw(t, b"\x1b[31");

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        assert_eq!(Result::Success, unsafe {
            continuation_alloc(t, Some(&c), &mut out_ptr, &mut out_len)
        });
        assert_eq!(b"\x1b[31", unsafe {
            core::slice::from_raw_parts(out_ptr, out_len)
        });
        // One growth (the first `writeByte` rebases: growCapacity(1) =
        // 129 bytes), then `toOwnedSlice` remaps it to the 4 bytes.
        assert_eq!(1, counting.alloc_calls.get());
        assert_eq!(Some((129, 0)), counting.last_alloc.get());
        assert_eq!(1, counting.remap_calls.get());
        assert_eq!(0, counting.free_calls.get());
        assert_eq!(4, counting.live_bytes.get());
        unsafe { crate::allocator::free(Some(&c), out_ptr, out_len) };
        assert_eq!(0, counting.live_bytes.get());

        // A failing caller allocator: OUT_OF_MEMORY, nothing handed out.
        for config in [
            Config {
                fail_index: 0,
                ..Config::default()
            },
            Config {
                fail_index: 1,
                resize_fail_index: 0,
            },
        ] {
            let failing = FailingAllocator::over_heap(config);
            let fc = failing.allocator();
            let mut out_ptr: *mut u8 = std::ptr::null_mut();
            let mut out_len: usize = 0;
            assert_eq!(Result::OutOfMemory, unsafe {
                continuation_alloc(t, Some(&fc), &mut out_ptr, &mut out_len)
            });
            assert!(out_ptr.is_null());
            assert!(failing.has_induced_failure.get());
            assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
        }
        unsafe { free(t) };
    });

    // restoreContinuation: the exported copy is on the terminal's
    // allocator, and a failure there is `error.OutOfMemory`.
    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    let (new_result, restored) = new_t(&c, 80, 24);
    assert_eq!(Result::Success, new_result);
    test_enable_continuation(restored);
    let (allocs, frees, live) = (
        counting.alloc_calls.get(),
        counting.free_calls.get(),
        counting.live_bytes.get(),
    );
    unsafe { restore_continuation(restored, b"") }.unwrap();
    // An empty continuation writes nothing: no call.
    assert_eq!(allocs, counting.alloc_calls.get());
    unsafe { restore_continuation(restored, b"\x1b[31") }.unwrap();
    // The replay fits the tracker's reservation; the exported copy is
    // one 129-byte growth, freed on return.
    assert_eq!(allocs + 1, counting.alloc_calls.get());
    assert_eq!(frees + 1, counting.free_calls.get());
    assert_eq!(Some((129, 0)), counting.last_alloc.get());
    assert_eq!(Some((129, 0)), counting.last_free.get());
    assert_eq!(live, counting.live_bytes.get());
    unsafe { free(restored) };
    assert_eq!(0, counting.live_bytes.get());

    let failing = FailingAllocator::over_heap(Config::default());
    let fc = failing.allocator();
    let (new_result, restored) = new_t(&fc, 80, 24);
    assert_eq!(Result::Success, new_result);
    test_enable_continuation(restored);
    vtw(restored, b"\x1b[31");
    // The next allocation (the exported copy) fails.
    failing.fail_index.set(failing.alloc_index.get());
    assert_eq!(Err(RestoreContinuationError::OutOfMemory), unsafe {
        restore_continuation(restored, b"")
    });
    assert!(failing.has_induced_failure.get());
    failing.fail_index.set(usize::MAX);
    unsafe { free(restored) };
    assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
}

// Parity tail S7-8 (T2, no reference counterpart): the allocation-failure
// sweep over the C API's allocating writers. For every n, a
// `FailingAllocator` refusing its n-th allocation is both the terminal's
// allocator and the output allocator of `encode_alloc` (whose continuation
// copy is on the terminal's) and `continuation_alloc`, and
// `restoreContinuation` exports on the terminal's. Nothing aborts, every
// result is SUCCESS or OUT_OF_MEMORY (restore: `InvalidContinuation`, the
// comparison against an empty continuation, or `OutOfMemory`), and every
// byte is returned. A second loop refuses remap/resize from their n-th
// call on: the writers fall back to alloc + copy + free and nothing fails.
#[test]
fn allocation_failure_sweep_through_the_allocating_writers() {
    use rustty_vt::alloc::testing::{Config, FailingAllocator};

    fn once(failing: &FailingAllocator, what: &str) -> (bool, usize) {
        let calloc = failing.allocator();
        let mut t: Terminal = std::ptr::null_mut();
        match unsafe { new(&calloc as *const CAllocator, &mut t, 10, 3) } {
            Result::Success => {}
            Result::OutOfMemory => {
                assert!(t.is_null(), "{what}");
                assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
                return (true, 0);
            }
            other => panic!("{what}: new returned {other:?}"),
        }
        let limit: usize = 1024;
        let r = unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &limit as *const usize as *const c_void,
            )
        };
        assert!(
            matches!(r, Result::Success | Result::OutOfMemory),
            "{what}: {r:?}"
        );
        vtw(t, b"hello\x1b[31");

        let mut failures = 0usize;
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let r =
            unsafe { crate::snapshot::encode_alloc(t, Some(&calloc), &mut out_ptr, &mut out_len) };
        match r {
            Result::Success => unsafe { crate::allocator::free(Some(&calloc), out_ptr, out_len) },
            // A tracker that could not be built or grown leaves the
            // continuation unavailable: INVALID_VALUE, as in the reference.
            Result::OutOfMemory | Result::InvalidValue => failures += 1,
            other => panic!("{what}: encode_alloc returned {other:?}"),
        }
        let r = unsafe { continuation_alloc(t, Some(&calloc), &mut out_ptr, &mut out_len) };
        match r {
            Result::Success => unsafe { crate::allocator::free(Some(&calloc), out_ptr, out_len) },
            Result::OutOfMemory | Result::InvalidValue => failures += 1,
            other => panic!("{what}: continuation_alloc returned {other:?}"),
        }
        match unsafe { restore_continuation(t, b"") } {
            Ok(()) => {}
            Err(RestoreContinuationError::OutOfMemory) => failures += 1,
            Err(
                RestoreContinuationError::InvalidContinuation
                | RestoreContinuationError::ContinuationDisabled
                | RestoreContinuationError::ContinuationUnavailable,
            ) => {}
        }
        unsafe { free(t) };
        assert_eq!(
            failing.allocated_bytes.get(),
            failing.freed_bytes.get(),
            "{what}: every byte is returned"
        );
        assert_eq!(
            failing.allocations.get(),
            failing.deallocations.get(),
            "{what}"
        );
        (false, failures)
    }

    let mut n = 0usize;
    let mut call_failures = 0usize;
    loop {
        let failing = FailingAllocator::over_heap(Config {
            fail_index: n,
            ..Config::default()
        });
        let (_, calls) = once(&failing, &format!("alloc n={n}"));
        call_failures += calls;
        if !failing.has_induced_failure.get() {
            break;
        }
        n += 1;
        assert!(n < 10_000, "the sweep must terminate");
    }
    assert!(call_failures > 0, "no writer reported a failure");

    let clean = FailingAllocator::over_heap(Config::default());
    assert_eq!((false, 0), once(&clean, "remap clean run"));
    let remaps = clean.resize_index.get();
    // The writers' `toOwnedSlice` remaps, at least.
    assert!(remaps >= 3, "only {remaps} remaps");
    for n in 0..=remaps {
        let failing = FailingAllocator::over_heap(Config {
            resize_fail_index: n,
            ..Config::default()
        });
        assert_eq!(
            (false, 0),
            once(&failing, &format!("remap n={n}")),
            "remap n={n}: a refused remap never fails a call"
        );
    }

    // Every remap refused AND the n-th allocation refused: the
    // `toOwnedSlice` fallback allocations fail too (the alloc loop's
    // remaps succeed, so it never reaches them).
    let mut n = 0usize;
    let mut call_failures = 0usize;
    loop {
        let failing = FailingAllocator::over_heap(Config {
            fail_index: n,
            resize_fail_index: 0,
        });
        let (_, calls) = once(&failing, &format!("no-remap alloc n={n}"));
        call_failures += calls;
        if !failing.has_induced_failure.get() {
            break;
        }
        n += 1;
        assert!(n < 10_000, "the sweep must terminate");
    }
    assert!(call_failures > 0, "no writer reported a failure");
}
