//! Port of `std.array_hash_map` (Zig 0.15.2) — ADDITIVE INFRA GLUE.
//!
//! PORT STATUS: infra (faithful-semantics port of Zig stdlib; not in the 3234
//! contract)
//!
//! `AutoArrayHashMap` / `AutoArrayHashMapUnmanaged`: an INSERTION-ORDERED hash
//! map. Unlike a plain hash map, the entries are stored in a contiguous array in
//! the order they were first inserted, and `keys()` / `values()` / iteration all
//! observe that order. A side index provides O(1) lookup by key.
//!
//! This is a faithful port of the *observable* semantics the port relies on:
//!
//!   * insertion-order `entries` (array of key/value), exposed via `keys()` /
//!     `values()` / `iter()` in that order;
//!   * `getOrPut` returns `found_existing` + the entry `index` and pointers;
//!   * `put` overwrites the value of an existing key (keeping its position) or
//!     appends a new entry;
//!   * `putNoClobber` asserts the key is new;
//!   * `get` / `getPtr` / `getIndex` / `contains` / `count`;
//!   * `swapRemove(key)` removes the entry by swapping the LAST entry into its
//!     slot — O(1), but CHANGES iteration order;
//!   * `orderedRemove(key)` removes the entry by shifting all following entries
//!     forward — O(n), but PRESERVES iteration order;
//!   * `fetchSwapRemove` / `fetchOrderedRemove` return the removed key+value.
//!
//! Backing memory comes from the map's allocator `A` (allocator-api2's
//! `Allocator`, the workspace's map of Zig's `std.mem.Allocator`): the entries
//! array is an allocator_api2 `Vec` and the index is a `hashbrown` table, both
//! over `A`. `A` defaults to `Global` (the Rust heap), so every existing
//! `AutoArrayHashMapUnmanaged<K, V>` keeps its prior backing. Zig's unmanaged
//! map takes the allocator per call; here it is bound at construction
//! (`new_in`), as allocator_api2 containers are (parity tail S7-3, so rustty-vt
//! can route `PageList.tracked_pins` and the glyph glossary through the
//! caller's allocator).
//!
//! Growth: the infallible methods (`put`, `put_no_clobber`, `get_or_put_*`)
//! abort on allocation failure, like std collections. The `try_*` methods are
//! Zig's `try map.put(alloc, ...)`: they reserve first and return
//! [`OutOfMemory`] with the map unchanged.
//!
//! Implementation choice (parity, sound): the Zig original hand-rolls an
//! `IndexHeader` open-addressing index alongside the entries array; we reproduce
//! the same OBSERVABLE behavior using a `Vec<(K, V)>` for the ordered entries
//! plus a `HashMap<K, usize>` mapping key -> entry index. This is pure-safe Rust
//! (no `unsafe`) and reproduces every documented semantic above exactly. Indices
//! that shift on removal are repaired so `getIndex`/lookup stay correct.
//!
//! PORT NOTE (call shape, T3): the allocation sequence is not Zig's. Zig keeps
//! the entries in a `MultiArrayList` and builds an index header only above a
//! small linear-scan threshold; here the entries `Vec` and the hash index grow
//! by their own policies. Which allocator is called (routing) is exact; the
//! count and sizes of the calls are not.

use std::hash::{Hash, RandomState};

use allocator_api2::alloc::{Allocator, Global};
use allocator_api2::vec::Vec as AVec;

pub use crate::memory_pool::OutOfMemory;

/// The side index: key -> entry index, over the map's allocator.
type Index<K, A> = hashbrown::HashMap<K, usize, RandomState, A>;

/// A removed key/value pair, returned by `fetch*` removals. Faithful port of the
/// `KV` struct (`{ key, value }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KV<K, V> {
    pub key: K,
    pub value: V,
}

/// Result of `getOrPut`. Faithful port of `GetOrPutResult` (minus the raw
/// pointers, which Rust exposes via `&mut` accessors keyed by `index`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetOrPutResult {
    /// Index of the entry in the insertion-ordered `entries` array.
    pub index: usize,
    /// Whether an entry with this key already existed.
    pub found_existing: bool,
}

/// Faithful port of `AutoArrayHashMapUnmanaged(K, V)`: an insertion-ordered hash
/// map. "Unmanaged" in Zig means it does not store its allocator; here the
/// allocator is bound at construction (see the module docs), so the
/// managed/unmanaged distinction collapses — `AutoArrayHashMap` is a type alias.
///
/// `K: Eq + Hash + Clone` (Clone is needed for the side index; Zig copies keys
/// into both the entries array and the index too).
pub struct AutoArrayHashMapUnmanaged<K, V, A: Allocator + Clone = Global> {
    /// Entries in insertion order. `keys()`/`values()` read from here.
    entries: AVec<(K, V), A>,
    /// key -> index into `entries`, for O(1) lookup (Zig's IndexHeader).
    index: Index<K, A>,
}

impl<K, V, A> Default for AutoArrayHashMapUnmanaged<K, V, A>
where
    K: Eq + Hash + Clone,
    A: Allocator + Clone + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V, A> AutoArrayHashMapUnmanaged<K, V, A>
where
    K: Eq + Hash + Clone,
    A: Allocator + Clone + Default,
{
    /// The empty map. Faithful port of `.empty` / default-init.
    pub fn new() -> Self {
        Self::new_in(A::default())
    }
}

impl<K, V, A> AutoArrayHashMapUnmanaged<K, V, A>
where
    K: Eq + Hash + Clone,
    A: Allocator + Clone,
{
    /// The empty map over `alloc`. Allocates nothing (`.empty`).
    pub fn new_in(alloc: A) -> Self {
        AutoArrayHashMapUnmanaged {
            entries: AVec::new_in(alloc.clone()),
            index: Index::with_hasher_in(RandomState::new(), alloc),
        }
    }

    /// The allocator this map grows and frees through.
    pub fn allocator(&self) -> &A {
        self.entries.allocator()
    }

    /// Returns the number of key/value pairs. Faithful port of `count`.
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// `true` if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Keys in insertion order. Faithful port of `keys()`.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.iter().map(|(k, _)| k)
    }

    /// Values in insertion order. Faithful port of `values()`.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }

    /// Mutable values in insertion order.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.entries.iter_mut().map(|(_, v)| v)
    }

    /// Entries (key, value) in insertion order. Faithful analog of iterating the
    /// parallel `keys`/`values` arrays.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    /// Index of `key` in the insertion-ordered array, or `None`. Faithful port
    /// of `getIndex`.
    pub fn get_index(&self, key: &K) -> Option<usize> {
        self.index.get(key).copied()
    }

    /// `true` if `key` is present. Faithful port of `contains`.
    pub fn contains(&self, key: &K) -> bool {
        self.index.contains_key(key)
    }

    /// Value for `key`, by copy/clone semantics. Faithful port of `get` (Zig
    /// returns `?V` by value). We return a reference to avoid forcing `V: Clone`;
    /// use `get_copied`/clone at the call site if a value copy is wanted.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.get_index(key).map(|i| &self.entries[i].1)
    }

    /// Mutable pointer to the value for `key`. Faithful port of `getPtr`.
    pub fn get_ptr(&mut self, key: &K) -> Option<&mut V> {
        match self.index.get(key).copied() {
            Some(i) => Some(&mut self.entries[i].1),
            None => None,
        }
    }

    /// The key stored for `key` (Zig `getKey` returns the stored key copy).
    pub fn get_key(&self, key: &K) -> Option<&K> {
        self.get_index(key).map(|i| &self.entries[i].0)
    }

    /// Entry (key, value) references at `index` (insertion order).
    pub fn entry_at(&self, index: usize) -> Option<(&K, &V)> {
        self.entries.get(index).map(|(k, v)| (k, v))
    }

    /// Reserve room for one more entry in both the entries array and the index,
    /// so the append that follows cannot allocate (Zig's `ensureUnusedCapacity`
    /// inside `getOrPut`).
    fn try_reserve_one(&mut self) -> Result<(), OutOfMemory> {
        self.entries.try_reserve(1).map_err(|_| OutOfMemory)?;
        self.index.try_reserve(1).map_err(|_| OutOfMemory)
    }

    /// Append a new entry. The caller has checked that `key` is absent.
    fn append(&mut self, key: K, value: V) -> usize {
        let i = self.entries.len();
        self.entries.push((key.clone(), value));
        self.index.insert(key, i);
        i
    }

    /// Faithful port of `getOrPut`. If `key` exists, returns its index with
    /// `found_existing = true` and leaves the value untouched. Otherwise appends
    /// a new entry whose value is left uninitialized in Zig; here the caller must
    /// supply it via the returned index (use `value_at_mut`), or prefer
    /// `get_or_put_value` / `put`.
    ///
    /// To keep this sound in Rust (no uninitialized `V`), the new slot is created
    /// with `value` filled by `make_default`. This matches the common port usage
    /// (`gop.value_ptr.* = ...` immediately after) — the value is always
    /// overwritten before observation. Use [`Self::get_or_put_with`] to control
    /// the initial value precisely.
    pub fn get_or_put_with<F>(&mut self, key: K, make_default: F) -> GetOrPutResult
    where
        F: FnOnce() -> V,
    {
        if let Some(&i) = self.index.get(&key) {
            GetOrPutResult {
                index: i,
                found_existing: true,
            }
        } else {
            let i = self.append(key, make_default());
            GetOrPutResult {
                index: i,
                found_existing: false,
            }
        }
    }

    /// `try map.getOrPut(alloc, key)`: [`Self::get_or_put_with`], failing with
    /// `error.OutOfMemory` (map unchanged) instead of aborting.
    pub fn try_get_or_put_with<F>(
        &mut self,
        key: K,
        make_default: F,
    ) -> Result<GetOrPutResult, OutOfMemory>
    where
        F: FnOnce() -> V,
    {
        if let Some(&i) = self.index.get(&key) {
            return Ok(GetOrPutResult {
                index: i,
                found_existing: true,
            });
        }
        self.try_reserve_one()?;
        let i = self.append(key, make_default());
        Ok(GetOrPutResult {
            index: i,
            found_existing: false,
        })
    }

    /// Faithful port of `getOrPutValue`: get-or-insert, and if newly inserted set
    /// the value to `value`. Returns the result (with `found_existing`).
    pub fn get_or_put_value(&mut self, key: K, value: V) -> GetOrPutResult {
        self.get_or_put_with(key, || value)
    }

    /// Mutable access to a value by entry index (insertion order). Pairs with
    /// `getOrPut` (`result.value_ptr`).
    pub fn value_at_mut(&mut self, index: usize) -> &mut V {
        &mut self.entries[index].1
    }

    /// Read access to a value by entry index.
    pub fn value_at(&self, index: usize) -> &V {
        &self.entries[index].1
    }

    /// Faithful port of `put`: insert or overwrite. If `key` already exists, its
    /// value is replaced (position preserved); otherwise a new entry is appended.
    pub fn put(&mut self, key: K, value: V) {
        if let Some(&i) = self.index.get(&key) {
            self.entries[i].1 = value;
        } else {
            self.append(key, value);
        }
    }

    /// `try map.put(alloc, key, value)`: [`Self::put`], failing with
    /// `error.OutOfMemory` (map unchanged) instead of aborting.
    pub fn try_put(&mut self, key: K, value: V) -> Result<(), OutOfMemory> {
        if let Some(&i) = self.index.get(&key) {
            self.entries[i].1 = value;
        } else {
            self.try_reserve_one()?;
            self.append(key, value);
        }
        Ok(())
    }

    /// Faithful port of `putNoClobber`: asserts `key` is not already present,
    /// then appends. Panics (Zig: `assert(!found_existing)`) if the key exists.
    pub fn put_no_clobber(&mut self, key: K, value: V) {
        assert!(
            !self.index.contains_key(&key),
            "putNoClobber: key already present"
        );
        self.append(key, value);
    }

    /// `try map.putNoClobber(alloc, key, value)`: [`Self::put_no_clobber`],
    /// failing with `error.OutOfMemory` (map unchanged) instead of aborting.
    pub fn try_put_no_clobber(&mut self, key: K, value: V) -> Result<(), OutOfMemory> {
        assert!(
            !self.index.contains_key(&key),
            "putNoClobber: key already present"
        );
        self.try_reserve_one()?;
        self.append(key, value);
        Ok(())
    }

    /// Internal: remove the entry at `index` by swapping the last entry into its
    /// slot. O(1); changes order. Faithful port of `swapRemoveAt` / `.swap`.
    fn swap_remove_at(&mut self, index: usize) -> (K, V) {
        let last = self.entries.len() - 1;
        // Remove the index mapping for the entry being deleted.
        let removed = self.entries.swap_remove(index);
        self.index.remove(&removed.0);
        // If we swapped a different element into `index`, fix its index mapping.
        // (The moved key is already in the index, so this overwrites in place
        // and never allocates.)
        if index != last {
            let moved_key = self.entries[index].0.clone();
            self.index.insert(moved_key, index);
        }
        removed
    }

    /// Internal: remove the entry at `index` by shifting all following entries
    /// forward. O(n); preserves order. Faithful port of `orderedRemoveAt` /
    /// `.ordered`.
    fn ordered_remove_at(&mut self, index: usize) -> (K, V) {
        let removed = self.entries.remove(index);
        self.index.remove(&removed.0);
        // Every entry after `index` shifted down by one; repair their indices.
        for (i, (k, _)) in self.entries.iter().enumerate().skip(index) {
            self.index.insert(k.clone(), i);
        }
        removed
    }

    /// Faithful port of `swapRemove(key)`: remove by swapping with the last
    /// element. Returns `true` if an entry was removed. CHANGES order.
    pub fn swap_remove(&mut self, key: &K) -> bool {
        match self.index.get(key).copied() {
            Some(i) => {
                self.swap_remove_at(i);
                true
            }
            None => false,
        }
    }

    /// Faithful port of `orderedRemove(key)`: remove by shifting following
    /// elements forward. Returns `true` if an entry was removed. PRESERVES order.
    pub fn ordered_remove(&mut self, key: &K) -> bool {
        match self.index.get(key).copied() {
            Some(i) => {
                self.ordered_remove_at(i);
                true
            }
            None => false,
        }
    }

    /// Faithful port of `fetchSwapRemove`: like `swapRemove` but returns the
    /// removed key/value pair.
    pub fn fetch_swap_remove(&mut self, key: &K) -> Option<KV<K, V>> {
        match self.index.get(key).copied() {
            Some(i) => {
                let (k, v) = self.swap_remove_at(i);
                Some(KV { key: k, value: v })
            }
            None => None,
        }
    }

    /// Faithful port of `fetchOrderedRemove`: like `orderedRemove` but returns
    /// the removed key/value pair.
    pub fn fetch_ordered_remove(&mut self, key: &K) -> Option<KV<K, V>> {
        match self.index.get(key).copied() {
            Some(i) => {
                let (k, v) = self.ordered_remove_at(i);
                Some(KV { key: k, value: v })
            }
            None => None,
        }
    }

    /// Faithful port of `swapRemoveAt(index)`: remove the entry at `index` by
    /// swapping in the last element. Asserts `index` is in bounds.
    pub fn swap_remove_index(&mut self, index: usize) {
        assert!(
            index < self.entries.len(),
            "swapRemoveAt: index out of bounds"
        );
        self.swap_remove_at(index);
    }

    /// Faithful port of `orderedRemoveAt(index)`: remove the entry at `index` by
    /// shifting. Asserts `index` is in bounds.
    pub fn ordered_remove_index(&mut self, index: usize) {
        assert!(
            index < self.entries.len(),
            "orderedRemoveAt: index out of bounds"
        );
        self.ordered_remove_at(index);
    }

    /// Remove and return the last-inserted entry, or `None` if empty. Faithful
    /// port of `pop` (which removes the final entry in the array).
    pub fn pop(&mut self) -> Option<KV<K, V>> {
        match self.entries.pop() {
            Some((k, v)) => {
                self.index.remove(&k);
                Some(KV { key: k, value: v })
            }
            None => None,
        }
    }

    /// Empties the map but retains allocated capacity. Faithful port of
    /// `clearRetainingCapacity`.
    pub fn clear_retaining_capacity(&mut self) {
        self.entries.clear();
        self.index.clear();
    }

    /// Empties the map and frees its storage. Faithful port of `clearAndFree`
    /// (and of `deinit` followed by `.empty`).
    pub fn clear_and_free(&mut self) {
        let alloc = self.entries.allocator().clone();
        self.entries = AVec::new_in(alloc.clone());
        self.index = Index::with_hasher_in(RandomState::new(), alloc);
    }

    /// Sorts the entries in place using the given `less_than` comparator, which
    /// is given the entry INDICES (`a_index`, `b_index`) into the current
    /// `keys()`/`values()` arrays — exactly the `Context.lessThan(ctx, a_index,
    /// b_index)` signature of `std.ArrayHashMap.sort`. The comparator reads the
    /// keys array (captured by the caller's context) at those indices. Faithful
    /// port of `sort(ctx)`: Zig sorts the parallel entries arrays with a STABLE
    /// sort using the index-based comparator, then rebuilds the index header.
    //
    // PORT NOTE: Zig's `sort` calls `std.sort.insertionContext` (stable) with the
    // entries' current positions. We snapshot the keys, run a stable sort over
    // `0..len` with the caller's index comparator, then permute the entries in
    // place and rebuild the side index so `getIndex`/lookup stay correct. The
    // permutation scratch (`order`) is on the Rust heap; the entries never leave
    // the map's own storage.
    pub fn sort<F>(&mut self, mut less_than: F)
    where
        F: FnMut(usize, usize) -> bool,
    {
        let len = self.entries.len();
        let mut order: Vec<usize> = (0..len).collect();
        // Stable sort (Zig's sort context here is `insertionContext`, stable).
        order.sort_by(|&a, &b| {
            if less_than(a, b) {
                std::cmp::Ordering::Less
            } else if less_than(b, a) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        // Apply the permutation in place: slot `i` must receive the entry that
        // was at `order[i]`. If that slot is already filled (`j < i`), step
        // `j` swapped its original entry forward along `order`, so follow the
        // chain until it names a slot at or after `i`.
        for i in 0..len {
            let mut j = order[i];
            while j < i {
                j = order[j];
            }
            self.entries.swap(i, j);
        }
        self.index.clear();
        for (i, (k, _)) in self.entries.iter().enumerate() {
            self.index.insert(k.clone(), i);
        }
    }
}

impl<K, V, A> AutoArrayHashMapUnmanaged<K, V, A>
where
    K: Eq + Hash + Clone,
    V: Clone,
    A: Allocator + Clone,
{
    /// Deep copy of the map (entries + index). Faithful port of
    /// `clone(alloc)` — Zig allocates a fresh entries array and index header and
    /// copies every key/value. Insertion order is preserved.
    //
    // PORT NOTE: named `clone` to match the Zig `clone(alloc)` it ports; the
    // `should_implement_trait` lint is allowed for that fidelity (callers invoke
    // `map.clone()` mirroring `try self.map.clone(alloc)`).
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Self {
        AutoArrayHashMapUnmanaged {
            entries: self.entries.clone(),
            index: self.index.clone(),
        }
    }
}

/// Faithful alias for `AutoArrayHashMap(K, V)`. Since our backing store owns its
/// allocations via `Vec`/`HashMap`, the managed form is identical to the
/// unmanaged form (Zig's managed wrapper only threads the allocator).
pub type AutoArrayHashMap<K, V, A = Global> = AutoArrayHashMapUnmanaged<K, V, A>;

#[cfg(test)]
mod tests {
    use super::*;

    /// keys()/values()/iter() observe insertion order, not hash order.
    #[test]
    fn insertion_order_iteration() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        // Insert in a deliberately non-sorted order.
        m.put(30, 300);
        m.put(10, 100);
        m.put(20, 200);
        m.put(5, 50);

        let keys: Vec<u32> = m.keys().copied().collect();
        let vals: Vec<u32> = m.values().copied().collect();
        assert_eq!(keys, vec![30, 10, 20, 5]);
        assert_eq!(vals, vec![300, 100, 200, 50]);

        let pairs: Vec<(u32, u32)> = m.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(pairs, vec![(30, 300), (10, 100), (20, 200), (5, 50)]);
    }

    /// put on an existing key overwrites in place (position preserved).
    #[test]
    fn put_overwrites_in_place() {
        let mut m: AutoArrayHashMap<&'static str, i32> = AutoArrayHashMap::new();
        m.put("a", 1);
        m.put("b", 2);
        m.put("c", 3);
        m.put("b", 22); // overwrite middle
        let keys: Vec<&str> = m.keys().copied().collect();
        assert_eq!(keys, vec!["a", "b", "c"]); // order unchanged
        assert_eq!(m.get(&"b"), Some(&22));
        assert_eq!(m.count(), 3);
    }

    /// getOrPut: found vs not-found behavior, index + found_existing.
    #[test]
    fn get_or_put_found_and_not_found() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();

        let r1 = m.get_or_put_value(1, 111);
        assert!(!r1.found_existing);
        assert_eq!(r1.index, 0);
        assert_eq!(m.value_at(r1.index), &111);

        let r2 = m.get_or_put_value(2, 222);
        assert!(!r2.found_existing);
        assert_eq!(r2.index, 1);

        // Existing key: found, value untouched, index points at original slot.
        let r3 = m.get_or_put_value(1, 999);
        assert!(r3.found_existing);
        assert_eq!(r3.index, 0);
        assert_eq!(m.value_at(r3.index), &111); // not overwritten

        // get_or_put_with: set value via returned index.
        let r4 = m.get_or_put_with(3, || 0);
        assert!(!r4.found_existing);
        *m.value_at_mut(r4.index) = 333;
        assert_eq!(m.get(&3), Some(&333));
    }

    /// putNoClobber appends; double-insert panics.
    #[test]
    fn put_no_clobber() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        m.put_no_clobber(1, 10);
        m.put_no_clobber(2, 20);
        assert_eq!(m.count(), 2);
        assert_eq!(m.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    #[should_panic(expected = "putNoClobber")]
    fn put_no_clobber_panics_on_dup() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        m.put_no_clobber(1, 10);
        m.put_no_clobber(1, 20);
    }

    /// swapRemove: O(1), swaps last into the hole, CHANGES order. Lookups stay
    /// correct after the swap.
    #[test]
    fn swap_remove_changes_order() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        for k in [1u32, 2, 3, 4, 5] {
            m.put(k, k * 10);
        }
        // Remove the entry at key 2 (index 1). Last entry (key 5) swaps in.
        assert!(m.swap_remove(&2));
        let keys: Vec<u32> = m.keys().copied().collect();
        assert_eq!(keys, vec![1, 5, 3, 4]); // 5 took 2's slot
        assert_eq!(m.count(), 4);
        // All remaining lookups correct.
        assert_eq!(m.get(&5), Some(&50));
        assert_eq!(m.get_index(&5), Some(1));
        assert_eq!(m.get(&2), None);
        assert!(!m.contains(&2));
        // Removing a missing key returns false.
        assert!(!m.swap_remove(&2));
    }

    /// swapRemove of the last element does not move anything.
    #[test]
    fn swap_remove_last() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        for k in [1u32, 2, 3] {
            m.put(k, k);
        }
        assert!(m.swap_remove(&3));
        assert_eq!(m.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(m.get_index(&2), Some(1));
    }

    /// orderedRemove: O(n), shifts following entries, PRESERVES order. Lookups
    /// stay correct after the shift.
    #[test]
    fn ordered_remove_preserves_order() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        for k in [1u32, 2, 3, 4, 5] {
            m.put(k, k * 10);
        }
        // Remove the entry at key 2 (index 1); 3,4,5 shift forward.
        assert!(m.ordered_remove(&2));
        let keys: Vec<u32> = m.keys().copied().collect();
        assert_eq!(keys, vec![1, 3, 4, 5]); // order preserved
        assert_eq!(m.count(), 4);
        // Indices repaired.
        assert_eq!(m.get_index(&1), Some(0));
        assert_eq!(m.get_index(&3), Some(1));
        assert_eq!(m.get_index(&4), Some(2));
        assert_eq!(m.get_index(&5), Some(3));
        assert_eq!(m.get(&5), Some(&50));
        assert!(!m.ordered_remove(&2));
    }

    /// Direct comparison of swapRemove vs orderedRemove ordering on identical
    /// inputs (the core distinguishing semantic).
    #[test]
    fn swap_vs_ordered_ordering_contrast() {
        let make = || {
            let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
            for k in [10u32, 20, 30, 40] {
                m.put(k, k);
            }
            m
        };

        let mut a = make();
        a.swap_remove(&20); // index 1 removed, last (40) swaps in
        assert_eq!(a.keys().copied().collect::<Vec<_>>(), vec![10, 40, 30]);

        let mut b = make();
        b.ordered_remove(&20); // index 1 removed, 30/40 shift
        assert_eq!(b.keys().copied().collect::<Vec<_>>(), vec![10, 30, 40]);
    }

    /// fetch variants return the removed key/value.
    #[test]
    fn fetch_remove_variants() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        for k in [1u32, 2, 3] {
            m.put(k, k * 100);
        }
        let kv = m.fetch_swap_remove(&2).unwrap();
        assert_eq!(kv, KV { key: 2, value: 200 });
        assert_eq!(m.fetch_swap_remove(&999), None);

        let mut m2: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        for k in [1u32, 2, 3] {
            m2.put(k, k * 100);
        }
        let kv2 = m2.fetch_ordered_remove(&1).unwrap();
        assert_eq!(kv2, KV { key: 1, value: 100 });
        assert_eq!(m2.keys().copied().collect::<Vec<_>>(), vec![2, 3]);
    }

    /// get / getPtr / getIndex / contains / count basics.
    #[test]
    fn accessors() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        m.put(7, 70);
        assert_eq!(m.count(), 1);
        assert!(m.contains(&7));
        assert_eq!(m.get(&7), Some(&70));
        assert_eq!(m.get_index(&7), Some(0));
        *m.get_ptr(&7).unwrap() = 71;
        assert_eq!(m.get(&7), Some(&71));
        assert_eq!(m.get(&8), None);
        assert_eq!(m.get_ptr(&8), None);
    }

    /// `*Pin -> void` set usage as in PageList's PinSet (pointer keys, unit
    /// values), with swapRemove (PageList: `tracked_pins.swapRemove(p)`).
    #[test]
    fn pointer_key_void_value_set() {
        let a = Box::new(1u8);
        let b = Box::new(2u8);
        let pa: *const u8 = &*a;
        let pb: *const u8 = &*b;
        let mut set: AutoArrayHashMapUnmanaged<*const u8, ()> = AutoArrayHashMapUnmanaged::new();
        set.put(pa, ());
        set.put(pb, ());
        assert_eq!(set.count(), 2);
        assert!(set.contains(&pa));
        assert!(set.swap_remove(&pa));
        assert!(!set.contains(&pa));
        assert!(set.contains(&pb));
        assert_eq!(set.count(), 1);
    }

    /// pop removes the last-inserted entry.
    #[test]
    fn pop_last() {
        let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
        m.put(1, 1);
        m.put(2, 2);
        let kv = m.pop().unwrap();
        assert_eq!(kv, KV { key: 2, value: 2 });
        assert_eq!(m.count(), 1);
        assert!(!m.contains(&2));
        assert_eq!(m.pop().unwrap(), KV { key: 1, value: 1 });
        assert_eq!(m.pop(), None);
    }

    /// `sort` applies the stable index-comparator order in place (every
    /// permutation of 5 keys).
    #[test]
    fn sort_applies_the_permutation_in_place() {
        fn perms(v: &mut Vec<u32>, k: usize, out: &mut Vec<Vec<u32>>) {
            if k == v.len() {
                out.push(v.clone());
                return;
            }
            for i in k..v.len() {
                v.swap(k, i);
                perms(v, k + 1, out);
                v.swap(k, i);
            }
        }
        let mut all = Vec::new();
        perms(&mut vec![4, 1, 3, 0, 2], 0, &mut all);
        assert_eq!(all.len(), 120);
        for keys in all {
            let mut m: AutoArrayHashMap<u32, u32> = AutoArrayHashMap::new();
            for &k in &keys {
                m.put(k, k * 10);
            }
            let snapshot = keys.clone();
            m.sort(|a, b| snapshot[a] < snapshot[b]);
            assert_eq!(m.keys().copied().collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
            assert_eq!(
                m.values().copied().collect::<Vec<_>>(),
                vec![0, 10, 20, 30, 40]
            );
            for k in 0..5u32 {
                assert_eq!(m.get_index(&k), Some(k as usize));
            }
        }
    }

    /// An allocator that counts live blocks and can be told to refuse.
    #[derive(Clone, Copy)]
    struct Probe<'a> {
        live: &'a core::cell::Cell<isize>,
        allocs: &'a core::cell::Cell<usize>,
        refuse: &'a core::cell::Cell<bool>,
    }

    unsafe impl allocator_api2::alloc::Allocator for Probe<'_> {
        fn allocate(
            &self,
            layout: core::alloc::Layout,
        ) -> Result<core::ptr::NonNull<[u8]>, allocator_api2::alloc::AllocError> {
            if self.refuse.get() {
                return Err(allocator_api2::alloc::AllocError);
            }
            let p = Global.allocate(layout)?;
            self.allocs.set(self.allocs.get() + 1);
            self.live.set(self.live.get() + 1);
            Ok(p)
        }

        unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
            self.live.set(self.live.get() - 1);
            unsafe { Global.deallocate(ptr, layout) }
        }
    }

    /// The map's storage (entries AND index) goes through its allocator, and
    /// the `try_*` growth returns `OutOfMemory` with the map unchanged.
    #[test]
    fn storage_routes_through_the_allocator_and_try_growth_fails_cleanly() {
        let live = core::cell::Cell::new(0isize);
        let allocs = core::cell::Cell::new(0usize);
        let refuse = core::cell::Cell::new(false);
        let probe = Probe {
            live: &live,
            allocs: &allocs,
            refuse: &refuse,
        };
        {
            let mut m: AutoArrayHashMapUnmanaged<u32, u32, Probe<'_>> =
                AutoArrayHashMapUnmanaged::new_in(probe);
            assert_eq!(allocs.get(), 0, "`.empty` allocates nothing");
            m.try_put_no_clobber(1, 10).unwrap();
            // One block for the entries array and one for the index.
            assert_eq!(allocs.get(), 2);
            assert_eq!(live.get(), 2);

            // Fill, then refuse: the next growth must fail cleanly.
            while m.count() < 64 {
                let k = m.count() as u32 + 1;
                m.try_put(k, k).unwrap();
            }
            refuse.set(true);
            let mut failed = false;
            for k in 1000..2000u32 {
                if m.try_put_no_clobber(k, k) == Err(OutOfMemory) {
                    failed = true;
                    assert!(!m.contains(&k), "a failed put leaves the map unchanged");
                    break;
                }
            }
            assert!(failed, "growth past capacity must reach the refusing allocator");
            let before = m.count();
            // An existing key needs no allocation.
            assert!(m.try_put(1, 11).is_ok());
            assert_eq!(m.get(&1), Some(&11));
            assert_eq!(m.count(), before);
            refuse.set(false);

            m.clear_and_free();
            assert_eq!(live.get(), 0, "clearAndFree returns the storage");
            m.try_put(7, 70).unwrap();
            assert!(live.get() > 0);
        }
        assert_eq!(live.get(), 0, "drop frees through the allocator");
    }
}
