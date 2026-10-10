//! P8-W5 T3 fix 1 (R2 §2.9; LIFETIMES rows `Effects unknown_sequence (the .osc
//! arm)` and `rustty_osc_set`). A std-only model of the seam:
//!
//! - `Wrapper` (vt-c `TerminalWrapper`) owns `Stream { handler, parser }`, and
//!   `parser.osc_parser` keeps the sequence in an INLINE buffer
//!   (`osc::Parser.buffer: [u8; MAX_BUF]`).
//! - `vt_write` drives the stream through a raw-self pointer
//!   (`Stream::next_slice_raw(addr_of_mut!((*w).stream))`, N452).
//! - The stream ends the OSC with a temporary `&mut osc::Parser` (`end`), which
//!   returns content borrowed from the inline buffer, and dispatches it with a
//!   disjoint `&mut handler` to the trampoline.
//! - The trampoline recovers the wrapper from the handler address and calls the
//!   C callback with a C struct `{ ptr, len }` over the borrowed content.
//! - The callback calls `rustty_terminal_set(UNKNOWN_MAX_BYTES)` on the wrapper
//!   and THEN reads the content.
//!
//! Candidates (the `set` arm's write of the OSC limit, times how the callback's
//! wrapper pointer is derived):
//!
//! - `mut_parser_strict`: the write forms `&mut osc::Parser` (what R2 §2.9
//!   forbids); the wrapper pointer keeps the handle's own provenance.
//! - `raw_place_strict`: the shipped raw place write, same provenance.
//! - `mut_parser_exposed` / `raw_place_exposed`: the same two writes with the
//!   wrapper pointer recovered by exposed provenance, as vt-c's
//!   `wrapper_from_handler` does (`with_exposed_provenance_mut`).
use std::ptr::addr_of_mut;

const MAX_BUF: usize = 16;

struct OscParser {
    buffer: [u8; MAX_BUF],
    len: usize,
    unknown_max_bytes: usize,
}

impl OscParser {
    /// `osc::Parser::end`: the command borrows the inline buffer.
    fn end(&mut self) -> &[u8] {
        let n = self.len.min(self.unknown_max_bytes);
        &self.buffer[..n]
    }
}

struct Parser {
    osc_parser: OscParser,
    state: u8,
}

/// C: the `{ ptr, len }` the callback receives (`RusttyString` in
/// `RusttyTerminalUnknownOscSequence`).
#[repr(C)]
struct CContent {
    ptr: *const u8,
    len: usize,
}

type Callback = fn(*mut Wrapper, &CContent);

struct Handler {
    apc_unknown_max_bytes: usize,
    callback: Option<Callback>,
}

struct Stream {
    handler: Handler,
    parser: Parser,
}

#[derive(Clone, Copy)]
enum Write {
    MutParser,
    RawPlace,
}

#[derive(Clone, Copy)]
enum Provenance {
    Strict,
    Exposed,
}

struct Wrapper {
    stream: Stream,
    write: Write,
    provenance: Provenance,
    /// The handle with its own provenance, for the `Strict` candidates.
    this: *mut Wrapper,
    /// `TerminalWrapper::live_handler` (N452): the trampoline's handler
    /// pointer while a callback runs.
    live_handler: *mut Handler,
    seen: Vec<u8>,
}

/// `rustty_terminal_set(t, UNKNOWN_MAX_BYTES, &v)`.
unsafe fn set_unknown_max_bytes(w: *mut Wrapper, v: usize) {
    unsafe {
        // The APC half goes through `handler_ptr` (the published live handler).
        let h = if (*w).live_handler.is_null() {
            addr_of_mut!((*w).stream.handler)
        } else {
            (*w).live_handler
        };
        (*h).apc_unknown_max_bytes = v;
        match (*w).write {
            // A `&mut osc::Parser` over every byte of the parser, the inline
            // buffer included.
            Write::MutParser => {
                let p: &mut OscParser = &mut (*w).stream.parser.osc_parser;
                p.unknown_max_bytes = v;
            }
            // The shipped write: a raw place, only the limit's bytes.
            Write::RawPlace => (*w).stream.parser.osc_parser.unknown_max_bytes = v,
        }
    }
}

/// The trampoline: recover the wrapper from the handler address (the D3
/// `offset_of!` seam) and call the C callback.
fn trampoline(handler: &mut Handler, content: &[u8]) {
    let handler: *mut Handler = handler;
    let off = std::mem::offset_of!(Wrapper, stream) + std::mem::offset_of!(Stream, handler);
    let addr = (handler as *mut u8).addr() - off;
    let exposed = std::ptr::with_exposed_provenance_mut::<Wrapper>(addr);
    // Reads of sibling fields go through the wildcard pointer in both modes.
    let (cb, provenance, this) = unsafe {
        (
            (*handler).callback,
            (*exposed).provenance,
            (*exposed).this,
        )
    };
    let w = match provenance {
        Provenance::Strict => this,
        Provenance::Exposed => exposed,
    };
    let c = CContent {
        ptr: content.as_ptr(),
        len: content.len(),
    };
    if let Some(cb) = cb {
        // `LiveHandler::publish` for the callback's duration.
        unsafe { (*exposed).live_handler = handler };
        cb(w, &c);
        unsafe { (*exposed).live_handler = std::ptr::null_mut() };
    }
}

/// `Stream::next_slice_raw`'s OSC end and dispatch.
unsafe fn next_slice_raw(this: *mut Stream) {
    unsafe {
        let cmd: &[u8] = (*this).parser.osc_parser.end();
        trampoline(&mut (*this).handler, cmd);
        (*this).parser.state = 0;
    }
}

/// The embedder's callback: re-enter, then read the borrowed content.
fn on_unknown(w: *mut Wrapper, c: &CContent) {
    unsafe {
        set_unknown_max_bytes(w, 3);
        let content = std::slice::from_raw_parts(c.ptr, c.len);
        let copy = content.to_vec();
        (*w).seen.extend_from_slice(&copy);
    }
}

fn run(write: Write, provenance: Provenance) {
    let w: *mut Wrapper = Box::into_raw(Box::new(Wrapper {
        stream: Stream {
            handler: Handler {
                apc_unknown_max_bytes: 8,
                callback: Some(on_unknown),
            },
            parser: Parser {
                osc_parser: OscParser {
                    buffer: *b"7400;abcdef\0\0\0\0\0",
                    len: 11,
                    unknown_max_bytes: 8,
                },
                state: 1,
            },
        },
        write,
        provenance,
        this: std::ptr::null_mut(),
        live_handler: std::ptr::null_mut(),
        seen: Vec::new(),
    }));
    unsafe {
        // `new_` exposes the whole wrapper once (vt-c terminal.rs).
        let _ = w.expose_provenance();
        (*w).this = w;
        // vt_write: the raw-self stream driver.
        next_slice_raw(addr_of_mut!((*w).stream));
        assert_eq!(b"7400;abc".as_slice(), (*w).seen.as_slice());
        assert_eq!(3, (*w).stream.parser.osc_parser.unknown_max_bytes);
        assert_eq!(3, (*w).stream.handler.apc_unknown_max_bytes);
        assert_eq!(0, (*w).stream.parser.state);
        drop(Box::from_raw(w));
    }
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    match which.as_str() {
        "mut_parser_strict" => run(Write::MutParser, Provenance::Strict),
        "raw_place_strict" => run(Write::RawPlace, Provenance::Strict),
        "mut_parser_exposed" => run(Write::MutParser, Provenance::Exposed),
        "raw_place_exposed" => run(Write::RawPlace, Provenance::Exposed),
        other => panic!("unknown candidate {other:?}"),
    }
    println!("{which}: ok");
}
