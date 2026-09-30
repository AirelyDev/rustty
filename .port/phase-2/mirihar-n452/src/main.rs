//! N452: effect callbacks that re-enter the Terminal C API (P7-W2 T9).
//! Isolated, non-FFI model; see `README.md` for the candidates and results.
//! Each candidate is one executed path (Miri checks only what runs), selected
//! by argv[1].
//!
//! The shape being modelled (rustty-vt `Stream`/`stream_terminal::Handler`,
//! rustty-vt-c `TerminalWrapper`):
//!
//! - `vt_write(w)` runs `(*w).stream.next_slice(input)`. In the shipped code
//!   every `Stream` driver takes `&mut self` (a protected Unique over every
//!   stream byte, parser and continuation tracker included), and the handler
//!   method it reaches takes `&mut self` (a protected Unique over the handler
//!   bytes).
//! - The handler fires an effect: `(self.effects.bell)(self)`. The vt-c
//!   trampoline recovers the wrapper ADDRESS from the handler (the D3
//!   `offset_of!` seam, whole-allocation exposed provenance) and calls the C
//!   callback with the wrapper.
//! - The C callback re-enters the C API on the same terminal. The reference
//!   forbids only `vt_write`/`vt_write_until_ground` there, so `resize`,
//!   `set` of a handler field, `get` of `VT_GROUND` / `SEMANTIC_FAILURE`, and
//!   the continuation APIs are legal, and each reaches stream or handler bytes
//!   through the wrapper pointer while the outer protected `&mut` is live.
//!
//! The fix (the `*_fixed_*` candidates):
//!
//! 1. The stream drivers are raw-self (`this: *mut Stream`, the reference's
//!    `self: *Self`), with narrow place accesses; the pure handler dispatch
//!    (`execute`, `csi_dispatch`, ...) takes `handler: &mut H` only. No stream
//!    byte outside the handler is protected across a dispatch.
//! 2. Each vt-c trampoline publishes its handler pointer, derived once from its
//!    own `&mut Handler` argument (a child of every live handler protector), in
//!    the wrapper's `live_handler` slot for the duration of the C callback
//!    (saved and restored, so nested effects nest). A C API that reaches
//!    handler bytes goes through that pointer when it is set, and through the
//!    wrapper by place otherwise.
//! 3. The C API reads stream-level state (`ground`, the continuation tracker)
//!    by place, never through a whole-`Stream` `&self`/`&mut self` reborrow
//!    (that would retag the protected handler bytes too).

#![allow(clippy::missing_safety_doc)]

use std::ffi::c_void;
use std::mem::offset_of;
use std::ptr::{addr_of, addr_of_mut};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// ===========================================================================
// Modes (one per candidate)
// ===========================================================================

/// The stream drivers are raw-self (fix part 1).
static RAW_STREAM: AtomicBool = AtomicBool::new(false);
/// The trampolines publish `live_handler` and the C API uses it (fix part 2).
static STASH: AtomicBool = AtomicBool::new(false);
/// The C API reads stream-level state by place (fix part 3).
static PLACE_READS: AtomicBool = AtomicBool::new(false);

const OP_RESIZE: u32 = 1 << 0;
const OP_SET: u32 = 1 << 1;
const OP_GET_GROUND: u32 = 1 << 2;
const OP_GET_HANDLER: u32 = 1 << 3;
const OP_CONTINUATION: u32 = 1 << 4;
const OP_ALL: u32 = OP_RESIZE | OP_SET | OP_GET_GROUND | OP_GET_HANDLER | OP_CONTINUATION;

/// What the bell callback re-enters.
static OPS: AtomicU32 = AtomicU32::new(0);
/// How many C callbacks ran (bell + nested write_pty).
static CALLS: AtomicU32 = AtomicU32::new(0);

fn flag(f: &AtomicBool) -> bool {
    f.load(Ordering::Relaxed)
}

// ===========================================================================
// rustty-vt: Terminal, Parser, continuation Tracker
// ===========================================================================

struct Terminal {
    rows: Vec<u32>,
    cols: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Ground,
    Escape,
    CsiParam,
}

struct Parser {
    state: State,
    params: [u16; 4],
    params_idx: u8,
}

struct Tracker {
    bytes: Vec<u8>,
    max_bytes: usize,
    broken: bool,
}

// ===========================================================================
// rustty-vt: stream_terminal::Handler and its Effects
// ===========================================================================

#[derive(Default)]
struct HEffects {
    bell: Option<fn(&mut Handler)>,
    write_pty: Option<fn(&mut Handler, &[u8])>,
}

struct Handler {
    terminal: *mut Terminal,
    effects: HEffects,
    title_report: bool,
    semantic_failure: bool,
    unknown_max_bytes: usize,
    bells: u32,
}

impl Handler {
    /// `Handler::vt(.bell)` -> `do_bell`: fires the effect, then keeps using
    /// `self` (as real handler methods do after an effect returns).
    fn bell(&mut self) {
        if let Some(func) = self.effects.bell {
            func(self);
        }
        self.bells += 1;
        let _ = self.title_report;
    }

    /// `Handler::resize`: mutates the terminal through the raw alias, then
    /// reports the in-band size (mode 2048) through `write_pty`.
    fn resize(&mut self, cols: u16) {
        // SAFETY: the terminal is a separate live allocation; the borrow ends
        // within the statement.
        unsafe {
            (*self.terminal).cols = cols;
            (*self.terminal).rows.push(cols as u32);
        }
        if let Some(func) = self.effects.write_pty {
            func(self, b"\x1b[48;24;80t");
        }
        self.semantic_failure = false;
    }
}

// ===========================================================================
// rustty-vt: Stream
// ===========================================================================

struct Stream {
    handler: Handler,
    parser: Parser,
    utf8: u32,
    continuation: Option<Tracker>,
}

impl Stream {
    // ----- shipped: `&mut self` drivers --------------------------------------

    fn next_slice(&mut self, input: &[u8]) {
        for &c in input {
            self.next_non_utf8(c);
        }
        if self.continuation.is_some() {
            self.track_continuation(input);
        }
    }

    fn next_non_utf8(&mut self, c: u8) {
        match c {
            0x1B => self.parser.state = State::Escape,
            b'[' => {
                self.parser.state = State::CsiParam;
                self.parser.params_idx = 0;
            }
            b'0'..=b'9' => {
                let i = self.parser.params_idx as usize;
                self.parser.params[i] = self.parser.params[i] * 10 + u16::from(c - b'0');
            }
            0x07 => {
                // A dispatch mid-sequence: the parser is not at ground while
                // the effect runs.
                self.execute_mut(c);
                self.parser.params_idx = self.parser.params_idx.saturating_add(0);
            }
            _ => self.parser.state = State::Ground,
        }
    }

    fn execute_mut(&mut self, c: u8) {
        if c == 0x07 {
            self.handler.bell();
        }
    }

    /// `ground(&self)`: a whole-stream shared reborrow.
    fn ground(&self) -> bool {
        self.parser.state == State::Ground && self.utf8 == 0
    }

    fn track_continuation(&mut self, input: &[u8]) {
        let ground = self.parser.state == State::Ground && self.utf8 == 0;
        let t = self.continuation.as_mut().unwrap();
        if ground {
            t.bytes.clear();
        } else if t.bytes.len() + input.len() <= t.max_bytes {
            t.bytes.extend_from_slice(input);
        } else {
            t.broken = true;
        }
    }

    // ----- fixed: raw-self drivers, handler-only dispatch --------------------

    /// # Safety
    /// `this` is a live stream; no `&mut`/`&` to it is live in the caller.
    unsafe fn next_slice_raw(this: *mut Stream, input: &[u8]) {
        for &c in input {
            unsafe { Self::next_non_utf8_raw(this, c) };
        }
        if unsafe { (*this).continuation.is_some() } {
            unsafe { (*this).track_continuation(input) };
        }
    }

    unsafe fn next_non_utf8_raw(this: *mut Stream, c: u8) {
        unsafe {
            match c {
                0x1B => (*this).parser.state = State::Escape,
                b'[' => {
                    (*this).parser.state = State::CsiParam;
                    (*this).parser.params_idx = 0;
                }
                b'0'..=b'9' => {
                    let i = (*this).parser.params_idx as usize;
                    (*this).parser.params[i] =
                        (*this).parser.params[i] * 10 + u16::from(c - b'0');
                }
                0x07 => {
                    Self::execute(&mut (*this).handler, c);
                    (*this).parser.params_idx = (*this).parser.params_idx.saturating_add(0);
                }
                _ => (*this).parser.state = State::Ground,
            }
        }
    }

    fn execute(handler: &mut Handler, c: u8) {
        if c == 0x07 {
            handler.bell();
        }
    }

    /// `ground` by place (`self: *const Self`).
    unsafe fn ground_raw(this: *const Stream) -> bool {
        unsafe { (*this).parser.state == State::Ground && (*this).utf8 == 0 }
    }
}

// ===========================================================================
// rustty-vt-c: TerminalWrapper, the C effects, the trampolines
// ===========================================================================

type CBellFn = extern "C" fn(*mut Wrapper, *mut c_void);
type CWritePtyFn = extern "C" fn(*mut Wrapper, *mut c_void, *const u8, usize);

struct CEffects {
    userdata: *mut c_void,
    bell: Option<CBellFn>,
    write_pty: Option<CWritePtyFn>,
}

struct Wrapper {
    terminal: *mut Terminal,
    stream: Stream,
    effects: CEffects,
    /// Fix part 2: the handler pointer of the innermost running trampoline.
    live_handler: *mut Handler,
}

/// The D3 seam: integer math off the handler address, then the exposed
/// whole-wrapper provenance.
fn wrapper_from_handler(handler: *mut Handler) -> *mut Wrapper {
    let off = offset_of!(Wrapper, stream) + offset_of!(Stream, handler);
    let addr = (handler as *mut u8).addr() - off;
    std::ptr::with_exposed_provenance_mut::<Wrapper>(addr)
}

/// Runs `f` with `handler` published in `live_handler` (fix part 2), or
/// directly (shipped).
fn with_live_handler<R>(handler: *mut Handler, f: impl FnOnce(*mut Wrapper) -> R) -> R {
    let w = wrapper_from_handler(handler);
    if !flag(&STASH) {
        return f(w);
    }
    // SAFETY: `w` is the live wrapper; `live_handler` is a sibling of `stream`.
    let prev = unsafe { std::ptr::replace(addr_of_mut!((*w).live_handler), handler) };
    let r = f(w);
    unsafe { *addr_of_mut!((*w).live_handler) = prev };
    r
}

fn bell_trampoline(handler: &mut Handler) {
    // Derived ONCE from the argument; the argument is not used again.
    let handler: *mut Handler = handler;
    with_live_handler(handler, |w| {
        let Some(func) = (unsafe { (*w).effects.bell }) else {
            return;
        };
        let ud = unsafe { (*w).effects.userdata };
        func(w, ud);
    })
}

fn write_pty_trampoline(handler: &mut Handler, data: &[u8]) {
    let handler: *mut Handler = handler;
    with_live_handler(handler, |w| {
        let Some(func) = (unsafe { (*w).effects.write_pty }) else {
            return;
        };
        let ud = unsafe { (*w).effects.userdata };
        func(w, ud, data.as_ptr(), data.len());
    })
}

// ----- the C API ------------------------------------------------------------

/// The handler a C API call must reach.
unsafe fn handler_ptr(w: *mut Wrapper) -> *mut Handler {
    if flag(&STASH) {
        let live = unsafe { (*w).live_handler };
        if !live.is_null() {
            return live;
        }
    }
    unsafe { addr_of_mut!((*w).stream.handler) }
}

unsafe fn c_new() -> *mut Wrapper {
    let terminal = Box::into_raw(Box::new(Terminal {
        rows: vec![0; 4],
        cols: 80,
    }));
    let w = Box::into_raw(Box::new(Wrapper {
        terminal,
        stream: Stream {
            handler: Handler {
                terminal,
                effects: HEffects {
                    bell: Some(bell_trampoline),
                    write_pty: Some(write_pty_trampoline),
                },
                title_report: false,
                semantic_failure: false,
                unknown_max_bytes: 0,
                bells: 0,
            },
            parser: Parser {
                state: State::Ground,
                params: [0; 4],
                params_idx: 0,
            },
            utf8: 0,
            continuation: Some(Tracker {
                bytes: Vec::new(),
                max_bytes: 64,
                broken: false,
            }),
        },
        effects: CEffects {
            userdata: std::ptr::null_mut(),
            bell: Some(bell_cb),
            write_pty: Some(write_pty_cb),
        },
        live_handler: std::ptr::null_mut(),
    }));
    let _ = w.expose_provenance();
    w
}

unsafe fn c_free(w: *mut Wrapper) {
    unsafe {
        let b = Box::from_raw(w);
        drop(Box::from_raw(b.terminal));
    }
}

unsafe fn c_vt_write(w: *mut Wrapper, input: &[u8]) {
    if flag(&RAW_STREAM) {
        unsafe { Stream::next_slice_raw(addr_of_mut!((*w).stream), input) };
    } else {
        unsafe { (*w).stream.next_slice(input) };
    }
}

unsafe fn c_resize(w: *mut Wrapper, cols: u16) {
    if flag(&STASH) {
        unsafe { (*handler_ptr(w)).resize(cols) };
    } else {
        unsafe { (*w).stream.handler.resize(cols) };
    }
}

/// `set(.title_report)` and `set(.unknown_max_bytes)`.
unsafe fn c_set(w: *mut Wrapper, v: usize) {
    if flag(&STASH) {
        let h = unsafe { handler_ptr(w) };
        unsafe {
            (*h).title_report = v != 0;
            (*h).unknown_max_bytes = v;
        }
    } else {
        unsafe {
            (*w).stream.handler.title_report = v != 0;
            (*w).stream.handler.unknown_max_bytes = v;
        }
    }
}

/// `get(.vt_ground)`.
unsafe fn c_get_ground(w: *mut Wrapper) -> bool {
    if flag(&PLACE_READS) {
        unsafe { Stream::ground_raw(addr_of!((*w).stream)) }
    } else {
        unsafe { (*w).stream.ground() }
    }
}

/// `get(.vt_processing_error)` (`semantic_failure`).
unsafe fn c_get_semantic_failure(w: *mut Wrapper) -> bool {
    if flag(&STASH) {
        unsafe { (*handler_ptr(w)).semantic_failure }
    } else {
        unsafe { (*w).stream.handler.semantic_failure }
    }
}

/// `set(.continuation_max_bytes)` then `continuation_alloc`: a field-level
/// tracker access plus the `ground()` check `setContinuationMaxBytes` makes.
unsafe fn c_continuation(w: *mut Wrapper, max_bytes: usize) -> Vec<u8> {
    unsafe {
        if let Some(t) = (*w).stream.continuation.as_mut() {
            t.max_bytes = max_bytes;
            if t.bytes.len() > max_bytes {
                t.bytes.clear();
                t.broken = true;
            }
        }
        if !c_get_ground(w) {
            if let Some(t) = (*w).stream.continuation.as_mut() {
                t.broken = t.broken || max_bytes == 0;
            }
        }
        (*w).stream
            .continuation
            .as_ref()
            .map(|t| t.bytes.clone())
            .unwrap_or_default()
    }
}

// ----- the C callbacks --------------------------------------------------------

extern "C" fn bell_cb(w: *mut Wrapper, _ud: *mut c_void) {
    CALLS.fetch_add(1, Ordering::Relaxed);
    let ops = OPS.load(Ordering::Relaxed);
    unsafe {
        if ops & OP_SET != 0 {
            c_set(w, 7);
        }
        if ops & OP_GET_GROUND != 0 {
            // The first bell fires mid-sequence (csi_param), the second at
            // ground.
            let first = CALLS.load(Ordering::Relaxed) == 1;
            assert_eq!(!first, c_get_ground(w));
        }
        if ops & OP_GET_HANDLER != 0 {
            let _ = c_get_semantic_failure(w);
        }
        if ops & OP_CONTINUATION != 0 {
            let _ = c_continuation(w, 32);
        }
        if ops & OP_RESIZE != 0 {
            // Fires write_pty (nested effect) from inside the handler.
            c_resize(w, 100);
        }
    }
}

/// The nested effect (from `resize`) re-enters too.
extern "C" fn write_pty_cb(w: *mut Wrapper, _ud: *mut c_void, ptr: *const u8, len: usize) {
    CALLS.fetch_add(1, Ordering::Relaxed);
    let data = unsafe { std::slice::from_raw_parts(ptr, len) };
    assert_eq!(b'\x1b', data[0]);
    if OPS.load(Ordering::Relaxed) == OP_ALL {
        unsafe {
            c_set(w, 9);
            let _ = c_get_semantic_failure(w);
            let _ = c_get_ground(w);
        }
    }
}

// ===========================================================================
// Candidates
// ===========================================================================

fn run(raw_stream: bool, stash: bool, place_reads: bool, ops: u32) {
    RAW_STREAM.store(raw_stream, Ordering::Relaxed);
    STASH.store(stash, Ordering::Relaxed);
    PLACE_READS.store(place_reads, Ordering::Relaxed);
    OPS.store(ops, Ordering::Relaxed);
    unsafe {
        let w = c_new();
        // "ESC [ 1 2 BEL 3 m": the BEL executes while the parser sits in
        // csi_param, then the sequence continues.
        c_vt_write(w, b"\x1b[12\x073m");
        c_vt_write(w, b"\x07");
        // Outside any callback (live_handler is null): the by-place paths.
        c_set(w, 1);
        assert!(c_get_ground(w));
        let _ = c_continuation(w, 16);
        assert_eq!(2, (*w).stream.handler.bells);
        if ops & OP_SET != 0 {
            assert!((*w).stream.handler.title_report);
        }
        if ops & OP_RESIZE != 0 {
            assert_eq!(100, (*(*w).terminal).cols);
        }
        c_free(w);
    }
    let calls = CALLS.load(Ordering::Relaxed);
    let expect = if ops & OP_RESIZE != 0 { 4 } else { 2 };
    assert_eq!(expect, calls, "callbacks ran");
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    match arg.as_str() {
        // The shipped shape, one API class each: UB.
        "n452_shipped_resize" => run(false, false, false, OP_RESIZE),
        "n452_shipped_set" => run(false, false, false, OP_SET),
        "n452_shipped_get_ground" => run(false, false, false, OP_GET_GROUND),
        "n452_shipped_get_handler" => run(false, false, false, OP_GET_HANDLER),
        "n452_shipped_continuation" => run(false, false, false, OP_CONTINUATION),
        // Each fix part alone is insufficient.
        "n452_raw_stream_only_set" => run(true, false, true, OP_SET),
        "n452_stash_only_get_ground" => run(false, true, true, OP_GET_GROUND),
        "n452_raw_stream_whole_ground" => run(true, true, false, OP_GET_GROUND),
        // The fix, one API class each, then all at once (nested effect).
        "n452_fixed_resize" => run(true, true, true, OP_RESIZE),
        "n452_fixed_set" => run(true, true, true, OP_SET),
        "n452_fixed_get_ground" => run(true, true, true, OP_GET_GROUND),
        "n452_fixed_get_handler" => run(true, true, true, OP_GET_HANDLER),
        "n452_fixed_continuation" => run(true, true, true, OP_CONTINUATION),
        "n452_fixed_all" => run(true, true, true, OP_ALL),
        other => panic!("unknown candidate {other:?}"),
    }
}
