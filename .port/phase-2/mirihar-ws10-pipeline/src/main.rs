//! Isolated, **non-FFI** Miri harness for the WS10 T20 pty read pipeline
//! (upstream `2f0e6659d` + `bb0ac4c72`, `crates/rustty-termio/src/exec.rs`).
//!
//! The upstream `Pipeline` is a 4-slot x 64 KiB ring whose stated invariant is:
//!
//!   "A buffer is owned by exactly one stage at a time, so buffer contents need
//!    no locking. Only the ring metadata is guarded by the mutex."
//!
//! In Zig that is a comment. In Rust it is an ALIASING CLAIM about one
//! allocation with two live threads inside it, and it is the whole risk of the
//! task: the gather stage writes slot `head` while the parse stage reads slot
//! `tail`, concurrently, out of the same `bufs` allocation.
//!
//! This harness models exactly that: two real threads, the same mutex + two
//! condvars + `count`/`head`/`tail`/`lens`/`done` protocol, a fake producer and
//! consumer, and NO `poll`/`read`/fd of any kind (Miri cannot run those). Each
//! candidate is one executed path selected by **argv** — deliberately not an
//! env var, because Miri does not forward the host environment, so an env
//! switch would silently collapse every candidate onto the default and make the
//! negative controls vacuous.
//!
//! Run (from this dir):
//!
//! ```sh
//! for c in shipped naive_mutslice naive_copyfield; do
//!   echo "== $c =="
//!   cargo +nightly miri run --quiet -- "$c"                          # Stacked Borrows
//!   MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" # Tree Borrows
//! done
//! ```
//!
//! `shipped` must be CLEAN under both. `naive_mutslice` and `naive_copyfield`
//! are the negative controls and must be UB under both — if a negative control
//! ever goes clean, this harness has stopped proving anything and the shipped
//! candidate's result is worthless.
#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

/// Scaled down from 4 x 64 KiB: Miri is ~100x slower than native, and the
/// aliasing structure is independent of the sizes.
const BUFFER_COUNT: usize = 4;
const BUFFER_CAPACITY: usize = 64;
const BATCHES: usize = 64;

/// The ring metadata. Everything here is guarded by `Pipeline::mutex`.
#[derive(Default)]
struct Meta {
    lens: [usize; BUFFER_COUNT],
    head: usize,
    tail: usize,
    count: usize,
    bridging: bool,
    done: bool,
}

/// The shared pipeline: guarded metadata plus the UNGUARDED buffer storage.
///
/// `bufs` is an `UnsafeCell` because the invariant under test is precisely that
/// its contents are NOT protected by the mutex — slot ownership is, and the
/// contents follow from it. Rust needs the interior-mutability marker to make
/// that legal at all; the marker alone does not make it SOUND, which is what
/// Miri is here to decide.
struct Pipeline {
    mutex: Mutex<Meta>,
    batch_ready: Condvar,
    slot_free: Condvar,
    bufs: std::cell::UnsafeCell<[[u8; BUFFER_CAPACITY]; BUFFER_COUNT]>,
}

// SAFETY (the claim under test): a slot is owned by exactly one stage at a time.
// The gather stage owns slot `head` from the moment it is claimed under the
// mutex until it is published under the mutex; the parse stage owns slot `tail`
// from the moment it is taken under the mutex until the tail is advanced under
// the mutex. Those windows are disjoint per slot, and the mutex acquire/release
// pairs that transfer ownership are the happens-before edges.
unsafe impl Sync for Pipeline {}

impl Pipeline {
    fn new() -> Pipeline {
        Pipeline {
            mutex: Mutex::new(Meta::default()),
            batch_ready: Condvar::new(),
            slot_free: Condvar::new(),
            bufs: std::cell::UnsafeCell::new([[0u8; BUFFER_CAPACITY]; BUFFER_COUNT]),
        }
    }

    /// The base address of slot `i`, derived from the whole-array raw pointer
    /// WITHOUT ever forming a reference to the array or to any other slot.
    ///
    /// This is the load-bearing helper. `UnsafeCell::get` yields a `*mut` to the
    /// whole storage; `cast::<[u8; CAP]>().add(i)` walks to the slot with
    /// pointer arithmetic only. Note the deliberate absence of
    /// `(*self.bufs.get())[i]` — that indexes THROUGH a place expression, which
    /// materializes a reference to the whole array and would put the other
    /// stage's live slot inside our tag's range.
    #[inline]
    fn slot_ptr(&self, i: usize) -> *mut u8 {
        let base: *mut [u8; BUFFER_CAPACITY] = self.bufs.get().cast();
        // SAFETY: `i < BUFFER_COUNT`, so this stays inside the allocation.
        unsafe { base.add(i).cast::<u8>() }
    }
}

/// A fake producer: hands out deterministic bytes so the consumer can checksum.
struct Producer {
    n: usize,
    total: usize,
}

impl Producer {
    /// Models one `read(2)` into `dst[..cap]`, returning the byte count.
    /// Deliberately takes a RAW pointer, never `&mut [u8]`: that is the shape
    /// the real gather stage must use.
    unsafe fn read_into(&mut self, dst: *mut u8, cap: usize) -> usize {
        if self.n >= self.total {
            return 0;
        }
        let want = core::cmp::min(cap, 13); // a partial read, like a real pty
        let take = core::cmp::min(want, self.total - self.n);
        for k in 0..take {
            // SAFETY: `k < cap` and `dst` is the base of a `cap`-byte slot.
            unsafe { dst.add(k).write((self.n + k) as u8) };
        }
        self.n += take;
        take
    }
}

static CHECKSUM: AtomicUsize = AtomicUsize::new(0);
static CONSUMED: AtomicUsize = AtomicUsize::new(0);

// ---------------------------------------------------------------------------
// Candidate: `shipped` — raw-pointer slot handoff (what exec.rs implements)
// ---------------------------------------------------------------------------

fn gather_shipped(p: &Pipeline, prod: &mut Producer) {
    loop {
        // Claim the next free slot. Only the METADATA is read under the lock;
        // the slot INDEX is copied out and the guard is dropped before we touch
        // the buffer, so no reference derived from the guard is live while we
        // write.
        let slot = {
            let mut m = p.mutex.lock().unwrap();
            while m.count == BUFFER_COUNT {
                m = p.slot_free.wait(m).unwrap();
            }
            m.head
        };

        // Fill the slot. `dst` is derived from the BASE pointer, so its
        // provenance covers the whole `bufs` allocation but the tag is fresh and
        // no reference to any other slot exists. The parse stage is concurrently
        // reading a DIFFERENT slot through its own such pointer.
        let dst = p.slot_ptr(slot);
        let mut total = 0usize;
        while total < BUFFER_CAPACITY {
            // SAFETY: we own `slot` until we publish it below.
            let n = unsafe { prod.read_into(dst.add(total), BUFFER_CAPACITY - total) };
            if n == 0 {
                break;
            }
            total += n;
        }

        if total > 0 {
            let mut m = p.mutex.lock().unwrap();
            m.lens[slot] = total;
            m.head = (m.head + 1) % BUFFER_COUNT;
            m.count += 1;
            drop(m);
            p.batch_ready.notify_one();
        }

        if total < BUFFER_CAPACITY {
            // Producer exhausted.
            let mut m = p.mutex.lock().unwrap();
            m.done = true;
            drop(m);
            p.batch_ready.notify_one();
            return;
        }
    }
}

fn parse_shipped(p: &Pipeline) {
    loop {
        let (slot, len) = {
            let mut m = p.mutex.lock().unwrap();
            while m.count == 0 {
                if m.done {
                    return;
                }
                m = p.batch_ready.wait(m).unwrap();
            }
            (m.tail, m.lens[m.tail])
        };

        // Read the batch OUTSIDE the lock, through a raw pointer to our slot
        // only. The gather stage is concurrently writing a different slot.
        let src = p.slot_ptr(slot);
        let mut sum = 0usize;
        for k in 0..len {
            // SAFETY: we own `slot` until the tail advance below.
            sum += unsafe { src.add(k).read() } as usize;
        }
        CHECKSUM.fetch_add(sum, Ordering::Relaxed);
        CONSUMED.fetch_add(len, Ordering::Relaxed);

        {
            let mut m = p.mutex.lock().unwrap();
            m.tail = (m.tail + 1) % BUFFER_COUNT;
            m.count -= 1;
            drop(m);
            p.slot_free.notify_one();
        }
    }
}

// ---------------------------------------------------------------------------
// Negative control 1: `naive_mutslice` — the author's own first draft.
//
// This is what the Zig reads like when transliterated directly: claim the slot
// and hand the stage a `&mut [u8]` (`break :buf &pipeline.bufs[pipeline.head]`
// really is a pointer to an element of the array). Forming `&mut (*bufs)[i]`
// goes through the whole-array place, so the reborrow's tag covers the WHOLE
// `bufs` allocation; the other stage's concurrent access to its own slot then
// invalidates it. MUST be UB.
// ---------------------------------------------------------------------------

fn gather_naive_mutslice(p: &Pipeline, prod: &mut Producer) {
    loop {
        let slot = {
            let mut m = p.mutex.lock().unwrap();
            while m.count == BUFFER_COUNT {
                m = p.slot_free.wait(m).unwrap();
            }
            m.head
        };

        // THE BUG: a `&mut [u8; CAP]` obtained by indexing the whole array.
        // SAFETY: deliberately unsound — this is the negative control.
        let buf: &mut [u8; BUFFER_CAPACITY] = unsafe { &mut (*p.bufs.get())[slot] };
        let mut total = 0usize;
        while total < BUFFER_CAPACITY {
            let n = unsafe { prod.read_into(buf.as_mut_ptr().add(total), BUFFER_CAPACITY - total) };
            if n == 0 {
                break;
            }
            total += n;
        }

        if total > 0 {
            let mut m = p.mutex.lock().unwrap();
            m.lens[slot] = total;
            m.head = (m.head + 1) % BUFFER_COUNT;
            m.count += 1;
            drop(m);
            p.batch_ready.notify_one();
        }

        if total < BUFFER_CAPACITY {
            let mut m = p.mutex.lock().unwrap();
            m.done = true;
            drop(m);
            p.batch_ready.notify_one();
            return;
        }
    }
}

fn parse_naive_mutslice(p: &Pipeline) {
    loop {
        let (slot, len) = {
            let mut m = p.mutex.lock().unwrap();
            while m.count == 0 {
                if m.done {
                    return;
                }
                m = p.batch_ready.wait(m).unwrap();
            }
            (m.tail, m.lens[m.tail])
        };

        // THE BUG (mirror image): a shared slice through the whole-array place.
        // SAFETY: deliberately unsound — this is the negative control.
        let batch: &[u8] = unsafe { &(&(*p.bufs.get())[slot])[..len] };
        let mut sum = 0usize;
        for &b in batch {
            sum += b as usize;
        }
        CHECKSUM.fetch_add(sum, Ordering::Relaxed);
        CONSUMED.fetch_add(len, Ordering::Relaxed);

        {
            let mut m = p.mutex.lock().unwrap();
            m.tail = (m.tail + 1) % BUFFER_COUNT;
            m.count -= 1;
            drop(m);
            p.slot_free.notify_one();
        }
    }
}

// ---------------------------------------------------------------------------
// Negative control 2: `naive_copyfield` — the "obvious cure" that is still UB.
//
// The tempting fix is "don't hold a reference, just COPY the slot bytes out by
// value and drop the reference immediately". It does not help: a by-value read
// of a `Copy` field is still a foreign ACCESS on the owning allocation, so it
// invalidates the other stage's live tag just as a write would. This candidate
// copies the WHOLE `bufs` array by value (`*p.bufs.get()`), which is exactly
// that mistake at full size, and MUST be UB.
// ---------------------------------------------------------------------------

fn gather_naive_copyfield(p: &Pipeline, prod: &mut Producer) {
    gather_shipped(p, prod)
}

fn parse_naive_copyfield(p: &Pipeline) {
    loop {
        let (slot, len) = {
            let mut m = p.mutex.lock().unwrap();
            while m.count == 0 {
                if m.done {
                    return;
                }
                m = p.batch_ready.wait(m).unwrap();
            }
            (m.tail, m.lens[m.tail])
        };

        // THE BUG: "just copy it, we hold no reference afterwards."
        // SAFETY: deliberately unsound — this is the negative control.
        let snapshot: [[u8; BUFFER_CAPACITY]; BUFFER_COUNT] = unsafe { *p.bufs.get() };
        let mut sum = 0usize;
        for k in 0..len {
            sum += snapshot[slot][k] as usize;
        }
        CHECKSUM.fetch_add(sum, Ordering::Relaxed);
        CONSUMED.fetch_add(len, Ordering::Relaxed);

        {
            let mut m = p.mutex.lock().unwrap();
            m.tail = (m.tail + 1) % BUFFER_COUNT;
            m.count -= 1;
            drop(m);
            p.slot_free.notify_one();
        }
    }
}

// ---------------------------------------------------------------------------
// Negative control 3: `naive_wholeref` — the literal transliteration.
//
// Zig's gather stage says `break :buf &pipeline.bufs[pipeline.head]`. Written
// out in Rust by hand that becomes "take the buffer array, then index it":
//
//     let bufs = unsafe { &mut *p.bufs.get() };   // <- the whole array
//     let buf  = &mut bufs[slot];
//
// which is the natural thing to write and is UNSOUND: `bufs` is a `&mut` whose
// tag spans EVERY slot, including the one the parse stage is concurrently
// reading. MUST be UB.
// ---------------------------------------------------------------------------

fn gather_naive_wholeref(p: &Pipeline, prod: &mut Producer) {
    // THE BUG: one long-lived `&mut` over the whole buffer storage, hoisted out
    // of the loop exactly as a reader of the Zig would hoist `&pipeline.bufs`.
    // SAFETY: deliberately unsound — this is the negative control.
    let bufs: &mut [[u8; BUFFER_CAPACITY]; BUFFER_COUNT] = unsafe { &mut *p.bufs.get() };
    loop {
        let slot = {
            let mut m = p.mutex.lock().unwrap();
            while m.count == BUFFER_COUNT {
                m = p.slot_free.wait(m).unwrap();
            }
            m.head
        };

        let buf = &mut bufs[slot];
        let mut total = 0usize;
        while total < BUFFER_CAPACITY {
            let n = unsafe { prod.read_into(buf.as_mut_ptr().add(total), BUFFER_CAPACITY - total) };
            if n == 0 {
                break;
            }
            total += n;
        }

        if total > 0 {
            let mut m = p.mutex.lock().unwrap();
            m.lens[slot] = total;
            m.head = (m.head + 1) % BUFFER_COUNT;
            m.count += 1;
            drop(m);
            p.batch_ready.notify_one();
        }

        if total < BUFFER_CAPACITY {
            let mut m = p.mutex.lock().unwrap();
            m.done = true;
            drop(m);
            p.batch_ready.notify_one();
            return;
        }
    }
}

// ---------------------------------------------------------------------------

fn run(candidate: &str) {
    let p = Pipeline::new();
    let total = BATCHES * BUFFER_CAPACITY;
    let mut prod = Producer { n: 0, total };

    let expect: usize = (0..total).map(|i| (i % 256) as usize).sum();

    std::thread::scope(|s| {
        let pr = &p;
        s.spawn(move || match candidate {
            "shipped" => parse_shipped(pr),
            "naive_mutslice" => parse_naive_mutslice(pr),
            "naive_copyfield" => parse_naive_copyfield(pr),
            "naive_wholeref" => parse_shipped(pr),
            other => panic!("unknown candidate {other}"),
        });
        match candidate {
            "shipped" => gather_shipped(&p, &mut prod),
            "naive_mutslice" => gather_naive_mutslice(&p, &mut prod),
            "naive_copyfield" => gather_naive_copyfield(&p, &mut prod),
            "naive_wholeref" => gather_naive_wholeref(&p, &mut prod),
            other => panic!("unknown candidate {other}"),
        }
    });

    // Correctness, not just soundness: every produced byte must be consumed
    // exactly once and in order. A harness that is clean but drops data would
    // be proving the wrong thing.
    let consumed = CONSUMED.load(Ordering::Relaxed);
    let sum = CHECKSUM.load(Ordering::Relaxed);
    assert_eq!(consumed, total, "{candidate}: byte count");
    assert_eq!(sum, expect, "{candidate}: checksum");
    println!("{candidate}: OK ({consumed} bytes, checksum {sum})");
}

fn main() {
    // ARGV, not an env var: Miri does not forward the host environment, so an
    // env switch would collapse every candidate onto one and make the negative
    // controls vacuous.
    let candidate = std::env::args().nth(1).unwrap_or_else(|| "shipped".into());
    run(&candidate);
}
