//! A collection of ArrayLists with methods for bulk operations.
//
// PORT NOTE: The Zig reference uses `std.ArrayListUnmanaged(T)` with an explicit
// `Allocator`. Rust's `Vec<T>` is the idiomatic equivalent (it owns its
// allocator implicitly via the global allocator). `init` with an
// `initial_capacity` maps to `Vec::with_capacity`, `deinit` is `Drop`, and
// `reset` -> `clearRetainingCapacity` maps to `Vec::clear` (which retains
// capacity in Rust). No allocator parameter is threaded since this leaf uses
// the global allocator (Miri-checked).

/// A collection of ArrayLists with methods for bulk operations.
pub struct ArrayListCollection<T> {
    // An array containing the lists that belong to this collection.
    pub lists: Vec<Vec<T>>,
}

impl<T> ArrayListCollection<T> {
    // The collection will be initialized with empty ArrayLists.
    pub fn init(list_count: usize, initial_capacity: usize) -> Self {
        let mut lists: Vec<Vec<T>> = Vec::with_capacity(list_count);
        for _ in 0..list_count {
            lists.push(Vec::with_capacity(initial_capacity));
        }

        ArrayListCollection { lists }
    }

    /// Clear all lists in the collection, retaining capacity.
    pub fn reset(&mut self) {
        for list in self.lists.iter_mut() {
            list.clear();
        }
    }
}
