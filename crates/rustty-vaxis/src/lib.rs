//! rustty-vaxis — pure-Rust port of the vendored `libvaxis` 0.5.1 subset (R3).
//!
//! CHUNK 1a — the CORE VALUE TYPES only. Module-per-Zig-file layout:
//! - [`cell`] — `Cell.zig` (`Style`/`Color`/`Segment`/`Cell` + SGR bits)
//! - [`key`] — `Key.zig` (`Key` + constants + matchers + `Modifiers`/`KittyFlags`)
//! - [`mouse`] — `Mouse.zig`
//! - [`grapheme_cache`] — `GraphemeCache.zig`
//! - [`image`] — `Image.zig` STRUCT SHAPE (zigimg/Window bodies are R5 seams)
//! - [`Winsize`] — the `main.zig:38` terminal-size struct
//!
//! CHUNK 1b adds the width/grapheme surface:
//! - [`gwidth`] — `gwidth.zig` (grapheme-width Method/eawToWidth/gwidth, 11-test pin)
//! - [`unicode`] — `unicode.zig` (Old-API grapheme iterator wrapper)
//! - [`uucode`] — the vendored `uucode` runtime subset (2826a37a since S-10b; types/get/grapheme/utf8
//!   + the stage1/stage2 tables `build.rs` reproduces from the vendored UCD 17.0.0)
//!
//! CHUNK 2 adds the RENDERING layer:
//! - [`screen`] — `Screen.zig` (the flat `Vec<Cell<'a>>` cell buffer)
//! - [`internal_screen`] — `InternalScreen.zig` (the diffing back buffer; arena → per-cell
//!   owned `Vec<u8>`)
//! - [`window`] — `Window.zig` (the window view: `child`/border, `write_cell`/`read_cell`,
//!   `fill`/`clear`, cursor, `print`/`print_segment` gwidth-aware layout, `scroll`,
//!   `has_mouse`; `Window<'a>` holds `screen: *mut Screen<'a>` and is `Copy`, §5)
//!
//! CHUNK 3a adds the INPUT-PARSING layer (the pure half of the backend):
//! - [`event`] — `event.zig` (the `Event<'a>` union; `Paste` owns a `Vec<u8>`)
//! - [`ctlseqs`] — `ctlseqs.zig` (~60 `&[u8]` control-sequence byte constants)
//! - [`parser`] — `Parser.zig` (the ground/escape/csi/osc/dcs/sos/pm/apc/ss3 state
//!   machine; Kitty keyboard, SGR/xterm mouse, bracketed paste, OSC-52 base64, the
//!   c1b-uucode grapheme path; `parse<'a>(&'a mut self, &'a [u8])` — PURE, no
//!   tty/Loop coupling, 34 assertion-for-assertion tests)
//!
//! CHUNK 3b adds the IMPURE half of the backend (the tty backend + concurrency):
//! - [`queue`] — `queue.zig` (the thread-safe fixed-size BLOCKING two-mirror ring;
//!   `Mutex` + `not_full`/`not_empty` `Condvar`; the 7-test Miri pin)
//! - [`tty`] — `tty.zig` (`PosixTty` termios/ioctl/sigaction FFI; the SIGWINCH global
//!   handler; `TestTty` pipe mock; `WindowsTty` = `#[cfg(windows)]` R5 seam)
//! - [`loop_`] — `Loop.zig` (the construct-then-spawn read thread: raw-self `*mut Loop`,
//!   `should_quit: AtomicBool`, `thread: Cell`, the type-erased winsize callback; the
//!   queued `T` OWNS its grapheme text via `Key::into_owned`; `handle_event_generic`
//!   ported LIVE against the [`vaxis::Vaxis`] placeholder; the `LoopEvent` trait for
//!   Zig's `@hasField`)
//! - [`vaxis`] — the Vaxis placeholder (Chunk 3b), later REPLACED by the real `Vaxis<'a>`
//!
//! CHUNK 4a REPLACES the placeholder with the real `Vaxis<'a>` — the main object + the
//! render diff-and-emit engine:
//! - [`vaxis`] — `Vaxis.zig` (the `Vaxis<'a>` struct: caps/screen/screen_last/state/opts +
//!   the query-futex; init/deinit/reset_state/resize/alt-screen/window; queryTerminal/
//!   deviceStatusReport capability queries; the RENDER diff-and-emit engine and its simpler
//!   `pretty_print` mirror; the setters/queries; the portable kitty-graphics transmit paths.
//!   The two `zigimg`-decode paths (`transmit_image`/`load_image`) are LIVE since WS12 T8/T9
//!   over `rustty_zigimg`. Filling the placeholder threaded `'a` through `Loop<'a, T>`.)
//!
//! PORT STATUS: done (chunk 1a — value types; chunk 1b — gwidth/unicode + the uucode
//! width/grapheme tables; chunk 2 — the Screen/InternalScreen/Window rendering layer;
//! chunk 3a — the event/ctlseqs/parser input-parsing layer; chunk 3b — the tty backend
//! and concurrency layer (queue/tty/loop_) plus the Vaxis placeholder; chunk 4a — the real
//! `Vaxis<'a>` object + the render diff-and-emit engine; chunk 4b — the `widgets` tier +
//! the `TextInput` gap-buffer widget). The chunk sequence is FINISHED.
//!
//! REMAINING LIVE SEAMS: **none.** The `#[cfg(windows)]` family — `WindowsTty`
//! (`tty.rs`, the full `tty.zig:189-691` console backend), the windows `tty_run` +
//! `handle_event_generic` arms (`loop_.rs`), and `TestTty::next_event` (whose upstream
//! body IS `error.SkipZigTest`) — was PORTED by WS20 (2026-07-30). What remains is
//! VERIFICATION debt, not port debt: this host has no Windows target toolchain
//! (`zlib-sys`'s C build cannot cross-compile to `x86_64-pc-windows-msvc` from macOS),
//! so the Windows arms are type-checked + clippy-clean via the host-side forced-`cfg`
//! recipe in `.port/phase-2/VERIFICATION_DEBT.md` (owner WS20) rather than by a real
//! Windows build.
//!
//! THE IMAGE TIER IS LIVE [WS12 T8–T11, 2026-07-29]. `Vaxis::transmit_image` and
//! `Vaxis::load_image` run over `rustty_zigimg`, which re-homes the six zigimg operations
//! `libvaxis` instantiates onto ghostty's own vendored codecs (decode via `rustty-wuffs`,
//! PNG encode over `zlib-sys`) — WS12 Decision 2, Option Z-C. One bounded, documented
//! divergence, a PORT NOTE and not a seam: `load_image` accepts PNG + JPEG rather than
//! zigimg's 15 formats, which is a superset of what ghostty itself decodes anywhere.
//! [`image::Image::draw`]/[`image::Image::cell_size`] are LIVE too and, as it turned out,
//! never needed zigimg at all — they are pure `Window` geometry. DEFERRALS.md
//! "rustty-vaxis Chunks 4a/4b" is now RESOLVED.
//!
//! CHUNK 4b adds the first WIDGET (the new `widgets` tier, ABOVE `window`):
//! - [`widgets`] — `widgets.zig` aggregator + [`widgets::text_input`] (`widgets/TextInput.zig`):
//!   the single-line text-input widget over a raw-UTF-8 **gap `Buffer`** (`&[u8]`/`Vec<u8>`,
//!   never a lossy `String`); `update(Event)` key-dispatch, cursor/word motion, delete, and a
//!   gwidth-aware `draw` (record-then-replay — the `Cell<'a>` borrows into `self.buf`, §6).
//!   Self-contained over `Window` → no R5 seam; a pure leaf → Miri-primary.

pub mod cell;
pub mod ctlseqs;
pub mod event;
pub mod grapheme_cache;
pub mod gwidth;
pub mod image;
pub mod internal_screen;
pub mod key;
pub mod loop_;
pub mod mouse;
pub mod parser;
pub mod queue;
pub mod screen;
pub mod tty;
pub mod unicode;
pub mod uucode;
pub mod vaxis;
// CHUNK 5 adds the vxfw (vaxis widget framework) SUBSET that `cli +boo` consumes:
// `vxfw/vxfw.zig` core (Widget/EventContext/DrawContext/Surface/SubSurface/Event/
// Command/Tick + value types + the VxfwArena) plus the three widgets boo uses:
// `vxfw::App`, `vxfw::Text`, `vxfw::Center`. The other 13 vxfw widgets are unused by
// ghostty (YAGNI) and out of scope.
pub mod vxfw;
pub mod widgets;
pub mod window;

// so `vaxis::Loop` matches Zig's `vaxis.Loop`, and `vaxis::Tty` matches `vaxis.Tty`
pub use loop_::Loop;
pub use tty::Tty;
pub use vaxis::Vaxis;

/// The size of the terminal screen
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Winsize {
    pub rows: u16,
    pub cols: u16,
    pub x_pixel: u16,
    pub y_pixel: u16,
}

/// The Zig `Panic` export (`main.zig:50-58`) + `panic_handler` (`main.zig:60-64`) as
/// an OPT-IN installer: a Rust panic hook that runs [`recover`] and then chains the
/// previously-installed hook — the exact `panic_handler` sequence (`recover();
/// std.debug.defaultPanic(...)`).
///
/// PORT NOTE (opt-in, W16-EXPLICIT-01): a Zig panic override activates only via a
/// root-module `pub const panic`/`Panic` declaration, and ghostty declares NONE (no
/// file in ghostty/src references `vaxis.Panic`/`vaxis.recover`) — so Ghostty
/// binaries never install libvaxis's panic recovery, and rustty's CLI must not
/// auto-install it either. Consumers that want the recovery path (what a Zig app
/// re-exporting `vaxis.Panic` would get) opt in by calling this once after
/// `Tty::init`.
pub fn install_panic_recovery() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        recover();
        prev(info);
    }));
}

/// Resets the terminal state using the global tty instance. Use this only to recover
/// during a panic (`main.zig:66-78`).
///
/// Zig: `if (tty.global_tty) |*gty| { gty.writer().writeAll(reset) catch {};
/// gty.writer().flush() catch {}; gty.deinit(); }` — the global is read, NOT
/// cleared, and every I/O error is
/// swallowed. The rustty `tty::GlobalTty` keeps only the restore fields (`fd` +
/// `termios` — see its divergence note), so the reset bundle goes to the fd directly
/// and `gty.deinit()` is inlined as the same tcsetattr/close `PosixTty::deinit` does.
///
/// PORT NOTE (writer divergence): Zig's `gty.writer()` is the tty's BUFFERED
/// `std.Io.Writer`, and vaxis 0.6.0 flushes it right after `writeAll`
/// (`main.zig:74-75`); rustty has no writer in the global (only `fd`), so the
/// bundle is written to the fd with `libc::write`, unbuffered, which reaches the
/// terminal at the same point the reference's flush does.
///
/// CORRECTION (task R13-R): this note used to say upstream's `writeAll` "only
/// reaches the terminal when the buffer happens to drain". That described a
/// `recover` without the flush; the pinned vaxis 0.6.0 has it.
#[cfg(not(windows))]
pub fn recover() {
    use core::ffi::c_void;

    // A poisoned lock must not abort the panic path (Zig has no lock here at all —
    // `global_tty` is a plain global); take the inner value regardless.
    let guard = tty::GLOBAL_TTY
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let Some(gty) = guard.as_ref() else { return };

    // const reset: []const u8 = ctlseqs.csi_u_pop ++ ctlseqs.mouse_reset ++
    //     ctlseqs.bp_reset ++ ctlseqs.rmcup;
    // gty.writer().writeAll(reset) catch {};
    // gty.writer().flush() catch {};
    // (one concatenated byte bundle; a failed write abandons the REST of the bundle,
    // exactly as `writeAll`'s error return would)
    'write_all: for seq in [
        ctlseqs::csi_u_pop,
        ctlseqs::mouse_reset,
        ctlseqs::bp_reset,
        ctlseqs::rmcup,
    ] {
        let mut off = 0;
        while off < seq.len() {
            let n = unsafe {
                libc::write(
                    gty.fd,
                    seq[off..].as_ptr() as *const c_void,
                    seq.len() - off,
                )
            };
            if n <= 0 {
                break 'write_all; // catch {}
            }
            off += n as usize;
        }
    }

    // gty.deinit() — `PosixTty::deinit` over the stored fields: restore the saved
    // termios, then close (except on macos, where closing /dev/tty may block
    // indefinitely — the same cfg `PosixTty::deinit` carries).
    if unsafe { libc::tcsetattr(gty.fd, libc::TCSAFLUSH, &gty.termios) } != 0 {
        // couldn't restore terminal (Zig logs the error; log.* dropped, §10.16)
    }
    #[cfg(not(target_os = "macos"))]
    unsafe {
        libc::close(gty.fd);
    }
}

/// The `#[cfg(windows)]` arm of [`recover`] — same `main.zig:66-78` shape over the
/// Windows [`tty::windows_tty::GlobalTty`]: the reset bundle goes out via `WriteFile`
/// on the stored stdout handle (the `TtyWriter` drain), then `gty.deinit()` is
/// inlined as the same codepage/console-mode restore + `CloseHandle` pair
/// `WindowsTty::deinit` does.
#[cfg(windows)]
pub fn recover() {
    use tty::windows_tty as wt;

    let guard = wt::GLOBAL_TTY
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let Some(gty) = guard.as_ref() else { return };
    let stdin = gty.stdin as wt::HANDLE;
    let stdout = gty.stdout as wt::HANDLE;

    'write_all: for seq in [
        ctlseqs::csi_u_pop,
        ctlseqs::mouse_reset,
        ctlseqs::bp_reset,
        ctlseqs::rmcup,
    ] {
        let mut off = 0;
        while off < seq.len() {
            let mut written: wt::DWORD = 0;
            // the bundle is ~48 bytes, so the `DWORD` cast cannot truncate
            let ok = unsafe {
                wt::WriteFile(
                    stdout,
                    seq[off..].as_ptr(),
                    (seq.len() - off) as wt::DWORD,
                    &mut written,
                    core::ptr::null_mut(),
                )
            };
            if ok == 0 || written == 0 {
                break 'write_all; // catch {}
            }
            off += written as usize;
        }
    }

    // gty.deinit() — `WindowsTty::deinit` over the stored fields.
    unsafe { wt::SetConsoleOutputCP(gty.initial_codepage) };
    let _ = unsafe { wt::setConsoleMode(stdin, gty.initial_input_mode) };
    let _ = unsafe { wt::setConsoleMode(stdout, gty.initial_output_mode) };
    unsafe { wt::CloseHandle(stdin) };
    unsafe { wt::CloseHandle(stdout) };
}

// The recover()/install_panic_recovery() proving tests (W16-EXPLICIT-01). POSIX-only:
// they seed `tty::GLOBAL_TTY`, which is the POSIX static (the Windows twin lives in
// `windows_tty` and cannot be exercised on this host — VERIFICATION_DEBT.md V10).
#[cfg(all(test, not(windows)))]
mod recover_tests {
    use super::*;
    use core::ffi::c_void;
    use libc::c_int;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    /// `tty::GLOBAL_TTY` is process-global mutable state; every test here must hold
    /// this lock for its whole body so the seeds cannot interleave.
    static GLOBAL_TTY_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// The exact `main.zig:69-73` comptime concatenation.
    fn reset_bundle() -> Vec<u8> {
        [
            ctlseqs::csi_u_pop,
            ctlseqs::mouse_reset,
            ctlseqs::bp_reset,
            ctlseqs::rmcup,
        ]
        .concat()
    }

    fn seed_global_tty(fd: c_int, termios: libc::termios) {
        *tty::GLOBAL_TTY.lock().unwrap() = Some(tty::GlobalTty { fd, termios });
    }

    fn clear_global_tty() {
        *tty::GLOBAL_TTY.lock().unwrap() = None;
    }

    /// Blocking read of exactly `len` bytes off `fd` (the pipe/pty read end).
    fn read_exact_fd(fd: c_int, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        let mut off = 0;
        while off < len {
            let n = unsafe { libc::read(fd, out[off..].as_mut_ptr() as *mut c_void, len - off) };
            assert!(n > 0, "short read at {off}/{len} (n={n})");
            off += n as usize;
        }
        out
    }

    /// recover() writes EXACTLY csi_u_pop ++ mouse_reset ++ bp_reset ++ rmcup to the
    /// stored fd (the `main.zig:69-74` writeAll), byte-for-byte. Pipe-backed so it
    /// runs everywhere the TestTty tests run.
    #[test]
    // `recover()` unconditionally restores the saved termios through
    // `tcsetattr` (lib.rs:203) before writing the reset bundle, so even this
    // pipe-backed variant reaches the same foreign function Miri cannot call.
    // Same exception class as its two siblings; see
    // .port/phase-2/MIRI_EXCEPTIONS.md, "rustty-vaxis Chunk 3b".
    #[cfg_attr(
        miri,
        ignore = "recover() calls tcsetattr — see MIRI_EXCEPTIONS.md rustty-vaxis Chunk 3b"
    )]
    fn recover_writes_exact_reset_bundle_to_stored_fd() {
        let _guard = GLOBAL_TTY_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());

        let mut fds = [0 as c_int; 2];
        assert_eq!(0, unsafe { libc::pipe(fds.as_mut_ptr()) });
        let (r, w) = (fds[0], fds[1]);

        // a zeroed termios: tcsetattr on a pipe fails (ENOTTY) and recover must
        // swallow it (the Zig `catch`/dropped-log arm) without touching the bytes.
        seed_global_tty(w, unsafe { std::mem::zeroed() });
        recover();

        let expected = reset_bundle();
        assert_eq!(expected, read_exact_fd(r, expected.len()));

        // Zig reads the global WITHOUT clearing it — recover must leave it Some.
        assert!(tty::GLOBAL_TTY.lock().unwrap().is_some());

        clear_global_tty();
        unsafe {
            libc::close(r);
            // on non-macos recover's inlined `gty.deinit()` already closed `w`
            #[cfg(target_os = "macos")]
            libc::close(w);
        }
    }

    /// The termios-restore arm (`gty.deinit()` → tcsetattr TCSAFLUSH of the SAVED
    /// termios) over a real pty pair: raw the slave via `make_raw`, seed the global
    /// with the pre-raw termios, and assert recover() restored it (ECHO/ICANON back
    /// on) — observed through the master, which shares the pty's termios and
    /// survives recover's close-on-linux of the slave fd.
    #[test]
    // Real-pty FFI: `openpty` + `tcgetattr`/`tcsetattr`, which Miri cannot call
    // ("can't call foreign function `tcsetattr` on OS `macos`") and has nothing
    // to model — the subject IS the terminal driver's saved-termios state. This
    // is the class `.port/phase-2/MIRI_EXCEPTIONS.md` already argues under
    // "rustty-vaxis Chunk 3b — the `tty.rs` termios/ioctl/sigaction FFI"
    // (R6 verifier: ASan/native); these two lib.rs tests simply never carried
    // the annotation. Both run in ci.sh's HARD `cargo test --workspace` step.
    #[cfg_attr(
        miri,
        ignore = "real pty + termios FFI (tcgetattr/tcsetattr) — see MIRI_EXCEPTIONS.md rustty-vaxis Chunk 3b"
    )]
    fn recover_restores_saved_termios_on_a_pty() {
        let _guard = GLOBAL_TTY_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());

        let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
        // R14: this used to return early ("unavailable in this sandbox"), which
        // reports `ok` having checked nothing. Every supported host has ptys (the
        // rustty-os and termio pty tests open them unconditionally), so a failure
        // here is a real environment problem and must say so.
        assert!(
            master >= 0,
            "posix_openpt failed: {}",
            std::io::Error::last_os_error()
        );
        assert_eq!(0, unsafe { libc::grantpt(master) });
        assert_eq!(0, unsafe { libc::unlockpt(master) });
        let name = unsafe { libc::ptsname(master) };
        assert!(!name.is_null());
        let slave = unsafe { libc::open(name, libc::O_RDWR | libc::O_NOCTTY, 0) };
        assert!(slave >= 0);

        // the pre-raw termios (ECHO/ICANON on for a fresh pty)
        let mut orig: libc::termios = unsafe { std::mem::zeroed() };
        assert_eq!(0, unsafe { libc::tcgetattr(slave, &mut orig) });
        assert_ne!(0, orig.c_lflag & libc::ECHO);
        assert_ne!(0, orig.c_lflag & libc::ICANON);

        // `Tty.makeRaw` — after it the pty is raw...
        let saved = tty::PosixTty::make_raw(slave).expect("make_raw");
        let mut now: libc::termios = unsafe { std::mem::zeroed() };
        assert_eq!(0, unsafe { libc::tcgetattr(master, &mut now) });
        assert_eq!(0, now.c_lflag & libc::ECHO);
        assert_eq!(0, now.c_lflag & libc::ICANON);

        // ...and recover() (seeded exactly as `PosixTty::init` seeds: fd + the
        // make_raw-returned ORIGINAL termios) puts it back.
        //
        // The master MUST be drained concurrently: recover's `gty.deinit()` does
        // `tcsetattr(TCSAFLUSH)`, and TCSAFLUSH first WAITS for the pty's pending
        // output (the reset bundle just written) to be transmitted — with nobody
        // reading the master that ioctl blocks forever (measured: the ioctl parks in
        // `tcsetattr`). Zig's `posix.tcsetattr(fd, .FLUSH, ...)` is the same call, so
        // this is the pty harness's obligation, not a port divergence.
        let expected = reset_bundle();
        let want = expected.len();
        let drain = std::thread::spawn(move || read_exact_fd(master, want));

        seed_global_tty(slave, saved);
        recover();

        // the reset bundle went out the slave -> readable on the master
        assert_eq!(expected, drain.join().expect("master drain thread"));

        let mut restored: libc::termios = unsafe { std::mem::zeroed() };
        assert_eq!(0, unsafe { libc::tcgetattr(master, &mut restored) });
        assert_ne!(0, restored.c_lflag & libc::ECHO);
        assert_ne!(0, restored.c_lflag & libc::ICANON);

        clear_global_tty();
        unsafe {
            libc::close(master);
            // on non-macos recover's inlined `gty.deinit()` already closed `slave`
            #[cfg(target_os = "macos")]
            libc::close(slave);
        }
    }

    /// install_panic_recovery(): a panic runs recover() AND then the previously
    /// installed hook, in that order (`panic_handler`: `recover(); defaultPanic(...)`).
    #[test]
    // Real-pty FFI: `openpty` + `tcgetattr`/`tcsetattr`, which Miri cannot call
    // ("can't call foreign function `tcsetattr` on OS `macos`") and has nothing
    // to model — the subject IS the terminal driver's saved-termios state. This
    // is the class `.port/phase-2/MIRI_EXCEPTIONS.md` already argues under
    // "rustty-vaxis Chunk 3b — the `tty.rs` termios/ioctl/sigaction FFI"
    // (R6 verifier: ASan/native); these two lib.rs tests simply never carried
    // the annotation. Both run in ci.sh's HARD `cargo test --workspace` step.
    #[cfg_attr(
        miri,
        ignore = "real pty + termios FFI (tcsetattr) — see MIRI_EXCEPTIONS.md rustty-vaxis Chunk 3b"
    )]
    fn install_panic_recovery_runs_recover_then_chains_previous_hook() {
        let _guard = GLOBAL_TTY_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());

        let mut fds = [0 as c_int; 2];
        assert_eq!(0, unsafe { libc::pipe(fds.as_mut_ptr()) });
        let (r, w) = (fds[0], fds[1]);
        seed_global_tty(w, unsafe { std::mem::zeroed() });

        // stand-in for the default hook (also keeps the intentional panic quiet)
        let prev_ran = Arc::new(AtomicBool::new(false));
        let prev_ran_hook = Arc::clone(&prev_ran);
        std::panic::set_hook(Box::new(move |_| {
            prev_ran_hook.store(true, Ordering::SeqCst);
        }));
        install_panic_recovery();

        let caught = std::panic::catch_unwind(|| panic!("boom"));
        assert!(caught.is_err());

        // restore the default hook before asserting, so a failure below reports
        // normally and no later test inherits the chain
        drop(std::panic::take_hook());

        assert!(prev_ran.load(Ordering::SeqCst), "previous hook must chain");
        let expected = reset_bundle();
        assert_eq!(expected, read_exact_fd(r, expected.len()));

        clear_global_tty();
        unsafe {
            libc::close(r);
            #[cfg(target_os = "macos")]
            libc::close(w);
        }
    }
}
