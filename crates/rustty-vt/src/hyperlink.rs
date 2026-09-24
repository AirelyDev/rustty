//! rustty-vt hyperlink
//!
//! PORTS: ghostty src/terminal/hyperlink.zig
//!
//! PORT STATUS: done
//!
//! PORT NOTE: the reference imports are
//!   const std = @import("std");
//!   const Allocator = std.mem.Allocator;
//!   const hash_map = @import("hash_map.zig");
//!   const AutoOffsetHashMap = hash_map.AutoOffsetHashMap;
//!   const pagepkg = @import("page.zig");
//!   const size = @import("size.zig");
//!   const Offset = size.Offset;
//!   const Cell = pagepkg.Cell;
//!   const Page = pagepkg.Page;
//!   const RefCountedSet = @import("ref_counted_set.zig").RefCountedSet;
//!   const Wyhash = std.hash.Wyhash;
//!   const autoHash = std.hash.autoHash;
//!   const autoHashStrat = std.hash.autoHashStrat;
//! `page` (Cell/Page) is its mutually-recursive sibling, ported alongside this
//! file. The offset types are the hoisted size types from rustty-render-types
//! (A1). `RefCountedSet` and `AutoOffsetHashMap` are the already-ported page
//! data structures.
//!
//! PORT NOTE: the reference `RefCountedSet` takes a per-call `ContextT` value
//! (passed to `addContext`/`lookupContext`/etc.). The ported Rust
//! `RefCountedSet` stores the context as a public field (`set.context`) and the
//! `add`/`lookup`/etc. methods read it. To reproduce the reference's per-call
//! context (which carries the `page`/`src_page` pointers needed by `hash`/
//! `eql`/`deleted`), callers mutate `set.context.page` / `set.context.src_page`
//! immediately before each call. The context here is `SetContext` with the same
//! two nullable page pointers as the reference's anonymous struct.

use rustty_render_types::term_size as size;
use rustty_render_types::term_size::{get_offset, Offset, OffsetSlice};

use crate::hash_map::AutoOffsetHashMap;
use crate::page::{Cell, Page};
use crate::ref_counted_set::{RefCountedSet, RefCountedSetContext};

/// The allocation-failure error, mirroring the reference `error{OutOfMemory}`
/// (Zig's `Allocator.Error`) returned by `PageEntry.dupe`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

/// The unique identifier for a hyperlink. This is at most the number of cells
/// that can fit in a single terminal page, since each cell can only contain
/// at most one hyperlink.
pub type Id = size::HyperlinkCountInt;

// The mapping of cell to hyperlink. We use an offset hash map to save space
// since its very unlikely a cell is a hyperlink, so its a waste to store
// the hyperlink ID in the cell itself.
pub type Map = AutoOffsetHashMap<Offset<Cell>, Id, 80>;

/// A fully decoded hyperlink that may or may not have its
/// memory within a page. The memory location of this is dependent
/// on the context so users should check with the source of the
/// hyperlink.
//
// PORT NOTE (parity tail S7-3): the reference's `uri`/`id` are byte slices
// whose memory "is dependent on the context". The port owns them, in `B`:
// `Vec<u8>` (the default: a value on the Rust heap, as every page and snapshot
// path uses) or [`AllocBytes`] (a copy made through an allocator handle, which
// is what `Screen.cursor.hyperlink` holds: the reference `alloc.create` +
// `source.dupe(alloc)` on the screen's allocator).
#[derive(Clone)]
pub struct Hyperlink<B = Vec<u8>> {
    pub id: HyperlinkId<B>,
    pub uri: B,
}

/// Hyperlink bytes allocated through an allocator handle ([`Hyperlink::dupe_in`]).
pub type AllocBytes = allocator_api2::vec::Vec<u8, crate::alloc::Allocator>;

/// See PageEntry.Id
// PORT NOTE: the reference is `Hyperlink.Id`, a nested union. Rust nested type
// names can't shadow the field name `id`, so it is hoisted to `HyperlinkId`.
#[derive(Clone)]
pub enum HyperlinkId<B = Vec<u8>> {
    Explicit(B),
    Implicit(size::OffsetInt),
}

impl<B: core::ops::Deref<Target = [u8]>> Hyperlink<B> {
    /// Deinit and deallocate all the pointers using the given
    /// allocator.
    ///
    /// WARNING: This should only be called if the hyperlink was
    /// heap-allocated. This DOES NOT need to be unconditionally
    /// called.
    // PORT NOTE: the reference `deinit(alloc)` frees `uri` and (if explicit) the
    // id. In Rust the owned `Vec<u8>`s are freed when the `Hyperlink` is
    // dropped, so this is a no-op kept for fidelity. (The reference's allocator
    // is the Rust global allocator here.)
    pub fn deinit(&self) {
        // alloc.free(self.uri);
        // switch (self.id) { .implicit => {}, .explicit => |v| alloc.free(v) }
    }

    /// Duplicate a hyperlink by allocating all values with the
    /// given allocator. The returned hyperlink should have deinit
    /// called.
    ///
    /// PORT NOTE: the reference is
    /// `dupe(self, alloc) Allocator.Error!Hyperlink` — every `alloc.dupe` is
    /// `try`'d, so allocation failure is RECOVERABLE and propagates to
    /// `Screen.startHyperlink`'s `IncreaseCapacityError!void`. A plain
    /// `Vec::clone` aborts the process instead, which silently voided that
    /// contract for every caller; the copies therefore go through
    /// `try_dupe_bytes` (`Vec::try_reserve_exact`, the Rust fallible-allocation
    /// primitive) and the signature is `Result<_, OutOfMemory>` like the
    /// reference's.
    pub fn dupe(&self) -> Result<Hyperlink, OutOfMemory> {
        // const uri = try alloc.dupe(u8, self.uri);
        let uri = try_dupe_bytes(&self.uri)?;
        // errdefer alloc.free(uri) — the owned Vec is dropped on the `?` below.
        let id: HyperlinkId = match &self.id {
            HyperlinkId::Implicit(v) => HyperlinkId::Implicit(*v),
            HyperlinkId::Explicit(v) => HyperlinkId::Explicit(try_dupe_bytes(v)?),
        };
        Ok(Hyperlink { id, uri })
    }

    /// `dupe(alloc)` through an allocator handle: every copy is made (and
    /// later freed) through `alloc`, as the reference's `alloc.dupe` calls are.
    pub fn dupe_in(
        &self,
        alloc: crate::alloc::Allocator,
    ) -> Result<Hyperlink<AllocBytes>, OutOfMemory> {
        // const uri = try alloc.dupe(u8, self.uri);
        let uri = try_dupe_bytes_in(&self.uri, alloc)?;
        // errdefer alloc.free(uri) — dropped (through `alloc`) on the `?` below.
        let id = match &self.id {
            HyperlinkId::Implicit(v) => HyperlinkId::Implicit(*v),
            HyperlinkId::Explicit(v) => HyperlinkId::Explicit(try_dupe_bytes_in(v, alloc)?),
        };
        Ok(Hyperlink { id, uri })
    }

    /// An infallible Rust-heap copy (`Clone` into the default byte storage).
    /// Port-only: used where the reference passes the value by (borrowed)
    /// value and allocates nothing, so the copy must not reach the caller's
    /// allocator.
    pub fn to_heap(&self) -> Hyperlink {
        Hyperlink {
            id: match &self.id {
                HyperlinkId::Implicit(v) => HyperlinkId::Implicit(*v),
                HyperlinkId::Explicit(v) => HyperlinkId::Explicit(v.to_vec()),
            },
            uri: self.uri.to_vec(),
        }
    }
}

/// `alloc.dupe(u8, src)` through an allocator handle: one exact-sized,
/// fallible allocation.
pub(crate) fn try_dupe_bytes_in(
    src: &[u8],
    alloc: crate::alloc::Allocator,
) -> Result<AllocBytes, OutOfMemory> {
    use crate::alloc::CopyVecExt;
    let mut v = AllocBytes::new_in(alloc);
    v.try_reserve_exact(src.len()).map_err(|_| OutOfMemory)?;
    v.extend_copy(src);
    Ok(v)
}

/// `alloc.dupe(u8, src)` — a fallible byte-slice copy.
///
/// PORT NOTE: Zig's `Allocator.dupe` RETURNS `error.OutOfMemory`; Rust's
/// `[T]::to_vec`/`Vec::clone` abort. `Vec::try_reserve_exact` is the fallible
/// equivalent (it also rejects a capacity that overflows `isize`, which Zig's
/// allocator interface reports the same way).
pub(crate) fn try_dupe_bytes(src: &[u8]) -> Result<Vec<u8>, OutOfMemory> {
    let mut v: Vec<u8> = Vec::new();
    v.try_reserve_exact(src.len()).map_err(|_| OutOfMemory)?;
    v.extend_from_slice(src);
    Ok(v)
}

/// A hyperlink that has been committed to page memory. This
/// is a "page entry" because while it represents a hyperlink,
/// some decoding (pointer chasing) is still necessary to get the
/// fully realized ID, URI, etc.
#[derive(Clone, Copy)]
pub struct PageEntry {
    pub id: PageEntryId,
    pub uri: OffsetSlice<u8>,
}

// PORT NOTE: the reference is `PageEntry.Id`, a nested union; hoisted to
// `PageEntryId` (same reason as `HyperlinkId`).
//
// PORT NOTE (P3 F1, uninit-in-page-mapping fix): this was a plain Rust `enum`
// and is now a byte-complete `#[repr(C)]` value, for exactly the reason
// recorded on `style::Color` (WS19 T5) and `size::OffsetSlice`. A `PageEntry`
// is stored in the page's `hyperlink_set` item array, which lives INSIDE the
// terminal page's backing mapping, and `compress::CompressedPage::init`
// compresses that whole mapping as one `&[u8]` — exactly as the oracle's
// `compress/Page.zig` compresses `page.memory`. A typed Rust write of an enum
// initializes only the active variant's payload and the discriminant, and
// actively DE-initializes everything else in the value — here the 7 bytes of
// discriminant padding at 1..8 (both variants) and the 12 unused payload bytes
// at 12..24 (`Implicit`). The compressor then read them, and Miri aborted with
// "constructing invalid value of type `[u8; 8]`: encountered uninitialized
// memory" inside `lz4::compress`. (Located precisely: the first uninit byte of
// the failing test's mapping was at +4452, i.e. `hyperlink_set` item 1's
// `PageEntryId` byte 4.)
//
// This is a JUSTIFIED R1 DIVERGENCE in representation only, not in value
// semantics or size: the oracle's `union(enum) { explicit: Offset(u8).Slice,
// implicit: size.OffsetInt }` has the same 24-byte size and the same undefined
// bytes — Zig simply permits reading undefined bytes and Rust does not, so
// R1 fidelity cannot carry the enum representation over. Every payload byte
// is now always written (see the R13-L2 note below for how), the tag is a raw
// `u8` so a value read back out of a page mapping can never be
// invalid-enum-value UB, and every read goes through a tag-checked accessor —
// the analogue of the reference's checked union-field access, and the identical
// shape `style::Color` already uses in this port.
//
// PORT NOTE (task R13-L2, NEW-FINDINGS N88): the two payloads SHARE their
// bytes, as the reference union's do. The first reshape above kept both
// payloads side by side (`explicit`, then `implicit`, then the tag), which was
// 24 bytes on 64-bit only by coincidence (16 + 4 + 1 rounds to 24, the union's
// 16 + 1 does too) and 16 on wasm32 against the union's 12 (8 + 1 rounded to
// 4), so the port's hyperlink `Set.Item` was 32 bytes there against the
// reference's 28 and did not fit the reference's slot (`RefItemGeometry`).
// Now one `OffsetSlice` holds the payload: `.explicit` is the whole slice, and
// `.implicit` is its first 4 bytes, the `Offset` field, which is where the
// reference's union puts the `u32` too (a union's fields all start at its
// byte 0). An implicit id keeps `len` at zero. No `union` and no `unsafe` are
// needed: both payloads are plain `u32`s in the same position, so the
// "shared bytes" are a field the two accessors read in turn, each behind the
// tag check. Sizes: 24/8 on 64-bit (16 + 1 + 7), 12/4 on 32-bit (8 + 1 + 3),
// the reference's `@sizeOf(PageEntry.Id)` on both (the wasm32 figure measured
// by the R13-L review's Zig probe for `wasm32-wasi`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageEntryId {
    /// The union payload. For `.explicit`, the slice itself. For
    /// `.implicit`, the `u32` lives in `payload.offset` and `payload.len` is
    /// zero, so that every byte is initialized and a given implicit value has
    /// exactly one byte image.
    payload: OffsetSlice<u8>,

    /// A `PageEntryIdTag` held as a raw `u8` (see the PORT NOTE above), with
    /// the reference's numbering: `0` is `.explicit`, so a zeroed mapping
    /// reads as the reference's zeroed union does (an empty `.explicit`).
    tag: u8,

    /// The tail-padding bytes the fields above imply (7 on 64-bit, 3 on
    /// 32-bit), named so that every byte of a `PageEntryId` written into a
    /// page mapping is initialized.
    _padding: [u8; core::mem::size_of::<usize>() - 1],
}

// `PageEntryId` must have no implicit padding at all, or the reshape above is
// pointless, and it must be the reference union's size. 64-bit: 16 + 1 + 7 ==
// 24 at align 8. 32-bit: `OffsetSlice` is 8 there (its `_padding` is
// 64-bit-only), so 8 + 1 + 3 == 12 at align 4.
//
// FINDING (P3-W6 T1): as with `OffsetSlice`'s pair, the literal asserts first
// written here were a hard compile error on any 32-bit target from the moment
// they landed (main @ 9d8fafb), and no gate saw it because A5's wasm build is
// skipped when a stale `.wasm` is present. Written against `size_of::<usize>()`
// they bind on both widths.
const _: () = assert!(core::mem::size_of::<PageEntryId>() == 3 * core::mem::size_of::<usize>());
const _: () = assert!(core::mem::align_of::<PageEntryId>() == core::mem::align_of::<usize>());
// The union shape, checkable on the host: the tag follows the one shared
// payload directly. Two payloads side by side (the shape before task R13-L2)
// put it 4 bytes later, which is 24 bytes on 64-bit too, so the size assert
// above cannot tell them apart there; only the wasm32 build's `ItemStride`
// assert could.
const _: () =
    assert!(core::mem::offset_of!(PageEntryId, tag) == core::mem::size_of::<OffsetSlice<u8>>());

/// `std.meta.activeTag(PageEntry.Id)` — the reference's union tag.
// PORT NOTE (task tail-s2 / S-2b): the reference's tag is an `enum(u1)` in
// declaration order, `explicit` 0 then `implicit` 1, and that value is the byte
// the union leaves in the page mapping (byte 16 on 64-bit, MEASURED with a Zig
// 0.16.0 probe at `20abdb50a`). The discriminants and the variant order follow
// it, so the stored byte matches (`page_entry_tag_byte_tests`); they were once
// the reverse.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageEntryIdTag {
    /// An explicitly provided ID via the OSC8 sequence.
    Explicit = 0,

    /// No ID was provided so we auto-generate the ID based on an
    /// incrementing counter attached to the screen.
    Implicit = 1,
}

impl PageEntryId {
    /// The reference's `.{ .explicit = slice }`. Named like the former enum
    /// variant it replaces, so the construction sites are unchanged by the
    /// byte-complete reshape (the same convention as `style::Color::Palette`).
    #[allow(non_snake_case)]
    #[inline]
    pub const fn Explicit(slice: OffsetSlice<u8>) -> PageEntryId {
        PageEntryId {
            payload: slice,
            tag: PageEntryIdTag::Explicit as u8,
            _padding: [0; core::mem::size_of::<usize>() - 1],
        }
    }

    /// The reference's `.{ .implicit = v }`. See `Explicit`.
    #[allow(non_snake_case)]
    #[inline]
    pub const fn Implicit(v: size::OffsetInt) -> PageEntryId {
        PageEntryId {
            payload: OffsetSlice::new(Offset::new(v), 0),
            tag: PageEntryIdTag::Implicit as u8,
            _padding: [0; core::mem::size_of::<usize>() - 1],
        }
    }

    /// The active tag — the reference's `std.meta.activeTag(self)`.
    #[inline]
    pub fn tag(self) -> PageEntryIdTag {
        // Only 0/1 are constructible (every write goes through the two
        // constructors above, and a zeroed mapping reads as `.explicit`), so
        // the fallback arm is unreachable and is `debug_assert`ed rather than
        // trapping — the same contract as `style::Color::tag`.
        debug_assert!(self.tag <= PageEntryIdTag::Implicit as u8);
        match self.tag {
            0 => PageEntryIdTag::Explicit,
            _ => PageEntryIdTag::Implicit,
        }
    }

    /// The reference's `self.explicit` union-field access. Only valid when
    /// `self.tag() == PageEntryIdTag::Explicit`, which the reference also
    /// requires (Zig checks it in safe modes; we `debug_assert` it).
    #[inline]
    pub fn explicit(self) -> OffsetSlice<u8> {
        debug_assert!(self.tag == PageEntryIdTag::Explicit as u8);
        self.payload
    }

    /// The reference's `self.implicit` union-field access. See `explicit`.
    #[inline]
    pub fn implicit(self) -> size::OffsetInt {
        debug_assert!(self.tag == PageEntryIdTag::Implicit as u8);
        self.payload.offset.offset
    }
}

// PORT NOTE (task tail-s2, S-2 review N3): the reference's `Item.value` is
// `undefined` (`ref_counted_set.zig:85`), so neither a fresh slot (zero-filled
// by `RefCountedSet::init`, which reads as an empty `.explicit`) nor a reaped
// one (written with `Item::default()`, this `.implicit` 0) has reference-defined
// bytes, and neither is ever read as a live value. The two therefore need not
// agree, and this default is left as it was.
impl Default for PageEntry {
    fn default() -> Self {
        PageEntry {
            id: PageEntryId::Implicit(0),
            uri: OffsetSlice::default(),
        }
    }
}

impl PageEntry {
    /// Duplicate this hyperlink from one page to another.
    ///
    /// # Safety
    ///
    /// `self_page` and `dst_page` must be valid pages whose backing memory
    /// contains the offsets referenced by this entry.
    pub unsafe fn dupe(
        &self,
        self_page: *const Page,
        dst_page: *mut Page,
    ) -> Result<PageEntry, OutOfMemory> {
        let mut copy = *self;

        // If the pages are the same then we can return a shallow copy.
        if std::ptr::eq(self_page, dst_page) {
            return Ok(copy);
        }

        // PORT NOTE: access the pages via field-projected helpers (see
        // `Page::string_alloc_alloc`) rather than `&*page` / `&mut *page` — `dupe`
        // is reachable from the hyperlink set context where the page's
        // `hyperlink_set` is mutably borrowed, so a whole-`Page` retag is UB.
        let self_base = unsafe { Page::memory_ptr_of(self_page) };
        let dst_base = unsafe { Page::memory_ptr_of(dst_page) };

        // Copy the URI
        {
            let uri = unsafe { self.uri.slice(self_base) };
            let buf =
                unsafe { Page::string_alloc_alloc(dst_page, uri.len()).map_err(|_| OutOfMemory)? };
            buf.copy_from_slice(uri);
            copy.uri =
                OffsetSlice::new(unsafe { get_offset::<u8, _>(dst_base, &buf[0]) }, uri.len());
        }
        // errdefer dst_page.string_alloc.free(...) — handled below on error.

        // Copy the ID
        match copy.id.tag() {
            PageEntryIdTag::Implicit => {} // Shallow is fine
            PageEntryIdTag::Explicit => {
                let slice = copy.id.explicit();
                let id = unsafe { slice.slice(self_base) };
                let id_len = id.len();
                let buf = match unsafe { Page::string_alloc_alloc(dst_page, id_len) } {
                    Ok(b) => b,
                    Err(_) => {
                        // errdefer for the URI allocation above.
                        unsafe {
                            let s = copy.uri.slice(dst_base);
                            Page::string_alloc_free(dst_page, s);
                        }
                        return Err(OutOfMemory);
                    }
                };
                buf.copy_from_slice(id);
                copy.id = PageEntryId::Explicit(OffsetSlice::new(
                    unsafe { get_offset::<u8, _>(dst_base, &buf[0]) },
                    id_len,
                ));
            }
        }

        Ok(copy)
    }

    /// # Safety
    ///
    /// `base` must be the true base of the backing memory holding the offsets
    /// referenced by this entry.
    pub unsafe fn hash<B: size::IntFromBase + Copy>(&self, base: B) -> u64 {
        // PORT NOTE (NEW-FINDINGS N82, task tail-s2): the reference's hash,
        // byte for byte (see `feed_hash_input` for what is fed, in order). It
        // is observable: the set picks buckets as `(hash +% i) & table_mask`,
        // so the hash decides placement, probe lengths, and which inputs reach
        // the PSL-overflow arm (UD131). The value was checked against the
        // reference's own `hash` over 600 random entries and against its set
        // placement at small capacities (`page_entry_hash_tests`, task tail-s2
        // report).
        //
        // var hasher = Wyhash.init(0);
        // ...
        // return hasher.final();
        //
        // PERF(port): the reference streams five or six short `update`s into
        // `Wyhash.init(0)`. Zig inlines that down to almost nothing; the
        // port's streaming `update`/`final` did not, and measured 3x slower
        // than the one-shot form on OSC 8-sized entries, a paired +4..7% on an
        // OSC 8-heavy `+terminal-stream` corpus (task tail-s2 report). So an
        // entry whose input fits `HASH_BUF` is gathered into one buffer and
        // hashed with the one-shot `Wyhash.hash(0, ..)`. The value is the same:
        // Wyhash's iterative API is concatenation-exact (Zig's `iterative api`
        // and `iterative maintains last sixteen` tests), and both paths are
        // checked against the reference. A longer entry streams, as the
        // reference does.
        const HASH_BUF: usize = 256;
        let mut len = 0;
        unsafe { self.feed_hash_input(base, |bytes| len += bytes.len()) };
        if len <= HASH_BUF {
            let mut buf = [0u8; HASH_BUF];
            let mut n = 0;
            unsafe {
                self.feed_hash_input(base, |bytes| {
                    buf[n..n + bytes.len()].copy_from_slice(bytes);
                    n += bytes.len();
                })
            };
            return rustty_zigstd::wyhash::Wyhash::hash(0, &buf[..n]);
        }
        let mut hasher = rustty_zigstd::wyhash::Wyhash::init(0);
        unsafe { self.feed_hash_input(base, |bytes| hasher.update(bytes)) };
        hasher.finalize()
    }

    /// The bytes the reference's `hash` feeds its hasher, in order, passed to
    /// `update` (`autoHash` / `autoHashStrat(.., .Deep)`,
    /// `lib/std/hash/auto_hash.zig`, Zig 0.16):
    ///   - the tag: `PageEntry.Id`'s tag is an `enum(u1)` in declaration order
    ///     (`explicit` 0, `implicit` 1, which `PageEntryIdTag`'s discriminants follow),
    ///     and a `u1` is fed as its one byte;
    ///   - `.implicit`: the `size.OffsetInt` (`u32`) as its native bytes;
    ///   - a `.Deep` slice (the `.explicit` id, then `uri`): each `u8` element,
    ///     then the slice's `len` as a native `usize`. The reference's per-byte
    ///     `update`s equal one `update` of the whole slice (Wyhash's iterative
    ///     API is concatenation-exact).
    ///
    /// # Safety
    ///
    /// As for `hash`.
    unsafe fn feed_hash_input<B: size::IntFromBase + Copy>(
        &self,
        base: B,
        mut update: impl FnMut(&[u8]),
    ) {
        // autoHash(&hasher, std.meta.activeTag(self.id));
        match self.id.tag() {
            PageEntryIdTag::Implicit => {
                // tag: implicit (1)
                update(&[PageEntryIdTag::Implicit as u8]);
                // .implicit => |v| autoHash(&hasher, v),
                update(&self.id.implicit().to_ne_bytes());
            }
            PageEntryIdTag::Explicit => {
                // tag: explicit (0)
                update(&[PageEntryIdTag::Explicit as u8]);
                // .explicit => |slice| autoHashStrat(&hasher, slice.slice(base), .Deep),
                let s = unsafe { self.id.explicit().slice(base) };
                update(s);
                update(&s.len().to_ne_bytes());
            }
        }
        // autoHashStrat(&hasher, self.uri.slice(base), .Deep);
        let uri = unsafe { self.uri.slice(base) };
        update(uri);
        update(&uri.len().to_ne_bytes());
    }

    /// # Safety
    ///
    /// `self_base`/`other_base` must be the true bases of the backing memory
    /// holding the offsets referenced by `self`/`other`.
    pub unsafe fn eql<B1: size::IntFromBase + Copy, B2: size::IntFromBase + Copy>(
        &self,
        self_base: B1,
        other: &PageEntry,
        other_base: B2,
    ) -> bool {
        // if (std.meta.activeTag(self.id) != std.meta.activeTag(other.id)) return false;
        match (self.id.tag(), other.id.tag()) {
            (PageEntryIdTag::Implicit, PageEntryIdTag::Implicit) => {
                if self.id.implicit() != other.id.implicit() {
                    return false;
                }
            }
            (PageEntryIdTag::Explicit, PageEntryIdTag::Explicit) => {
                let self_ptr = unsafe { self.id.explicit().slice(self_base) };
                let other_ptr = unsafe { other.id.explicit().slice(other_base) };
                if self_ptr != other_ptr {
                    return false;
                }
            }
            _ => return false,
        }

        let self_uri = unsafe { self.uri.slice(self_base) };
        let other_uri = unsafe { other.uri.slice(other_base) };
        self_uri == other_uri
    }

    /// Free the memory for this entry from its page.
    ///
    /// # Safety
    ///
    /// `page` must be a valid page whose backing memory holds the offsets
    /// referenced by this entry.
    pub unsafe fn free(&self, page: *mut Page) {
        // PORT NOTE: field-projected access (see `Page::string_alloc_free`) — this
        // runs from the hyperlink set context's `deleted` while the page's
        // `hyperlink_set` is mutably borrowed.
        let base = unsafe { Page::memory_ptr_of(page) };
        // PORT NOTE (oracle 2fc238ed0): snapshot decode can materialize
        // zero-length slices; guard both frees with `len > 0`.
        match self.id.tag() {
            PageEntryIdTag::Implicit => {}
            PageEntryIdTag::Explicit => {
                let v = self.id.explicit();
                if v.len > 0 {
                    let s = unsafe { v.slice(base) };
                    unsafe { Page::string_alloc_free(page, s) };
                }
            }
        }
        if self.uri.len > 0 {
            let uri = unsafe { self.uri.slice(base) };
            unsafe { Page::string_alloc_free(page, uri) };
        }
    }
}

/// The context for the hyperlink set. See the module-level PORT NOTE: this
/// carries the per-call page pointers the reference passes as an anonymous
/// struct. Callers set `page`/`src_page` immediately before each set call.
// PORT NOTE: the reference is the anonymous struct passed as `ContextT`:
//   struct {
//       page: ?*Page = null,
//       src_page: ?*const Page = null,
//       pub fn hash(...) ...
//       pub fn eql(...) ...
//       pub fn deleted(...) ...
//   }
// PORT NOTE: the page pointers are stored as exposed addresses (`usize`) rather
// than raw `*mut Page`. The reference holds `*Page` directly, but in Rust storing
// a `*mut Page` derived from `&mut self` and then continuing to use `self` would
// invalidate that pointer's Stacked-Borrows tag (the `set.context.page = self`
// assignment is followed by `&mut self.hyperlink_set` accesses). Round-tripping
// through `usize` gives the stored pointer exposed provenance that survives those
// accesses — the same sound pattern `render-types`' `Offset` uses for its
// int-to-ptr base resolution. The address is reconstructed on each use.
#[derive(Clone, Copy, Default)]
pub struct SetContext {
    /// The page which holds the strings for items in this set (exposed address).
    pub page: Option<usize>,

    /// The page which holds the strings for items
    /// looked up with, e.g., `add` or `lookup`,
    /// if different from the destination page (exposed address).
    pub src_page: Option<usize>,
}

impl SetContext {
    /// Set the destination page from a raw pointer, exposing its provenance.
    pub fn set_page(&mut self, page: *mut Page) {
        self.page = Some((page as *const Page).expose_provenance());
    }

    /// Set the source page from a raw pointer, exposing its provenance.
    pub fn set_src_page(&mut self, page: Option<*const Page>) {
        self.src_page = page.map(|p| p.expose_provenance());
    }

    fn page_ptr(&self) -> *mut Page {
        core::ptr::with_exposed_provenance_mut(self.page.unwrap())
    }

    fn src_or_page(&self) -> *const Page {
        core::ptr::with_exposed_provenance(self.src_page.unwrap_or(self.page.unwrap()))
    }
}

impl RefCountedSetContext<PageEntry> for SetContext {
    fn hash(&self, link: PageEntry) -> u64 {
        // return link.hash((self.src_page orelse self.page.?).memory);
        // PORT NOTE: read `memory_ptr` via field projection (see
        // `Page::memory_ptr_of`) — this callback runs while the page's
        // `hyperlink_set` is mutably borrowed, so forming a `&Page` is UB.
        let base: *const Page = self.src_or_page();
        unsafe { link.hash(Page::memory_ptr_of(base)) }
    }

    fn eql(&self, a: PageEntry, b: PageEntry) -> bool {
        // return a.eql((self.src_page orelse self.page.?).memory, &b, self.page.?.memory);
        let a_base: *const Page = self.src_or_page();
        let b_base: *const Page = self.page_ptr() as *const Page;
        unsafe { a.eql(Page::memory_ptr_of(a_base), &b, Page::memory_ptr_of(b_base)) }
    }

    fn deleted(&self, link: PageEntry) {
        // link.free(self.page.?);
        unsafe { link.free(self.page_ptr()) }
    }
}

/// The set of hyperlinks. This is ref-counted so that a set of cells
/// can share the same hyperlink without duplicating the data.
pub type Set = RefCountedSet<PageEntry, Id, size::CellCountInt, SetContext>;

// PORT NOTE (layout parity, NEW-FINDINGS N88): the set is laid out with the
// reference's `Set.Item` geometry (see `ref_counted_set::RefItemGeometry`),
// derived from shadows of the reference's declarations at `20abdb50a`:
//   - `Offset(u8).Slice = struct { offset: Offset(u8), len: usize }`
//     (`size.zig`), 16/8;
//   - `PageEntry.Id = union(enum) { explicit: Offset(u8).Slice, implicit:
//     size.OffsetInt }` (`hyperlink.zig:84`), 24/8;
//   - `PageEntry = struct { id: PageEntry.Id, uri: Offset(u8).Slice }`
//     (`hyperlink.zig:80`), 40/8;
//   - `RefCountedSet.Item = struct { value: T, meta: Metadata }`, the
//     metadata three `u16`s (`Id = size.HyperlinkCountInt`, `RefCountInt =
//     size.CellCountInt`, `hyperlink.zig:18`, `:211`), 46 rounded to 48/8.
// On a 32-bit `usize` (wasm32) the same declarations give 8/4, 12/4, 20/4 and
// 26 rounded to 28/4, measured by the R13-L review's Zig probe for
// `wasm32-wasi`. The port's item is 48/8 on 64-bit and, since `PageEntryId`
// shares its payload bytes as the union does (task R13-L2), 28/4 on 32-bit:
// the same geometry on both. The assert below keeps the hyperlink section tied
// to the reference if the port's `PageEntry` changes; `ref_geometry_golden_tests`
// checks the shadows and the port's types against the G-REFGEOM golden, the
// wasm32 lines through a layout model (task R13-L2).
#[repr(C)]
#[allow(dead_code)] // never built: only its size and alignment are read
pub(crate) struct RefShadowSlice {
    offset: u32,
    len: usize,
}
#[repr(C)]
#[allow(dead_code)] // never built: only its size and alignment are read
pub(crate) struct RefShadowPageEntryId {
    payload: RefShadowSlice,
    tag: u8,
}
#[repr(C)]
#[allow(dead_code)] // never built: only its size and alignment are read
pub(crate) struct RefShadowItem {
    value_id: RefShadowPageEntryId,
    value_uri: RefShadowSlice,
    bucket: Id,
    psl: Id,
    ref_count: size::CellCountInt,
}

impl crate::ref_counted_set::RefItemGeometry for PageEntry {
    const REF_ITEM_SIZE: usize = core::mem::size_of::<RefShadowItem>();
    const REF_ITEM_ALIGN: usize = core::mem::align_of::<RefShadowItem>();
}

const _: () = assert!(
    core::mem::size_of::<crate::ref_counted_set::Item<PageEntry, Id, size::CellCountInt>>()
        <= <PageEntry as crate::ref_counted_set::RefItemGeometry>::REF_ITEM_SIZE
);

// The shadows themselves, pinned on a 32-bit `usize` to the G-REFGEOM golden's
// `wasm32-wasi` lines (task R13-L3). The host-run `ref_geometry_golden_tests`
// checks the wasm32 lines only through a hand-written model of these fields,
// which cannot see a field's real 32-bit type (a `len: u64` would pass it and
// make the wasm32 item 32/8 against the reference's 28/4). The wasm32 builds
// (G-XTARGET, `check_cross_targets.sh`) compile these.
#[cfg(target_pointer_width = "32")]
const _: () = {
    use core::mem::{align_of, size_of};
    // wasm32-wasi size.Offset(u8).Slice 8 4
    assert!(size_of::<RefShadowSlice>() == 8 && align_of::<RefShadowSlice>() == 4);
    // wasm32-wasi hyperlink.PageEntry.Id 12 4
    assert!(size_of::<RefShadowPageEntryId>() == 12 && align_of::<RefShadowPageEntryId>() == 4);
    // wasm32-wasi hyperlink.Set.Item 28 4
    assert!(size_of::<RefShadowItem>() == 28 && align_of::<RefShadowItem>() == 4);
    assert!(<PageEntry as crate::ref_counted_set::RefItemGeometry>::REF_ITEM_SIZE == 28);
    assert!(<PageEntry as crate::ref_counted_set::RefItemGeometry>::REF_ITEM_ALIGN == 4);
};

// rustty-only test of the tag byte `PageEntry.Id` leaves in the page mapping
// (task tail-s2 / S-2b). The reference's `union(enum)` tag is an `enum(u1)` in
// declaration order, `explicit` 0 and `implicit` 1, stored in the byte after the
// 16-byte payload. MEASURED with a Zig 0.16.0 probe built from `20abdb50a`
// (`probe_n82.zig`, `Page.insertHyperlink` then `std.mem.asBytes(&entry.id)`):
//   implicit 0x11223344 -> 44 33 22 11 .. .. | 01 at byte 16
//   explicit "my-id"    -> ..                | 00 at byte 16
// and an all-zero `PageEntry.Id` is `.explicit` (offset 0, len 0).
// The probe measured the 64-bit layout (`usize` 8).
#[cfg(all(test, target_pointer_width = "64"))]
mod page_entry_tag_byte_tests {
    use super::*;
    use crate::page::Capacity;

    /// The bytes of a `PageEntryId`. It has no padding (every byte is a
    /// named field, see the asserts above), so all of them are initialized.
    fn id_bytes(id: &PageEntryId) -> [u8; core::mem::size_of::<PageEntryId>()] {
        let mut out = [0u8; core::mem::size_of::<PageEntryId>()];
        // SAFETY: `PageEntryId` is `repr(C)` with no implicit padding, so
        // reading its bytes reads initialized memory.
        let src = unsafe {
            core::slice::from_raw_parts(
                (id as *const PageEntryId).cast::<u8>(),
                core::mem::size_of::<PageEntryId>(),
            )
        };
        out.copy_from_slice(src);
        out
    }

    #[test]
    fn the_stored_tag_byte_is_the_references() {
        const TAG: usize = core::mem::offset_of!(PageEntryId, tag);
        // The probe's 64-bit layout: the tag follows the 16-byte payload.
        assert_eq!(16, TAG);

        let mut cap = Capacity::new(10, 10);
        cap.string_bytes = 1 << 16;
        let mut page = Page::init(cap).unwrap();
        let implicit = page
            .insert_hyperlink(Hyperlink {
                id: HyperlinkId::Implicit(0x1122_3344),
                uri: b"https://example.com".to_vec(),
            })
            .unwrap();
        let explicit = page
            .insert_hyperlink(Hyperlink {
                id: HyperlinkId::Explicit(b"my-id".to_vec()),
                uri: b"https://example.com".to_vec(),
            })
            .unwrap();
        let base = page.memory_ptr();

        // Read back out of the mapping, as the compressor sees it.
        let imp = id_bytes(unsafe { &(*page.hyperlink_set.get(base, implicit)).id });
        assert_eq!(1, imp[TAG], "implicit tag byte");
        assert_eq!([0x44, 0x33, 0x22, 0x11], imp[0..4], "implicit payload");
        let exp = id_bytes(unsafe { &(*page.hyperlink_set.get(base, explicit)).id });
        assert_eq!(0, exp[TAG], "explicit tag byte");

        // The constructors agree with the mapping.
        assert_eq!(1, id_bytes(&PageEntryId::Implicit(0))[TAG]);
        assert_eq!(
            0,
            id_bytes(&PageEntryId::Explicit(OffsetSlice::default()))[TAG]
        );
        page.deinit();
    }

    /// A zeroed id (a dead item's value, a zeroed mapping) is the reference's
    /// tag 0, `.explicit`, with an empty slice.
    #[test]
    fn a_zeroed_id_is_explicit() {
        let zero = PageEntryId::Explicit(OffsetSlice::default());
        assert!(id_bytes(&zero).iter().all(|&b| b == 0));
        assert_eq!(PageEntryIdTag::Explicit, zero.tag());
        assert_eq!(0, zero.explicit().len);
    }
}

// rustty-only tests of `PageEntry.hash` (`hyperlink.zig:143`) against the
// reference (NEW-FINDINGS N82, task tail-s2). The expected values were produced
// by the reference's OWN `PageEntry.hash` and `hyperlink.Set`, run through
// `Page.insertHyperlink` in a Zig 0.16.0 probe built from `20abdb50a`
// (`probe_n82.zig`, task tail-s2 report); the port must give the same 64-bit
// value and, from it, the same bucket placement. The probe ran on a 64-bit
// target (aarch64-macos), and the reference hashes each slice's `len` as a
// native `usize`, so the expected values hold only where `usize` is 8 bytes.
#[cfg(all(test, target_pointer_width = "64"))]
mod page_entry_hash_tests {
    use super::*;
    use crate::page::Capacity;
    use crate::ref_counted_set::item_at;

    type Row = (u8, u32, Vec<u8>, Vec<u8>, u64);

    /// `(tag, implicit, explicit id, uri, reference hash)`. `tag` 1 is
    /// `.implicit`, 0 is `.explicit` (the reference's declaration order).
    fn reference_rows() -> Vec<Row> {
        vec![
            (
                1,
                0,
                vec![],
                b"https://example.com".to_vec(),
                0x0ddc_eb58_fb25_fd66,
            ),
            (
                0,
                0,
                b"0".to_vec(),
                b"https://example.com".to_vec(),
                0xd4b2_d241_9751_4ed5,
            ),
            (
                1,
                1,
                vec![],
                b"https://example.com".to_vec(),
                0x9464_0835_9003_474a,
            ),
            (1, 0xFFFF_FFFF, vec![], b"a".to_vec(), 0x18d2_a151_74f6_4a67),
            (0, 0, b"a".to_vec(), b"b".to_vec(), 0xbd7b_2482_c397_7bc0),
            // The next two feed the same bytes ("abx") and differ only in
            // where the id ends: `autoHashStrat(.., .Deep)` hashes each
            // slice's `len` after its bytes, which is what tells them apart.
            (0, 0, b"ab".to_vec(), b"x".to_vec(), 0x6202_0994_ca19_0075),
            (0, 0, b"a".to_vec(), b"bx".to_vec(), 0x52fb_9e1d_447e_e0b3),
            (
                0,
                0,
                b"my-link-id".to_vec(),
                b"file:///home/user/some/long/path/to/a/file.txt".to_vec(),
                0x60dd_23cc_dfd6_cd5a,
            ),
            (
                1,
                42,
                vec![],
                [b"http://a.b/".to_vec(), b"x".repeat(100)].concat(),
                0xf414_efff_e98a_97f8,
            ),
            // Longer than `hash`'s 256-byte one-shot buffer: the streaming path.
            (
                0,
                0,
                b"long".to_vec(),
                [b"https://localhost/".to_vec(), b"y".repeat(300)].concat(),
                0xb7ff_dde8_f0b3_b24e,
            ),
            // Either side of the buffer's edge: 1 + 1 + 8 + 238 + 8 = 256 bytes
            // of input (one-shot), then 257 (streaming); the same for an
            // implicit id, 1 + 4 + 243 + 8 = 256, then 257.
            (0, 0, b"b".to_vec(), b"z".repeat(238), 0x3878_e69c_4022_9c2f),
            (0, 0, b"b".to_vec(), b"z".repeat(239), 0x73f7_05bf_72fb_8d3a),
            (1, 7, vec![], b"z".repeat(243), 0x0ab9_411c_d33b_ca2b),
            (1, 7, vec![], b"z".repeat(244), 0xf749_c3f3_ae66_8d03),
        ]
    }

    fn link(tag: u8, implicit: u32, id: &[u8], uri: &[u8]) -> Hyperlink {
        Hyperlink {
            id: if tag == 1 {
                HyperlinkId::Implicit(implicit)
            } else {
                HyperlinkId::Explicit(id.to_vec())
            },
            uri: uri.to_vec(),
        }
    }

    #[test]
    fn hash_matches_the_reference() {
        let mut cap = Capacity::new(10, 10);
        cap.string_bytes = 1 << 16;
        let mut page = Page::init(cap).unwrap();
        let rows = reference_rows();
        for (tag, implicit, id, uri, want) in &rows {
            let set_id = page
                .insert_hyperlink(link(*tag, *implicit, id, uri))
                .unwrap();
            let base = page.memory_ptr();
            let entry = unsafe { *page.hyperlink_set.get(base, set_id) };
            assert_eq!(*tag == 1, entry.id.tag() == PageEntryIdTag::Implicit);
            let got = unsafe { entry.hash(base) };
            assert_eq!(
                *want,
                got,
                "{:?} {:?}",
                String::from_utf8_lossy(id),
                String::from_utf8_lossy(uri)
            );

            // The set's context hashes through the same function.
            let mut ctx = SetContext::default();
            ctx.set_page(&mut page as *mut Page);
            assert_eq!(*want, ctx.hash(entry));

            unsafe { page.hyperlink_set.release(base, set_id) };
        }
        // The two "abx" entries stay distinct.
        assert_ne!(rows[5].4, rows[6].4);
        page.deinit();
    }

    /// The set derives the bucket from the hash as the reference does
    /// (`(hash +% i) & table_mask`, `ref_counted_set.zig:629`), so with the
    /// reference's hash a sequence of inserts must land in the reference's
    /// buckets with the reference's probe lengths. Count 16 is a 16-bucket
    /// table with 13 items (id 0 reserved), so the last two adds fail.
    #[test]
    fn set_placement_matches_the_reference() {
        let mut cap = Capacity::new(10, 10);
        cap.hyperlink_bytes =
            (16 * <PageEntry as crate::ref_counted_set::RefItemGeometry>::REF_ITEM_SIZE) as u16;
        cap.string_bytes = 1 << 16;
        let mut page = Page::init(cap).unwrap();
        assert_eq!(16, page.hyperlink_set.layout.table_cap);

        let mut adds = Vec::new();
        for k in 0..14u32 {
            let id = format!("id{k}").into_bytes();
            let uri = format!("u{k}").into_bytes();
            adds.push(page.insert_hyperlink(link(if k % 2 == 0 { 1 } else { 0 }, k, &id, &uri)));
        }
        let want_adds: Vec<Result<Id, crate::page::InsertHyperlinkError>> = (1..=12)
            .map(Ok)
            .chain([
                Err(crate::page::InsertHyperlinkError::SetOutOfMemory),
                Err(crate::page::InsertHyperlinkError::SetOutOfMemory),
            ])
            .collect();
        assert_eq!(want_adds, adds);

        let base = page.memory_ptr();
        let set = &page.hyperlink_set;
        let table: Vec<Id> = (0..set.layout.table_cap)
            .map(|i| unsafe { *set.table.ptr(base).add(i) })
            .collect();
        let items = unsafe { set.items.ptr(base) };
        let psl: Vec<Id> = table
            .iter()
            .map(|&id| {
                if id == 0 {
                    0
                } else {
                    unsafe { (*item_at(items, id as usize)).meta.psl }
                }
            })
            .collect();
        assert_eq!(
            vec![0, 0, 11, 0, 0, 3, 8, 5, 4, 7, 12, 10, 2, 1, 6, 9],
            table
        );
        assert_eq!(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 1, 1, 0], psl);
        assert_eq!(2, set.max_psl);
        page.deinit();
    }
}

// rustty-only tests of `PageEntry.eql`'s first line (`hyperlink.zig:165`):
// `if (std.meta.activeTag(self.id) != std.meta.activeTag(other.id)) return
// false;`. An implicit link (OSC 8 with no `id=`) and an explicit one with the
// same URI are different hyperlinks, so the set must keep both. No other test
// adds the two with one URI, so an `eql` that ignored a tag mismatch and fell
// through to the URI compare stayed green (the R13-L2 review's mutation M1).
#[cfg(test)]
mod page_entry_eql_tests {
    use super::*;
    use crate::page::Capacity;

    const URI: &[u8] = b"https://example.com";

    /// The page with an implicit and an explicit link on one URI, and their
    /// set ids. The explicit id's bytes are `"0"`, and the implicit id is 0:
    /// the two agree on everything but the tag.
    fn page_with_both() -> (Page, Id, Id) {
        let mut page = Page::init(Capacity::new(10, 10)).unwrap();
        let implicit = page
            .insert_hyperlink(Hyperlink {
                id: HyperlinkId::Implicit(0),
                uri: URI.to_vec(),
            })
            .unwrap();
        let explicit = page
            .insert_hyperlink(Hyperlink {
                id: HyperlinkId::Explicit(b"0".to_vec()),
                uri: URI.to_vec(),
            })
            .unwrap();
        (page, implicit, explicit)
    }

    #[test]
    fn eql_is_false_when_only_the_tag_differs() {
        let (mut page, implicit, explicit) = page_with_both();
        let base = page.memory_ptr();
        let a = unsafe { *page.hyperlink_set.get(base, implicit) };
        let b = unsafe { *page.hyperlink_set.get(base, explicit) };
        assert_eq!(PageEntryIdTag::Implicit, a.id.tag());
        assert_eq!(PageEntryIdTag::Explicit, b.id.tag());
        unsafe {
            assert_eq!(a.uri.slice(base), b.uri.slice(base));
            // Different tags, same URI: not equal, in both directions.
            assert!(!a.eql(base, &b, base));
            assert!(!b.eql(base, &a, base));
            // Each still equals itself.
            assert!(a.eql(base, &a, base));
            assert!(b.eql(base, &b, base));
        }
        page.deinit();
    }

    #[test]
    fn the_set_keeps_an_implicit_and_an_explicit_link_with_one_uri() {
        let (mut page, implicit, explicit) = page_with_both();
        assert_ne!(implicit, explicit);
        assert_eq!(2, page.hyperlink_set.count());
        // A lookup of each finds its own entry, not the other.
        let base = page.memory_ptr();
        for (want, name) in [(implicit, "implicit"), (explicit, "explicit")] {
            let entry = unsafe { *page.hyperlink_set.get(base, want) };
            let page_ptr: *mut Page = &mut page;
            page.hyperlink_set.context.set_page(page_ptr);
            page.hyperlink_set.context.set_src_page(None);
            assert_eq!(
                Some(want),
                unsafe { page.hyperlink_set.lookup(base, entry) },
                "{name}"
            );
        }
        page.deinit();
    }
}

// rustty-only tests (the reference's `dupe` contract is carried by its
// `Allocator.Error!Hyperlink` signature, which Zig callers must `try`): pin that
// the ported `dupe` is FALLIBLE rather than an aborting `Vec::clone`, and that
// it is a byte-exact deep copy.
#[cfg(test)]
mod hyperlink_dupe_tests {
    use super::*;

    #[test]
    fn dupe_is_fallible_and_byte_exact() {
        // Raw (non-UTF-8) bytes: the reference's `[]const u8` is a byte slice.
        let src = Hyperlink {
            id: HyperlinkId::Explicit(vec![0xFF, 0x00, 0x80]),
            uri: b"http://example.com/\xC3\x28".to_vec(),
        };

        // `Result`, not a bare `Hyperlink` — an OOM here must be RETURNED, the
        // way `Screen.startHyperlink`'s `IncreaseCapacityError!void` promises.
        let copy: Result<Hyperlink, OutOfMemory> = src.dupe();
        let copy = copy.unwrap();

        assert_eq!(src.uri, copy.uri);
        match (&src.id, &copy.id) {
            (HyperlinkId::Explicit(a), HyperlinkId::Explicit(b)) => assert_eq!(a, b),
            _ => unreachable!(),
        }
        // Deep copy: the buffers are independent.
        assert!(!std::ptr::eq(src.uri.as_ptr(), copy.uri.as_ptr()));
    }

    #[test]
    fn dupe_preserves_implicit_id() {
        let src = Hyperlink {
            id: HyperlinkId::Implicit(1234),
            uri: b"x".to_vec(),
        };
        let copy = src.dupe().unwrap();
        match copy.id {
            HyperlinkId::Implicit(v) => assert_eq!(1234, v),
            HyperlinkId::Explicit(_) => unreachable!(),
        }
    }

    // A genuine allocation failure REPORTS instead of aborting the process:
    // `try_reserve_exact` of an impossible capacity is the only portable way to
    // drive a real allocator failure, and it is the same primitive `dupe` uses.
    #[test]
    // MIRI EXCEPTION (rustty-vt hyperlink): the test's whole subject is a REAL
    // allocator failure — `try_reserve_exact(isize::MAX)` must return `Err`
    // rather than abort. Miri does not model a failing allocation: an
    // impossible request is an interpreter-level hard error ("resource
    // exhaustion: tried to allocate more memory than available to compiler"),
    // which aborts the whole rustty-vt lib binary before the `is_err()` this
    // test asserts is ever evaluated. There is no Miri-compatible variant: any
    // request small enough for Miri to serve SUCCEEDS, making the assertion
    // vacuous. Runs in ci.sh's HARD `cargo test --workspace` step, where the
    // real allocator refuses the request and the reporting path is exercised.
    #[cfg_attr(
        miri,
        ignore = "drives a REAL allocator failure; Miri hard-errors instead of failing the alloc"
    )]
    fn try_dupe_bytes_reports_allocation_failure() {
        let mut v: Vec<u8> = Vec::new();
        assert!(v.try_reserve_exact(isize::MAX as usize).is_err());
        assert_eq!(Ok(vec![1u8, 2, 3]), try_dupe_bytes(&[1u8, 2, 3]));
    }
}
