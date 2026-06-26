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
//! Backing memory is the Rust system allocator (`std::alloc`) via `Vec` and
//! `std::collections::HashMap` for the index — the faithful map of Zig's `gpa`.
//!
//! Implementation choice (parity, sound): the Zig original hand-rolls an
//! `IndexHeader` open-addressing index alongside the entries array; we reproduce
//! the same OBSERVABLE behavior using a `Vec<(K, V)>` for the ordered entries
//! plus a `HashMap<K, usize>` mapping key -> entry index. This is pure-safe Rust
//! (no `unsafe`) and reproduces every documented semantic above exactly. Indices
//! that shift on removal are repaired so `getIndex`/lookup stay correct.

use std::collections::HashMap;
use std::hash::Hash;

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
/// map. "Unmanaged" in Zig means it does not store its allocator; here we use
/// the global allocator via `Vec`/`HashMap`, so the managed/unmanaged
/// distinction collapses — `AutoArrayHashMap` is a type alias.
///
/// `K: Eq + Hash + Clone` (Clone is needed for the side index; Zig copies keys
/// into both the entries array and the index too).
pub struct AutoArrayHashMapUnmanaged<K, V> {
    /// Entries in insertion order. `keys()`/`values()` read from here.
    entries: Vec<(K, V)>,
    /// key -> index into `entries`, for O(1) lookup (Zig's IndexHeader).
    index: HashMap<K, usize>,
}

impl<K, V> Default for AutoArrayHashMapUnmanaged<K, V>
where
    K: Eq + Hash + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> AutoArrayHashMapUnmanaged<K, V>
where
    K: Eq + Hash + Clone,
{
    /// The empty map. Faithful port of `.empty` / default-init.
    pub fn new() -> Self {
        AutoArrayHashMapUnmanaged {
            entries: Vec::new(),
            index: HashMap::new(),
        }
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
            let i = self.entries.len();
            self.entries.push((key.clone(), make_default()));
            self.index.insert(key, i);
            GetOrPutResult {
                index: i,
                found_existing: false,
            }
        }
    }

    /// Faithful port of `getOrPutValue`: get-or-insert, and if newly inserted set
    /// the value to `value`. Returns the result (with `found_existing`).
    pub fn get_or_put_value(&mut self, key: K, value: V) -> GetOrPutResult {
        if let Some(&i) = self.index.get(&key) {
            GetOrPutResult {
                index: i,
                found_existing: true,
            }
        } else {
            let i = self.entries.len();
            self.entries.push((key.clone(), value));
            self.index.insert(key, i);
            GetOrPutResult {
                index: i,
                found_existing: false,
            }
        }
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
            let i = self.entries.len();
            self.entries.push((key.clone(), value));
            self.index.insert(key, i);
        }
    }

    /// Faithful port of `putNoClobber`: asserts `key` is not already present,
    /// then appends. Panics (Zig: `assert(!found_existing)`) if the key exists.
    pub fn put_no_clobber(&mut self, key: K, value: V) {
        assert!(
            !self.index.contains_key(&key),
            "putNoClobber: key already present"
        );
        let i = self.entries.len();
        self.entries.push((key.clone(), value));
        self.index.insert(key, i);
    }

    /// Internal: remove the entry at `index` by swapping the last entry into its
    /// slot. O(1); changes order. Faithful port of `swapRemoveAt` / `.swap`.
    fn swap_remove_at(&mut self, index: usize) -> (K, V) {
        let last = self.entries.len() - 1;
        // Remove the index mapping for the entry being deleted.
        let removed = self.entries.swap_remove(index);
        self.index.remove(&removed.0);
        // If we swapped a different element into `index`, fix its index mapping.
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
    // `0..len` with the caller's index comparator, then permute the entries and
    // rebuild the side index so `getIndex`/lookup stay correct.
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
        // Move the entries into the sorted order. Stage each slot through an
        // `Option` so we can move (not clone) the owned `(K, V)` out exactly once.
        let mut staging: Vec<Option<(K, V)>> = self.entries.drain(..).map(Some).collect();
        let mut new_entries: Vec<(K, V)> = Vec::with_capacity(len);
        for &i in &order {
            new_entries.push(staging[i].take().expect("sort: each index visited once"));
        }
        self.entries = new_entries;
        self.index.clear();
        for (i, (k, _)) in self.entries.iter().enumerate() {
            self.index.insert(k.clone(), i);
        }
    }
}

impl<K, V> AutoArrayHashMapUnmanaged<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
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
pub type AutoArrayHashMap<K, V> = AutoArrayHashMapUnmanaged<K, V>;

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
}
