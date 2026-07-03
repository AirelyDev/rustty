//! queue_mpsc.rs — faithful port of libxev `queue_mpsc.zig` (D6, XEV_BLUEPRINT
//! §8 chunk C1).
//!
//! The Vyukov MPSC queue (thread-pool completions). Needed for the `Completion`
//! type refs even with the pool deferred. Pure atomics; no syscalls.
//
// PORT NOTE: like `queue.rs`, the Zig `T` "must have a field 'next' of type `?*T`"
// requirement is bridged by a `Node` trait exposing the ADDRESS of the intrusive
// `next` field. The FIFO here is lock-free: `queue.rs` accesses `next` plainly,
// this file accesses the SAME field-shape ATOMICALLY (via `AtomicPtr::from_ptr`
// over the field address). Zig's `.unordered` atomic ordering has no Rust
// equivalent (LLVM `unordered` is weaker than `monotonic`/Relaxed); we map it to
// `Ordering::Relaxed`, the closest faithful ordering (strictly stronger, never
// weaker — R5-safe). The queue is self-referential (`head`/`tail` point at the
// embedded `stub`), so all methods take a raw `*mut Self` (Zig `*Self`) to keep
// allocation-wide pointer provenance; a `&mut self` receiver would invalidate the
// stored stub pointer under Stacked Borrows.

use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

/// Trait for the intrusive `next` link (see `queue::Node`). The implementing
/// type must store a "next" field of type `*mut Self` (Zig `?*T`); `next_field`
/// exposes its address for atomic access.
pub trait Node {
    /// Address of the intrusive `next` field of the element at `this`.
    ///
    /// # Safety
    /// `this` must be a valid, dereferenceable pointer to `Self`.
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self;
}

/// An intrusive MPSC (multi-provider, single consumer) queue implementation.
/// The type T must have a field "next" of type `?*T`.
///
/// This is an implementatin of a Vyukov Queue[1].
/// TODO(mitchellh): I haven't audited yet if I got all the atomic operations
/// correct. I was short term more focused on getting something that seemed
/// to work; I need to make sure it actually works.
///
/// For those unaware, an intrusive variant of a data structure is one in which
/// the data type in the list has the pointer to the next element, rather
/// than a higher level "node" or "container" type. The primary benefit
/// of this (and the reason we implement this) is that it defers all memory
/// management to the caller: the data structure implementation doesn't need
/// to allocate "nodes" to contain each element. Instead, the caller provides
/// the element and how its allocated is up to them.
///
/// [1]: https://www.1024cores.net/home/lock-free-algorithms/queues/intrusive-mpsc-node-based-queue
pub struct Intrusive<T: Node> {
    /// Head is the front of the queue and tail is the back of the queue.
    pub head: *mut T,
    pub tail: *mut T,
    pub stub: T,
}

// PORT NOTE [ADV-FIX F2]: the intrusive raw-ptr queue is auto-`!Send`; ghostty
// MOVES the owning `Loop` onto the termio thread. SAFETY / single-thread affinity:
// producers `push` from any thread but the whole queue's lifecycle is owned by the
// one event-loop thread that constructs and consumes it (the "single consumer" of
// MPSC), so transferring ownership is sound.
unsafe impl<T: Node> Send for Intrusive<T> {}

impl<T: Node> Intrusive<T> {
    /// Create the queue with its `stub` sentinel in place. Rust glue: Zig uses
    /// `var q: Queue = undefined; q.init();`, but Rust has no `undefined`, so we
    /// place a valid `stub` (and null head/tail) here and let `init` wire the
    /// self-referential stub pointers.
    pub fn new(stub: T) -> Self {
        Self {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
            stub,
        }
    }

    /// Initialize the queue. This requires a stable pointer to itself.
    /// This must be called before the queue is used concurrently.
    ///
    /// # Safety
    /// `this` must be a valid pointer to a `Self` that will NOT move for the
    /// lifetime of the queue (the stub is self-referenced).
    pub unsafe fn init(this: *mut Self) {
        unsafe {
            let stub: *mut T = ptr::addr_of_mut!((*this).stub);
            // self.head = &self.stub;
            ptr::addr_of_mut!((*this).head).write(stub);
            // self.tail = &self.stub;
            ptr::addr_of_mut!((*this).tail).write(stub);
            // self.stub.next = null;
            T::next_field(stub).write(ptr::null_mut());
        }
    }

    /// Push an item onto the queue. This can be called by any number
    /// of producers.
    ///
    /// # Safety
    /// `this` must be a valid, initialized queue pointer and `v` a valid element
    /// pointer not already enqueued.
    pub unsafe fn push(this: *mut Self, v: *mut T) {
        unsafe {
            // @atomicStore(?*T, &v.next, null, .unordered);
            AtomicPtr::from_ptr(T::next_field(v)).store(ptr::null_mut(), Ordering::Relaxed);
            // const prev = @atomicRmw(*T, &self.head, .Xchg, v, .acq_rel);
            let prev =
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).head)).swap(v, Ordering::AcqRel);
            // @atomicStore(?*T, &prev.next, v, .release);
            AtomicPtr::from_ptr(T::next_field(prev)).store(v, Ordering::Release);
        }
    }

    /// Pop the first in element from the queue. This must be called
    /// by only a single consumer at any given time.
    ///
    /// # Safety
    /// `this` must be a valid, initialized queue pointer and this must be the
    /// only consumer running concurrently.
    pub unsafe fn pop(this: *mut Self) -> *mut T {
        unsafe {
            // var tail = @atomicLoad(*T, &self.tail, .unordered);
            let mut tail =
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).tail)).load(Ordering::Relaxed);
            // var next_ = @atomicLoad(?*T, &tail.next, .acquire);
            let mut next_ = AtomicPtr::from_ptr(T::next_field(tail)).load(Ordering::Acquire);
            // if (tail == &self.stub) {
            let stub: *mut T = ptr::addr_of_mut!((*this).stub);
            if tail == stub {
                // const next = next_ orelse return null;
                if next_.is_null() {
                    return ptr::null_mut();
                }
                let next = next_;
                // @atomicStore(*T, &self.tail, next, .unordered);
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).tail)).store(next, Ordering::Relaxed);
                // tail = next;
                tail = next;
                // next_ = @atomicLoad(?*T, &tail.next, .acquire);
                next_ = AtomicPtr::from_ptr(T::next_field(tail)).load(Ordering::Acquire);
            }

            // if (next_) |next| {
            if !next_.is_null() {
                let next = next_;
                // @atomicStore(*T, &self.tail, next, .release);
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).tail)).store(next, Ordering::Release);
                // tail.next = null;
                T::next_field(tail).write(ptr::null_mut());
                // return tail;
                return tail;
            }

            // const head = @atomicLoad(*T, &self.head, .unordered);
            let head = AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).head)).load(Ordering::Relaxed);
            // if (tail != head) return null;
            if tail != head {
                return ptr::null_mut();
            }
            // self.push(&self.stub);
            Self::push(this, stub);

            // next_ = @atomicLoad(?*T, &tail.next, .acquire);
            next_ = AtomicPtr::from_ptr(T::next_field(tail)).load(Ordering::Acquire);
            // if (next_) |next| {
            if !next_.is_null() {
                let next = next_;
                // @atomicStore(*T, &self.tail, next, .unordered);
                AtomicPtr::from_ptr(ptr::addr_of_mut!((*this).tail)).store(next, Ordering::Relaxed);
                // tail.next = null;
                T::next_field(tail).write(ptr::null_mut());
                // return tail;
                return tail;
            }

            // return null;
            ptr::null_mut()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Types
    struct Elem {
        // next: ?*Self = null,
        next: *mut Elem,
    }

    impl Default for Elem {
        fn default() -> Self {
            Elem {
                next: ptr::null_mut(),
            }
        }
    }

    impl Node for Elem {
        unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
            unsafe { ptr::addr_of_mut!((*this).next) }
        }
    }

    // test Intrusive
    #[test]
    fn intrusive() {
        // const Queue = Intrusive(Elem);
        // var q: Queue = undefined;
        // q.init();
        //
        // PORT/MIRI NOTE: the mpsc queue is self-referential (head/tail point at the
        // embedded `stub`), so it needs a STABLE address. We Box it and drive every
        // op through the SAME `*mut Self` (allocation-wide provenance), never through
        // `&mut self` — a fresh `&mut self` per call would invalidate the stored stub
        // pointer under Stacked Borrows. This mirrors the Zig contract ("This
        // requires a stable pointer to itself").
        let q: *mut Intrusive<Elem> = Box::into_raw(Box::new(Intrusive::new(Elem::default())));
        unsafe { Intrusive::init(q) };

        // Elems
        // var elems: [10]Elem = .{Elem{}} ** 10;
        let elems: [*mut Elem; 10] =
            core::array::from_fn(|_| Box::into_raw(Box::new(Elem::default())));

        unsafe {
            // One
            // try testing.expect(q.pop() == null);
            assert!(Intrusive::pop(q).is_null());
            // q.push(&elems[0]);
            Intrusive::push(q, elems[0]);
            // try testing.expect(q.pop().? == &elems[0]);
            assert_eq!(Intrusive::pop(q), elems[0]);
            // try testing.expect(q.pop() == null);
            assert!(Intrusive::pop(q).is_null());

            // Two
            // try testing.expect(q.pop() == null);
            assert!(Intrusive::pop(q).is_null());
            // q.push(&elems[0]);
            Intrusive::push(q, elems[0]);
            // q.push(&elems[1]);
            Intrusive::push(q, elems[1]);
            // try testing.expect(q.pop().? == &elems[0]);
            assert_eq!(Intrusive::pop(q), elems[0]);
            // try testing.expect(q.pop().? == &elems[1]);
            assert_eq!(Intrusive::pop(q), elems[1]);
            // try testing.expect(q.pop() == null);
            assert!(Intrusive::pop(q).is_null());

            // // Interleaved
            // try testing.expect(q.pop() == null);
            assert!(Intrusive::pop(q).is_null());
            // q.push(&elems[0]);
            Intrusive::push(q, elems[0]);
            // try testing.expect(q.pop().? == &elems[0]);
            assert_eq!(Intrusive::pop(q), elems[0]);
            // q.push(&elems[1]);
            Intrusive::push(q, elems[1]);
            // try testing.expect(q.pop().? == &elems[1]);
            assert_eq!(Intrusive::pop(q), elems[1]);
            // try testing.expect(q.pop() == null);
            assert!(Intrusive::pop(q).is_null());

            // Free the heap-allocated elems + queue.
            for e in elems {
                drop(Box::from_raw(e));
            }
            drop(Box::from_raw(q));
        }
    }
}
