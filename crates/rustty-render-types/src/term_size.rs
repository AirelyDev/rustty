// PORTS: ghostty src/terminal/size.zig.
//
// A1-HOIST (per the crate_map contract): in the ghostty tree this is
// `terminal/size.zig`, part of the terminal package. It is hoisted into
// rustty-render-types so that render-types is a clean LEAF depending ONLY on
// rustty-util (for `quirks::inlineAssert`): `renderer/size.zig` imports the
// `CellCountInt` type from here, and pulling the whole terminal crate in would
// recreate the cycle the hoist breaks. No behavior/comments/tests are lost.
//
// The reference file is `const std = @import("std");` plus
// `const assert = @import("../quirks.zig").inlineAssert;`. In rustty the assert
// quirk lives in the rustty-util leaf, so:

use rustty_util::quirks::inlineAssert as assert;

/// The maximum size of a page in bytes. We use a u16 here because any
/// smaller bit size by Zig is upgraded anyways to a u16 on mainstream
/// CPU architectures, and because 65KB is a reasonable page size. To
/// support better configurability, we derive everything from this.
pub const MAX_PAGE_SIZE: u32 = u32::MAX;

/// The int type that can contain the maximum memory offset in bytes,
/// derived from the maximum terminal page size.
//
// PORT NOTE: the reference is
// `std.math.IntFittingRange(0, max_page_size - 1)`; with `max_page_size ==
// maxInt(u32)` the range `[0, 2^32 - 2]` fits in a `u32`, so `OffsetInt == u32`
// (asserted by the `Offset` test). We bind the alias to `u32` to match.
pub type OffsetInt = u32;

// Int types for maximum values of things. A lot of these sizes are
// based on "X is enough for any reasonable use case" principles.
// The goal is that a user can have the maxInt amount of all of these
// present at one time and be able to address them in a single Page.zig.

// Total number of cells that are possible in each dimension (row/col).
// Based on 2^16 being enough for any reasonable terminal size and allowing
// IDs to remain 16-bit.
pub type CellCountInt = u16;

// Total number of styles and hyperlinks that are possible in a page.
// We match CellCountInt here because each cell in a single row can have at
// most one style, making it simple to split a page by splitting rows.
//
// Note due to the way RefCountedSet works, we are short one value, but
// this is a theoretical limit we accept. A page with a single row max
// columns wide would be one short of having every cell have a unique style.
pub type StyleCountInt = CellCountInt;
pub type HyperlinkCountInt = CellCountInt;

// Total number of bytes that can be taken up by grapheme data and string
// data. Both of these technically unlimited with malicious input, but
// we choose a reasonable limit of 2^32 (4GB) per.
pub type GraphemeBytesInt = u32;
pub type StringBytesInt = u32;

/// The offset from the base address of the page to the start of some data.
/// This is typed for ease of use.
///
/// This is a packed struct so we can attach methods to an int.
//
// PORT NOTE: the Zig reference is a `comptime fn Offset(comptime T: type) type`
// returning `packed struct(OffsetInt) { offset: OffsetInt = 0, ... }` — a
// generic, ABI-transparent wrapper over a single `OffsetInt`. In Rust the
// generic type plus methods is expressed as `Offset<T>`, kept ABI-transparent
// with `#[repr(transparent)]` over the `OffsetInt` field (the packed-struct
// layout). `PhantomData<T>` carries the element type with zero storage.
#[repr(transparent)]
pub struct Offset<T> {
    pub offset: OffsetInt,
    _marker: core::marker::PhantomData<T>,
}

// `packed struct(OffsetInt) { offset: OffsetInt = 0 }` — default is a zero
// offset, and the wrapper is freely Copy/Clone/Eq like an integer.
impl<T> Default for Offset<T> {
    fn default() -> Self {
        Offset {
            offset: 0,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<T> Clone for Offset<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Offset<T> {}

impl<T> PartialEq for Offset<T> {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
    }
}

impl<T> Eq for Offset<T> {}

// PORT NOTE: `Offset` is used as a key in `AutoOffsetHashMap` (the page's
// grapheme/hyperlink maps key on `Offset(Cell)`). The map's auto-context hashes
// the key, so `Offset` must be `Hash`; it hashes by its single integer field,
// matching the reference's `std.hash.autoHash` over the packed `OffsetInt`.
impl<T> core::hash::Hash for Offset<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.offset.hash(state);
    }
}

impl<T> core::fmt::Debug for Offset<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Offset")
            .field("offset", &self.offset)
            .finish()
    }
}

impl<T> Offset<T> {
    /// Construct an `Offset` from a raw offset value. (The reference uses
    /// struct literals like `.{ .offset = N }`; this is the Rust equivalent
    /// constructor and carries no extra semantics.)
    pub const fn new(offset: OffsetInt) -> Self {
        Offset {
            offset,
            _marker: core::marker::PhantomData,
        }
    }

    /// Returns a pointer to the start of the data, properly typed.
    ///
    /// # Safety
    ///
    /// The reference is an `inline fn` returning `[*]T` from raw integer
    /// arithmetic; the caller must guarantee `base + offset` points at valid,
    /// properly-typed `T` data for the intended length (the reference has no
    /// such guard either). Returns a raw `*mut T`.
    #[inline]
    pub unsafe fn ptr<B: IntFromBase>(self, base: B) -> *mut T {
        // The offset must be properly aligned for the type since
        // our return type is naturally aligned. We COULD modify this
        // to return arbitrary alignment, but its not something we need.
        let addr = base.int_from_base() + self.offset as usize;
        // PARITY: the reference is `assert(addr % @alignOf(T) == 0)`; the modulo
        // form is kept verbatim rather than rewritten to `is_multiple_of` so the
        // ported expression matches the reference char-for-char.
        #[allow(clippy::manual_is_multiple_of)]
        {
            assert(addr % core::mem::align_of::<T>() == 0);
        }
        addr as *mut T
    }
}

/// A slice of type T that stores via a base offset and len.
//
// PORT NOTE: the reference declares this as `Offset(T).Slice`, a nested struct
// holding an `Offset(T)` plus a `len`. Rust associated types can't be generic
// over the outer `T` in the same way, so it is hoisted to a standalone
// `OffsetSlice<T>` with the identical fields and `slice` method.
//
// PORT NOTE (P3 F1, uninit-in-page-mapping fix): this is `#[repr(C)]` with an
// EXPLICIT `_padding` field rather than a plain `repr(Rust)` two-field struct,
// for exactly the reason recorded on `style::Color`. `OffsetSlice` values are
// stored INSIDE the terminal page's backing mapping (`hyperlink.PageEntry.uri`
// and `.id`, and the grapheme map's values), and
// `compress::CompressedPage::init` compresses that whole mapping as one
// `&[u8]` — as the oracle's `compress/Page.zig` compresses `page.memory`. A
// `u32` next to a `usize` under `repr(Rust)` yields 4 bytes of TAIL PADDING,
// and a typed Rust write leaves padding UNINITIALIZED even over freshly-zeroed
// memory, so the compressor then read uninit bytes (Miri: "constructing
// invalid value of type `[u8; 4]`: encountered uninitialized memory" inside
// `lz4::compress`).
//
// Naming the padding is a faithfulness IMPROVEMENT, not a divergence: the
// oracle's `Offset(T).Slice` is a Zig auto-layout struct with the same `u32` +
// `usize` fields and therefore the same 4 padding bytes and the same
// `@sizeOf == 16`; Zig simply permits reading those undefined bytes and Rust
// does not. `repr(C)` in the declared order gives `offset` @0, `_padding` @4,
// `len` @8 — 16 bytes, align 8, every byte covered by a field. This port
// already names such implicit padding elsewhere (`Row._padding: u23`).
#[repr(C)]
pub struct OffsetSlice<T> {
    pub offset: Offset<T>,
    /// The 4 tail-padding bytes the field pair implies, named so that every
    /// byte of an `OffsetSlice` written into a page mapping is initialized.
    /// Always zero; never read.
    _padding: u32,
    pub len: usize,
}

// `OffsetSlice` must have no implicit padding at all, or the reshape above is
// pointless. 4 + 4 + 8 == 16 at align 8.
const _: () = assert!(core::mem::size_of::<OffsetSlice<u8>>() == 16);
const _: () = assert!(core::mem::align_of::<OffsetSlice<u8>>() == 8);

// PORT NOTE: the reference `Offset(T).Slice` is a plain value type copied freely
// (`var x = self.uri`); both fields are `Copy`, so the wrapper is `Copy`/`Clone`.
impl<T> Clone for OffsetSlice<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for OffsetSlice<T> {}

impl<T> Default for OffsetSlice<T> {
    fn default() -> Self {
        OffsetSlice::new(Offset::default(), 0)
    }
}

impl<T> OffsetSlice<T> {
    /// Construct an `OffsetSlice` from its two reference fields. (The reference
    /// uses struct literals `.{ .offset = ..., .len = ... }`; this constructor
    /// exists only so callers never have to name the private `_padding` field
    /// added by the byte-completeness reshape above.)
    #[inline]
    pub const fn new(offset: Offset<T>, len: usize) -> Self {
        OffsetSlice {
            offset,
            _padding: 0,
            len,
        }
    }

    /// Returns a slice for the data, properly typed.
    ///
    /// # Safety
    ///
    /// Mirrors `Offset(T).Slice.slice`: returns `self.offset.ptr(base)[0..len]`.
    /// The caller must guarantee the resulting range is valid `T` data.
    #[inline]
    pub unsafe fn slice<'a, B: IntFromBase>(self, base: B) -> &'a [T] {
        unsafe { core::slice::from_raw_parts(self.offset.ptr(base), self.len) }
    }
}

/// Represents a buffer that is offset from some base pointer.
/// Offset-based structures should use this as their initialization
/// parameter so that they can know what segment of memory they own
/// while at the same time initializing their offset fields to be
/// against the true base.
///
/// The term "true base" is used to describe the base address of
/// the allocation, which i.e. can include memory that you do NOT
/// own and is used by some other structures. All offsets are against
/// this "true base" so that to determine addresses structures don't
/// need to add up all the intermediary offsets.
#[derive(Clone, Copy)]
pub struct OffsetBuf {
    /// The true base pointer to the backing memory. This is
    /// "byte zero" of the allocation. This plus the offset make
    /// it easy to pass in the base pointer in all usage to this
    /// structure and the offsets are correct.
    pub base: *mut u8,

    /// Offset from base where the beginning of /this/ data
    /// structure is located. We use this so that we can slowly
    /// build up a chain of offset-based structures but always
    /// have the base pointer sent into functions be the true base.
    pub offset: usize,
}

impl OffsetBuf {
    /// Initialize a zero-offset buffer from a base.
    pub fn init<B: IntFromBase>(base: B) -> OffsetBuf {
        OffsetBuf::init_offset(base, 0)
    }

    /// Initialize from some base pointer and offset.
    pub fn init_offset<B: IntFromBase>(base: B, offset: usize) -> OffsetBuf {
        OffsetBuf {
            base: base.int_from_base() as *mut u8,
            offset,
        }
    }

    /// The base address for the start of the data for the user
    /// of this OffsetBuf. This is where your data structure should
    /// begin; anything before this is NOT your memory.
    ///
    /// # Safety
    ///
    /// Mirrors the reference `start`: `base + offset`. The caller must ensure
    /// the resulting pointer is within (or one past the end of) the backing
    /// allocation.
    pub unsafe fn start(self) -> *mut u8 {
        // const ptr = self.base + self.offset;
        let ptr = unsafe { self.base.add(self.offset) };
        // return @ptrCast(ptr); — `ptr` is already `*mut u8` so the cast is a
        // no-op (the reference's `@ptrCast` is from `[*]u8`). The reference keeps
        // the intermediate `ptr` const, so we preserve that two-statement shape.
        #[allow(clippy::let_and_return)]
        ptr
    }

    /// Returns an Offset calculation for some child member of
    /// your struct. The offset is against the true base pointer
    /// so that future callers can pass that in as the base.
    pub fn member<T>(self, len: usize) -> Offset<T> {
        Offset::new((self.offset + len) as OffsetInt)
    }

    /// Add an offset to the current offset.
    // PARITY: named `add` after the reference `OffsetBuf.add`; not the
    // `std::ops::Add::add` trait method.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, offset: usize) -> OffsetBuf {
        OffsetBuf {
            base: self.base,
            offset: self.offset + offset,
        }
    }

    /// Rebase the offset to have a zero offset by rebasing onto start.
    /// This is similar to `add` but all of the offsets are merged into base.
    ///
    /// # Safety
    ///
    /// Mirrors the reference `rebase`: `base = start() + offset`. The caller
    /// must ensure the resulting base stays within the backing allocation.
    pub unsafe fn rebase(self, offset: usize) -> OffsetBuf {
        OffsetBuf {
            base: unsafe { self.start().add(offset) },
            offset: 0,
        }
    }
}

/// Get the offset for a given type from some base pointer to the
/// actual pointer to the type.
///
/// # Safety
///
/// Mirrors `getOffset`: subtracts the base integer from the pointer integer.
/// The caller must guarantee `ptr` lies at or after `base` within the same
/// allocation so the subtraction does not underflow (the reference relies on
/// the same invariant).
#[inline]
pub unsafe fn get_offset<T, B: IntFromBase>(base: B, ptr: *const T) -> Offset<T> {
    let base_int = base.int_from_base();
    let ptr_int = ptr as usize;
    let offset = ptr_int - base_int;
    Offset::new(offset as OffsetInt)
}

/// The `intFromBase` dispatch from the reference. The Zig reference is a
/// `comptime` switch over `@typeInfo(@TypeOf(base))`:
///   - a pointer (`.one`/`.many`/`.c`) → `@intFromPtr(base)`
///   - a slice (`.slice`) → `@intFromPtr(base.ptr)`
///   - an `OffsetBuf` → `@intFromPtr(base.base)`
///   - anything else → `@compileError("invalid base type")`
///
/// Rust expresses this type-directed dispatch with a trait implemented for the
/// accepted base types; an unaccepted type simply does not implement it (the
/// compile-time equivalent of the `@compileError` arm).
pub trait IntFromBase {
    fn int_from_base(self) -> usize;
}

// .one, .many, .c => @intFromPtr(base),
impl<T> IntFromBase for *const T {
    fn int_from_base(self) -> usize {
        self as usize
    }
}

impl<T> IntFromBase for *mut T {
    fn int_from_base(self) -> usize {
        self as usize
    }
}

// .slice => @intFromPtr(base.ptr),
impl<T> IntFromBase for &[T] {
    fn int_from_base(self) -> usize {
        self.as_ptr() as usize
    }
}

impl<T> IntFromBase for &mut [T] {
    fn int_from_base(self) -> usize {
        self.as_ptr() as usize
    }
}

// OffsetBuf => @intFromPtr(base.base),
impl IntFromBase for OffsetBuf {
    fn int_from_base(self) -> usize {
        self.base as usize
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn Offset() {
        // This test is here so that if Offset changes, we can be very aware
        // of this effect and think about the implications of it.
        assert!(core::any::TypeId::of::<OffsetInt>() == core::any::TypeId::of::<u32>());
    }

    #[test]
    fn Offset_ptr_u8() {
        let offset: Offset<u8> = Offset::new(42);
        let base_int: usize = (&offset as *const Offset<u8>) as usize;
        let actual = unsafe { offset.ptr(&offset as *const Offset<u8> as *const u8) };
        assert_eq!(base_int + 42, actual as usize);
    }

    #[test]
    fn Offset_ptr_structural() {
        #[allow(dead_code)]
        struct Struct {
            x: u32,
            y: u32,
        }
        let offset: Offset<Struct> =
            Offset::new((core::mem::align_of::<Struct>() * 4) as OffsetInt);
        let base_int: usize = align_forward(
            (&offset as *const Offset<Struct>) as usize,
            core::mem::align_of::<Struct>(),
        );
        let base: *const u8 = base_int as *const u8;
        let actual = unsafe { offset.ptr(base) };
        assert_eq!(base_int + offset.offset as usize, actual as usize);
    }

    #[test]
    fn getOffset_bytes() {
        let widgets: &[u8] = b"ABCD";
        let offset = unsafe { get_offset(widgets.as_ptr(), &widgets[2] as *const u8) };
        assert_eq!(2 as OffsetInt, offset.offset);
    }

    #[test]
    fn getOffset_structs() {
        #[derive(Clone, Copy)]
        struct Widget {
            #[allow(dead_code)]
            x: u32,
            #[allow(dead_code)]
            y: u32,
        }
        let widgets: &[Widget] = &[
            Widget { x: 1, y: 2 },
            Widget { x: 3, y: 4 },
            Widget { x: 5, y: 6 },
            Widget { x: 7, y: 8 },
            Widget { x: 9, y: 10 },
        ];
        let offset = unsafe { get_offset(widgets.as_ptr(), &widgets[2] as *const Widget) };
        assert_eq!(
            (core::mem::size_of::<Widget>() * 2) as OffsetInt,
            offset.offset,
        );
    }

    // Mirror of `std.mem.alignForward(usize, addr, alignment)` used by the
    // `Offset ptr structural` test to round the base up to the type alignment.
    fn align_forward(addr: usize, alignment: usize) -> usize {
        addr.next_multiple_of(alignment)
    }
}
