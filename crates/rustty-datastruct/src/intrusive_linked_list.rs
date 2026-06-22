//! An intrusive doubly-linked list. The type T must have a "next" and "prev"
//! field pointing to itself.
//!
//! This is an adaptation of the DoublyLinkedList from the Zig standard
//! library, which is MIT licensed. I've removed functionality that I don't
//! need.
//
// PORT NOTE: The Zig reference is `fn DoublyLinkedList(comptime T: type) type`
// where `T` is the node type itself (intrusive: the node owns its own
// `next`/`prev`). In Rust we model the node fields via the `Node` trait so the
// list can read/write the embedded `prev`/`next` raw pointers without owning
// the nodes (callers keep them on the stack/elsewhere, exactly like the Zig
// tests). The list is therefore inherently `unsafe` to drive — the same memory
// model as the reference (raw self-referential pointers).

use core::ptr;

/// Trait for an intrusive node. The implementing type must store a "next" and
/// "prev" pointer to itself; these accessors expose them to the list. This is
/// the Rust seam for the Zig requirement that `T` "must have a 'next' and
/// 'prev' field pointing to itself".
pub trait Node {
    /// Get the `prev` pointer.
    fn prev(&self) -> *mut Self;
    /// Get the `next` pointer.
    fn next(&self) -> *mut Self;
    /// Set the `prev` pointer.
    fn set_prev(&mut self, p: *mut Self);
    /// Set the `next` pointer.
    fn set_next(&mut self, n: *mut Self);
}

/// An intrusive doubly-linked list. The type T must have a "next" and "prev"
/// field pointing to itself.
///
/// This is an adaptation of the DoublyLinkedList from the Zig standard
/// library, which is MIT licensed. I've removed functionality that I don't
/// need.
pub struct DoublyLinkedList<T: Node> {
    pub first: *mut T,
    pub last: *mut T,
}

impl<T: Node> Default for DoublyLinkedList<T> {
    fn default() -> Self {
        Self {
            first: ptr::null_mut(),
            last: ptr::null_mut(),
        }
    }
}

impl<T: Node> DoublyLinkedList<T> {
    /// Insert a new node after an existing one.
    ///
    /// Arguments:
    ///     node: Pointer to a node in the list.
    ///     new_node: Pointer to the new node to insert.
    ///
    /// # Safety
    /// `node` and `new_node` must be valid, distinct, dereferenceable pointers,
    /// with `node` already in the list.
    #[inline]
    pub unsafe fn insert_after(&mut self, node: *mut T, new_node: *mut T) {
        unsafe {
            (*new_node).set_prev(node);
            let next_node = (*node).next();
            if !next_node.is_null() {
                // Intermediate node.
                (*new_node).set_next(next_node);
                (*next_node).set_prev(new_node);
            } else {
                // Last element of the list.
                (*new_node).set_next(ptr::null_mut());
                self.last = new_node;
            }
            (*node).set_next(new_node);
        }
    }

    /// Insert a new node before an existing one.
    ///
    /// Arguments:
    ///     node: Pointer to a node in the list.
    ///     new_node: Pointer to the new node to insert.
    ///
    /// # Safety
    /// `node` and `new_node` must be valid, distinct, dereferenceable pointers,
    /// with `node` already in the list.
    #[inline]
    pub unsafe fn insert_before(&mut self, node: *mut T, new_node: *mut T) {
        unsafe {
            (*new_node).set_next(node);
            let prev_node = (*node).prev();
            if !prev_node.is_null() {
                // Intermediate node.
                (*new_node).set_prev(prev_node);
                (*prev_node).set_next(new_node);
            } else {
                // First element of the list.
                (*new_node).set_prev(ptr::null_mut());
                self.first = new_node;
            }
            (*node).set_prev(new_node);
        }
    }

    /// Insert a new node at the end of the list.
    ///
    /// Arguments:
    ///     new_node: Pointer to the new node to insert.
    ///
    /// # Safety
    /// `new_node` must be a valid, dereferenceable pointer not already in the
    /// list.
    #[inline]
    pub unsafe fn append(&mut self, new_node: *mut T) {
        unsafe {
            if !self.last.is_null() {
                // Insert after last.
                self.insert_after(self.last, new_node);
            } else {
                // Empty list.
                self.prepend(new_node);
            }
        }
    }

    /// Insert a new node at the beginning of the list.
    ///
    /// Arguments:
    ///     new_node: Pointer to the new node to insert.
    ///
    /// # Safety
    /// `new_node` must be a valid, dereferenceable pointer not already in the
    /// list.
    #[inline]
    pub unsafe fn prepend(&mut self, new_node: *mut T) {
        unsafe {
            if !self.first.is_null() {
                // Insert before first.
                self.insert_before(self.first, new_node);
            } else {
                // Empty list.
                self.first = new_node;
                self.last = new_node;
                (*new_node).set_prev(ptr::null_mut());
                (*new_node).set_next(ptr::null_mut());
            }
        }
    }

    /// Remove a node from the list.
    ///
    /// Arguments:
    ///     node: Pointer to the node to be removed.
    ///
    /// # Safety
    /// `node` must be a valid, dereferenceable pointer currently in the list.
    #[inline]
    pub unsafe fn remove(&mut self, node: *mut T) {
        unsafe {
            let prev_node = (*node).prev();
            if !prev_node.is_null() {
                // Intermediate node.
                (*prev_node).set_next((*node).next());
            } else {
                // First element of the list.
                self.first = (*node).next();
            }

            let next_node = (*node).next();
            if !next_node.is_null() {
                // Intermediate node.
                (*next_node).set_prev((*node).prev());
            } else {
                // Last element of the list.
                self.last = (*node).prev();
            }
        }
    }

    /// Remove and return the last node in the list.
    ///
    /// Returns:
    ///     A pointer to the last node in the list.
    ///
    /// # Safety
    /// The list and its nodes must be valid.
    #[inline]
    pub unsafe fn pop(&mut self) -> *mut T {
        let last = self.last;
        if last.is_null() {
            return ptr::null_mut();
        }
        unsafe {
            self.remove(last);
        }
        last
    }

    /// Remove and return the first node in the list.
    ///
    /// Returns:
    ///     A pointer to the first node in the list.
    ///
    /// # Safety
    /// The list and its nodes must be valid.
    #[inline]
    pub unsafe fn pop_first(&mut self) -> *mut T {
        let first = self.first;
        if first.is_null() {
            return ptr::null_mut();
        }
        unsafe {
            self.remove(first);
        }
        first
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    struct TestNode {
        data: u32,
        prev: *mut TestNode,
        next: *mut TestNode,
    }

    impl TestNode {
        fn new(data: u32) -> Self {
            TestNode {
                data,
                prev: ptr::null_mut(),
                next: ptr::null_mut(),
            }
        }
    }

    impl Node for TestNode {
        fn prev(&self) -> *mut Self {
            self.prev
        }
        fn next(&self) -> *mut Self {
            self.next
        }
        fn set_prev(&mut self, p: *mut Self) {
            self.prev = p;
        }
        fn set_next(&mut self, n: *mut Self) {
            self.next = n;
        }
    }

    #[test]
    fn basic_DoublyLinkedList_test() {
        let l: DoublyLinkedList<TestNode> = DoublyLinkedList::default();
        let mut list = l;

        // PORT/MIRI NOTE: The Zig test declares the nodes as `var one: Node`
        // stack values and passes `&one` to the list. Under Rust's Stacked
        // Borrows model, taking `&mut one` for a *second* call invalidates the
        // raw pointer the list already stored from the first `&mut one`. To stay
        // UB-free under Miri while preserving the exact intrusive-list behavior,
        // we heap-allocate each node once via `Box::into_raw` and pass the SAME
        // stable raw pointer to every operation (the canonical Rust idiom for
        // intrusive lists). Field reads use raw-pointer `read()` to avoid
        // forming aliasing references. The nodes are freed at the end.
        let one: *mut TestNode = Box::into_raw(Box::new(TestNode::new(1)));
        let two: *mut TestNode = Box::into_raw(Box::new(TestNode::new(2)));
        let three: *mut TestNode = Box::into_raw(Box::new(TestNode::new(3)));
        let four: *mut TestNode = Box::into_raw(Box::new(TestNode::new(4)));
        let five: *mut TestNode = Box::into_raw(Box::new(TestNode::new(5)));

        unsafe {
            list.append(two); // {2}
            list.append(five); // {2, 5}
            list.prepend(one); // {1, 2, 5}
            list.insert_before(five, four); // {1, 2, 4, 5}
            list.insert_after(two, three); // {1, 2, 3, 4, 5}

            // Traverse forwards.
            {
                let mut it = list.first;
                let mut index: u32 = 1;
                while !it.is_null() {
                    assert!(core::ptr::addr_of!((*it).data).read() == index);
                    index += 1;
                    it = core::ptr::addr_of!((*it).next).read();
                }
            }

            // Traverse backwards.
            {
                let mut it = list.last;
                let mut index: u32 = 1;
                while !it.is_null() {
                    assert!(core::ptr::addr_of!((*it).data).read() == (6 - index));
                    index += 1;
                    it = core::ptr::addr_of!((*it).prev).read();
                }
            }

            let _ = list.pop_first(); // {2, 3, 4, 5}
            let _ = list.pop(); // {2, 3, 4}
            list.remove(three); // {2, 4}

            assert!(core::ptr::addr_of!((*list.first).data).read() == 2);
            assert!(core::ptr::addr_of!((*list.last).data).read() == 4);

            // Free the heap-allocated nodes.
            drop(Box::from_raw(one));
            drop(Box::from_raw(two));
            drop(Box::from_raw(three));
            drop(Box::from_raw(four));
            drop(Box::from_raw(five));
        }
    }
}
