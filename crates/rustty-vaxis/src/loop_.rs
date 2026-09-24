//! Port of `libvaxis` `Loop.zig` (libvaxis 0.6.0: 450 LOC, 1 `test{}`) — the read thread that reads the
//! tty, feeds the Chunk-3a `Parser`, routes through `handle_event_generic`, and pushes
//! parsed events to the [`Queue`]. (`loop` is a Rust keyword, so the module is `loop_`;
//! `lib.rs` re-exports `Loop` so `vaxis::Loop` matches Zig's `vaxis.Loop`.)
//!
//! # The aliasing model (Chunk 3b §4 — the CORE ASK)
//!
//! The read thread (`tty_run`) captures the Loop and holds it for its whole life while
//! the main thread concurrently drives `stop`/`next_event`. This is the construct-then-
//! spawn + userdata-callback-reentry shape PROVEN by the `renderthread_*`/`loopthread_*`
//! pairs in `.port/phase-2/mirihar-chunk3b`. The faithful model is **raw-self
//! `*mut Loop<T>`, pinned** (NOT an `Arc` split — Zig stores the WHOLE Loop as a
//! type-erased `*anyopaque` SIGWINCH context and recovers it in `winsize_callback`, a
//! userdata round-trip an `Arc` cannot model). Discipline that keeps it SOUND under
//! SB+TB:
//! 1. The read thread holds `self_ptr: *mut Loop<T>` and does ONLY narrow field
//!    projections (`(*sp).should_quit` atomic, `(*sp).tty`/`(*sp).vaxis` raw,
//!    `(*sp).queue` shared push). It NEVER binds a whole `&mut *self_ptr`.
//! 2. The winsize callback fires with `context = self_ptr` (type-erased), casts back,
//!    and likewise projects narrow fields only.
//! 3. The main thread post-spawn NEVER takes a whole `&mut Loop` while the thread is
//!    live: `next_event`/… take `&self` through the queue's interior `Mutex`; `stop`
//!    stores `should_quit` (`AtomicBool`, `&self`) then joins; `thread` is touched by
//!    main ONLY, via the interior-mutable [`Cell`].
//!    3b. Nor a whole `&mut Tty`/`&Vaxis` (H1): `stop`'s DSR wake fires while the read
//!    thread is BLOCKED inside `Tty::read` (a raw-receiver fn holding no whole-Tty
//!    borrow), so it projects `tty_writer` raw (`Tty::writer_raw`) and calls the
//!    self-less `Vaxis::device_status_report` (upstream's receiver is `_: Vaxis`,
//!    unused) — proven by the mirihar `stopread_*` quartet.
//! 4. `queue` is the internally-synchronized channel (both threads hold only `&Queue`).
//!
//! # The queued event OWNS its grapheme text (RESOLVED BLOCKER — §4.5/§6)
//!
//! Zig `handleEventGeneric` does `mut_key.text = cache.put(text)` into a read-thread-
//! local 8 KiB ring that WRAPS with no synchronization — the queued `Key.text` would
//! borrow that ring while the main thread pops and the read thread overwrites it (an
//! unsynchronized cross-thread data race, and a lifetime unnameable in safe Rust). The
//! faithful-AND-sound port does NOT reproduce it: `handle_event_generic` promotes the
//! transient `Key.text` to an OWNED `Vec<u8>` via [`Key::into_owned`] (a [`KeyOwned`])
//! BEFORE posting — the same owned-promotion already applied to `Paste`. Nothing crosses
//! the queue by reference; `tty_run` constructs NO `GraphemeCache`. The `LoopEvent`
//! supertrait bound `Send + 'static` statically forbids a `T` that borrows the read
//! thread.

use crate::cell;
use crate::event::Event;
use crate::gwidth::Method;
use crate::key::KeyOwned;
use crate::mouse::Mouse;
use crate::parser::Parser;
use crate::queue::Queue;
use crate::tty::{Tty, TtyError};
use crate::vaxis::Vaxis;
use crate::Winsize;
#[cfg(not(windows))]
use core::ffi::c_void;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

#[cfg(all(not(windows), not(test)))]
use crate::tty::SignalHandler;

/// The faithful Rust image of Zig's `@hasField(Event, "…")` comptime field-presence
/// reflection. Zig `Loop(comptime T)` duck-types the user's event union: a variant is
/// posted ONLY if `T` declares it. Rust has no `@hasField`, so each event has an
/// `Option<Self>`-returning constructor — `Some` ≡ `@hasField` true (post it), `None` ≡
/// the field-absent skip (the default). A user's `T` overrides only the constructors for
/// the variants it has. Key constructors take an OWNED [`KeyOwned`] (§6): a borrowed
/// `Key<'a>` in a variant would fail the `'static` bound (the resolved blocker).
pub trait LoopEvent: Sized + Send + 'static {
    fn key_press(_k: KeyOwned) -> Option<Self> {
        None
    }
    fn key_release(_k: KeyOwned) -> Option<Self> {
        None
    }
    fn mouse(_m: Mouse) -> Option<Self> {
        None
    }
    fn winsize(_ws: Winsize) -> Option<Self> {
        None
    }
    fn focus_in() -> Option<Self> {
        None
    }
    fn focus_out() -> Option<Self> {
        None
    }
    fn mouse_leave() -> Option<Self> {
        None
    }
    fn paste_start() -> Option<Self> {
        None
    }
    fn paste_end() -> Option<Self> {
        None
    }
    fn paste(_b: Vec<u8>) -> Option<Self> {
        None
    }
    fn color_report(_r: cell::Report) -> Option<Self> {
        None
    }
    fn color_scheme(_s: cell::Scheme) -> Option<Self> {
        None
    }
    // (no cap_* constructors: caps set vx.caps, never postEvent'd)
}

pub struct Loop<'a, T> {
    /// Zig `tty: *Tty`. Raw so the read thread can project it narrowly.
    pub tty: *mut Tty,
    /// Zig `vaxis: *Vaxis`. `Vaxis<'a>` gained a lifetime (it owns `Screen<'a>`), so the
    /// raw pointer threads `'a`; the Loop router never names the borrowed graphemes, so the
    /// threading is a pure annotation exercise (VAXIS_C4A_DESIGN §3).
    pub vaxis: *mut Vaxis<'a>,
    /// internally-synced (§2) — both threads hold only `&Queue`.
    queue: Queue<T, 512>,
    /// Zig `thread: ?std.Thread`. A `Cell` (interior-mutable through `&self`): `start`/
    /// `stop` mutate it while the read thread may hold `*mut Loop`, so writing a plain
    /// field through a `*mut` derived from `&self` would be UB under SB/TB. `Cell`
    /// (an `UnsafeCell`) is the sanctioned channel; it is MAIN-THREAD-ONLY.
    thread: Cell<Option<JoinHandle<()>>>,
    /// Zig `should_quit: bool` — an `AtomicBool` (Zig's plain bool is a latent
    /// cross-thread race; `store`/`load` `Relaxed` makes it explicit and sound while
    /// keeping `stop` a pure `&self` op).
    ///
    /// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD108: the
    /// reference's plain `bool` is written by `stop` on the caller's thread and
    /// read by the read thread's loop condition with no synchronization.
    should_quit: AtomicBool,
}

/// A `Send` newtype carrying the raw `*mut Loop<T>` into the read thread (§4.2). Sound:
/// the pointee is pinned for the thread's life and only ever touched via narrow field
/// projections on both sides.
// PORT NOTE (`'a` erasure across the thread boundary): `std::thread::spawn` requires a
// `'static` closure, so `SendLoop` carries a LIFETIME-ERASED `*mut Loop<'static, T>` (cast
// in `start`). This is sound: the read thread only projects lifetime-agnostic fields
// (`tty`/`vaxis` caps/state/`screen.width_method`/`queries_done`/`query_futex`) and NEVER
// dereferences the `'a`-borrowed cell graphemes; the pointee (a `Loop<'a, T>`) outlives the
// thread (pinned by the owner), so reading those `'a`-free fields through the erased pointer
// is valid.
struct SendLoop<T>(*mut Loop<'static, T>);
// SAFETY: the raw self-pointer is only projected narrowly (see the module aliasing
// discipline); the pointee outlives the thread (pinned by the owner).
unsafe impl<T> Send for SendLoop<T> {}

impl<'a, T: LoopEvent> Loop<'a, T> {
    /// `pub fn init(io: std.Io, tty: *Tty, vx: *Vaxis) Self` (libvaxis 0.6.0; 0.5.1 had no
    /// constructor and its `init` installed the resize handler, now
    /// [`Self::install_resize_handler`]). The caller passes raw pointers to a PINNED
    /// `tty`/`vaxis` that outlive the Loop. The Loop itself must then be pinned (Box/stack
    /// held for the read thread's life) before `install_resize_handler`/`start` — the
    /// intrusive requirement below. `io` is dropped (the port's queue and thread are
    /// `std::sync` / `std::thread`, see [`crate::queue`]).
    pub fn init(tty: *mut Tty, vaxis: *mut Vaxis<'a>) -> Self {
        Loop {
            tty,
            vaxis,
            queue: Queue::default(),
            thread: Cell::new(None),
            should_quit: AtomicBool::new(false),
        }
    }

    /// `installResizeHandler` (libvaxis 0.6.0; the body of 0.5.1's `init`, whose doc
    /// read: "Initialize the event loop. This is an intrusive init so that we have a
    /// stable pointer to register signal callbacks with posix TTYs").
    #[cfg_attr(any(windows, test), allow(clippy::unnecessary_wraps))]
    pub fn install_resize_handler(&self) -> Result<(), TtyError> {
        // windows/test → no-op (windows: seam; test: `ttyRun` returns early anyway).
        #[cfg(all(not(windows), not(test)))]
        {
            let handler = SignalHandler {
                context: self as *const Self as *mut c_void,
                callback: Self::winsize_callback,
            };
            Tty::notify_winsize(handler)?;
        }
        Ok(())
    }

    /// spawns the input thread to read input from the tty
    pub fn start(&self) -> std::io::Result<()> {
        // if self.thread |_| return  (peek the Cell for Some, restoring it)
        let existing = self.thread.take();
        let already = existing.is_some();
        self.thread.set(existing);
        if already {
            return Ok(());
        }
        let self_ptr: *mut Loop<'a, T> = self as *const Self as *mut Self;
        // Erase `'a` for the thread boundary (see the SendLoop PORT NOTE). clippy sees the
        // cast as a no-op because it ignores lifetimes, but it is load-bearing: raw pointers
        // are invariant, so `*mut Loop<'a, T>` does NOT coerce to `*mut Loop<'static, T>`.
        #[allow(clippy::unnecessary_cast)]
        let send = SendLoop(self_ptr as *mut Loop<'static, T>);
        // PORT NOTE: Zig passes `self.vaxis.opts.system_clipboard_allocator` as the paste
        // allocator; dropped here (paste owns its `Vec`, §5/3a precedent).
        let handle = std::thread::Builder::new().spawn(move || {
            let s = send;
            Self::tty_run(s);
        })?;
        self.thread.set(Some(handle));
        Ok(())
    }

    /// stops reading from the tty.
    pub fn stop(&self) {
        // If we don't have a thread, we have nothing to stop
        let handle = self.thread.take();
        let Some(handle) = handle else {
            return;
        };
        self.should_quit.store(true, Ordering::Relaxed);
        // trigger a read (Zig `self.vaxis.deviceStatusReport(self.tty.writer()) catch {}`).
        // The read thread is BY DESIGN blocked inside `Tty::read` right now — waking it
        // is what the DSR is for — so NO whole `&mut Tty` (nor `&Vaxis`) may be formed
        // here: `(*tty).writer()` would be a whole-Tty `Unique` retag popping the
        // reader's call-protected tag (SB/TB protector UB, VALIDATION_REPORT H1). The
        // writer is a narrow raw field projection (`Tty::writer_raw`) and the DSR is
        // self-less (upstream's receiver is `_: Vaxis`, unused) — proven by the
        // mirihar-chunk3b `stopread_{naive,projonly,dsrref,shipped}` quartet. Errors
        // ignored (Zig `catch {}`).
        let self_ptr: *mut Loop<'a, T> = self as *const Self as *mut Self;
        unsafe {
            let tty = (*self_ptr).tty;
            let w = Tty::writer_raw(tty);
            let _ = Vaxis::device_status_report(&mut *w);
        }
        // join, leave the Cell None, reset should_quit
        handle.join().ok();
        self.should_quit.store(false, Ordering::Relaxed);
    }

    /// returns the next available event, blocking until one is available
    ///
    /// PORT NOTE (libvaxis 0.6.0): `nextEvent`, `pollEvent`, `tryEvent`, `postEvent` and
    /// `tryPostEvent` now return `!T` / `!void`: the only error is the queue's
    /// `error.Canceled` (an `std.Io` task cancelation), which the port's `std::sync`
    /// queue cannot produce (see [`crate::queue`]), so they stay infallible here.
    pub fn next_event(&self) -> T {
        self.queue.pop()
    }

    /// blocks until an event is available. Useful when your application is
    /// operating on a poll + drain architecture (see try_event)
    pub fn poll_event(&self) {
        self.queue.poll();
    }

    /// returns an event if one is available, otherwise null. Non-blocking.
    pub fn try_event(&self) -> Option<T> {
        self.queue.try_pop()
    }

    /// posts an event into the event queue. Will block if there is not
    /// capacity for the event
    pub fn post_event(&self, event: T) {
        self.queue.push(event);
    }

    pub fn try_post_event(&self, event: T) -> bool {
        self.queue.try_push(event)
    }

    /// The SIGWINCH winsize callback. Fires with the type-erased `*mut Loop<T>` context;
    /// casts back and projects narrow fields only (§4.2 rule 2). `extern "C"` so it can
    /// adapt the signal-handler ABI.
    ///
    /// PORT NOTE (windows): upstream's `Loop.init` has `.windows => {}` — no signal
    /// handler is ever registered, and `winsizeCallback` (which reads `self.tty.fd`,
    /// a field `WindowsTty` does not have) is never semantically analyzed. Rust
    /// analyzes every item, so the callback is `cfg`-gated off exactly where upstream
    /// elides it.
    #[cfg(not(windows))]
    pub extern "C" fn winsize_callback(ptr: *mut c_void) {
        let self_ptr = ptr as *mut Loop<'a, T>;
        unsafe {
            // We will be receiving winsize updates in-band
            if (*(*self_ptr).vaxis).state.in_band_resize {
                return;
            }
            let winsize = match (*(*self_ptr).tty).get_winsize() {
                Ok(w) => w,
                Err(_) => return,
            };
            if let Some(ev) = T::winsize(winsize) {
                (*self_ptr).queue.push(ev);
            }
        }
    }

    /// read input from the tty. This is run in a separate thread
    fn tty_run(loop_ptr: SendLoop<T>) {
        // Return early if we're in test mode to avoid infinite loops
        if cfg!(test) {
            return;
        }
        // (No `GraphemeCache` here — the queued event OWNS its text, §4.5.)
        let sp = loop_ptr.0;
        #[cfg(not(windows))]
        unsafe {
            // get our initial winsize
            let winsize = match (*(*sp).tty).get_winsize() {
                Ok(w) => w,
                Err(_) => return,
            };
            if let Some(ev) = T::winsize(winsize) {
                (*sp).queue.push(ev);
            }

            let mut parser = Parser::default();

            // initialize the read buffer
            let mut buf = [0u8; 1024];
            let mut read_start: usize = 0;
            // read loop
            'read_loop: while !(*sp).should_quit.load(Ordering::Relaxed) {
                // Raw-receiver read (H1): the thread BLOCKS here while `stop()` writes
                // the DSR wake into `tty_writer`; `Tty::read(this: *const _)` holds no
                // whole-Tty borrow across the blocked syscall (see tty.rs).
                let n = match Tty::read((*sp).tty, &mut buf[read_start..]) {
                    Ok(n) => n,
                    Err(_) => return,
                };
                let mut seq_start: usize = 0;
                while seq_start < n {
                    let result = match parser.parse(&buf[seq_start..n]) {
                        Ok(r) => r,
                        Err(_) => return,
                    };
                    if result.n == 0 {
                        // release the buf/parser borrow before the overlapping copy
                        drop(result);
                        // copy the read to the beginning. We don't use memcpy because
                        // this could be overlapping, and it's also rare
                        let initial_start = seq_start;
                        while seq_start < n {
                            buf[seq_start - initial_start] = buf[seq_start];
                            seq_start += 1;
                        }
                        read_start = seq_start - initial_start + 1;
                        continue 'read_loop;
                    }
                    read_start = 0;
                    seq_start += result.n;

                    match result.event {
                        None => continue,
                        Some(event) => handle_event_generic(sp, (*sp).vaxis, event),
                    }
                }
            }
        }
        // The windows read path. Upstream (`Loop.zig:116-122`) is a much simpler pump:
        // the console driver hands us decoded `INPUT_RECORD`s, so there is no read
        // buffer, no partial-sequence carry, and no initial `getWinsize` post — the
        // first `WINDOW_BUFFER_SIZE_EVENT` supplies the size.
        #[cfg(windows)]
        unsafe {
            let mut parser = Parser::default();
            let parser_ptr: *mut Parser = std::ptr::addr_of_mut!(parser);
            while !(*sp).should_quit.load(Ordering::Relaxed) {
                // Raw-receiver next_event (H1): the thread BLOCKS inside
                // `ReadConsoleInputW` while `stop()` projects `tty_writer` raw, so no
                // whole-`&mut Tty` may be formed here (see tty.rs).
                let event = match Tty::next_event((*sp).tty, parser_ptr) {
                    Ok(e) => e,
                    Err(_) => return,
                };
                handle_event_generic(sp, (*sp).vaxis, event);
            }
        }
    }
}

// PORT NOTE (rustty-only, VALIDATION_REPORT §3c.5): upstream libvaxis's Loop has
// no deinit and NEVER deregisters the SIGWINCH handler `init` registered — the
// process-global `handlers` array keeps the dead Loop's `*anyopaque` context and
// the next SIGWINCH dereferences it (a latent upstream use-after-free; UB is not
// behavior R1 preserves). Rust's Drop is the natural end-of-life hook Zig lacks:
// remove this Loop's registration (keyed on the context pointer — the SAME
// address `init` registered, valid because the Loop is PINNED per the `new`
// contract) before the pointee dies. HAZARD if that contract is violated: a
// Loop moved between `init` and drop deregisters its DROP-time address and
// leaves the stale INIT-time entry in `HANDLERS` — no worse than upstream
// (which never removes it, and the moved Loop's registered ctx is already
// dangling for the live handler anyway), but not fixed by this Drop either;
// only the pinning contract makes the handler sound while the Loop is alive.
// Proven on the isolated Miri harness
// (.port/phase-2/mirihar-chunk3b): `winch_naive` (no dereg) = UAF UB under both
// SB and TB; `winch_shipped` (dereg-on-drop) = CLEAN. The cfg mirrors `init`'s:
// on windows/test nothing was registered, so nothing is removed.
impl<'a, T> Drop for Loop<'a, T> {
    fn drop(&mut self) {
        #[cfg(all(not(windows), not(test)))]
        Tty::deregister_winsize(self as *const Self as *mut c_void);
    }
}

/// The event router (Zig top-level `handleEventGeneric`). Ported LIVE against the
/// [`Vaxis`] placeholder (§5): the capability-flag WRITES + the F3 width/scaled-text
/// detection are live; only `translate_mouse`/`device_status_report`/the futex wake are
/// `unimplemented!()` leaves. `self_ptr`/`vx` are raw so the routing does narrow
/// projections only (§4.2). The queued `T` OWNS its text via [`Key::into_owned`].
///
/// PORT NOTE (the two `builtin.os.tag` arms): Zig has a `.windows` switch-arm and an
/// `else` arm. Both are ported, each behind the `cfg` that models its `builtin.os.tag`:
/// this is the `else` (POSIX) arm; [the windows twin](handle_event_generic) follows.
/// The two are NOT interchangeable — the windows arm's `winsize` skips the
/// in-band-resize latch and `resetSignalHandler`, and it has no `mouse_leave`/`paste`/
/// color/`cap_*` (other than `cap_da1`) handling, all of which fall into its `else =>
/// {}`.
///
/// # Safety
/// `self_ptr`/`vx` must be valid, and the caller must hold no conflicting whole-`&mut`.
///
// Use return on the self.post_event's so it can either return error union or void
#[cfg(not(windows))]
pub unsafe fn handle_event_generic<'a, T: LoopEvent>(
    self_ptr: *mut Loop<'a, T>,
    vx: *mut Vaxis<'a>,
    event: Event<'_>,
) {
    unsafe {
        match event {
            Event::KeyPress(key) => {
                // Check for a cursor position response for our explicit width query. This
                // will always be an F3 key with shift = true, and we must be looking for
                // queries
                if key.codepoint == crate::key::f3
                    && key.mods.shift
                    && !(*vx).queries_done.load(Ordering::Relaxed)
                {
                    // explicit width capability detected
                    (*vx).caps.explicit_width = true;
                    (*vx).caps.unicode = Method::Unicode;
                    (*vx).screen.width_method = Method::Unicode;
                    return;
                }
                // Check for a cursor position response for our scaled text query. This
                // will always be an F3 key with alt = true, and we must be looking for
                // queries
                if key.codepoint == crate::key::f3
                    && key.mods.alt
                    && !(*vx).queries_done.load(Ordering::Relaxed)
                {
                    // scaled text capability detected
                    (*vx).caps.scaled_text = true;
                    return;
                }
                // HACK: yuck. there has to be a better way (Rust's better way: OWN the bytes)
                // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD107: the
                // reference queues a slice of the read thread's 8 KiB grapheme ring,
                // which the read thread overwrites when it wraps (and which dies
                // with the thread); the event owns a copy here (module note).
                if let Some(ev) = T::key_press(key.into_owned()) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::KeyRelease(key) => {
                // HACK: yuck. there has to be a better way
                if let Some(ev) = T::key_release(key.into_owned()) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::Mouse(mouse) => {
                if let Some(ev) = T::mouse((*vx).translate_mouse(mouse)) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::MouseLeave => {
                if let Some(ev) = T::mouse_leave() {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::FocusIn => {
                if let Some(ev) = T::focus_in() {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::FocusOut => {
                if let Some(ev) = T::focus_out() {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::PasteStart => {
                if let Some(ev) = T::paste_start() {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::PasteEnd => {
                if let Some(ev) = T::paste_end() {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::Paste(text) => {
                // T::paste(text) returning None drops the Vec — Rust's owned-drop replaces
                // Zig's `paste_allocator.?.free(text)` (no explicit free).
                if let Some(ev) = T::paste(text) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::ColorReport(report) => {
                if let Some(ev) = T::color_report(report) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::ColorScheme(scheme) => {
                if let Some(ev) = T::color_scheme(scheme) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::CapKittyKeyboard => {
                // kitty keyboard capability detected
                (*vx).caps.kitty_keyboard = true;
            }
            Event::CapKittyGraphics => {
                if !(*vx).caps.kitty_graphics {
                    // kitty graphics capability detected
                    (*vx).caps.kitty_graphics = true;
                }
            }
            Event::CapRgb => {
                // rgb capability detected
                (*vx).caps.rgb = true;
            }
            Event::CapSgrPixels => {
                // pixel mouse capability detected
                (*vx).caps.sgr_pixels = true;
            }
            Event::CapUnicode => {
                // unicode capability detected
                (*vx).caps.unicode = Method::Unicode;
                (*vx).screen.width_method = Method::Unicode;
            }
            Event::CapColorSchemeUpdates => {
                // color_scheme_updates capability detected
                (*vx).caps.color_scheme_updates = true;
            }
            Event::CapMultiCursor => {
                // multi cursor capability detected
                (*vx).caps.multi_cursor = true;
            }
            Event::CapDa1 => {
                (*vx).wake_query_futex(); // std.Thread.Futex.wake(&vx.query_futex, 10)
                (*vx).queries_done.store(true, Ordering::Relaxed);
            }
            Event::Winsize(winsize) => {
                (*vx).state.in_band_resize = true;
                // Reset the signal handler if we are receiving in_band_resize
                Tty::reset_signal_handler();
                if let Some(ev) = T::winsize(winsize) {
                    (*self_ptr).queue.push(ev);
                }
            }
        }
    }
}

/// The `.windows` arm of Zig's `handleEventGeneric` (`Loop.zig:167-234`).
///
/// It is deliberately NOT the POSIX arm: `winsize` posts the size without latching
/// `vx.state.in_band_resize` (there is no SIGWINCH to reset), and everything other than
/// `winsize`/`key_press`/`key_release`/`cap_da1`/`mouse`/`focus_in`/`focus_out` falls
/// into upstream's `else => {}` — including `mouse_leave`, `paste*`, the color reports,
/// and every `cap_*` but `cap_da1`. Those CAN arrive here (the console escape-sequence
/// path runs the same `Parser`), and upstream drops them; R1 keeps that.
///
/// # Safety
/// `self_ptr`/`vx` must be valid, and the caller must hold no conflicting whole-`&mut`.
///
// Use return on the self.post_event's so it can either return error union or void
#[cfg(windows)]
pub unsafe fn handle_event_generic<'a, T: LoopEvent>(
    self_ptr: *mut Loop<'a, T>,
    vx: *mut Vaxis<'a>,
    event: Event<'_>,
) {
    unsafe {
        match event {
            Event::Winsize(ws) => {
                if let Some(ev) = T::winsize(ws) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::KeyPress(key) => {
                // Check for a cursor position response for our explicit width query.
                // This will always be an F3 key with shift = true, and we must be
                // looking for queries
                if key.codepoint == crate::key::f3
                    && key.mods.shift
                    && !(*vx).queries_done.load(Ordering::Relaxed)
                {
                    // explicit width capability detected
                    (*vx).caps.explicit_width = true;
                    (*vx).caps.unicode = Method::Unicode;
                    (*vx).screen.width_method = Method::Unicode;
                    return;
                }
                // Check for a cursor position response for our scaled text query. This
                // will always be an F3 key with alt = true, and we must be looking for
                // queries
                if key.codepoint == crate::key::f3
                    && key.mods.alt
                    && !(*vx).queries_done.load(Ordering::Relaxed)
                {
                    // scaled text capability detected
                    (*vx).caps.scaled_text = true;
                    return;
                }
                // HACK: yuck. there has to be a better way (Rust's better way: OWN the bytes)
                if let Some(ev) = T::key_press(key.into_owned()) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::KeyRelease(key) => {
                // HACK: yuck. there has to be a better way
                if let Some(ev) = T::key_release(key.into_owned()) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::CapDa1 => {
                (*vx).wake_query_futex(); // std.Thread.Futex.wake(&vx.query_futex, 10)
                (*vx).queries_done.store(true, Ordering::Relaxed);
            }
            Event::Mouse(mouse) => {
                if let Some(ev) = T::mouse((*vx).translate_mouse(mouse)) {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::FocusIn => {
                if let Some(ev) = T::focus_in() {
                    (*self_ptr).queue.push(ev);
                }
            }
            Event::FocusOut => {
                if let Some(ev) = T::focus_out() {
                    (*self_ptr).queue.push(ev);
                }
            }
            // Unsupported currently
            _ => {}
        }
    }
}

// The one Zig `test Loop`. Now that the Vaxis chunk (C4a) FILLED the placeholder leaves
// (`device_status_report` is real), the init/start/stop lifecycle runs to completion, so it
// is UN-IGNORED. `tty_run` returns early in test mode, so it asserts the init/start/stop
// lifecycle compiles + runs against `TestTty`. It counts toward `check_test_counts`.
#[cfg(test)]
mod tests {
    use super::*;

    // The test's event type: `{ key_press, winsize, focus_in, foo }` — impls only those
    // three `LoopEvent` constructors (the rest default to `None` ≡ `@hasField` false).
    // (Payloads are never read: the test only drives the init/start/stop lifecycle.)
    #[derive(Debug)]
    #[allow(dead_code)]
    enum TestEvent {
        KeyPress(KeyOwned),
        Winsize(Winsize),
        FocusIn,
        Foo(u8),
    }

    impl LoopEvent for TestEvent {
        fn key_press(k: KeyOwned) -> Option<Self> {
            Some(TestEvent::KeyPress(k))
        }
        fn winsize(ws: Winsize) -> Option<Self> {
            Some(TestEvent::Winsize(ws))
        }
        fn focus_in() -> Option<Self> {
            Some(TestEvent::FocusIn)
        }
    }

    /// UD107 (R13, task R13-D): libvaxis 0.6.0 queues each key event's text
    /// as a slice of the read thread's 8 KiB `GraphemeCache` ring, so the text
    /// of an event still in the queue is overwritten once the read thread has
    /// put another 8 KiB through the ring. Here more than 8 KiB of key text is
    /// posted through `handle_event_generic` from ONE buffer that is rewritten
    /// after every post, as the ring would be; every queued event must still
    /// hold its own text. (The reference's borrow cannot be reinstated in safe
    /// Rust: `LoopEvent: Send + 'static` rejects an event that borrows the
    /// read thread.)
    #[test]
    fn queued_key_text_is_owned_not_a_view_of_the_read_buffer() {
        let mut tty = Tty::init(Vec::new()).unwrap();
        let mut vx = Vaxis::default();
        let mut l: Loop<'_, TestEvent> = Loop::init(&mut tty, &mut vx);

        const EVENTS: usize = 100;
        const LEN: usize = 100; // 100 x 100 bytes > 8 KiB
        let mut ring = [0u8; LEN];
        for i in 0..EVENTS {
            ring.fill(b'a' + (i % 26) as u8);
            let key = crate::key::Key {
                codepoint: u32::from(ring[0]),
                text: Some(&ring[..]),
                shifted_codepoint: None,
                base_layout_codepoint: None,
                mods: Default::default(),
            };
            // SAFETY: `l` and `vx` are live locals; no other borrow is held.
            unsafe { handle_event_generic(&mut l, &mut vx, Event::KeyPress(key)) };
        }
        ring.fill(0);

        for i in 0..EVENTS {
            match l.try_event() {
                Some(TestEvent::KeyPress(k)) => assert_eq!(
                    Some(vec![b'a' + (i % 26) as u8; LEN]),
                    k.text,
                    "event {i} kept its own text"
                ),
                other => panic!("event {i}: expected a key press, got {other:?}"),
            }
        }
        assert!(l.try_event().is_none());
        drop(l);
        tty.deinit();
    }

    /// libvaxis 0.6.0 `test Loop` (the Loop is constructed by `init` and no resize
    /// handler is installed; the query waits `.fromSeconds(1)`, 0.5.1 waited 1 ms).
    #[test]
    fn loop_lifecycle() {
        let mut tty = Tty::init(Vec::new()).unwrap();
        let mut vx = Vaxis::default();
        let tty_ptr: *mut Tty = &raw mut tty;
        let vx_ptr: *mut Vaxis<'_> = &raw mut vx;

        let l: Loop<'_, TestEvent> = Loop::init(tty_ptr, vx_ptr);

        l.start().unwrap();

        // Optionally enter the alternate screen
        // SAFETY: the reborrows go through the same pointers the Loop holds; under
        // cfg(test) the read thread returns at once (`if (builtin.is_test) return;`).
        unsafe {
            (*vx_ptr).enter_alt_screen((*tty_ptr).writer()).unwrap();
            (*vx_ptr)
                .query_terminal((*tty_ptr).writer(), std::time::Duration::from_secs(1))
                .unwrap();
        }
        // defer loop.stop();
        l.stop();
        // defer vx.deinit(std.testing.allocator, tty.writer());
        unsafe { (*vx_ptr).deinit((*tty_ptr).writer()) };
        drop(l);
        // defer tty.deinit();
        tty.deinit();
    }
}
