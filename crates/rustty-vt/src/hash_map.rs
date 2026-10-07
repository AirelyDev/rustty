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
// Since `6112935a2` nothing in the reference file calls `getOffset` any more
// (the import stays in the reference); the port drops the unused `use`.
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

use rustty_render_types::term_size::{Offset, OffsetBuf};
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

/// The default allows every raw slot to be occupied. Callers whose maps see
/// removal-heavy churn should choose a lower value to bound probe lengths.
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
// PORT NOTE: the reference auto context hashes the key with seed-0 Wyhash over
// the key's bytes (`std.hash.Wyhash.hash(0, std.mem.asBytes(&key))` for
// `hasUniqueRepresentation` keys, `std.hash.autoHash` into a `Wyhash.init(0)`
// otherwise) and compares with `std.meta.eql`. The port reproduces that hash
// BIT-EXACTLY via the `std.hash.Wyhash` port in rustty-zigstd: the hash is not
// an implementation detail here, because the map lives inside the page's single
// backing allocation, so it decides home slots (`hash & mask`), metadata
// fingerprints (top 7 bits), probe/iterator order, and therefore the raw page
// bytes `compress/page.rs` hands to LZ4. Equality stays Rust's `Eq`, the direct
// map of `std.meta.eql` for these key types.
//
// The keys are fed through Rust's `Hash`, which for the port's key types
// (integers, and `#[repr(transparent)]` `Offset<T>` over a `u32`, whose `Hash`
// impl hashes just that field) writes exactly the native-endian bytes
// `std.mem.asBytes(&key)` yields — see the hasher note in
// `rustty_zigstd::wyhash`.
//
// PORT NOTE (`3d9b2b483` wasm arm): the reference's `hash` carries a
// wasm32-only small-key (`@sizeOf(K) <= 8`) branch — a splitmix64-style
// finalizer instead of Wyhash (see the comment in `hash` below). That arm is
// `comptime builtin.cpu.arch.isWasm()`-gated dead code on every native target;
// because it changes home slots/fingerprints/probe order, i.e. the raw page
// bytes, it landed TOGETHER with the wasm page-pool wiring (`492c26067`,
// `87f69a12e` — the wasm/datastruct tier).
pub struct AutoContext<K>(PhantomData<K>);

/// The byte collector behind the wasm small-key hash arm: gathers the key's
/// `Hash`-written bytes into the low bytes of a `u64`, exactly the Zig
/// `var x: u64 = 0; @memcpy(std.mem.asBytes(&x)[0..@sizeOf(K)],
/// std.mem.asBytes(&key));` on a little-endian target (wasm is always LE) —
/// the port's key types write exactly their native-endian bytes through `Hash`
/// (see the note above). `finish` returns the raw collected word; the
/// splitmix64-style finalizer is applied at the call site so the Zig code
/// shape stays visible.
struct SmallKeyBytes {
    x: u64,
    len: usize,
}

impl core::hash::Hasher for SmallKeyBytes {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // `@sizeOf(K) <= 8` is checked by the branch guard; the port's key
        // types write only their own bytes.
        debug_assert!(self.len + bytes.len() <= 8);
        for &b in bytes {
            self.x |= (b as u64) << (8 * self.len);
            self.len += 1;
        }
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.x
    }
}

impl<K> Default for AutoContext<K> {
    fn default() -> Self {
        AutoContext(PhantomData)
    }
}

impl<K: core::hash::Hash + PartialEq> HashMapContext<K> for AutoContext<K> {
    // PORT NOTE (`f2a7652ab`'s `hash_map.zig` hunk, WS10 T16): upstream replaced
    // `std.hash_map.getAutoHashFn` with an explicit body that force-inlines the
    // hash for `hasUniqueRepresentation(K)` types, because:
    //
    // LLVM 21 (Zig 0.16) failed to inline this which resulted
    // in a measurable almost 2x slowdown on our hyperlink map
    // benchmark. So, force it.
    //
    // This port's context is an explicit body (see the note above), and the
    // `hasUniqueRepresentation` split IS load-bearing here, not just a spelling
    // choice: the one-shot `Wyhash::hash` on a small key goes straight through
    // `small_key` with no hasher-state materialization, while the iterative
    // `init`/`update`/`finalize` route spills the 88-byte state and buffers the
    // key even when inlined (measured: the iterative route alone made churn on
    // the hyperlink map benchmark ~4x slower than the reference). Small
    // fixed-width keys (all of the terminal page maps') take the one-shot path
    // below — `SmallKeyBytes` reassembles the exact byte sequence the key's
    // `Hash` impl writes, so the digest is unchanged (one-shot and iterative
    // Wyhash agree on the same byte sequence).
    #[inline]
    fn hash(&self, key: K) -> u64 {
        use core::hash::Hasher;

        // On wasm32, Wyhash's 64x64 -> 128 multiplies lower to
        // __multi3 libcalls (wasm has no widening multiply
        // instruction), which makes the hash itself the
        // dominant cost of small-key map operations.
        //
        // Keys here fit in a u64, so use a splitmix64-style finalizer
        // instead: two native i64.mul with full avalanche in
        // both the low bits (slot index) and high bits
        // (metadata fingerprint).
        // if (comptime builtin.cpu.arch.isWasm() and @sizeOf(K) <= 8) {
        if cfg!(target_arch = "wasm32") && core::mem::size_of::<K>() <= 8 {
            // var x: u64 = 0;
            // @memcpy(
            //     std.mem.asBytes(&x)[0..@sizeOf(K)],
            //     std.mem.asBytes(&key),
            // );
            let mut collector = SmallKeyBytes { x: 0, len: 0 };
            key.hash(&mut collector);
            let mut x = collector.finish();
            // x ^= x >> 33;
            x ^= x >> 33;
            // x *%= 0xff51afd7ed558ccd;
            x = x.wrapping_mul(0xff51afd7ed558ccd);
            // x ^= x >> 33;
            x ^= x >> 33;
            // x *%= 0xc4ceb9fe1a85ec53;
            x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
            // x ^= x >> 33;
            x ^= x >> 33;
            return x;
        }

        // `if (comptime std.meta.hasUniqueRepresentation(K))` — the one-shot
        // form over the key's bytes, force-inlined (see the note above). The
        // key types taking this arm are integers and `#[repr(transparent)]`
        // wrappers over them, whose `Hash` impls write exactly their own
        // bytes, so `SmallKeyBytes` reproduces `std.mem.asBytes(&key)`.
        //     return @call(
        //         .always_inline,
        //         std.hash.Wyhash.hash,
        //         .{ 0, std.mem.asBytes(&key) },
        //     );
        if core::mem::size_of::<K>() <= 8 {
            let mut collector = SmallKeyBytes { x: 0, len: 0 };
            key.hash(&mut collector);
            // `asBytes` has a COMPTIME length; feeding a runtime `collector.len`
            // here left the ≤16-byte branch in `Wyhash::hash` unfoldable and the
            // whole hash out-of-line. `size_of::<K>()` is the same value as a
            // compile-time constant (these keys write exactly their own bytes).
            debug_assert_eq!(collector.len, core::mem::size_of::<K>());
            let bytes = collector.x.to_le_bytes();
            return rustty_zigstd::wyhash::Wyhash::hash(0, &bytes[..core::mem::size_of::<K>()]);
        }

        let mut hasher = rustty_zigstd::wyhash::Wyhash::init(0);
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
    /// The slot metadata in the backing memory. The map's size counter
    /// sits immediately before it (see `Unmanaged.Header`).
    pub metadata: Offset<Metadata>,

    /// The key and value arrays in the backing memory.
    pub keys: Offset<K>,
    pub values: Offset<V>,

    /// The number of slots. This never changes after init, so it is
    /// kept here rather than in the backing memory.
    pub capacity: Size,

    _marker: PhantomData<(K, V, Context)>,
}

// PORT NOTE: the reference `OffsetHashMap` is a plain struct of three `Offset`s
// (each a `u32`) and a `u32` capacity, so `var x = self.*` copies it trivially.
// We mirror that with `Clone`/`Copy` (every field is `Copy`); needed by `Page`
// clone, which copies the whole struct (`var result = self.*`).
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

// PORT NOTE: the reference field defaults are `metadata: Offset(..) = .{}`,
// `keys: Offset(K) = .{}`, `values: Offset(V) = .{}`, `capacity: Size = 0`.
impl<K, V, Context: HashMapContext<K>, const P: u8> Default for OffsetHashMap<K, V, Context, P> {
    fn default() -> Self {
        OffsetHashMap {
            metadata: Offset::default(),
            keys: Offset::default(),
            values: Offset::default(),
            capacity: 0,
            _marker: PhantomData,
        }
    }
}

impl<K: Copy, V: Copy, Context: HashMapContext<K>, const P: u8> OffsetHashMap<K, V, Context, P> {
    /// This is the alignment that the base pointer must have.
    // `pub const base_align = Unmanaged.base_align;`
    // PERF(port): `const fn` — the reference's `base_align` is a comptime
    // constant, and `Page::MetaLayout::ALIGNMENT` folds it into a `const`.
    pub const fn base_align() -> usize {
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
        let this = unsafe { Self::init_assume_zeroed(buf, l) };
        let mut m = unsafe { this.map(buf) };
        m.clear_retaining_capacity();
        this
    }

    /// Like `init`, but for backing memory that the caller guarantees
    /// is already zero-filled (e.g. fresh OS pages). This writes
    /// nothing to the backing memory: all-zero slot metadata means
    /// every slot is free and a zero size counter means empty, so the
    /// OS pages behind the map stay untouched until the first insert.
    ///
    /// # Safety
    ///
    /// As `init`, and the header and metadata region of the buffer must be
    /// zero. Behavior is undefined otherwise.
    pub unsafe fn init_assume_zeroed(buf: OffsetBuf, l: Layout) -> Self {
        assert(unsafe { buf.start() } as usize % Self::base_align() == 0);
        OffsetHashMap {
            metadata: buf.member(l.metadata_start),
            keys: buf.member(l.keys_start),
            values: buf.member(l.vals_start),
            capacity: l.capacity,
            _marker: PhantomData,
        }
    }

    /// Returns the pointer-based map from a base pointer.
    ///
    /// # Safety
    ///
    /// `base` must be the true base pointer this map was initialized against.
    //
    // PORT NOTE (P7-W1 T2, Stacked/Tree Borrows): the reference passes
    // `base: anytype` to each of the three `Offset.ptr` calls. The port reads
    // the base pointer ONCE (`ptr_from_base`) and derives metadata, keys and
    // values from that one raw pointer, so all three cached pointers share the
    // same provenance. Every page caller hands in the page's raw
    // `memory_ptr()`, never a `&mut [u8]`/`&mut Page` projection: a cached
    // pointer derived from a `&mut` retag is invalidated by the next `&mut`
    // over the page memory (the hyperlink set release or the grapheme alloc
    // between two map calls). Ledger: `.port/audits/GHOSTTY_LIFETIMES.tsv`
    // (`HashMapUnmanaged` metadata/keys/values); proof harness:
    // `.port/phase-2/mirihar-p7-hashmap/`.
    pub unsafe fn map<B: rustty_render_types::term_size::IntFromBase>(
        &self,
        base: B,
    ) -> HashMapUnmanaged<K, V, Context, P> {
        let base = base.ptr_from_base();
        HashMapUnmanaged {
            metadata: unsafe { self.metadata.ptr(base) },
            keys: unsafe { self.keys.ptr(base) },
            values: unsafe { self.values.ptr(base) },
            cap: self.capacity,
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

/// The part of the map's state that changes after init. It lives
/// in the backing buffer so that the map can be handed around by
/// value; its zero value is the empty map.
//
// PORT NOTE: the reference `Header` is a plain Zig struct laid out immediately
// before the metadata. Since `6112935a2` its only field is the size counter
// (the capacity and the key/value offsets moved into the map structs).
#[repr(C)]
struct Header {
    size: Size,
}

/// The memory layout for the underlying buffer for a given capacity.
/// All offsets are from the start of the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The total size of the buffer required. The buffer is expected
    /// to be aligned to `base_align`.
    pub total_size: usize,

    /// The offset to the start of the slot metadata. The header
    /// occupies the bytes before it.
    pub metadata_start: usize,

    /// The offset to the start of the keys data.
    pub keys_start: usize,

    /// The offset to the start of the values data.
    pub vals_start: usize,

    /// The capacity that was used to calculate this layout.
    pub capacity: Size,
}

/// Fork of stdlib.HashMap as of Zig 0.12 modified to use offsets for
/// the key/values pointer, and backward-shift deletion in place of
/// tombstones. The metadata is still a pointer to limit the amount of
/// arithmetic required to access it. See the file comment for full details.
//
// LIFETIMES (`.port/audits/GHOSTTY_LIFETIMES.tsv`, P7-W1 T2 rows):
//   metadata / keys / values — BORROW_FIELD, raw non-owning pointers into the
//     backing buffer (the page's memory). All three are derived from ONE raw
//     base (`OffsetHashMap::map`, `init_assume_zeroed`), never from a `&mut`
//     projection; the buffer's owner outlives every map value built from it.
pub struct HashMapUnmanaged<K, V, Context: HashMapContext<K>, const MAX_LOAD_PERCENTAGE: u8> {
    // The backing buffer holds a `Header` (the size counter) followed
    // by the `Metadata`s, then the keys and values arrays. Everything
    // the map needs that does not change after init, the capacity and
    // the entry pointers, lives in this struct instead of the buffer,
    // so that a zero-filled buffer is a valid empty map and init never
    // has to write to it.
    /// Pointer to the slot metadata. The header sits right before it.
    pub metadata: *mut Metadata,

    /// The key and value arrays.
    pub keys: *mut K,
    pub values: *mut V,

    /// The number of slots. Always zero or a power of two.
    pub cap: Size,

    _marker: PhantomData<(K, V, Context)>,
}

impl<K: Copy, V: Copy, Context: HashMapContext<K>, const MAX_LOAD_PERCENTAGE: u8>
    HashMapUnmanaged<K, V, Context, MAX_LOAD_PERCENTAGE>
{
    // comptime { assert(@alignOf(Metadata) == 1); }
    const _ASSERT_META_ALIGN: () = assert!(core::mem::align_of::<Metadata>() == 1);
    // comptime { assert(max_load_percentage > 0); assert(max_load_percentage <= 100); }
    const _ASSERT_MAX_LOAD: () = assert!(MAX_LOAD_PERCENTAGE > 0 && MAX_LOAD_PERCENTAGE <= 100);

    const fn header_align() -> usize {
        core::mem::align_of::<Header>()
    }
    const fn key_align() -> usize {
        if core::mem::size_of::<K>() == 0 {
            1
        } else {
            core::mem::align_of::<K>()
        }
    }
    const fn val_align() -> usize {
        if core::mem::size_of::<V>() == 0 {
            1
        } else {
            core::mem::align_of::<V>()
        }
    }

    /// `base_align: mem.Alignment = .fromByteUnits(@max(header_align, key_align, val_align));`
    // PERF(port): `const fn` (comptime in the reference); `usize::max` is not
    // `const`, hence the comparisons.
    pub const fn base_align() -> usize {
        let (h, k, v) = (Self::header_align(), Self::key_align(), Self::val_align());
        let hk = if h > k { h } else { k };
        if hk > v {
            hk
        } else {
            v
        }
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
        let mut map = unsafe { Self::init_assume_zeroed(buf, layout) };
        map.clear_retaining_capacity();
        map
    }

    /// Like `init`, but for a buffer that the caller guarantees is
    /// already zero-filled. Nothing is written: an all-zero metadata
    /// byte is a free slot (see `Metadata.isFree`) and a zero header
    /// is an empty map. Behavior is undefined if the header and
    /// metadata region is not zero.
    ///
    /// # Safety
    ///
    /// As `init`, and the header and metadata region must be zero.
    pub unsafe fn init_assume_zeroed(buf: OffsetBuf, layout: Layout) -> Self {
        // Force the comptime asserts to be evaluated for this instantiation.
        let () = Self::_ASSERT_META_ALIGN;
        let () = Self::_ASSERT_META_SIZE;
        let () = Self::_ASSERT_META_ALIGN2;
        let () = Self::_ASSERT_MAX_LOAD;

        assert(unsafe { buf.start() } as usize % Self::base_align() == 0);
        HashMapUnmanaged {
            metadata: unsafe { buf.start().add(layout.metadata_start) } as *mut Metadata,
            keys: unsafe { buf.member::<K>(layout.keys_start).ptr(buf) },
            values: unsafe { buf.member::<V>(layout.vals_start).ptr(buf) },
            cap: layout.capacity,
            _marker: PhantomData,
        }
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
        unsafe {
            self.init_metadatas();
            (*self.header()).size = 0;
        }
    }

    pub fn count(&self) -> Size {
        unsafe { (*self.header()).size }
    }

    /// # Safety: metadata must point into a live backing buffer.
    unsafe fn header(&self) -> *mut Header {
        // @ptrCast(@as([*]Header, @ptrCast(@alignCast(self.metadata))) - 1)
        let meta = self.metadata as *mut Header;
        unsafe { meta.sub(1) }
    }

    pub fn capacity(&self) -> Size {
        self.cap
    }

    /// Maximum number of entries the map will hold. This is less than
    /// capacity when MAX_LOAD_PERCENTAGE is below 100, which keeps free
    /// slots in every probe chain and bounds probe lengths.
    pub fn max_load(&self) -> Size {
        Self::max_load_for_capacity(self.cap)
    }

    pub fn iterator(&self) -> Iterator<'_, K, V, Context, MAX_LOAD_PERCENTAGE> {
        Iterator { hm: self, index: 0 }
    }

    // pub fn keyIterator(self: *const Self) KeyIterator {
    //     return .{
    //         .len = self.cap,
    //         .metadata = self.metadata,
    //         .items = self.keys,
    //     };
    // }
    pub fn key_iterator(&self) -> KeyIterator<'_, K> {
        FieldIterator {
            len: self.cap as usize,
            metadata: self.metadata,
            items: self.keys,
            _marker: PhantomData,
        }
    }

    // pub fn valueIterator(self: *const Self) ValueIterator { ... }
    // (the same shape as `keyIterator`, over `self.values`)
    pub fn value_iterator(&self) -> ValueIterator<'_, V> {
        FieldIterator {
            len: self.cap as usize,
            metadata: self.metadata,
            items: self.values,
            _marker: PhantomData,
        }
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
        debug_assert!(unsafe { (*self.header()).size } < self.cap);

        let hash = ctx.hash(key);
        let mask = self.cap - 1;
        let mut idx = (hash & (mask as u64)) as usize;

        let mut metadata = unsafe { self.metadata.add(idx) };
        while unsafe { (*metadata).is_used() } {
            idx = (idx + 1) & (mask as usize);
            metadata = unsafe { self.metadata.add(idx) };
        }

        unsafe {
            (*metadata).fill(Metadata::take_fingerprint(hash));
            *self.keys.add(idx) = key;
            *self.values.add(idx) = value;
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
            key: unsafe { *self.keys.add(idx) },
            value: unsafe { *self.values.add(idx) },
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
        let mask = self.cap - 1;
        let fingerprint = Metadata::take_fingerprint(hash);
        // Don't loop indefinitely when there are no free slots.
        let mut limit = self.cap;
        let mut idx = (hash & (mask as u64)) as usize;

        let mut metadata = unsafe { self.metadata.add(idx) };
        while !unsafe { (*metadata).is_free() } && limit != 0 {
            if unsafe { (*metadata).is_used() }
                && unsafe { (*metadata).fingerprint() } == fingerprint
            {
                let test_key = unsafe { self.keys.add(idx) };
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
            metadata = unsafe { self.metadata.add(idx) };
        }

        None
    }

    pub fn get_entry_context(&self, key: K, ctx: &Context) -> Option<Entry<K, V>> {
        self.get_entry_adapted(key, ctx)
    }
    // PERF(port): `#[inline]` — hot probe path; cross-crate callers can't
    // inline without LTO (banned), the reference's single-CU build inlines it.
    #[inline]
    pub fn get_entry_adapted(&self, key: K, ctx: &Context) -> Option<Entry<K, V>> {
        if let Some(idx) = self.get_index(key, ctx) {
            return Some(Entry {
                key_ptr: unsafe { self.keys.add(idx) },
                value_ptr: unsafe { self.values.add(idx) },
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
            return Some(unsafe { self.keys.add(idx) });
        }
        None
    }

    /// Get a copy of the actual key associated with adapted key, if present.
    pub fn get_key_context(&self, key: K, ctx: &Context) -> Option<K> {
        self.get_key_adapted(key, ctx)
    }
    pub fn get_key_adapted(&self, key: K, ctx: &Context) -> Option<K> {
        if let Some(idx) = self.get_index(key, ctx) {
            return Some(unsafe { *self.keys.add(idx) });
        }
        None
    }

    /// Get an optional pointer to the value associated with key, if present.
    pub fn get_ptr_context(&self, key: K, ctx: &Context) -> Option<*mut V> {
        self.get_ptr_adapted(key, ctx)
    }
    pub fn get_ptr_adapted(&self, key: K, ctx: &Context) -> Option<*mut V> {
        if let Some(idx) = self.get_index(key, ctx) {
            return Some(unsafe { self.values.add(idx) });
        }
        None
    }

    /// Get a copy of the value associated with key, if present.
    // PERF(port): `#[inline]` — hot probe path; cross-crate callers can't
    // inline without LTO (banned), the reference's single-CU build inlines it.
    #[inline]
    pub fn get_context(&self, key: K, ctx: &Context) -> Option<V> {
        self.get_adapted(key, ctx)
    }
    // PERF(port): `#[inline]` — hot probe path; cross-crate callers can't
    // inline without LTO (banned), the reference's single-CU build inlines it.
    #[inline]
    pub fn get_adapted(&self, key: K, ctx: &Context) -> Option<V> {
        if let Some(idx) = self.get_index(key, ctx) {
            return Some(unsafe { *self.values.add(idx) });
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
                    key_ptr: unsafe { self.keys.add(index) },
                    value_ptr: unsafe { self.values.add(index) },
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
        let mask = self.cap - 1;
        let fingerprint = Metadata::take_fingerprint(hash);
        let mut limit = self.cap;
        let mut idx = (hash & (mask as u64)) as usize;

        let mut metadata = unsafe { self.metadata.add(idx) };
        while !unsafe { (*metadata).is_free() } && limit != 0 {
            if unsafe { (*metadata).is_used() }
                && unsafe { (*metadata).fingerprint() } == fingerprint
            {
                let test_key = unsafe { self.keys.add(idx) };
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
                        value_ptr: unsafe { self.values.add(idx) },
                        found_existing: true,
                    };
                }
            }

            limit -= 1;
            idx = (idx + 1) & (mask as usize);
            metadata = unsafe { self.metadata.add(idx) };
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
            let new_key = self.keys.add(idx);
            let new_value = self.values.add(idx);
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
    // while also reading `keys[j]` to rehash it. `metadata`, `keys` and
    // `values` are therefore kept as RAW pointers; do NOT form `&mut [K]` and
    // `&mut [Metadata]` simultaneously from the same base for the convenience
    // of indexing. Proof harness: `.port/phase-3/mirihar-hashmap-shift/`.
    //
    // CONTRACT: this moves other entries within the probe cluster, so every key
    // and value pointer the map has handed out is invalidated. Caller audit:
    // `.port/phase-3/HASHMAP_CALLER_AUDIT.md` (12/12 rows resolved).
    //
    // PERF(port, N511): `#[inline]` here and on `remove_by_ptr_context`. The
    // reference's single-CU ReleaseFast build inlines `removeByPtr` and this
    // body into `Page.clearHyperlink`; rustc's LLVM left them out of line,
    // which made the caller spill the map's four fields to the stack for the
    // call and re-materialize the Wyhash constants inside it (part of the
    // `+hyperlink-map --mode=churn` gap).
    #[inline]
    fn remove_by_index_context(&mut self, idx: usize, ctx: &Context) {
        let mask: usize = (self.cap - 1) as usize;
        let metadata = self.metadata;
        let keys_ptr = self.keys;
        let values_ptr = self.values;

        // A completely full table has no free slot to terminate the
        // scan, so bound it to one full cycle. That is sufficient: the
        // hole only ever moves forward to slots the scan has already
        // visited, so each entry needs to be considered exactly once.
        let mut hole = idx;
        let mut j = idx;
        let mut limit = self.cap - 1;
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
    // PERF(port, N511): `#[inline]`, see `remove_by_index_context`.
    #[inline]
    pub unsafe fn remove_by_ptr_context(&mut self, key_ptr: *mut K, ctx: &Context) {
        // TODO: replace with pointer subtraction once supported by zig
        // if @sizeOf(K) == 0 then there is at most one item in the hash
        // map, which is assumed to exist as key_ptr must be valid.  This
        // item must be at index 0.
        //
        // PORT NOTE (DELIBERATE DIVERGENCE, W24-B03-ZST-01): the reference's
        // zero-sized-key fallback is `idx = 0`, and the first half of its
        // comment is right (a ZST key type has exactly one possible key, so the
        // map holds at most one entry) while the conclusion is not: that single
        // entry sits at its home slot `hash(key) & (capacity - 1)`, which is 0
        // only when the hash happens to end in enough zero bits. With seed-0
        // Wyhash, `hash(void) = Wyhash.hash(0, "") = 0x0409638ee2bde459`, so at
        // capacity 16 the entry lives at slot 9 and the reference removes the
        // FREE slot 0 instead — decrementing `size` while leaving the used
        // metadata behind, after which the key is neither findable nor
        // re-insertable. The oracle has the same bug (its named
        // "removeByPtr 0 sized key" test uses a non-ZST `i32` key, so nothing
        // exercises the path), but it is unreachable there and here: every
        // product caller keys on `Offset(Cell)` (grapheme/hyperlink maps). The
        // port scans for the one used slot instead, which is invisible to every
        // non-ZST caller (the branch is `size_of::<K>() == 0` only) and correct
        // for the generic contract the tests below now pin.
        let idx = if core::mem::size_of::<K>() > 0 {
            ((key_ptr as usize) - (self.keys as usize)) / core::mem::size_of::<K>()
        } else {
            let metadata = self.metadata;
            let capacity = self.cap as usize;
            let mut found: Option<usize> = None;
            for i in 0..capacity {
                if unsafe { (*metadata.add(i)).is_used() } {
                    found = Some(i);
                    break;
                }
            }
            // `key_ptr` must be valid, so the entry exists (same precondition
            // the reference relies on to justify its fallback).
            found.expect("removeByPtr with a zero-sized key on an empty map")
        };

        self.remove_by_index_context(idx, ctx);
    }

    /// # Safety: metadata must be valid.
    unsafe fn init_metadatas(&mut self) {
        // @memset(@as([*]u8, @ptrCast(self.metadata))[0 .. @sizeOf(Metadata) * self.cap], 0);
        unsafe {
            core::slice::from_raw_parts_mut(
                self.metadata as *mut u8,
                core::mem::size_of::<Metadata>() * self.cap as usize,
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

        if let Some(msg) = self.canonical_violation(ctx) {
            panic!("{msg}");
        }
    }

    /// `expectCanonical(map, ctx)` — the reference's TEST helper, which is a
    /// plain `try expect(...)` and therefore runs at EVERY optimize mode.
    //
    // PORT NOTE (parity S155-F4): the port folded this into
    // `debug_assert_canonical` above, which is strictly MORE coverage in a debug
    // build (every removal on every map, not just four tests) and strictly LESS
    // in a release one, where it returns immediately — so under
    // `cargo test -p rustty-vt --release` the four tests that exist to check
    // this invariant checked nothing at all and still passed. Both now exist:
    // the production debug invariant, and this unconditional test-side
    // assertion, which is the one the reference has.
    #[cfg(test)]
    pub(crate) fn expect_canonical(&self, ctx: &Context) {
        if let Some(msg) = self.canonical_violation(ctx) {
            panic!("{msg}");
        }
    }

    /// The shared scan: `None` when the canonical placement invariant holds.
    fn canonical_violation(&self, ctx: &Context) -> Option<String> {
        let cap = self.capacity();
        if cap == 0 {
            return None;
        }
        let mask = (cap - 1) as usize;
        let mut used: Size = 0;
        for idx in 0..cap as usize {
            let metadata = unsafe { *self.metadata.add(idx) };
            if !metadata.is_used() {
                continue;
            }
            used += 1;

            let mut probe: usize =
                (ctx.hash(unsafe { *self.keys.add(idx) }) & (mask as u64)) as usize;
            while probe != idx {
                if !unsafe { (*self.metadata.add(probe)).is_used() } {
                    return Some(format!(
                        "canonical placement violated: entry at slot {idx} is separated \
                         from its home slot by the free slot {probe}"
                    ));
                }
                probe = (probe + 1) & mask;
            }
        }
        if self.count() != used {
            return Some(format!(
                "canonical placement violated: count() == {} but {used} slots are used",
                self.count()
            ));
        }
        None
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

        // Pack our header, metadata, keys, and values.
        let meta_start = core::mem::size_of::<Header>();
        let meta_end = meta_start + cap * core::mem::size_of::<Metadata>();
        let keys_start = align_forward(meta_end, Self::key_align());
        let keys_end = keys_start + cap * core::mem::size_of::<K>();
        let vals_start = align_forward(keys_end, Self::val_align());
        let vals_end = vals_start + cap * core::mem::size_of::<V>();

        // Our total memory size required is the end of our values
        // aligned to the base required alignment.
        let total_size = align_forward(vals_end, Self::base_align());

        Layout {
            total_size,
            metadata_start: meta_start,
            keys_start,
            vals_start,
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

/// Iterates the entries of the map. Any mutation of the map
/// invalidates the iterator: removal may move entries across the
/// iteration cursor.
///
/// `pub const Iterator = struct { hm, index };`
pub struct Iterator<'a, K, V, Context: HashMapContext<K>, const MAX_LOAD_PERCENTAGE: u8> {
    hm: &'a HashMapUnmanaged<K, V, Context, MAX_LOAD_PERCENTAGE>,
    index: Size,
}

impl<K: Copy, V: Copy, Context: HashMapContext<K>, const P: u8> Iterator<'_, K, V, Context, P> {
    pub fn next(&mut self) -> Option<Entry<K, V>> {
        assert(self.index <= self.hm.cap);
        if unsafe { (*self.hm.header()).size } == 0 {
            return None;
        }

        let cap = self.hm.cap;
        let end = unsafe { self.hm.metadata.add(cap as usize) };
        let mut metadata = unsafe { self.hm.metadata.add(self.index as usize) };

        while metadata != end {
            if unsafe { (*metadata).is_used() } {
                let key = unsafe { self.hm.keys.add(self.index as usize) };
                let value = unsafe { self.hm.values.add(self.index as usize) };
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

// pub const KeyIterator = FieldIterator(K);
// pub const ValueIterator = FieldIterator(V);
// PORT NOTE (parity S155-F2): Zig nests these in the map type; Rust has no
// inherent associated types, so they are module-level aliases carrying the
// map borrow's lifetime.
pub type KeyIterator<'a, K> = FieldIterator<'a, K>;
pub type ValueIterator<'a, V> = FieldIterator<'a, V>;

// fn FieldIterator(comptime T: type) type {
//     return struct {
//         len: usize,
//         metadata: [*]const Metadata,
//         items: [*]T,
//         ...
//     };
// }
/// Walks one of the map's parallel arrays (keys or values), yielding a
/// pointer to each slot whose metadata is used.
// PORT NOTE: the `'a` lifetime ties the iterator to the `&self` borrow it was
// made from, as `Iterator` above is; the reference's raw many-pointers carry
// no lifetime. `next` returns `*mut T` like `Entry`'s pointers.
pub struct FieldIterator<'a, T> {
    len: usize,
    metadata: *const Metadata,
    items: *mut T,
    _marker: PhantomData<&'a ()>,
}

impl<T> FieldIterator<'_, T> {
    // pub fn next(self: *@This()) ?*T {
    pub fn next(&mut self) -> Option<*mut T> {
        // while (self.len > 0) {
        while self.len > 0 {
            // self.len -= 1;
            self.len -= 1;
            // const used = self.metadata[0].isUsed();
            // SAFETY: `len` counts the slots left in both arrays, so
            // `metadata` and `items` point at a live slot here.
            let used = unsafe { (*self.metadata).is_used() };
            // const item = &self.items[0];
            let item = self.items;
            // self.metadata += 1;
            // self.items += 1;
            // SAFETY: one past the last slot at most, which is not read.
            self.metadata = unsafe { self.metadata.add(1) };
            self.items = unsafe { self.items.add(1) };
            // if (used) { return item; }
            if used {
                return Some(item);
            }
        }
        // return null;
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
        // `alloc.alignedAlloc` + `@memset(buf, 0)`: the zero-filled backing
        // memory `initAssumeZeroed` requires (fresh OS pages in the product).
        fn new_zeroed(total_size: usize, base_align: usize) -> AlignedBuf {
            let layout = StdLayout::from_size_align(total_size, base_align).unwrap();
            let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
            assert!(!ptr.is_null());
            AlignedBuf { ptr, layout }
        }
        fn bytes(&self) -> &[u8] {
            unsafe { core::slice::from_raw_parts(self.ptr, self.layout.size()) }
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

    // Parity S155-F2 (no reference test): `keyIterator`/`valueIterator` yield
    // exactly the used slots, in slot order, and nothing for an empty map.
    #[test]
    fn HashMap_key_and_value_iterators() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;

        let cap = 16;
        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        assert!(map.key_iterator().next().is_none());
        assert!(map.value_iterator().next().is_none());

        for i in 0..5u32 {
            map.put_context(i, i * 10, &c).unwrap();
        }
        map.remove_context(2, &c);

        let mut from_entries: Vec<(u32, u32)> = Vec::new();
        let mut it = map.iterator();
        while let Some(kv) = it.next() {
            from_entries.push(unsafe { (*kv.key_ptr, *kv.value_ptr) });
        }

        let mut keys: Vec<u32> = Vec::new();
        let mut key_it = map.key_iterator();
        while let Some(k) = key_it.next() {
            keys.push(unsafe { *k });
        }
        let mut values: Vec<u32> = Vec::new();
        let mut value_it = map.value_iterator();
        while let Some(v) = value_it.next() {
            values.push(unsafe { *v });
        }

        assert_eq!(4, keys.len());
        assert_eq!(from_entries.iter().map(|e| e.0).collect::<Vec<_>>(), keys);
        assert_eq!(from_entries.iter().map(|e| e.1).collect::<Vec<_>>(), values);

        // The value pointers write through to the map.
        let mut value_it = map.value_iterator();
        while let Some(v) = value_it.next() {
            unsafe { *v += 1 };
        }
        assert_eq!(Some(31), map.get_context(3, &c));
    }

    // ADDITIVE (no reference counterpart): pins `AutoContext` to the ORACLE's
    // physical table layout, not merely to logically-correct lookups. The map
    // lives inside the page's backing allocation, so the home slot
    // (`hash & mask`), the metadata fingerprint and hence the slot each key
    // lands in are observable in the raw page bytes that `compress/page.rs`
    // compresses. The expected order below is the slot order ghostty produces
    // for this exact insertion sequence at capacity 8 (seed-0 Wyhash over
    // `asBytes(&key)`); it is NOT the order a differently-seeded/algorithm
    // hasher gives.
    #[test]
    fn HashMap_auto_context_matches_reference_wyhash_layout() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 8;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        // The per-key hashes the oracle's `AutoContext.hash` produces.
        for (key, expected) in [
            (0u32, 0x14b8_3016_dc46_0955u64),
            (1, 0xd145_1a1b_34d5_fb64),
            (2, 0xb52d_3031_bc13_56f4),
            (3, 0x3579_1793_81f3_5bc6),
            (4, 0x63b2_677c_5d80_f97a),
            (8, 0xe850_de3f_7e60_b2fa),
            (16, 0x052a_446e_fa88_3433),
            (24, 0xa226_4b2b_07f9_9c5a),
        ] {
            assert_eq!(expected, c.hash(key), "hash({key})");
        }

        for key in [0u32, 1, 2, 3, 4, 8, 16, 24] {
            map.put_context(key, 1000 + key, &c).unwrap();
        }
        assert_eq!(8, map.count());

        let mut order: Vec<(u32, u32)> = Vec::new();
        let mut it = map.iterator();
        while let Some(kv) = it.next() {
            order.push(unsafe { (*kv.key_ptr, *kv.value_ptr) });
        }
        assert_eq!(
            vec![
                (16, 1016),
                (24, 1024),
                (4, 1004),
                (8, 1008),
                (1, 1001),
                (0, 1000),
                (2, 1002),
                (3, 1003),
            ],
            order,
        );

        // Lookups agree with the oracle too, of course.
        for key in [0u32, 1, 2, 3, 4, 8, 16, 24] {
            assert_eq!(Some(1000 + key), map.get_context(key, &c));
        }
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

        // var prng = std.Random.DefaultPrng.init(0);
        // const random = prng.random();
        //
        // PORT NOTE (parity S155-F5): this was a hand-rolled LCG Fisher-Yates,
        // which is a DIFFERENT permutation sequence from the reference's and so
        // a different input distribution — the test no longer exercised the
        // insert/remove orders upstream exercises. `rustty_synthetic::rand` is
        // the crate's faithful `std.Random` port (Xoshiro256++ with the
        // SplitMix64 seeding, pinned bit-for-bit against Zig's own std tests);
        // `shuffle` lands there next to the rest of the surface. The first two
        // permutations are pinned below against ground truth generated by
        // running zig 0.16 itself.
        let prng = rustty_synthetic::rand::DefaultPrng::init(0);
        let random = prng.random();

        while i < iterations {
            // random.shuffle(u32, keys.items);
            random.shuffle(&mut keys);
            if i == size {
                // Ground truth from `std.Random.DefaultPrng.init(0)` +
                // `random.shuffle(u32, keys)` under zig 0.16.
                assert_eq!(
                    &[
                        10u32, 12, 1, 3, 17, 5, 28, 6, 15, 0, 16, 2, 14, 11, 8, 21, 26, 18, 22, 24,
                        23, 4, 13, 25, 27, 19, 9, 30, 31, 29, 20, 7
                    ][..],
                    &keys[..],
                    "the shuffle must reproduce std.Random.DefaultPrng's permutation"
                );
            }
            if i == size + 1 {
                assert_eq!(
                    &[
                        14u32, 2, 30, 31, 0, 22, 16, 27, 10, 4, 26, 5, 11, 15, 18, 12, 21, 23, 25,
                        9, 24, 29, 28, 13, 20, 6, 3, 8, 19, 1, 7, 17
                    ][..],
                    &keys[..],
                    "the shuffle must reproduce std.Random.DefaultPrng's permutation"
                );
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
            // try expectCanonical(&map, Context{});
            map.expect_canonical(&c);
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
            // try expectCanonical(&map, Context{});
            map.expect_canonical(&c);
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
        // try expectCanonical(&map, AutoContext(u32){});
        map.expect_canonical(&c);
    }

    // `test "HashMap random operations against an oracle"` (fedd42e8d)
    //
    // PORT NOTE (parity S155-F5): the reference's oracle is
    // `std.AutoHashMapUnmanaged` and its PRNG is `std.Random.DefaultPrng`
    // (Xoshiro256++) seeded 0xdeadbeef. The port uses
    // `std::collections::HashMap` as the oracle — fine, it is only a known-good
    // reference map — but its PRNG was an inline SPLITMIX64 (the note here used
    // to claim "an inline xoshiro256++ with the same seeding discipline", which
    // was simply false: splitmix64 is Xoshiro's SEEDER, not its generator), and
    // it derived draws with `%` instead of `uintLessThan`/`int`. A different
    // generator and a different derivation are a different input distribution,
    // i.e. different coverage of the map under test. It now drives
    // `rustty_synthetic::rand`, the crate's faithful `std.Random` port, through
    // the reference's own `uintLessThan`/`int` calls; the first eight draws are
    // pinned below against ground truth generated by running zig 0.16 itself.
    #[test]
    fn HashMap_random_operations_against_an_oracle() {
        type Map = AutoHashMapUnmanaged<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap: u32 = 64;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<u32>();

        let mut oracle: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();

        // var prng = std.Random.DefaultPrng.init(0xdeadbeef);
        // const random = prng.random();
        let prng = rustty_synthetic::rand::DefaultPrng::init(0xdeadbeef);
        let random = prng.random();

        // Ground truth for the first eight iterations, generated by running
        // `std.Random.DefaultPrng.init(0xdeadbeef)` under zig 0.16 with exactly
        // the draw sequence below: (key, op) and, when op < 2, the value.
        let expected_draws: [(u32, u8, Option<u32>); 8] = [
            (95, 3, None),
            (73, 2, None),
            (7, 0, Some(2_525_345_492)),
            (44, 1, Some(3_199_506_276)),
            (65, 1, Some(1_110_749_104)),
            (26, 1, Some(4_217_228_002)),
            (49, 2, None),
            (34, 0, Some(3_001_515_670)),
        ];

        // A small key space forces frequent hits, misses, and re-insertions
        // at every load factor from empty to completely full.
        let key_space = cap + cap / 2;
        for iteration in 0..20_000 {
            // const key = random.uintLessThan(u32, key_space);
            let key = random.uint_less_than::<u32>(key_space);
            // switch (random.uintLessThan(u8, 4))
            let op = random.uint_less_than::<u8>(4);
            if iteration < expected_draws.len() {
                assert_eq!(
                    (expected_draws[iteration].0, expected_draws[iteration].1),
                    (key, op),
                    "draw {iteration} diverged from std.Random.DefaultPrng"
                );
            }
            match op {
                0 | 1 => {
                    // const value = random.int(u32);
                    let value = random.int::<u32>();
                    if iteration < expected_draws.len() {
                        assert_eq!(expected_draws[iteration].2, Some(value));
                    }
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
        // try expectCanonical(&map, AutoContext(u32){});
        map.expect_canonical(&c);
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
        // The `size_of::<K>() == 0` branch of `remove_by_ptr_context` is
        // therefore NOT covered by this test; `HashMap_removeByPtr_true_zero
        // _sized_key` below covers it (and pins the deliberate divergence from
        // the reference's `idx = 0` fallback documented there).
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

    // ADDITIVE (no reference counterpart — the reference's same-named test uses
    // a non-ZST key, see the note above): exercises `remove_by_ptr_context` with
    // a TRUE zero-sized key, the case the reference's `idx = 0` fallback gets
    // wrong. `hash(()) = Wyhash.hash(0, "") = 0x0409638ee2bde459`, so at
    // capacity 16 the single entry lives at slot 9; removing slot 0 would
    // decrement `size` while leaving slot 9 used, so the key would afterwards be
    // neither findable nor re-insertable.
    #[test]
    fn HashMap_removeByPtr_true_zero_sized_key() {
        type Map = AutoHashMapUnmanaged<(), u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let cap = 16;

        let layout = Map::layout_for_capacity(cap);
        let buf = AlignedBuf::new(layout.total_size, Map::base_align());
        let mut map = unsafe { Map::init(buf.offset_buf(), layout) };
        let c = ctx::<()>();

        // The home slot is NOT 0 — this is what makes the reference fallback
        // wrong rather than merely lucky.
        assert_eq!(0x0409_638e_e2bd_e459, c.hash(()));
        assert_eq!(9, (c.hash(()) & (map.capacity() as u64 - 1)) as usize);

        map.put_context((), 123, &c).unwrap();
        assert_eq!(1, map.count());

        let key_ptr = map.get_key_ptr_context((), &c).unwrap();
        unsafe { map.remove_by_ptr_context(key_ptr, &c) };

        assert_eq!(0, map.count());
        assert_eq!(None, map.get_context((), &c));

        // The slot is genuinely free again, not a stale used entry.
        map.put_context((), 456, &c).unwrap();
        assert_eq!(1, map.count());
        assert_eq!(Some(456), map.get_context((), &c));
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

    // ADDITIVE (P7-W1 T2, `c0a4f80d8` + `6112935a2`; the reference adds no
    // test): `initAssumeZeroed` writes nothing to the backing memory, and the
    // zero-filled buffer is already a valid empty map that works as one.
    #[test]
    fn OffsetHashMap_initAssumeZeroed_writes_nothing() {
        type OffsetMap = AutoOffsetHashMap<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let layout = OffsetMap::layout(16);
        let buf = AlignedBuf::new_zeroed(layout.total_size, OffsetMap::base_align());

        let offset_map = unsafe { OffsetMap::init_assume_zeroed(buf.offset_buf(), layout) };
        assert!(buf.bytes().iter().all(|&b| b == 0));
        assert_eq!(layout.capacity, offset_map.capacity);

        let mut map = unsafe { offset_map.map(buf.base() as *const u8) };
        let c = ctx::<u32>();
        assert_eq!(0, map.count());
        assert_eq!(layout.capacity, map.capacity());
        assert!(map.key_iterator().next().is_none());
        assert!(buf.bytes().iter().all(|&b| b == 0));

        for i in 0..5u32 {
            map.put_context(i, i * 10, &c).unwrap();
        }
        assert_eq!(5, map.count());
        for i in 0..5u32 {
            assert_eq!(i * 10, map.get_context(i, &c).unwrap());
        }
        assert!(map.remove_context(3, &c));
        assert_eq!(4, map.count());
        map.expect_canonical(&c);
    }

    // ADDITIVE (P7-W1 T2): `init` still resets a dirty buffer, now as
    // `initAssumeZeroed` + `clearRetainingCapacity`.
    #[test]
    fn OffsetHashMap_init_clears_dirty_memory() {
        type OffsetMap = AutoOffsetHashMap<u32, u32, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let layout = OffsetMap::layout(16);
        let buf = AlignedBuf::new(layout.total_size, OffsetMap::base_align());
        unsafe { core::ptr::write_bytes(buf.base(), 0xAA, layout.total_size) };

        let offset_map = unsafe { OffsetMap::init(buf.offset_buf(), layout) };
        let mut map = unsafe { offset_map.map(buf.base() as *const u8) };
        let c = ctx::<u32>();
        assert_eq!(0, map.count());
        assert!(map.key_iterator().next().is_none());
        // The header and the metadata are zero; the entry arrays are not
        // touched (their bytes are never read for a free slot).
        let meta_end = layout.metadata_start + layout.capacity as usize;
        assert!(buf.bytes()[..meta_end].iter().all(|&b| b == 0));

        map.put_context(7, 70, &c).unwrap();
        assert_eq!(Some(70), map.get_context(7, &c));
        assert_eq!(1, map.count());
    }

    // ADDITIVE (P7-W1 T2, `6112935a2`): the header is only the size counter,
    // every layout offset is from the start of the buffer, and the map's
    // cached key/value pointers land on those offsets.
    #[test]
    fn HashMap_layout_offsets_are_from_buffer_start() {
        type Map = AutoHashMapUnmanaged<u32, u64, DEFAULT_MAX_LOAD_PERCENTAGE>;
        let layout = Map::layout_for_capacity(16);
        assert_eq!(core::mem::size_of::<Size>(), core::mem::size_of::<Header>());
        assert_eq!(4, layout.metadata_start);
        // meta_end = 4 + 16 = 20, u32 keys aligned to 4.
        assert_eq!(20, layout.keys_start);
        // keys_end = 20 + 64 = 84, u64 values aligned to 8.
        assert_eq!(88, layout.vals_start);
        assert_eq!(88 + 16 * 8, layout.total_size);

        let buf = AlignedBuf::new_zeroed(layout.total_size, Map::base_align());
        let map = unsafe { Map::init(buf.offset_buf(), layout) };
        let base = buf.base() as usize;
        assert_eq!(base + layout.metadata_start, map.metadata as usize);
        assert_eq!(base + layout.keys_start, map.keys as usize);
        assert_eq!(base + layout.vals_start, map.values as usize);
        assert_eq!(16, map.cap);
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
