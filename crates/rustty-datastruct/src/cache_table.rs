//! An associative data structure used for efficiently storing and
//! retrieving values which are able to be recomputed if necessary.
//
// PORT NOTE: The Zig reference is `fn CacheTable(K, V, Context, bucket_count,
// bucket_size) type`. `Context` supplies `hash`, `eql`, and an OPTIONAL
// `evicted` hook (detected via `@hasDecl`). In Rust we express `Context` as a
// trait with `hash`/`eql` required and `evicted` a defaulted (no-op) method;
// callers override it to register the hook, exactly reproducing the optional
// semantics. `bucket_count`/`bucket_size` are const generics. We use
// `MaybeUninit`-free storage (KV: copyable) initialized to defaults — the Zig
// `undefined` buckets are never read outside a bucket's stored length, so
// default-initializing them is observationally identical and Miri-safe.

use rustty_util::fastmem;
use rustty_util::quirks::inlineAssert as assert;

/// `Context` trait for CacheTable behaviors.
///   - `fn hash(*Context, K) u64`    - Return a hash for a key.
///   - `fn eql(*Context, K, K) bool` - Check two keys for equality.
///
///   - `fn evicted(*Context, K, V) void` - [OPTIONAL] Eviction callback.
///     If present, called whenever an item is evicted from the cache.
pub trait Context<K, V> {
    /// Return a hash for a key.
    fn hash(&self, key: K) -> u64;
    /// Check two keys for equality.
    fn eql(&self, a: K, b: K) -> bool;
    /// [OPTIONAL] Eviction callback. If overridden, called whenever an item is
    /// evicted from the cache. The default is a no-op (i.e. "no `evicted`
    /// decl").
    fn evicted(&mut self, _key: K, _value: V) {}
}

/// A key-value pair stored in the table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KV<K, V> {
    pub key: K,
    pub value: V,
}

/// An associative data structure used for efficiently storing and
/// retrieving values which are able to be recomputed if necessary.
///
/// This structure is effectively a hash table with fixed-sized buckets.
///
/// When inserting an item in to a full bucket, the least recently used
/// item is replaced.
///
/// To achieve this, when an item is accessed, it's moved to the end of
/// the bucket, and the rest of the items are moved over to fill the gap.
///
/// This should provide very good query performance and keep frequently
/// accessed items cached indefinitely.
///
/// Parameters:
///
/// `Context`
///   A type containing methods to define CacheTable behaviors.
///   - `fn hash(*Context, K) u64`    - Return a hash for a key.
///   - `fn eql(*Context, K, K) bool` - Check two keys for equality.
///
///   - `fn evicted(*Context, K, V) void` - [OPTIONAL] Eviction callback.
///     If present, called whenever an item is evicted from the cache.
///
/// `bucket_count`
///   Should ideally be close to the median number of important items that
///   you expect to be cached at any given point. This is required to be a
///   power of 2 since performance suffers if it's not and there's no good
///   reason to allow it to be anything else.
///
/// `bucket_size`
///   should be larger if you expect a large number of unimportant items to
///   enter the cache at a time. Having larger buckets will avoid important
///   items being dropped from the cache prematurely.
///
pub struct CacheTable<K, V, C, const BUCKET_COUNT: usize, const BUCKET_SIZE: usize>
where
    K: Copy + Default,
    V: Copy + Default,
    C: Context<K, V>,
{
    /// `bucket_count` buckets containing `bucket_size` KV pairs each.
    ///
    /// We don't need to initialize this memory because we don't use it
    /// unless it's within a bucket's stored length, which will guarantee
    /// that we put actual items there.
    buckets: [[KV<K, V>; BUCKET_SIZE]; BUCKET_COUNT],

    /// We use this array to keep track of how many slots in each bucket
    /// have actual items in them. Once all the buckets fill up this will
    /// become a pointless check, but hopefully branch prediction picks
    /// up on it at that point. The memory cost isn't too bad since it's
    /// just bytes, so should be a fraction the size of the main table.
    lengths: [u8; BUCKET_COUNT],

    /// An instance of the context structure.
    /// Must be initialized before calling any operations.
    pub context: C,
}

impl<K, V, C, const BUCKET_COUNT: usize, const BUCKET_SIZE: usize>
    CacheTable<K, V, C, BUCKET_COUNT, BUCKET_SIZE>
where
    K: Copy + Default,
    V: Copy + Default,
    C: Context<K, V>,
{
    /// Create a new CacheTable with the given context.
    pub fn new(context: C) -> Self {
        // comptime { assert(isPowerOfTwo(bucket_count)); assert(bucket_count <= maxInt(usize)); }
        assert((BUCKET_COUNT & (BUCKET_COUNT - 1)) == 0);
        // bucket_count <= maxInt(usize) is trivially true for a usize const.

        CacheTable {
            buckets: [[KV {
                key: K::default(),
                value: V::default(),
            }; BUCKET_SIZE]; BUCKET_COUNT],
            lengths: [0u8; BUCKET_COUNT],
            context,
        }
    }

    /// Adds an item to the cache table. If an old value was removed to
    /// make room then it is returned in a struct with its key and value.
    pub fn put(&mut self, key: K, value: V) -> Option<KV<K, V>> {
        let kv: KV<K, V> = KV { key, value };
        let idx: usize = (self.context.hash(key) % (BUCKET_COUNT as u64)) as usize;

        // If we have space available in the bucket then we just append
        if (self.lengths[idx] as usize) < BUCKET_SIZE {
            let len = self.lengths[idx] as usize;
            self.buckets[idx][len] = kv;
            self.lengths[idx] += 1;
            return None;
        }
        assert(self.lengths[idx] as usize == BUCKET_SIZE);

        // Append our new item and return the oldest
        let evicted = fastmem::rotateIn(&mut self.buckets[idx], kv);

        // The Context is allowed to register an eviction hook.
        self.context.evicted(evicted.key, evicted.value);

        Some(evicted)
    }

    /// Retrieves an item from the cache table.
    ///
    /// Returns null if no item is found with the provided key.
    pub fn get(&mut self, key: K) -> Option<V> {
        let idx: usize = (self.context.hash(key) % (BUCKET_COUNT as u64)) as usize;
        let len = self.lengths[idx] as usize;
        let mut i: usize = len;
        while i > 0 {
            i -= 1;
            if self.context.eql(key, self.buckets[idx][i].key) {
                let value = self.buckets[idx][i].value;
                // defer fastmem.rotateOnce(KV, self.buckets[idx][i..len]);
                fastmem::rotateOnce(&mut self.buckets[idx][i..len]);
                return Some(value);
            }
        }

        None
    }

    /// Removes all items from the cache table.
    ///
    /// If your `Context` has an `evicted` method,
    /// it will be called with all removed items.
    pub fn clear(&mut self) {
        for (b, l) in self.buckets.iter().zip(self.lengths.iter()) {
            for kv in b[0..*l as usize].iter() {
                self.context.evicted(kv.key, kv.value);
            }
        }

        for l in self.lengths.iter_mut() {
            *l = 0;
        }
    }
}

/// Creates a Context automatically for the given key type. This uses the
/// same logic as std.hash_map.AutoContext today since the API matches.
//
// PORT NOTE: In Zig this is `fn AutoContext(K) type` deferring to
// `std.hash_map.AutoContext(K)`. Rust has no comptime type-returning function
// and no direct std equivalent that fits the `Context` trait; this helper is
// not used by the reference's own tests. It is preserved here as documentation
// of the reference's auto-context facility; callers in rustty implement
// `Context` directly. No logic is lost (the only behavior was "use the std
// auto-hash context").
#[allow(dead_code)]
fn auto_context() {}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn CacheTable() {
        // Construct a table that purposely has a predictable hash so we can
        // test all edge cases.
        struct Ctx;
        impl Context<u32, u32> for Ctx {
            fn hash(&self, key: u32) -> u64 {
                key as u64
            }
            fn eql(&self, a: u32, b: u32) -> bool {
                a == b
            }
        }
        let mut t: CacheTable<u32, u32, Ctx, 2, 2> = CacheTable::new(Ctx);

        // Fill the table
        assert!(t.put(0, 0).is_none());
        assert!(t.put(1, 0).is_none());
        assert!(t.put(2, 0).is_none());
        assert!(t.put(3, 0).is_none());

        // It should now be full, so any insert should evict the oldest item.
        // NOTE: For the sake of this test, we're assuming that the first item
        // is evicted but we don't need to promise this.
        assert_eq!(KV { key: 0, value: 0 }, t.put(4, 0).unwrap());

        // The first item should now be gone
        assert!(t.get(0).is_none());
    }
}
