//! heap.rs — faithful port of libxev `heap.zig` (D6, XEV_BLUEPRINT §8 chunk C1).
//!
//! The intrusive pairing heap = the xev **timer heap**. Pure pointer surgery; no
//! syscalls; Miri-clean.
//
// PORT NOTE: the Zig reference is `fn Intrusive(comptime T, comptime Context,
// comptime less) type`, where `T` "must have a field 'heap' of type
// InstrusiveHeapField". Rust cannot do generic field access, so the `heap` field
// is reached through the `Node` trait (the Rust seam for that requirement), and
// the comptime `less` comparator — passed as a comptime `*const fn(ctx, a, b)
// bool` in Zig — is carried as a plain fn-pointer field (identical behavior, one
// indirection). Every node link is a RAW pointer end-to-end (never `&mut`): the
// heap stores caller-owned node pointers and mutates their `heap` sub-fields
// through raw-pointer field access (`addr_of!`/`addr_of_mut!`), never forming a
// `&mut` to a node (Stacked-Borrows safe; memory `rust-mut-args-strongly-protected`).

use core::ptr;

/// Trait for the intrusive `heap` field. The implementing type must store a
/// "heap" field of type `IntrusiveField<Self>`; `heap_field` exposes its address
/// so the heap can read/write the child/prev/next links through raw pointers.
/// This is the Rust seam for the Zig requirement that the element T "is expected
/// to have a field 'heap' of type InstrusiveHeapField".
pub trait Node: Sized {
    /// Address of the intrusive `heap` field of the element at `this`.
    ///
    /// # Safety
    /// `this` must be a valid, dereferenceable pointer to `Self`.
    unsafe fn heap_field(this: *mut Self) -> *mut IntrusiveField<Self>;
}

// Raw-pointer accessors for the three `IntrusiveField` links. These compute the
// field address via `addr_of!`/`addr_of_mut!` and read/write the pointer-sized
// link WITHOUT forming a `&mut` to the node (Stacked-Borrows safe). They mirror
// Zig's `v.heap.child` / `v.heap.prev` / `v.heap.next` place expressions.

#[inline]
unsafe fn get_child<T: Node>(v: *mut T) -> *mut T {
    unsafe { ptr::addr_of!((*T::heap_field(v)).child).read() }
}
#[inline]
unsafe fn set_child<T: Node>(v: *mut T, x: *mut T) {
    unsafe { ptr::addr_of_mut!((*T::heap_field(v)).child).write(x) };
}
#[inline]
unsafe fn get_prev<T: Node>(v: *mut T) -> *mut T {
    unsafe { ptr::addr_of!((*T::heap_field(v)).prev).read() }
}
#[inline]
unsafe fn set_prev<T: Node>(v: *mut T, x: *mut T) {
    unsafe { ptr::addr_of_mut!((*T::heap_field(v)).prev).write(x) };
}
#[inline]
unsafe fn get_next<T: Node>(v: *mut T) -> *mut T {
    unsafe { ptr::addr_of!((*T::heap_field(v)).next).read() }
}
#[inline]
unsafe fn set_next<T: Node>(v: *mut T, x: *mut T) {
    unsafe { ptr::addr_of_mut!((*T::heap_field(v)).next).write(x) };
}

/// An intrusive heap implementation backed by a pairing heap[1] implementation.
///
/// Why? Intrusive data structures require the element type to hold the metadata
/// required for the structure, rather than an additional container structure.
/// There are numerous pros/cons that are documented well by Boost[2]. For Zig,
/// I think the primary benefits are making data structures allocation free
/// (rather, shifting allocation up to the consumer which can choose how they
/// want the memory to be available). There are various costs to this such as
/// the costs of pointer chasing, larger memory overhead, requiring the element
/// type to be aware of its container, etc. But for certain use cases an intrusive
/// data structure can yield much better performance.
///
/// Usage notes:
/// - The element T is expected to have a field "heap" of type InstrusiveHeapField.
///   See the tests for a full example of how to set this.
/// - You can easily make this a min or max heap by inverting the result of
///   "less" below.
///
/// [1]: https://en.wikipedia.org/wiki/Pairing_heap
/// [2]: https://www.boost.org/doc/libs/1_64_0/doc/html/intrusive/intrusive_vs_nontrusive.html
pub struct Intrusive<T: Node, Context: Copy> {
    pub root: *mut T,
    pub context: Context,
    // `less`: the comptime `*const fn(ctx: Context, a: *T, b: *T) bool` comparator,
    // carried as a fn-pointer field (see the module PORT NOTE).
    less: fn(Context, *mut T, *mut T) -> bool,
}

// PORT NOTE [ADV-FIX F2]: the intrusive raw-ptr heap (the `Loop`'s timer heap) is
// auto-`!Send`; ghostty MOVES the owning `Loop` onto the termio thread. SAFETY /
// single-thread affinity: the heap is touched by exactly one thread at a time —
// the owning event-loop thread — after the move, so transferring ownership is
// sound.
unsafe impl<T: Node, Context: Copy> Send for Intrusive<T, Context> {}

impl<T: Node, Context: Copy> Intrusive<T, Context> {
    /// Create the heap. Rust glue for Zig's `.{ .context = ctx }` aggregate:
    /// `root` defaults to null and the comptime `less` comparator is stored.
    pub fn new(context: Context, less: fn(Context, *mut T, *mut T) -> bool) -> Self {
        Self {
            root: ptr::null_mut(),
            context,
            less,
        }
    }

    /// Insert a new element v into the heap. An element v can only
    /// be a member of a single heap at any given time. When compiled
    /// with runtime-safety, assertions will help verify this property.
    ///
    /// # Safety
    /// `v` must be a valid, dereferenceable node pointer not already in a heap.
    pub unsafe fn insert(&mut self, v: *mut T) {
        self.root = if !self.root.is_null() {
            unsafe { self.meld(v, self.root) }
        } else {
            v
        };
    }

    /// Look at the next minimum value but do not remove it.
    pub fn peek(&self) -> *mut T {
        self.root
    }

    /// Delete the minimum value from the heap and return it.
    ///
    /// # Safety
    /// The heap and its nodes must be valid.
    pub unsafe fn delete_min(&mut self) -> *mut T {
        let root = self.root;
        if root.is_null() {
            return ptr::null_mut();
        }
        let root_child = unsafe { get_child(root) };
        self.root = if !root_child.is_null() {
            unsafe { self.combine_siblings(root_child) }
        } else {
            ptr::null_mut()
        };

        // Clear pointers with runtime safety so we can verify on
        // insert that values aren't incorrectly being set multiple times.
        // root.heap = .{};
        unsafe { T::heap_field(root).write(IntrusiveField::default()) };

        root
    }

    /// Remove the value v from the heap.
    ///
    /// # Safety
    /// `v` must be a valid, dereferenceable node pointer that is a member of
    /// this heap.
    pub unsafe fn remove(&mut self, v: *mut T) {
        // If v doesn't have a previous value, this must be the root
        // element. If it is NOT the root element, v can't be in this
        // heap and we trigger an assertion failure.
        let prev = unsafe { get_prev(v) };
        if prev.is_null() {
            debug_assert!(self.root == v);
            let _ = unsafe { self.delete_min() };
            return;
        }

        // Detach "v" from the tree and clean up any links so it
        // is as if this node never nexisted. The previous value
        // must point to the proper next value and the pointers
        // must all be cleaned up.
        let v_next = unsafe { get_next(v) };
        if !v_next.is_null() {
            unsafe { set_prev(v_next, prev) };
        }
        if unsafe { get_child(prev) } == v {
            unsafe { set_child(prev, v_next) };
        } else {
            unsafe { set_next(prev, v_next) };
        }
        unsafe { set_prev(v, ptr::null_mut()) };
        unsafe { set_next(v, ptr::null_mut()) };

        // If we have children, then we need to merge them back in.
        let child = unsafe { get_child(v) };
        if child.is_null() {
            return;
        }
        unsafe { set_child(v, ptr::null_mut()) };
        let x = unsafe { self.combine_siblings(child) };
        self.root = unsafe { self.meld(x, self.root) };
    }

    /// Meld (union) two heaps together. This isn't a generalized
    /// union. It assumes that a.heap.next is null so this is only
    /// meant in specific scenarios in the pairing heap where meld
    /// is expected.
    ///
    /// For example, when melding a new value "v" with an existing
    /// root "root", "v" must always be the first param.
    ///
    /// # Safety
    /// `a` and `b` must be valid, dereferenceable node pointers with
    /// `a.heap.next == null`.
    unsafe fn meld(&self, a: *mut T, b: *mut T) -> *mut T {
        debug_assert!(unsafe { get_next(a) }.is_null());

        if (self.less)(self.context, a, b) {
            // B points back to A
            unsafe { set_prev(b, a) };

            // If B has siblings, then A inherits B's siblings
            // and B's immediate sibling must point back to A to
            // maintain the doubly linked list.
            let b_next = unsafe { get_next(b) };
            if !b_next.is_null() {
                unsafe { set_next(a, b_next) };
                unsafe { set_prev(b_next, a) };
                unsafe { set_next(b, ptr::null_mut()) };
            }

            // If A has a child, then B becomes the leftmost sibling
            // of that child.
            let a_child = unsafe { get_child(a) };
            if !a_child.is_null() {
                unsafe { set_next(b, a_child) };
                unsafe { set_prev(a_child, b) };
            }

            // B becomes the leftmost child of A
            unsafe { set_child(a, b) };

            return a;
        }

        // Replace A with B in the tree. Any of B's children
        // become siblings of A. A becomes the leftmost child of B.
        // A points back to B
        unsafe { set_prev(b, get_prev(a)) };
        unsafe { set_prev(a, b) };
        let b_child = unsafe { get_child(b) };
        if !b_child.is_null() {
            unsafe { set_next(a, b_child) };
            unsafe { set_prev(b_child, a) };
        }
        unsafe { set_child(b, a) };
        b
    }

    /// Combine the siblings of the leftmost value "left" into a single
    /// new rooted with the minimum value.
    ///
    /// # Safety
    /// `left` must be a valid, dereferenceable node pointer heading a sibling
    /// list.
    unsafe fn combine_siblings(&self, left: *mut T) -> *mut T {
        unsafe { set_prev(left, ptr::null_mut()) };

        // Merge pairs right
        let mut root: *mut T = {
            let mut a: *mut T = left;
            loop {
                let b = unsafe { get_next(a) };
                if b.is_null() {
                    break a;
                }
                unsafe { set_next(a, ptr::null_mut()) };
                let b = unsafe { self.meld(a, b) };
                let b_next = unsafe { get_next(b) };
                if b_next.is_null() {
                    break b;
                }
                a = b_next;
            }
        };

        // Merge pairs left
        loop {
            let b = unsafe { get_prev(root) };
            if b.is_null() {
                return root;
            }
            unsafe { set_next(b, ptr::null_mut()) };
            root = unsafe { self.meld(b, root) };
        }
    }
}

/// The state that is required for IntrusiveHeap element types. This
/// should be set as the "heap" field in the type T.
pub struct IntrusiveField<T> {
    pub child: *mut T,
    pub prev: *mut T,
    pub next: *mut T,
}

// PORT NOTE [ADV-FIX F3]: hand-written `Default` (all links null), NOT
// `#[derive(Default)]` — `*mut T` has no `Default` impl. Mirrors the Zig field
// defaults `child/prev/next: ?*T = null`; `root.heap = .{}` uses it.
impl<T> Default for IntrusiveField<T> {
    fn default() -> Self {
        Self {
            child: ptr::null_mut(),
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The reference re-declares this `Elem`, its `less`, and the `Heap`
    // instantiation inside every test{}; they are identical, so we hoist them
    // once here and each ported test reuses them (behavior preserved).
    #[derive(Default)]
    struct Elem {
        // value: usize = 0,
        value: usize,
        // heap: IntrusiveField(Self) = .{},
        heap: IntrusiveField<Elem>,
    }

    impl Node for Elem {
        unsafe fn heap_field(this: *mut Self) -> *mut IntrusiveField<Self> {
            unsafe { ptr::addr_of_mut!((*this).heap) }
        }
    }

    // fn less(ctx: void, a: *Elem, b: *Elem) bool { return a.value < b.value; }
    fn less(_ctx: (), a: *mut Elem, b: *mut Elem) -> bool {
        unsafe { ptr::addr_of!((*a).value).read() < ptr::addr_of!((*b).value).read() }
    }

    // Read a node's `value` through a raw pointer (Zig `elem.value`).
    unsafe fn value(p: *mut Elem) -> usize {
        unsafe { ptr::addr_of!((*p).value).read() }
    }

    // test "heap"
    #[test]
    fn heap() {
        // var a: Elem = .{ .value = 12 };
        let a = Box::into_raw(Box::new(Elem {
            value: 12,
            ..Default::default()
        }));
        // var b: Elem = .{ .value = 24 };
        let b = Box::into_raw(Box::new(Elem {
            value: 24,
            ..Default::default()
        }));
        // var c: Elem = .{ .value = 7 };
        let c = Box::into_raw(Box::new(Elem {
            value: 7,
            ..Default::default()
        }));
        // var d: Elem = .{ .value = 9 };
        let d = Box::into_raw(Box::new(Elem {
            value: 9,
            ..Default::default()
        }));

        // var h: Heap = .{ .context = {} };
        let mut h: Intrusive<Elem, ()> = Intrusive::new((), less);
        unsafe {
            h.insert(a);
            h.insert(b);
            h.insert(c);
            h.insert(d);
            h.remove(d);

            // try testing.expect(h.deleteMin().?.value == 7);
            assert_eq!(value(h.delete_min()), 7);
            // try testing.expect(h.deleteMin().?.value == 12);
            assert_eq!(value(h.delete_min()), 12);
            // try testing.expect(h.deleteMin().?.value == 24);
            assert_eq!(value(h.delete_min()), 24);
            // try testing.expect(h.deleteMin() == null);
            assert!(h.delete_min().is_null());

            drop(Box::from_raw(a));
            drop(Box::from_raw(b));
            drop(Box::from_raw(c));
            drop(Box::from_raw(d));
        }
    }

    // test "heap remove root"
    #[test]
    fn heap_remove_root() {
        // var a: Elem = .{ .value = 12 };
        let a = Box::into_raw(Box::new(Elem {
            value: 12,
            ..Default::default()
        }));
        // var b: Elem = .{ .value = 24 };
        let b = Box::into_raw(Box::new(Elem {
            value: 24,
            ..Default::default()
        }));

        // var h: Heap = .{ .context = {} };
        let mut h: Intrusive<Elem, ()> = Intrusive::new((), less);
        unsafe {
            h.insert(a);
            h.insert(b);
            h.remove(a);

            // try testing.expect(h.deleteMin().?.value == 24);
            assert_eq!(value(h.delete_min()), 24);
            // try testing.expect(h.deleteMin() == null);
            assert!(h.delete_min().is_null());

            drop(Box::from_raw(a));
            drop(Box::from_raw(b));
        }
    }

    // test "heap remove with children"
    #[test]
    fn heap_remove_with_children() {
        // var a: Elem = .{ .value = 36 };
        let a = Box::into_raw(Box::new(Elem {
            value: 36,
            ..Default::default()
        }));
        // var b: Elem = .{ .value = 24 };
        let b = Box::into_raw(Box::new(Elem {
            value: 24,
            ..Default::default()
        }));
        // var c: Elem = .{ .value = 12 };
        let c = Box::into_raw(Box::new(Elem {
            value: 12,
            ..Default::default()
        }));

        // var h: Heap = .{ .context = {} };
        let mut h: Intrusive<Elem, ()> = Intrusive::new((), less);
        unsafe {
            h.insert(a);
            h.insert(b);
            h.insert(c);
            h.remove(b);

            // try testing.expect(h.deleteMin().?.value == 12);
            assert_eq!(value(h.delete_min()), 12);
            // try testing.expect(h.deleteMin().?.value == 36);
            assert_eq!(value(h.delete_min()), 36);
            // try testing.expect(h.deleteMin() == null);
            assert!(h.delete_min().is_null());

            drop(Box::from_raw(a));
            drop(Box::from_raw(b));
            drop(Box::from_raw(c));
        }
    }

    // test "heap equal values"
    #[test]
    fn heap_equal_values() {
        // var a: Elem = .{ .value = 1 };
        let a = Box::into_raw(Box::new(Elem {
            value: 1,
            ..Default::default()
        }));
        // var b: Elem = .{ .value = 2 };
        let b = Box::into_raw(Box::new(Elem {
            value: 2,
            ..Default::default()
        }));
        // var c: Elem = .{ .value = 3 };
        let c = Box::into_raw(Box::new(Elem {
            value: 3,
            ..Default::default()
        }));
        // var d: Elem = .{ .value = 4 };
        let d = Box::into_raw(Box::new(Elem {
            value: 4,
            ..Default::default()
        }));

        // var h: Heap = .{ .context = {} };
        let mut h: Intrusive<Elem, ()> = Intrusive::new((), less);
        unsafe {
            h.insert(a);
            h.insert(b);
            h.insert(c);
            h.insert(d);

            // try testing.expect(h.deleteMin().?.value == 1);
            assert_eq!(value(h.delete_min()), 1);
            // try testing.expect(h.deleteMin().?.value == 2);
            assert_eq!(value(h.delete_min()), 2);
            // try testing.expect(h.deleteMin().?.value == 3);
            assert_eq!(value(h.delete_min()), 3);
            // try testing.expect(h.deleteMin().?.value == 4);
            assert_eq!(value(h.delete_min()), 4);
            // try testing.expect(h.deleteMin() == null);
            assert!(h.delete_min().is_null());

            drop(Box::from_raw(a));
            drop(Box::from_raw(b));
            drop(Box::from_raw(c));
            drop(Box::from_raw(d));
        }
    }

    // test "heap: million values"
    #[test]
    fn heap_million_values() {
        // const NUM_TIMERS: usize = 1000 * 1000;
        //
        // PORT/MIRI NOTE: native `cargo test` runs the faithful 1_000_000. Miri
        // interprets every pointer op, so 1e6 heap ops would take far too long, and
        // UB detection does not depend on the count — under `cfg(miri)` we exercise a
        // smaller N (the algorithm and every code path are identical).
        const NUM_TIMERS: usize = if cfg!(miri) { 1000 } else { 1000 * 1000 };

        // var elems = try alloc.alloc(Elem, NUM_TIMERS);
        // defer alloc.free(elems);
        let mut elems: Vec<*mut Elem> = Vec::with_capacity(NUM_TIMERS);

        // var i: usize = 0;
        // var value: usize = 0;
        // while (i < NUM_TIMERS) : (i += 1) {
        //     if (i % 100 == 0) value += 1;
        //     elems[i] = .{ .value = value };
        // }
        let mut val: usize = 0;
        for i in 0..NUM_TIMERS {
            if i % 100 == 0 {
                val += 1;
            }
            elems.push(Box::into_raw(Box::new(Elem {
                value: val,
                ..Default::default()
            })));
        }

        // var h: Heap = .{ .context = {} };
        let mut h: Intrusive<Elem, ()> = Intrusive::new((), less);
        unsafe {
            // for (elems) |*elem| { h.insert(elem); }
            for &elem in &elems {
                h.insert(elem);
            }

            // var count: usize = 0;
            // var last: usize = 0;
            let mut count: usize = 0;
            let mut last: usize = 0;
            // while (h.deleteMin()) |elem| {
            loop {
                let elem = h.delete_min();
                if elem.is_null() {
                    break;
                }
                // count += 1;
                count += 1;
                // try testing.expect(elem.value >= last);
                assert!(value(elem) >= last);
                // last = elem.value;
                last = value(elem);
            }
            // try testing.expect(h.deleteMin() == null);
            assert!(h.delete_min().is_null());
            // try testing.expect(count == NUM_TIMERS);
            assert_eq!(count, NUM_TIMERS);

            for elem in elems {
                drop(Box::from_raw(elem));
            }
        }
    }

    // test "heap: dangling next pointer"
    #[test]
    fn heap_dangling_next_pointer() {
        // var a: Elem = .{ .value = 2 };
        let a = Box::into_raw(Box::new(Elem {
            value: 2,
            ..Default::default()
        }));
        // var b: Elem = .{ .value = 4 };
        let b = Box::into_raw(Box::new(Elem {
            value: 4,
            ..Default::default()
        }));
        // var c: Elem = .{ .value = 5 };
        let c = Box::into_raw(Box::new(Elem {
            value: 5,
            ..Default::default()
        }));
        // var d: Elem = .{ .value = 1 };
        let d = Box::into_raw(Box::new(Elem {
            value: 1,
            ..Default::default()
        }));
        // var e: Elem = .{ .value = 3 };
        let e = Box::into_raw(Box::new(Elem {
            value: 3,
            ..Default::default()
        }));

        // var h: Heap = .{ .context = {} };
        let mut h: Intrusive<Elem, ()> = Intrusive::new((), less);
        unsafe {
            h.insert(a);
            h.insert(b);
            h.insert(c);
            h.insert(d);
            h.insert(e);

            // try testing.expect(h.deleteMin().?.value == 1);
            assert_eq!(value(h.delete_min()), 1);
            // try testing.expect(h.deleteMin().?.value == 2);
            assert_eq!(value(h.delete_min()), 2);
            // try testing.expect(h.deleteMin().?.value == 3);
            assert_eq!(value(h.delete_min()), 3);
            // try testing.expect(h.deleteMin().?.value == 4);
            assert_eq!(value(h.delete_min()), 4);
            // try testing.expect(h.deleteMin().?.value == 5);
            assert_eq!(value(h.delete_min()), 5);
            // try testing.expect(h.deleteMin() == null);
            assert!(h.delete_min().is_null());

            drop(Box::from_raw(a));
            drop(Box::from_raw(b));
            drop(Box::from_raw(c));
            drop(Box::from_raw(d));
            drop(Box::from_raw(e));
        }
    }
}
