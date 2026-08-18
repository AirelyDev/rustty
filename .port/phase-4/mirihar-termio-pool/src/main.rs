//! Isolated, non-FFI Miri harness for the W6 (upstream e0ef934f7) termio
//! write-path rewrite: `SegmentedPool` x 2 -> one `MemoryPool(Write)` where
//! `Write = { td: *ThreadData, req: xev.WriteRequest, buf: [64]u8 }`.
//!
//! It faithfully models the exact aliasing structure of
//! `crates/rustty-termio/src/exec.rs` (`queueWrite`/`ttyWrite`) over
//! `crates/rustty-xev/src/stream.rs` (`queue_write`/`queue_write_trampoline`)
//! and `crates/rustty-zigstd/src/memory_pool.rs` (arena-backed LIFO free-list
//! pool with Node punning):
//!
//!   * `ThreadData` holds the pool header (`write_pool`) AND the intrusive
//!     `write_queue` inline; the engine holds `&write_queue` RAW across ticks.
//!   * `create()` autorefs `&mut (*exec).write_pool` momentarily; the returned
//!     node lives in the pool's ARENA heap block (never inside `ThreadData`),
//!     so the autoref never covers node bytes.
//!   * The checked-out slot is logically UNINITIALIZED; `queueWrite` writes
//!     `w.td`/`w.buf` via `addr_of_mut!` raw places — never a `&mut Write`.
//!   * `queue_write` re-initializes the undefined `req` by whole-value
//!     assignment (`*req = WriteRequest { .. }`, no drop glue: a pure write).
//!   * The loop holds `&w.req.completion` raw across ticks; the trampoline
//!     peeks/pops the queue and then the user callback (`ttyWrite`) destroys
//!     the node via the `w.td` BACKREF (`(*td).write_pool.destroy(w)` — a
//!     momentary `&mut` of the pool header while OTHER in-flight nodes'
//!     addresses stay live), after which the trampoline touches only the NEXT
//!     queue head.
//!   * Destroyed slots are punned to free-list `Node`s and reused LIFO.
//!
//! Candidates (argv[1]; Miri only checks executed code):
//!   * `ported`          — the shipped discipline. MUST be clean under both
//!                         Stacked Borrows and Tree Borrows.
//!   * `ub_threaddata_mut` — DOCUMENTED-UB foil: takes a `&mut ThreadData`
//!                         re-borrow between queue and completion (the shape
//!                         `rust-mut-args-strongly-protected` bans). Expected
//!                         to be flagged by Miri (SB): the re-borrow
//!                         invalidates the engine's raw `&write_queue`.
//!
//! Run (from this dir):
//! ```sh
//! cargo +nightly miri run -q -- ported                          # Stacked Borrows
//! MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- ported  # Tree Borrows
//! cargo +nightly miri run -q -- ub_threaddata_mut               # expected: UB report
//! ```
#![allow(dead_code)]

use std::alloc::{alloc, dealloc, Layout};
use std::ptr::{self, addr_of_mut};

// ---------------------------------------------------------------------------
// Mini event loop (models xev::Loop): holds `*mut Completion` raw across ticks.
// ---------------------------------------------------------------------------

type LoopCallback = fn(*mut (), *mut Loop) -> CallbackAction;

#[derive(PartialEq, Eq, Clone, Copy)]
enum CallbackAction {
    Disarm,
    Rearm,
}

/// Models `xev.Completion`: userdata + callback, re-entered by the loop.
struct Completion {
    userdata: *mut (),
    callback: Option<LoopCallback>,
    /// Models the queued write length the backend would perform.
    op_len: usize,
}

struct Loop {
    /// Pending completions, FIFO (models the submission/completion queues).
    pending: Vec<*mut Completion>,
}

impl Loop {
    /// SAFETY: raw-self — the loop is driven and armed exclusively through the
    /// one raw pointer taken at setup (the xev port's raw loop discipline,
    /// proven separately at .port/phase-2/mirihar-xev-mpsc); no competing
    /// `&mut Loop` ever exists while completions are in flight.
    unsafe fn add(l: *mut Loop, c: *mut Completion) {
        unsafe { (*l).pending.push(c) };
    }

    /// One tick: run every completion queued so far (completions re-queued by
    /// callbacks run on the next tick, like a real loop turn).
    ///
    /// SAFETY: raw-self, see `add`.
    unsafe fn run(l: *mut Loop) {
        let batch: Vec<*mut Completion> = unsafe { std::mem::take(&mut (*l).pending) };
        for c in batch {
            // SAFETY: `c` is `&w.req.completion` — the node stays alive until
            // its own callback destroys it (below), and the loop touches it
            // only through this raw pointer.
            let (cb, ud) = unsafe { ((*c).callback.unwrap(), (*c).userdata) };
            let _ = cb(ud, l);
        }
    }
}

// ---------------------------------------------------------------------------
// Intrusive write queue (models xev::WriteQueue = queue::Intrusive<WriteRequest>).
// ---------------------------------------------------------------------------

struct WriteQueue {
    head: *mut WriteRequest,
    tail: *mut WriteRequest,
}

impl WriteQueue {
    const fn new() -> Self {
        WriteQueue {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
        }
    }
    fn empty(&self) -> bool {
        self.head.is_null()
    }
    /// SAFETY: `req` outlives its time in the queue; `next` reached raw.
    unsafe fn push(&mut self, req: *mut WriteRequest) {
        unsafe {
            *addr_of_mut!((*req).next) = ptr::null_mut();
            if self.tail.is_null() {
                self.head = req;
            } else {
                *addr_of_mut!((*self.tail).next) = req;
            }
            self.tail = req;
        }
    }
    unsafe fn pop(&mut self) -> *mut WriteRequest {
        let req = self.head;
        if !req.is_null() {
            unsafe {
                self.head = (*req).next;
            }
            if self.head.is_null() {
                self.tail = ptr::null_mut();
            }
        }
        req
    }
}

// ---------------------------------------------------------------------------
// WriteRequest (models xev::WriteRequest: completion + userdata + intrusive next;
// no drop glue — whole-value assignment over an undefined slot is a pure write).
// ---------------------------------------------------------------------------

struct WriteRequest {
    completion: Completion,
    userdata: *mut (),
    /// Models `full_write_buffer` (here: the queued length).
    full_len: usize,
    next: *mut WriteRequest,
}

// ---------------------------------------------------------------------------
// MemoryPool (models rustty_zigstd::memory_pool::MemoryPool<Write>): one arena
// heap block, bump allocation, LIFO free list with Node punning. All slot
// pointers derive from the block's raw base — no intermediate references.
// ---------------------------------------------------------------------------

struct FreeNode {
    next: *mut FreeNode,
}

const SLOT_SIZE: usize = {
    let w = std::mem::size_of::<Write>();
    let n = std::mem::size_of::<FreeNode>();
    if w > n {
        w
    } else {
        n
    }
};
const POOL_CAP: usize = 8; // slots per arena block

struct MemoryPool {
    base: *mut u8, // one arena block (models the ArenaAllocator backing node)
    bump: usize,   // next fresh slot index
    free: *mut FreeNode,
}

impl MemoryPool {
    fn layout() -> Layout {
        Layout::from_size_align(SLOT_SIZE * POOL_CAP, std::mem::align_of::<Write>()).unwrap()
    }
    fn init() -> Self {
        // SAFETY: non-zero-size layout.
        let base = unsafe { alloc(Self::layout()) };
        assert!(!base.is_null());
        MemoryPool {
            base,
            bump: 0,
            free: ptr::null_mut(),
        }
    }
    /// `create()` — LIFO reuse else bump. Returns a raw, logically-uninitialized
    /// slot (the caller writes fields via `addr_of_mut!` raw places).
    fn create(&mut self) -> *mut Write {
        if !self.free.is_null() {
            let node = self.free;
            // SAFETY: `node` is a live free-list node we own.
            self.free = unsafe { (*node).next };
            return node.cast::<Write>();
        }
        assert!(self.bump < POOL_CAP, "harness pool exhausted");
        // SAFETY: in-bounds offset within the arena block; derived from `base`
        // (raw provenance — no intermediate reference over the block).
        let slot = unsafe { self.base.add(self.bump * SLOT_SIZE) };
        self.bump += 1;
        slot.cast::<Write>()
    }
    /// `destroy(ptr)` — pun the slot to a free-list Node, push LIFO.
    ///
    /// SAFETY: `ptr` came from `create()` on this pool and is not already free.
    unsafe fn destroy(&mut self, ptr: *mut Write) {
        let node = ptr.cast::<FreeNode>();
        unsafe {
            node.write(FreeNode { next: self.free });
        }
        self.free = node;
    }
}

impl Drop for MemoryPool {
    fn drop(&mut self) {
        // `deinit`: the arena frees every slot at once.
        unsafe { dealloc(self.base, Self::layout()) };
    }
}

// ---------------------------------------------------------------------------
// ThreadData + Write (models exec::ThreadData / exec::Write).
// ---------------------------------------------------------------------------

struct ThreadData {
    write_pool: MemoryPool,
    write_queue: WriteQueue,
    /// Models the rest of ThreadData (`exited` etc.) — a sibling field the
    /// UB foil re-borrows through.
    exited: bool,
    /// Completion tally observed by the test.
    completed: usize,
    completed_bytes: usize,
}

/// The state for a single queued pty write (models `exec::Write`).
struct Write {
    td: *mut ThreadData,
    req: WriteRequest,
    buf: [u8; 64],
}

// ---------------------------------------------------------------------------
// queueWrite / ttyWrite (models exec.rs) over queue_write / trampoline
// (models stream.rs).
// ---------------------------------------------------------------------------

/// Models `Stream::queue_write`: fully re-initializes the undefined `req` by
/// whole-value assignment, then arms the loop/queue with raw pointers into the
/// node and into `ThreadData.write_queue`.
unsafe fn stream_queue_write(
    loop_: *mut Loop,
    q: *mut WriteQueue,
    req: *mut WriteRequest,
    len: usize,
    userdata: *mut Write,
) {
    unsafe {
        // req.* = .{ .full_write_buffer = buf }; — pure write (no drop glue)
        // over the logically-uninitialized pool slot.
        *req = WriteRequest {
            completion: Completion {
                userdata: ptr::null_mut(),
                callback: None,
                op_len: 0,
            },
            userdata: ptr::null_mut(),
            full_len: len,
            next: ptr::null_mut(),
        };
        (*req).completion.op_len = len;
        (*req).completion.userdata = q.cast::<()>();
        (*req).completion.callback = Some(queue_write_trampoline);
        (*req).userdata = userdata.cast::<()>();

        if (*q).empty() {
            Loop::add(loop_, addr_of_mut!((*req).completion));
        }
        (*q).push(req);
    }
}

/// Models `Stream::queue_write_trampoline`: peek head, (full write) pop, call
/// the user callback (which DESTROYS the node), then arm the next head.
fn queue_write_trampoline(ud: *mut (), l_inner: *mut Loop) -> CallbackAction {
    let q_inner = ud as *mut WriteQueue;
    // SAFETY: `q_inner` points at `ThreadData.write_queue`, alive for the run;
    // reached only through this raw pointer (never a `&mut ThreadData`).
    let req_inner: *mut WriteRequest = unsafe { (*q_inner).head };
    assert!(!req_inner.is_null());

    let written = unsafe { (*req_inner).completion.op_len };

    // Full write: pop the previously peeked request.
    let popped = unsafe { (*q_inner).pop() };
    assert_eq!(popped, req_inner);

    // The user callback — ttyWrite — destroys the node.
    let action = tty_write(
        unsafe { (*req_inner).userdata.cast::<Write>() },
        l_inner,
        Ok(written),
    );

    // (Rearm would re-push; ttyWrite always disarms.)
    assert!(action == CallbackAction::Disarm);

    // If we have another request, add that completion next — touches only the
    // NEXT node, never the just-destroyed one.
    let req_next = unsafe { (*q_inner).head };
    if !req_next.is_null() {
        unsafe { Loop::add(l_inner, addr_of_mut!((*req_next).completion)) };
    }
    CallbackAction::Disarm
}

/// Models `Exec::ttyWrite`: recover the node from userdata, follow the `td`
/// BACKREF, destroy the node via a momentary `&mut` of the pool header only.
fn tty_write(w_: *mut Write, _l: *mut Loop, r: Result<usize, ()>) -> CallbackAction {
    let w = w_;
    // SAFETY: `w` is the pool-arena node this write checked out; `w.td` was
    // written at checkout and the ThreadData outlives the loop. The
    // `&mut write_pool` autoref covers only the pool header inside ThreadData
    // — never the arena node bytes — so other in-flight nodes stay valid.
    unsafe {
        let td = *addr_of_mut!((*w).td);
        let d = r.unwrap();
        // Tally through raw places on td (models the log; also proves sibling
        // field access through the raw td stays fine).
        *addr_of_mut!((*td).completed) += 1;
        *addr_of_mut!((*td).completed_bytes) += d;
        (*td).write_pool.destroy(w);
    }
    CallbackAction::Disarm
}

/// Models `Exec::queueWrite`: chunk `data` into pooled 64-byte nodes.
unsafe fn queue_write(exec: *mut ThreadData, loop_: *mut Loop, data: &[u8], linefeed: bool) {
    unsafe {
        if *addr_of_mut!((*exec).exited) {
            return;
        }
        let mut i: usize = 0;
        while i < data.len() {
            // const w = try exec.write_pool.create(alloc);
            // Momentary `&mut write_pool` autoref through the raw `exec` —
            // covers the pool header only, never arena node bytes.
            let w: *mut Write = (*exec).write_pool.create();
            // w.td = exec; — raw place write into the uninitialized slot.
            addr_of_mut!((*w).td).write(exec);
            // const buf = &w.buf;
            let buf: *mut [u8; 64] = addr_of_mut!((*w).buf);
            let buf_len = 64usize;

            let slice_len = {
                let max = std::cmp::min(data.len(), i + buf_len);
                if !linefeed {
                    let len = max - i;
                    ptr::copy_nonoverlapping(data[i..max].as_ptr(), buf.cast::<u8>(), len);
                    i = max;
                    len
                } else {
                    let mut buf_i: usize = 0;
                    while i < data.len() && buf_i < buf_len - 1 {
                        let ch = data[i];
                        i += 1;
                        if ch != b'\r' {
                            (*buf)[buf_i] = ch;
                            buf_i += 1;
                            continue;
                        }
                        (*buf)[buf_i] = b'\r';
                        (*buf)[buf_i + 1] = b'\n';
                        buf_i += 2;
                    }
                    buf_i
                }
            };

            stream_queue_write(
                loop_,
                addr_of_mut!((*exec).write_queue),
                addr_of_mut!((*w).req),
                slice_len,
                w,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Candidates
// ---------------------------------------------------------------------------

/// The shipped discipline: multi-chunk queue, FIFO completion with destroy-in-
/// callback while later nodes are still in flight, LIFO slot reuse on a second
/// wave, and a linefeed (CRLF expansion) wave.
fn ported() {
    let mut td = ThreadData {
        write_pool: MemoryPool::init(),
        write_queue: WriteQueue::new(),
        exited: false,
        completed: 0,
        completed_bytes: 0,
    };
    let exec: *mut ThreadData = &mut td;
    let mut l = Loop { pending: Vec::new() };
    let loop_: *mut Loop = &mut l;

    // Wave 1: 3 chunks in flight at once (150 bytes / 64-byte buffers).
    let data = [b'a'; 150];
    unsafe { queue_write(exec, loop_, &data, false) };
    // Drain: each tick completes one write and arms the next head.
    unsafe {
        while !(*loop_).pending.is_empty() {
            Loop::run(loop_);
        }
    }

    // Wave 2: LIFO reuse of the just-freed slots + CRLF expansion path.
    unsafe { queue_write(exec, loop_, b"one\rtwo\rthree", true) };
    unsafe {
        while !(*loop_).pending.is_empty() {
            Loop::run(loop_);
        }
    }

    // Only now re-borrow td naturally (the loop is drained; no raw pointers
    // into it are live anymore).
    assert_eq!(td.completed, 4, "3 chunk writes + 1 linefeed write");
    assert_eq!(td.completed_bytes, 150 + "one\r\ntwo\r\nthree".len());
    // td (and its pool arena, freeing every slot at once) drops here.
}

/// DOCUMENTED-UB foil (`rust-mut-args-strongly-protected`): a `&mut ThreadData`
/// re-borrow while the engine still holds raw pointers into `write_queue`
/// invalidates them. Expected: Miri flags the trampoline's queue access.
fn ub_threaddata_mut() {
    let mut td = ThreadData {
        write_pool: MemoryPool::init(),
        write_queue: WriteQueue::new(),
        exited: false,
        completed: 0,
        completed_bytes: 0,
    };
    let exec: *mut ThreadData = &mut td;
    let mut l = Loop { pending: Vec::new() };
    let loop_: *mut Loop = &mut l;

    unsafe { queue_write(exec, loop_, &[b'a'; 100], false) };

    // The banned shape: a whole-struct `&mut` between queue and completion.
    let td_mut: &mut ThreadData = &mut td;
    td_mut.exited = false;

    // The engine now re-enters `&write_queue` through its stale raw pointer.
    unsafe {
        while !(*loop_).pending.is_empty() {
            Loop::run(loop_);
        }
    }
}

fn main() {
    let candidate = std::env::args().nth(1).unwrap_or_else(|| "ported".into());
    match candidate.as_str() {
        "ported" => ported(),
        "ub_threaddata_mut" => ub_threaddata_mut(),
        other => panic!("unknown candidate: {other}"),
    }
    println!("ok: {candidate}");
}
