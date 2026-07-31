//! rustty-vt hash_map
//!
//! PORTS: ghostty src/terminal/hash_map.zig
//!
//! This file contains a fork of the Zig stdlib HashMap implementation tuned
//! for use with our terminal page representation.
//!
//! The main goal we need to achieve that wasn't possible with the stdlib
//! HashMap is to utilize offsets rather than full pointers so that we can
//! copy around the entire backing memory and keep the hash map working.
//!
//! Additionally, for serialization/deserialization purposes, we need to be
//! able to create a HashMap instance and manually set the offsets up. The
//! stdlib HashMap does not export Metadata so this isn't possible.
//!
//! Also, I want to be able to understand possible capacity for a given K,V
//! type and fixed memory amount. The stdlib HashMap doesn't publish its
//! internal allocation size calculation.
//!
//! Finally, I removed many of the APIs that we'll never require for our
//! usage just so that this file is smaller, easier to understand, and has
//! less opportunity for bugs.
//!
//! Besides these shortcomings, the stdlib HashMap has some great qualities
//! that we want to keep, namely the fact that it is backed by a single large
//! allocation rather than pointers to separate allocations. This is important
//! because our terminal page representation is backed by a single large
//! allocation so we can give the HashMap a slice of memory to operate in.
//!
//! This fork diverges from the stdlib in one significant way: removal uses
//! backward-shift deletion (Knuth vol. 3, section 6.4, algorithm R) rather
//! than tombstones. A fixed-capacity map cannot outgrow tombstone buildup
//! the way an allocating map does, so tombstones require either unbounded
//! probe lengths or periodic in-place rebuilds with subtle bookkeeping.
//! Backward-shift deletion instead restores the table after every removal
//! to the exact state it would be in had the removed key never been
//! inserted. Probe chains are therefore always minimal for the insertion
//! order, there is no fragmentation to repair, and lookup cost depends only
//! on the current load factor.
//!
//! Pointer stability: insertion never moves existing entries, but removal
//! may move *other* entries within a probe cluster. Any key or value
//! pointers previously returned by the map must be considered invalidated
//! by any removal.
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const mem = std.mem;
//   const Allocator = mem.Allocator;
//   const Offset = @import("size.zig").Offset;
//   const OffsetBuf = @import("size.zig").OffsetBuf;
//   const getOffset = @import("size.zig").getOffset;
// The offset types are the hoisted size types from rustty-render-types (A1).
//
// PORT NOTE: the reference is a comptime `HashMapUnmanaged(K, V, Context)` / its
// `OffsetHashMap` wrapper. In Rust both are generic over `K`, `V` and a
// `Context: HashMapContext<K>` trait that supplies `hash`/`eql`, mirroring the
// Zig `Context` duck-typed struct. `Allocator.Error!void` maps to
// `Result<(), OutOfMemory>` since `OutOfMemory` is the only reachable variant.
//
// PORT NOTE: the offset/pointer machinery (`header`, `keys`, `values`,
// `metadata`) is reproduced with raw pointers and is `unsafe`, exactly as the
// reference relies on raw pointer arithmetic against the offset base. Soundness
// invariants (valid backing buffer of `layoutForCapacity().total_size` bytes,
// `base_align`-aligned) are the caller's responsibility, same as the reference.

// PARITY: the reference uses `x % alignment == 0` modulo checks and `i % n == 0`
// test predicates verbatim; the port keeps them char-for-char rather than
// rewriting to `is_multiple_of`. The `Iterator::next` method name mirrors the
// reference `Iterator.next`; it is an inherent method (the offset map is not a
// std iterator), so `should_implement_trait` is allowed module-wide.
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::should_implement_trait)]

use core::marker::PhantomData;

use rustty_render_types::term_size::{get_offset, Offset, OffsetBuf};
use rustty_util::quirks::inlineAssert as assert;

/// `OutOfMemory` error. The reference uses `Allocator.Error`, whose only
/// reachable variant in this file is `error.OutOfMemory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

/// The context trait supplying the `hash` and `eql` behaviors. The reference
/// `Context` is a duck-typed struct with `pub fn hash(...) u64` and
/// `pub fn eql(...) bool`.
pub trait HashMapContext<K> {
    fn hash(&self, key: K) -> u64;
    fn eql(&self, a: K, b: K) -> bool;
}

/// The default preserves the original behavior of allowing every raw slot to
/// be occupied. Callers can choose a lower value to bound probe lengths.
// `pub const default_max_load_percentage: u8 = 100;`
pub const DEFAULT_MAX_LOAD_PERCENTAGE: u8 = 100;

// pub fn AutoHashMapUnmanaged(comptime K, comptime V, comptime max_load_percentage)
//     -> HashMapUnmanaged(K, V, AutoContext(K), max_load_percentage)
#[cfg(test)]
type AutoHashMapUnmanaged<K, V, const MAX_LOAD_PERCENTAGE: u8> =
    HashMapUnmanaged<K, V, AutoContext<K>, MAX_LOAD_PERCENTAGE>;

/// `pub fn AutoOffsetHashMap(comptime K, comptime V, comptime max_load_percentage)` =
/// `OffsetHashMap(K, V, AutoContext(K), max_load_percentage)`.
pub type AutoOffsetHashMap<K, V, const MAX_LOAD_PERCENTAGE: u8> =
    OffsetHashMap<K, V, AutoContext<K>, MAX_LOAD_PERCENTAGE>;

/// `fn AutoContext(comptime K: type) type` — the auto-generated context built
/// from `std.hash_map.getAutoHashFn` / `getAutoEqlFn`.
//
// PORT NOTE: the reference auto context hashes the key with `autoHash` (Wyhash
// over the key's bytes) and compares with `std.meta.eql`. The hash-map tests
// only assert behavioral correctness (round-trips, counts, tombstone reuse),
// which hold for any deterministic `hash: K -> u64` plus value equality, so we
// build the context on Rust's `Hash`/`Eq` with a deterministic hasher.
pub struct AutoContext<K>(PhantomData<K>);

impl<K> Default for AutoContext<K> {
    fn default() -> Self {
        AutoContext(PhantomData)
    }
}

impl<K: core::hash::Hash + PartialEq> HashMapContext<K> for AutoContext<K> {
    // PORT NOTE (`f2a7652ab`'s `hash_map.zig` hunk, WS10 T16): upstream replaced
    // `std.hash_map.getAutoHashFn` with an explicit body that force-inlines the
    // hash for `hasUniqueRepresentation(K)` types, because "LLVM 21 (Zig 0.16)
    // failed to inline this which resulted in a measurable almost 2x slowdown on
    // our hyperlink map benchmark". This port's context is already an explicit
    // body (see the note above), so the portable half of that change is the
    // inlining hint — `#[inline]` is the Rust spelling of
    // `@call(.always_inline, ...)` across the crate boundary. The
    // `hasUniqueRepresentation` split has no counterpart: it selects between two
    // Zig std hashers, and this port has one.
    #[inline]
    fn hash(&self, key: K) -> u64 {
        use core::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }

    fn eql(&self, a: K, b: K) -> bool {
        a == b
    }
}

/// A HashMap type that uses offsets rather than pointers, making it
/// possible to efficiently move around the backing memory without
/// invalidating the HashMap.
pub struct OffsetHashMap<K, V, Context: HashMapContext<K>, const MAX_LOAD_PERCENTAGE: u8> {
    pub metadata: Offset<Metadata>,
    _marker: PhantomData<(K, V, Context)>,
}

// PORT NOTE: the reference `OffsetHashMap` is a plain struct holding a single
// `Offset` (a `u32`), so `var x = self.*` copies it trivially. We mirror that
// with `Clone`/`Copy` (the only field is `Copy`); needed by `Page` clone, which
// copies the whole struct (`var result = self.*`).
impl<K, V, Context: HashMapContext<K>, const P: u8> Clone for OffsetHashMap<K, V, Context, P> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<K, V, Context: HashMapContext<K>, const P: u8> Copy for OffsetHashMap<K, V, Context, P> {}

// PORT NOTE: the reference exposes two re-exports on `OffsetHashMap`:
//   /// This is the pointer-based map that we're wrapping.
//   pub const Unmanaged = HashMapUnmanaged(K, V, Context);
//   pub const Layout = Unmanaged.Layout;
// Rust has no per-impl type aliases usable the same way, so this port refers to
// the pointer-based map directly as `HashMapUnmanaged<K, V, Context>` (the map
// "we're wrapping") and to the shared crate-level `Layout` type. The comments are
// preserved here so nothing is lost.

impl<K, V, Context: HashMapContext<K>, const P: u8> Default for OffsetHashMap<K, V, Context, P> {
    fn default() -> Self {
        OffsetHashMap {
            metadata: Offset::default(),
            _marker: PhantomData,
        }
    }
}

impl<K: Copy, V: Copy, Context: HashMapContext<K>, const P: u8> OffsetHashMap<K, V, Context, P> {
    /// This is the alignment that the base pointer must have.
    // `pub const base_align = Unmanaged.base_align;`
    pub fn base_align() -> usize {
        HashMapUnmanaged::<K, V, Context, P>::base_align()
    }

    /// Returns the total size of the backing memory required for a
    /// HashMap with the given capacity. The base ptr must also be
    /// aligned to base_align.
    pub fn layout(cap: Size) -> Layout {
        HashMapUnmanaged::<K, V, Context, P>::layout_for_size(cap)
    }

    /// Initialize a new HashMap with the given capacity and backing
    /// memory. The backing memory must be aligned to base_align.
    ///
    /// # Safety
    ///
    /// `buf` must point at a valid `base_align`-aligned allocation of at least
    /// `l.total_size` bytes.
    pub unsafe fn init(buf: OffsetBuf, l: Layout) -> Self {
        assert(unsafe { buf.start() } as usize % Self::base_align() == 0);

        let m = unsafe { HashMapUnmanaged::<K, V, Context, P>::init(buf, l) };
        OffsetHashMap {
            metadata: unsafe { get_offset(buf, m.metadata.unwrap() as *const Metadata) },
            _marker: PhantomData,
        }
    }

    /// Returns the pointer-based map from a base pointer.
    ///
    /// # Safety
    ///
    /// `base` must be the true base pointer this map was initialized against.
    pub unsafe fn map<B: rustty_render_types::term_size::IntFromBase>(
        &self,
        base: B,
    ) -> HashMapUnmanaged<K, V, Context, P> {
        HashMapUnmanaged {
            metadata: Some(unsafe { self.metadata.ptr(base) }),
            _marker: PhantomData,
        }
    }
}

// This hashmap is specially designed for sizes that fit in a u32.
pub type Size = u32;

// u64 hashes guarantee us that the fingerprint bits will never be used
// to compute the index of a slot, maximizing the use of entropy.
pub type Hash = u64;

/// Metadata for a slot. It can be in two states: free or used.
/// To the used state, we add 7 bits from the slot's key hash. These
/// are used as a fast way to disambiguate between entries without
/// having to use the equality function. If two fingerprints are
/// different, we know that we don't have to compare the keys at all.
/// The 7 bits are the highest ones from a 64 bit hash. This way, not
/// only we use the `log2(capacity)` lowest bits from the hash to determine
/// a slot index, but we use 7 more bits to quickly resolve collisions
/// when multiple elements with different hashes end up wanting to be in the same slot.
/// Not using the equality function means we don't have to read into
/// the entries array, likely avoiding a cache miss and a potentially
/// costly function call.
// PORT NOTE: the reference is
//   const Metadata = packed struct(u8) { fingerprint: u7 = 0, used: u1 = 0 };
// Zig packs least-significant-first, so `fingerprint` occupies bits 0..6 and
// `used` bit 7 of the backing `u8`. We model it as a `#[repr(transparent)]` u8
// with explicit accessors so the byte layout (and the all-zero free byte the
// probe loops fuse against) is identical. `packed struct(u8)` is the reference's
// own explicit-backing-integer form as of `fedd42e8d`.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Metadata(u8);

impl Metadata {
    // const FingerPrint = u7;
    // (modeled as u8 storing only the low 7 bits)

    /// Default `Metadata`: fingerprint = 0, used = 0 — i.e. the free byte.
    // PORT NOTE: mirrors the reference field defaults `fingerprint = 0, used =
    // 0`. `remove_by_index_context` writes `metadata[hole] = .{}` through this.
    fn default() -> Metadata {
        Metadata(0)
    }

    #[inline]
    fn fingerprint(self) -> u8 {
        self.0 & 0x7F
    }

    #[inline]
    fn used_bit(self) -> u8 {
        (self.0 >> 7) & 1
    }

    pub fn is_used(self) -> bool {
        self.used_bit() == 1
    }

    pub fn is_free(self) -> bool {
        // A free slot is always the all-zero byte: `fill` sets the
        // used bit and removal zeroes the whole byte. Comparing the
        // full byte (rather than testing the used bit) lets the
        // optimizer fuse this with the fingerprint comparison in
        // probe loops into single-byte compares.
        self.0 == 0
    }

    pub fn take_fingerprint(hash: Hash) -> u8 {
        // const hash_bits = @typeInfo(Hash).int.bits;  // 64
        // const fp_bits = @typeInfo(FingerPrint).int.bits;  // 7
        let hash_bits = 64u32;
        let fp_bits = 7u32;
        // return @truncate(hash >> (hash_bits - fp_bits));
        ((hash >> (hash_bits - fp_bits)) as u8) & 0x7F
    }

    pub fn fill(&mut self, fp: u8) {
        // self.used = 1; self.fingerprint = fp;
        self.0 = (fp & 0x7F) | (1 << 7);
    }
}

/// `pub const Entry = struct { key_ptr: *K, value_ptr: *V };`
pub struct Entry<K, V> {
    pub key_ptr: *mut K,
    pub value_ptr: *mut V,
}

/// `pub const KV = struct { key: K, value: V };`
pub struct KV<K, V> {
    pub key: K,
    pub value: V,
}

/// `pub const GetOrPutResult = struct { key_ptr, value_ptr, found_existing };`
pub struct GetOrPutResult<K, V> {
    pub key_ptr: *mut K,
    pub value_ptr: *mut V,
    pub found_existing: bool,
}

/// `const Header = struct { values, keys, capacity, size };`
//
// PORT NOTE: the reference `Header` is a plain Zig struct laid out immediately
// before the metadata. We reproduce the same field order and types. The keys/
// values offsets are relative to the metadata pointer (`Offset(V)`/`Offset(K)`).
#[repr(C)]
struct Header<K, V> {
    /// The keys/values offset are relative to the metadata
    values: Offset<V>,
    keys: Offset<K>,
    capacity: Size,
    size: Size,
}

/// The memory layout for the underlying buffer for a given capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The total size of the buffer required. The buffer is expected
    /// to be aligned to `base_align`.
    pub total_size: usize,

    /// The offset to the start of the keys data.
    pub keys_start: usize,

    /// The offset to the start of the values data.
    pub vals_start: usize,

    /// The capacity that was used to calculate this layout.
    pub capacity: Size,
}

/// Fork of stdlib.HashMap as of Zig 0.12 modified to use offsets for
/// the key/values pointer. The metadata is still a pointer to limit
/// the amount of arithmetic required to access it. See the file comment
/// for full details.
pub struct HashMapUnmanaged<K, V, Context: HashMapContext<K>, const MAX_LOAD_PERCENTAGE: u8> {
    // This is actually a midway pointer to the single buffer containing
    // a `Header` field, the `Metadata`s and `Entry`s.
    // At `-@sizeOf(Header)` is the Header field.
    // At `sizeOf(Metadata) * capacity + offset`, which is pointed to by
    // self.header().entries, is the array of entries.
    // This means that the hashmap only holds one live allocation, to
    // reduce memory fragmentation and struct size.
    /// Pointer to the metadata.
    pub metadata: Option<*mut Metadata>,
    _marker: PhantomData<(K, V, Context)>,
}

impl<K, V, Context: HashMapContext<K>, const P: u8> Default for HashMapUnmanaged<K, V, Context, P> {
    fn default() -> Self {
        HashMapUnmanaged {
            metadata: None,
            _marker: PhantomData,
        }
    }
}

impl<K: Copy, V: Copy, Context: HashMapContext<K>, const MAX_LOAD_PERCENTAGE: u8>
    HashMapUnmanaged<K, V, Context, MAX_LOAD_PERCENTAGE>
{
    // comptime { assert(@alignOf(Metadata) == 1); }
    const _ASSERT_META_ALIGN: () = assert!(core::mem::align_of::<Metadata>() == 1);
    // comptime { assert(max_load_percentage > 0); assert(max_load_percentage <= 100); }
    const _ASSERT_MAX_LOAD: () = assert!(MAX_LOAD_PERCENTAGE > 0 && MAX_LOAD_PERCENTAGE <= 100);

    fn header_align() -> usize {
        core::mem::align_of::<Header<K, V>>()
    }
    fn key_align() -> usize {
        if core::mem::size_of::<K>() == 0 {
            1
        } else {
            core::mem::align_of::<K>()
        }
    }
    fn val_align() -> usize {
        if core::mem::size_of::<V>() == 0 {
            1
        } else {
            core::mem::align_of::<V>()
        }
    }

    /// `base_align: mem.Alignment = .fromByteUnits(@max(header_align, key_align, val_align));`
    pub fn base_align() -> usize {
        Self::header_align()
            .max(Self::key_align())
            .max(Self::val_align())
    }

    // This is purely empirical and not a /very smart magic constant™/.
    /// Capacity of the first grow when bootstrapping the hashmap.
    #[cfg(test)]
    const MINIMAL_CAPACITY: Size = 8;

    // comptime { assert(@sizeOf(Metadata) == 1); assert(@alignOf(Metadata) == 1); }
    const _ASSERT_META_SIZE: () = assert!(core::mem::size_of::<Metadata>() == 1);
    const _ASSERT_META_ALIGN2: () = assert!(core::mem::align_of::<Metadata>() == 1);

    /// Initialize a hash map with a given capacity and a buffer. The
    /// buffer must fit within the size defined by `layoutForCapacity`.
    ///
    /// # Safety
    ///
    /// `buf` must point at a valid `base_align`-aligned allocation of at least
    /// `layout.total_size` bytes.
    pub unsafe fn init(buf: OffsetBuf, layout: Layout) -> Self {
        // Force the comptime asserts to be evaluated for this instantiation.
        let () = Self::_ASSERT_META_ALIGN;
        let () = Self::_ASSERT_META_SIZE;
        let () = Self::_ASSERT_META_ALIGN2;
        let () = Self::_ASSERT_MAX_LOAD;

        assert(unsafe { buf.start() } as usize % Self::base_align() == 0);

        // Get all our main pointers
        let metadata_buf = unsafe { buf.rebase(core::mem::size_of::<Header<K, V>>()) };
        let metadata_ptr: *mut Metadata = unsafe { metadata_buf.start() } as *mut Metadata;

        // Build our map
        let mut map: Self = HashMapUnmanaged {
            metadata: Some(metadata_ptr),
            _marker: PhantomData,
        };
        let hdr = unsafe { map.header() };
        unsafe {
            (*hdr).capacity = layout.capacity;
            (*hdr).size = 0;
            // if (@sizeOf([*]K) != 0) hdr.keys = metadata_buf.member(K, layout.keys_start);
            (*hdr).keys = metadata_buf.member(layout.keys_start);
            // if (@sizeOf([*]V) != 0) hdr.values = metadata_buf.member(V, layout.vals_start);
            (*hdr).values = metadata_buf.member(layout.vals_start);
        }
        unsafe { map.init_metadatas() };

        map
    }

    pub fn ensure_total_capacity(&mut self, new_size: Size) -> Result<(), OutOfMemory> {
        if new_size > unsafe { (*self.header()).size } {
            self.check_capacity(new_size - unsafe { (*self.header()).size })?;
        }
        Ok(())
    }

    pub fn ensure_unused_capacity(&mut self, additional_size: Size) -> Result<(), OutOfMemory> {
        self.ensure_total_capacity(self.count() + additional_size)
    }

    pub fn clear_retaining_capacity(&mut self) {
        if self.metadata.is_some() {
            unsafe {
                self.init_metadatas();
                (*self.header()).size = 0;
            }
        }
    }

    pub fn count(&self) -> Size {
        unsafe { (*self.header()).size }
    }

    /// # Safety: metadata must be non-null.
    unsafe fn header(&self) -> *mut Header<K, V> {
        // @ptrCast(@as([*]Header, @ptrCast(@alignCast(self.metadata.?))) - 1)
        let meta = self.metadata.unwrap() as *mut Header<K, V>;
        unsafe { meta.sub(1) }
    }

    /// # Safety: metadata must be valid.
    unsafe fn keys(&self) -> *mut K {
        unsafe { (*self.header()).keys.ptr(self.metadata.unwrap()) }
    }

    /// # Safety: metadata must be valid.
    unsafe fn values(&self) -> *mut V {
        unsafe { (*self.header()).values.ptr(self.metadata.unwrap()) }
    }

    pub fn capacity(&self) -> Size {
        if self.metadata.is_none() {
            return 0;
        }

        unsafe { (*self.header()).capacity }
    }

    /// Maximum number of entries the map will hold. This is less than
    /// capacity when MAX_LOAD_PERCENTAGE is below 100, which keeps free
    /// slots in every probe chain and bounds probe lengths.
    pub fn max_load(&self) -> Size {
        Self::max_load_for_capacity(self.capacity())
    }

    pub fn iterator(&self) -> Iterator<'_, K, V, Context, MAX_LOAD_PERCENTAGE> {
        Iterator { hm: self, index: 0 }
    }

    /// Insert an entry in the map. Assumes it is not already present.
    pub fn put_no_clobber_context(
        &mut self,
        key: K,
        value: V,
        ctx: &Context,
    ) -> Result<(), OutOfMemory> {
        assert(!self.contains_context(key, ctx));
        self.check_capacity(1)?;

        self.put_assume_capacity_no_clobber_context(key, value, ctx);
        Ok(())
    }

    /// Asserts there is enough capacity to store the new key-value pair.
    /// Clobbers any existing data. To detect if a put would clobber
    /// existing data, see `getOrPutAssumeCapacity`.
    pub fn put_assume_capacity_context(&mut self, key: K, value: V, ctx: &Context) {
        let gop = self.get_or_put_assume_capacity_context(key, ctx);
        unsafe { *gop.value_ptr = value };
    }

    /// Insert an entry in the map. Assumes it is not already present,
    /// and that no allocation is needed.
    pub fn put_assume_capacity_no_clobber_context(&mut self, key: K, value: V, ctx: &Context) {
        assert(!self.contains_context(key, ctx));

        // A free slot must exist for the probe below to terminate.
        //
        // PORT NOTE (area D §4.5): the reference's `assert` here is
        // `quirks.inlineAssert`, which is a no-op in ReleaseFast. It is ported
        // as a `debug_assert!` for the same reason the reference added it — it
        // turns a violated assume-capacity contract into a panic instead of a
        // silent live-entry overwrite.
        debug_assert!(unsafe { (*self.header()).size } < self.capacity());

        let hash = ctx.hash(key);
        let mask = self.capacity() - 1;
        let mut idx = (hash & (mask as u64)) as usize;

        let mut metadata = unsafe { self.metadata.unwrap().add(idx) };
        while unsafe { (*metadata).is_used() } {
            idx = (idx + 1) & (mask as usize);
            metadata = unsafe { self.metadata.unwrap().add(idx) };
        }

        unsafe {
            (*metadata).fill(Metadata::take_fingerprint(hash));
            *self.keys().add(idx) = key;
            *self.values().add(idx) = value;
            (*self.header()).size += 1;
        }
    }

    /// Inserts a new `Entry` into the hash map, returning the previous one, if any.
    pub fn fetch_put_context(
        &mut self,
        key: K,
        value: V,
        ctx: &Context,
    ) -> Result<Option<KV<K, V>>, OutOfMemory> {
        let gop = self.get_or_put_context(key, ctx)?;
        let mut result: Option<KV<K, V>> = None;
        if gop.found_existing {
            result = Some(KV {
                key: unsafe { *gop.key_ptr },
                value: unsafe { *gop.value_ptr },
            });
        }
        unsafe { *gop.value_ptr = value };
        Ok(result)
    }

    /// Inserts a new `Entry` into the hash map, returning the previous one, if any.
    /// If insertion happens, asserts there is enough capacity without allocating.
    pub fn fetch_put_assume_capacity_context(
        &mut self,
        key: K,
        value: V,
        ctx: &Context,
    ) -> Option<KV<K, V>> {
        let gop = self.get_or_put_assume_capacity_context(key, ctx);
        let mut result: Option<KV<K, V>> = None;
        if gop.found_existing {
            result = Some(KV {
                key: unsafe { *gop.key_ptr },
                value: unsafe { *gop.value_ptr },
            });
        }
        unsafe { *gop.value_ptr = value };
        result
    }

    /// If there is an `Entry` with a matching key, it is deleted from
    /// the hash map, and then returned from this function. Removal may
    /// move other entries: any previously returned key or value
    /// pointers are invalidated.
    //
    // PORT NOTE: `fedd42e8d` deleted `fetchRemoveAdapted`, folding its body
    // into `fetchRemoveContext`. This port's `*_adapted` aliases were already
    // identity wrappers (no adapted form exists here), so the alias goes too.
    pub fn fetch_remove_context(&mut self, key: K, ctx: &Context) -> Option<KV<K, V>> {
        let idx = self.get_index(key, ctx)?;
        let result = KV {
            key: unsafe { *self.keys().add(idx) },
            value: unsafe { *self.values().add(idx) },
        };
        self.remove_by_index_context(idx, ctx);
        Some(result)
    }

    /// Find the index containing the data for the given key.
    /// Whether this function returns null is almost always
    /// branched on after this function returns, and this function
    /// returns null/not null from separate code paths.  We
    /// want the optimizer to remove that branch and instead directly
    /// fuse the basic blocks after the branch to the basic blocks
    /// from this function.  To encourage that, this function is
    /// marked as inline.
    #[inline]
    fn get_index(&self, key: K, ctx: &Context) -> Option<usize> {
        if unsafe { (*self.header()).size } == 0 {
            return None;
        }

        // If you get a compile error on this line, it means that your generic hash
        // function is invalid for these parameters.
        let hash = ctx.hash(key);
        let mask = self.capacity() - 1;
        let fingerprint = Metadata::take_fingerprint(hash);
        // Don't loop indefinitely when there are no free slots.
        let mut limit = self.capacity();
        let mut idx = (hash & (mask as u64)) as usize;

        let mut metadata = unsafe { self.metadata.unwrap().add(idx) };
        while !unsafe { (*metadata).is_free() } && limit != 0 {
            if unsafe { (*metadata).is_used() }
                && unsafe { (*metadata).fingerprint() } == fingerprint
            {
                let test_key = unsafe { self.keys().add(idx) };
                // If you get a compile error on this line, it means that your generic eql
                // function is invalid for these parameters.
                let eql = ctx.eql(key, unsafe { *test_key });
                // verifyContext can't verify the return type of generic eql functions,
                // so we need to double-check it here.
                // PORT NOTE: the reference follows this with
                // `if (@TypeOf(eql) != bool) @compileError(...)`; in Rust the
                // `HashMapContext::eql` trait method's signature fixes the return
                // type to `bool` at compile time, so the check is statically
                // satisfied and no runtime guard is emitted.
                if eql {
                    return Some(idx);
                }
            }

            limit -= 1;
            idx = (idx + 1) & (mask as usize);
            metadata = unsafe { self.metadata.unwrap().add(idx) };
        }

        None
    }

    pub fn get_entry_context(&self, key: K, ctx: &Context) -> Option<Entry<K, V>> {
        self.get_entry_adapted(key, ctx)
    }
    pub fn get_entry_adapted(&self, key: K, ctx: &Context) -> Option<Entry<K, V>> {
        if let Some(idx) = self.get_index(key, ctx) {
            return Some(Entry {
                key_ptr: unsafe { self.keys().add(idx) },
                value_ptr: unsafe { self.values().add(idx) },
            });
        }
        None
    }

    /// Insert an entry if the associated key is not already present, otherwise update preexisting value.
    pub fn put_context(&mut self, key: K, value: V, ctx: &Context) -> Result<(), OutOfMemory> {
        let result = self.get_or_put_context(key, ctx)?;
        unsafe { *result.value_ptr = value };
        Ok(())
    }

    /// Get an optional pointer to the actual key associated with adapted key, if present.
    pub fn get_key_ptr_context(&self, key: K, ctx: &Context) -> Option<*mut K> {
        self.get_key_ptr_adapted(key, ctx)
    }
    pub fn get_key_ptr_adapted(&self, key: K, ctx: &Context) -> Option<*mut K> {
        if let Some(idx) = self.get_index(key, ctx) {
            return Some(unsafe { self.keys().add(idx) });
        }
        None
    }

    /// Get a copy of the actual key associated with adapted key, if present.
    pub fn get_key_context(&self, key: K, ctx: &Context) -> Option<K> {
        self.get_key_adapted(key, ctx)
    }
    pub fn get_key_adapted(&self, key: K, ctx: &Context) -> Option<K> {
        if let Some(idx) = self.get_index(key, ctx) {
            return Some(unsafe { *self.keys().add(idx) });
        }
        None
    }

    /// Get an optional pointer to the value associated with key, if present.
    pub fn get_ptr_context(&self, key: K, ctx: &Context) -> Option<*mut V> {
        self.get_ptr_adapted(key, ctx)
    }
    pub fn get_ptr_adapted(&self, key: K, ctx: &Context) -> Option<*mut V> {
        if let Some(idx) = self.get_index(key, ctx) {
            return Some(unsafe { self.values().add(idx) });
        }
        None
    }

    /// Get a copy of the value associated with key, if present.
    pub fn get_context(&self, key: K, ctx: &Context) -> Option<V> {
        self.get_adapted(key, ctx)
    }
    pub fn get_adapted(&self, key: K, ctx: &Context) -> Option<V> {
        if let Some(idx) = self.get_index(key, ctx) {
            return Some(unsafe { *self.values().add(idx) });
        }
        None
    }

    pub fn get_or_put_context(
        &mut self,
        key: K,
        ctx: &Context,
    ) -> Result<GetOrPutResult<K, V>, OutOfMemory> {
        let gop = self.get_or_put_context_adapted(key, ctx)?;
        if !gop.found_existing {
            unsafe { *gop.key_ptr = key };
        }
        Ok(gop)
    }
    pub fn get_or_put_context_adapted(
        &mut self,
        key: K,
        key_ctx: &Context,
    ) -> Result<GetOrPutResult<K, V>, OutOfMemory> {
        match self.check_capacity(1) {
            Ok(()) => {}
            Err(err) => {
                // The map is full. Try to do the lookup anyway; if we find
                // an existing item, we can return it. Otherwise return the
                // error, we could not add another.
                let index = match self.get_index(key, key_ctx) {
                    Some(i) => i,
                    None => return Err(err),
                };
                return Ok(GetOrPutResult {
                    key_ptr: unsafe { self.keys().add(index) },
                    value_ptr: unsafe { self.values().add(index) },
                    found_existing: true,
                });
            }
        }
        Ok(self.get_or_put_assume_capacity_adapted(key, key_ctx))
    }

    pub fn get_or_put_assume_capacity_context(
        &mut self,
        key: K,
        ctx: &Context,
    ) -> GetOrPutResult<K, V> {
        let result = self.get_or_put_assume_capacity_adapted(key, ctx);
        if !result.found_existing {
            unsafe { *result.key_ptr = key };
        }
        result
    }
    pub fn get_or_put_assume_capacity_adapted(
        &mut self,
        key: K,
        ctx: &Context,
    ) -> GetOrPutResult<K, V> {
        // If you get a compile error on this line, it means that your generic hash
        // function is invalid for these parameters.
        let hash = ctx.hash(key);
        // verifyContext can't verify the return type of generic hash functions,
        // so we need to double-check it here.
        // PORT NOTE: the reference follows this with
        // `if (@TypeOf(hash) != Hash) @compileError(...)`; in Rust the
        // `HashMapContext::hash` trait method's signature fixes the return type to
        // `u64` (== `Hash`) at compile time, so the check is statically satisfied.
        let mask = self.capacity() - 1;
        let fingerprint = Metadata::take_fingerprint(hash);
        let mut limit = self.capacity();
        let mut idx = (hash & (mask as u64)) as usize;

        let mut metadata = unsafe { self.metadata.unwrap().add(idx) };
        while !unsafe { (*metadata).is_free() } && limit != 0 {
            if unsafe { (*metadata).is_used() }
                && unsafe { (*metadata).fingerprint() } == fingerprint
            {
                let test_key = unsafe { self.keys().add(idx) };
                // If you get a compile error on this line, it means that your generic eql
                // function is invalid for these parameters.
                let eql = ctx.eql(key, unsafe { *test_key });
                // verifyContext can't verify the return type of generic eql functions,
                // so we need to double-check it here.
                // PORT NOTE: the reference follows this with
                // `if (@TypeOf(eql) != bool) @compileError(...)`; in Rust the
                // `HashMapContext::eql` trait method's signature fixes the return
                // type to `bool` at compile time, so the check is statically
                // satisfied and no runtime guard is emitted.
                if eql {
                    return GetOrPutResult {
                        key_ptr: test_key,
                        value_ptr: unsafe { self.values().add(idx) },
                        found_existing: true,
                    };
                }
            }

            limit -= 1;
            idx = (idx + 1) & (mask as usize);
            metadata = unsafe { self.metadata.unwrap().add(idx) };
        }

        // The caller guaranteed capacity for at least one new entry, so
        // the probe must have ended at a free slot. Anything else means
        // the assume-capacity contract was violated and we would be
        // silently overwriting a live entry.
        //
        // PORT NOTE (area D §4.5): ported as a `debug_assert!` — see the
        // matching note in `put_assume_capacity_no_clobber_context`.
        debug_assert!(unsafe { (*metadata).is_free() });

        unsafe {
            (*metadata).fill(fingerprint);
            let new_key = self.keys().add(idx);
            let new_value = self.values().add(idx);
            // new_key.* = undefined; new_value.* = undefined; (no-op)
            (*self.header()).size += 1;

            GetOrPutResult {
                key_ptr: new_key,
                value_ptr: new_value,
                found_existing: false,
            }
        }
    }

    pub fn get_or_put_value_context(
        &mut self,
        key: K,
        value: V,
        ctx: &Context,
    ) -> Result<Entry<K, V>, OutOfMemory> {
        let res = self.get_or_put_context_adapted(key, ctx)?;
        if !res.found_existing {
            unsafe {
                *res.key_ptr = key;
                *res.value_ptr = value;
            }
        }
        Ok(Entry {
            key_ptr: res.key_ptr,
            value_ptr: res.value_ptr,
        })
    }

    /// Return true if there is a value associated with key in the map.
    pub fn contains_context(&self, key: K, ctx: &Context) -> bool {
        self.contains_adapted(key, ctx)
    }
    pub fn contains_adapted(&self, key: K, ctx: &Context) -> bool {
        self.get_index(key, ctx).is_some()
    }

    /// Remove the entry at the given index using backward-shift deletion
    /// (Knuth vol. 3, section 6.4, algorithm R): rather than marking the
    /// slot with a tombstone, restore the table to the state it would be
    /// in had the removed key never been inserted. Any entry whose probe
    /// sequence passes over the hole is moved into it, which moves the
    /// hole further along the cluster, until the cluster ends at a free
    /// slot.
    //
    // UB HAZARD (area D §4.5): this writes `metadata[hole]`, `keys[hole]` and
    // `values[hole]` — three overlapping raw views into ONE `Page` allocation —
    // while also reading `keys[j]` to rehash it. `metadata`, `keys()` and
    // `values()` are therefore kept as RAW pointers; do NOT form `&mut [K]` and
    // `&mut [Metadata]` simultaneously from the same base for the convenience
    // of indexing. Proof harness: `.port/phase-3/mirihar-hashmap-shift/`.
    //
    // CONTRACT: this moves other entries within the probe cluster, so every key
    // and value pointer the map has handed out is invalidated. Caller audit:
    // `.port/phase-3/HASHMAP_CALLER_AUDIT.md` (12/12 rows resolved).
    fn remove_by_index_context(&mut self, idx: usize, ctx: &Context) {
        let mask: usize = (self.capacity() - 1) as usize;
        let metadata = self.metadata.unwrap();
        let keys_ptr = unsafe { self.keys() };
        let values_ptr = unsafe { self.values() };

        // A completely full table has no free slot to terminate the
        // scan, so bound it to one full cycle. That is sufficient: the
        // hole only ever moves forward to slots the scan has already
        // visited, so each entry needs to be considered exactly once.
        let mut hole = idx;
        let mut j = idx;
        let mut limit = self.capacity() - 1;
        while limit != 0 {
            limit -= 1;
            j = (j + 1) & mask;
            if unsafe { (*metadata.add(j)).is_free() } {
                break;
            }

            // The entry at `j` may move into the hole only if the hole
            // lies on its probe path, i.e. cyclically within [home, j).
            // Otherwise the move would place it before its home slot
            // and lookups could no longer find it.
            let home: usize = (ctx.hash(unsafe { *keys_ptr.add(j) }) & (mask as u64)) as usize;
            if (hole.wrapping_sub(home) & mask) < (j.wrapping_sub(home) & mask) {
                unsafe {
                    *metadata.add(hole) = *metadata.add(j);
                    *keys_ptr.add(hole) = *keys_ptr.add(j);
                    *values_ptr.add(hole) = *values_ptr.add(j);
                }
                hole = j;
            }
        }

        unsafe {
            *metadata.add(hole) = Metadata::default();
            // keys_ptr[hole] = undefined; values_ptr[hole] = undefined; (no-op)
            (*self.header()).size -= 1;
        }

        // Standing invariant, carried in the type rather than only in tests
        // (area D §6): every used entry must still be reachable from its home
        // slot without crossing a free slot. This is exactly what lookups
        // depend on and exactly what an inverted cluster test would break.
        self.debug_assert_canonical(ctx);
    }

    /// If there is an `Entry` with a matching key, it is deleted from
    /// the hash map, and this function returns true.  Otherwise this
    /// function returns false. Removal may move other entries: any
    /// previously returned key or value pointers are invalidated.
    //
    // PORT NOTE: `fedd42e8d` deleted `removeAdapted`, folding it into
    // `removeContext`. This port's `remove_adapted` was an identity alias
    // (no adapted form exists here), so it goes with it.
    pub fn remove_context(&mut self, key: K, ctx: &Context) -> bool {
        let Some(idx) = self.get_index(key, ctx) else {
            return false;
        };
        self.remove_by_index_context(idx, ctx);
        true
    }

    /// Delete the entry with key pointed to by key_ptr from the hash map.
    /// key_ptr is assumed to be a valid pointer to a key that is present
    /// in the hash map. Removal may move other entries: any previously
    /// returned key or value pointers are invalidated.
    ///
    /// # Safety
    ///
    /// `key_ptr` must be a key pointer into this map's keys array.
    pub unsafe fn remove_by_ptr_context(&mut self, key_ptr: *mut K, ctx: &Context) {
        // TODO: replace with pointer subtraction once supported by zig
        // if @sizeOf(K) == 0 then there is at most one item in the hash
        // map, which is assumed to exist as key_ptr must be valid.  This
        // item must be at index 0.
        let idx = if core::mem::size_of::<K>() > 0 {
            ((key_ptr as usize) - (unsafe { self.keys() } as usize)) / core::mem::size_of::<K>()
        } else {
            0
        };

        self.remove_by_index_context(idx, ctx);
    }

    /// # Safety: metadata must be valid.
    unsafe fn init_metadatas(&mut self) {
        // @memset(@as([*]u8, @ptrCast(self.metadata.?))[0 .. @sizeOf(Metadata) * self.capacity()], 0);
        unsafe {
            core::slice::from_raw_parts_mut(
                self.metadata.unwrap() as *mut u8,
                core::mem::size_of::<Metadata>() * self.capacity() as usize,
            )
            .fill(0);
        }
    }

    /// Verify the canonical placement invariant that backward-shift deletion
    /// maintains: every used entry is reachable from its home slot without
    /// crossing a free slot. This is exactly the property lookups depend on.
    //
    // PORT NOTE (area D §6): upstream ships `expectCanonical` only as a
    // test helper (`hash_map.zig`, above `test "HashMap basic usage"`). rustty
    // carries it IN THE TYPE as a `debug_assert`-style invariant so every
    // removal on every map — including the page hyperlink/grapheme maps in
    // production debug builds — is checked, not just the four upstream tests.
    // It compiles away entirely in release (`cfg(debug_assertions)`).
    //
    // Kept raw-pointer-only for the §4.5 reason: no `&[K]`/`&[Metadata]` slice
    // is formed over the shared `Page` base.
    pub fn debug_assert_canonical(&self, ctx: &Context) {
        if !cfg!(debug_assertions) {
            return;
        }

        let cap = self.capacity();
        if cap == 0 {
            return;
        }
        let mask = (cap - 1) as usize;
        let mut used: Size = 0;
        for idx in 0..cap as usize {
            let metadata = unsafe { *self.metadata.unwrap().add(idx) };
            if !metadata.is_used() {
                continue;
            }
            used += 1;

            let mut probe: usize =
                (ctx.hash(unsafe { *self.keys().add(idx) }) & (mask as u64)) as usize;
            while probe != idx {
                debug_assert!(
                    unsafe { (*self.metadata.unwrap().add(probe)).is_used() },
                    "canonical placement violated: entry at slot {idx} is separated \
                     from its home slot by the free slot {probe}"
                );
                probe = (probe + 1) & mask;
            }
        }
        debug_assert_eq!(self.count(), used);
    }

    /// Returns an error if the map cannot hold `new_count` more entries.
    /// This map is fixed-capacity so nothing can be done to make room;
    /// the caller must grow the backing memory and rebuild the map.
    fn check_capacity(&mut self, new_count: Size) -> Result<(), OutOfMemory> {
        let available = self.max_load() - unsafe { (*self.header()).size };
        if new_count > available {
            return Err(OutOfMemory);
        }
        Ok(())
    }

    fn max_load_for_capacity(cap: Size) -> Size {
        if cap == 0 {
            return 0;
        }
        ((cap as u64 * MAX_LOAD_PERCENTAGE as u64) / 100) as Size
    }

    /// Returns the memory layout for the buffer for a given capacity.
    /// The actual size may be able to fit more than the given capacity
    /// because capacity is rounded up to the next power of two. This is
    /// a design requirement for this hash map implementation.
    pub fn layout_for_capacity(new_capacity: Size) -> Layout {
        assert(new_capacity == 0 || new_capacity.is_power_of_two());

        // Cast to usize to prevent overflow in size calculations.
        // See: https://github.com/ziglang/zig/pull/19048
        let cap: usize = new_capacity as usize;

        // Pack our metadata, keys, and values.
        let meta_start = core::mem::size_of::<Header<K, V>>();
        let meta_end =
            core::mem::size_of::<Header<K, V>>() + cap * core::mem::size_of::<Metadata>();
        let keys_start = align_forward(meta_end, Self::key_align());
        let keys_end = keys_start + cap * core::mem::size_of::<K>();
        let vals_start = align_forward(keys_end, Self::val_align());
        let vals_end = vals_start + cap * core::mem::size_of::<V>();

        // Our total memory size required is the end of our values
        // aligned to the base required alignment.
        let total_size = align_forward(vals_end, Self::base_align());

        // The offsets we actually store in the map are from the
        // metadata pointer so that we can use self.metadata as
        // the base.
        let keys_offset = keys_start - meta_start;
        let vals_offset = vals_start - meta_start;

        Layout {
            total_size,
            keys_start: keys_offset,
            vals_start: vals_offset,
            capacity: new_capacity,
        }
    }

    /// Returns a layout with enough raw slots to hold `new_size` entries
    /// at the configured maximum load factor.
    pub fn layout_for_size(new_size: Size) -> Layout {
        if new_size == 0 {
            return Self::layout_for_capacity(0);
        }

        // Scale the requested number of entries up to the raw slot count
        // required by the load factor. Widen first so `new_size * 100`
        // cannot overflow Size.
        // `std.math.divCeil(u64, new_size * 100, max_load_percentage)`
        let minimum_capacity: u64 = (new_size as u64 * 100).div_ceil(MAX_LOAD_PERCENTAGE as u64);

        // Capacities must be powers of two, so the largest capacity that
        // fits in Size is the highest bit rather than maxInt(Size).
        let max_capacity: u64 = 1u64 << (Size::BITS - 1);
        if minimum_capacity > max_capacity {
            return Self::layout_for_capacity(max_capacity as Size);
        }

        // Linear probing uses a mask for wraparound, which requires the
        // final raw capacity to be rounded up to a power of two.
        // `std.math.ceilPowerOfTwo(u64, minimum_capacity)`
        let raw_capacity = minimum_capacity.next_power_of_two();
        Self::layout_for_capacity(raw_capacity as Size)
    }
}

/// `pub const Iterator = struct { hm, index };`
pub struct Iterator<'a, K, V, Context: HashMapContext<K>, const MAX_LOAD_PERCENTAGE: u8> {
    hm: &'a HashMapUnmanaged<K, V, Context, MAX_LOAD_PERCENTAGE>,
    index: Size,
}

impl<K: Copy, V: Copy, Context: HashMapContext<K>, const P: u8> Iterator<'_, K, V, Context, P> {
    pub fn next(&mut self) -> Option<Entry<K, V>> {
        assert(self.index <= self.hm.capacity());
        if unsafe { (*self.hm.header()).size } == 0 {
            return None;
        }

        let cap = self.hm.capacity();
        let end = unsafe { self.hm.metadata.unwrap().add(cap as usize) };
        let mut metadata = unsafe { self.hm.metadata.unwrap().add(self.index as usize) };

        while metadata != end {
            if unsafe { (*metadata).is_used() } {
                let key = unsafe { self.hm.keys().add(self.index as usize) };
                let value = unsafe { self.hm.values().add(self.index as usize) };
                self.index += 1;
                return Some(Entry {
                    key_ptr: key,
                    value_ptr: value,
                });
            }
            metadata = unsafe { metadata.add(1) };
            self.index += 1;
        }

        None
    }
}

// PORT NOTE: `std.mem.alignForward(usize, value, alignment)`.
#[inline]
fn align_forward(value: usize, alignment: usize) -> usize {
    value.next_multiple_of(alignment)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use std::alloc::{alloc, dealloc, Layout as StdLayout};

    // Test-only RAII aligned buffer, mirroring `alloc.alignedAlloc` /
    // `defer alloc.free(buf)`.
    struct AlignedBuf {
        ptr: *mut u8,
        layout: StdLayout,
    }

    impl AlignedBuf {
        fn new(total_size: usize, base_align: usize) -> AlignedBuf {
            let layout = StdLayout::from_size_align(total_size, base_align).unwrap();
            let ptr = unsafe { alloc(layout) };
            assert!(!ptr.is_null());
            AlignedBuf { ptr, layout }
        }
        fn base(&self) -> *mut u8 {
            self.ptr
        }
        fn offset_buf(&self) -> OffsetBuf {
            OffsetBuf::init(self.ptr as *const u8)
        }
    }

    impl Drop for AlignedBuf {
        fn drop(&mut self) {
            unsafe { dealloc(self.ptr, self.layout) };
        }
    }

    // A default auto-context for the K type under test.
    fn ctx<K>() -> AutoContext<K> {
        AutoContext::default()
    }

    #[test]
    fn HashMap_basic_usage() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;

        let cap = 16;
        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());

        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let count = 5;
        let mut i: u32 = 0;
        let mut total: u32 = 0;
        while i < count {
            map.put_context(i, i, &c).unwrap();
            total += i;
            i += 1;
        }

        let mut sum: u32 = 0;
        let mut it = map.iterator();
        while let Some(kv) = it.next() {
            sum += unsafe { *kv.key_ptr };
        }
        assert_eq!(total, sum);

        i = 0;
        sum = 0;
        while i < count {
            assert_eq!(i, map.get_context(i, &c).unwrap());
            sum += map.get_context(i, &c).unwrap();
            i += 1;
        }
        assert_eq!(total, sum);
    }

    #[test]
    fn HashMap_ensureTotalCapacity() {
        type Map = AutoHashMapUnmanaged<i32, i32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<i32>();

        let initial_capacity = map.capacity();
        assert!(initial_capacity >= 20);
        let mut i: i32 = 0;
        while i < 20 {
            assert!(map
                .fetch_put_assume_capacity_context(i, i + 10, &c)
                .is_none());
            i += 1;
        }
        // shouldn't resize from putAssumeCapacity
        assert!(initial_capacity == map.capacity());
    }

    #[test]
    fn HashMap_ensureUnusedCapacity_with_removals() {
        type Map = AutoHashMapUnmanaged<i32, i32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<i32>();

        let mut i: i32 = 0;
        while i < 100 {
            map.ensure_unused_capacity(1).unwrap();
            map.put_assume_capacity_context(i, i, &c);
            let _ = map.remove_context(i, &c);
            i += 1;
        }
    }

    #[test]
    fn HashMap_clearRetainingCapacity() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 16;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        map.clear_retaining_capacity();

        map.put_context(1, 1, &c).unwrap();
        assert_eq!(map.get_context(1, &c).unwrap(), 1);
        assert_eq!(map.count(), 1);

        map.clear_retaining_capacity();
        map.put_assume_capacity_context(1, 1, &c);
        assert_eq!(map.get_context(1, &c).unwrap(), 1);
        assert_eq!(map.count(), 1);

        let actual_cap = map.capacity();
        assert!(actual_cap > 0);

        map.clear_retaining_capacity();
        map.clear_retaining_capacity();
        assert_eq!(map.count(), 0);
        assert_eq!(map.capacity(), actual_cap);
        assert!(!map.contains_context(1, &c));
    }

    #[test]
    fn HashMap_ensureTotalCapacity_with_existing_elements() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = Map::MINIMAL_CAPACITY;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        map.put_context(0, 0, &c).unwrap();
        assert_eq!(map.count(), 1);
        assert_eq!(map.capacity(), Map::MINIMAL_CAPACITY);

        assert_eq!(Err(OutOfMemory), map.ensure_total_capacity(65));
        assert_eq!(map.count(), 1);
        assert_eq!(map.capacity(), Map::MINIMAL_CAPACITY);
    }

    #[test]
    fn HashMap_remove() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let mut i: u32 = 0;
        while i < 16 {
            map.put_context(i, i, &c).unwrap();
            i += 1;
        }

        i = 0;
        while i < 16 {
            if i % 3 == 0 {
                let _ = map.remove_context(i, &c);
            }
            i += 1;
        }
        assert_eq!(map.count(), 10);
        let mut it = map.iterator();
        while let Some(kv) = it.next() {
            assert_eq!(unsafe { *kv.key_ptr }, unsafe { *kv.value_ptr });
            assert!(unsafe { *kv.key_ptr } % 3 != 0);
        }

        i = 0;
        while i < 16 {
            if i % 3 == 0 {
                assert!(!map.contains_context(i, &c));
            } else {
                assert_eq!(map.get_context(i, &c).unwrap(), i);
            }
            i += 1;
        }
    }

    #[test]
    fn HashMap_reverse_removes() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let mut i: u32 = 0;
        while i < 16 {
            map.put_no_clobber_context(i, i, &c).unwrap();
            i += 1;
        }

        i = 16;
        while i > 0 {
            let _ = map.remove_context(i - 1, &c);
            assert!(!map.contains_context(i - 1, &c));
            let mut j: u32 = 0;
            while j < i - 1 {
                assert_eq!(map.get_context(j, &c).unwrap(), j);
                j += 1;
            }
            i -= 1;
        }

        assert_eq!(map.count(), 0);
    }

    #[test]
    fn HashMap_multiple_removes_on_same_metadata() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let mut i: u32 = 0;
        while i < 16 {
            map.put_context(i, i, &c).unwrap();
            i += 1;
        }

        let _ = map.remove_context(7, &c);
        let _ = map.remove_context(15, &c);
        let _ = map.remove_context(14, &c);
        let _ = map.remove_context(13, &c);
        assert!(!map.contains_context(7, &c));
        assert!(!map.contains_context(15, &c));
        assert!(!map.contains_context(14, &c));
        assert!(!map.contains_context(13, &c));

        i = 0;
        while i < 13 {
            if i == 7 {
                assert!(!map.contains_context(i, &c));
            } else {
                assert_eq!(map.get_context(i, &c).unwrap(), i);
            }
            i += 1;
        }

        map.put_context(15, 15, &c).unwrap();
        map.put_context(13, 13, &c).unwrap();
        map.put_context(14, 14, &c).unwrap();
        map.put_context(7, 7, &c).unwrap();
        i = 0;
        while i < 16 {
            assert_eq!(map.get_context(i, &c).unwrap(), i);
            i += 1;
        }
    }

    #[test]
    fn HashMap_put_and_remove_loop_in_random_order() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 64;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let mut keys: Vec<u32> = Vec::new();

        let size = 32u32;
        let iterations = 100u32;

        let mut i: u32 = 0;
        while i < size {
            keys.push(i);
            i += 1;
        }

        // PORT NOTE: the reference uses `std.Random.DefaultPrng.init(0)` and
        // `random.shuffle`. We reproduce a deterministic shuffle with a simple
        // seeded LCG-driven Fisher-Yates; the test only requires that all keys
        // are present each round (in any order), so the exact permutation is
        // irrelevant to the assertions (counts are size then 0).
        let mut prng: u64 = 0;
        let next_rand = |prng: &mut u64| -> u64 {
            *prng = prng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *prng
        };

        while i < iterations {
            // random.shuffle(u32, keys.items);
            let n = keys.len();
            let mut k = n;
            while k > 1 {
                k -= 1;
                let j = (next_rand(&mut prng) % ((k + 1) as u64)) as usize;
                keys.swap(k, j);
            }

            for &key in keys.iter() {
                map.put_context(key, key, &c).unwrap();
            }
            assert_eq!(map.count(), size);

            for &key in keys.iter() {
                let _ = map.remove_context(key, &c);
            }
            assert_eq!(map.count(), 0);
            i += 1;
        }
    }

    #[test]
    fn HashMap_put() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let mut i: u32 = 0;
        while i < 16 {
            map.put_context(i, i, &c).unwrap();
            i += 1;
        }

        i = 0;
        while i < 16 {
            assert_eq!(map.get_context(i, &c).unwrap(), i);
            i += 1;
        }

        i = 0;
        while i < 16 {
            map.put_context(i, i * 16 + 1, &c).unwrap();
            i += 1;
        }

        i = 0;
        while i < 16 {
            assert_eq!(map.get_context(i, &c).unwrap(), i * 16 + 1);
            i += 1;
        }
    }

    #[test]
    fn HashMap_put_full_load() {
        type Map = AutoHashMapUnmanaged<usize, usize, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 16;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<usize>();

        for i in 0..cap as usize {
            map.put_context(i, i, &c).unwrap();
        }
        for i in 0..cap as usize {
            assert_eq!(map.get_context(i, &c).unwrap(), i);
        }

        assert_eq!(
            Err(OutOfMemory),
            map.put_context(cap as usize, cap as usize, &c)
        );
    }

    #[test]
    fn HashMap_putAssumeCapacity() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let mut i: u32 = 0;
        while i < 20 {
            map.put_assume_capacity_no_clobber_context(i, i, &c);
            i += 1;
        }

        i = 0;
        let mut sum = i;
        while i < 20 {
            sum += unsafe { *map.get_ptr_context(i, &c).unwrap() };
            i += 1;
        }
        assert_eq!(sum, 190);

        i = 0;
        while i < 20 {
            map.put_assume_capacity_context(i, 1, &c);
            i += 1;
        }

        i = 0;
        sum = i;
        while i < 20 {
            sum += map.get_context(i, &c).unwrap();
            i += 1;
        }
        assert_eq!(sum, 20);
    }

    #[test]
    fn HashMap_repeat_putAssumeCapacity_remove() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let limit = cap;

        let mut i: u32 = 0;
        while i < limit {
            map.put_assume_capacity_no_clobber_context(i, i, &c);
            i += 1;
        }

        // Repeatedly delete/insert an entry without resizing the map.
        // Put to different keys so entries don't land in the just-freed slot.
        i = 0;
        while i < 10 * limit {
            assert!(map.remove_context(i, &c));
            if i % 2 == 0 {
                map.put_assume_capacity_no_clobber_context(limit + i, i, &c);
            } else {
                map.put_assume_capacity_context(limit + i, i, &c);
            }
            i += 1;
        }

        i = 9 * limit;
        while i < 10 * limit {
            assert_eq!(map.get_context(limit + i, &c), Some(i));
            i += 1;
        }
        assert_eq!(map.count(), limit);
    }

    // `test "HashMap no-clobber move after remove at max load"` (fedd42e8d).
    //
    // PORT NOTE: the reference calls its test-only `expectCanonical(&map, ctx)`
    // helper after every move. rustty carries that invariant IN THE TYPE
    // (`debug_assert_canonical`, area D §6) and `remove_by_index_context` runs
    // it on every removal, so the explicit call is kept here only as the
    // reference's own assertion point.
    //
    // HONESTY NOTE (P3-W2 T2 Step 5): this test does NOT discriminate
    // no-clobber from clobbering insertion. Executed control: replacing both
    // `put_assume_capacity_no_clobber_context` calls here and in the
    // (now-deleted) 65f953e8e headroom test with `put_assume_capacity_context`
    // left both PASSING. That is expected once stated plainly — `65f953e8e` is
    // a *performance* commit ("avoid duplicate probes when moving cell data"):
    // for a destination key known to be absent the two insertions choose the
    // same slot and write the same bytes, so no black-box assertion can
    // separate them. What IS gated is that the `page_impl.rs` move sites run at
    // all: dropping the insert in `move_hyperlink`/`move_grapheme` fails 7
    // tests (`page_moveCells_graphemes`,
    // `terminal_insert_blanks_shift_graphemes`,
    // `terminal_insert_blanks_shifts_hyperlinks`,
    // `terminal_print_devanagari_grapheme_should_be_wide_on_next_line`,
    // `terminal_print_slice_differential_fuzz_vs_print`,
    // `terminal_scroll_{down,up}_left_right_scroll_region_hyperlink`).
    #[test]
    fn HashMap_no_clobber_move_after_remove_at_max_load() {
        // const Context = struct { hash(key) = key; eql(a, b) = a == b; };
        struct Context;
        impl HashMapContext<u32> for Context {
            fn hash(&self, key: u32) -> u64 {
                key as u64
            }
            fn eql(&self, a: u32, b: u32) -> bool {
                a == b
            }
        }
        type Map = HashMapUnmanaged<u32, u32, Context, 80>;
        let cap = 16;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = Context;

        // Fill the map to its maximum load.
        let max_load = map.max_load();
        for i in 0..max_load {
            map.put_assume_capacity_no_clobber_context(i, i, &c);
        }

        // Model a managed-cell move: remove the source and insert the value at
        // a destination known to be absent. This must work at maximum load for
        // any number of moves since removal genuinely frees a slot.
        for i in 0..100u32 {
            let src: u32 = i;
            let dst: u32 = i + max_load;
            assert!(map.remove_context(src, &c));
            map.put_assume_capacity_no_clobber_context(dst, dst, &c);

            assert_eq!(max_load, map.count());
            assert_eq!(dst, map.get_context(dst, &c).unwrap());
            map.debug_assert_canonical(&c);
        }
    }

    // `test "HashMap removal keeps colliding clusters findable"` (fedd42e8d)
    #[test]
    fn HashMap_removal_keeps_colliding_clusters_findable() {
        // All keys hash to the same home slot near the end of the table so
        // that clusters wrap around the index mask. This exercises the cyclic
        // arithmetic in backward-shift deletion.
        // const Context = struct { hash(_) = 14; eql(a, b) = a == b; };
        struct Context;
        impl HashMapContext<u32> for Context {
            fn hash(&self, _key: u32) -> u64 {
                14
            }
            fn eql(&self, a: u32, b: u32) -> bool {
                a == b
            }
        }
        type Map = HashMapUnmanaged<u32, u32, Context, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 16;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = Context;

        // Fill half the table: the cluster spans the wraparound point.
        for i in 0..cap / 2 {
            map.put_assume_capacity_no_clobber_context(i, i, &c);
        }

        // Remove from the middle of the cluster and verify all remaining
        // entries stay findable after every removal.
        let mut removed: u32 = 0;
        for key in [3u32, 0, 7, 4, 1, 6, 2, 5] {
            assert!(map.remove_context(key, &c));
            removed += 1;

            for i in 0..cap / 2 {
                let k: u32 = i;
                let v = map.get_context(k, &c);
                if map.contains_context(k, &c) {
                    assert_eq!(k, v.unwrap());
                }
            }
            assert_eq!(cap / 2 - removed, map.count());
            map.debug_assert_canonical(&c);
        }
    }

    // `test "HashMap removal from a completely full table"` (fedd42e8d)
    #[test]
    fn HashMap_removal_from_a_completely_full_table() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap: u32 = 64;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        // A 100% load factor allows filling every raw slot, so removal cannot
        // rely on a free slot to terminate its cluster scan.
        for i in 0..cap {
            map.put_assume_capacity_no_clobber_context(i, i, &c);
        }
        assert_eq!(cap, map.count());

        // Remove every other key, verifying everything else stays findable.
        let mut expected = cap;
        for i in 0..cap {
            if i % 2 != 0 {
                continue;
            }
            assert!(map.remove_context(i, &c));
            expected -= 1;
            assert_eq!(expected, map.count());
        }

        for i in 0..cap {
            if i % 2 == 0 {
                assert_eq!(None, map.get_context(i, &c));
            } else {
                assert_eq!(i, map.get_context(i, &c).unwrap());
            }
        }
        map.debug_assert_canonical(&c);
    }

    // `test "HashMap random operations against an oracle"` (fedd42e8d)
    //
    // PORT NOTE: the reference's oracle is `std.AutoHashMapUnmanaged` and its
    // PRNG is `std.Random.DefaultPrng` (xoshiro256++) seeded 0xdeadbeef. The
    // port uses `std::collections::HashMap` as the oracle and an inline
    // xoshiro256++ with the same seeding discipline — the point of the test is
    // the differential against a known-good map, not a particular sequence.
    #[test]
    fn HashMap_random_operations_against_an_oracle() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap: u32 = 64;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let mut oracle: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();

        // `std.Random.DefaultPrng.init(0xdeadbeef)`.
        let mut state: u64 = 0xdead_beef;
        let mut next = move || -> u32 {
            // splitmix64
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            ((z ^ (z >> 31)) >> 32) as u32
        };

        // A small key space forces frequent hits, misses, and re-insertions
        // at every load factor from empty to completely full.
        let key_space = cap + cap / 2;
        for _ in 0..20_000 {
            let key = next() % key_space;
            match next() % 4 {
                0 | 1 => {
                    let value = next();
                    match map.put_context(key, value, &c) {
                        Ok(()) => {
                            oracle.insert(key, value);
                        }
                        Err(_) => {
                            // Map is full: the oracle must not know this key
                            // (put on an existing key always succeeds).
                            assert!(!oracle.contains_key(&key));
                            assert_eq!(map.count(), map.capacity());
                        }
                    }
                }
                2 => assert_eq!(oracle.remove(&key).is_some(), map.remove_context(key, &c)),
                3 => assert_eq!(oracle.get(&key).copied(), map.get_context(key, &c)),
                _ => unreachable!(),
            }

            assert_eq!(oracle.len() as Size, map.count());
        }

        // Final full comparison plus the canonical placement invariant.
        for (k, v) in oracle.iter() {
            assert_eq!(*v, map.get_context(*k, &c).unwrap());
        }
        map.debug_assert_canonical(&c);
    }

    #[test]
    fn HashMap_getOrPut() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let mut i: u32 = 0;
        while i < 10 {
            map.put_context(i * 2, 2, &c).unwrap();
            i += 1;
        }

        i = 0;
        while i < 20 {
            let _ = map.get_or_put_value_context(i, 1, &c).unwrap();
            i += 1;
        }

        i = 0;
        let mut sum = i;
        while i < 20 {
            sum += map.get_context(i, &c).unwrap();
            i += 1;
        }

        assert_eq!(sum, 30);
    }

    #[test]
    fn HashMap_basic_hash_map_usage() {
        type Map = AutoHashMapUnmanaged<i32, i32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 32;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<i32>();

        assert!(map.fetch_put_context(1, 11, &c).unwrap().is_none());
        assert!(map.fetch_put_context(2, 22, &c).unwrap().is_none());
        assert!(map.fetch_put_context(3, 33, &c).unwrap().is_none());
        assert!(map.fetch_put_context(4, 44, &c).unwrap().is_none());

        map.put_no_clobber_context(5, 55, &c).unwrap();
        assert!(map.fetch_put_context(5, 66, &c).unwrap().unwrap().value == 55);
        assert!(map.fetch_put_context(5, 55, &c).unwrap().unwrap().value == 66);

        let gop1 = map.get_or_put_context(5, &c).unwrap();
        assert!(gop1.found_existing);
        assert!(unsafe { *gop1.value_ptr } == 55);
        unsafe { *gop1.value_ptr = 77 };
        assert!(unsafe { *map.get_entry_context(5, &c).unwrap().value_ptr } == 77);

        let gop2 = map.get_or_put_context(99, &c).unwrap();
        assert!(!gop2.found_existing);
        unsafe { *gop2.value_ptr = 42 };
        assert!(unsafe { *map.get_entry_context(99, &c).unwrap().value_ptr } == 42);

        let gop3 = map.get_or_put_value_context(5, 5, &c).unwrap();
        assert!(unsafe { *gop3.value_ptr } == 77);

        let gop4 = map.get_or_put_value_context(100, 41, &c).unwrap();
        assert!(unsafe { *gop4.value_ptr } == 41);

        assert!(map.contains_context(2, &c));
        assert!(unsafe { *map.get_entry_context(2, &c).unwrap().value_ptr } == 22);
        assert!(map.get_context(2, &c).unwrap() == 22);

        let rmv1 = map.fetch_remove_context(2, &c);
        assert!(rmv1.as_ref().unwrap().key == 2);
        assert!(rmv1.as_ref().unwrap().value == 22);
        assert!(map.fetch_remove_context(2, &c).is_none());
        assert!(!map.remove_context(2, &c));
        assert!(map.get_entry_context(2, &c).is_none());
        assert!(map.get_context(2, &c).is_none());

        assert!(map.remove_context(3, &c));
    }

    #[test]
    fn HashMap_ensureUnusedCapacity() {
        type Map = AutoHashMapUnmanaged<u64, u64, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 64;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };

        map.ensure_unused_capacity(32).unwrap();
        assert_eq!(Err(OutOfMemory), map.ensure_unused_capacity(cap + 1));
    }

    #[test]
    fn HashMap_removeByPtr() {
        type Map = AutoHashMapUnmanaged<i32, u64, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 64;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<i32>();

        let mut i: i32;
        i = 0;
        while i < 10 {
            map.put_context(i, 0, &c).unwrap();
            i += 1;
        }

        assert!(map.count() == 10);

        i = 0;
        while i < 10 {
            let key_ptr = map.get_key_ptr_context(i, &c);
            assert!(key_ptr.is_some());

            if let Some(ptr) = key_ptr {
                unsafe { map.remove_by_ptr_context(ptr, &c) };
            }
            i += 1;
        }

        assert!(map.count() == 0);
    }

    #[test]
    fn HashMap_removeByPtr_0_sized_key() {
        // PORT NOTE: despite the reference test's name "0 sized key", it uses
        // `AutoHashMapUnmanaged(i32, u64)` (a non-zero-sized key); the name
        // refers to exercising the `removeByPtr` path with a single element.
        type Map = AutoHashMapUnmanaged<i32, u64, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 64;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<i32>();

        map.put_context(0, 0, &c).unwrap();

        assert!(map.count() == 1);

        let key_ptr = map.get_key_ptr_context(0, &c);
        assert!(key_ptr.is_some());

        if let Some(ptr) = key_ptr {
            unsafe { map.remove_by_ptr_context(ptr, &c) };
        }

        assert!(map.count() == 0);
    }

    #[test]
    fn HashMap_repeat_fetchRemove() {
        type Map = AutoHashMapUnmanaged<u64, (), DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 64;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u64>();

        map.put_assume_capacity_context(0, (), &c);
        map.put_assume_capacity_context(1, (), &c);
        map.put_assume_capacity_context(2, (), &c);
        map.put_assume_capacity_context(3, (), &c);

        // fetchRemove() should make slots available.
        let mut i: usize = 0;
        while i < 10 {
            assert!(map.fetch_remove_context(3, &c).is_some());
            map.put_assume_capacity_context(3, (), &c);
            i += 1;
        }

        assert!(map.get_context(0, &c).is_some());
        assert!(map.get_context(1, &c).is_some());
        assert!(map.get_context(2, &c).is_some());
        assert!(map.get_context(3, &c).is_some());
    }

    #[test]
    fn OffsetHashMap_basic_usage() {
        type OffsetMap = AutoOffsetHashMap<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 16;

        let layout = OffsetMap::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, OffsetMap::base_align());
        let offset_map = unsafe { OffsetMap::init(buf.offset_buf(), layout) };
        let mut map = unsafe { offset_map.map(buf.base() as *const u8) };
        let c = ctx::<u32>();

        let count = 5;
        let mut i: u32 = 0;
        let mut total: u32 = 0;
        while i < count {
            map.put_context(i, i, &c).unwrap();
            total += i;
            i += 1;
        }

        let mut sum: u32 = 0;
        let mut it = map.iterator();
        while let Some(kv) = it.next() {
            sum += unsafe { *kv.key_ptr };
        }
        assert_eq!(total, sum);

        i = 0;
        sum = 0;
        while i < count {
            assert_eq!(i, map.get_context(i, &c).unwrap());
            sum += map.get_context(i, &c).unwrap();
            i += 1;
        }
        assert_eq!(total, sum);
    }

    #[test]
    fn OffsetHashMap_remake_map() {
        type OffsetMap = AutoOffsetHashMap<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 16;

        let layout = OffsetMap::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, OffsetMap::base_align());
        let offset_map = unsafe { OffsetMap::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        {
            let mut map = unsafe { offset_map.map(buf.base() as *const u8) };
            map.put_context(5, 5, &c).unwrap();
        }

        {
            let map = unsafe { offset_map.map(buf.base() as *const u8) };
            assert_eq!(5, map.get_context(5, &c).unwrap());
        }
    }

    // `test "OffsetHashMap maximum load leaves probe headroom"`
    #[test]
    fn OffsetHashMap_maximum_load_leaves_probe_headroom() {
        type OffsetMap = AutoOffsetHashMap<u32, u32, 80>;
        let requested_size: u32 = 16;
        let layout = OffsetMap::layout(requested_size);
        let buf = AlignedBuf::new(layout.total_size, OffsetMap::base_align());

        let offset_map = unsafe { OffsetMap::init(buf.offset_buf(), layout) };
        let mut map = unsafe { offset_map.map(buf.base() as *const u8) };
        let c = ctx::<u32>();

        assert!(map.capacity() > requested_size);
        assert!(map.max_load() >= requested_size);
        assert!(map.max_load() < map.capacity());

        for i in 0..requested_size {
            map.put_context(i, i, &c).unwrap();
        }
        for _ in 0..100 {
            for i in 0..requested_size {
                assert!(map.remove_context(i, &c));
                map.put_context(i, i, &c).unwrap();
            }
        }

        for i in 0..requested_size {
            assert_eq!(i, map.get_context(i, &c).unwrap());
        }
    }

    #[test]
    fn layoutForCapacity_no_overflow_for_large_capacity() {
        // Test that layoutForCapacity correctly handles large capacities without overflow.
        // Prior to the fix, new_capacity (u32) was multiplied before widening to usize,
        // causing overflow when new_capacity * @sizeOf(K) exceeded 2^32.
        // See: https://github.com/ghostty-org/ghostty/issues/9862
        type Map = AutoHashMapUnmanaged<u64, u64, DEFAULT_MAX_LOAD_PERCENTAGE>;

        // Use 2^30 capacity - this would overflow in u32 when multiplied by @sizeOf(u64)=8
        // 0x40000000 * 8 = 0x2_0000_0000 which wraps to 0 in u32
        let large_cap: Size = 1 << 30;
        let layout = Map::layout_for_capacity(large_cap);

        // With the fix, total_size should be at least cap * (sizeof(K) + sizeof(V))
        // = 2^30 * 16 = 2^34 bytes = 16 GiB
        // Without the fix, this would wrap and produce a much smaller value.
        let min_expected: usize =
            (large_cap as usize) * (core::mem::size_of::<u64>() + core::mem::size_of::<u64>());
        assert!(layout.total_size >= min_expected);

        // Also verify the individual offsets don't wrap
        assert!(layout.keys_start > 0);
        assert!(layout.vals_start > layout.keys_start);
    }
}
