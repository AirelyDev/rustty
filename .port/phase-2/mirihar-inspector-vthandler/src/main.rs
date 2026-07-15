//! Isolated, non-FFI Miri harness that PROVES the inspector termio widget's
//! `VTHandler.State.events` **self-alias** is Stacked/Tree-Borrows clean —
//! PREPORT_INSPECTOR.md **risk R1** (the top SB hazard of the whole tier) and
//! LIFETIMES row `termio.zig VTHandler.State events *VTEvent.Ring` ("SELF-ALIAS
//! of Stream.events (sibling field of same Stream); route appends via raw-self
//! helper, prove on Miri").
//!
//! # What the real code does (faithful model)
//!
//! `src/inspector/widgets/termio.zig` — the VT-event inspector owns a
//! `Stream { events: VTEvent.Ring, parser_stream: Stream(VTHandler) }`. On every
//! pty read (`recordPtyRead`, termio.zig:45-58) it wires a **raw alias of its own
//! `events` field** into the parser's handler, drives the parser, then clears it:
//!
//! ```zig
//! self.parser_stream.handler.state = .{ .events = &self.events, ... };  // :54
//! defer self.parser_stream.handler.state = null;                         // :56
//! self.parser_stream.nextSlice(data);                                    // :57
//! ```
//!
//! Inside `nextSlice`, for every parser action the handler's `vtRaw`
//! (termio.zig:745-803) appends the built event **through that raw alias**
//! (`vt_events.append(ev)`, :784), including the INV-9 grow-then-evict path
//! (`resize` / `deleteOldest` then re-`append`, :784-798).
//!
//! So there is a long-lived raw `*mut Ring` aliasing the **sibling field**
//! `Stream.events` that is mutated-through while `Stream.parser_stream` is
//! `&mut`-borrowed by `nextSlice`. That is exactly the
//! [[shaper-hook-sb-clean-interleave]] hazard shape ("long-lived raw alias into a
//! field + interleaved `&mut` to that field = SB/TB UB").
//!
//! # The load-bearing SB/TB fact this harness certifies
//!
//! The self-alias MUST be a **raw `*mut Ring`** (mirroring the Zig `*VTEvent.Ring`
//! pointer), never a Rust `&mut Ring` *reference*. The append is routed entirely
//! through that raw pointer — copied out of `state.events` as a `*mut Ring`,
//! dereferenced with a narrow `&mut *ptr` reborrow *per append* — while the parser
//! drive borrows only the **disjoint** sibling `parser_stream` / `handler`.
//!
//! A raw pointer created with `&raw mut self.events` sits at the *base* of that
//! field's borrow stack (SB) / aliases the parent tag (TB), so it is **robustly
//! preserved** across `&mut` reborrows — both of the disjoint sibling `handler`
//! AND even of `self.events` itself — which is precisely *why* the raw-self
//! routing is sound. The discipline is **load-bearing** because the NAIVE
//! alternative is not: the NEGATIVE candidate materializes a long-lived
//! `&mut Ring` *reference* from the alias (the mistake of porting `*VTEvent.Ring`
//! as a Rust reference and holding it across the capture window) and then suffers
//! one interleaved parent reborrow of `events` — under SB the reference is popped
//! off the stack, under TB it transitions to `Disabled` by the foreign write, and
//! the next append through it is UB. Miri flags it, proving the `*mut` raw-self
//! routing is not incidentally green.
//!
//! # Candidates (Miri only interprets executed code; select by argv[1])
//!
//! | candidate | models | expect |
//! |---|---|---|
//! | `raw_alias_append_clean` | **shipped**: wire `&raw mut self.events` (raw `*mut`) into the handler, drive the parser (disjoint `&mut self.handler`), append through the raw alias | CLEAN (SB+TB) |
//! | `raw_alias_grow_evict_clean` | **shipped**, INV-9: the append through the raw alias triggers grow-then-evict (`resize`+`delete_oldest`+re-`append`) — all through the raw alias | CLEAN (SB+TB) |
//! | `ref_alias_ub` | NEGATIVE: the naive port that holds a `&mut Ring` *reference* alias across an interleaved parent reborrow of `events`, then appends through the stale reference | UB (SB+TB) |
//!
//! Pass criterion: the two shipped candidates are CLEAN under BOTH Stacked
//! Borrows (default) and Tree Borrows (`-Zmiri-tree-borrows`); the NEGATIVE is
//! flagged UB at the through-the-reference append under both.
#![allow(dead_code)]

// ---- Ring analogue (`VTEvent.Ring = CircBuf(VTEvent, undefined)`) ----------------
// A minimal grow-then-evict buffer carrying the two operations vtRaw drives
// through the alias: `append` (with an OutOfMemory-at-capacity signal) and the
// INV-9 recovery (`resize` grow, else `delete_oldest`+re-append). We store plain
// `u64` "events" — the SB/TB question is about the *aliasing of the field*, not
// the element payload, so a scalar element keeps the model honest and small.
struct Ring {
    storage: Vec<u64>,
    len: usize,
    cap: usize,
}

impl Ring {
    fn init(cap: usize) -> Ring {
        Ring { storage: vec![0; cap], len: 0, cap }
    }

    /// `CircBuf.append`: Err on full (the OutOfMemory ring-full signal).
    fn append(&mut self, v: u64) -> Result<(), ()> {
        if self.len >= self.cap {
            return Err(());
        }
        self.storage[self.len] = v;
        self.len += 1;
        Ok(())
    }

    /// `CircBuf.resize`: grow the backing storage (INV-9 grow arm).
    fn resize(&mut self, new_cap: usize) {
        self.storage.resize(new_cap, 0);
        self.cap = new_cap;
    }

    /// `CircBuf.deleteOldest(1)`: drop the oldest element, shifting down (INV-9
    /// evict arm — element drop before slot reuse).
    fn delete_oldest(&mut self) {
        if self.len > 0 {
            self.storage.copy_within(1..self.len, 0);
            self.len -= 1;
        }
    }

    fn capacity(&self) -> usize {
        self.cap
    }
}

// ---- Handler analogue (`VTHandler` + its `State`) --------------------------------
// `State.events: *VTEvent.Ring` is the SELF-ALIAS (termio.zig:54,719). It is a
// RAW pointer, never a `&mut` — the whole point of R1.
struct State {
    // events: *VTEvent.Ring  — raw alias of the sibling `Stream.events` field.
    events: *mut Ring,
}

struct Handler {
    // state: ?State  — set for the capture window, else null ⇒ drop events.
    state: Option<State>,
    // current_seq: usize  — the monotonically-incrementing sequence (termio.zig:701).
    current_seq: u64,
}

impl Handler {
    fn new() -> Handler {
        Handler { state: None, current_seq: 1 }
    }

    /// Faithful `vtRaw` (termio.zig:745-803): for one action, build an event and
    /// append it **through the raw alias** with the INV-9 grow-then-evict policy.
    /// Borrows only `&mut self` (the Handler) — NEVER `&mut` the aliased Ring
    /// directly; the Ring is reached solely via the raw `*mut` copied out of
    /// `state.events`. This is the "route via the alias's OWN pointer + raw-self
    /// helper" discipline.
    fn vt_raw(&mut self, action: u64) {
        // const state: *State = if (self.state) |*s| s else return;
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };
        // Copy the raw alias OUT — we never hold a borrow into self.state across
        // the append, and never form `&mut (the Ring)` other than the narrow
        // `&mut *vt_events` reborrow of this raw pointer.
        let vt_events: *mut Ring = state.events;

        // defer self.current_seq +%= 1;
        let ev = action;

        // vt_events.append(ev) catch grow-then-evict (INV-9):
        const MAX_CAPACITY: usize = 4; // (termio.zig `max_capacity = 100`, shrunk)
        // SAFETY: `vt_events` is a live raw alias of `Stream.events`, obtained via
        // `&raw mut self.events` at the Stream level; the parser drive borrows only
        // the disjoint sibling `handler`, so the alias retains provenance here.
        unsafe {
            if (*vt_events).append(ev).is_err() {
                if (*vt_events).capacity() < MAX_CAPACITY {
                    let new_capacity = core::cmp::min((*vt_events).capacity() * 2, MAX_CAPACITY);
                    (*vt_events).resize(new_capacity);
                    let _ = (*vt_events).append(ev);
                } else {
                    (*vt_events).delete_oldest();
                    let _ = (*vt_events).append(ev);
                }
            }
        }

        self.current_seq = self.current_seq.wrapping_add(1);
    }
}

// ---- ParserStream analogue (`terminal.Stream(VTHandler)`) ------------------------
// Owns the `Handler`. `next_slice` borrows `&mut self` (⇒ `&mut self.handler`),
// disjoint from `Stream.events`, and drives `vt_raw` per input byte.
struct ParserStream {
    handler: Handler,
}

impl ParserStream {
    fn new() -> ParserStream {
        ParserStream { handler: Handler::new() }
    }

    // pub inline fn nextSlice(self: *Self, input: []const u8) void
    fn next_slice(&mut self, input: &[u8]) {
        for &b in input {
            self.handler.vt_raw(b as u64);
        }
    }
}

// ---- Stream analogue (termio.zig `Stream`) ---------------------------------------
// The widget: `events` and `parser_stream` are SIBLING fields; `recordPtyRead`
// wires a raw alias of `events` into `parser_stream.handler.state`.
struct Stream {
    events: Ring,
    parser_stream: ParserStream,
}

impl Stream {
    fn new(cap: usize) -> Stream {
        Stream { events: Ring::init(cap), parser_stream: ParserStream::new() }
    }

    /// Faithful `recordPtyRead` (termio.zig:45-58). THE shipped discipline:
    ///   1. take a RAW alias of the sibling `events` field (`&raw mut self.events`),
    ///   2. stash it in the handler's capture state,
    ///   3. drive the parser through the DISJOINT `&mut self.parser_stream`
    ///      (which appends through the raw alias), then
    ///   4. clear the capture state.
    /// No `&mut self.events` is ever formed while the alias is live.
    fn record_pty_read(&mut self, data: &[u8]) {
        // self.parser_stream.handler.state = .{ .events = &self.events, ... };
        let events_ptr: *mut Ring = &raw mut self.events;
        self.parser_stream.handler.state = Some(State { events: events_ptr });

        // self.parser_stream.nextSlice(data);  — borrows the disjoint sibling.
        self.parser_stream.next_slice(data);

        // defer self.parser_stream.handler.state = null;
        self.parser_stream.handler.state = None;
    }
}

// ============ CANDIDATE: raw_alias_append_clean (expect CLEAN SB+TB) ============
fn raw_alias_append_clean() {
    let mut s = Stream::new(8);
    s.record_pty_read(b"abc");
    assert_eq!(s.events.len, 3, "three actions appended through the raw alias");
    assert_eq!(&s.events.storage[0..3], &[b'a' as u64, b'b' as u64, b'c' as u64]);
    assert_eq!(s.parser_stream.handler.current_seq, 4, "seq incremented per action");
    assert!(s.parser_stream.handler.state.is_none(), "capture window cleared by defer");
    println!("raw_alias_append_clean: OK (appended through raw self-alias, no UB)");
}

// ============ CANDIDATE: raw_alias_grow_evict_clean (expect CLEAN SB+TB) ============
// INV-9: start below MAX_CAPACITY so appends first GROW (resize) then, at cap,
// EVICT (delete_oldest) — all mutating the aliased Ring through the raw alias
// while `parser_stream` is the live `&mut` borrow.
fn raw_alias_grow_evict_clean() {
    let mut s = Stream::new(1); // cap 1 < MAX_CAPACITY(4): forces grow, then evict
    s.record_pty_read(b"abcdef"); // 6 actions into a ring that grows to 4 then evicts
    assert_eq!(s.events.len, 4, "ring grew to MAX_CAPACITY then evicted oldest");
    // After growth to cap 4 and eviction, the newest 4 survive: c d e f.
    assert_eq!(
        &s.events.storage[0..4],
        &[b'c' as u64, b'd' as u64, b'e' as u64, b'f' as u64],
        "grow-then-evict kept the newest events (INV-9 ordering)"
    );
    println!("raw_alias_grow_evict_clean: OK (grow+evict through raw alias, no UB)");
}

// ============ NEGATIVE: ref_alias_ub (expect UB — SB + TB) ============
// The NAIVE port: instead of the raw `*mut Ring` self-alias, it materializes a
// long-lived `&mut Ring` *reference* alias of `self.events` (the mistake of
// porting the Zig `*VTEvent.Ring` as a Rust reference and holding it across the
// capture window). One interleaved parent reborrow of `events` then invalidates
// that reference — under Stacked Borrows it is popped off the field's borrow
// stack; under Tree Borrows the foreign write transitions it to `Disabled` — and
// the next append THROUGH the stale reference is UB. This is exactly what the
// raw-self routing (candidates above) forbids: a `*mut` alias would survive the
// same interleave (it sits at the stack base / aliases the parent tag), so if
// deref-through-reference were sound the shipped discipline would be needless and
// this would be false-green.
fn ref_alias_ub() {
    let mut s = Stream::new(8);

    // NAIVE: hold a long-lived `&mut Ring` *reference* alias of self.events
    // (transmuted past the borrow checker to model the mis-port — the checker
    // would otherwise reject holding it across the interleave below).
    let bad_ref: &mut Ring = unsafe { &mut *(&raw mut s.events) };

    // INTERLEAVE one direct reborrow of the same field from the parent `s`
    // (e.g. the widget reading/appending its own `events`). This pops/Disables
    // `bad_ref`.
    s.events.append(111).unwrap();

    // …now append through the stale reference → UB (Miri flags here).
    bad_ref.append(222).unwrap();

    println!("ref_alias_ub: (unreachable — Miri should have flagged UB) len={}", s.events.len);
}

// The two shipped-discipline candidates double as Miri tests so
// `cargo +nightly miri test` is a single green gate. `ref_alias_ub` is UB by
// construction and is NOT a `#[test]` — observe it via
// `cargo +nightly miri run -- ref_alias_ub` (see the module doc table).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_self_alias_append_is_sb_tb_clean() {
        raw_alias_append_clean();
    }

    #[test]
    fn raw_self_alias_grow_evict_is_sb_tb_clean() {
        raw_alias_grow_evict_clean();
    }
}

fn main() {
    let which = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "raw_alias_append_clean".to_string());
    match which.as_str() {
        "raw_alias_append_clean" => raw_alias_append_clean(),
        "raw_alias_grow_evict_clean" => raw_alias_grow_evict_clean(),
        "ref_alias_ub" => ref_alias_ub(),
        other => {
            eprintln!("unknown candidate: {other}");
            std::process::exit(2);
        }
    }
}
