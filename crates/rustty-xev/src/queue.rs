//! queue.rs — faithful port of libxev `queue.zig` (D6, XEV_BLUEPRINT §8 chunk C1).
//!
//! An intrusive singly-linked FIFO of `Completion` (raw-ptr head/tail push/pop/
//! empty). This is one of the three pure intrusive data structures (queue /
//! queue_mpsc / heap) that establish the raw-pointer discipline for the whole xev
//! port. No syscalls; Miri-clean.
//
// PORT NOTE: the Zig reference is `fn Intrusive(comptime T: type) type` where `T`
// "must have a field 'next' of type `?*T`" (comptime field access). Rust cannot do
// generic field access, so the intrusive `next` link is reached through the `Node`
// trait (the Rust seam for that comptime requirement) — exactly as
// rustty-datastruct's intrusive_linked_list models its Zig node fields via a `Node`
// trait. The link is a RAW pointer end-to-end (never `&mut`/lifetimes): the loop
// holds `*mut Completion` into caller memory and re-enters it across ticks, so a
// `&mut Completion` re-read across a tick would be Stacked-Borrows UB (memory
// `rust-mut-args-strongly-protected`).

use core::ptr;

/// Trait for the intrusive `next` link. The implementing type must store a
/// "next" field of type `*mut Self` (Zig `?*T`, with null meaning none);
/// `next_field` exposes that field's ADDRESS so the queue can read/write the
/// link through a raw pointer without ever forming a `&mut` to the element
/// (Stacked-Borrows safe). This is the Rust seam for the Zig requirement that
/// `T` "must have a field 'next' of type `?*T`".
pub trait Node {
    /// Address of the intrusive `next` field of the element at `this`.
    ///
    /// # Safety
    /// `this` must be a valid, dereferenceable pointer to `Self`.
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self;
}

/// An intrusive queue implementation. The type T must have a field
/// "next" of type `?*T`.
///
/// For those unaware, an intrusive variant of a data structure is one in which
/// the data type in the list has the pointer to the next element, rather
/// than a higher level "node" or "container" type. The primary benefit
/// of this (and the reason we implement this) is that it defers all memory
/// management to the caller: the data structure implementation doesn't need
/// to allocate "nodes" to contain each element. Instead, the caller provides
/// the element and how its allocated is up to them.
pub struct Intrusive<T: Node> {
    /// Head is the front of the queue and tail is the back of the queue.
    pub head: *mut T,
    pub tail: *mut T,
}

// PORT NOTE [ADV-FIX F3]: hand-written `Default` (head/tail = null), NOT
// `#[derive(Default)]` — `*mut T` has no `Default` impl, and the Zig field
// defaults are `head: ?*T = null, tail: ?*T = null`. This is load-bearing:
// termio default-constructs `WriteQueue = queue::Intrusive<WriteRequest>` (`.{}`).
impl<T: Node> Default for Intrusive<T> {
    fn default() -> Self {
        Self {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
        }
    }
}

// PORT NOTE [ADV-FIX F2]: the intrusive raw-ptr queue is auto-`!Send`; ghostty
// MOVES a `Loop` (which owns these queues) onto the termio thread. SAFETY /
// single-thread affinity: a queue is touched by exactly one thread at a time —
// the owning event-loop thread — after the move, so transferring ownership is
// sound.
unsafe impl<T: Node> Send for Intrusive<T> {}

impl<T: Node> Intrusive<T> {
    /// Enqueue a new element to the back of the queue.
    ///
    /// # Safety
    /// `v` must be a valid, dereferenceable pointer that is not already a member
    /// of any queue (its `next` link must be null).
    pub unsafe fn push(&mut self, v: *mut T) {
        // assert(v.next == null) — ports as `debug_assert` (a runtime-safety-only
        // check, matching Zig's `std.debug.assert`).
        debug_assert!(unsafe { T::next_field(v).read() }.is_null());

        if !self.tail.is_null() {
            // If we have elements in the queue, then we add a new tail.
            unsafe { T::next_field(self.tail).write(v) };
            self.tail = v;
        } else {
            // No elements in the queue we setup the initial state.
            self.head = v;
            self.tail = v;
        }
    }

    /// Dequeue the next element from the queue.
    ///
    /// # Safety
    /// The queue and every element still linked into it must be valid,
    /// dereferenceable pointers.
    pub unsafe fn pop(&mut self) -> *mut T {
        // The next element is in "head".
        let next = self.head;
        if next.is_null() {
            return ptr::null_mut();
        }

        // If the head and tail are equal this is the last element
        // so we also set tail to null so we can now be empty.
        if self.head == self.tail {
            self.tail = ptr::null_mut();
        }

        // Head is whatever is next (if we're the last element,
        // this will be null);
        self.head = unsafe { T::next_field(next).read() };

        // We set the "next" field to null so that this element
        // can be inserted again.
        unsafe { T::next_field(next).write(ptr::null_mut()) };
        next
    }

    /// Returns true if the queue is empty.
    pub fn empty(&self) -> bool {
        self.head.is_null()
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
        // var q: Queue = .{};
        let mut q: Intrusive<Elem> = Intrusive::default();
        // try testing.expect(q.empty());
        assert!(q.empty());

        // Elems
        // var elems: [10]Elem = .{Elem{}} ** 10;
        //
        // PORT/MIRI NOTE: the Zig test declares `elems` as a stack array and passes
        // `&elems[i]` to the queue. Under Stacked Borrows, re-taking `&mut elems[i]`
        // for a later op invalidates the raw pointer the queue already stored. To
        // stay UB-free under Miri while preserving the exact intrusive-FIFO behavior
        // we heap-allocate each element once via `Box::into_raw` and pass the SAME
        // stable raw pointer to every op (the canonical Rust intrusive idiom; see
        // rustty-datastruct's DoublyLinkedList).
        let elems: [*mut Elem; 10] =
            core::array::from_fn(|_| Box::into_raw(Box::new(Elem::default())));

        unsafe {
            // One
            // try testing.expect(q.pop() == null);
            assert!(q.pop().is_null());
            // q.push(&elems[0]);
            q.push(elems[0]);
            // try testing.expect(!q.empty());
            assert!(!q.empty());
            // try testing.expect(q.pop().? == &elems[0]);
            assert_eq!(q.pop(), elems[0]);
            // try testing.expect(q.pop() == null);
            assert!(q.pop().is_null());
            // try testing.expect(q.empty());
            assert!(q.empty());

            // Two
            // try testing.expect(q.pop() == null);
            assert!(q.pop().is_null());
            // q.push(&elems[0]);
            q.push(elems[0]);
            // q.push(&elems[1]);
            q.push(elems[1]);
            // try testing.expect(q.pop().? == &elems[0]);
            assert_eq!(q.pop(), elems[0]);
            // try testing.expect(q.pop().? == &elems[1]);
            assert_eq!(q.pop(), elems[1]);
            // try testing.expect(q.pop() == null);
            assert!(q.pop().is_null());

            // Interleaved
            // try testing.expect(q.pop() == null);
            assert!(q.pop().is_null());
            // q.push(&elems[0]);
            q.push(elems[0]);
            // try testing.expect(q.pop().? == &elems[0]);
            assert_eq!(q.pop(), elems[0]);
            // q.push(&elems[1]);
            q.push(elems[1]);
            // try testing.expect(q.pop().? == &elems[1]);
            assert_eq!(q.pop(), elems[1]);
            // try testing.expect(q.pop() == null);
            assert!(q.pop().is_null());

            // Free the heap-allocated elems.
            for e in elems {
                drop(Box::from_raw(e));
            }
        }
    }
}
