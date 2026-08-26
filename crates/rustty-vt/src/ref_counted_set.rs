//! rustty-vt ref_counted_set
//!
//! PORTS: ghostty src/terminal/ref_counted_set.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const size = @import("size.zig");
//   const Offset = size.Offset;
//   const OffsetBuf = size.OffsetBuf;
// The offset types are the hoisted size types from rustty-render-types (A1); the
// `inlineAssert` quirk lives in rustty-util.
//
// PORT NOTE: the reference is a comptime
//   RefCountedSet(comptime T, comptime IdT, comptime RefCountInt, comptime ContextT)
// In Rust this is a struct generic over `T`, the id integer type `Id`, the
// reference-count integer type `RefCount`, and a `Context: RefCountedSetContext`
// trait that supplies the duck-typed `hash`/`eql`/(optional)`deleted` behaviors.
// To keep the integer arithmetic generic without pulling in a numeric-traits
// crate, the `Id` / `RefCount` types are constrained by the small `IdInt` /
// `RefCountIntTrait` helper traits defined below, which expose exactly the
// operations the reference uses (`maxInt`, wrapping add, `&`, `+= 1`, etc.).
//
// PORT NOTE: the pre-range reference file had NO `test {}` blocks (the Phase-0
// inventory records 0 tests for ref_counted_set.zig); the type is exercised
// transitively by the page.zig sub-batch's tests. Upstream b61fd5fbb ("add
// iterator to ref counted set") added the first one — `test "iterator visits
// live entries in ID order"` — carried in the `tests` module below.
//
// PORT NOTE: the offset/pointer machinery is reproduced with raw pointers and is
// `unsafe`, exactly as the reference relies on raw pointer arithmetic against the
// offset base. Soundness invariants (valid backing buffer of
// `Layout.init(cap).total_size` bytes, `base_align`-aligned) are the caller's
// responsibility, same as the reference.

// PARITY: the `default()` constructors mirror the reference struct-literal
// defaults (`.{}` / `Item{}` / `Metadata{}`) used to zero-initialize item/table
// slots; they are inherent (not `std::Default`) because the surrounding generic
// bounds differ per call site, so `should_implement_trait` is allowed. Each
// `unsafe fn` documents its base-pointer invariant inline (`# Safety: base must
// ...`); the section-header form clippy wants would be redundant, so
// `missing_safety_doc` is allowed module-wide.
#![allow(clippy::should_implement_trait)]
#![allow(clippy::missing_safety_doc)]

use core::marker::PhantomData;

use rustty_render_types::term_size::{IntFromBase, Offset, OffsetBuf};
use rustty_util::quirks::inlineAssert as assert;

/// Integer trait for the `Id` type parameter. Exposes exactly the operations the
/// reference performs on `Id` values (which are unsigned table/item indices).
pub trait IdInt: Copy + PartialEq + PartialOrd + Eq + Ord {
    const MAX: Self;
    const ZERO: Self;
    const ONE: Self;
    fn to_usize(self) -> usize;
    fn from_usize(v: usize) -> Self;
    /// `a +% b` wrapping add.
    fn wrapping_add(self, other: Self) -> Self;
    /// `a & b` bitwise and.
    fn bit_and(self, other: Self) -> Self;
    fn add(self, other: Self) -> Self;
    fn sub(self, other: Self) -> Self;
}

macro_rules! impl_id_int {
    ($($t:ty),*) => {$(
        impl IdInt for $t {
            const MAX: Self = <$t>::MAX;
            const ZERO: Self = 0;
            const ONE: Self = 1;
            #[inline] fn to_usize(self) -> usize { self as usize }
            #[inline] fn from_usize(v: usize) -> Self { v as $t }
            #[inline] fn wrapping_add(self, other: Self) -> Self { <$t>::wrapping_add(self, other) }
            #[inline] fn bit_and(self, other: Self) -> Self { self & other }
            #[inline] fn add(self, other: Self) -> Self { self + other }
            #[inline] fn sub(self, other: Self) -> Self { self - other }
        }
    )*};
}
impl_id_int!(u8, u16, u32, u64, usize);

/// Integer trait for the `RefCountInt` type parameter.
pub trait RefCountIntTrait: Copy + PartialEq + PartialOrd + Eq + Ord {
    const ZERO: Self;
    const ONE: Self;
    fn add(self, other: Self) -> Self;
    fn sub(self, other: Self) -> Self;
    /// `@intCast` from a usize the caller guarantees fits.
    fn from_usize(v: usize) -> Self;
}

macro_rules! impl_refcount_int {
    ($($t:ty),*) => {$(
        impl RefCountIntTrait for $t {
            const ZERO: Self = 0;
            const ONE: Self = 1;
            #[inline] fn add(self, other: Self) -> Self { self + other }
            #[inline] fn sub(self, other: Self) -> Self { self - other }
            #[inline] fn from_usize(v: usize) -> Self { v as $t }
        }
    )*};
}
impl_refcount_int!(u8, u16, u32, u64, usize);

/// The context trait supplying the duck-typed behaviors of the reference
/// `Context` struct:
///
///   - `fn hash(*Context, T) u64`    - Return a hash for an item.
///
///   - `fn eql(*Context, T, T) bool` - Check two items for equality.
///     The first of the two items passed in is guaranteed to be from
///     a value passed in to an `add` or `lookup` function, the second
///     is guaranteed to be a value already resident in the set.
///
///   - `fn deleted(*Context, T) void` - [OPTIONAL] Deletion callback.
///     If present, called whenever an item is finally deleted.
///     Useful if the item has memory that needs to be freed.
//
// PORT NOTE: the reference gates the deletion callback on
// `comptime @hasDecl(Context, "deleted")`. Rust has no per-type optional method,
// so `deleted` is a trait method with a default no-op body; calling the no-op
// default is behaviorally identical to the reference's "decl absent → no call".
pub trait RefCountedSetContext<T> {
    fn hash(&self, value: T) -> u64;
    fn eql(&self, a: T, b: T) -> bool;
    fn deleted(&self, _value: T) {}
}

/// Set item
pub struct Item<T, Id: IdInt, RefCount: RefCountIntTrait> {
    /// The value this item represents.
    // PORT NOTE: the reference field defaults to `undefined`; we use a default
    // value via the `Default` bound when initializing item arrays. The metadata
    // governs whether the value is live.
    pub value: T,

    /// Metadata for this item.
    pub meta: Metadata<Id, RefCount>,
}

/// `Item.Metadata`
pub struct Metadata<Id: IdInt, RefCount: RefCountIntTrait> {
    /// The bucket in the hash table where this item
    /// is referenced.
    pub bucket: Id,

    /// The length of the probe sequence between this
    /// item's starting bucket and the bucket it's in,
    /// used for Robin Hood hashing.
    pub psl: Id,

    /// The reference count for this item.
    pub ref_count: RefCount,
}

impl<Id: IdInt, RefCount: RefCountIntTrait> Clone for Metadata<Id, RefCount> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Id: IdInt, RefCount: RefCountIntTrait> Copy for Metadata<Id, RefCount> {}

impl<Id: IdInt, RefCount: RefCountIntTrait> Metadata<Id, RefCount> {
    /// Default metadata: `bucket = maxInt(Id), psl = 0, ref = 0`.
    pub fn default() -> Self {
        Metadata {
            bucket: Id::MAX,
            psl: Id::ZERO,
            ref_count: RefCount::ZERO,
        }
    }
}

impl<T: Copy + Default, Id: IdInt, RefCount: RefCountIntTrait> Item<T, Id, RefCount> {
    /// Default item: `value = undefined, meta = .{}` — modeled with `T::default`.
    pub fn default() -> Self {
        Item {
            value: T::default(),
            meta: Metadata::default(),
        }
    }
}

impl<T: Copy, Id: IdInt, RefCount: RefCountIntTrait> Clone for Item<T, Id, RefCount> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Copy, Id: IdInt, RefCount: RefCountIntTrait> Copy for Item<T, Id, RefCount> {}

/// `pub const Layout = struct { ... }`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout<Id: IdInt> {
    pub cap: usize,
    pub table_cap: usize,
    pub table_mask: Id,
    pub table_start: usize,
    pub items_start: usize,
    pub total_size: usize,
}

/// Possible errors for `add` and `addWithId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddError {
    /// There is not enough memory to add a new item.
    /// Remove items or grow and reinitialize.
    OutOfMemory,

    /// The set needs to be rehashed, as there are many dead
    /// items with lower IDs which are inaccessible for reuse.
    NeedsRehash,
}

/// A reference counted set.
///
/// This set is created with some capacity in mind. You can determine
/// the exact memory requirement of a given capacity by calling `layout`
/// and checking the total size.
///
/// When the set exceeds capacity, an `OutOfMemory` or `NeedsRehash` error
/// is returned from any memory-using methods. The caller is responsible
/// for determining a path forward.
///
/// This set is reference counted. Each item in the set has an associated
/// reference count. The caller is responsible for calling release for an
/// item when it is no longer being used. Items with 0 references will be
/// kept until another item is written to their bucket. This allows items
/// to be resurrected if they are re-added before they get overwritten.
///
/// The backing data structure of this set is an open addressed hash table
/// with linear probing and Robin Hood hashing, and a flat array of items.
///
/// The table maps values to item IDs, which are indices in the item array
/// which contain the item's value and its reference count. Item IDs can be
/// used to efficiently access an item and update its reference count after
/// it has been added to the table, to avoid having to use the hash map to
/// look the value back up.
///
/// ID 0 is reserved and will never be assigned.
///
/// Parameters:
///
/// `Context`
///   A type containing methods to define behaviors.
///
///   - `fn hash(*Context, T) u64`    - Return a hash for an item.
///
///   - `fn eql(*Context, T, T) bool` - Check two items for equality.
///     The first of the two items passed in is guaranteed to be from
///     a value passed in to an `add` or `lookup` function, the second
///     is guaranteed to be a value already resident in the set.
///
///   - `fn deleted(*Context, T) void` - [OPTIONAL] Deletion callback.
///     If present, called whenever an item is finally deleted.
///     Useful if the item has memory that needs to be freed.
///
// PORT NOTE: the reference re-exports the id/context types on the set so callers
// can name them:
//   // Re-export these types so they can be referenced by the caller.
//   pub const Id = IdT;
//   pub const Context = ContextT;
// In Rust `Id` and `Context` are generic parameters of `RefCountedSet`, directly
// nameable by callers, so no aliases are needed; the comment is preserved here.
pub struct RefCountedSet<T, Id, RefCount, Context>
where
    T: Copy + Default,
    Id: IdInt,
    RefCount: RefCountIntTrait,
    Context: RefCountedSetContext<T>,
{
    /// A hash table of item indices
    pub table: Offset<Id>,

    /// By keeping track of the max probe sequence length
    /// we can bail out early when looking up values that
    /// aren't present.
    pub max_psl: Id,

    /// We keep track of how many items have a PSL of any
    /// given length, so that we can shrink max_psl when
    /// we delete items.
    ///
    /// A probe sequence of length 32 or more is astronomically
    /// unlikely. Roughly a (1/table_cap)^32 -- with any normal
    /// table capacity that is so unlikely that it's not worth
    /// handling.
    ///
    /// However, that assumes a uniform hash function, which
    /// is not guaranteed and can be subverted with a crafted
    /// input. We handle this gracefully by returning an error
    /// anywhere where we're about to insert if there's any
    /// item with a PSL in the last slot of the stats array.
    pub psl_stats: [Id; 32],

    /// The backing store of items
    pub items: Offset<Item<T, Id, RefCount>>,

    /// The number of living items currently stored in the set.
    pub living: usize,

    /// The next index to store an item at.
    /// Id 0 is reserved for unused items.
    pub next_id: Id,

    pub layout: Layout<Id>,

    /// An instance of the context structure.
    pub context: Context,

    _marker: PhantomData<RefCount>,
}

// PORT NOTE: the reference set is a plain struct of `Offset`s, ints, a fixed
// array, a `Layout`, and the context, so `var result = self.*` copies it
// shallowly. We mirror that with `Clone` (requiring `Context: Clone`); needed by
// `Page` clone, which copies the whole struct (`var result = self.*`). Offsets
// are relative to the (newly memcpy'd) backing memory, so a shallow copy is
// exactly correct.
impl<T, Id, RefCount, Context> Clone for RefCountedSet<T, Id, RefCount, Context>
where
    T: Copy + Default,
    Id: IdInt,
    RefCount: RefCountIntTrait,
    Context: RefCountedSetContext<T> + Clone,
{
    fn clone(&self) -> Self {
        RefCountedSet {
            table: self.table,
            max_psl: self.max_psl,
            psl_stats: self.psl_stats,
            items: self.items,
            living: self.living,
            next_id: self.next_id,
            layout: self.layout,
            context: self.context.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Id: IdInt> Layout<Id> {
    /// This is the max load until the set returns OutOfMemory and
    /// requires more capacity.
    ///
    /// Experimentally, this load factor works quite well.
    pub const LOAD_FACTOR: f64 = 0.8125;

    /// Returns the memory layout for the given base offset and
    /// desired capacity. The layout can be used by the caller to
    /// determine how much memory to allocate, and the layout must
    /// be used to initialize the set so that the set knows all
    /// the offsets for the various buffers.
    ///
    /// The capacity passed for cap will be used for the hash table,
    /// which has a load factor of `0.8125` (13/16), so the number of
    /// items which can actually be stored in the set will be smaller.
    ///
    /// The laid out capacity will be at least `cap`, but may be higher,
    /// since it is rounded up to the next power of 2 for efficiency.
    ///
    /// The returned layout `cap` property will be 1 more than the number
    /// of items that the set can actually store, since ID 0 is reserved.
    pub fn init<T, RefCount>(cap: usize) -> Layout<Id>
    where
        T: Copy + Default,
        RefCount: RefCountIntTrait,
    {
        assert(cap <= (Id::MAX.to_usize()) + 1);

        // Zero-cap set is valid, return special case
        if cap == 0 {
            return Layout {
                cap: 0,
                table_cap: 0,
                table_mask: Id::ZERO,
                table_start: 0,
                items_start: 0,
                total_size: 0,
            };
        }

        // const table_cap: usize = std.math.ceilPowerOfTwoAssert(usize, cap);
        let table_cap: usize = cap.next_power_of_two();
        // const items_cap: usize = @intFromFloat(load_factor * @as(f64, @floatFromInt(table_cap)));
        let items_cap: usize = (Self::LOAD_FACTOR * (table_cap as f64)) as usize;

        // const table_mask: Id = @intCast((@as(usize, 1) << std.math.log2_int(usize, table_cap)) - 1);
        let table_mask: Id = Id::from_usize((1usize << log2_int(table_cap)) - 1);

        let table_start = 0;
        let table_end = table_start + table_cap * core::mem::size_of::<Id>();

        // const items_start = std.mem.alignForward(usize, table_end, @alignOf(Item));
        let items_start = align_forward(table_end, core::mem::align_of::<Item<T, Id, RefCount>>());
        let items_end = items_start + items_cap * core::mem::size_of::<Item<T, Id, RefCount>>();

        let total_size = items_end;

        Layout {
            cap: items_cap,
            table_cap,
            table_mask,
            table_start,
            items_start,
            total_size,
        }
    }
}

impl<T, Id, RefCount, Context> RefCountedSet<T, Id, RefCount, Context>
where
    T: Copy + Default,
    Id: IdInt,
    RefCount: RefCountIntTrait,
    Context: RefCountedSetContext<T>,
{
    /// `pub const base_align: std.mem.Alignment = .fromByteUnits(@max(...))`
    pub fn base_align() -> usize {
        // @max(@alignOf(Context), @alignOf(Layout), @alignOf(Item), @alignOf(Id))
        core::mem::align_of::<Context>()
            .max(core::mem::align_of::<Layout<Id>>())
            .max(core::mem::align_of::<Item<T, Id, RefCount>>())
            .max(core::mem::align_of::<Id>())
    }

    /// This is the max load until the set returns OutOfMemory and
    /// requires more capacity.
    ///
    /// Experimentally, this load factor works quite well.
    pub const LOAD_FACTOR: f64 = 0.8125;

    /// Returns the minimum capacity needed to store `n` items,
    /// accounting for the load factor and the reserved ID 0.
    pub fn capacity_for_count(n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        // +1 because ID 0 is reserved, so we need at least n+1 slots.
        // return @intFromFloat(@ceil(@as(f64, @floatFromInt(n + 1)) / load_factor));
        (((n + 1) as f64) / Self::LOAD_FACTOR).ceil() as usize
    }

    /// # Safety
    ///
    /// `base` must point at a valid `base_align`-aligned allocation of at least
    /// `l.total_size` bytes.
    pub unsafe fn init(base: OffsetBuf, l: Layout<Id>, context: Context) -> Self {
        let table: Offset<Id> = base.member(l.table_start);
        let items: Offset<Item<T, Id, RefCount>> = base.member(l.items_start);

        // @memset(table.ptr(base)[0..l.table_cap], 0);
        unsafe {
            core::slice::from_raw_parts_mut(table.ptr(base), l.table_cap).fill(Id::ZERO);
        }
        // @memset(items.ptr(base)[0..l.cap], .{});
        //
        // PORT NOTE (WS7 Phase E, uninit-in-page-mapping fix): this is
        // DELIBERATELY not the obvious `slice.fill(Item::default())`. A typed
        // Rust write of `Item<T, ..>` initializes only the bytes covered by the
        // struct's fields and the *active* payload of each enum inside them;
        // every other byte of the destination — struct padding, and the unused
        // payload bytes of a `style::Color::None` / `Color::Palette` — is left
        // UNINITIALIZED. Because this array lives inside the terminal page's
        // backing mapping, and `compress::CompressedPage::init` compresses that
        // mapping as one `&[u8]` (exactly as the oracle's `compress/Page.zig`
        // compresses `page.memory`), a typed fill here makes the compressor read
        // uninitialized memory: Miri aborts with "constructing invalid value of
        // type `[u8; 8]`: encountered uninitialized memory" inside
        // `lz4::compress`. See `.port/phase-2/MIRI_EXCEPTIONS.md`.
        //
        // Instead we zero every byte of the item array (so no byte of the
        // mapping is ever uninitialized) and then set the metadata FIELD BY
        // FIELD. A field-wise write only touches that field's bytes, so it
        // cannot re-poison the surrounding ones.
        //
        // This is faithful, not a weakening: the reference's `Item.value` field
        // has no default — the reference comment on `value` is that it is
        // `undefined` — so the reference's `.{}` leaves `value` unspecified and
        // reads of a dead item's value are already forbidden. Storing zeroes
        // there is strictly more defined than the reference. `meta` gets exactly
        // the reference's defaults (`bucket = maxInt(Id)`, `psl = 0`, `ref = 0`).
        // The all-zero `value` state is also already reachable today: the page
        // mapping arrives zeroed from mmap and `Page::reinit` re-zeroes it.
        unsafe {
            let items_ptr = items.ptr(base);
            core::ptr::write_bytes(
                items_ptr as *mut u8,
                0,
                l.cap * core::mem::size_of::<Item<T, Id, RefCount>>(),
            );
            let default_meta = Metadata::<Id, RefCount>::default();
            for i in 0..l.cap {
                let item = items_ptr.add(i);
                core::ptr::write(
                    core::ptr::addr_of_mut!((*item).meta.bucket),
                    default_meta.bucket,
                );
                core::ptr::write(core::ptr::addr_of_mut!((*item).meta.psl), default_meta.psl);
                core::ptr::write(
                    core::ptr::addr_of_mut!((*item).meta.ref_count),
                    default_meta.ref_count,
                );
            }
        }

        RefCountedSet {
            table,
            max_psl: Id::ZERO,
            psl_stats: [Id::ZERO; 32],
            items,
            living: 0,
            next_id: Id::ONE,
            layout: l,
            context,
            _marker: PhantomData,
        }
    }

    /// Add an item to the set if not present and increment its ref count.
    ///
    /// Returns the item's ID.
    ///
    /// If the set has no more room, then an OutOfMemory error is returned.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    // PERF(port) (reshape): the internal bodies (`add_context`,
    // `lookup_context`, `upsert`, `insert`) and the `_ptr` entry points take
    // `value: *const T` instead of `T`. Zig passes big parameters by const
    // reference (its `addWithId(value: T)` compiles to a pointer with no
    // defensive copy); the Rust by-value ABI for a >16-byte `T` instead
    // marshals a fresh stack copy at EVERY call site (measured: the 28-byte
    // `Style` q-register store/reload per `add_with_id` call was part of the
    // screen-clone residual floor, PERF_LOG round 3). The public by-value
    // surface is preserved as `#[inline]` shims that pass `&raw const value`,
    // so the observable API and behavior are byte-identical. A raw pointer
    // (not `&T`) carries exactly Zig's parameter aliasing semantics: no
    // `noalias`/protector is asserted for the pointee. LIFETIMES: the pointee
    // is only ever read, never written, for the duration of each call; every
    // `*value` read below is a plain copy-out.
    #[inline]
    pub unsafe fn add<B: IntFromBase + Copy>(&mut self, base: B, value: T) -> Result<Id, AddError> {
        // PORT NOTE: the reference calls `addContext(base, value, self.context)`;
        // since `self.context` would alias `self` here, we route through a private
        // helper that takes the context by reference, matching semantics.
        unsafe { self.add_context(base, &raw const value) }
    }
    unsafe fn add_context<B: IntFromBase + Copy>(
        &mut self,
        base: B,
        value: *const T,
    ) -> Result<Id, AddError> {
        let items = unsafe { self.items.ptr(base) };

        // Trim dead items from the end of the list.
        while self.next_id > Id::ONE
            && unsafe {
                (*items.add(self.next_id.sub(Id::ONE).to_usize()))
                    .meta
                    .ref_count
            } == RefCount::ZERO
        {
            self.next_id = self.next_id.sub(Id::ONE);
            unsafe { self.delete_item(base, self.next_id) };
        }

        // If the item already exists, return it.
        if let Some(id) = unsafe { self.lookup_context(base, value) } {
            // Notify the context that the value is "deleted" because
            // we're reusing the existing value in the set. This allows
            // callers to clean up any resources associated with the value.
            self.context.deleted(unsafe { *value });

            unsafe {
                (*items.add(id.to_usize())).meta.ref_count = (*items.add(id.to_usize()))
                    .meta
                    .ref_count
                    .add(RefCount::ONE);
            }
            return Ok(id);
        }

        // While it should be statistically impossible to exceed the
        // bounds of `psl_stats`, the hash function is not perfect and
        // in such a case we want to remain stable. If we're about to
        // insert an item and there's something with a PSL of `len - 1`,
        // we may end up with a PSL of `len` which would exceed the bounds.
        // In such a case, we claim to be out of memory.
        if self.psl_stats[self.psl_stats.len() - 1] > Id::ZERO {
            return Err(AddError::OutOfMemory);
        }

        // If the item doesn't exist, we need an available ID.
        if self.next_id.to_usize() >= self.layout.cap {
            // Arbitrarily chosen, threshold for rehashing.
            // If less than 90% of currently allocated IDs
            // correspond to living items, we should rehash.
            // Otherwise, claim we're out of memory because
            // we assume that we'll end up running out of
            // memory or rehashing again very soon if we
            // rehash with only a few IDs left.
            let rehash_threshold = 0.9;
            // if (self.living < @as(Id, @intFromFloat(@as(f64, @floatFromInt(self.layout.cap)) * rehash_threshold)))
            if self.living < (((self.layout.cap as f64) * rehash_threshold) as usize) {
                return Err(AddError::NeedsRehash);
            }

            // If we don't have at least 10% dead items then
            // we claim we're out of memory.
            return Err(AddError::OutOfMemory);
        }

        let id = unsafe { self.insert(base, value, self.next_id) };
        unsafe {
            (*items.add(id.to_usize())).meta.ref_count = (*items.add(id.to_usize()))
                .meta
                .ref_count
                .add(RefCount::ONE);
            assert((*items.add(id.to_usize())).meta.ref_count == RefCount::ONE);
        }
        self.living += 1;

        // Its possible insert returns a different ID by reusing a
        // dead item so we only need to update next id if we used it.
        if id == self.next_id {
            self.next_id = self.next_id.add(Id::ONE);
        }

        Ok(id)
    }

    /// Add an item to the set if not present and increment its
    /// ref count. If possible, use the provided ID.
    ///
    /// Returns the item's ID, or null if the provided ID was used.
    ///
    /// If the set has no more room, then an OutOfMemory error is returned.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    #[inline]
    pub unsafe fn add_with_id<B: IntFromBase + Copy>(
        &mut self,
        base: B,
        value: T,
        id: Id,
    ) -> Result<Option<Id>, AddError> {
        unsafe { self.add_with_id_ptr(base, &raw const value, id) }
    }

    /// `add_with_id` with the value passed by const pointer (see the
    /// PERF(port) reshape note above `add`): identical body, no per-call
    /// marshal of `T`. Hot callers that already hold a stable pointer to the
    /// value (the page clone chain) use this directly.
    ///
    /// # Safety: `base` must be the true base of the backing buffer; `value`
    /// must be valid for reads for the duration of the call (it is never
    /// written through).
    pub(crate) unsafe fn add_with_id_ptr<B: IntFromBase + Copy>(
        &mut self,
        base: B,
        value: *const T,
        id: Id,
    ) -> Result<Option<Id>, AddError> {
        let items = unsafe { self.items.ptr(base) };

        assert(id > Id::ZERO);

        if id < self.next_id {
            if unsafe { (*items.add(id.to_usize())).meta.ref_count } == RefCount::ZERO {
                // See comment in `addContext` for details.
                if self.psl_stats[self.psl_stats.len() - 1] > Id::ZERO {
                    return Err(AddError::OutOfMemory);
                }

                unsafe { self.delete_item(base, id) };

                let added_id = unsafe { self.upsert(base, value, id) };

                unsafe {
                    (*items.add(added_id.to_usize())).meta.ref_count = (*items
                        .add(added_id.to_usize()))
                    .meta
                    .ref_count
                    .add(RefCount::ONE);
                }

                self.living += 1;

                return Ok(if added_id == id { None } else { Some(added_id) });
            } else if self.context.eql(unsafe { *value }, unsafe {
                (*items.add(id.to_usize())).value
            }) {
                // Notify the context that the value is "deleted" because
                // we're reusing the existing value in the set. This allows
                // callers to clean up any resources associated with the value.
                self.context.deleted(unsafe { *value });

                unsafe {
                    (*items.add(id.to_usize())).meta.ref_count = (*items.add(id.to_usize()))
                        .meta
                        .ref_count
                        .add(RefCount::ONE);
                }

                return Ok(None);
            }
        }

        // return try self.addContext(base, value, ctx); — `Id` coerces to `?Id`.
        unsafe { self.add_context(base, value) }.map(Some)
    }

    /// True iff `add_with_id(base, *value, id)` would take its pure
    /// "existing live equal item" fast path: `id` is in range, the item at
    /// `id` is alive, and it compares equal to `*value`. On that path the
    /// call's ONLY effects are `context.deleted(*value)` and a +1 on the
    /// item's ref count, and it returns `Ok(None)` — and since none of those
    /// change this predicate, it stays true for any number of repeated
    /// identical calls.
    ///
    /// Callers use this to batch a run of identical `add_with_id` calls into
    /// a single `use_multiple` (PERF(port) (reshape), see the clone chain in
    /// `page_impl.rs`). That is only behavior-preserving when
    /// `Context::deleted` is a no-op (true for the style set context, whose
    /// `deleted` is the trait default) — a context with a real `deleted`
    /// (e.g. the hyperlink set) would observe the skipped callbacks.
    ///
    /// # Safety: `base` must be the true base of the backing buffer; `value`
    /// must be valid for reads.
    pub(crate) unsafe fn add_with_id_is_pure_ref_bump<B: IntFromBase + Copy>(
        &self,
        base: B,
        value: *const T,
        id: Id,
    ) -> bool {
        if !(id > Id::ZERO && id < self.next_id) {
            return false;
        }
        let items = unsafe { self.items.ptr(base) };
        let item = unsafe { items.add(id.to_usize()) };
        let live = unsafe { (*item).meta.ref_count } > RefCount::ZERO;
        live && self
            .context
            .eql(unsafe { *value }, unsafe { (*item).value })
    }

    /// Increment an item's reference count by 1.
    ///
    /// Asserts that the item's reference count is greater than 0.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    pub unsafe fn use_id<B: IntFromBase + Copy>(&self, base: B, id: Id) {
        assert(id > Id::ZERO);
        assert(id.to_usize() < self.layout.cap);

        let items = unsafe { self.items.ptr(base) };
        let item = unsafe { &mut *items.add(id.to_usize()) };

        // If `use` is being called on an item with 0 references, then
        // either someone forgot to call it before, released too early
        // or lied about releasing. In any case something is wrong and
        // shouldn't be allowed.
        assert(item.meta.ref_count > RefCount::ZERO);

        item.meta.ref_count = item.meta.ref_count.add(RefCount::ONE);
    }

    /// Increment an item's reference count by a specified number.
    ///
    /// Asserts that the item's reference count is greater than 0.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    pub unsafe fn use_multiple<B: IntFromBase + Copy>(&self, base: B, id: Id, n: RefCount) {
        assert(id > Id::ZERO);
        assert(id.to_usize() < self.layout.cap);

        let items = unsafe { self.items.ptr(base) };
        let item = unsafe { &mut *items.add(id.to_usize()) };

        // If `use` is being called on an item with 0 references, then
        // either someone forgot to call it before, released too early
        // or lied about releasing. In any case something is wrong and
        // shouldn't be allowed.
        assert(item.meta.ref_count > RefCount::ZERO);

        item.meta.ref_count = item.meta.ref_count.add(n);
    }

    /// Get an item by its ID without incrementing its reference count.
    ///
    /// Asserts that the item's reference count is greater than 0.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    pub unsafe fn get<B: IntFromBase + Copy>(&self, base: B, id: Id) -> *mut T {
        assert(id > Id::ZERO);
        assert(id.to_usize() < self.layout.cap);

        let items = unsafe { self.items.ptr(base) };
        // SB: raw `*mut Item`, NOT `&mut *items.add(..)`. `get` returns a `*T`
        // (reference `*T`) and callers hold MULTIPLE results into the SAME set at
        // once — the render hyperlink comparison (render.rs) compares two entries
        // of one page's `hyperlink_set`. A `&mut` Unique retag here pops the
        // sibling result off the borrow stack and makes its later use UB
        // (rust-mut-args-strongly-protected). Field access goes through the raw.
        let item = unsafe { items.add(id.to_usize()) };

        assert(unsafe { (*item).meta.ref_count } > RefCount::ZERO);

        unsafe { core::ptr::addr_of_mut!((*item).value) }
    }

    // PERF(port): `get` split in two for callers that resolve MANY ids against
    // one set in a loop (the render managed-cell walk): `items_ptr` hoists the
    // base resolution once, `get_at` is the per-id remainder with `get`'s
    // exact asserts. `get(base, id)` == `get_at(items_ptr(base), id)`.

    /// The resolved items base pointer (the hoistable half of `get`).
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    pub unsafe fn items_ptr<B: IntFromBase + Copy>(&self, base: B) -> *mut Item<T, Id, RefCount> {
        unsafe { self.items.ptr(base) }
    }

    /// Get an item by its ID from a pre-resolved `items_ptr`, without
    /// incrementing its reference count.
    ///
    /// Asserts that the item's reference count is greater than 0.
    ///
    /// # Safety: `items` must be `self.items_ptr(base)` for this set's true
    /// backing buffer base.
    pub unsafe fn get_at(&self, items: *mut Item<T, Id, RefCount>, id: Id) -> *mut T {
        assert(id > Id::ZERO);
        assert(id.to_usize() < self.layout.cap);

        // SB: raw `*mut Item` — see `get`.
        let item = unsafe { items.add(id.to_usize()) };

        assert(unsafe { (*item).meta.ref_count } > RefCount::ZERO);

        unsafe { core::ptr::addr_of_mut!((*item).value) }
    }

    /// Releases a reference to an item by its ID.
    ///
    /// Asserts that the item's reference count is greater than 0.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    pub unsafe fn release<B: IntFromBase + Copy>(&mut self, base: B, id: Id) {
        assert(id > Id::ZERO);
        assert(id.to_usize() < self.layout.cap);

        let items = unsafe { self.items.ptr(base) };
        let item = unsafe { &mut *items.add(id.to_usize()) };

        assert(item.meta.ref_count > RefCount::ZERO);
        item.meta.ref_count = item.meta.ref_count.sub(RefCount::ONE);
        if item.meta.ref_count == RefCount::ZERO {
            self.living -= 1;
        }
    }

    /// Release a specified number of references to an item by its ID.
    ///
    /// Asserts that the item's reference count is at least `n`.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    // PORT NOTE: the reference signature is `releaseMultiple(self, base, id, n: Id)`
    // — the `n` parameter is typed `Id`, not `RefCountInt`. We preserve that.
    pub unsafe fn release_multiple<B: IntFromBase + Copy>(&mut self, base: B, id: Id, n: Id) {
        assert(id > Id::ZERO);
        assert(id.to_usize() < self.layout.cap);

        let items = unsafe { self.items.ptr(base) };
        let item = unsafe { &mut *items.add(id.to_usize()) };

        // assert(item.meta.ref >= n); — `n` is an `Id`; compare via usize.
        assert(item.meta.ref_count >= ref_from_id::<RefCount, Id>(n));
        item.meta.ref_count = item.meta.ref_count.sub(ref_from_id::<RefCount, Id>(n));

        if item.meta.ref_count == RefCount::ZERO {
            self.living -= 1;
        }
    }

    /// Get the ref count for an item by its ID.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    pub unsafe fn ref_count<B: IntFromBase + Copy>(&self, base: B, id: Id) -> RefCount {
        assert(id > Id::ZERO);
        assert(id.to_usize() < self.layout.cap);

        let items = unsafe { self.items.ptr(base) };
        let item = unsafe { &*items.add(id.to_usize()) };
        item.meta.ref_count
    }

    /// Get the current number of non-dead items in the set.
    pub fn count(&self) -> usize {
        self.living
    }

    /// Return an iterator over the live entries in this set.
    ///
    /// # Safety: `base` must be the true base of the backing buffer, and the
    /// returned iterator must not outlive that buffer.
    pub unsafe fn iterator<B: IntFromBase + Copy>(&self, base: B) -> Iterator<T, Id, RefCount> {
        Iterator {
            items: unsafe { self.items.ptr(base) },
            // id: Id = 1 (the reference field default; ID 0 is reserved).
            id: Id::ONE,
            end: self.next_id,
        }
    }

    /// Delete an item, removing any references from
    /// the table, and freeing its ID to be reused.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    unsafe fn delete_item<B: IntFromBase + Copy>(&mut self, base: B, id: Id) {
        let table = unsafe { self.table.ptr(base) };
        let items = unsafe { self.items.ptr(base) };

        let item = unsafe { *items.add(id.to_usize()) };

        if item.meta.bucket.to_usize() > self.layout.table_cap {
            return;
        }

        assert(unsafe { *table.add(item.meta.bucket.to_usize()) } == id);

        // Inform the context struct that we're
        // deleting the dead item's value for good.
        self.context.deleted(item.value);

        self.psl_stats[item.meta.psl.to_usize()] =
            self.psl_stats[item.meta.psl.to_usize()].sub(Id::ONE);
        unsafe {
            *table.add(item.meta.bucket.to_usize()) = Id::ZERO;
            *items.add(id.to_usize()) = Item::default();
        }

        let mut p: Id = item.meta.bucket;
        let mut n: Id = p.wrapping_add(Id::ONE).bit_and(self.layout.table_mask);

        while unsafe { *table.add(n.to_usize()) } != Id::ZERO
            && unsafe { (*items.add((*table.add(n.to_usize())).to_usize())).meta.psl } > Id::ZERO
        {
            unsafe {
                let tn = *table.add(n.to_usize());
                (*items.add(tn.to_usize())).meta.bucket = p;
                let psl = (*items.add(tn.to_usize())).meta.psl;
                self.psl_stats[psl.to_usize()] = self.psl_stats[psl.to_usize()].sub(Id::ONE);
                (*items.add(tn.to_usize())).meta.psl =
                    (*items.add(tn.to_usize())).meta.psl.sub(Id::ONE);
                let psl2 = (*items.add(tn.to_usize())).meta.psl;
                self.psl_stats[psl2.to_usize()] = self.psl_stats[psl2.to_usize()].add(Id::ONE);
                *table.add(p.to_usize()) = *table.add(n.to_usize());
            }
            p = n;
            n = p.wrapping_add(Id::ONE).bit_and(self.layout.table_mask);
        }

        while self.max_psl > Id::ZERO && self.psl_stats[self.max_psl.to_usize()] == Id::ZERO {
            self.max_psl = self.max_psl.sub(Id::ONE);
        }

        unsafe {
            *table.add(p.to_usize()) = Id::ZERO;
        }

        unsafe { self.assert_integrity(base) };
    }

    /// Find an item in the table and return its ID.
    /// If the item does not exist in the table, null is returned.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    #[inline]
    pub unsafe fn lookup<B: IntFromBase + Copy>(&self, base: B, value: T) -> Option<Id> {
        unsafe { self.lookup_context(base, &raw const value) }
    }
    unsafe fn lookup_context<B: IntFromBase + Copy>(&self, base: B, value: *const T) -> Option<Id> {
        // A zero-capacity set (a valid special case of Layout.init)
        // contains nothing and has a zero-size table, so we can't
        // probe it: table[0] would read whatever memory follows the
        // set in the backing buffer.
        // PORT NOTE (parity e44f5cb0f): the reference marks this branch
        // `@branchHint(.cold)`; Rust has no stable branch-hint intrinsic, the
        // guard itself is the fix.
        if self.layout.table_cap == 0 {
            return None;
        }

        let table = unsafe { self.table.ptr(base) };
        let items = unsafe { self.items.ptr(base) };

        let hash: u64 = self.context.hash(unsafe { *value });

        // for (0..self.max_psl + 1) |i| {
        for i in 0..self.max_psl.to_usize() + 1 {
            // const p: usize = @intCast((hash +% i) & self.layout.table_mask);
            let p: usize =
                (hash.wrapping_add(i as u64) & (self.layout.table_mask.to_usize() as u64)) as usize;
            let id = unsafe { *table.add(p) };

            // Empty bucket, our item cannot have probed to
            // any point after this, meaning it's not present.
            if id == Id::ZERO {
                return None;
            }

            let item = unsafe { *items.add(id.to_usize()) };

            // An item with a shorter probe sequence length would never
            // end up in the middle of another sequence, since it would
            // be swapped out if inserted before the new sequence, and
            // would not be swapped in if inserted afterwards.
            //
            // As such, our item cannot be present.
            if item.meta.psl.to_usize() < i {
                return None;
            }

            // If the item is a part of the same probe sequence,
            // we make sure it's not dead and then check to see
            // if it matches the value we're looking for.
            if item.meta.psl.to_usize() == i
                && item.meta.ref_count > RefCount::ZERO
                && self.context.eql(unsafe { *value }, item.value)
            {
                return Some(id);
            }
        }

        None
    }

    /// Find the provided value in the hash table, or add a new item
    /// for it if not present. If a new item is added, `new_id` will
    /// be used as the ID. If an existing item is found, the `new_id`
    /// is ignored and the existing item's ID is returned.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    unsafe fn upsert<B: IntFromBase + Copy>(&mut self, base: B, value: *const T, new_id: Id) -> Id {
        // If the item already exists, return it.
        if let Some(id) = unsafe { self.lookup_context(base, value) } {
            // Notify the context that the value is "deleted" because
            // we're reusing the existing value in the set. This allows
            // callers to clean up any resources associated with the value.
            self.context.deleted(unsafe { *value });

            return id;
        }

        unsafe { self.insert(base, value, new_id) }
    }

    /// Insert the given value into the hash table with the given ID.
    ///
    /// If runtime safety is enabled, asserts that
    /// the value is not already present in the table.
    ///
    /// # Safety: `base` must be the true base of the backing buffer.
    unsafe fn insert<B: IntFromBase + Copy>(&mut self, base: B, value: *const T, new_id: Id) -> Id {
        // if (comptime std.debug.runtime_safety)
        //     assert(self.lookupContext(base, value, ctx) == null);
        if cfg!(debug_assertions) {
            assert(unsafe { self.lookup_context(base, value) }.is_none());
        }

        let table = unsafe { self.table.ptr(base) };
        let items = unsafe { self.items.ptr(base) };

        // The new item that we'll put in to the table.
        // PERF(port) (reshape): `*value` here is the single copy of `T` into
        // the new item — the same one copy the by-value reference performs.
        let mut new_item: Item<T, Id, RefCount> = Item {
            value: unsafe { *value },
            meta: Metadata {
                bucket: Id::MAX,
                psl: Id::ZERO,
                ref_count: RefCount::ZERO,
            },
        };

        let hash: u64 = self.context.hash(new_item.value);

        let mut held_id: Id = new_id;
        // PORT NOTE: `held_item` is a `*Item` that may point either at the local
        // `new_item` or at an item inside the `items` array. We track which with
        // an enum so the borrow checker is satisfied while preserving the exact
        // pointer aliasing of the reference.
        let mut held_is_new = true;
        let mut held_item_idx: usize = 0; // valid when !held_is_new

        let mut chosen_id: Id = new_id;

        macro_rules! held_meta {
            () => {
                if held_is_new {
                    &mut new_item.meta
                } else {
                    unsafe { &mut (*items.add(held_item_idx)).meta }
                }
            };
        }

        // for (0..self.layout.table_cap - 1) |i| {
        for i in 0..self.layout.table_cap - 1 {
            // const p: Id = @intCast((hash +% i) & self.layout.table_mask);
            let p: Id = Id::from_usize(
                (hash.wrapping_add(i as u64) & (self.layout.table_mask.to_usize() as u64)) as usize,
            );
            let id = unsafe { *table.add(p.to_usize()) };

            // Empty bucket, put our held item in to it and break.
            if id == Id::ZERO {
                unsafe {
                    *table.add(p.to_usize()) = held_id;
                }
                let hm = held_meta!();
                hm.bucket = p;
                let hpsl = hm.psl;
                self.psl_stats[hpsl.to_usize()] = self.psl_stats[hpsl.to_usize()].add(Id::ONE);
                self.max_psl = max_id(self.max_psl, hpsl);
                break;
            }

            let item = unsafe { &mut *items.add(id.to_usize()) };

            // If there's a dead item then we resurrect it
            // for our value so that we can reuse its ID,
            // unless its ID is greater than the one we're
            // given (i.e. prefer smaller IDs).
            if item.meta.ref_count == RefCount::ZERO {
                // Dead items aren't super common relative
                // to other places to insert/swap the held
                // item in to the set.

                // Inform the context struct that we're
                // deleting the dead item's value for good.
                self.context.deleted(item.value);

                // Reap the dead item.
                self.psl_stats[item.meta.psl.to_usize()] =
                    self.psl_stats[item.meta.psl.to_usize()].sub(Id::ONE);
                // `*item = Item.default()` — but FIELD-WISE. See the PORT NOTE
                // in `init`: `Item<T, ..>` has 1 byte of trailing struct padding
                // (`meta` is align-2, `value` is align-1), and this array lives
                // inside the page mapping that `CompressedPage::init` compresses
                // as raw bytes. A whole-`Item` typed write de-initializes that
                // padding byte even though `init` zeroed it, so the compressor
                // would read uninitialized memory. A field-wise write touches
                // only the fields' own bytes and leaves the padding zeroed.
                let d = Item::<T, Id, RefCount>::default();
                item.value = d.value;
                item.meta = d.meta;

                // Only resurrect this item if it has a
                // smaller id than the one we were given.
                if id < new_id {
                    chosen_id = id;
                }

                // Put the currently held item in to the
                // bucket of the item that we just reaped.
                unsafe {
                    *table.add(p.to_usize()) = held_id;
                }
                let hm = held_meta!();
                hm.bucket = p;
                let hpsl = hm.psl;
                self.psl_stats[hpsl.to_usize()] = self.psl_stats[hpsl.to_usize()].add(Id::ONE);
                self.max_psl = max_id(self.max_psl, hpsl);

                break;
            }

            // If this item has a lower PSL, or has equal PSL and lower ref
            // count, then we swap it out with our held item. By doing this,
            // items with high reference counts are prioritized for earlier
            // placement. The assumption is that an item which has a higher
            // reference count will be accessed more frequently, so we want
            // to minimize the time it takes to find it.
            let held_psl = held_meta!().psl;
            let held_ref = if held_is_new {
                new_item.meta.ref_count
            } else {
                unsafe { (*items.add(held_item_idx)).meta.ref_count }
            };
            if item.meta.psl < held_psl
                || (item.meta.psl == held_psl && item.meta.ref_count < held_ref)
            {
                // Put our held item in the bucket.
                unsafe {
                    *table.add(p.to_usize()) = held_id;
                }
                let hm = held_meta!();
                hm.bucket = p;
                let hpsl = hm.psl;
                self.psl_stats[hpsl.to_usize()] = self.psl_stats[hpsl.to_usize()].add(Id::ONE);
                self.max_psl = max_id(self.max_psl, hpsl);

                // Pick up the item that has a lower PSL.
                held_id = id;
                held_is_new = false;
                held_item_idx = id.to_usize();
                self.psl_stats[item.meta.psl.to_usize()] =
                    self.psl_stats[item.meta.psl.to_usize()].sub(Id::ONE);
            }

            // Advance to the next probe position for our held item.
            let hm = held_meta!();
            hm.psl = hm.psl.add(Id::ONE);
        }

        // Our chosen ID may have changed if we decided
        // to reuse a dead item's ID, so we make sure
        // the chosen bucket contains the correct ID.
        unsafe {
            *table.add(new_item.meta.bucket.to_usize()) = chosen_id;
        }

        // Finally place our new item in to our array.
        //
        // FIELD-WISE, not `*items.add(..) = new_item` — see the PORT NOTE in
        // `init` and the reap above: a whole-`Item` typed write would
        // de-initialize the type's 1 trailing padding byte inside the page
        // mapping, and `CompressedPage::init` compresses that mapping as raw
        // bytes. The two field writes copy exactly the same value bytes.
        unsafe {
            let dst = items.add(chosen_id.to_usize());
            (*dst).value = new_item.value;
            (*dst).meta = new_item.meta;
        }

        unsafe { self.assert_integrity(base) };

        chosen_id
    }

    /// # Safety: `base` must be the true base of the backing buffer.
    unsafe fn assert_integrity<B: IntFromBase + Copy>(&self, base: B) {
        // Disabled because this is excessively slow, only enable
        // if debugging a RefCountedSet issue or modifying its logic.
        //
        // PORT NOTE: the reference body is guarded by `if (false and
        // std.debug.runtime_safety)`, so it is dead at runtime. The full body is
        // preserved verbatim below as a comment so no content is lost:
        //
        // if (false and std.debug.runtime_safety) {
        //     const table = self.table.ptr(base);
        //     const items = self.items.ptr(base);
        //
        //     var psl_stats: [32]Id = @splat(0);
        //
        //     for (items[0..self.layout.cap], 0..) |item, id| {
        //         if (item.meta.bucket < std.math.maxInt(Id)) {
        //             assert(table[item.meta.bucket] == id);
        //             psl_stats[item.meta.psl] += 1;
        //         }
        //     }
        //
        //     std.testing.expectEqualSlices(Id, &psl_stats, &self.psl_stats) catch assert(false);
        //
        //     assert(std.mem.eql(Id, &psl_stats, &self.psl_stats));
        //
        //     psl_stats = @splat(0);
        //
        //     for (table[0..self.layout.table_cap], 0..) |id, bucket| {
        //         const item = items[id];
        //         if (item.meta.bucket < std.math.maxInt(Id)) {
        //             assert(item.meta.bucket == bucket);
        //
        //             const hash: u64 = ctx.hash(item.value);
        //             const p: usize = @intCast((hash +% item.meta.psl) & self.layout.table_mask);
        //             assert(p == bucket);
        //
        //             psl_stats[item.meta.psl] += 1;
        //         }
        //     }
        //
        //     std.testing.expectEqualSlices(Id, &psl_stats, &self.psl_stats) catch assert(false);
        // }
        let _ = base;
    }
}

/// A live entry returned by `Iterator`.
//
// PORT NOTE: the reference nests `Entry`/`Iterator` inside the comptime
// `RefCountedSet(...)` type; Rust hoists them to sibling generics (the same
// treatment as `Item`/`Metadata`/`Layout` above). The reference
// `value_ptr: *T` maps to a raw `*mut T`, not a `&mut T`: the pointee lives in
// the set's backing buffer and a held reference would alias any later set
// access (see the `Iterator` invalidation note below).
pub struct Entry<T, Id: IdInt> {
    pub id: Id,
    pub value_ptr: *mut T,
}

/// Iterates live entries in ascending ID order.
///
/// Released entries whose reference count reached zero are skipped.
/// Any mutation of the set invalidates the iterator.
//
// LIFETIMES:
//   items: *mut Item — BORROW_FIELD, raw non-owning pointer into the set's
//     backing buffer (`self.items.ptr(base)`, the reference `[*]Item`); the
//     buffer's owner (the page / test harness) outlives the iterator per the
//     `iterator()` safety contract. Kept raw exactly as the reference keeps a
//     many-item pointer: any mutation of the set invalidates it, and no `&`/
//     `&mut` is ever formed over the whole item array.
pub struct Iterator<T, Id: IdInt, RefCount: RefCountIntTrait> {
    pub items: *mut Item<T, Id, RefCount>,
    pub id: Id,
    pub end: Id,
}

impl<T, Id: IdInt, RefCount: RefCountIntTrait> Iterator<T, Id, RefCount> {
    pub fn next(&mut self) -> Option<Entry<T, Id>> {
        while self.id < self.end {
            let id = self.id;
            self.id = self.id.add(Id::ONE);

            // const item = &self.items[id];
            //
            // SAFETY: `items` points at the live item array of the set this
            // iterator was created from (invalidation contract above), and
            // `id < end <= next_id <= layout.cap` bounds the index. Raw
            // pointer arithmetic + field projections only — no reference to
            // the item array is formed.
            let item = unsafe { self.items.add(id.to_usize()) };
            if unsafe { (*item).meta.ref_count } == RefCount::ZERO {
                continue;
            }

            return Some(Entry {
                id,
                // &item.value
                value_ptr: unsafe { core::ptr::addr_of_mut!((*item).value) },
            });
        }

        None
    }
}

/// `@max(a, b)` over `Id`.
#[inline]
fn max_id<Id: IdInt>(a: Id, b: Id) -> Id {
    if a > b {
        a
    } else {
        b
    }
}

/// Convert an `Id` value to a `RefCount` value, mirroring the implicit widening
/// the reference performs when comparing/subtracting the `n: Id` argument of
/// `releaseMultiple` against a `RefCountInt` field.
#[inline]
fn ref_from_id<RefCount: RefCountIntTrait, Id: IdInt>(n: Id) -> RefCount {
    // Build up `n` ones; in practice callers pass small counts. We avoid a
    // numeric-cast dependency by repeated addition from zero.
    let mut acc = RefCount::ZERO;
    let mut i: usize = 0;
    let target = n.to_usize();
    while i < target {
        acc = acc.add(RefCount::ONE);
        i += 1;
    }
    acc
}

// PORT NOTE: `std.math.log2_int(usize, x)` — floor(log2(x)) for x >= 1.
#[inline]
fn log2_int(x: usize) -> u32 {
    assert(x > 0);
    (usize::BITS - 1) - x.leading_zeros()
}

// PORT NOTE: `std.mem.alignForward(usize, value, alignment)`.
#[inline]
fn align_forward(value: usize, alignment: usize) -> usize {
    value.next_multiple_of(alignment)
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use super::*;

    // The reference test context:
    //   struct {
    //       pub fn hash(_: *const @This(), value: u32) u64 {
    //           return std.hash.int(value);
    //       }
    //       pub fn eql(_: *const @This(), a: u32, b: u32) bool {
    //           return a == b;
    //       }
    //   },
    #[derive(Default, Clone, Copy)]
    struct TestContext;

    impl RefCountedSetContext<u32> for TestContext {
        fn hash(&self, value: u32) -> u64 {
            // std.hash.int(value) — u32 result widened to u64.
            hash_int_u32(value) as u64
        }

        fn eql(&self, a: u32, b: u32) -> bool {
            a == b
        }
    }

    // PORT NOTE: `std.hash.int` on a u32 dispatches to `uint32` (the lowbias32
    // finalizer). Ported verbatim, matching the `hash_int_u64` treatment in
    // `style.rs`.
    fn hash_int_u32(input: u32) -> u32 {
        // fn uint32(input: u32) u32 {
        let mut x: u32 = input;
        x = (x ^ (x >> 16)).wrapping_mul(0x7feb352d);
        x = (x ^ (x >> 15)).wrapping_mul(0x846ca68b);
        x = x ^ (x >> 16);
        x
    }

    // const TestSet = RefCountedSet(u32, u16, u16, ...);
    type TestSet = RefCountedSet<u32, u16, u16, TestContext>;

    // ref: test "iterator visits live entries in ID order"
    #[test]
    fn iterator_visits_live_entries_in_ID_order() {
        // const layout: TestSet.Layout = .init(8);
        let layout: Layout<u16> = Layout::init::<u32, u16>(8);
        // const buf = try alloc.alignedAlloc(u8, TestSet.base_align, layout.total_size);
        // defer alloc.free(buf);
        let alloc_layout =
            core::alloc::Layout::from_size_align(layout.total_size, TestSet::base_align()).unwrap();
        // SAFETY: `total_size` for a cap-8 layout is non-zero.
        let buf = unsafe { std::alloc::alloc(alloc_layout) };
        assert!(!buf.is_null());
        let base = OffsetBuf::init(buf);

        // var set: TestSet = .init(.init(buf), layout, .{});
        //
        // SAFETY: `buf` is a live `base_align`-aligned allocation of
        // `layout.total_size` bytes, used as the base for every call below.
        unsafe {
            let mut set: TestSet = TestSet::init(base, layout, TestContext);
            let first = set.add(base, 11).unwrap();
            let released = set.add(base, 22).unwrap();
            let last = set.add(base, 33).unwrap();
            let _ = set.add(base, 11).unwrap();
            set.release(base, released);

            let mut it = set.iterator(base);

            let first_entry = it.next().unwrap();
            assert_eq!(first, first_entry.id);
            assert_eq!(11u32, *first_entry.value_ptr);

            let last_entry = it.next().unwrap();
            assert_eq!(last, last_entry.id);
            assert_eq!(33u32, *last_entry.value_ptr);

            assert!(it.next().is_none());

            // defer alloc.free(buf);
            std::alloc::dealloc(buf, alloc_layout);
        }
    }
}
