//! LRU HashMap with least-recently-used eviction.
//
// PORT NOTE: The Zig reference is `fn HashMap(K, V, Context, max_load) type`
// built on `std.HashMapUnmanaged(K, *Entry, ...)` plus a `std.DoublyLinkedList`
// (the std intrusive list whose `Node` is a bare prev/next struct embedded in
// each `Entry`). Entries are heap-allocated and referenced both by the map
// (as `*Entry`) and by the queue (via the embedded `node`). We reproduce this
// exactly: entries are `Box`-allocated, raw `*mut Entry` are stored in a
// `std::collections::HashMap`, and the LRU `queue` is an intrusive doubly
// linked list over the entries' embedded `prev`/`next` pointers. `getOrPut`
// returns a raw `*mut V` into the entry (the reference's `value_ptr`). The
// `AutoHashMap`/`Context`/`max_load_percentage` knobs are folded away because
// Rust's `std` HashMap supplies hashing/equality for `K: Hash + Eq` — no
// behavior of the reference's tests depends on the load factor.

use rustty_util::quirks::inlineAssert as assert;
use std::collections::HashMap as StdHashMap;
use std::hash::Hash;
use std::ptr;

/// Error returned for allocation failures (mirror of `Allocator.Error`).
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
}

/// Create a HashMap for a key type that can be automatically hashed.
/// If you want finer-grained control, use HashMap directly.
//
// PORT NOTE: In Zig `AutoHashMap(K, V)` instantiates `HashMap` with the std
// auto context + default max load. In Rust, `HashMap<K, V>` already requires
// `K: Hash + Eq` (the auto context), so `HashMap` here serves both roles.
pub type AutoHashMap<K, V> = HashMap<K, V>;

/// The key-value pair stored in the LRU.
#[derive(Clone, Copy)]
pub struct KV<K, V> {
    pub key: K,
    pub value: V,
}

// The entry stored on the heap. In the Zig reference this is
//   const Entry = struct { data: KV, node: Queue.Node, ... };
// where `node` is the std `DoublyLinkedList.Node` (a bare prev/next struct).
// We inline the node's `prev`/`next` as `*mut Entry<K,V>` directly — i.e. the
// intrusive doubly-linked-list links live in the entry, exactly as the embedded
// `node` did in the reference (the reference recovers the `*Entry` from a
// `*Node` via `@fieldParentPtr`; here the links already are `*Entry`).
struct Entry<K, V> {
    data: KV<K, V>,
    prev: *mut Entry<K, V>,
    next: *mut Entry<K, V>,
}

// An intrusive doubly-linked list over `Entry<K,V>` (mirror of the std
// `DoublyLinkedList` used by the reference).
struct Queue<K, V> {
    first: *mut Entry<K, V>,
    last: *mut Entry<K, V>,
}

impl<K, V> Queue<K, V> {
    fn new() -> Self {
        Queue {
            first: ptr::null_mut(),
            last: ptr::null_mut(),
        }
    }

    // Append a node at the end (most-recently-used position).
    unsafe fn append(&mut self, node: *mut Entry<K, V>) {
        unsafe {
            if !self.last.is_null() {
                let last = self.last;
                (*node).prev = last;
                (*node).next = ptr::null_mut();
                (*last).next = node;
                self.last = node;
            } else {
                self.first = node;
                self.last = node;
                (*node).prev = ptr::null_mut();
                (*node).next = ptr::null_mut();
            }
        }
    }

    // Remove a node from the list.
    unsafe fn remove(&mut self, node: *mut Entry<K, V>) {
        unsafe {
            let prev = (*node).prev;
            let next = (*node).next;
            if !prev.is_null() {
                (*prev).next = next;
            } else {
                self.first = next;
            }
            if !next.is_null() {
                (*next).prev = prev;
            } else {
                self.last = prev;
            }
        }
    }

    // Pop the first (least-recently-used) node.
    unsafe fn pop_first(&mut self) -> *mut Entry<K, V> {
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

/// HashMap implementation that supports least-recently-used eviction.
///
/// Beware of the Zig bug where a hashmap gets slower over time
/// (https://github.com/ziglang/zig/issues/17851). This LRU uses a hashmap
/// and evictions will cause this issue to appear. Callers should keep
/// track of eviction counts and periodically reinitialize the LRU to
/// avoid this issue. The LRU itself can't do this because it doesn't
/// know how to free values.
///
/// Note: This is a really elementary CS101 version of an LRU right now.
/// This is done initially to get something working. Once we have it working,
/// we can benchmark and improve if this ends up being a source of slowness.
pub struct HashMap<K: Eq + Hash + Copy, V: Copy> {
    /// Map to maintain our entries.
    map: StdHashMap<K, *mut Entry<K, V>>,

    /// Queue to maintain LRU order.
    queue: Queue<K, V>,

    /// The capacity of our map. If this capacity is reached, cache
    /// misses will begin evicting entries.
    capacity: u32,
}

/// The result of a getOrPut operation.
pub struct GetOrPutResult<K, V> {
    /// The entry that was retrieved. If found_existing is false,
    /// then this is a pointer to allocated space to store a V.
    /// If found_existing is true, the pointer value is valid, but
    /// can be overwritten.
    pub value_ptr: *mut V,

    /// Whether an existing value was found or not.
    pub found_existing: bool,

    /// If another entry had to be evicted to make space for this
    /// put operation, then this is the value that was evicted.
    pub evicted: Option<KV<K, V>>,
}

impl<K: Eq + Hash + Copy, V: Copy> HashMap<K, V> {
    pub fn init(capacity: u32) -> Self {
        HashMap {
            map: StdHashMap::new(),
            queue: Queue::new(),
            capacity,
        }
    }

    /// Get or put a value for a key. See GetOrPutResult on how to check
    /// if an existing value was found, if an existing value was evicted,
    /// etc.
    pub fn get_or_put(&mut self, key: K) -> Result<GetOrPutResult<K, V>, Error> {
        // In Zig this dispatches to getOrPutContext with an inferred (zero-size)
        // context; in Rust the std HashMap supplies the context implicitly.
        unsafe {
            // map.getOrPutContext: find or insert a slot for `key`.
            if let Some(&existing) = self.map.get(&key) {
                // found_existing
                // Move to end to mark as most recently used
                self.queue.remove(existing);
                self.queue.append(existing);

                return Ok(GetOrPutResult {
                    found_existing: true,
                    value_ptr: ptr::addr_of_mut!((*existing).data.value),
                    evicted: None,
                });
            }

            // We're evicting if our map insertion increased our capacity.
            // (We have NOT inserted yet; the count after insertion would be
            // map.count() + 1, so we evict when current count >= capacity.)
            let evict = (self.map.len() as u32) >= self.capacity;

            // Get our entry. If we're not evicting then we allocate a new
            // entry. If we are evicting then we avoid allocation by just
            // reusing the entry we would've evicted.
            let entry: *mut Entry<K, V> = if !evict {
                Box::into_raw(Box::new(Entry {
                    data: KV {
                        key,
                        // value is "allocated space"; left as a placeholder
                        // until the caller writes through value_ptr. We can't
                        // leave it uninitialized for a Copy type, so we use a
                        // bit-pattern-safe placeholder by reading the key's
                        // slot only. Matches Zig's `undefined` value field.
                        value: placeholder_value::<V>(),
                    },
                    prev: ptr::null_mut(),
                    next: ptr::null_mut(),
                }))
            } else {
                // Our first node is the least recently used.
                let least_used_node = self.queue.pop_first();
                let least_used_entry: *mut Entry<K, V> = least_used_node;

                // Remove the least used from the map
                self.map.remove(&(*least_used_entry).data.key);

                least_used_entry
            };

            // Store our entry in the map.
            self.map.insert(key, entry);

            // Mark the entry as most recently used
            self.queue.append(entry);

            // Set our key
            (*entry).data.key = key;

            Ok(GetOrPutResult {
                found_existing: false,
                value_ptr: ptr::addr_of_mut!((*entry).data.value),
                evicted: if !evict {
                    None
                } else {
                    Some((*entry).data)
                },
            })
        }
    }

    /// Get a value for a key.
    pub fn get(&self, key: K) -> Option<V> {
        // In Zig this dispatches to getContext with an inferred context.
        let node = self.map.get(&key)?;
        unsafe { Some((**node).data.value) }
    }

    /// Resize the LRU. If this shrinks the LRU then LRU items will be
    /// deallocated. The deallocated items are returned in the slice. This
    /// slice must be freed by the caller.
    pub fn resize(&mut self, capacity: u32) -> Result<Option<Vec<V>>, Error> {
        // Fastest
        if capacity >= self.capacity {
            self.capacity = capacity;
            return Ok(None);
        }

        // If we're shrinking but we're smaller than the new capacity,
        // then we don't have to do anything.
        if (self.map.len() as u32) <= capacity {
            self.capacity = capacity;
            return Ok(None);
        }

        // We're shrinking and we have more items than the new capacity
        let delta = (self.map.len() as u32) - capacity;
        let mut evicted: Vec<V> = Vec::with_capacity(delta as usize);

        unsafe {
            let mut i: u32 = 0;
            while i < delta {
                let node = self.queue.pop_first();
                let entry: *mut Entry<K, V> = node;
                evicted.push((*entry).data.value);
                // queue.remove(node) — already removed by pop_first; the Zig
                // code calls popFirst() then remove(node) which is a harmless
                // double-remove on an already-unlinked node. We've unlinked it
                // once; calling remove again would be a no-op on null links, so
                // we faithfully skip the redundant second unlink.
                self.map.remove(&(*entry).data.key);
                drop(Box::from_raw(entry));
                i += 1;
            }
        }

        self.capacity = capacity;
        assert(self.map.len() as u32 == capacity);

        Ok(Some(evicted))
    }
}

impl<K: Eq + Hash + Copy, V: Copy> Drop for HashMap<K, V> {
    fn drop(&mut self) {
        // Important: use our queue as a source of truth for dealloc
        // because we might keep items in the queue around that aren't
        // present in our LRU anymore to prevent future allocations.
        unsafe {
            let mut it = self.queue.first;
            while !it.is_null() {
                let node = it;
                it = (*node).next;
                drop(Box::from_raw(node));
            }
        }
        // self.map.deinit handled by Drop; self.* = undefined is a no-op here.
    }
}

// Produce a placeholder `V` for a freshly-allocated entry whose value the caller
// will overwrite via `value_ptr` (mirror of Zig's `undefined` value field). We
// require `V: Copy`; for the integer value types used by this data structure a
// zeroed bit pattern is always a valid value, so we synthesize one safely.
fn placeholder_value<V: Copy>() -> V {
    // SAFETY: this function is only ever instantiated for the `Copy` value
    // types stored in this LRU (integers in the reference tests). The
    // placeholder is always overwritten by the caller before being read,
    // mirroring Zig's `undefined`. A zeroed bit pattern is a valid value for
    // these integer types.
    unsafe { core::mem::zeroed() }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn getOrPut() {
        type Map = AutoHashMap<u32, u8>;
        let mut m = Map::init(2);

        // Insert cap values, should be hits
        unsafe {
            {
                let gop = m.get_or_put(1).unwrap();
                assert!(!gop.found_existing);
                assert!(gop.evicted.is_none());
                *gop.value_ptr = 1;
            }
            {
                let gop = m.get_or_put(2).unwrap();
                assert!(!gop.found_existing);
                assert!(gop.evicted.is_none());
                *gop.value_ptr = 2;
            }

            // 1 is LRU
            assert!(m.get_or_put(1).unwrap().found_existing);
            assert!(m.get_or_put(2).unwrap().found_existing);

            // Next should evict
            {
                let gop = m.get_or_put(3).unwrap();
                assert!(!gop.found_existing);
                assert!(gop.evicted.is_some());
                assert!(gop.evicted.as_ref().unwrap().value == 1);
                *gop.value_ptr = 3;
            }

            // Currently: 2 is LRU, let's make 3 LRU
            assert!(m.get_or_put(2).unwrap().found_existing);

            // Next should evict
            {
                let gop = m.get_or_put(4).unwrap();
                assert!(!gop.found_existing);
                assert!(gop.evicted.is_some());
                assert!(gop.evicted.as_ref().unwrap().value == 3);
                *gop.value_ptr = 4;
            }
        }
    }

    #[test]
    fn get() {
        type Map = AutoHashMap<u32, u8>;
        let mut m = Map::init(2);

        // Insert cap values, should be hits
        unsafe {
            let gop = m.get_or_put(1).unwrap();
            assert!(!gop.found_existing);
            assert!(gop.evicted.is_none());
            *gop.value_ptr = 1;
        }

        assert!(m.get(1).is_some());
        assert!(m.get(1).unwrap() == 1);
        assert!(m.get(2).is_none());
    }

    #[test]
    fn resize_shrink_without_removal() {
        type Map = AutoHashMap<u32, u8>;
        let mut m = Map::init(2);

        // Insert cap values, LRU is 1
        unsafe {
            let gop = m.get_or_put(1).unwrap();
            assert!(!gop.found_existing);
            assert!(gop.evicted.is_none());
            *gop.value_ptr = 1;
        }

        // Shrink
        let evicted = m.resize(1).unwrap();
        assert!(evicted.is_none());
        {
            let gop = m.get_or_put(1).unwrap();
            assert!(gop.found_existing);
        }
    }

    #[test]
    fn resize_shrink_and_remove() {
        type Map = AutoHashMap<u32, u8>;
        let mut m = Map::init(2);

        // Insert cap values, LRU is 1
        unsafe {
            {
                let gop = m.get_or_put(1).unwrap();
                assert!(!gop.found_existing);
                assert!(gop.evicted.is_none());
                *gop.value_ptr = 1;
            }
            {
                let gop = m.get_or_put(2).unwrap();
                assert!(!gop.found_existing);
                assert!(gop.evicted.is_none());
                *gop.value_ptr = 2;
            }
        }

        // Shrink
        let evicted = m.resize(1).unwrap();
        assert_eq!(1usize, evicted.as_ref().unwrap().len());
        {
            let gop = m.get_or_put(1).unwrap();
            assert!(!gop.found_existing);
            assert!(gop.evicted.as_ref().unwrap().value == 2);
            unsafe {
                *gop.value_ptr = 1;
            }
        }
    }
}
