//! Isolated, non-FFI Miri harness reproducing the rustty-xev kqueue MPSC
//! completion-queue latent SB/TB UB (LIFETIMES-audit findings M1 + M2).
//!
//! The real hazard (crates/rustty-xev/src/kqueue.rs): the Vyukov intrusive MPSC
//! queue `thread_pool_completions` is SELF-REFERENTIAL — its `head`/`tail` point
//! at the EMBEDDED `stub` field (an address INSIDE the owning `Loop`). init writes
//! those stub self-pointers ONCE (first tick); pop/push read+deref them on EVERY
//! later tick. But every access site forms a fresh `&mut self.thread_pool_completions`
//! coerced to `*mut Self`, and — crucially — `tick(&mut self)` itself reborrows the
//! whole `Loop` each call. Under Stacked/Tree Borrows those reborrows invalidate the
//! provenance the stored stub self-pointers were minted with, so the later deref of
//! the stored `tail`/`stub.next` is use-after-invalidation. The branch is DEAD today
//! (thread_pool always None), so no existing test reaches it and a normal Miri run
//! won't flag it — hence this synthetic reproduction of the init->tick->pop cycle
//! and the cross-thread thread_perform push.
//!
//! Each candidate is one executed path (Miri only checks executed code), selected by
//! argv[1]. Goal: find the MINIMAL faithful (Zig `*Self`-preserving) structure that
//! is CLEAN under BOTH -Zmiri-stacked-borrows and -Zmiri-tree-borrows, while the
//! naive shapes reproduce the UB.
#![allow(dead_code)]

use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

// The intrusive element (models `Completion`, whose `.next` link the queue threads).
struct Elem {
    next: *mut Elem,
    val: u32,
}
impl Elem {
    fn new(val: u32) -> Self {
        Elem { next: ptr::null_mut(), val }
    }
    // Address of the intrusive `next` field (the Node-trait seam in the real port).
    unsafe fn next_field(this: *mut Elem) -> *mut *mut Elem {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

// ===========================================================================
// PLAIN queue — faithful port of queue_mpsc.rs as it stands today (plain
// `*mut Elem` head/tail + embedded `stub: Elem`). Methods take `*mut Self`.
// ===========================================================================
struct QPlain {
    head: *mut Elem,
    tail: *mut Elem,
    stub: Elem,
}
impl QPlain {
    fn new() -> Self {
        QPlain { head: ptr::null_mut(), tail: ptr::null_mut(), stub: Elem::new(0) }
    }
    unsafe fn init(this: *mut Self) {
        unsafe {
            let stub: *mut Elem = ptr::addr_of_mut!((*this).stub);
            ptr::addr_of_mut!((*this).head).write(stub);
            ptr::addr_of_mut!((*this).tail).write(stub);
            Elem::next_field(stub).write(ptr::null_mut());
        }
    }
    unsafe fn push(this: *mut Self, v: *mut Elem) {
        unsafe {
            AtomicPtr::from_ptr(Elem::next_field(v)).store(ptr::null_mut(), Ordering::Relaxed);
            let prev =
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).head)).swap(v, Ordering::AcqRel);
            AtomicPtr::from_ptr(Elem::next_field(prev)).store(v, Ordering::Release);
        }
    }
    unsafe fn pop(this: *mut Self) -> *mut Elem {
        unsafe {
            let mut tail =
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).tail)).load(Ordering::Relaxed);
            let mut next_ = AtomicPtr::from_ptr(Elem::next_field(tail)).load(Ordering::Acquire);
            let stub: *mut Elem = ptr::addr_of_mut!((*this).stub);
            if tail == stub {
                if next_.is_null() {
                    return ptr::null_mut();
                }
                let next = next_;
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).tail)).store(next, Ordering::Relaxed);
                tail = next;
                next_ = AtomicPtr::from_ptr(Elem::next_field(tail)).load(Ordering::Acquire);
            }
            if !next_.is_null() {
                let next = next_;
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).tail)).store(next, Ordering::Release);
                Elem::next_field(tail).write(ptr::null_mut());
                return tail;
            }
            let head = AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).head)).load(Ordering::Relaxed);
            if tail != head {
                return ptr::null_mut();
            }
            Self::push(this, stub);
            next_ = AtomicPtr::from_ptr(Elem::next_field(tail)).load(Ordering::Acquire);
            if !next_.is_null() {
                let next = next_;
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).tail)).store(next, Ordering::Relaxed);
                Elem::next_field(tail).write(ptr::null_mut());
                return tail;
            }
            ptr::null_mut()
        }
    }
}

// ===========================================================================
// CELL queue — the candidate fix: head/tail/stub live in `UnsafeCell` so a
// later `&mut Loop` reborrow grants SharedReadWrite (not Unique) over those
// bytes and does NOT invalidate the stored stub self-pointers. Methods still
// take `*mut Self` (Zig `*Self` preserved) and route ALL access through raw
// `UnsafeCell::raw_get` — never a `&`/`&mut` to the cells.
// ===========================================================================
struct QCell {
    head: UnsafeCell<*mut Elem>,
    tail: UnsafeCell<*mut Elem>,
    stub: UnsafeCell<Elem>,
}
impl QCell {
    fn new() -> Self {
        QCell {
            head: UnsafeCell::new(ptr::null_mut()),
            tail: UnsafeCell::new(ptr::null_mut()),
            stub: UnsafeCell::new(Elem::new(0)),
        }
    }
    #[inline]
    unsafe fn head_ptr(this: *mut Self) -> *mut *mut Elem {
        unsafe { UnsafeCell::raw_get(ptr::addr_of!((*this).head)) }
    }
    #[inline]
    unsafe fn tail_ptr(this: *mut Self) -> *mut *mut Elem {
        unsafe { UnsafeCell::raw_get(ptr::addr_of!((*this).tail)) }
    }
    #[inline]
    unsafe fn stub_ptr(this: *mut Self) -> *mut Elem {
        unsafe { UnsafeCell::raw_get(ptr::addr_of!((*this).stub)) }
    }
    unsafe fn init(this: *mut Self) {
        unsafe {
            let stub: *mut Elem = Self::stub_ptr(this);
            Self::head_ptr(this).write(stub);
            Self::tail_ptr(this).write(stub);
            Elem::next_field(stub).write(ptr::null_mut());
        }
    }
    unsafe fn push(this: *mut Self, v: *mut Elem) {
        unsafe {
            AtomicPtr::from_ptr(Elem::next_field(v)).store(ptr::null_mut(), Ordering::Relaxed);
            let prev = AtomicPtr::from_ptr(Self::head_ptr(this)).swap(v, Ordering::AcqRel);
            AtomicPtr::from_ptr(Elem::next_field(prev)).store(v, Ordering::Release);
        }
    }
    unsafe fn pop(this: *mut Self) -> *mut Elem {
        unsafe {
            let mut tail = AtomicPtr::from_ptr(Self::tail_ptr(this)).load(Ordering::Relaxed);
            let mut next_ = AtomicPtr::from_ptr(Elem::next_field(tail)).load(Ordering::Acquire);
            let stub: *mut Elem = Self::stub_ptr(this);
            if tail == stub {
                if next_.is_null() {
                    return ptr::null_mut();
                }
                let next = next_;
                AtomicPtr::from_ptr(Self::tail_ptr(this)).store(next, Ordering::Relaxed);
                tail = next;
                next_ = AtomicPtr::from_ptr(Elem::next_field(tail)).load(Ordering::Acquire);
            }
            if !next_.is_null() {
                let next = next_;
                AtomicPtr::from_ptr(Self::tail_ptr(this)).store(next, Ordering::Release);
                Elem::next_field(tail).write(ptr::null_mut());
                return tail;
            }
            let head = AtomicPtr::from_ptr(Self::head_ptr(this)).load(Ordering::Relaxed);
            if tail != head {
                return ptr::null_mut();
            }
            Self::push(this, stub);
            next_ = AtomicPtr::from_ptr(Elem::next_field(tail)).load(Ordering::Acquire);
            if !next_.is_null() {
                let next = next_;
                AtomicPtr::from_ptr(Self::tail_ptr(this)).store(next, Ordering::Relaxed);
                Elem::next_field(tail).write(ptr::null_mut());
                return tail;
            }
            ptr::null_mut()
        }
    }
}

// ===========================================================================
// The owning Loop analogue. Embeds the queue inline (faithful: `Loop` embeds
// `thread_pool_completions: TaskCompletionQueue`) plus a scalar `other` and a
// `flags_init` bool, both written via `&mut self` each tick to force SB/TB to
// exercise the whole-Loop reborrow that pops stale internal tags.
// ===========================================================================
struct LPlain {
    flags_init: bool,
    other: u32,
    q: QPlain,
}
impl LPlain {
    fn new() -> Self {
        LPlain { flags_init: false, other: 0, q: QPlain::new() }
    }

    // ---- NAIVE: `&mut self.q` coercions (exactly what kqueue.rs ships) ----
    fn tick_naive(&mut self, e: *mut Elem) {
        if !self.flags_init {
            self.flags_init = true;
            unsafe { QPlain::init(&mut self.q) };
        }
        self.other = self.other.wrapping_add(1); // narrow &mut self write
        unsafe { QPlain::push(&mut self.q, e) };
        let _ = unsafe { QPlain::pop(&mut self.q) };
    }

    // ---- PRESCRIPTION: self_ptr addr_of_mut, but tick still `&mut self` ----
    // (the DEFERRALS one-line fix — tests whether it survives cross-tick retag.)
    fn tick_selfptr(&mut self, e: *mut Elem) {
        let sp: *mut LPlain = self;
        unsafe {
            if !(*sp).flags_init {
                (*sp).flags_init = true;
                QPlain::init(ptr::addr_of_mut!((*sp).q));
            }
            (*sp).other = (*sp).other.wrapping_add(1);
            QPlain::push(ptr::addr_of_mut!((*sp).q), e);
            let _ = QPlain::pop(ptr::addr_of_mut!((*sp).q));
        }
    }
}

struct LCell {
    flags_init: bool,
    other: u32,
    q: QCell,
}
impl LCell {
    fn new() -> Self {
        LCell { flags_init: false, other: 0, q: QCell::new() }
    }

    // ---- CELL + self_ptr, tick `&mut self` (candidate minimal fix) ----
    fn tick_cell(&mut self, e: *mut Elem) {
        let sp: *mut LCell = self;
        unsafe {
            if !(*sp).flags_init {
                (*sp).flags_init = true;
                QCell::init(ptr::addr_of_mut!((*sp).q));
            }
            (*sp).other = (*sp).other.wrapping_add(1);
            QCell::push(ptr::addr_of_mut!((*sp).q), e);
            let _ = QCell::pop(ptr::addr_of_mut!((*sp).q));
        }
    }
}

// ---- RAW-TICK: driver holds `*mut LPlain`; tick takes `*mut Self` (the Zig
// `tick(self: *Loop)` shape). No `&mut self` entry reborrow EVER. ----
impl LPlain {
    unsafe fn tick_raw(this: *mut LPlain, e: *mut Elem) {
        unsafe {
            if !(*this).flags_init {
                (*this).flags_init = true;
                QPlain::init(ptr::addr_of_mut!((*this).q));
            }
            (*this).other = (*this).other.wrapping_add(1);
            QPlain::push(ptr::addr_of_mut!((*this).q), e);
            let _ = QPlain::pop(ptr::addr_of_mut!((*this).q));
        }
    }
}

// ===========================================================================
// BOXQ: keep the PUBLIC `tick(&mut self)` API but give the self-referential
// queue a SEPARATE, STABLE heap allocation (Box) plus a cached raw `*mut QPlain`
// established ONCE at init from the box. All init/push/pop route through the
// cached raw pointer, whose tag is rooted in the HEAP allocation — untouched by
// the per-tick whole-`Loop` `&mut self` reborrow (which only retags the Loop
// allocation). After init the box itself is NEVER reborrowed (only the cached
// raw ptr is used), so the heap alloc's borrow stack keeps the stub-pointer
// lineage alive across ticks. This is the "queue in its own allocation, reached
// via a stable raw" shape, mirroring how the OTHER intrusive queues stay clean
// (their head/tail point at EXTERNAL allocations, immune to Loop reborrows).
// ===========================================================================
struct LBoxq {
    flags_init: bool,
    other: u32,
    qbox: Box<QPlain>,
    qptr: *mut QPlain, // cached at init; heap-rooted; the ONLY post-init handle
}
impl LBoxq {
    fn new() -> Self {
        LBoxq {
            flags_init: false,
            other: 0,
            qbox: Box::new(QPlain::new()),
            qptr: ptr::null_mut(),
        }
    }
    fn tick_boxq(&mut self, e: *mut Elem) {
        let sp: *mut LBoxq = self;
        unsafe {
            if !(*sp).flags_init {
                (*sp).flags_init = true;
                // establish the cached raw ONCE, heap-rooted, before init writes
                // the stub self-pointers through it.
                let qp: *mut QPlain = &raw mut *(*sp).qbox;
                ptr::addr_of_mut!((*sp).qptr).write(qp);
                QPlain::init(qp);
            }
            (*sp).other = (*sp).other.wrapping_add(1);
            // read the cached heap-rooted raw (its provenance survives tick retags)
            let qp: *mut QPlain = (*sp).qptr;
            QPlain::push(qp, e);
            let _ = QPlain::pop(qp);
        }
    }
}

// CONTROL: inline queue + cached raw `qptr = addr_of_mut!((*sp).q)` at init.
// Proves the CACHE alone is NOT enough — the cached raw still points INTO the
// Loop allocation, so the next tick's whole-`Loop` `&mut self` Unique retag pops
// it. Only a SEPARATE allocation (boxq) launders the provenance. Expect UB.
struct LInlineCached {
    flags_init: bool,
    other: u32,
    q: QPlain,
    qptr: *mut QPlain,
}
impl LInlineCached {
    fn new() -> Self {
        LInlineCached { flags_init: false, other: 0, q: QPlain::new(), qptr: ptr::null_mut() }
    }
    fn tick(&mut self, e: *mut Elem) {
        let sp: *mut LInlineCached = self;
        unsafe {
            if !(*sp).flags_init {
                (*sp).flags_init = true;
                let qp: *mut QPlain = ptr::addr_of_mut!((*sp).q);
                ptr::addr_of_mut!((*sp).qptr).write(qp);
                QPlain::init(qp);
            }
            (*sp).other = (*sp).other.wrapping_add(1);
            let qp: *mut QPlain = (*sp).qptr;
            QPlain::push(qp, e);
            let _ = QPlain::pop(qp);
        }
    }
}
fn run_inline_cached() {
    let mut lp: Box<LInlineCached> = Box::new(LInlineCached::new());
    let elems: Vec<*mut Elem> =
        (0..TICKS).map(|i| Box::into_raw(Box::new(Elem::new(i as u32)))).collect();
    for i in 0..TICKS {
        lp.tick(elems[i]);
    }
    println!("inline_cached: other={}", lp.other);
    for e in elems {
        drop(unsafe { Box::from_raw(e) });
    }
}

fn run_boxq() {
    let mut lp: Box<LBoxq> = Box::new(LBoxq::new());
    let elems: Vec<*mut Elem> =
        (0..TICKS).map(|i| Box::into_raw(Box::new(Elem::new(i as u32)))).collect();
    for i in 0..TICKS {
        lp.tick_boxq(elems[i]);
    }
    println!("boxq: other={}", lp.other);
    for e in elems {
        drop(unsafe { Box::from_raw(e) });
    }
}

// Number of ticks to drive (must be >= 2 so the init-tick's stub pointers are
// re-derefed on a LATER tick after a fresh whole-Loop reborrow).
const TICKS: usize = 4;

fn run_plain_naive() {
    let mut lp: Box<LPlain> = Box::new(LPlain::new());
    let elems: Vec<*mut Elem> =
        (0..TICKS).map(|i| Box::into_raw(Box::new(Elem::new(i as u32)))).collect();
    for i in 0..TICKS {
        lp.tick_naive(elems[i]);
    }
    println!("plain_naive: other={}", lp.other);
    for e in elems {
        drop(unsafe { Box::from_raw(e) });
    }
}

fn run_plain_selfptr() {
    let mut lp: Box<LPlain> = Box::new(LPlain::new());
    let elems: Vec<*mut Elem> =
        (0..TICKS).map(|i| Box::into_raw(Box::new(Elem::new(i as u32)))).collect();
    for i in 0..TICKS {
        lp.tick_selfptr(elems[i]);
    }
    println!("plain_selfptr: other={}", lp.other);
    for e in elems {
        drop(unsafe { Box::from_raw(e) });
    }
}

fn run_plain_rawtick() {
    let mut lp: Box<LPlain> = Box::new(LPlain::new());
    let this: *mut LPlain = &raw mut *lp; // ONE raw root, stable for the whole run
    let elems: Vec<*mut Elem> =
        (0..TICKS).map(|i| Box::into_raw(Box::new(Elem::new(i as u32)))).collect();
    for i in 0..TICKS {
        unsafe { LPlain::tick_raw(this, elems[i]) };
    }
    println!("plain_rawtick: other={}", unsafe { (*this).other });
    for e in elems {
        drop(unsafe { Box::from_raw(e) });
    }
    drop(lp);
}

fn run_cell_selfptr() {
    let mut lp: Box<LCell> = Box::new(LCell::new());
    let elems: Vec<*mut Elem> =
        (0..TICKS).map(|i| Box::into_raw(Box::new(Elem::new(i as u32)))).collect();
    for i in 0..TICKS {
        lp.tick_cell(elems[i]);
    }
    println!("cell_selfptr: other={}", lp.other);
    for e in elems {
        drop(unsafe { Box::from_raw(e) });
    }
}

// ===========================================================================
// CROSS-THREAD (M1 thread_perform): a pool thread PUSHES a completed completion
// into the queue while the loop thread POPS. A deterministic channel handshake
// sequences the two so there is a happens-before (no data race to mask the
// borrow verdict); Miri still checks each executed borrow op.
// ===========================================================================
use std::sync::mpsc::{channel, Receiver, Sender};

struct SendCell(*mut LCell);
unsafe impl Send for SendCell {}
struct SendBoxq(*mut LBoxq);
unsafe impl Send for SendBoxq {}
struct SendPlain(*mut LPlain);
unsafe impl Send for SendPlain {}

// NAIVE cross-thread: the pool thread forms `&mut (*lp).q` (exactly kqueue.rs:1394
// `&mut (*(*c).task_loop).thread_pool_completions`) — a unique reborrow of the
// queue formed on the pool thread, concurrent with the loop thread's tags.
fn run_xthread_naive() {
    let mut lp: Box<LPlain> = Box::new(LPlain::new());
    // init on the main/loop thread first (tick 0).
    let e0: *mut Elem = Box::into_raw(Box::new(Elem::new(0)));
    lp.tick_selfptr(e0); // establishes the stub pointers
    let this: *mut LPlain = &raw mut *lp;

    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel();
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel();
    let pushed: *mut Elem = Box::into_raw(Box::new(Elem::new(99)));
    let send = SendPlain(this);
    let sp = pushed as usize;
    let handle = std::thread::spawn(move || {
        let s = send;
        let _ = go_rx.recv();
        // NAIVE: whole `&mut (*lp).q` reborrow on the pool thread.
        unsafe { QPlain::push(&mut (*s.0).q, sp as *mut Elem) };
        ack_tx.send(1).unwrap();
    });
    go_tx.send(0).unwrap();
    let _ = ack_rx.recv();
    // loop thread drains
    let got = unsafe { QPlain::pop(ptr::addr_of_mut!((*this).q)) };
    handle.join().unwrap();
    println!("xthread_naive: got={}", if got.is_null() { u32::MAX } else { unsafe { (*got).val } });
    drop(unsafe { Box::from_raw(e0) });
    drop(unsafe { Box::from_raw(pushed) });
    drop(lp);
}

// SHIPPED cross-thread: the pool thread PUSHES through the SAME stable raw
// lineage (addr_of_mut / raw_get), never forming a `&mut` to the queue or Loop.
fn run_xthread_shipped() {
    let mut lp: Box<LCell> = Box::new(LCell::new());
    let e0: *mut Elem = Box::into_raw(Box::new(Elem::new(0)));
    lp.tick_cell(e0); // init + establish stub pointers on the loop thread
    let this: *mut LCell = &raw mut *lp;

    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel();
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel();
    let pushed: *mut Elem = Box::into_raw(Box::new(Elem::new(99)));
    let send = SendCell(this);
    let sp = pushed as usize;
    let handle = std::thread::spawn(move || {
        let s = send;
        let _ = go_rx.recv();
        // SHIPPED: raw addr_of_mut through the shared raw `*mut LCell`.
        unsafe { QCell::push(ptr::addr_of_mut!((*s.0).q), sp as *mut Elem) };
        ack_tx.send(1).unwrap();
    });
    go_tx.send(0).unwrap();
    let _ = ack_rx.recv();
    let got = unsafe { QCell::pop(ptr::addr_of_mut!((*this).q)) };
    handle.join().unwrap();
    println!("xthread_shipped: got={}", if got.is_null() { u32::MAX } else { unsafe { (*got).val } });
    drop(unsafe { Box::from_raw(e0) });
    drop(unsafe { Box::from_raw(pushed) });
    drop(lp);
}

// SHIPPED cross-thread modelling the EXACT applied kqueue.rs fix: the queue is
// heap-rooted (Box) with a cached raw `qptr` established at init on the loop
// thread; the pool thread (`thread_perform`) reads that cached raw off the Loop
// and pushes THROUGH it — never forming a `&mut` to the Loop or the queue —
// exactly `(*(*c).task_loop).thread_pool_completions_ptr`. Loop thread pops via
// the same cache. Sequenced by a channel handshake for a stable verdict.
fn run_xthread_boxq() {
    let mut lp: Box<LBoxq> = Box::new(LBoxq::new());
    let e0: *mut Elem = Box::into_raw(Box::new(Elem::new(0)));
    lp.tick_boxq(e0); // init: establishes qptr (heap-rooted cache) on loop thread
    let this: *mut LBoxq = &raw mut *lp;

    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel();
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel();
    let pushed: *mut Elem = Box::into_raw(Box::new(Elem::new(99)));
    let send = SendBoxq(this);
    let sp = pushed as usize;
    let handle = std::thread::spawn(move || {
        let s = send;
        let _ = go_rx.recv();
        // thread_perform: read the cached raw off the Loop, push through it.
        let qp: *mut QPlain = unsafe { (*s.0).qptr };
        unsafe { QPlain::push(qp, sp as *mut Elem) };
        ack_tx.send(1).unwrap();
    });
    go_tx.send(0).unwrap();
    let _ = ack_rx.recv();
    let qp: *mut QPlain = unsafe { (*this).qptr };
    let got = unsafe { QPlain::pop(qp) };
    handle.join().unwrap();
    println!("xthread_boxq: got={}", if got.is_null() { u32::MAX } else { unsafe { (*got).val } });
    drop(unsafe { Box::from_raw(e0) });
    drop(unsafe { Box::from_raw(pushed) });
    drop(lp);
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    match arg.as_str() {
        "plain_naive" => run_plain_naive(),
        "plain_selfptr" => run_plain_selfptr(),
        "plain_rawtick" => run_plain_rawtick(),
        "cell_selfptr" => run_cell_selfptr(),
        "inline_cached" => run_inline_cached(),
        "boxq" => run_boxq(),
        "xthread_naive" => run_xthread_naive(),
        "xthread_shipped" => run_xthread_shipped(),
        "xthread_boxq" => run_xthread_boxq(),
        other => {
            eprintln!("unknown candidate: {other:?}");
            eprintln!(
                "candidates: plain_naive plain_selfptr plain_rawtick cell_selfptr \
                 xthread_naive xthread_shipped"
            );
            std::process::exit(2);
        }
    }
}
